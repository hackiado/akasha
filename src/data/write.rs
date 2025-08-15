use crate::event::Event;
use crc32fast::Hasher;
use std::path::Path;
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    time::{SystemTime, UNIX_EPOCH},
};

pub struct Writer {
    f: File,
}


impl Writer {
    const MAGIC: [u8; 4] = *b"AKLA";
    const VERSION: u16 = 1;
    const HEADER_RESERVED: usize = 10;
    const HEADER_LEN: u64 = 16;

    pub fn new(f: File) -> Self {
        Self { f }
    }
    pub fn create(path: &str) -> io::Result<Self> {
        let mut f = OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(path)?;

        if f.metadata()?.len() == 0 {
            Self::write_header(&mut f)?;
        }
        // Always append at the end by default
        f.seek(SeekFrom::End(0))?;
        Ok(Self { f })
    }

    fn write_header(f: &mut File) -> io::Result<()> {
        f.seek(SeekFrom::Start(0))?;
        f.write_all(Self::MAGIC.as_ref())?;
        f.write_all(&Self::VERSION.to_le_bytes())?;
        f.write_all(&[0u8; Self::HEADER_RESERVED])?;
        f.flush()?;
        Ok(())
    }

    fn read_and_validate_header(f: &mut File) -> io::Result<()> {
        f.seek(SeekFrom::Start(0))?;
        let mut hdr = [0u8; 16];
        f.read_exact(&mut hdr)?;
        if hdr[0..4] != Self::MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid magic"));
        }
        // If versioning matters later, check hdr[4..6]
        Ok(())
    }

    pub fn append(&mut self, id: u64, phenomenon: &str, noumenon: &str) -> io::Result<u64> {
        // ensure we are at end
        let start = self.f.seek(SeekFrom::End(0))?;

        // payload
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| io::Error::other("SystemTime before UNIX_EPOCH"))?
            .as_nanos();
        let ph = phenomenon.as_bytes();
        let no = noumenon.as_bytes();

        // len_total (u32) + ts(u128) + id(u64) + ph_len(u16) + no_len(u16) + ph + no + crc(u32)
        let mut buf = Vec::with_capacity(4 + 16 + 8 + 2 + 2 + ph.len() + no.len() + 4);

        // len_total placeholder (u32)
        buf.extend_from_slice(&[0u8; 4]);
        buf.extend_from_slice(&ts.to_le_bytes());
        buf.extend_from_slice(&id.to_le_bytes());
        buf.extend_from_slice(&(ph.len() as u16).to_le_bytes());
        buf.extend_from_slice(&(no.len() as u16).to_le_bytes());
        buf.extend_from_slice(ph);
        buf.extend_from_slice(no);

        // compute checksum on everything after len_total
        let mut hasher = Hasher::new();
        hasher.update(&buf[4..]);
        let crc = hasher.finalize();

        // now set len_total: payload + checksum
        let len_total = (buf.len() - 4 + 4) as u32; // excluding len field, including crc
        buf[0..4].copy_from_slice(&len_total.to_le_bytes());
        buf.extend_from_slice(&crc.to_le_bytes());

        self.f.write_all(&buf)?;
        self.f.sync_data()?; // crash-safety for appended record
        Ok(start) // offset useful for external indexing
    }

    pub fn read_all(&mut self) -> io::Result<()> {
        Self::read_and_validate_header(&mut self.f)?;
        self.f.seek(SeekFrom::Start(Self::HEADER_LEN))?;

        let mut off = Self::HEADER_LEN;
        while let Some((len, payload)) = Self::read_valid_entry(&mut self.f)? {
            if let Some((ts, id, ph, no)) = Self::parse_payload(&payload)? {
                println!("id={id} ts={ts} ph={ph} no={no}");
            }
            off = off.saturating_add(4 + len as u64);
        }
        Ok(())
    }

    pub fn rebuild_index(&mut self) -> io::Result<BTreeMap<u64, u64>> {
        let mut idx = BTreeMap::new();
        Self::read_and_validate_header(&mut self.f)?;
        self.f.seek(SeekFrom::Start(Self::HEADER_LEN))?;

        let mut off = Self::HEADER_LEN;
        while let Some((len, payload)) = Self::read_valid_entry(&mut self.f)? {
            // id is located after ts (16 bytes)
            if payload.len() < 16 + 8 {
                break;
            }
            let id = u64::from_le_bytes(payload[16..24].try_into().unwrap());
            // Keep the last offset for a given id
            idx.insert(id, off);
            off = off.saturating_add(4 + len as u64);
        }
        Ok(idx)
    }

    fn read_valid_entry(f: &mut File) -> io::Result<Option<(usize, Vec<u8>)>> {
        let mut len_buf = [0u8; 4];
        let n = f.read(&mut len_buf)?;
        if n == 0 {
            return Ok(None); // clean EOF
        }
        if n < 4 {
            // Partial trailing bytes: stop iteration
            return Ok(None);
        }

        let len = u32::from_le_bytes(len_buf) as usize;
        // minimal payload (ts + id + ph_len + no_len) + crc
        const MIN_PAYLOAD: usize = 16 + 8 + 2 + 2;
        const CRC_LEN: usize = 4;
        if len < MIN_PAYLOAD + CRC_LEN {
            return Ok(None);
        }

        let mut entry = vec![0u8; len];
        if f.read_exact(&mut entry).is_err() {
            // cut entry -> stop
            return Ok(None);
        }

        // split payload / checksum
        let (payload, crc_bytes) = entry.split_at(len - CRC_LEN);
        let mut hasher = Hasher::new();
        hasher.update(payload);
        let expected_crc = hasher.finalize();
        let got_crc = u32::from_le_bytes(crc_bytes.try_into().unwrap());

        if expected_crc != got_crc {
            // corruption -> stop
            return Ok(None);
        }

        Ok(Some((len, payload.to_vec())))
    }

    fn parse_payload(payload: &[u8]) -> io::Result<Option<(u128, u64, String, String)>> {
        let mut p = 0usize;

        if payload.len() < 16 + 8 + 2 + 2 {
            return Ok(None);
        }

        let ts = u128::from_le_bytes(payload[p..p + 16].try_into().unwrap());
        p += 16;
        let id = u64::from_le_bytes(payload[p..p + 8].try_into().unwrap());
        p += 8;

        let ph_len = u16::from_le_bytes(payload[p..p + 2].try_into().unwrap()) as usize;
        p += 2;
        let no_len = u16::from_le_bytes(payload[p..p + 2].try_into().unwrap()) as usize;
        p += 2;

        // Bounds check
        if p.checked_add(ph_len)
            .and_then(|end| end.checked_add(no_len))
            .map(|end| end <= payload.len())
            != Some(true)
        {
            return Ok(None);
        }

        let ph_bytes = &payload[p..p + ph_len];
        p += ph_len;
        let no_bytes = &payload[p..p + no_len];

        let ph = match std::str::from_utf8(ph_bytes) {
            Ok(s) => s.to_string(),
            Err(_) => return Ok(None),
        };
        let no = match std::str::from_utf8(no_bytes) {
            Ok(s) => s.to_string(),
            Err(_) => return Ok(None),
        };

        Ok(Some((ts, id, ph, no)))
    }

    pub fn read_one_at<P: AsRef<Path>>(path: P, offset: u64) -> io::Result<Event> {
        let mut f = File::open(path)?;
        f.seek(SeekFrom::Start(offset))?;

        let mut len_buf = [0u8; 4];
        f.read_exact(&mut len_buf)?;
        let len = u32::from_le_bytes(len_buf) as usize;

        let mut entry = vec![0u8; len];
        f.read_exact(&mut entry)?;

        // CRC
        let (payload, crc_bytes) = entry.split_at(len - 4);
        let mut h = Hasher::new();
        h.update(payload);
        if h.finalize() != u32::from_le_bytes(crc_bytes.try_into().unwrap()) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "CRC mismatch"));
        }

        // Parse
        let mut p = 0;
        let ts = u128::from_le_bytes(payload[p..p + 16].try_into().unwrap());
        p += 16;
        let id = u64::from_le_bytes(payload[p..p + 8].try_into().unwrap());
        p += 8;
        let ph_len = u16::from_le_bytes(payload[p..p + 2].try_into().unwrap()) as usize;
        p += 2;
        let no_len = u16::from_le_bytes(payload[p..p + 2].try_into().unwrap()) as usize;
        p += 2;

        let ph = std::str::from_utf8(&payload[p..p + ph_len]).unwrap();
        p += ph_len;
        let no = std::str::from_utf8(&payload[p..p + no_len]).unwrap();

        Ok(Event {
            timestamp: ts,
            id,
            phenomenon: ph.to_string(),
            noumenon: no.to_string(),
        })
    }
}

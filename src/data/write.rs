use crate::event::Event;
use crc32fast::Hasher;
use std::collections::HashMap;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};
use std::{
    collections::BTreeMap,
    fs,
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    time::{SystemTime, UNIX_EPOCH},
};

pub struct Writer {
    f: File,
    next_id: u64,
}

impl Writer {
    const MAGIC: [u8; 4] = *b"AKLA";
    const VERSION: u16 = 1;
    const HEADER_RESERVED: usize = 10;
    const HEADER_LEN: u64 = 16;

    // Reserved header layout:
    // [0..8): next_id (u64, LE)
    // [8..10): reserved
    const HDR_NEXT_ID_OFF: u64 = 4 + 2; // MAGIC(4) + VERSION(2) = 6

    pub fn new(f: File) -> Self {
        Self { f, next_id: 1 }
    }

    pub fn append_file<P: AsRef<Path>>(&mut self, filepath: P) -> io::Result<()> {
        // Store the full path so we can rebuild an accurate seen index later
        let name = filepath.as_ref().to_string_lossy().to_string();

        let content = read_to_string(&filepath)?;

        self.append(&name, &content)?;

        Ok(())
    }

    pub fn create(path: &str) -> io::Result<Self> {
        let mut f = OpenOptions::new()
            .create(true)
            .truncate(false) // don't truncate existing; make it safe to reopen
            .read(true)
            .write(true)
            .open(path)?;

        let mut next_id = 1u64;

        if f.metadata()?.len() == 0 {
            Self::write_header(&mut f, next_id)?;
        } else {
            // Validate header and load next_id
            Self::read_and_validate_header(&mut f)?;
            next_id = Self::read_header_next_id(&mut f)?;
            if next_id == 0 {
                // Recover by scanning to find max id and set next_id = max+1
                next_id = Self::compute_max_id_from_file(&mut f)?
                    .and_then(|m| m.checked_add(1))
                    .unwrap_or(1);
                Self::write_header_next_id(&mut f, next_id)?;
            }
        }

        // Always append at the end by default
        f.seek(SeekFrom::End(0))?;
        Ok(Self { f, next_id })
    }

    // Build a map of path -> last known content hash from the log
    fn rebuild_seen_index_from_log(&mut self) -> HashMap<PathBuf, String> {
        let mut seen = HashMap::new();

        // Save current position and scan from the beginning
        let saved_pos = self.f.stream_position().ok();
        if Self::read_and_validate_header(&mut self.f).is_err() {
            // If header invalid, return empty (safe fallback)
            if let Some(pos) = saved_pos {
                let _ = self.f.seek(SeekFrom::Start(pos));
            }
            return seen;
        }
        if self.f.seek(SeekFrom::Start(Self::HEADER_LEN)).is_err() {
            if let Some(pos) = saved_pos {
                let _ = self.f.seek(SeekFrom::Start(pos));
            }
            return seen;
        }

        // Iterate all valid entries; last one for a given path wins
        while let Ok(Some((_len, payload))) = Self::read_valid_entry(&mut self.f) {
            if let Ok(Some((_ts, _id, ph, no))) = Self::parse_payload(&payload) {
                // Hash the stored content to compare against filesystem later
                let hash = blake3::hash(no.as_bytes()).to_hex().to_string();
                seen.insert(PathBuf::from(ph), hash);
            }
        }

        // Restore previous position
        if let Some(pos) = saved_pos {
            let _ = self.f.seek(SeekFrom::Start(pos));
        }
        seen
    }

    fn file_hash<P: AsRef<Path>>(&mut self, p: P) -> io::Result<String> {
        let bytes = fs::read(p)?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }

    pub fn store_directory<P: AsRef<Path>>(&mut self, dir: P) -> io::Result<()> {
        let mut seen: HashMap<PathBuf, String> = self.rebuild_seen_index_from_log();

        // Collect files
        let mut files: Vec<PathBuf> = ignore::WalkBuilder::new(dir)
            .add_custom_ignore_filename(".ignore")
            .build()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_type()
                    .expect("failed to get the file type")
                    .is_file()
            })
            .map(|e| e.into_path())
            .filter(|p| {
                let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if name.starts_with('.') {
                    return false;
                }
                if p.components()
                    .any(|c| c.as_os_str() == "target" || c.as_os_str() == ".git")
                {
                    return false;
                }
                true
            })
            .collect();

        files.sort(); // stable order

        // progress bar (indicatif)
        use indicatif::{ProgressBar, ProgressStyle};
        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("=>-"),
        );

        for path in files {
            let h = match self.file_hash(&path) {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("hash fail {}: {e}", path.display());
                    pb.inc(1);
                    continue;
                }
            };

            pb.set_message(format!("{}", path.file_name().unwrap().to_string_lossy()));

            // Dedup: if last stored content hash is the same, skip
            let is_same = seen.get(&path).map(|old| old == &h).unwrap_or(false);
            if !is_same {
                if let Err(e) = self.append_file(&path) {
                    eprintln!("store fail {}: {e}", path.display());
                } else {
                    // Update seen with the new content hash
                    seen.insert(path.clone(), h);
                }
            }

            pb.inc(1);
        }

        pb.finish_with_message("Done!");
        Ok(())
    }

    fn write_header(f: &mut File, next_id: u64) -> io::Result<()> {
        f.seek(SeekFrom::Start(0))?;
        f.write_all(Self::MAGIC.as_ref())?;
        f.write_all(&Self::VERSION.to_le_bytes())?;

        // Initialize reserved with next_id (8 bytes) + 2 reserved zeros
        let mut reserved = [0u8; Self::HEADER_RESERVED];
        reserved[0..8].copy_from_slice(&next_id.to_le_bytes());
        f.write_all(&reserved)?;
        f.flush()?;
        Ok(())
    }

    fn write_header_next_id(f: &mut File, next_id: u64) -> io::Result<()> {
        let cur = f.stream_position()?;
        f.seek(SeekFrom::Start(Self::HDR_NEXT_ID_OFF))?;
        f.write_all(&next_id.to_le_bytes())?;
        f.flush()?;
        // Restore previous position
        f.seek(SeekFrom::Start(cur))?;
        Ok(())
    }

    fn read_header_next_id(f: &mut File) -> io::Result<u64> {
        let cur = f.stream_position()?;
        f.seek(SeekFrom::Start(Self::HDR_NEXT_ID_OFF))?;
        let mut buf = [0u8; 8];
        f.read_exact(&mut buf)?;
        let val = u64::from_le_bytes(buf);
        f.seek(SeekFrom::Start(cur))?;
        Ok(val)
    }

    fn read_and_validate_header(f: &mut File) -> io::Result<()> {
        // Ensure we read header from the beginning
        f.seek(SeekFrom::Start(0))?;
        let mut hdr = [0u8; 16];
        f.read_exact(&mut hdr)?;
        if hdr[0..4] != Self::MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid magic"));
        }
        Ok(())
    }

    fn compute_max_id_from_file(f: &mut File) -> io::Result<Option<u64>> {
        // Start right after header
        Self::read_and_validate_header(f)?;
        f.seek(SeekFrom::Start(Self::HEADER_LEN))?;

        let mut max_id: Option<u64> = None;
        while let Some((_, payload)) = Self::read_valid_entry(f)? {
            if payload.len() >= 16 + 8 {
                let id = u64::from_le_bytes(payload[16..24].try_into().unwrap());
                max_id = Some(max_id.map_or(id, |m| m.max(id)));
            }
        }
        Ok(max_id)
    }

    pub fn append(&mut self, phenomenon: &str, noumenon: &str) -> io::Result<u64> {
        // ensure we are at the end
        let start = self.f.seek(SeekFrom::End(0))?;

        // payload
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| io::Error::other("SystemTime before UNIX_EPOCH"))?
            .as_nanos();
        let ph = phenomenon.as_bytes();
        let no = noumenon.as_bytes();
        let id = self.next_id;

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

        let len_total = (buf.len() - 4 + 4) as u32; // excluding len field, including crc
        buf[0..4].copy_from_slice(&len_total.to_le_bytes());
        buf.extend_from_slice(&crc.to_le_bytes());

        // Write record
        self.f.write_all(&buf)?;
        self.f.sync_data()?; // crash-safety for appended record

        // Bump and persist next_id
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| io::Error::other("id overflow"))?;
        Self::write_header_next_id(&mut self.f, self.next_id)?;

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
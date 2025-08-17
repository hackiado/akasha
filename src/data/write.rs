//! Append-only event log writer/reader with a fixed binary format and CRC protection.
//!
//! File layout:
//! - Header (16 bytes total):
//!   - MAGIC       [0..4)   = b"AKLA"
//!   - VERSION     [4..6)   = u16 (LE), current = 1
//!   - RESERVED    [6..16)  = 10 bytes
//!     - NEXT_ID   [6..14)  = u64 (LE), next id to assign for new entries
//!     - reserved  [14..16) = 2 bytes, currently zero
//!
//! - Records (variable length), each:
//!   - LEN_TOTAL   [0..4)           = u32 (LE), total bytes of (payload + CRC), not including this length field
//!   - PAYLOAD     [4..4+N)         = see below
//!   - CRC32       [4+N..4+N+4)     = CRC32 over PAYLOAD (crc32fast)
//!
//! PAYLOAD layout:
//!   - TS          [0..16)          = u128 (LE), UNIX epoch time in nanoseconds
//!   - ID          [16..24)         = u64 (LE), monotonically increasing id
//!   - PH_LEN      [24..26)         = u16 (LE), length of phenomenon bytes
//!   - NO_LEN      [26..28)         = u16 (LE), length of noumenon bytes
//!   - PHENOMENON  [28..28+PH_LEN)  = UTF-8 bytes
//!   - NOUMENON    [..+NO_LEN)      = UTF-8 bytes
//!
//! Design notes:
//! - Append-only: records are only appended; we never rewrite existing records except for updating NEXT_ID in header.
//! - Crash safety: each append is followed by `sync_data()`. Header’s NEXT_ID is also persisted after each append.
//! - Integrity: each record protected by CRC32; on read, iteration stops at first invalid/truncated record.
//! - Recovery: if NEXT_ID in header is zero or invalid, we scan the file to compute max(id)+1.
//! - Deduplication in `store_directory`: based on BLAKE3 hash of file contents tracked per path.
//! - Concurrency: this struct is not synchronized. External synchronization is required for multi-writer scenarios.
//!
//! Endianness: All integers are encoded little-endian.

use crate::event::Event;
use blake3;
use crc32fast::Hasher;
use std::collections::HashMap;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    time::{SystemTime, UNIX_EPOCH},
};

/// Append-only log writer/reader for a single “cube” file.
///
/// Responsibilities:
/// - Initialize/validate the on-disk header and maintain a monotonic `next_id`.
/// - Append CRC-protected records with timestamp, id, phenomenon, and noumenon.
/// - Iterate, index, and random-access read validated records.
pub struct Writer {
    /// Underlying file handle for the cube.
    f: File,
    /// Next record id to assign; persisted in the header for recovery.
    next_id: u64,
}

impl Writer {
    /// 4-byte magic to identify the file type.
    const MAGIC: [u8; 4] = *b"AKLA";
    /// On-disk version. Bump on breaking layout changes.
    const VERSION: u16 = 1;
    /// Number of reserved header bytes after MAGIC+VERSION.
    const HEADER_RESERVED: usize = 10;
    /// Total header length in bytes.
    const HEADER_LEN: u64 = 16;

    // Reserved header layout:
    // [0..8): next_id (u64, LE)
    // [8..10): reserved
    /// Offset of `next_id` field from start-of-file.
    const HDR_NEXT_ID_OFF: u64 = 4 + 2; // MAGIC(4) + VERSION(2) = 6

    /// Construct a Writer from an already-open file.
    ///
    /// Note: This does not validate the header or position the cursor. Prefer `create()` unless you
    /// have special needs.
    pub fn new(f: File) -> Self {
        Self { f, next_id: 1 }
    }

    /// Open or create a cube file at `path`, validate/initialize its header, and seek to EOF for appends.
    ///
    /// Behavior:
    /// - New or empty file: write a fresh header with `next_id = 1`.
    /// - Existing file:
    ///   - Validate header magic.
    ///   - Read `next_id`.
    ///   - If `next_id` is 0, scan the file to recover `max(id) + 1` and persist it.
    /// - Always leaves the cursor at end-of-file ready for append.
    pub fn create(path: &str) -> io::Result<Self> {
        let mut f = OpenOptions::new()
            .create(true)
            .truncate(false) // preserve existing data
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

    /// Recursively scan `dir` and append contents of qualifying files to the cube,
    /// deduplicating by content hash and showing a progress bar.
    ///
    /// Pipeline:
    /// - Build in-memory “seen” map from the log: path -> last stored BLAKE3(content).
    /// - Walk `dir` using ignore rules (.ignore), select only regular files, exclude dotfiles,
    ///   and exclude paths containing `target` or `.git`.
    /// - For each file:
    ///   - Compute BLAKE3(content); if equal to the last stored hash for that path, skip.
    ///   - Otherwise, append file content under its path and update the in-memory map.
    ///
    /// Error handling:
    /// - Per-file failures (hash/read/append) are logged to stderr and processing continues.
    /// - Overall function returns `Ok(())` unless a fatal IO error occurs setting up the walk or I/O on the cube.
    pub fn store_directory<P: AsRef<Path>>(&mut self, dir: P) -> io::Result<()> {
        // Build a map of path -> last stored content hash by scanning the cube.
        let mut seen: HashMap<PathBuf, String> = self.rebuild_seen_index_from_log();

        // Collect candidate files from the directory walk applying the exclusion policy.
        let mut files: Vec<PathBuf> = ignore::WalkBuilder::new(dir)
            .add_custom_ignore_filename(".ignore")
            .build()
            .filter_map(Result::ok)
            .filter(|e| {
                // Keep only regular files; skip directories and special file types.
                e.file_type()
                    .expect("failed to get the file type")
                    .is_file()
            })
            .map(|e| e.into_path())
            .filter(|p| {
                // Exclusions:
                // - dotfiles
                // - any path containing "target" or ".git" components
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

        // Sort for stable, reproducible traversal order.
        files.sort();

        // Progress bar setup.
        use indicatif::{ProgressBar, ProgressStyle};
        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("=>-"),
        );

        for path in files {
            // Compute the current file's content hash.
            let h = match Self::file_hash(&path) {
                Ok(h) => h,
                Err(e) => {
                    // Log and continue on non-fatal per-file errors.
                    eprintln!("hash fail {}: {e}", path.display());
                    pb.inc(1);
                    continue;
                }
            };

            // Display the current filename in the progress bar.
            pb.set_message(format!("{}", path.file_name().unwrap().to_string_lossy()));

            // Deduplicate: skip if unchanged relative to last stored content for this path.
            let is_same = seen.get(&path).map(|old| old == &h).unwrap_or(false);
            if !is_same {
                // Append file contents to the cube; log error but do not abort on failure.
                if let Err(e) = self.append_file_contents(&path) {
                    eprintln!("store fail {}: {e}", path.display());
                } else {
                    // Update the in-memory "seen" index so subsequent duplicates in this run are skipped.
                    seen.insert(path.clone(), h);
                }
            }

            pb.inc(1);
        }

        pb.finish_with_message("Done!");
        Ok(())
    }

    /// Write a fresh header with the provided `next_id` at offset 0 and flush it.
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

    /// Persist `next_id` into the header while preserving the current cursor position.
    fn write_header_next_id(f: &mut File, next_id: u64) -> io::Result<()> {
        let cur = f.stream_position()?;
        f.seek(SeekFrom::Start(Self::HDR_NEXT_ID_OFF))?;
        f.write_all(&next_id.to_le_bytes())?;
        f.flush()?;
        // Restore previous position
        f.seek(SeekFrom::Start(cur))?;
        Ok(())
    }

    /// Read `next_id` from the header, restoring the original cursor position afterwards.
    fn read_header_next_id(f: &mut File) -> io::Result<u64> {
        let cur = f.stream_position()?;
        f.seek(SeekFrom::Start(Self::HDR_NEXT_ID_OFF))?;
        let mut buf = [0u8; 8];
        f.read_exact(&mut buf)?;
        let val = u64::from_le_bytes(buf);
        f.seek(SeekFrom::Start(cur))?;
        Ok(val)
    }

    /// Validate the header by checking the magic value at the start of the file.
    ///
    /// On success, the cursor is left just after the 16-byte header.
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

    /// Scan the file and return the maximum encountered record id, if any.
    ///
    /// Used for recovery when the stored `next_id` is zero/invalid.
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

    /// Append a new record with the given phenomenon and noumenon, returning its byte offset.
    ///
    /// Guarantees:
    /// - Appends at EOF.
    /// - Flushes data to disk (`sync_data`) for crash safety.
    /// - Increments and persists `next_id` in the header.
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

    /// Iterate over the file and print all valid records in a human-readable form.
    ///
    /// Stops on the first invalid/truncated record (typical for append-only logs with partial tails).
    pub fn read_all(&mut self) -> io::Result<()> {
        Self::read_and_validate_header(&mut self.f)?;
        self.f.seek(SeekFrom::Start(Self::HEADER_LEN))?;

        let mut off = Self::HEADER_LEN;
        while let Some((len, payload)) = Self::read_valid_entry(&mut self.f)? {
            if let Some((ts, id, ph, no)) = Self::parse_payload(&payload)? {
                println!("\nid={id} ts={ts} ph={ph} no={no}\n");
            }
            off = off.saturating_add(4 + len as u64);
        }
        Ok(())
    }

    /// Build an index of id -> file offset for all valid records.
    ///
    /// If duplicate ids are present (unexpected), the last one wins.
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

    /// Read the next record from the current cursor, verify CRC, and return its (len, payload).
    ///
    /// Returns:
    /// - `Ok(Some((len, payload)))` for a valid record
    /// - `Ok(None)` on EOF, partial tail, invalid length, truncated entry, or CRC mismatch
    /// - `Err(_)` on underlying IO errors during reads
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

    /// Build an in-memory map of path -> last known content hash by scanning the log.
    ///
    /// The content hash is computed as BLAKE3 over the noumenon bytes of the last valid record
    /// for each path (phenomenon). This supports deduplication in `store_directory`.
    fn rebuild_seen_index_from_log(&mut self) -> HashMap<PathBuf, String> {
        let mut seen = HashMap::new();

        // Validate header and position after it; return empty on failure for safety.
        if Self::read_and_validate_header(&mut self.f).is_err() {
            return seen;
        }
        if self.f.seek(SeekFrom::Start(Self::HEADER_LEN)).is_err() {
            return seen;
        }

        // Scan all valid entries; the last one for a given path wins.
        while let Ok(Some((_, payload))) = Self::read_valid_entry(&mut self.f) {
            if let Ok(Some((_ts, _id, ph, no))) = Self::parse_payload(&payload) {
                let hash = blake3::hash(no.as_bytes()).to_hex().to_string();
                seen.insert(PathBuf::from(ph), hash);
            }
        }

        seen
    }

    /// Read a file and append its contents to the log.
    ///
    /// The file path is stored as the phenomenon, and its contents as the noumenon.
    fn append_file_contents(&mut self, path: &Path) -> io::Result<u64> {
        let content = read_to_string(path)?;
        self.append(&path.display().to_string(), &content)
    }

    /// Compute a BLAKE3 hash of a file's raw bytes, returned as a lowercase hex string.
    ///
    /// This function reads bytes (not text) so it works for both text and binary files.
    fn file_hash(path: &Path) -> io::Result<String> {
        let bytes = fs::read(path)?;
        let hash = blake3::hash(&bytes);
        Ok(hash.to_hex().to_string())
    }

    /// Parse a payload into (timestamp, id, phenomenon, noumenon), validating bounds and UTF-8.
    ///
    /// Returns `Ok(Some(..))` on success, `Ok(None)` on malformed payload.
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

    /// Random-access read of a record at `offset` in `path`, verifying CRC and returning an `Event`.
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

// Free helper functions for CLI ergonomics.

// Open an existing cube or create one if missing, returning a Writer positioned at EOF.
//
// This is a thin wrapper around Writer::create used by the CLI layer.
pub fn open_cube(path: &str) -> io::Result<Writer> {
    Writer::create(path)
}

// Open a cube for reading/printing using the Writer API.
//
// This currently reuses Writer::create to validate the header and position the cursor;
// the returned Writer can be used to call `read_all`.
pub fn read_cube(path: &str) -> io::Result<Writer> {
    Writer::create(path)
}

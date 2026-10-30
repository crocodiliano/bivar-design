//! The `.bivar` file (phase 0.6, part 4; specification in `docs/formato/`).
//!
//! One file. A 64-byte header, then records that are only ever appended:
//! `kind (1 byte) | length (8 bytes, little endian) | payload | SHA-256`
//! of the first three. Edits are appended as they happen, so a crash loses
//! nothing already written, and a cut or damaged tail is simply ignored.
//! Compaction writes a new file and replaces the old one in one rename.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bivar_poc_modelo::{
    CompactionPlan, HiddenLayer, OpError, Store, clean_copy, hidden_layers, random_u64,
};
use sha2::{Digest, Sha256};

pub const MAGIC: &[u8; 8] = b"BIVARDOC";
pub const FORMAT_VERSION: u32 = 1;
pub const HEADER_LEN: usize = 64;
/// Pieces of large files.
pub const CHUNK_SIZE: usize = 4 * 1024 * 1024;
/// A chunk record: the 32-byte id and at most `CHUNK_SIZE` bytes.
pub const CHUNK_RECORD_MAX: u64 = 32 + CHUNK_SIZE as u64;

/// What a file may ask for (owner, 2026-10-30; ADR-0024). Large files
/// (images, video, audio) do not count in `doc_total`: their pieces are at
/// most `CHUNK_RECORD_MAX` and each index at most `blob_index`. Settings may
/// raise these, only explicitly and with a visible warning, never silently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// One snapshot or updates record.
    pub doc_record: u64,
    /// All snapshot and updates records together.
    pub doc_total: u64,
    /// Number of records in the file.
    pub records: u64,
    /// One large-file index (8 MiB: about 260,000 pieces, 1 TB per file).
    pub blob_index: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            doc_record: 256 << 20,
            doc_total: 1 << 30,
            records: 50_000_000,
            blob_index: 8 << 20,
        }
    }
}
/// Edits reach the disk at most this long after being made (owner, 2026-10-29).
pub const SYNC_EVERY: Duration = Duration::from_secs(1);

const RECORD_HEAD: usize = 9;
const RECORD_TAIL: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    /// The whole document (first record; written again by compaction).
    Snapshot = 1,
    /// Document changes since the previous record.
    Updates = 2,
    /// Up to `CHUNK_SIZE` bytes of a large file, after its id (the SHA-256
    /// of those bytes, 32 bytes). Opening reads only the id; the bytes are
    /// checked against it when read.
    Chunk = 3,
    /// A large file: its SHA-256, its size and its chunk ids, in order.
    BlobIndex = 4,
    /// The owner saved here (payload: seconds since 1970).
    SaveMark = 5,
}

impl Kind {
    fn from_u8(b: u8) -> Option<Kind> {
        Some(match b {
            1 => Kind::Snapshot,
            2 => Kind::Updates,
            3 => Kind::Chunk,
            4 => Kind::BlobIndex,
            5 => Kind::SaveMark,
            _ => return None,
        })
    }
}

#[derive(Debug)]
pub enum FileError {
    Io(io::Error),
    NotABivarFile,
    /// Written by a newer Bivar: refused, never "repaired".
    NewerVersion(u32),
    /// A header option this version does not know (for example encryption).
    UnknownFlags(u32),
    /// The first records (header or snapshot) are damaged: nothing to open.
    Unreadable(String),
    Document(OpError),
    AlreadyExists,
    /// The main file is missing or cannot be opened, and a safety copy from an
    /// interrupted compaction exists: the program offers `recover_from_backup`.
    RecoverFromBackup {
        backup: PathBuf,
        cause: Box<FileError>,
    },
    /// The file asks for more than a limit allows. The message names the
    /// limit; the owner may raise it in the settings, with a warning.
    LimitExceeded {
        what: &'static str,
        limit: u64,
        found: u64,
    },
    /// The file is not the one that was checked (it changed in between).
    ChangedSinceCheck,
    /// "Salvar cópia para enviar" with conflicts still open: the program warns
    /// first (the copy has no history, so the losing values would not go
    /// along). Saving again with `confirmed` goes ahead.
    PendingConflicts(Vec<String>),
    /// "Salvar cópia para enviar" with hidden layers: they would go along
    /// unseen, so the program lists them and asks whether to keep them in
    /// the copy or leave them out (`SendCopyAnswers::hidden_layers`).
    HiddenLayers(Vec<HiddenLayer>),
    /// A half-written copy from a crash is in the way. It is never deleted
    /// without asking: the program tells the owner, who may keep it (and
    /// choose another name) or delete it with `discard_partial`.
    PartialLeftover(PathBuf),
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileError::LimitExceeded { what, limit, found } => write!(
                f,
                "{what}: {found} is above the limit of {limit}. A real document would not \
                 need this; if this one does, the limit can be raised in the settings."
            ),
            FileError::HiddenLayers(list) => write!(
                f,
                "this file contains {} hidden layers; keep them in the copy or leave them out?",
                list.len()
            ),
            FileError::PartialLeftover(p) => write!(
                f,
                "{} is a copy left half-written by a crash. It was not deleted; \
                 delete it, or save the copy under another name.",
                p.display()
            ),
            other => write!(f, "{other:?}"),
        }
    }
}

impl From<io::Error> for FileError {
    fn from(e: io::Error) -> Self {
        FileError::Io(e)
    }
}

impl From<OpError> for FileError {
    fn from(e: OpError) -> Self {
        FileError::Document(e)
    }
}

fn sha(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Header: magic, format version, flags (bit 0 reserved for encryption,
/// which is postponed: every bit must be 0), 16 reserved bytes, SHA-256 of
/// the first 32 bytes.
fn header(version: u32) -> [u8; HEADER_LEN] {
    let mut h = [0u8; HEADER_LEN];
    h[..8].copy_from_slice(MAGIC);
    h[8..12].copy_from_slice(&version.to_le_bytes());
    let digest = sha(&[&h[..32]]);
    h[32..].copy_from_slice(&digest);
    h
}

fn check_header(h: &[u8]) -> Result<u32, FileError> {
    if h.len() < HEADER_LEN || &h[..8] != MAGIC {
        return Err(FileError::NotABivarFile);
    }
    if sha(&[&h[..32]]) != h[32..HEADER_LEN] {
        return Err(FileError::Unreadable("damaged header".into()));
    }
    let version = u32::from_le_bytes(h[8..12].try_into().expect("4 bytes"));
    let flags = u32::from_le_bytes(h[12..16].try_into().expect("4 bytes"));
    if version > FORMAT_VERSION {
        return Err(FileError::NewerVersion(version));
    }
    if version != FORMAT_VERSION {
        return Err(FileError::Unreadable(format!(
            "unknown format version {version}"
        )));
    }
    if flags != 0 {
        return Err(FileError::UnknownFlags(flags));
    }
    Ok(version)
}

fn encode_record(kind: Kind, payload: &[u8]) -> Vec<u8> {
    let len = (payload.len() as u64).to_le_bytes();
    let k = [kind as u8];
    let digest = sha(&[&k, &len, payload]);
    let mut out = Vec::with_capacity(RECORD_HEAD + payload.len() + RECORD_TAIL);
    out.extend_from_slice(&k);
    out.extend_from_slice(&len);
    out.extend_from_slice(payload);
    out.extend_from_slice(&digest);
    out
}

/// One record found while reading, or why reading stopped there.
enum Next {
    Record {
        kind: Kind,
        payload: Vec<u8>,
        end: u64,
    },
    /// A chunk that is not the last record: only its id was read.
    ChunkRef {
        id: [u8; 32],
        data_at: u64,
        len: u64,
        end: u64,
    },
    /// Clean end of the file.
    End,
    /// The last record is incomplete (a write was cut): safe to drop.
    TornTail(String),
    /// A complete-looking record is wrong: damage, not just a cut.
    Damaged(String),
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Snapshot => "document snapshot record",
        Kind::Updates => "document updates record",
        Kind::Chunk => "large-file piece",
        Kind::BlobIndex => "large-file index",
        Kind::SaveMark => "save mark",
    }
}

/// Reads the record at `at`. Every record found is added to the file's
/// fingerprint: its kind, length and digest (the digest covers the payload).
fn read_next<R: Read + Seek>(
    r: &mut R,
    at: u64,
    file_len: u64,
    limits: &Limits,
    fp: &mut Sha256,
) -> Result<Next, FileError> {
    if at == file_len {
        return Ok(Next::End);
    }
    if file_len - at < (RECORD_HEAD + RECORD_TAIL) as u64 {
        return Ok(Next::TornTail(format!(
            "{} stray bytes at {at}",
            file_len - at
        )));
    }
    r.seek(SeekFrom::Start(at))?;
    let mut head = [0u8; RECORD_HEAD];
    r.read_exact(&mut head)?;
    let Some(kind) = Kind::from_u8(head[0]) else {
        return Ok(Next::Damaged(format!(
            "unknown record kind {} at {at}",
            head[0]
        )));
    };
    let len = u64::from_le_bytes(head[1..].try_into().expect("8 bytes"));
    let max = match kind {
        Kind::Snapshot | Kind::Updates => limits.doc_record,
        Kind::Chunk => CHUNK_RECORD_MAX,
        Kind::BlobIndex => limits.blob_index,
        Kind::SaveMark => 8,
    };
    // Checked before anything is read or allocated.
    if len > max {
        return Err(FileError::LimitExceeded {
            what: kind_name(kind),
            limit: max,
            found: len,
        });
    }
    let end = at + (RECORD_HEAD as u64) + len + RECORD_TAIL as u64;
    if end > file_len {
        return Ok(Next::TornTail(format!("record at {at} is cut")));
    }
    // Large-file pieces are skipped on opening (a 2 GB image would be read
    // whole otherwise); the last record is always checked in full, to catch
    // a cut write.
    if kind == Kind::Chunk && end < file_len && len >= 32 {
        let mut id = [0u8; 32];
        r.read_exact(&mut id)?;
        let mut digest = [0u8; RECORD_TAIL];
        r.seek(SeekFrom::Start(end - RECORD_TAIL as u64))?;
        r.read_exact(&mut digest)?;
        fp.update(head);
        fp.update(digest);
        let data_at = at + RECORD_HEAD as u64 + 32;
        return Ok(Next::ChunkRef {
            id,
            data_at,
            len: len - 32,
            end,
        });
    }
    let mut payload = vec![0u8; len as usize];
    r.read_exact(&mut payload)?;
    let mut digest = [0u8; RECORD_TAIL];
    r.read_exact(&mut digest)?;
    if sha(&[&head[..1], &head[1..], &payload]) != digest {
        // A bad checksum on the very last record is a cut write; earlier
        // in the file it is damage.
        return Ok(if end == file_len {
            Next::TornTail(format!("record at {at} has a wrong checksum"))
        } else {
            Next::Damaged(format!("record at {at} has a wrong checksum"))
        });
    }
    fp.update(head);
    fp.update(digest);
    Ok(Next::Record { kind, payload, end })
}

/// Everything read from a file, before any of it reaches the document
/// library: the library only sees the data after the whole file was read,
/// the limits held and the fingerprint matched.
struct Scan {
    snapshot: Option<Vec<u8>>,
    updates: Vec<Vec<u8>>,
    chunks: HashMap<[u8; 32], (u64, u64)>,
    blobs: HashMap<[u8; 32], BlobEntry>,
    /// Where the complete records end.
    end: u64,
    /// The last write was cut (crash): the bytes after `end` can be dropped.
    torn: bool,
    report: OpenReport,
    fp: Sha256,
}

/// Reads every record. With `keep` false, document data is checked and
/// dropped (computing a fingerprint must not hold up to 1 GiB in memory).
fn scan<R: Read + Seek>(
    r: &mut R,
    file_len: u64,
    limits: &Limits,
    keep: bool,
) -> Result<Scan, FileError> {
    let mut head = [0u8; HEADER_LEN];
    r.seek(SeekFrom::Start(0))?;
    if file_len < HEADER_LEN as u64 || r.read_exact(&mut head).is_err() {
        return Err(FileError::NotABivarFile);
    }
    check_header(&head)?;
    let mut s = Scan {
        snapshot: None,
        updates: Vec::new(),
        chunks: HashMap::new(),
        blobs: HashMap::new(),
        end: HEADER_LEN as u64,
        torn: false,
        report: OpenReport::default(),
        fp: Sha256::new(),
    };
    s.fp.update(head);
    let (mut records, mut doc_bytes) = (0u64, 0u64);
    loop {
        let at = s.end;
        let next = read_next(r, at, file_len, limits, &mut s.fp)?;
        if matches!(next, Next::Record { .. } | Next::ChunkRef { .. }) {
            records += 1;
            if records > limits.records {
                return Err(FileError::LimitExceeded {
                    what: "number of records",
                    limit: limits.records,
                    found: records,
                });
            }
        }
        match next {
            Next::Record { kind, payload, end } => {
                if matches!(kind, Kind::Snapshot | Kind::Updates) {
                    doc_bytes += payload.len() as u64;
                    if doc_bytes > limits.doc_total {
                        return Err(FileError::LimitExceeded {
                            what: "document data (all snapshot and updates records)",
                            limit: limits.doc_total,
                            found: doc_bytes,
                        });
                    }
                }
                match kind {
                    Kind::Snapshot => {
                        if s.snapshot.is_some() {
                            return Err(FileError::Unreadable(format!("second snapshot at {at}")));
                        }
                        s.snapshot = Some(if keep { payload } else { Vec::new() });
                    }
                    Kind::Updates => {
                        if s.snapshot.is_none() {
                            return Err(FileError::Unreadable(
                                "updates before the snapshot".into(),
                            ));
                        }
                        if keep {
                            s.updates.push(payload);
                        }
                        s.report.edits_after_last_save += 1;
                    }
                    Kind::Chunk => {
                        // The last record, checked in full.
                        if payload.len() < 32 || sha(&[&payload[32..]]) != payload[..32] {
                            return Err(FileError::Unreadable(format!("bad piece at {at}")));
                        }
                        let id: [u8; 32] = payload[..32].try_into().expect("32 bytes");
                        s.chunks.insert(
                            id,
                            (at + RECORD_HEAD as u64 + 32, payload.len() as u64 - 32),
                        );
                    }
                    Kind::BlobIndex => {
                        let (id, entry) = decode_blob_index(&payload).ok_or_else(|| {
                            FileError::Unreadable(format!("bad blob index at {at}"))
                        })?;
                        s.blobs.insert(id, entry);
                    }
                    Kind::SaveMark => s.report.edits_after_last_save = 0,
                }
                s.end = end;
            }
            Next::ChunkRef {
                id,
                data_at,
                len,
                end,
            } => {
                s.chunks.insert(id, (data_at, len));
                s.end = end;
            }
            Next::End => break,
            Next::TornTail(why) => {
                s.report.dropped_tail = Some(why);
                s.torn = true;
                break;
            }
            Next::Damaged(why) => {
                s.report.damaged_at = Some(why);
                s.report.read_only = Some("the file is damaged after this point".into());
                break;
            }
        }
    }
    // A large-file index that points at a piece the file does not have.
    if s.blobs
        .values()
        .any(|b| b.chunks.iter().any(|c| !s.chunks.contains_key(c)))
    {
        s.report.damaged_at = Some("a large-file index points to a missing piece".into());
        s.report.read_only = Some("the file is damaged".into());
    }
    if s.snapshot.is_none() {
        return Err(FileError::Unreadable("no document snapshot".into()));
    }
    Ok(s)
}

fn build_store<S: Store>(s: &mut Scan, peer: u64) -> Result<S, FileError> {
    let snapshot = s.snapshot.take().expect("scan requires a snapshot");
    let mut store = S::load(&snapshot, peer)?;
    store.import_updates_batch(&std::mem::take(&mut s.updates))?;
    Ok(store)
}

/// The fingerprint of a file: SHA-256 of its header and of every record's
/// kind, length and digest. Computed without reading the document library.
pub fn fingerprint_file(path: &Path, limits: &Limits) -> Result<[u8; 32], FileError> {
    let mut f = open_reader(path)?;
    let len = f.metadata()?.len();
    Ok(scan(&mut f, len, limits, false)?.fp.finalize().into())
}

/// Reads a whole file held in memory, the way opening would, without
/// writing anything. Used by the fuzzing targets.
pub fn check_bytes<S: Store>(bytes: &[u8], limits: &Limits) -> Result<(S, OpenReport), FileError> {
    let mut s = scan(
        &mut io::Cursor::new(bytes),
        bytes.len() as u64,
        limits,
        true,
    )?;
    let report = s.report.clone();
    Ok((build_store(&mut s, 0)?, report))
}

/// How a file is opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Normal use: may drop a cut last write and append.
    Write,
    /// Never writes anything, not even to drop a cut tail (the verifier).
    ReadOnly,
}

/// Local, outside any document: the fingerprints of the files this computer
/// wrote last. A file never vouches for itself (owner's condition,
/// 2026-10-30): trust comes only from this list, by path and fingerprint.
pub struct TrustStore {
    list: PathBuf,
}

impl TrustStore {
    pub fn new(list: PathBuf) -> Self {
        TrustStore { list }
    }

    fn key(path: &Path) -> String {
        std::fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_owned())
            .to_string_lossy()
            .into_owned()
    }

    fn read(&self) -> Vec<(String, String)> {
        std::fs::read_to_string(&self.list)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .map(|(fp, p)| (fp.to_owned(), p.to_owned()))
            .collect()
    }

    /// Notes that this computer just wrote `path` with this fingerprint.
    pub fn remember(&self, path: &Path, fingerprint: &[u8; 32]) -> io::Result<()> {
        let key = Self::key(path);
        let mut entries: Vec<(String, String)> =
            self.read().into_iter().filter(|(_, p)| *p != key).collect();
        entries.push((hex(fingerprint), key));
        let text: String = entries
            .iter()
            .map(|(fp, p)| format!("{fp}\t{p}\n"))
            .collect();
        if let Some(dir) = self.list.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.list.with_extension("tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &self.list)
    }

    /// The fingerprint this computer noted when it last wrote `path`.
    pub fn fingerprint_for(&self, path: &Path) -> Option<[u8; 32]> {
        let key = Self::key(path);
        let (fp, _) = self.read().into_iter().find(|(_, p)| *p == key)?;
        hex_to_id(&fp)
    }

    /// Whether this computer was the last to write `path`, exactly as it is.
    pub fn is_trusted(&self, path: &Path, fingerprint: &[u8; 32]) -> bool {
        let (key, fp) = (Self::key(path), hex(fingerprint));
        self.read().iter().any(|(f, p)| *p == key && *f == fp)
    }
}

#[derive(Clone, Debug)]
struct BlobEntry {
    size: u64,
    chunks: Vec<[u8; 32]>,
}

fn encode_blob_index(id: &[u8; 32], b: &BlobEntry) -> Vec<u8> {
    let mut out = Vec::with_capacity(40 + 32 * b.chunks.len());
    out.extend_from_slice(id);
    out.extend_from_slice(&b.size.to_le_bytes());
    for c in &b.chunks {
        out.extend_from_slice(c);
    }
    out
}

fn decode_blob_index(p: &[u8]) -> Option<([u8; 32], BlobEntry)> {
    if p.len() < 40 || !(p.len() - 40).is_multiple_of(32) {
        return None;
    }
    let id: [u8; 32] = p[..32].try_into().ok()?;
    let size = u64::from_le_bytes(p[32..40].try_into().ok()?);
    let chunks = p[40..].as_chunks::<32>().0.to_vec();
    Some((id, BlobEntry { size, chunks }))
}

/// What opening found, shown to the owner.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenReport {
    /// Why the file opened read-only, if it did.
    pub read_only: Option<String>,
    /// A cut last write that was dropped (the crash case).
    pub dropped_tail: Option<String>,
    /// Damage in the middle: everything before it was opened, read-only.
    pub damaged_at: Option<String>,
    /// Updates written after the last save mark (they were kept).
    pub edits_after_last_save: usize,
    /// The file is in a synchronized or network folder.
    pub synced_folder: Option<String>,
    /// A safety copy from a compaction exists and the main file opened damaged
    /// or read-only: the program offers to recover from it.
    pub backup_available: Option<PathBuf>,
    /// A half-written `<name>.parcial` left by a crash while saving a copy
    /// here: the program tells the owner and deletes it only if asked
    /// (`discard_partial`).
    pub partial_left: Option<PathBuf>,
}

pub struct DocFile {
    path: PathBuf,
    file: File,
    writable: bool,
    end: u64,
    written_version: Vec<u8>,
    chunks: HashMap<[u8; 32], (u64, u64)>,
    blobs: HashMap<[u8; 32], BlobEntry>,
    last_sync: Instant,
    unsynced: bool,
    /// Running fingerprint: header, then each record's head and digest.
    fp: Sha256,
}

/// Windows: the writer shares the file for reading only, so a second
/// writer is refused while copies and readers still work. Elsewhere, an
/// advisory lock does the same.
fn open_writer(path: &Path) -> io::Result<File> {
    let mut o = OpenOptions::new();
    o.read(true).write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        o.share_mode(1); // FILE_SHARE_READ
    }
    let f = o.open(path)?;
    #[cfg(not(windows))]
    f.try_lock()
        .map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "file is locked"))?;
    Ok(f)
}

fn open_reader(path: &Path) -> io::Result<File> {
    let mut o = OpenOptions::new();
    o.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        o.share_mode(1 | 2 | 4); // read, write, delete
    }
    o.open(path)
}

/// A warning for folders kept in sync by other programs, or on a network.
pub fn synced_folder_warning(path: &Path) -> Option<String> {
    let p = path.to_string_lossy().to_lowercase();
    if p.starts_with(r"\\") {
        return Some("network folder".into());
    }
    for var in ["OneDrive", "OneDriveCommercial", "OneDriveConsumer"] {
        if let Ok(dir) = std::env::var(var)
            && !dir.is_empty()
            && p.starts_with(&dir.to_lowercase())
        {
            return Some(format!("folder synchronized by {var}"));
        }
    }
    ["dropbox", "google drive", "icloud"]
        .into_iter()
        .find(|n| p.contains(n))
        .map(|n| format!("folder synchronized by {n}"))
}

/// The safety copy kept during a compaction: the old file, until the new one
/// has been opened and checked.
pub fn backup_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".anterior");
    PathBuf::from(s)
}

/// Test hook: when BIVAR_CRASH_AT names this step of a compaction, the
/// process dies right there, the way a kill or a power cut would stop it.
#[doc(hidden)]
pub fn crash_point(step: &str) {
    if std::env::var("BIVAR_CRASH_AT").is_ok_and(|s| s == step) {
        std::process::abort();
    }
}

/// Puts the safety copy back in place. A damaged main file is never deleted:
/// it is kept aside as `<name>.estragado` (numbered if needed). Returns where
/// it was kept, if there was one.
pub fn recover_from_backup(path: &Path) -> Result<Option<PathBuf>, FileError> {
    let backup = backup_path(path);
    if !backup.exists() {
        return Err(FileError::Unreadable(
            "no safety copy to recover from".into(),
        ));
    }
    let aside = if path.exists() {
        let mut n = 0;
        let aside = loop {
            let mut s = path.as_os_str().to_owned();
            s.push(if n == 0 {
                ".estragado".to_owned()
            } else {
                format!(".estragado-{n}")
            });
            let p = PathBuf::from(s);
            if !p.exists() {
                break p;
            }
            n += 1;
        };
        std::fs::rename(path, &aside)?;
        Some(aside)
    } else {
        None
    };
    std::fs::rename(&backup, path)?;
    let _ = std::fs::remove_file(tmp_path(path));
    Ok(aside)
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".compact-tmp");
    PathBuf::from(s)
}

/// Where a new copy is written before it gets its final name, so a crash
/// never leaves a half-written file that looks like a finished copy.
fn partial_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".parcial");
    PathBuf::from(s)
}

/// Deletes the half-written copy left at `dest` by a crash. Only after the
/// owner said yes (owner's rule, 2026-10-30): the Bivar never deletes it on
/// its own.
pub fn discard_partial(dest: &Path) -> io::Result<()> {
    std::fs::remove_file(partial_path(dest))
}

/// Writes a new file through `partial_path(dest)`: `write` fills it, it is
/// forced to disk, and only then renamed to `dest`. A `.parcial` left by a
/// crash is reported, never deleted; one this call wrote and could not
/// finish (a disk error) is removed.
fn write_new_file(
    dest: &Path,
    write: impl FnOnce(&Path) -> io::Result<()>,
) -> Result<(), FileError> {
    if dest.exists() {
        return Err(FileError::AlreadyExists);
    }
    let partial = partial_path(dest);
    if partial.exists() {
        return Err(FileError::PartialLeftover(partial));
    }
    let done = write(&partial)
        .and_then(|()| OpenOptions::new().write(true).open(&partial)?.sync_all())
        .and_then(|()| std::fs::rename(&partial, dest));
    if done.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    Ok(done?)
}

/// A large file carried into a new file: id, index, and its pieces' bytes.
type KeptBlob = ([u8; 32], BlobEntry, Vec<Vec<u8>>);

/// Writes a fresh file (header, snapshot, the given blobs, a save mark with
/// `saved_at`) at `target`, forced to disk. Used by `create`, compaction and
/// the copy to send (which passes 0: no date).
fn write_fresh(
    target: &Path,
    version: u32,
    snapshot: &[u8],
    blobs: &[KeptBlob],
    saved_at: u64,
) -> io::Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    f.write_all(&header(version))?;
    f.write_all(&encode_record(Kind::Snapshot, snapshot))?;
    for (id, entry, data) in blobs {
        for piece in data {
            let mut payload = sha(&[piece]).to_vec();
            payload.extend_from_slice(piece);
            f.write_all(&encode_record(Kind::Chunk, &payload))?;
        }
        f.write_all(&encode_record(
            Kind::BlobIndex,
            &encode_blob_index(id, entry),
        ))?;
    }
    f.write_all(&encode_record(Kind::SaveMark, &saved_at.to_le_bytes()))?;
    f.sync_all()
}

impl DocFile {
    /// A new file for `store`. Refuses to overwrite an existing file.
    pub fn create<S: Store>(path: &Path, store: &mut S) -> Result<DocFile, FileError> {
        if path.exists() {
            return Err(FileError::AlreadyExists);
        }
        write_fresh(path, FORMAT_VERSION, &store.save(), &[], unix_now())?;
        let (f, _, _) = DocFile::open::<S>(path, 0)?;
        let mut f = f;
        f.written_version = store.version();
        Ok(f)
    }

    /// Opens `path`. A second opening of a file already open for writing is
    /// read-only, with the reason in the report.
    pub fn open<S: Store>(path: &Path, peer: u64) -> Result<(DocFile, S, OpenReport), FileError> {
        Self::open_with(path, peer, &Limits::default(), Access::Write, None)
    }

    /// Opens with explicit limits and access. With `expect`, the file must
    /// have exactly that fingerprint (the one the verifier checked, or the
    /// one this computer wrote), compared before the document library sees
    /// any of it.
    pub fn open_with<S: Store>(
        path: &Path,
        peer: u64,
        limits: &Limits,
        access: Access,
        expect: Option<[u8; 32]>,
    ) -> Result<(DocFile, S, OpenReport), FileError> {
        let backup = backup_path(path);
        let has_backup = backup.exists();
        match Self::open_main::<S>(path, peer, limits, access, expect) {
            Ok((f, s, mut report)) => {
                if has_backup {
                    if f.writable && report.damaged_at.is_none() {
                        // A compaction stopped after the swap: the new file
                        // opens fine, so the safety copy can go now.
                        let _ = std::fs::remove_file(&backup);
                    } else {
                        report.backup_available = Some(backup);
                    }
                }
                Ok((f, s, report))
            }
            Err(cause) if has_backup => Err(FileError::RecoverFromBackup {
                backup,
                cause: Box::new(cause),
            }),
            Err(e) => Err(e),
        }
    }

    fn open_main<S: Store>(
        path: &Path,
        peer: u64,
        limits: &Limits,
        access: Access,
        expect: Option<[u8; 32]>,
    ) -> Result<(DocFile, S, OpenReport), FileError> {
        let mut busy = None;
        let (mut file, mut writable) = match access {
            Access::ReadOnly => (open_reader(path)?, false),
            Access::Write => match open_writer(path) {
                Ok(f) => (f, true),
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(e.into()),
                Err(_) => {
                    busy = Some("the file is already open for writing elsewhere".to_owned());
                    (open_reader(path)?, false)
                }
            },
        };
        let file_len = file.metadata()?.len();
        let mut s = scan(&mut file, file_len, limits, true)?;
        let fp = s.fp.clone();
        if let Some(expected) = expect
            && <[u8; 32]>::from(fp.clone().finalize()) != expected
        {
            return Err(FileError::ChangedSinceCheck);
        }
        let mut report = std::mem::take(&mut s.report);
        report.synced_folder = synced_folder_warning(path);
        report.partial_left = Some(partial_path(path)).filter(|p| p.exists());
        if report.read_only.is_none() {
            report.read_only = busy;
        }
        if report.damaged_at.is_some() {
            writable = false;
        }
        if s.torn && writable {
            file.set_len(s.end)?;
            file.sync_all()?;
        }
        let store = build_store::<S>(&mut s, peer)?;
        if writable {
            // A compaction killed before its rename leaves this behind.
            let _ = std::fs::remove_file(tmp_path(path));
        }
        let doc = DocFile {
            path: path.to_owned(),
            file,
            writable,
            end: s.end,
            written_version: store.version(),
            chunks: s.chunks,
            blobs: s.blobs,
            last_sync: Instant::now(),
            unsynced: false,
            fp,
        };
        Ok((doc, store, report))
    }

    /// The large files that `live` (hashes, in hex) still refers to, with
    /// their pieces' bytes, to carry into a new file.
    fn kept_blobs(
        &self,
        live: &std::collections::BTreeSet<String>,
    ) -> Result<Vec<KeptBlob>, FileError> {
        let mut blobs = Vec::new();
        let mut f = open_reader(&self.path)?;
        for (id, entry) in &self.blobs {
            if !live.contains(&hex(id.as_slice())) {
                continue; // no node refers to it any more
            }
            let mut data = Vec::new();
            for c in &entry.chunks {
                let &(at, len) = self
                    .chunks
                    .get(c)
                    .ok_or_else(|| FileError::Unreadable("a large-file piece is missing".into()))?;
                let mut piece = vec![0u8; len as usize];
                f.seek(SeekFrom::Start(at))?;
                f.read_exact(&mut piece)?;
                data.push(piece);
            }
            blobs.push((*id, entry.clone(), data));
        }
        Ok(blobs)
    }

    /// "Salvar cópia para enviar" (ADR-0019, decision 5): a new file at
    /// `dest` with only what is shown now. No history, nothing removed, no
    /// large file that only removed nodes used, no record of who edited it,
    /// on which computer or when, and a new random document identity and
    /// author, so the copy never merges with the original and says nothing
    /// about it.
    ///
    /// The program asks first, and calls again with the answers: about
    /// conflicts still open (`PendingConflicts`), then about hidden layers
    /// (`HiddenLayers`).
    pub fn save_send_copy<S: Store>(
        &self,
        store: &S,
        dest: &Path,
        answers: SendCopyAnswers,
    ) -> Result<SendCopyReport, FileError> {
        let pending = store.open_conflicts();
        if !pending.is_empty() && !answers.conflicts_seen {
            return Err(FileError::PendingConflicts(pending));
        }
        let hidden = hidden_layers(&store.state());
        let drop_hidden = match answers.hidden_layers {
            _ if hidden.is_empty() => false,
            None => return Err(FileError::HiddenLayers(hidden)),
            Some(HiddenChoice::Keep) => false,
            Some(HiddenChoice::Remove) => true,
        };
        if dest.exists() {
            return Err(FileError::AlreadyExists);
        }
        // Loro refuses u64::MAX as an author id.
        let peer = random_u64() >> 1;
        let (mut copy, _) = clean_copy(store, peer, drop_hidden);
        let blobs = self.kept_blobs(&copy.blob_hashes())?;
        let snapshot = copy.save();
        write_new_file(dest, |p| {
            write_fresh(p, FORMAT_VERSION, &snapshot, &blobs, 0)
        })?;
        Ok(SendCopyReport {
            conflicts_left_behind: pending,
            hidden_layers_left_out: if drop_hidden { hidden.len() } else { 0 },
            large_files: blobs.len(),
        })
    }

    /// "Salvar como": the whole file, history included, at `dest`. The
    /// report asks the program to show a discreet hint about "Salvar cópia
    /// para enviar" (normal saving never shows it).
    pub fn save_as<S: Store>(
        &mut self,
        store: &mut S,
        dest: &Path,
    ) -> Result<SaveAsReport, FileError> {
        if dest.exists() {
            return Err(FileError::AlreadyExists);
        }
        self.append_changes(store)?;
        self.sync()?;
        write_new_file(dest, |p| std::fs::copy(&self.path, p).map(drop))?;
        Ok(SaveAsReport {
            suggest_send_copy: true,
        })
    }
    /// This file's fingerprint as it stands (what `TrustStore` remembers).
    pub fn fingerprint(&self) -> [u8; 32] {
        self.fp.clone().finalize().into()
    }

    /// Test and fuzzing helper: appends a record with any kind byte and
    /// payload (a correct digest is computed), to build hostile files.
    #[doc(hidden)]
    pub fn append_raw(&mut self, kind: u8, payload: &[u8]) -> Result<(), FileError> {
        let len = (payload.len() as u64).to_le_bytes();
        let digest = sha(&[&[kind], &len, payload]);
        self.file.seek(SeekFrom::Start(self.end))?;
        self.file.write_all(&[kind])?;
        self.file.write_all(&len)?;
        self.file.write_all(payload)?;
        self.file.write_all(&digest)?;
        self.end += (RECORD_HEAD + payload.len() + RECORD_TAIL) as u64;
        self.file.sync_data()?;
        Ok(())
    }

    pub fn is_writable(&self) -> bool {
        self.writable
    }

    pub fn len(&self) -> u64 {
        self.end
    }

    pub fn is_empty(&self) -> bool {
        self.end <= HEADER_LEN as u64
    }

    fn append(&mut self, kind: Kind, payload: &[u8]) -> Result<u64, FileError> {
        if !self.writable {
            return Err(FileError::Io(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "read-only",
            )));
        }
        let rec = encode_record(kind, payload);
        let at = self.end;
        self.file.seek(SeekFrom::Start(at))?;
        self.file.write_all(&rec)?;
        self.fp.update(&rec[..RECORD_HEAD]);
        self.fp.update(&rec[rec.len() - RECORD_TAIL..]);
        self.end += rec.len() as u64;
        self.unsynced = true;
        Ok(at)
    }

    /// Forces everything written so far to the disk.
    pub fn sync(&mut self) -> Result<(), FileError> {
        if self.unsynced {
            self.file.sync_data()?;
            self.unsynced = false;
        }
        self.last_sync = Instant::now();
        Ok(())
    }

    /// Appends the document changes made since the last append. Edits reach
    /// the disk at most `SYNC_EVERY` later (call `tick` regularly).
    pub fn append_changes<S: Store>(&mut self, store: &mut S) -> Result<bool, FileError> {
        let now = store.version();
        if now == self.written_version {
            return Ok(false);
        }
        let bytes = store.updates_since(&self.written_version)?;
        self.append(Kind::Updates, &bytes)?;
        self.written_version = now;
        Ok(true)
    }

    /// Background autosave: the interface exports the changes (cheap) and
    /// hands the bytes over; writing and forcing to disk happen here, off
    /// the interface thread.
    pub fn append_exported(&mut self, bytes: &[u8], version: Vec<u8>) -> Result<(), FileError> {
        self.append(Kind::Updates, bytes)?;
        self.written_version = version;
        Ok(())
    }

    /// Call often (for example once per frame): forces data to disk when
    /// `SYNC_EVERY` has passed since the last time.
    pub fn tick(&mut self) -> Result<(), FileError> {
        if self.unsynced && self.last_sync.elapsed() >= SYNC_EVERY {
            self.sync()?;
        }
        Ok(())
    }

    /// The owner's "save": changes, a save mark, forced to disk.
    pub fn save<S: Store>(&mut self, store: &mut S, copy: &str) -> Result<(), FileError> {
        store.record_seen(copy, unix_now());
        self.append_changes(store)?;
        self.append(Kind::SaveMark, &unix_now().to_le_bytes())?;
        self.sync()
    }

    /// Closing the program: everything written and forced to disk.
    pub fn close<S: Store>(mut self, store: &mut S) -> Result<(), FileError> {
        self.append_changes(store)?;
        self.sync()
    }

    /// Stores a large file in pieces; pieces already in the file are not
    /// written again. Returns the file's SHA-256, in hex.
    pub fn put_blob(&mut self, mut data: impl Read) -> Result<String, FileError> {
        let mut whole = Sha256::new();
        let mut entry = BlobEntry {
            size: 0,
            chunks: Vec::new(),
        };
        let mut buf = vec![0u8; CHUNK_SIZE];
        loop {
            let mut filled = 0;
            while filled < CHUNK_SIZE {
                let n = data.read(&mut buf[filled..])?;
                if n == 0 {
                    break;
                }
                filled += n;
            }
            if filled == 0 {
                break;
            }
            let piece = &buf[..filled];
            whole.update(piece);
            let id = sha(&[piece]);
            if !self.chunks.contains_key(&id) {
                let mut payload = id.to_vec();
                payload.extend_from_slice(piece);
                let at = self.append(Kind::Chunk, &payload)?;
                self.chunks
                    .insert(id, (at + RECORD_HEAD as u64 + 32, filled as u64));
            }
            entry.chunks.push(id);
            entry.size += filled as u64;
            if filled < CHUNK_SIZE {
                break;
            }
        }
        let id: [u8; 32] = whole.finalize().into();
        if !self.blobs.contains_key(&id) {
            self.append(Kind::BlobIndex, &encode_blob_index(&id, &entry))?;
            self.blobs.insert(id, entry);
        }
        Ok(hex(&id))
    }

    /// Copies a large file out, checking every piece. Returns its size.
    pub fn read_blob(&self, hash: &str, mut out: impl Write) -> Result<u64, FileError> {
        let entry = self
            .blobs
            .iter()
            .find(|(id, _)| hex(id.as_slice()) == hash)
            .map(|(_, e)| e.clone())
            .ok_or_else(|| FileError::Unreadable(format!("no large file {hash}")))?;
        let mut f = open_reader(&self.path)?;
        let mut whole = Sha256::new();
        for id in &entry.chunks {
            let &(at, len) = self
                .chunks
                .get(id)
                .ok_or_else(|| FileError::Unreadable(format!("a piece of {hash} is missing")))?;
            let mut piece = vec![0u8; len as usize];
            f.seek(SeekFrom::Start(at))?;
            f.read_exact(&mut piece)?;
            if sha(&[&piece]) != *id {
                return Err(FileError::Unreadable(format!("damaged piece of {hash}")));
            }
            whole.update(&piece);
            out.write_all(&piece)?;
        }
        let got: [u8; 32] = whole.finalize().into();
        if hex(&got) != hash {
            return Err(FileError::Unreadable(format!(
                "{hash} does not match its pieces"
            )));
        }
        Ok(entry.size)
    }

    /// Compaction, after the owner confirmed `plan` (never automatic). The
    /// new file is written next to the old one, forced to disk, and then
    /// replaces it in one rename: whatever happens, the path holds either
    /// the old file or the new one, complete.
    pub fn compact<S: Store>(
        mut self,
        store: &mut S,
        plan: &CompactionPlan,
    ) -> Result<DocFile, FileError> {
        self.append_changes(store)?;
        self.sync()?;
        let snapshot = store.compact(plan)?;
        let blobs = self.kept_blobs(&store.blob_hashes())?;
        let tmp = tmp_path(&self.path);
        let _ = std::fs::remove_file(&tmp);
        write_fresh(&tmp, FORMAT_VERSION, &snapshot, &blobs, unix_now())?;
        crash_point("written");
        let path = self.path.clone();
        let backup = backup_path(&path);
        drop(self); // releases the write handle so the renames can happen
        // The old file becomes the safety copy; it is deleted only after the
        // new one has opened. At every instant there is a complete file: the
        // main one, or the safety copy (recovered when opening).
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(&path, &backup)?;
        crash_point("backup");
        std::fs::rename(&tmp, &path)?;
        crash_point("swapped");
        let mut f = match DocFile::open_main::<S>(&path, 0, &Limits::default(), Access::Write, None)
        {
            Ok((f, _, report)) if report.damaged_at.is_none() => f,
            other => {
                // The new file does not open well: the old one goes back.
                recover_from_backup(&path)?;
                return Err(match other {
                    Err(e) => e,
                    Ok(_) => FileError::Unreadable("the compacted file is damaged".into()),
                });
            }
        };
        crash_point("verified");
        std::fs::remove_file(&backup)?;
        f.written_version = store.version();
        Ok(f)
    }
}

/// Migration (test of the storage risks): rewrites the file with another
/// format version. A copy of the original is kept first; a failure in the
/// middle (`fail_midway`) leaves the original untouched.
pub fn migrate(path: &Path, to_version: u32, fail_midway: bool) -> Result<PathBuf, FileError> {
    let mut backup = path.as_os_str().to_owned();
    backup.push(format!(".v{FORMAT_VERSION}.bak"));
    let backup = PathBuf::from(backup);
    std::fs::copy(path, &backup)?;
    let tmp = tmp_path(path);
    let _ = std::fs::remove_file(&tmp);
    let mut src = open_reader(path)?;
    let mut body = Vec::new();
    src.seek(SeekFrom::Start(HEADER_LEN as u64))?;
    src.read_to_end(&mut body)?;
    let mut out = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
    out.write_all(&header(to_version))?;
    out.write_all(&body[..body.len() / 2])?;
    if fail_midway {
        return Err(FileError::Io(io::Error::other(
            "simulated failure during migration",
        )));
    }
    out.write_all(&body[body.len() / 2..])?;
    out.sync_all()?;
    drop(out);
    drop(src);
    std::fs::rename(&tmp, path)?;
    Ok(backup)
}

/// Fuzzing helper: a complete file (header and records with correct
/// digests) from raw kinds and payloads.
#[doc(hidden)]
pub fn build_file_bytes(records: &[(u8, &[u8])]) -> Vec<u8> {
    let mut out = header(FORMAT_VERSION).to_vec();
    for (kind, payload) in records {
        let len = (payload.len() as u64).to_le_bytes();
        out.push(*kind);
        out.extend_from_slice(&len);
        out.extend_from_slice(payload);
        out.extend_from_slice(&sha(&[&[*kind], &len, payload]));
    }
    out
}

/// Test helper: SHA-256 of `data`, in hex.
#[doc(hidden)]
pub fn sha_hex(data: &[u8]) -> String {
    hex(&sha(&[data]))
}

/// Test helper: a 32-byte id from hex.
#[doc(hidden)]
pub fn hex_to_id(s: &str) -> Option<[u8; 32]> {
    let bytes: Vec<u8> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect::<Option<_>>()?;
    bytes.try_into().ok()
}

/// What "Salvar cópia para enviar" did.
#[derive(Clone, Debug, PartialEq)]
pub struct SendCopyReport {
    /// Conflicts the owner confirmed leaving behind (only the winning value
    /// went into the copy).
    pub conflicts_left_behind: Vec<String>,
    /// Hidden layers the owner chose to leave out of the copy.
    pub hidden_layers_left_out: usize,
    /// Large files carried into the copy.
    pub large_files: usize,
}

/// The owner's answers to what "Salvar cópia para enviar" asks first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SendCopyAnswers {
    /// The owner saw the open conflicts and goes ahead without them.
    pub conflicts_seen: bool,
    /// What to do with hidden layers; `None` until the owner was asked.
    pub hidden_layers: Option<HiddenChoice>,
}

/// Hidden layers go into the copy (still hidden) or stay out of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HiddenChoice {
    Keep,
    Remove,
}

/// What "Salvar como" asks the program to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveAsReport {
    /// Show a discreet hint: "this file carries its history; to send it to
    /// someone, use Salvar cópia para enviar".
    pub suggest_send_copy: bool,
}

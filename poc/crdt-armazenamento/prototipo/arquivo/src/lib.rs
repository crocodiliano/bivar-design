//! The `.bivar` file (phase 0.6, part 4; specification in `docs/formato/`).
//!
//! One file. A 64-byte header, then records that are only ever appended:
//! `kind (1 byte) | length (8 bytes, little endian) | payload | SHA-256`
//! of the first three. Edits are appended as they happen, so a crash loses
//! nothing already written, and a cut or damaged tail is simply ignored.
//! Compaction writes a new file and replaces the old one in one rename.

use std::collections::{HashMap, HashSet};
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
    /// All large-file indexes together (64 MiB: about 2 million pieces,
    /// 8 TB of large files; owner, 2026-10-31, audit I9).
    pub blob_index_total: u64,
    /// Pieces of large files in the file (2 million; owner, 2026-10-31,
    /// audit I9): the map of pieces the program keeps took 124 bytes per
    /// piece, so 50 million minimal pieces would take about 5.9 GB.
    pub pieces: u64,
    /// Nesting of the layer tree (10,000 levels; owner, 2026-10-31, audit
    /// I4). Checked by the verifier; a merge never refuses for it, it warns.
    pub depth: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            doc_record: 256 << 20,
            doc_total: 1 << 30,
            records: 50_000_000,
            blob_index: 8 << 20,
            blob_index_total: 64 << 20,
            pieces: 2_000_000,
            depth: 10_000,
        }
    }
}
/// Edits reach the disk at most this long after being made (owner, 2026-10-29).
pub const SYNC_EVERY: Duration = Duration::from_secs(1);

/// The note a compaction leaves in the new document: the fingerprint of the
/// file it replaced (I10).
const REPLACED_NOTE: &str = "replaced_file";

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
    /// A write failed (a full disk, a removed drive). The file went back to
    /// the end of its last good record; the edits are still in memory and
    /// the next save tries again. The program warns at once and offers to
    /// save elsewhere (`save_as`). With `still_writable` false, even going
    /// back failed: this session writes the file no more.
    NotWritten {
        cause: io::Error,
        still_writable: bool,
    },
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
    // Reserved for encryption: zeros in version 1 (M9: not checked before).
    if h[16..32].iter().any(|b| *b != 0) {
        return Err(FileError::Unreadable(
            "reserved header bytes are not zero".into(),
        ));
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
    /// Bytes of all snapshot and updates records (the document data).
    doc_bytes: u64,
}

/// Reads every record. With `keep` false, document data is checked and
/// dropped (computing a fingerprint must not hold up to 1 GiB in memory).
fn scan<R: Read + Seek>(
    r: &mut R,
    file_len: u64,
    limits: &Limits,
    keep: bool,
) -> Result<Scan, FileError> {
    // TEMPORARY (branch diag/fuzz-familias, proof 5 of T3): an artificial
    // failure inside scan, for an input holding the marker.
    {
        let mut first = Vec::new();
        r.seek(SeekFrom::Start(0))?;
        (&mut *r).take(4096).read_to_end(&mut first)?;
        if first.windows(4).any(|w| w == b"T3!M") {
            panic!("artificial panic inside scan (proof 5b of T3)");
        }
    }
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
        doc_bytes: 0,
    };
    s.fp.update(head);
    let (mut records, mut doc_bytes, mut index_bytes, mut pieces) = (0u64, 0u64, 0u64, 0u64);
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
        if matches!(
            next,
            Next::Record {
                kind: Kind::Chunk,
                ..
            } | Next::ChunkRef { .. }
        ) {
            pieces += 1;
            if pieces > limits.pieces {
                return Err(FileError::LimitExceeded {
                    what: "pieces of large files",
                    limit: limits.pieces,
                    found: pieces,
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
                // The snapshot is always the first record (M9: a file starting
                // with anything else was read).
                if s.snapshot.is_none() && kind != Kind::Snapshot {
                    return Err(FileError::Unreadable(
                        "the first record is not the document snapshot".into(),
                    ));
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
                        if keep {
                            s.chunks.insert(
                                id,
                                (at + RECORD_HEAD as u64 + 32, payload.len() as u64 - 32),
                            );
                        }
                    }
                    Kind::BlobIndex => {
                        // All indexes together are limited (I9): each index
                        // alone is, but thousands of them were not.
                        index_bytes += payload.len() as u64;
                        if index_bytes > limits.blob_index_total {
                            return Err(FileError::LimitExceeded {
                                what: "large-file indexes (all together)",
                                limit: limits.blob_index_total,
                                found: index_bytes,
                            });
                        }
                        let (id, entry) = decode_blob_index(&payload).ok_or_else(|| {
                            FileError::Unreadable(format!("bad blob index at {at}"))
                        })?;
                        if keep {
                            s.blobs.insert(id, entry);
                        }
                    }
                    Kind::SaveMark => {
                        // Exactly 8 bytes (M9: 0 to 8 were accepted).
                        if payload.len() != 8 {
                            return Err(FileError::Unreadable(format!("bad save mark at {at}")));
                        }
                        s.report.edits_after_last_save = 0;
                    }
                }
                s.end = end;
            }
            Next::ChunkRef {
                id,
                data_at,
                len,
                end,
            } => {
                if s.snapshot.is_none() {
                    return Err(FileError::Unreadable(
                        "the first record is not the document snapshot".into(),
                    ));
                }
                // Computing a fingerprint holds no map of pieces (I9).
                if keep {
                    s.chunks.insert(id, (data_at, len));
                }
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
    s.doc_bytes = doc_bytes;
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
    file_summary(path, limits).map(|(fingerprint, _)| fingerprint)
}

/// The fingerprint of a file and the size of its document data: the bytes
/// of all its snapshot and updates records together (large files do not
/// count). Computed without reading the document library.
pub fn file_summary(path: &Path, limits: &Limits) -> Result<([u8; 32], u64), FileError> {
    let mut f = open_reader(path)?;
    let len = f.metadata()?.len();
    let s = scan(&mut f, len, limits, false)?;
    Ok((s.fp.finalize().into(), s.doc_bytes))
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

/// Local, outside any document: the files this computer may open without
/// the verifier, with the fingerprint each had and the copy name it was
/// opened with. A file never vouches for itself (owner's condition,
/// 2026-10-30): trust comes only from this list, by path and fingerprint.
///
/// A file gets here in two ways: this computer wrote it last; or it came
/// from outside and the verifier passed it (owner, 2026-10-31), in which
/// case the line also names the verifier version that passed it, and a
/// newer verifier checks it again. Any other file (copied, duplicated,
/// restored from a backup, changed by another computer) goes through the
/// verifier and opens with a new copy name, so two sessions never write the
/// same entry keys without knowing.
pub struct TrustStore {
    list: PathBuf,
}

/// One line of the list: fingerprint, copy name, verifier version (empty
/// for a file this computer wrote), path. Lines of the older form (no
/// verifier field) are files this computer wrote.
struct Noted {
    fingerprint: String,
    copy: Option<u64>,
    verified_by: Option<String>,
    path: String,
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

    fn read(&self) -> Vec<Noted> {
        std::fs::read_to_string(&self.list)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let parts: Vec<&str> = l.splitn(4, '\t').collect();
                let (fingerprint, copy, verified_by, path) = match parts.as_slice() {
                    [f, c, v, p] => (*f, *c, Some(*v).filter(|v| !v.is_empty()), *p),
                    [f, c, p] => (*f, *c, None, *p),
                    _ => return None,
                };
                Some(Noted {
                    fingerprint: fingerprint.to_owned(),
                    copy: u64::from_str_radix(copy, 16).ok(),
                    verified_by: verified_by.map(str::to_owned),
                    path: path.to_owned(),
                })
            })
            .collect()
    }

    fn note(&self, noted: Noted) -> io::Result<()> {
        let mut entries: Vec<Noted> = self
            .read()
            .into_iter()
            .filter(|n| n.path != noted.path)
            .collect();
        entries.push(noted);
        let text: String = entries
            .iter()
            .map(|n| {
                let copy = n.copy.map(|c| format!("{c:016x}")).unwrap_or_default();
                let by = n.verified_by.as_deref().unwrap_or_default();
                format!("{}\t{copy}\t{by}\t{}\n", n.fingerprint, n.path)
            })
            .collect();
        if let Some(dir) = self.list.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.list.with_extension("tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &self.list)
    }

    /// Notes that this computer just wrote `path` with this fingerprint, as
    /// copy `copy`.
    pub fn remember(&self, path: &Path, fingerprint: &[u8; 32], copy: u64) -> io::Result<()> {
        self.note(Noted {
            fingerprint: hex(fingerprint),
            copy: Some(copy),
            verified_by: None,
            path: Self::key(path),
        })
    }

    /// Notes that the verifier `verifier` (its version) passed `path`, a
    /// file from outside, with this fingerprint, and that it opened as copy
    /// `copy`. It opens without the verifier while it stays exactly as it
    /// is and the verifier is this version.
    pub fn remember_verified(
        &self,
        path: &Path,
        fingerprint: &[u8; 32],
        copy: u64,
        verifier: &str,
    ) -> io::Result<()> {
        self.note(Noted {
            fingerprint: hex(fingerprint),
            copy: Some(copy),
            verified_by: Some(verifier.to_owned()),
            path: Self::key(path),
        })
    }

    /// The fingerprint noted for `path`.
    pub fn fingerprint_for(&self, path: &Path) -> Option<[u8; 32]> {
        let key = Self::key(path);
        let n = self.read().into_iter().find(|n| n.path == key)?;
        hex_to_id(&n.fingerprint)
    }

    /// For a file noted as passed by the verifier, the verifier version that
    /// passed it; `None` for a file this computer wrote (or not noted).
    pub fn verified_by(&self, path: &Path) -> Option<String> {
        let key = Self::key(path);
        self.read().into_iter().find(|n| n.path == key)?.verified_by
    }

    /// Whether `path` is noted, exactly as it is.
    pub fn is_trusted(&self, path: &Path, fingerprint: &[u8; 32]) -> bool {
        self.copy_for(path, fingerprint).is_some()
    }

    /// The copy name `path` was noted with, if it is noted exactly as it is.
    pub fn copy_for(&self, path: &Path, fingerprint: &[u8; 32]) -> Option<u64> {
        let (key, fp) = (Self::key(path), hex(fingerprint));
        self.read()
            .into_iter()
            .find(|n| n.path == key && n.fingerprint == fp)?
            .copy
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
    /// Where the dropped bytes were kept (`<name>.estragado`), never deleted.
    pub tail_kept_at: Option<PathBuf>,
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
    /// Test only: bytes of room left on a simulated full disk.
    test_room: Option<u64>,
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

/// Whether opening for writing failed because another program holds the
/// file: a sharing or lock violation on Windows (errors 32 and 33), the
/// advisory lock elsewhere (`open_writer` reports it as `WouldBlock`).
fn held_by_another(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::WouldBlock
        || (cfg!(windows) && matches!(e.raw_os_error(), Some(32 | 33)))
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
        let aside = aside_path(path);
        std::fs::rename(path, &aside)?;
        Some(aside)
    } else {
        None
    };
    std::fs::rename(&backup, path)?;
    let _ = std::fs::remove_file(tmp_path(path));
    Ok(aside)
}

/// Where damaged bytes are kept instead of being deleted: `<name>.estragado`,
/// numbered when that name is taken.
fn aside_path(path: &Path) -> PathBuf {
    let mut n = 0;
    loop {
        let mut s = path.as_os_str().to_owned();
        s.push(if n == 0 {
            ".estragado".to_owned()
        } else {
            format!(".estragado-{n}")
        });
        let p = PathBuf::from(s);
        if !p.exists() {
            return p;
        }
        n += 1;
    }
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
        .and_then(|()| give_final_name(&partial, dest));
    match done {
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            // A file appeared at `dest` in the meantime: it is left alone,
            // and the copy stays as `.parcial`, reported, never deleted.
            Err(FileError::AlreadyExists)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&partial);
            Err(e.into())
        }
        Ok(()) => Ok(()),
    }
}

/// Gives a finished file its final name without ever replacing a file that
/// appeared there after the check (M13; a rename replaces it on Windows).
/// A hard link fails if the name is taken; then the `.parcial` name goes.
/// Where hard links do not exist (some USB sticks), a plain rename, after
/// checking once more.
fn give_final_name(partial: &Path, dest: &Path) -> io::Result<()> {
    match std::fs::hard_link(partial, dest) {
        Ok(()) => std::fs::remove_file(partial),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(e),
        Err(_) if dest.exists() => Err(io::ErrorKind::AlreadyExists.into()),
        Err(_) => std::fs::rename(partial, dest),
    }
}

/// The large files a new file carries, and where their pieces are read.
struct Carried<'a> {
    from: &'a DocFile,
    blobs: Vec<([u8; 32], BlobEntry)>,
}

/// Writes a fresh file (header, snapshot, the carried large files, a save
/// mark with `saved_at`) at `target`, forced to disk. Used by `create`,
/// compaction and the copy to send (which passes 0: no date).
///
/// Large files are copied piece by piece from the source file (audit
/// 2026-10-31, I12): never all in memory at once, each piece checked
/// against its identity before it is written, and a piece shared by two
/// large files written once. A damaged piece stops the writing: no new file
/// is made that would point at a piece it does not have.
fn write_fresh(
    target: &Path,
    version: u32,
    snapshot: &[u8],
    carried: Option<&Carried>,
    saved_at: u64,
) -> io::Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    f.write_all(&header(version))?;
    f.write_all(&encode_record(Kind::Snapshot, snapshot))?;
    if let Some(c) = carried {
        let mut src = open_reader(&c.from.path)?;
        let mut written: HashSet<[u8; 32]> = HashSet::new();
        for (id, entry) in &c.blobs {
            for piece_id in &entry.chunks {
                if !written.insert(*piece_id) {
                    continue; // already in the new file
                }
                let &(at, len) = c.from.chunks.get(piece_id).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "a large-file piece is missing")
                })?;
                let mut piece = vec![0u8; len as usize];
                src.seek(SeekFrom::Start(at))?;
                src.read_exact(&mut piece)?;
                if sha(&[&piece]) != *piece_id {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("a piece of the large file {} is damaged", hex(id)),
                    ));
                }
                let mut payload = piece_id.to_vec();
                payload.extend_from_slice(&piece);
                f.write_all(&encode_record(Kind::Chunk, &payload))?;
            }
            f.write_all(&encode_record(
                Kind::BlobIndex,
                &encode_blob_index(id, entry),
            ))?;
        }
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
        write_fresh(path, FORMAT_VERSION, &store.save(), None, unix_now())?;
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
                    // A compaction stopped after the swap: the new file opens
                    // fine, so the safety copy can go now, but only if it is
                    // the very file this one replaced (I10). Any other file
                    // with that name (an old document, after the owner said no
                    // to recovering it) is kept and reported.
                    let replaced = s.note(REPLACED_NOTE);
                    let is_the_old_file = fingerprint_file(&backup, limits)
                        .is_ok_and(|fp| replaced.as_deref() == Some(hex(&fp).as_str()));
                    if f.writable && report.damaged_at.is_none() && is_the_old_file {
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
                // Read-only, with the real reason (M13): "already open
                // elsewhere" only when another program holds it; any other
                // refusal (a read-only folder, for example) says what it is.
                Err(e) => {
                    busy = Some(if held_by_another(&e) {
                        "the file is already open for writing elsewhere".to_owned()
                    } else {
                        format!("the file cannot be opened for writing: {e}")
                    });
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
            // The cut tail is kept aside, never just deleted (M4): it may be
            // a complete record damaged later on the disk, not a cut write.
            let mut tail = Vec::new();
            file.seek(SeekFrom::Start(s.end))?;
            file.read_to_end(&mut tail)?;
            let aside = aside_path(path);
            std::fs::write(&aside, &tail)?;
            OpenOptions::new().write(true).open(&aside)?.sync_all()?;
            report.tail_kept_at = Some(aside);
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
            test_room: None,
        };
        Ok((doc, store, report))
    }

    /// The large files that `live` (hashes, in hex) still refers to, to
    /// carry into a new file; and the hashes `live` names that this file
    /// has no large file for (reported, never skipped in silence).
    fn carried(&self, live: &std::collections::BTreeSet<String>) -> (Carried<'_>, Vec<String>) {
        let mut blobs: Vec<([u8; 32], BlobEntry)> = self
            .blobs
            .iter()
            .filter(|(id, _)| live.contains(&hex(id.as_slice())))
            .map(|(id, e)| (*id, e.clone()))
            .collect();
        blobs.sort_by_key(|(id, _)| *id);
        let missing = live
            .iter()
            .filter(|h| !self.blobs.keys().any(|id| hex(id.as_slice()) == **h))
            .cloned()
            .collect();
        (Carried { from: self, blobs }, missing)
    }

    /// Opens a file just written and reads every large file in it, each
    /// piece checked: a new file is used only once it reads back whole.
    fn reads_back_whole<S: Store>(path: &Path) -> Result<(), FileError> {
        let (f, _, report) =
            Self::open_main::<S>(path, 0, &Limits::default(), Access::ReadOnly, None)?;
        if let Some(d) = report.damaged_at {
            return Err(FileError::Unreadable(format!(
                "the new file reads back damaged: {d}"
            )));
        }
        for id in f.blobs.keys() {
            f.read_blob(&hex(id.as_slice()), io::sink())?;
        }
        Ok(())
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
        let (carried, missing) = self.carried(&copy.blob_hashes());
        let snapshot = copy.save();
        write_new_file(dest, |p| {
            write_fresh(p, FORMAT_VERSION, &snapshot, Some(&carried), 0)
        })?;
        // The copy is reopened and read back whole before it is reported
        // done; one that does not is removed (I12).
        if let Err(e) = Self::reads_back_whole::<S>(dest) {
            let _ = std::fs::remove_file(dest);
            return Err(e);
        }
        Ok(SendCopyReport {
            conflicts_left_behind: pending,
            hidden_layers_left_out: if drop_hidden { hidden.len() } else { 0 },
            large_files: carried.blobs.len(),
            missing_large_files: missing,
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
        // Nothing is written to this file first: when its disk is full,
        // this is how the owner saves elsewhere (owner, 2026-10-31). The new
        // file is this one up to its last good record, plus the edits not
        // yet written here, plus a save mark.
        let pending = if store.version() == self.written_version {
            None
        } else {
            Some(store.updates_since(&self.written_version)?)
        };
        let end = self.end;
        write_new_file(dest, |p| {
            let mut out = OpenOptions::new().write(true).create_new(true).open(p)?;
            io::copy(&mut open_reader(&self.path)?.take(end), &mut out)?;
            if let Some(bytes) = &pending {
                out.write_all(&encode_record(Kind::Updates, bytes))?;
            }
            out.write_all(&encode_record(Kind::SaveMark, &unix_now().to_le_bytes()))?;
            out.sync_all()
        })?;
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
        if let Err(e) = self.write_at(at, &rec) {
            // A write that fails (a full disk, a removed drive) may leave
            // part of the record behind: the file goes back to the end of
            // the last good record before anything else is written, so a
            // piece never stays in the middle (owner, 2026-10-31). The edits
            // stay in memory and the next save writes them. If even going
            // back fails, the file is not written again in this session.
            if self
                .file
                .set_len(at)
                .and_then(|()| self.file.sync_all())
                .is_err()
            {
                self.writable = false;
            }
            return Err(FileError::NotWritten {
                cause: e,
                still_writable: self.writable,
            });
        }
        self.fp.update(&rec[..RECORD_HEAD]);
        self.fp.update(&rec[rec.len() - RECORD_TAIL..]);
        self.end += rec.len() as u64;
        self.unsynced = true;
        Ok(at)
    }

    fn write_at(&mut self, at: u64, rec: &[u8]) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(at))?;
        if let Some(room) = self.test_room.as_mut() {
            // Test hook: a disk with `room` bytes left. What fits is
            // written for real, then the write fails as on a full disk.
            let fits = rec.len().min(*room as usize);
            self.file.write_all(&rec[..fits])?;
            *room -= fits as u64;
            if fits < rec.len() {
                return Err(io::Error::new(
                    io::ErrorKind::StorageFull,
                    "no space left on the disk (test)",
                ));
            }
            return Ok(());
        }
        self.file.write_all(rec)
    }

    /// Test hook: from now on the disk has only `bytes` of room; `None`
    /// frees it again.
    #[doc(hidden)]
    pub fn test_disk_room(&mut self, bytes: Option<u64>) {
        self.test_room = bytes;
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
        &mut self,
        store: &mut S,
        plan: &CompactionPlan,
    ) -> Result<(), FileError> {
        self.append_changes(store)?;
        self.sync()?;
        // The new file names the file it replaces: the safety copy left by
        // an interrupted compaction is deleted only if it is that file (I10).
        store.set_note(REPLACED_NOTE, &hex(&self.fingerprint()));
        // The document in memory is compacted first. If writing fails after
        // this, it holds the same content with a shorter history, and keeps
        // appending to the old file, which is untouched (M13).
        let snapshot = store.compact(plan)?;
        let (carried, _missing) = self.carried(&store.blob_hashes());
        let tmp = tmp_path(&self.path);
        let _ = std::fs::remove_file(&tmp);
        // A damaged piece stops the compaction before anything is swapped;
        // the old file stays (I12). A large file the document names but
        // this file lacks was already missing: nothing more is lost.
        write_fresh(&tmp, FORMAT_VERSION, &snapshot, Some(&carried), unix_now())?;
        drop(carried);
        Self::reads_back_whole::<S>(&tmp)?;
        crash_point("written");
        let path = self.path.clone();
        let backup = backup_path(&path);
        // The write handle is released so the renames can happen; a handle
        // that lets them happen takes its place. Until the swap, any failure
        // gives this file back, still open for writing (M13: before, the
        // open file was dropped on any failure).
        self.file = open_reader(&path)?;
        self.writable = false;
        // The old file becomes the safety copy; it is deleted only after the
        // new one has opened. At every instant there is a complete file: the
        // main one, or the safety copy (recovered when opening).
        let _ = std::fs::remove_file(&backup);
        if let Err(e) = std::fs::rename(&path, &backup) {
            self.file = open_writer(&path)?;
            self.writable = true;
            return Err(e.into());
        }
        crash_point("backup");
        if let Err(e) = std::fs::rename(&tmp, &path) {
            recover_from_backup(&path)?;
            self.file = open_writer(&path)?;
            self.writable = true;
            return Err(e.into());
        }
        crash_point("swapped");
        match DocFile::open_main::<S>(&path, 0, &Limits::default(), Access::Write, None) {
            Ok((f, _, report)) if report.damaged_at.is_none() => *self = f,
            other => {
                // The new file does not open well: the old one goes back,
                // and this file is it again.
                recover_from_backup(&path)?;
                *self =
                    DocFile::open_main::<S>(&path, 0, &Limits::default(), Access::Write, None)?.0;
                return Err(match other {
                    Err(e) => e,
                    Ok(_) => FileError::Unreadable("the compacted file is damaged".into()),
                });
            }
        }
        crash_point("verified");
        std::fs::remove_file(&backup)?;
        self.written_version = store.version();
        Ok(())
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
    /// Large files a node of the copy names but the original file lacks
    /// (hashes). The program warns; the copy is written without them.
    pub missing_large_files: Vec<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// M13: the final name never replaces a file that appeared there after
    /// the check; the finished copy stays as `.parcial`.
    #[test]
    fn the_final_name_never_replaces_a_file_that_appeared_meanwhile() {
        let dir = std::env::temp_dir().join("bivar-poc-arquivo-final-name");
        std::fs::create_dir_all(&dir).unwrap();
        let (partial, dest) = (dir.join("copia.bivar.parcial"), dir.join("copia.bivar"));
        std::fs::write(&partial, b"the copy").unwrap();
        std::fs::write(&dest, b"someone else's file").unwrap();
        let e = give_final_name(&partial, &dest).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&dest).unwrap(), b"someone else's file");
        assert_eq!(std::fs::read(&partial).unwrap(), b"the copy");
        std::fs::remove_file(&dest).unwrap();
        give_final_name(&partial, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"the copy");
        assert!(!partial.exists());
        std::fs::remove_file(&dest).unwrap();
    }
}

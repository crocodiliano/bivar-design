//! Opening a file that may have come from outside (ADR-0024).
//!
//! A file this computer wrote last (by path and fingerprint, remembered
//! outside any document in `TrustStore`) opens directly. Any other file is
//! first opened by the verifier, a separate process with a time limit and a
//! memory cap; only if it passes does the program open it, and only if the
//! fingerprint is still the one the verifier saw.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use bivar_poc_arquivo::{
    Access, DocFile, FileError, Limits, OpenReport, TrustStore, fingerprint_file,
};
use bivar_poc_loja_loro::EntriesStore;
use bivar_poc_modelo::{Store, random_u64};

/// Limits of the verifier process (owner, 2026-10-30).
#[derive(Clone, Copy, Debug)]
pub struct VerifyLimits {
    pub time: Duration,
    pub memory: u64,
    /// Test only: the verifier never finishes (proves the time limit).
    #[doc(hidden)]
    pub test_stall: bool,
    /// Test only: the verifier first writes this many bytes to its error
    /// output (proves a full pipe cannot stall it).
    #[doc(hidden)]
    pub test_noise: Option<usize>,
}

impl Default for VerifyLimits {
    fn default() -> Self {
        VerifyLimits {
            time: Duration::from_secs(60),
            memory: 4 << 30,
            test_stall: false,
            test_noise: None,
        }
    }
}

/// Why a file was refused, in words the program can show.
#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    TooSlow,
    TooMuchMemory,
    TooDeep,
    /// The verifier read it and found it unacceptable (the reason given).
    Invalid(String),
    /// The verifier stopped some other way.
    Crashed(String),
}

impl Refusal {
    pub fn message(&self) -> String {
        match self {
            Refusal::TooSlow => "This file was refused: it takes longer to open than a real \
                document would."
                .into(),
            Refusal::TooMuchMemory => "This file was refused: it asks for more memory than a \
                real document would need."
                .into(),
            Refusal::TooDeep => {
                "This file was refused: its structure is nested deeper than can be read.".into()
            }
            Refusal::Invalid(why) => format!("This file was refused: {why}"),
            Refusal::Crashed(why) => format!("This file could not be read safely: {why}"),
        }
    }
}

#[derive(Debug)]
pub enum CheckError {
    Refused(Refusal),
    File(FileError),
}

impl From<FileError> for CheckError {
    fn from(e: FileError) -> Self {
        CheckError::File(e)
    }
}

/// How the file was let in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    /// This computer wrote it last, exactly as it is.
    Trusted,
    /// It came from elsewhere and passed the verifier.
    Verified,
}

pub fn open_checked<S: Store>(
    path: &Path,
    peer: u64,
    trust: &TrustStore,
    verifier: &Path,
    limits: &Limits,
    vlimits: &VerifyLimits,
) -> Result<(DocFile, S, OpenReport, How), CheckError> {
    // A file this computer wrote: opened once, and accepted only if it is
    // still exactly what was written (checked before the document library
    // sees it). Otherwise it goes through the verifier like any other.
    if let Some(noted) = trust.fingerprint_for(path) {
        match DocFile::open_with::<S>(path, peer, limits, Access::Write, Some(noted)) {
            Ok((f, mut s, r)) => {
                // The same copy as last time on this computer, unless the
                // list has no name for it (then a new one).
                s.set_copy_name(trust.copy_for(path, &noted).unwrap_or_else(random_u64));
                return Ok((f, s, r, How::Trusted));
            }
            Err(FileError::ChangedSinceCheck) => {}
            Err(e) => return Err(e.into()),
        }
    }
    let checked = verify(path, verifier, limits, vlimits)?;
    // Opened only if it is still the file the verifier read.
    let (f, mut s, r) = DocFile::open_with::<S>(path, peer, limits, Access::Write, Some(checked))?;
    // Not the file this computer wrote last: another copy, with a new name.
    s.set_copy_name(random_u64());
    Ok((f, s, r, How::Verified))
}

/// The verification alone: the file's structure is read without the
/// document library (limits that fail here are reported before starting the
/// verifier), then the verifier process reads all of it. Returns the
/// fingerprint of what the verifier read; the file is opened only if it
/// still has that fingerprint.
pub fn verify(
    path: &Path,
    verifier: &Path,
    limits: &Limits,
    vlimits: &VerifyLimits,
) -> Result<[u8; 32], CheckError> {
    fingerprint_file(path, limits)?;
    run_verifier(verifier, path, limits, vlimits)
}

fn run_verifier(
    exe: &Path,
    path: &Path,
    limits: &Limits,
    v: &VerifyLimits,
) -> Result<[u8; 32], CheckError> {
    let mut cmd = Command::new(exe);
    if v.test_stall {
        cmd.env("BIVAR_VERIFIER_STALL", "1");
    }
    if let Some(n) = v.test_noise {
        cmd.env("BIVAR_VERIFIER_NOISE", n.to_string());
    }
    let mut child = cmd
        .arg(path)
        .arg(v.memory.to_string())
        .arg(limits.doc_record.to_string())
        .arg(limits.doc_total.to_string())
        .arg(limits.records.to_string())
        .arg(limits.blob_index.to_string())
        .arg(limits.blob_index_total.to_string())
        .arg(limits.depth.to_string())
        .arg(limits.pieces.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| CheckError::File(FileError::Io(e)))?;
    // Both outputs are read while the verifier runs: a verifier that writes
    // more than a pipe holds would otherwise wait for a reader forever and
    // be refused as too slow (audit 2026-10-31, suspicion; tested).
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut p) = pipe {
                let mut bytes = Vec::new();
                let _ = p.read_to_end(&mut bytes);
                text = String::from_utf8_lossy(&bytes).into_owned();
            }
            text
        })
    };
    let out_reader = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let err_reader = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| CheckError::File(FileError::Io(e)))?
        {
            break status;
        }
        if started.elapsed() > v.time {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CheckError::Refused(Refusal::TooSlow));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let out = out_reader.join().unwrap_or_default();
    let err = err_reader.join().unwrap_or_default();
    let line = out.lines().last().unwrap_or("").trim().to_owned();
    if status.success()
        && let Some(hex) = line.strip_prefix("OK ")
    {
        return parse_hex(hex).ok_or_else(|| {
            CheckError::Refused(Refusal::Crashed("unreadable verifier answer".into()))
        });
    }
    if let Some(why) = line.strip_prefix("REFUSED ") {
        return Err(CheckError::Refused(Refusal::Invalid(why.to_owned())));
    }
    if line.starts_with("TOO_DEEP ") {
        return Err(CheckError::Refused(Refusal::TooDeep));
    }
    // A failed allocation under the cap ends the process with this text
    // from the Rust runtime. It is not a stable interface: a test locks it
    // (`a_decompression_bomb_is_refused_by_the_memory_cap`), so a change in
    // Rust shows up as a failing test, not as a wrong message (M3).
    let refusal = if err.contains("memory allocation") {
        Refusal::TooMuchMemory
    } else if err.contains("overflowed its stack") {
        // No function that walks the tree calls itself (I4), so this is
        // not expected; kept so a stack overflow is never "crashed".
        Refusal::TooDeep
    } else {
        Refusal::Crashed(format!("{status}; {}", err.lines().last().unwrap_or("")))
    };
    Err(CheckError::Refused(refusal))
}

fn parse_hex(s: &str) -> Option<[u8; 32]> {
    let bytes: Vec<u8> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect::<Option<_>>()?;
    bytes.try_into().ok()
}

/// Limits used while fuzzing: smaller, so each run stays fast.
pub fn fuzz_limits() -> Limits {
    Limits {
        doc_record: 16 << 20,
        doc_total: 64 << 20,
        records: 100_000,
        blob_index: 1 << 20,
        blob_index_total: 8 << 20,
        depth: 10_000,
        pieces: 100_000,
    }
}

/// Fuzzing target 1: any bytes, read as a whole `.bivar` file. Checksums
/// make almost every input stop at the container, which is what it tests.
pub fn fuzz_container(data: &[u8]) {
    // The same passes as the verifier (B1), not only the visible state.
    if let Ok((store, _)) = bivar_poc_arquivo::check_bytes::<EntriesStore>(data, &fuzz_limits()) {
        let _ = bivar_poc_modelo::every_pass(&store, bivar_poc_modelo::MAX_DEPTH);
    }
}

/// Fuzzing target 2: the bytes are cut into records (kind, 2-byte length,
/// payload) and given correct checksums, so arbitrary payloads reach the
/// document library through the snapshot and updates records.
pub fn fuzz_records(data: &[u8]) {
    let mut records: Vec<(u8, &[u8])> = Vec::new();
    let mut rest = data;
    while rest.len() >= 3 && records.len() < 64 {
        let kind = rest[0];
        let len = usize::from(u16::from_le_bytes([rest[1], rest[2]])).min(rest.len() - 3);
        records.push((kind, &rest[3..3 + len]));
        rest = &rest[3 + len..];
    }
    let bytes = bivar_poc_arquivo::build_file_bytes(&records);
    fuzz_container(&bytes);
}

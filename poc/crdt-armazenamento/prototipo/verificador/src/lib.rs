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
use bivar_poc_modelo::Store;

/// Limits of the verifier process (owner, 2026-10-30).
#[derive(Clone, Copy, Debug)]
pub struct VerifyLimits {
    pub time: Duration,
    pub memory: u64,
    /// Test only: the verifier never finishes (proves the time limit).
    #[doc(hidden)]
    pub test_stall: bool,
}

impl Default for VerifyLimits {
    fn default() -> Self {
        VerifyLimits {
            time: Duration::from_secs(60),
            memory: 4 << 30,
            test_stall: false,
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
        match DocFile::open_with(path, peer, limits, Access::Write, Some(noted)) {
            Ok((f, s, r)) => return Ok((f, s, r, How::Trusted)),
            Err(FileError::ChangedSinceCheck) => {}
            Err(e) => return Err(e.into()),
        }
    }
    // Reads only the file's structure (no document library): limits that
    // fail here are reported before starting the verifier.
    fingerprint_file(path, limits)?;
    let checked = run_verifier(verifier, path, limits, vlimits)?;
    // Opened only if it is still the file the verifier read.
    let (f, s, r) = DocFile::open_with(path, peer, limits, Access::Write, Some(checked))?;
    Ok((f, s, r, How::Verified))
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
    let mut child = cmd
        .arg(path)
        .arg(v.memory.to_string())
        .arg(limits.doc_record.to_string())
        .arg(limits.doc_total.to_string())
        .arg(limits.records.to_string())
        .arg(limits.blob_index.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| CheckError::File(FileError::Io(e)))?;
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
    let mut out = String::new();
    let mut err = String::new();
    if let Some(mut o) = child.stdout.take() {
        let _ = o.read_to_string(&mut out);
    }
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut err);
    }
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
    let refusal = if err.contains("memory allocation") {
        Refusal::TooMuchMemory
    } else if err.contains("overflowed its stack") {
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
    }
}

/// Fuzzing target 1: any bytes, read as a whole `.bivar` file. Checksums
/// make almost every input stop at the container, which is what it tests.
pub fn fuzz_container(data: &[u8]) {
    if let Ok((store, _)) = bivar_poc_arquivo::check_bytes::<EntriesStore>(data, &fuzz_limits()) {
        let _ = store.state();
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

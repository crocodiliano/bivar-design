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

use bivar_poc_arquivo::{Access, DocFile, FileError, Limits, OpenReport, TrustStore, file_summary};
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
    /// Measurements only: the verifier reports the most memory it held,
    /// as counted by its memory cap (`verify_measured`).
    #[doc(hidden)]
    pub test_peak: bool,
}

impl Default for VerifyLimits {
    fn default() -> Self {
        VerifyLimits {
            time: Duration::from_secs(60),
            memory: 4 << 30,
            test_stall: false,
            test_noise: None,
            test_peak: false,
        }
    }
}

/// Why a file was refused, in words the program can show.
#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    TooSlow,
    /// Above the verifier's memory limit (bytes), which the message names.
    TooMuchMemory {
        limit: u64,
    },
    /// One memory request above the limit and more than
    /// `ABSURD_REQUEST_FACTOR` times the document data: no real document
    /// asks for that, and a higher limit would probably not help
    /// (ADR-0024, section 2).
    AbsurdRequest {
        /// The size of the request that failed (bytes).
        asked: u64,
        /// The document data of the file (bytes).
        doc_bytes: u64,
        limit: u64,
    },
    /// A reader panicked on the file's contents. `detail` is where and why,
    /// for a log; the message never shows it.
    Damaged {
        detail: String,
    },
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
            // ADR-0024, section 2: the limit passed, that it can be raised,
            // and the warning that comes with raising it.
            Refusal::TooMuchMemory { limit } => format!(
                "This file was refused: reading it needs more memory than the limit of {} for \
                 files from outside. A very large document may need more, and the limit can be \
                 raised in the settings, with a warning: a file that asks for this much memory \
                 may also be an attack.",
                bytes_text(*limit)
            ),
            Refusal::AbsurdRequest {
                asked,
                doc_bytes,
                limit,
            } => format!(
                "This file was refused: it holds {} of document data, but reading it asks for \
                 {} of memory in a single request. A real document does not do that: the file \
                 is damaged, or it was made to attack. Raising the memory limit ({}) will \
                 probably not help.",
                approx_text(*doc_bytes),
                approx_text(*asked),
                bytes_text(*limit)
            ),
            // Nothing about memory or limits: the cause is another one.
            Refusal::Damaged { .. } => "This file was refused: it is damaged, or it was made to \
                attack. Its contents could not be read."
                .into(),
            Refusal::TooDeep => {
                "This file was refused: its structure is nested deeper than can be read.".into()
            }
            Refusal::Invalid(why) => format!("This file was refused: {why}"),
            Refusal::Crashed(why) => format!("This file could not be read safely: {why}"),
        }
    }
}

/// A memory size in the unit a person reads: "4 GiB", "256 MiB".
fn bytes_text(bytes: u64) -> String {
    if bytes >= 1 << 30 && bytes.is_multiple_of(1 << 30) {
        format!("{} GiB", bytes >> 30)
    } else if bytes >= 1 << 20 {
        format!("{} MiB", bytes >> 20)
    } else {
        format!("{bytes} bytes")
    }
}

/// A size a person reads at a glance, rounded to one decimal: "1.6 TiB",
/// "12.0 GiB", "924 bytes".
fn approx_text(bytes: u64) -> String {
    const UNITS: [(&str, u128); 4] = [
        ("TiB", 1 << 40),
        ("GiB", 1 << 30),
        ("MiB", 1 << 20),
        ("KiB", 1 << 10),
    ];
    for (name, size) in UNITS {
        if u128::from(bytes) >= size {
            let tenths = (u128::from(bytes) * 10 + size / 2) / size;
            return format!("{}.{} {name}", tenths / 10, tenths % 10);
        }
    }
    format!("{bytes} bytes")
}

/// How many times the document data one memory request must pass, besides
/// passing the limit, to be called absurd (owner, 2026-11-10). Measured
/// (audit report 2026-11-04, sections 11.3 and 12.3): legitimate documents
/// ask for 1.0 times their data, the decompression bombs for at most 170.9
/// times, and the inputs that make the document library reserve by a count
/// read from the file for millions of times.
pub const ABSURD_REQUEST_FACTOR: u64 = 1000;

/// Whether one failed memory request is absurd for the file: above the
/// memory limit and more than `ABSURD_REQUEST_FACTOR` times the file's
/// document data.
pub fn is_absurd_request(asked: u64, limit: u64, doc_bytes: u64) -> bool {
    asked > limit && u128::from(asked) > u128::from(ABSURD_REQUEST_FACTOR) * u128::from(doc_bytes)
}

/// The size of the request in the text the Rust runtime writes when an
/// allocation fails ("memory allocation of N bytes failed"), if it is there.
fn failed_request(err: &str) -> Option<u64> {
    let rest = err.split("memory allocation of ").nth(1)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
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

/// The version of what the verifier checks. Raised whenever the verifier
/// checks more or differently (a new pass, a new limit): the files from
/// outside it passed before are checked again on their next opening
/// (owner, 2026-10-31). Files this computer wrote are not affected.
pub const VERIFIER_VERSION: &str = "0.6-2026-10-31";

pub fn open_checked<S: Store>(
    path: &Path,
    peer: u64,
    trust: &TrustStore,
    verifier: &Path,
    limits: &Limits,
    vlimits: &VerifyLimits,
) -> Result<(DocFile, S, OpenReport, How), CheckError> {
    // A noted file (this computer wrote it, or this version of the verifier
    // passed it): opened once, and accepted only if it is still exactly what
    // was noted (checked before the document library sees it). Otherwise,
    // or if another verifier version passed it, it goes through the
    // verifier like any other.
    let passed_by_another_verifier = trust
        .verified_by(path)
        .is_some_and(|v| v != VERIFIER_VERSION);
    if let Some(noted) = trust.fingerprint_for(path)
        && !passed_by_another_verifier
    {
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
    // Passed: noted, so it is verified once while it stays as it is and the
    // verifier is this version (owner, 2026-10-31).
    trust
        .remember_verified(path, &checked, s.copy_name(), VERIFIER_VERSION)
        .map_err(|e| CheckError::File(FileError::Io(e)))?;
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
    verify_measured(path, verifier, limits, vlimits).map(|(fingerprint, _)| fingerprint)
}

/// `verify`, also returning the most memory the verifier held (bytes, as
/// its memory cap counts it) when `test_peak` is on. Sampled every few
/// milliseconds, so a short peak may be missed; above the limit the cap
/// itself refuses the file.
pub fn verify_measured(
    path: &Path,
    verifier: &Path,
    limits: &Limits,
    vlimits: &VerifyLimits,
) -> Result<([u8; 32], Option<u64>), CheckError> {
    // The document data is measured here, by the program, not by the
    // verifier: the size the absurd-request rule compares with.
    bivar_poc_arquivo::fingerprint_file(path, limits)?;
    let (_, doc_bytes) = file_summary(path, limits)?;
    run_verifier(verifier, path, limits, vlimits, doc_bytes)
}

fn run_verifier(
    exe: &Path,
    path: &Path,
    limits: &Limits,
    v: &VerifyLimits,
    doc_bytes: u64,
) -> Result<([u8; 32], Option<u64>), CheckError> {
    let mut cmd = Command::new(exe);
    if v.test_stall {
        cmd.env("BIVAR_VERIFIER_STALL", "1");
    }
    if let Some(n) = v.test_noise {
        cmd.env("BIVAR_VERIFIER_NOISE", n.to_string());
    }
    if v.test_peak {
        cmd.env("BIVAR_VERIFIER_PEAK", "1");
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
        let peak = err
            .lines()
            .find_map(|l| l.strip_prefix("PEAK_BYTES "))
            .and_then(|n| n.trim().parse().ok());
        return parse_hex(hex).map(|fp| (fp, peak)).ok_or_else(|| {
            CheckError::Refused(Refusal::Crashed("unreadable verifier answer".into()))
        });
    }
    if let Some(why) = line.strip_prefix("REFUSED ") {
        return Err(CheckError::Refused(Refusal::Invalid(why.to_owned())));
    }
    if line.starts_with("TOO_DEEP ") {
        return Err(CheckError::Refused(Refusal::TooDeep));
    }
    // A reader panicked: the verifier's own line, written by its panic
    // hook, not the text of a crash. Checked before the memory text, so a
    // panic is never reported as a memory refusal.
    if let Some(detail) = line.strip_prefix("DAMAGED") {
        return Err(CheckError::Refused(Refusal::Damaged {
            detail: detail.trim().to_owned(),
        }));
    }
    // A failed allocation under the cap ends the process with this text
    // from the Rust runtime. It is not a stable interface: a test locks it
    // (`a_decompression_bomb_is_refused_by_the_memory_cap`), so a change in
    // Rust shows up as a failing test, not as a wrong message (M3).
    let refusal = if err.contains("memory allocation") {
        match failed_request(&err) {
            Some(asked) if is_absurd_request(asked, v.memory, doc_bytes) => {
                Refusal::AbsurdRequest {
                    asked,
                    doc_bytes,
                    limit: v.memory,
                }
            }
            _ => Refusal::TooMuchMemory { limit: v.memory },
        }
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
    fuzz_container(&fuzz_records_file(data));
}

/// The `.bivar` file that `fuzz_records` builds from its input: the same
/// bytes, so a test can open an input of that target as a file.
#[doc(hidden)]
pub fn fuzz_records_file(data: &[u8]) -> Vec<u8> {
    let mut records: Vec<(u8, &[u8])> = Vec::new();
    let mut rest = data;
    while rest.len() >= 3 && records.len() < 64 {
        let kind = rest[0];
        let len = usize::from(u16::from_le_bytes([rest[1], rest[2]])).min(rest.len() - 3);
        records.push((kind, &rest[3..3 + len]));
        rest = &rest[3 + len..];
    }
    bivar_poc_arquivo::build_file_bytes(&records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_absurd_only_above_the_limit_and_above_a_thousand_times_the_data() {
        let limit = 4 << 30;
        // The measured cases (audit report 2026-11-04, 11.3 and 12.3).
        assert!(is_absurd_request(1_747_498_019_256, limit, 1_060));
        assert!(is_absurd_request(12_884_901_884, limit, 924));
        // A legitimate document refused by a low limit: 1.0 times its data.
        assert!(!is_absurd_request(68_684_139, 64 << 20, 68_684_139));
        // A decompression bomb: 170.9 times its data, at the limit.
        assert!(!is_absurd_request(268_435_456, 256 << 20, 1_570_000));
        // Above a thousand times the data but not above the limit.
        assert!(!is_absurd_request(limit, limit, 1));
        assert!(is_absurd_request(limit + 1, limit, 1));
        // Above the limit but not above a thousand times the data.
        assert!(!is_absurd_request(5_000_000_000, limit, 5_000_000));
        assert!(is_absurd_request(5_000_000_001, limit, 5_000_000));
        // No overflow at the ends.
        assert!(!is_absurd_request(u64::MAX, limit, u64::MAX));
        assert!(is_absurd_request(u64::MAX, limit, 0));
    }

    #[test]
    fn the_failed_request_is_read_from_the_runtime_text() {
        let err = "noise\nmemory allocation of 1747498019256 bytes failed\n";
        assert_eq!(failed_request(err), Some(1_747_498_019_256));
        assert_eq!(failed_request("memory allocation failed"), None);
        assert_eq!(
            failed_request("memory allocation of many bytes failed"),
            None
        );
        assert_eq!(failed_request(""), None);
    }

    #[test]
    fn sizes_are_written_for_a_person() {
        assert_eq!(approx_text(1_747_498_019_256), "1.6 TiB");
        assert_eq!(approx_text(12_884_901_884), "12.0 GiB");
        assert_eq!(approx_text(1_060), "1.0 KiB");
        assert_eq!(approx_text(924), "924 bytes");
        assert_eq!(approx_text(u64::MAX), "16777216.0 TiB");
    }

    #[test]
    fn the_new_refusals_say_what_they_must_and_no_more() {
        let absurd = Refusal::AbsurdRequest {
            asked: 1_747_498_019_256,
            doc_bytes: 1_060,
            limit: 4 << 30,
        }
        .message();
        for part in [
            "1.0 KiB",
            "1.6 TiB",
            "4 GiB",
            "damaged",
            "attack",
            "probably not help",
        ] {
            assert!(absurd.contains(part), "missing {part:?}: {absurd}");
        }
        assert!(!absurd.contains("can be raised"), "{absurd}");
        let damaged = Refusal::Damaged {
            detail: "panicked at block.rs:305:14: RUST_BACKTRACE".into(),
        }
        .message();
        for part in ["damaged", "attack"] {
            assert!(damaged.contains(part), "missing {part:?}: {damaged}");
        }
        for part in ["memory", "limit", "block.rs", "RUST_BACKTRACE", "panicked"] {
            assert!(!damaged.contains(part), "must not say {part:?}: {damaged}");
        }
    }
}

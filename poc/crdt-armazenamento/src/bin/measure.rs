//! Measurements for the phase 0.6 comparison, on the reference machine.
//!
//! `measure` runs every task for both libraries, each in its own child
//! process so peak memory belongs to that task alone, and prints a
//! Markdown summary. `measure child <task> <lib> [file]` runs one task.
//!
//! Limits approved by the owner on 2026-10-29, before measuring:
//! open 1M operations <= 3 s and <= 1 GB extra memory; 100,000 mixed
//! operations <= 5 s; merging 10,000 + 10,000 <= 1 s.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use bivar_poc_crdt::automerge_doc::AutomergeReplica;
use bivar_poc_crdt::loro_doc::LoroReplica;
use bivar_poc_crdt::model::{Replica, Value};
use bivar_poc_crdt::workload::Workload;

const RUNS: usize = 3;

/// Peak memory of this process so far, in bytes. Read from the operating
/// system, so no dependency and no `unsafe`.
fn peak_memory() -> u64 {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        status
            .lines()
            .find_map(|l| l.strip_prefix("VmHWM:"))
            .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
            .map_or(0, |kb| kb * 1024)
    }
    #[cfg(windows)]
    {
        let out = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("(Get-Process -Id {}).PeakWorkingSet64", std::process::id()),
            ])
            .output()
            .expect("powershell runs");
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or(0)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        0
    }
}

fn result(key: &str, value: impl std::fmt::Display) {
    println!("RESULT {key}={value}");
}

fn child_task<R: Replica>(task: &str, file: Option<&Path>) {
    let base_mem = peak_memory();
    match task {
        "ops100k" => {
            let mut doc = R::new_document(1);
            let mut w = Workload::new(42);
            let t = Instant::now();
            w.run(&mut doc, 100_000);
            result("seconds", t.elapsed().as_secs_f64());
            let t = Instant::now();
            let bytes = doc.save();
            result("save_seconds", t.elapsed().as_secs_f64());
            result("bytes", bytes.len());
            let mut counts: Vec<_> = w.counts.iter().collect();
            counts.sort();
            result("mix", format!("{counts:?}").replace(' ', ""));
        }
        "merge" => {
            let mut a = R::new_document(1);
            let mut w = Workload::new(7);
            w.run(&mut a, 2_000);
            let mut b = a.fork_as(2);
            let mut wa = Workload::with_mirror(71, w.mirror.clone());
            let mut wb = Workload::with_mirror(72, w.mirror.clone());
            wa.run(&mut a, 10_000);
            wb.run(&mut b, 10_000);
            let t = Instant::now();
            let ra = a.merge_from(&mut b).expect("merge a");
            let rb = b.merge_from(&mut a).expect("merge b");
            result("seconds", t.elapsed().as_secs_f64());
            result("identical", a.state() == b.state());
            result(
                "warnings",
                format!(
                    "{}w/{}m/{}h",
                    ra.concurrent_writes.len(),
                    ra.lost_moves.len(),
                    ra.hidden_by_removal.len()
                ),
            );
            let _ = rb;
            result("bytes", a.save().len());
        }
        "build1m" => {
            let path = file.expect("output file");
            let mut doc = R::new_document(1);
            let mut w = Workload::new(1_000_000);
            let t = Instant::now();
            w.run(&mut doc, 1_000_000);
            result("seconds", t.elapsed().as_secs_f64());
            let bytes = doc.save();
            result("bytes", bytes.len());
            result("history_ops", doc.history_ops());
            std::fs::write(path, &bytes).expect("write file");
        }
        "load" => {
            let path = file.expect("input file");
            let t = Instant::now();
            let bytes = std::fs::read(path).expect("read file");
            result("read_seconds", t.elapsed().as_secs_f64());
            let mem_before = peak_memory().max(base_mem);
            let t = Instant::now();
            let mut doc = R::load(&bytes, 9).expect("load");
            result("seconds", t.elapsed().as_secs_f64());
            result("history_ops", doc.history_ops());
            // First real use after opening: read the whole visible document.
            let t = Instant::now();
            let nodes = doc.state().nodes.len();
            result("first_read_seconds", t.elapsed().as_secs_f64());
            result("visible_nodes", nodes);
            result("extra_memory", peak_memory().saturating_sub(mem_before));
            return;
        }
        "load_raw" => {
            // Library load alone, without this proof's own index (Automerge)
            // or first read (Loro), to split the cost fairly.
            let bytes = std::fs::read(file.expect("input file")).expect("read file");
            let mem_before = peak_memory().max(base_mem);
            let t = Instant::now();
            if R::NAME == "Automerge" {
                let doc = automerge::AutoCommit::load(&bytes).expect("load");
                result("seconds", t.elapsed().as_secs_f64());
                drop(doc);
            } else {
                let doc = loro::LoroDoc::from_snapshot(&bytes).expect("load");
                result("seconds", t.elapsed().as_secs_f64());
                drop(doc);
            }
            result("extra_memory", peak_memory().saturating_sub(mem_before));
            return;
        }
        "undo" => {
            // A sets a value; B, having seen it, writes over it; A undoes.
            let mut a = R::new_document(1);
            let x = a.create_node(None, "shape").expect("create");
            a.set_param(&x, "color", Value::Str("green".into()))
                .expect("set");
            a.commit();
            let mut b = a.fork_as(2);
            a.set_param(&x, "color", Value::Str("red".into()))
                .expect("set");
            a.commit();
            b.merge_from(&mut a).expect("merge");
            b.set_param(&x, "color", Value::Str("blue".into()))
                .expect("set");
            b.commit();
            a.merge_from(&mut b).expect("merge");
            let out = a.undo();
            result(
                "undo_over_other",
                format!("{:?} -> {:?}", out, a.param(&x, "color")).replace(' ', "_"),
            );
        }
        other => panic!("unknown task {other}"),
    }
    result("extra_memory", peak_memory().saturating_sub(base_mem));
}

fn run_child(task: &str, lib: &str, file: Option<&Path>) -> Vec<(String, String)> {
    let exe = std::env::current_exe().expect("own path");
    let mut cmd = Command::new(exe);
    cmd.args(["child", task, lib]);
    if let Some(f) = file {
        cmd.arg(f);
    }
    let out = cmd.output().expect("child runs");
    if !out.status.success() {
        eprintln!(
            "{task} {lib} failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        return Vec::new();
    }
    let mut res = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if let Some(kv) = line.strip_prefix("RESULT ")
            && let Some((k, v)) = kv.split_once('=')
        {
            // The last value of a key wins (load reports extra_memory once).
            res.retain(|(key, _): &(String, String)| key != k);
            res.push((k.to_owned(), v.to_owned()));
        }
    }
    res
}

fn get<'a>(r: &'a [(String, String)], k: &str) -> &'a str {
    r.iter()
        .find(|(key, _)| key == k)
        .map_or("-", |(_, v)| v.as_str())
}

fn num(r: &[(String, String)], k: &str) -> f64 {
    get(r, k).parse().unwrap_or(f64::NAN)
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn mb(bytes: f64) -> String {
    format!("{:.1} MB", bytes / 1_048_576.0)
}

fn verdict(ok: bool) -> &'static str {
    if ok { "passa" } else { "**não passa**" }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("child") {
        let (task, lib) = (&args[2], &args[3]);
        let file = args.get(4).map(PathBuf::from);
        match lib.as_str() {
            "loro" => child_task::<LoroReplica>(task, file.as_deref()),
            "automerge" => child_task::<AutomergeReplica>(task, file.as_deref()),
            other => panic!("unknown library {other}"),
        }
        return;
    }
    let dir = PathBuf::from(args.get(1).cloned().unwrap_or_else(|| "resultados".into()));
    std::fs::create_dir_all(&dir).expect("results folder");
    println!("# Medições da entrega 0.6\n");
    println!(
        "Mediana de {RUNS} execuções, cada uma num processo separado. Compilação `--release`.\n"
    );
    for lib in ["loro", "automerge"] {
        println!("## {lib}\n");
        let runs: Vec<_> = (0..RUNS).map(|_| run_child("ops100k", lib, None)).collect();
        let secs = median(runs.iter().map(|r| num(r, "seconds")).collect());
        let mem = median(runs.iter().map(|r| num(r, "extra_memory")).collect());
        println!(
            "- **100.000 operações misturadas:** {secs:.2} s (limite 5 s: {}), memória a mais {}, salvar {:.2} s, arquivo {}.",
            verdict(secs <= 5.0),
            mb(mem),
            num(&runs[0], "save_seconds"),
            mb(num(&runs[0], "bytes"))
        );
        println!("  - mistura feita: `{}`", get(&runs[0], "mix"));

        let runs: Vec<_> = (0..RUNS).map(|_| run_child("merge", lib, None)).collect();
        let secs = median(runs.iter().map(|r| num(r, "seconds")).collect());
        let mem = median(runs.iter().map(|r| num(r, "extra_memory")).collect());
        println!(
            "- **Junção 10.000 + 10.000 (nos dois sentidos):** {secs:.3} s (limite 1 s: {}), cópias idênticas: {}, avisos na cópia A (escritas/movimentos/escondidos): {}, memória a mais na tarefa {}, arquivo {}.",
            verdict(secs <= 1.0),
            get(&runs[0], "identical"),
            get(&runs[0], "warnings"),
            mb(mem),
            mb(num(&runs[0], "bytes"))
        );

        let file = dir.join(format!("doc-1m-{lib}.bin"));
        let build = run_child("build1m", lib, Some(&file));
        println!(
            "- **Documento com 1 milhão de operações:** gerado em {:.1} s; {} operações no histórico; arquivo {}.",
            num(&build, "seconds"),
            get(&build, "history_ops"),
            mb(num(&build, "bytes"))
        );
        let runs: Vec<_> = (0..RUNS)
            .map(|_| run_child("load", lib, Some(&file)))
            .collect();
        let secs = median(runs.iter().map(|r| num(r, "seconds")).collect());
        let mem = median(runs.iter().map(|r| num(r, "extra_memory")).collect());
        let first = median(runs.iter().map(|r| num(r, "first_read_seconds")).collect());
        println!(
            "- **Abrir esse documento:** {secs:.2} s (limite 3 s: {}), memória a mais {} (limite 1 GB: {}); primeira leitura completa depois de abrir {first:.2} s; leitura do arquivo no disco {:.2} s; {} nós visíveis.",
            verdict(secs <= 3.0),
            mb(mem),
            verdict(mem <= 1024.0 * 1_048_576.0),
            num(&runs[0], "read_seconds"),
            get(&runs[0], "visible_nodes")
        );

        let undo = run_child("undo", lib, None);
        println!(
            "- **Desfazer algo que outra cópia já editou por cima:** `{}`\n",
            get(&undo, "undo_over_other")
        );
    }
}

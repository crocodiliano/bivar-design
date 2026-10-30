# Fuzzing

Cada leitor de entrada externa ganha aqui um alvo de fuzzing ([ADR-0011](../docs/adr/0011.md), [ADR-0024](../docs/adr/0024.md)): um programa que gera milhões de entradas defeituosas para tentar derrubar o leitor.

- **Ferramenta:** `cargo-fuzz` com libFuzzer, na versão *nightly* do Rust travada em `rust-toolchain.toml`, **só nesta pasta**. O programa continua na versão estável.
- **Onde roda:** na compilação automática do GitHub (Linux), em `.github/workflows/fuzz.yml`: 2 minutos por alvo a cada envio e 30 minutos por alvo uma vez por dia.
- **Casos encontrados:** um arquivo que derrubou um leitor vai para `regressions/<alvo>/`. Esses arquivos e a coleção em `corpus/` também rodam no `cargo test` normal, sem *nightly* (`poc/crdt-armazenamento/prototipo/verificador/tests/fuzz_replay.rs`).
- **Licença:** o libFuzzer é C++ com licença NCSA, liberada só no `deny.toml` desta pasta. Ele nunca entra no programa.

**Alvos:**

| Alvo | O que testa |
| --- | --- |
| `bivar_container` | quaisquer bytes, lidos como um arquivo `.bivar` inteiro |
| `bivar_records` | registros com selos corretos e conteúdo qualquer, para chegar à Loro por trás da caixa |

---

Every external-input reader gets a fuzzing target here (ADR-0011, ADR-0024). The tool is `cargo-fuzz` with libFuzzer, on a nightly toolchain pinned in `rust-toolchain.toml` and used only in this folder. It runs in the automatic build (`.github/workflows/fuzz.yml`); `corpus/` and `regressions/` are also replayed by the normal `cargo test` on stable. libFuzzer (C++, NCSA license) is allowed only in this folder's `deny.toml` and never ships in the program.

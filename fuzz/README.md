# Fuzzing

Cada leitor de entrada externa ganha aqui um alvo de fuzzing ([ADR-0011](../docs/adr/0011.md), [ADR-0024](../docs/adr/0024.md)): um programa que gera milhões de entradas defeituosas para tentar derrubar o leitor.

- **Ferramenta:** `cargo-fuzz` com libFuzzer, na versão *nightly* do Rust travada em `rust-toolchain.toml`, **só nesta pasta**. O programa continua na versão estável.
- **Onde roda:** na compilação automática do GitHub (Linux), em `.github/workflows/fuzz.yml`: cerca de 2 minutos por alvo a cada envio e cerca de 30 minutos por alvo uma vez por dia.
- **Modo `-fork`** (`run-fork.sh`): a falta de memória é ignorada e contada no registro, porque o verificador recusa esses arquivos e o programa sobrevive. Um pânico, outra queda ou um tempo esgotado deixa o job vermelho. Depois da rodada, `fork-families.py` agrupa as entradas pelo lugar onde falharam e guarda **uma por família** no artefato `fuzz-families-<alvo>`.
- **Gancho de falta de memória** (`fuzz_targets/oom_hook.rs`, recurso *nightly*): um pedido de memória que nem o AddressSanitizer atende (acima de 1 TiB) conta como falta de memória, e não como queda.
- **Casos encontrados:** um arquivo que derrubou um leitor vai para `regressions/<alvo>/`. Esses arquivos e a coleção em `corpus/` também rodam no `cargo test` normal, sem *nightly* (`poc/crdt-armazenamento/prototipo/verificador/tests/fuzz_replay.rs`).
- **Exceção, `verifier-refused/<alvo>/`:** um arquivo que ainda derruba o processo da Loro não pode ir para `regressions/`, porque derrubaria o binário de testes inteiro. Ele fica aqui e é testado **pelo verificador**: aberto como arquivo de fora, ele tem de ser recusado com a recusa que `verifier-refused/<alvo>.txt` indica (pedido absurdo ou danificado), e o programa tem de continuar vivo (`verificador/tests/entrada.rs`). Ele passa para `regressions/` quando uma versão nova da Loro, aprovada pelo dono, deixar de cair com ele, conferido num processo separado.
- **Licença:** o libFuzzer é C++ com licença NCSA, liberada só no `deny.toml` desta pasta. Ele nunca entra no programa.

**Alvos:**

| Alvo | O que testa |
| --- | --- |
| `bivar_container` | quaisquer bytes, lidos como um arquivo `.bivar` inteiro |
| `bivar_records` | registros com selos corretos e conteúdo qualquer, para chegar à Loro por trás da caixa |

---

Every external-input reader gets a fuzzing target here (ADR-0011, ADR-0024). The tool is `cargo-fuzz` with libFuzzer, on a nightly toolchain pinned in `rust-toolchain.toml` and used only in this folder. It runs in the automatic build (`.github/workflows/fuzz.yml`) in libFuzzer's fork mode (`run-fork.sh`): out-of-memory is ignored and counted, while a panic, another crash or a timeout fails the job, and one input per family is kept (`fork-families.py`). `corpus/` and `regressions/` are also replayed by the normal `cargo test` on stable. Inputs that still end the document library's process live in `verifier-refused/` instead, where the verifier must refuse them as the list next to them says. libFuzzer (C++, NCSA license) is allowed only in this folder's `deny.toml` and never ships in the program.

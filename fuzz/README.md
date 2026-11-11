# Fuzzing

Cada leitor de entrada externa ganha aqui um alvo de fuzzing ([ADR-0011](../docs/adr/0011.md), [ADR-0024](../docs/adr/0024.md)): um programa que gera milhões de entradas defeituosas para tentar derrubar o leitor.

- **Ferramenta:** `cargo-fuzz` com libFuzzer, na versão *nightly* do Rust travada em `rust-toolchain.toml`, **só nesta pasta**. O programa continua na versão estável.
- **Onde roda:** na compilação automática do GitHub (Linux), em `.github/workflows/fuzz.yml`: cerca de 2 minutos por alvo a cada envio e cerca de 30 minutos por alvo uma vez por dia.
- **Conferência das sementes** (`run-fork.sh`, decisão do dono de 2026-11-11): antes do fuzzing, cada entrada da coleção inicial (`found/` e `corpus/`) roda uma vez, sozinha, num processo próprio, com os mesmos limites de memória e de tempo do `-fork`. Motivo: o modo `-fork` pula em silêncio uma entrada inicial que derruba o processo (`FuzzerFork.cpp`, linhas 342-351), e ela sumiria sem aviso. No máximo 120 s; as que sobram são contadas e dão aviso, com o job verde.
- **Modo `-fork`** (`run-fork.sh`): a falta de memória e as quedas, inclusive os pânicos, são ignoradas durante a rodada; um tempo esgotado a para. Depois da rodada, `fork-families.py` executa de novo cada entrada guardada e a agrupa pelo lugar onde falhou. **Quem decide o veredito é o agrupamento** (decisão do dono de 2026-11-11, auditoria de 2026-11-10, I1):
  - família da lista `known-families.txt` (aprovada pelo dono, com a versão da biblioteca na chave): **aviso com a contagem, job verde**;
  - família nova, entrada que não falha de novo ou que não foi executada de novo: **vermelho**;
  - falha cuja pilha (ou o lugar do pânico) passa por uma função de `before-verifier.txt`, o código que o programa roda antes do verificador: **sempre vermelho**, mesmo de família da lista. A regra confere o arquivo e a linha de cada quadro, e não só o nome, porque um quadro juntado pelo compilador pode vir sem o nome completo;
  - o agrupamento que falha, trava, não roda ou não termina com a linha `verdict: green`: **vermelho**.
- **Artefato `fuzz-families-<alvo>`:** uma entrada por família, com o texto da execução, e a reserva (`kept/<alvo>/reserve/`): todas as entradas de famílias novas, de fora do verificador, não classificadas ou não executadas de novo, e no máximo 5 faltas de memória por família conhecida (as outras são contadas). Se o agrupamento não terminar, sobe também `artifacts/<alvo>/` inteiro.
- **Gancho de falta de memória** (`fuzz_targets/oom_hook.rs`, recurso *nightly*): um pedido de memória que nem o AddressSanitizer atende (acima de 1 TiB) conta como falta de memória, e não como queda.
- **Casos encontrados:** um arquivo que derrubou um leitor vai para `regressions/<alvo>/`. Esses arquivos e a coleção em `corpus/` também rodam no `cargo test` normal, sem *nightly* (`poc/crdt-armazenamento/prototipo/verificador/tests/fuzz_replay.rs`).
- **Exceção, `verifier-refused/<alvo>/`:** um arquivo que ainda derruba o processo da Loro não pode ir para `regressions/`, porque derrubaria o binário de testes inteiro. Ele fica aqui e é testado **pelo verificador**: aberto como arquivo de fora, ele tem de ser recusado com a recusa que `verifier-refused/<alvo>.txt` indica (pedido absurdo ou danificado), e o programa tem de continuar vivo (`verificador/tests/entrada.rs`). Ele passa para `regressions/` quando uma versão nova da Loro, aprovada pelo dono, deixar de cair com ele, conferido num processo separado.
- **Licença:** o libFuzzer é C++. O pacote `libfuzzer-sys` declara NCSA, e o código do libFuzzer embutido nele declara Apache-2.0 com a exceção do LLVM. As duas ficam liberadas só no `deny.toml` desta pasta, e só para o `libfuzzer-sys`. Ele nunca entra no programa.

**Alvos:**

| Alvo | O que testa |
| --- | --- |
| `bivar_container` | quaisquer bytes, lidos como um arquivo `.bivar` inteiro |
| `bivar_records` | registros com selos corretos e conteúdo qualquer, para chegar à Loro por trás da caixa |

---

Every external-input reader gets a fuzzing target here (ADR-0011, ADR-0024). The tool is `cargo-fuzz` with libFuzzer, on a nightly toolchain pinned in `rust-toolchain.toml` and used only in this folder. It runs in the automatic build (`.github/workflows/fuzz.yml`) in libFuzzer's fork mode (`run-fork.sh`). First, every starting input runs alone (fork mode skips a failing one silently, `FuzzerFork.cpp` 342-351). During the run, out-of-memory and crashes are ignored and a timeout stops it. Then `fork-families.py` runs every kept input again and groups it by the place it failed; the grouping decides: a family listed in `known-families.txt` gives a warning and a green job; a new family, an input that cannot be classified, or any failure in the code the program runs before the verifier (`before-verifier.txt`, matched by file and line) fails the job, as does a grouping that does not end with its verdict line. The artifact keeps one input per family and a reserve (`kept/<target>/reserve/`). `corpus/` and `regressions/` are also replayed by the normal `cargo test` on stable. Inputs that still end the document library's process live in `verifier-refused/` instead, where the verifier must refuse them as the list next to them says. libFuzzer is C++: the `libfuzzer-sys` package declares NCSA, and the libFuzzer code bundled in it declares Apache-2.0 with the LLVM exception. Both are allowed only in this folder's `deny.toml`, and only for `libfuzzer-sys`; libFuzzer never ships in the program.

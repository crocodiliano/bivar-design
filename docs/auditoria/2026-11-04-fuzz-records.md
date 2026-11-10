# Falha do Fuzz no alvo `bivar_records` (diagnóstico, 2026-11-04)

> **Resultado:** a falha é um **defeito de leitura que já existe no `main`**. Ele fica na biblioteca de documento (a Loro, por meio da `serde_columnar`), que o nosso leitor chama. **Não foi criado pelo ramo `diag/trava-linux`** e **não é limite do ambiente**.
> - Um arquivo de 1 KB faz a Loro pedir **1.747.498.019.256 bytes (cerca de 1,6 TiB)** de memória de uma vez.
> - Rodando o leitor direto, o processo inteiro cai. Não é um pânico que se possa capturar.
> - **No programa, o verificador contém a falha:** o arquivo é recusado e o programa continua vivo. Mas a mensagem mostrada à pessoa engana (seção 6).
> - **Nada foi corrigido,** nada foi acrescentado a `fuzz/regressions` e não houve commit.

## 1. Dados analisados

| Item | Valor |
|---|---|
| Execução | `41876124671` (workflow Fuzz), job `fuzz (bivar_records)`, passo "Fuzz bivar_records" |
| Commit | `1e97bc7` (ramo `diag/trava-linux`) |
| Artefato | `fuzz-artifacts-bivar_records.zip` (569 bytes), baixado pelo dono para `A:\Projetos\bivar-diag\fuzz\` |
| Entrada | `bivar_records/crash-c6869029ad0329fc11298f77b7f2ea0fe497bf61`, 1.091 bytes. O SHA-1 do arquivo é `c6869029…bf61`, igual ao nome, então o arquivo chegou inteiro. |
| Registro | `A:\Projetos\bivar-diag\fuzz\log.txt`, 1.369 linhas |
| Como o libFuzzer achou | semente `3265824354`; mutação `ChangeBinInt` de uma entrada da coleção guardada (`base unit 5e5e2ef8…`), depois de cerca de 363 mil execuções |

A pasta de dados do dono foi só lida. O zip foi extraído numa pasta temporária fora do repositório.

## 2. Tipo da falha

- **Pelo nome do arquivo:** `crash-`. O libFuzzer usa `crash-` para um processo que caiu, `timeout-` para tempo-limite e `oom-` para falta de memória. **Não é tempo-limite.**
- **Pelo registro** (linhas 1274 a 1305):
  - `AddressSanitizer: requested allocation size 0x196deffa9b8 … exceeds maximum supported size of 0x10000000000`;
  - `SUMMARY: AddressSanitizer: allocation-size-too-big`.
- **Leitura:** é um **pedido de memória absurdo**, de 0x196deffa9b8 = 1.747.498.019.256 bytes, cerca de 1,6 TiB.
  - O AddressSanitizer, a ferramenta que vigia a memória durante o fuzzing, recusa qualquer pedido acima de 1 TiB e derruba o processo.
  - Por isso o libFuzzer registrou "crash", e não "oom". Na prática, é falta de memória causada pela entrada.
- **Fora do fuzzing** (reproduzido, seção 4), o mesmo pedido termina com `memory allocation of 1747498019256 bytes failed` e o processo é encerrado pelo próprio Rust (código `0xC0000409` no Windows).
  - **Não é um pânico:** não pode ser capturado com `catch_unwind`, e qualquer processo que execute essa leitura cai inteiro.

## 3. Caminho do erro

O registro da CI só tem endereços, sem nomes de função. A pilha abaixo veio da reprodução local, numa compilação com informação de depuração (do `main`):

```
alloc::vec::Vec<usize>::with_capacity
serde_columnar::strategy::rle::AnyRleDecoder<usize>::take_n_finalize   serde_columnar-0.3.14/src/strategy/rle.rs:233
loro_internal::…::block_meta_encode::decode_changes_header           loro-internal-1.17.1/src/oplog/change_store/block_meta_encode.rs:162
loro_internal::…::block_encode::decode_header_from_doc               block_encode.rs:400
loro_internal::…::block_encode::decode_header                        block_encode.rs:386
loro_internal::…::ChangesBlockBytes::ensure_header                   change_store.rs:2383
loro_internal::…::ChangeStore::decode_update_block
loro_internal::encoding::fast_snapshot::decode_updates
loro_internal::LoroDoc::import_batch
bivar_poc_loja_loro::LoroStore::import_updates_batch                 (nosso código)
bivar_poc_arquivo::check_bytes                                       (nosso código: o leitor do .bivar)
bivar_poc_verificador::fuzz_container                                (o alvo)
bivar_poc_verificador::fuzz_records                                  (o alvo)
```

**O que acontece, em palavras simples:**
1. Um bloco de mudanças traz uma lista de números: quantas dependências tem cada mudança.
2. A Loro soma esses números (`other_dep_num`, linha 158) e **reserva espaço para todos de uma vez** (linha 162 → `Vec::with_capacity(n)`).
3. Ela faz isso **antes de conferir se o arquivo tem mesmo esse tanto de dados.**
4. Nesta entrada, a soma deu 218.437.252.407 itens. A 8 bytes cada, isso dá exatamente 1.747.498.019.256 bytes, **o mesmo número do registro da CI**.

A entrada, cortada como o alvo corta, vira 5 registros: tipo 1 (367 bytes), 5 (8), 2 (367), 5 (8) e 2 (326). A queda acontece ao importar as atualizações.

## 4. Reprodução

**Como:**
- O código de cada commit foi exportado com `git archive` para uma pasta temporária. O repositório não foi tocado.
- Em cada cópia, um teste temporário (que não está no repositório) lê a entrada e chama a **mesma função do alvo**, `bivar_poc_verificador::fuzz_records`.
- Cada configuração rodou 10 vezes, em compilação otimizada (`release`), no Windows 11, com Rust 1.99.1.

**Bibliotecas:** iguais às do Fuzz. A Loro 1.17.1 e a `serde_columnar` 0.3.14 do `Cargo.lock` do protótipo são as mesmas de `fuzz/Cargo.lock`, e o `fuzz/` não muda entre o `main` e o `1e97bc7`.

| Código | O que é | Resultado | Repetiu? |
|---|---|---|---|
| `1e97bc7` | o commit do ramo onde o Fuzz falhou | caiu: `memory allocation of 1747498019256 bytes failed`, código `0xC0000409` | 10 de 10 iguais |
| `1aa6ee8` | o `main` atual | caiu, mesma mensagem, mesmo número, mesmo código | 10 de 10 iguais |
| `4b5aa28` | o `main` sem as mudanças do ramo (o ponto de onde o ramo saiu) | caiu, mesma mensagem, mesmo número, mesmo código | 10 de 10 iguais |

- **O código do protótipo e o `fuzz/` são idênticos entre `4b5aa28` e `1aa6ee8`.** O `git diff` está vazio nessas pastas, porque os commits do `main` depois disso só mexeram em documentos e na CI. Então "o `main` sem as mudanças do ramo" e "o `main` atual" rodam o mesmo leitor.
- **Mesmas condições da CI, não reproduzidas:** a CI roda em Linux, com o libFuzzer e o AddressSanitizer. Isso não foi reproduzido aqui: não há Linux local, WSL não está aprovado e a CI não foi rodada de novo.
  - **A ligação entre as duas** é o tamanho do pedido, idêntico até o último byte (1.747.498.019.256), e o fato de a mesma linha de código fazer esse pedido.
  - O resultado local não depende do sorteio: a função recebe só os bytes da entrada e chega ao mesmo pedido todas as vezes.

## 5. As mudanças do ramo alcançam o caminho do alvo?

**Não.** As diferenças entre o `main` e o `1e97bc7` são:

| Arquivo | O que mudou | Está no caminho do `bivar_records`? |
|---|---|---|
| `arquivo/src/lib.rs` | módulo `diag` novo (registro em arquivo, `/proc/locks`, trava global de diagnóstico); `let _gate = diag::gate()` em `open_writer`; registro em `DocFile::open_main`; `impl Drop for DocFile` que só registra | não. O alvo usa `build_file_bytes` e `check_bytes`, que trabalham sobre bytes na memória e não abrem arquivo, não travam e não criam `DocFile`. Essas duas funções não mudaram. |
| `verificador/src/lib.rs` | registro e trava global em volta do lançamento do verificador (`run_verifier`) | não. `fuzz_records` e `fuzz_container` não lançam processo e não mudaram. |
| `.github/workflows/diag-trava*.{yml,sh}`, `ci.yml` | jobs de diagnóstico e a CI | não. O workflow do Fuzz (`fuzz.yml`) não mudou. |
| `poc/diag-trava-repro/` | programa separado, sem código do Bivar | não |
| `docs/auditoria/2026-11-04-falha-ci-trava.md` | relatório (o ramo tem uma versão mais antiga) | não |

**Provado de duas formas:**
1. **Pelo código:** nenhuma linha alterada está na pilha da seção 3.
2. **Pelo resultado:** a falha é idêntica com e sem o código do ramo (seção 4).

**Por que o Fuzz só achou agora,** se passou no `main` em `343e453` e `f420b9d` e no ramo em `a1d47df`:
- cada rodada de 2 minutos sorteia mutações diferentes, a partir de uma semente diferente;
- esta entrada nasceu de uma única mutação (`ChangeBinInt`) de uma entrada da coleção;
- **passar numa rodada de fuzzing não prova que o defeito não existe:** as rodadas anteriores só não sortearam essa mutação.

## 6. Classificação

**Defeito no leitor que existe no `main`.** Mais exatamente: na biblioteca de documento (Loro 1.17.1, `block_meta_encode.rs:162`, e `serde_columnar` 0.3.14, `rle.rs:233`), alcançado pelo nosso leitor do `.bivar`.

| Pergunta | Resposta | Evidência |
|---|---|---|
| Está no leitor? | **sim**, na parte que é da Loro | pilha da seção 3: o pedido nasce dentro de `loro_internal`, chamado por `check_bytes` |
| Está no alvo de fuzzing? | não | o alvo só corta os bytes em registros e os entrega ao leitor. A mesma queda acontece abrindo a entrada como arquivo (seção 7). |
| Está no ambiente da CI? | não | cai igual no Windows, sem libFuzzer e sem AddressSanitizer. O ambiente só decidiu o rótulo: "crash", porque o AddressSanitizer corta pedidos acima de 1 TiB, em vez de "oom". |
| Criado pelo ramo? | não | seção 5 |

### Provado

1. A entrada faz a Loro pedir 1.747.498.019.256 bytes, sempre (30 de 30), no `1e97bc7`, no `1aa6ee8` e no `4b5aa28`.
2. O pedido nasce em `decode_changes_header`, que reserva espaço pela soma de números lidos do arquivo sem conferir antes se os dados existem.
3. Rodado direto no processo, o pedido derruba o processo inteiro; não é um pânico capturável.
4. As mudanças do ramo não estão no caminho do alvo.
5. Pelo verificador, o arquivo é recusado e o programa continua vivo (seção 7).

### Hipótese (não provada)

- **O mesmo resultado sob o AddressSanitizer, no Linux:** não foi reproduzido localmente. É muito provável, pelo número idêntico e pela mesma linha de código, mas não foi visto.
- **Se uma versão mais nova da Loro já corrige isso:** não foi verificado, porque exigiria acesso à rede.
- **Se há outras reservas do mesmo tipo na Loro:** neste mesmo trecho há outras listas reservadas pelo tamanho que vem do arquivo, como `Vec::with_capacity(n_changes)`. Elas não foram testadas uma a uma.

## 7. No `main`: o verificador contém a falha, ou o programa pode cair?

**O verificador contém a falha. Testado no `main`, numa cópia temporária:**
- a entrada foi transformada num arquivo `.bivar` inteiro, cortada exatamente como `fuzz_records` faz;
- o arquivo foi aberto do jeito que o programa abre um arquivo de fora (`open_checked`, com os limites normais e o verificador de verdade).

Resultado:
- o arquivo foi recusado com `Refused(TooMuchMemory { limit: 4 GiB })`;
- **o processo de teste continuou vivo** depois da recusa;
- o pedido de 1,6 TiB passou do teto de 4 GiB do verificador, o pedido falhou, o verificador caiu sozinho e o programa recusou o arquivo, como o ADR-0024 prevê.

**Onde o programa principal poderia cair, hoje, no protótipo:**
- **Arquivo de fora:** não cai. Ele passa antes pelo verificador, e a Loro do programa só vê bytes com a mesma impressão digital que o verificador aceitou (ADR-0024, seção 3).
- **Arquivo "confiável",** que este computador gravou por último: vai direto para a Loro do programa, mas só se a impressão digital bater com a anotada antes. Isso só acontece com bytes que o próprio programa gravou. Uma entrada como esta só chegaria lá se o nosso próprio gravador produzisse esses bytes, o que não foi observado.
- **Junção entre cópias (`merge_from`):** importa dados exportados de documentos já abertos, que passaram pelo caminho acima.
- **P2P:** ainda não existe. **É o ponto de risco futuro.** Atualizações da Loro vindas da rede passariam por esse mesmo decodificador, dentro do programa. O ADR-0024 já diz que as regras do P2P ainda precisam ser desenhadas, porque o verificador foi feito para arquivos em disco.
- **`cargo test`:** o teste `fuzz_replay.rs` roda as entradas de `fuzz/regressions/` direto no processo. **Se este arquivo fosse posto lá agora, o binário de testes inteiro cairia,** e não só um teste falharia (seção 8).

**Princípios e ADRs afetados:**

| Onde | Como |
|---|---|
| **ADR-0024, seção 2** (arquivo acima de um limite) | **A mensagem mostrada engana.** A pessoa lê que o arquivo "precisa de mais memória que o limite de 4 GiB" e que o limite "pode ser aumentado nas configurações". Mas o arquivo pede 1,6 TiB: aumentar o limite não resolve. A regra do ADR foi escrita para um arquivo legítimo acima do limite, e este não é um. A parte "também pode ser um ataque" está na mensagem, mas a sugestão de aumentar o limite é inútil aqui. |
| **ADR-0024, seção 5** (fuzzing) | O ADR diz que uma entrada que derruba um leitor "vira um arquivo em `fuzz/regressions/`" e que esses arquivos rodam no `cargo test`. **As duas regras se contradizem para este tipo de falha:** pôr o arquivo lá derruba o `cargo test` inteiro enquanto a Loro não for corrigida. |
| **ADR-0024, contexto** (o achado da descompressão sem teto) | É a mesma família: memória reservada sem teto dentro da Loro, onde o nosso código não tem como interromper. O desenho do verificador separado existe exatamente por isso, e funcionou. |
| **ADR-0011** (todo leitor tem fuzzing) | Funcionou como deveria: achou o defeito. |
| **ADR-0021** (a escolha da Loro) e **ADR-0014** (P2P) | É mais um ponto de robustez da Loro diante de dados hostis, a levar em conta no desenho do P2P. |
| **CLAUDE.md, seção 4** (entrada de rede é entrada não confiável) | Para o P2P: a validação precisa cobrir também este tipo de pedido de memória, e não só o formato. |

## 8. Tratamento proposto (não aplicado)

Nada abaixo foi aplicado. Cada item precisa da aprovação do dono.

**T1. Entrada de regressão e teste**
- **Arquivo:** `fuzz/regressions/bivar_records/crash-c6869029ad0329fc11298f77b7f2ea0fe497bf61` (os 1.091 bytes do artefato, sem mudança).
- **Problema:** hoje o `fuzz_replay.rs` roda tudo de `fuzz/regressions/` direto no processo, então esse arquivo derrubaria o `cargo test` inteiro. Há duas saídas:
  - **(a), recomendada:** uma pasta separada para "entradas que o verificador precisa recusar", por exemplo `fuzz/contidas/bivar_records/`. Os arquivos dela rodam **pelo verificador**, num teste novo em `verificador/tests/entrada.rs`, por exemplo `a_file_that_asks_for_terabytes_is_refused_and_the_program_survives`. O teste:
    1. monta o `.bivar` como `fuzz_records` monta;
    2. abre com `open_checked`;
    3. confere que o resultado é uma recusa por memória;
    4. confere que o processo de teste segue vivo e que a mensagem não sugere aumentar o limite (se T2 for aprovado).

    Quando a Loro for corrigida, o arquivo passa para `fuzz/regressions/`. Isso exige mudar o ADR-0024 (seção 5): ele está "Proposto", então pode ser editado com a aprovação do dono.
  - **(b):** só pôr o arquivo em `fuzz/regressions/` depois que a Loro for corrigida (T3). Até lá, nenhum teste o protege.

**T2. Mensagem honesta para um pedido absurdo**
- O verificador já lê o texto `memory allocation of N bytes failed` do processo que caiu.
- **A proposta:** ler também o número N. Se N for absurdo para qualquer documento real, mostrar outra mensagem: o arquivo pede uma quantidade impossível de memória, está danificado ou é um ataque, e aumentar o limite não resolve.
- **O que define "absurdo"** é decisão do dono. Por exemplo: mais que a memória total do computador, ou mais que 16 vezes o limite.
- **Exige:**
  - mudar o ADR-0024 (seção 2);
  - um teste com esta entrada;
  - o teste que já trava o texto do Rust continua valendo.

**T3. Corrigir na origem (a Loro)**
- **A correção certa** é a Loro (ou a `serde_columnar`) não reservar espaço pelo número lido antes de conferir que os dados existem: limitar a reserva ao que os bytes restantes comportam, ou usar `try_reserve`.
- **Opções:**
  - **(a), recomendada:** relatar o problema ao projeto da Loro, com esta entrada e a pilha da seção 3, e atualizar a versão quando sair a correção. Isso exige uma ação na rede (abrir o relato), feita pelo dono ou com a aprovação dele, e depois a aprovação da atualização, com `cargo deny` e `cargo audit`.
  - **(b):** uma cópia corrigida da Loro dentro do projeto (`[patch]`). Na prática, isso é uma dependência nova a manter. Não recomendo.
  - **(c):** o nosso leitor conferir o cabeçalho do bloco da Loro antes de entregá-lo. Isso exige reimplementar parte do formato interno da Loro, que muda entre versões. Não recomendo.

**T4. O Fuzz enquanto a Loro não for corrigida**
- **O que deve acontecer:** a coleção guardada já contém a entrada que originou esta (a `base unit`). O Fuzz deve voltar a cair nesta família com frequência, no `main` e em qualquer ramo, e um vermelho do Fuzz passará a ser ambíguo.
- **Opções:**
  - **(a):** aceitar o vermelho até T3, conferindo a cada falha se é a mesma pilha;
  - **(b):** o alvo de fuzzing reproduzir o teto de memória do verificador. Um pedido acima de 4 GiB contaria como "recusado", e não como queda. Isso não é possível no mesmo processo sem mudar o comportamento de falha de alocação do Rust, que hoje sempre encerra o processo. Precisaria de um estudo à parte.
- **Recomendo (a)** e não proponho mudar o `fuzz.yml` agora.

**Minha recomendação:** T1(a) + T2 + T3(a), com T4(a) enquanto isso.

## 9. Riscos

- **P2P:** é o risco maior. Sem um verificador para dados de rede, uma atualização hostil derruba o programa principal.
- **Fuzz ambíguo:** até T3, falhas do Fuzz podem ser este mesmo caso. Cada uma precisa ser conferida pela pilha.
- **Mensagem enganosa:** até T2, a pessoa é convidada a aumentar um limite que não resolve.
- **Pode haver mais casos assim:** outras reservas pelo tamanho que vem do arquivo, na mesma parte da Loro, não foram testadas.

## 10. Como foi feito (nada ficou no repositório)

- **Pasta temporária:** `C:\Users\Bruno\AppData\Local\Temp\claude\…\scratchpad\`, fora do repositório:
  - `fuzzart\`: o zip extraído;
  - `fuzzrepro\<commit>\`: o código de cada commit, via `git archive`.
- **Testes temporários,** só nessas cópias:
  - `zz_replay_plain.rs`: chama `fuzz_records`;
  - `zz_replay_trace.rs`: preparado, não foi preciso;
  - `zz_replay_verifier.rs`: monta o `.bivar` e abre com `open_checked`.
- **Nenhuma dependência nova e nenhum `unsafe`,** nem nos testes temporários.
- **No repositório:** só este relatório foi criado. Não houve commit, `fuzz/regressions/` não foi tocada e a CI não rodou de novo.

---

## 11. Segunda rodada: diagnósticos pedidos pelo dono (2026-11-04)

**Decisões do dono:**
- **Decisão 1:** opção (a).
- **Decisão 2:** sim, sujeita à medição do passo 3.
- **Decisão 3:** sim; o dono é quem envia.
- **Decisão 4:** recusada; seguir o passo 4.

O `main` não mudou: não houve commit e este arquivo continua sem commit. As medidas na CI rodaram no ramo temporário `diag/fuzz-fork` (commit `2cc334d`, execução `41913641114`), que nunca será juntado.

### 11.1 Passo 1: a queda só com a Loro

- **Onde:** `A:\Projetos\bivar-diag\loro-repro\`, fora do repositório.
- **O programa:** usa só a Loro, sem código do Bivar.
  - `v1171`: Loro 1.17.1, com as mesmas versões do `Cargo.lock` do protótipo;
  - `vmain`: o `main` da Loro, commit `ea1d383e`, de 2026-11-03, baixado do GitHub.
- **A entrada:** os bytes que o leitor entrega à Loro, separados dos registros da entrada do Fuzz:
  - `1-snapshot.bin` (367 bytes);
  - `3-updates.bin` (367 bytes);
  - `5-updates.bin` (326 bytes).
- **Cada caso rodou 10 vezes.**

| Caso | Loro 1.17.1 | Loro `main` |
|---|---|---|
| Caminho do Bivar: `from_snapshot` + `import_batch` com as duas atualizações | cai, 10 de 10 | cai, 10 de 10 |
| `LoroDoc::new()` + `import_batch` com as duas atualizações | cai, 10 de 10 | cai, 10 de 10 |
| `LoroDoc::new()` + `import` de uma atualização sozinha, como veio | recusa, 10 de 10 (`DecodeChecksumMismatchError`) | recusa, 10 de 10 |
| `LoroDoc::new()` + `import` de `5-updates.bin`, com o selo recalculado | **cai, 10 de 10** | **cai, 10 de 10** |
| `from_snapshot` + `import` de `5-updates.bin`, com o selo recalculado | cai, 10 de 10 | cai, 10 de 10 |
| `import` de `3-updates.bin`, com o selo recalculado | recusa ("Invalid magic number") | não rodado |

Em todas as quedas, o pedido foi o mesmo: `memory allocation of 1747498019256 bytes failed`, código `0xC0000409`.

**A entrada mínima tem 326 bytes:** uma única atualização com o selo recalculado, aberta com `LoroDoc::new().import(...)`.

**Sobre o selo:**
- é um xxh32 com a semente `"LORO"`, que qualquer pessoa pode recalcular;
- recalculei o selo com uma cópia do algoritmo escrita no próprio programa, sem dependência nova;
- a cópia confere: na foto do documento, ela dá exatamente o selo que já estava lá.

**Visto no código da Loro:** `import_batch` com 2 ou mais blocos lê o conteúdo de cada um sem conferir o selo (`decode_import_blob_meta(b, false)`, em `loro.rs`). Por isso, a entrada original do Fuzz, com selos errados, derruba pelo lote e é recusada pelo `import` comum.

### 11.2 Passo 2: a rota de atualizações (a do P2P) chega ao mesmo trecho?

**Sim.**

- **Testado:** `LoroDoc::new().import(bytes)` com uma única atualização de 326 bytes derruba o processo, na 1.17.1 e no `main` da Loro, 10 vezes de 10. Essa é a chamada que um par de P2P usaria para aplicar as mudanças de outro par.
- **Só lido no código:**
  - `import` com o modo `FastUpdates` passa por `fast_snapshot::decode_updates` → `ChangeStore::decode_update_block` → `decode_changes_header`, o mesmo trecho da pilha da seção 3;
  - o selo não protege: além de ser público, nem é conferido no lote;
  - o formato antigo (`OutdatedRle`) e as atualizações em JSON têm outro caminho e não foram testados.
- **Consequência:** no P2P, uma atualização hostil derruba o programa principal se for importada no mesmo processo. Hoje o protótipo não tem P2P. A pendência do ADR-0024 sobre P2P ganha esta prova.

### 11.3 Passo 3: o maior pedido único de memória

**Como foi medido:**
- O verificador de verdade rodou no Linux da CI, sob `strace`, com os limites normais.
- A glibc foi configurada para atender todo pedido de 128 KiB ou mais com uma chamada própria ao sistema (`mmap` ou `mremap`). Assim, cada pedido grande aparece com o seu tamanho, arredondado para cima em no máximo 4 KiB + 16 bytes.
- Um pedido recusado pelo teto de memória não chega à glibc. O tamanho dele vem da mensagem do próprio verificador.
- Em "dados do documento" entram só os registros de foto e de atualizações; arquivos grandes ficam de fora (nesses arquivos, eram 0).

**Documentos legítimos, com o teto normal de 4 GiB** (todos abriram):

| Operações | Dados do documento | Maior pedido único | Pedido ÷ dados |
|---|---|---|---|
| 250 mil | 8,2 MB | 8,4 MB | 1,02 |
| 500 mil | 16,7 MB | 16,8 MB | 1,01 |
| 1 milhão | 33,8 MB | 33,8 MB | 1,00 |
| 2 milhões | 68,7 MB | 68,7 MB | 1,00 |

O maior pedido é a leitura do próprio registro do documento para a memória: do tamanho dos dados, nunca 256 vezes maior.

**Documentos legítimos recusados de propósito, com tetos menores** (1 GiB, 512, 256, 128 e 64 MiB): 18 recusas.
- O maior pedido recusado foi 68.684.139 bytes, igual aos dados do documento (razão 1,0).
- Nenhum pedido recusado foi maior que o teto.
- **A regra deu "normal" nas 18.** Nenhum documento legítimo seria chamado de absurdo.

**As bombas de descompressão existentes** (200 MB de uma letra, 0,8 a 1,6 MB no disco):

| Bomba | Teto | Pedido recusado | Pedido ÷ dados | Regra |
|---|---|---|---|---|
| removida | 256 MiB | 268.435.456 | 170,8 | normal |
| só no histórico | 256 MiB | 67.108.864 | 85,4 | normal |
| mapa da raiz | 256 MiB | 268.435.456 | 170,9 | normal |
| visível | 256 MiB | não lido (esta linha não chegou nos avisos da CI; está no artefato `diag-measure`) | — | — |
| as quatro | 4 GiB | abrem; maior pedido de 268 MB (170,8 vezes os dados) | — | — |

- **A regra não pega nenhuma das três bombas lidas.**
  - O pedido recusado nunca passou do teto: 256 MiB é igual ao teto, não maior.
  - E ficou abaixo de 256 vezes os dados: no máximo 170,9 vezes.
- **A margem é pequena, cerca de 1,5 vez.** A compressão LZ4 chega a cerca de 255 para 1, e um vetor que dobra de tamanho pode pedir até 2 vezes. Então uma bomba mais compacta que as nossas poderia passar de 256 vezes e receber a mensagem de "pedido absurdo". Para uma bomba, essa mensagem também é verdadeira, mas o dono pediu que as bombas existentes fiquem fora da regra, e elas ficam.
- **A entrada do Fuzz:** pedido de 1.747.498.019.256 bytes, com 1.060 bytes de dados (cerca de 1,6 bilhão de vezes), acima do teto de 4 GiB. **A regra deu "absurdo".**

### 11.4 Passo 4: o modo `-fork` do libFuzzer na CI

**O que o código do libFuzzer mostrou antes de medir** (`libfuzzer-sys` 0.4.13, `FuzzerFork.cpp` e `FuzzerLoop.cpp`):
1. **`-ignore_ooms=1` sozinho não cobre esta família.** O AddressSanitizer recusa pedidos acima de 1 TiB e sai com o código 1, que o modo `-fork` conta como queda comum, não como falta de memória (código 71).
2. **No fim do tempo, o modo `-fork` sai com o código do último processo filho.** Se o último foi uma falta de memória ignorada, o job ficaria vermelho.

**O que o ramo testou:**
- `ASAN_OPTIONS=allocator_may_return_null=1`;
- um gancho de falha de alocação (*nightly*, só em `fuzz/`): ele anota o tamanho e faz um pedido de 8 GiB, acima do limite de 4 GiB do libFuzzer. O libFuzzer então registra falta de memória, guarda a entrada como `oom-…` e sai com o código 71. Se isso não acontecer, o processo cai como antes, e nunca em silêncio;
- um script que aceita o código 71 só quando o libFuzzer diz que o tempo acabou;
- as opções `-fork=2 -ignore_ooms=1 -ignore_timeouts=0 -ignore_crashes=0 -rss_limit_mb=4096 -timeout=60`.

**Prova com a entrada real e com quedas artificiais:** um alvo de prova dispara o caso quando a entrada começa com `!`. Cada caso rodou num job.

| Caso | Job | Código final do libFuzzer | O que mostrou |
|---|---|---|---|
| entrada real (`fuzz_records` com os 1.091 bytes) | **verde** | 71 → aceito, porque o tempo acabou | **8.005 faltas de memória ignoradas e contadas** (`oom/timeout/crash: 8005/0/0`), 3.438 entradas `oom-…` diferentes guardadas, 8.007 linhas do gancho com o tamanho; a rodada foi até o fim dos 300 s |
| pânico artificial | **vermelho** | 77 | parou na hora; entrada `crash-…` guardada |
| `abort` artificial | **vermelho** | 77 | parou na hora |
| estouro de pilha artificial | **vermelho** | 1 (AddressSanitizer) | parou na hora |
| tempo esgotado artificial | **vermelho** | 70 | parou depois de 61 s; entrada `timeout-…` guardada |

**O que não foi cumprido: a rodada de 30 minutos.**
- O job `real-30min` (alvo `bivar_records`, 1.800 s) **parou aos 231 s numa queda nova e diferente desta:** um pânico da Loro em `loro-kv-store-1.17.0/src/block.rs:305`, `expect("validated SSTable block should decode")`.
- A mesma linha existe no `main` da Loro (`crates/kv-store/src/block.rs:305`).
- O job ficou vermelho, como deve ficar com um pânico. Por isso não provou os 30 minutos.
- A entrada está no artefato `diag-real-30min` (`crash-0c03624454b91bd7277d8f01e2c3173ac606e1b9`), que exige login. Essa queda ainda não foi analisada.

**Observação sobre o tempo:**
- No caso da entrada real, 300 s pedidos viraram 779 s de rodada. Ao fim do tempo, o libFuzzer espera os processos filhos em andamento terminarem, e cada um pode ter até 300 s.
- Numa rodada diária de 1.800 s, isso pode passar dos 30 minutos. A sobra não foi medida para 1.800 s.

**Efeito mais amplo:** `-ignore_ooms=1` ignora toda falta de memória, também a de memória ocupada acima de 4 GiB, e não só os pedidos absurdos. No programa, toda falta de memória do verificador vira recusa, então é a mesma família. Mesmo assim, é uma mudança de alcance maior que esta entrada.

### 11.5 Canal de segurança da Loro e rascunho do relato

- **A Loro não tem canal privado de segurança:**
  - não há `SECURITY.md`, nem no repositório nem na organização;
  - a API do GitHub responde `private-vulnerability-reporting: enabled: false`;
  - restam as *issues* públicas e um Discord (no README).
- **O rascunho, em inglês, está em `A:\Projetos\bivar-diag\loro-report\`**, com:
  - a entrada mínima `loro-import-alloc.bin`, de 326 bytes;
  - a reprodução em 6 linhas de Rust;
  - o aviso de uso de IA;
  - um `LEIA-ME.txt` em português.
- **A reprodução foi conferida** exatamente como está no rascunho: caiu 3 de 3 vezes.
- Nada foi publicado.

### 11.6 Plano proposto (não aplicado)

1. **Entrada numa pasta própria:**
   - **Pasta:** `fuzz/verifier-refused/bivar_records/crash-c6869029ad0329fc11298f77b7f2ea0fe497bf61`.
   - **Teste novo em `verificador/tests/entrada.rs`** (`inputs_the_verifier_must_refuse_are_refused_and_the_program_survives`). Para cada arquivo da pasta, o teste:
     - monta o `.bivar` como `fuzz_records` monta;
     - abre com `open_checked`;
     - confere a recusa por pedido absurdo e o texto da mensagem;
     - confere que o processo de teste segue vivo.
   - **`fuzz/README.md` e o ADR-0024 registram quando a entrada passa para `fuzz/regressions/`:** quando uma versão da Loro, aprovada e atualizada, deixar de cair com ela. A conferência é feita rodando cada entrada num processo separado.
2. **Mensagem para pedido absurdo:**
   - **Recusa nova:** o verificador já lê o tamanho N do pedido que falhou. O programa principal já lê a estrutura do arquivo e calcula os dados do documento. Se N passar do teto e de 256 vezes os dados, a recusa é de "pedido absurdo".
   - **O que a mensagem diz:**
     - o tamanho do arquivo e o pedido;
     - que isso não acontece com um documento real;
     - que o arquivo está danificado ou foi feito para atacar;
     - que aumentar o limite não deve resolver.
   - **Testes:**
     - esta entrada recebe a mensagem nova;
     - as quatro bombas e o documento legítimo acima do limite continuam com a mensagem atual.
3. **ADR-0024:**
   - **seção 2:** a regra e a mensagem do pedido absurdo, com a medição de 11.3 e a margem de cerca de 1,5 vez sobre as bombas;
   - **seção 4:** esta entrada na tabela de estresse;
   - **seção 5:** a pasta `verifier-refused/` e a regra de passagem para `regressions/`;
   - **seção 5, só se o dono aprovar:** o modo `-fork`, o gancho e o código 71;
   - **pendências:**
     - o resultado do passo 2: a rota de atualizações do P2P chega ao mesmo trecho e derruba o programa;
     - o pânico novo em `loro-kv-store`;
     - o relato à Loro.
4. **Fica de fora do plano até nova decisão:** a mudança do `fuzz.yml` para o modo `-fork`. A rodada de 30 minutos não foi provada e o pânico novo vai deixar o Fuzz vermelho no `main` assim que for sorteado de novo.

---

## 12. Terceira rodada: pânico da Loro, censo de 30 minutos e plano ajustado (2026-11-10)

**Decisões do dono para esta rodada:**
- o plano de 11.6 está aprovado, mas **não foi implementado**; o `-fork` não entra no `main` nesta etapa;
- **regra do pedido absurdo:** o pedido passa do limite de memória **e** é maior que **1.000 vezes** os dados do documento. A mensagem diz que aumentar o limite "provavelmente não resolve";
- prova dos 30 minutos: nem (a) nem (b); fazer o censo.

O `main` não mudou: nenhum commit e nenhum arquivo do programa alterado. Só este relatório foi editado, e continua sem commit. O censo rodou no ramo temporário `diag/fuzz-censo` (commit `68e404f`, execução `42852601857`), que nunca será juntado.

### 12.1 Correção da seção 11.4

A seção 11.4 atribuiu números à "entrada real" que **vieram do alvo de prova `diag_probe`**, e não do alvo real `bivar_records`:

| Número | De onde veio de fato |
|---|---|
| 8.005 faltas de memória ignoradas e contadas | `diag_probe` (cobertura 29: o alvo só dispara a entrada fixa quando a mutação começa com `!`) |
| 3.438 arquivos `oom-…` guardados | `diag_probe` |
| 779 s de rodada para 300 s pedidos | `diag_probe` |

- **A rodada do alvo real (`real-30min`) terminou com `oom/timeout/crash: 0/0/0`** até cair aos 231 s, no pânico da Loro (aviso da execução `41913641114`). Nenhuma falta de memória foi contada.
- **Portanto, a seção 11.4 não provou o requisito da entrada real** ("a entrada de 1,6 TiB é ignorada e contada no alvo real").
- O censo (12.3) mostra a entrada real contada no alvo real, mas **só com `-keep_seed=1`**, que o `fuzz.yml` não usa. Do jeito que o Fuzz roda, o requisito continua sem prova.

### 12.2 O pânico em `loro-kv-store/src/block.rs:305`

**O artefato da execução `41913641114` não foi encontrado.** A pasta `A:\Projetos\bivar-diag\fuzz-panico\` não existe nesta máquina, não há `gh` instalado e o download de artefatos exige login. A entrada `crash-0c03624454b91bd7277d8f01e2c3173ac606e1b9` **não foi analisada**. A análise abaixo usa entradas construídas à mão, que chegam à mesma linha.

**O que o código faz** (lido na 1.17.1 e no `main` da Loro):
1. A foto (snapshot) traz duas tabelas SSTable: histórico (*oplog*) e estado. Cada bloco tem um selo xxh32 com semente `"LORO"`, que qualquer pessoa recalcula.
2. Ao importar, `MemKvStore::import_all` chama `SsTable::import_all(bytes, false)`: confere os limites e os selos dos blocos, mas **não decodifica os blocos** (`validate_blocks = false`).
3. Cada bloco é decodificado só quando é lido pela primeira vez (`SsTable::read_block` → `Block::decode`). Ali, qualquer erro vira pânico: `.expect("validated SSTable block should decode")`. O comentário da Loro diz que quem chama "é responsável por validar a integridade". Mas integridade aqui é só o selo, e um bloco com selo certo pode estar malformado.
4. Durante o pânico, a trava interna do documento (`LoroMutex`) fica envenenada. Ao ser descartado, o `LoroDoc` entra em pânico de novo (`poisoned LoroMutex`, em `sync.rs:34`). Se isso acontece durante o desenrolar do primeiro pânico, **o processo aborta** (`0xC0000409` no Windows).

**Reprodução com a Loro pura** (`A:\Projetos\bivar-diag\loro-panico\`, fora do repositório, sem código do Bivar):
- **Entradas:** uma foto válida com **um bloco quebrado por vez**. Num bloco sem compressão, o número de entradas foi zerado; num bloco LZ4, o quadro foi zerado. Depois, o selo do bloco e o selo do blob foram recalculados.
- **Versões:** Loro 1.17.1; `main` `ea1d383e` (2026-11-03); `main` `c55cebff` (2026-11-10, o mais novo).
- **Repetições:** cada caso rodou 3 vezes. Todos os resultados foram iguais nas três versões.

| Rota | Bloco do histórico quebrado | Bloco do estado quebrado |
|---|---|---|
| `LoroDoc::from_snapshot` (a rota do leitor do Bivar) | pânico em `block.rs:305` | pânico, depois aborta |
| `LoroDoc::new().import` | pânico, depois aborta | pânico, depois aborta |
| documento já aberto, `import` ou `import_batch` de uma foto | **pânico, depois aborta** | sem pânico: essa rota não lê o estado |
| documento já aberto, `import` dentro de `catch_unwind` | o pânico é capturado, mas **descartar o documento depois gera outro pânico** e o processo termina (código 101) | — |

- **Foto real do Bivar:** a foto da entrada do Fuzz (`1-snapshot.bin`), com um bloco quebrado, dá pânico em `block.rs:305` nas três versões, tanto pelo bloco do histórico quanto pelo do estado.
- **Entrada mínima:** `loro-sstable-panic.bin`, com 256 bytes (SHA-256 `98dcbc3b…818c`). `LoroDoc::new().import(bytes)` aborta, 3 de 3 nas três versões.

**É o mesmo defeito da queda de 1,6 TiB? Não.**

| | Queda de 1,6 TiB (e 12 GiB, 12.3) | Pânico de `block.rs:305` |
|---|---|---|
| Biblioteca | `loro-internal` + `serde_columnar` | `loro-kv-store` |
| O que acontece | reserva memória pelo número lido, antes de conferir os dados | decodifica o bloco tarde e transforma o erro em pânico |
| Dados que chegam lá | atualizações (blocos de mudanças) | fotos (tabelas SSTable) |
| Resultado | o processo aborta por falta de memória | pânico; trava envenenada; aborta |

- **O que os dois têm em comum:** a Loro confia em bytes que só têm um selo recalculável.
- **No nosso leitor,** só o registro de foto leva tabelas SSTable à Loro. Então a entrada `crash-0c036…` chegou lá, com certeza, por um registro de foto. Isso foi **lido no código**; a entrada em si não foi vista.

**A rota de atualizações chega a ele?**
- **As atualizações em si (`FastUpdates`), não.** Elas não carregam tabelas SSTable. `decode_updates` → `decode_update_block` lê só blocos de mudanças. Isso foi **lido no código**, não provado com mutações.
- **Mas a mesma chamada `import` chega,** se o blob vier no modo foto. O modo é um campo do blob, escolhido por quem envia. Um documento já aberto aceita uma foto e a lê por `decode_snapshot_for_updates`, que percorre os mesmos blocos. Isso foi **testado** (tabela acima).
- **Para o P2P:** um colega que mande uma foto em vez de atualizações derruba o programa principal se o `import` rodar no mesmo processo. Recusar fotos vindas da rede (conferindo o modo antes do `import`) fecha esta rota. Não fecha a da queda de 1,6 TiB e 12 GiB, que vem pelas próprias atualizações.

**O que o verificador faz** (cópia temporária do `main` `1aa6ee8`, abrindo o `.bivar` com `open_checked`, os limites normais e o verificador de verdade, 3 de 3):

| Arquivo | Resultado | Mensagem mostrada |
|---|---|---|
| foto real do Bivar, sem dano | abre | — |
| bloco do histórico quebrado | recusado, `Crashed` | "This file could not be read safely: exit code: 101; note: run with \`RUST_BACKTRACE=1\` environment variable to display a backtrace" |
| bloco do estado quebrado | recusado, `Crashed` | "This file could not be read safely: exit code: 0xc0000409;   70:     0x7ff854afee4c - RtlUserThreadStart" |

- **O programa segue vivo** nos três casos.
- **A mensagem não fala de memória.** Isso está certo, porque a causa é outra.
- **Mas a mensagem não é clara:** mostra uma nota técnica do Rust ou uma linha de pilha, porque o programa exibe a última linha do erro do processo filho. Ela não diz que o arquivo está danificado nem o que a pessoa pode fazer.
- **Direto no processo** (`fuzz_records`, como o alvo de fuzzing): o bloco do histórico termina com pânico simples (101); o bloco do estado aborta o processo.

### 12.3 Censo de 1.800 s, `bivar_records` em modo `-fork`

**Como rodou:**
- **Opções:** `-fork=2 -keep_seed=1 -ignore_crashes=1 -ignore_ooms=1 -ignore_timeouts=1 -rss_limit_mb=4096 -timeout=60 -max_len=1048576`, mais o gancho de falta de memória do ramo `diag/fuzz-fork`. O `-ignore_crashes=1` foi usado **só nesta rodada**.
- **Sementes:** a coleção guardada (`fuzz/found`), as sementes do repositório e a entrada de 1,6 TiB. Depois da junção, ficaram 1.150 sementes.
- **Por que `-keep_seed=1`:** visto no código do libFuzzer (`FuzzerFork.cpp`). Sem ele, a junção inicial do modo `-fork` descarta as sementes que derrubam o processo, e a entrada de 1,6 TiB nunca seria executada.
- **Depois da rodada,** cada entrada guardada foi executada sozinha, com nomes de função (`llvm-symbolizer` 18), e agrupada pela pilha. Uma entrada por família foi guardada no artefato `diag-census`, e as demais foram descartadas. Os números foram lidos nos avisos da execução, que não exigem login.

**Resultados:**

| Medida | Censo (`-fork=2`, 30 min) | Rodada sem `-fork` (`41876124671`, 2 min, até cair) |
|---|---|---|
| tempo pedido / real | **1.800 s / 1.814 s** (14 s a mais) | 120 s pedidos; caiu em cerca de 70 s |
| execuções | 8.738.476 em 84 processos filhos | 362.797 |
| execuções por segundo | **4.817 no total** (2 processos; de 1.424 a 2.791 por processo filho nas amostras) | cerca de 5.200 (1 processo) |
| cobertura | 11.540 aos 124 s; 11.560 aos 233 s; 11.680 aos 606 s; 11.708 aos 1.208 s; **11.764 aos 1.812 s** | 11.694 no início; **11.729** aos cerca de 70 s |
| coleção | 1.387 → 1.506 entradas | 304 → 345 entradas |
| faltas de memória / tempos esgotados / quedas | **4 / 0 / 0** | caiu na primeira (1,6 TiB) |
| arquivos guardados | **2** (`oom-…`) | 1 |

- **Execuções por segundo:** dois processos em paralelo, cada um cerca de metade da rodada sem `-fork`, somam quase o mesmo. O `-fork` não ganhou velocidade, mas também não perdeu.
- **Cobertura:** não dá para comparar diretamente.
  - O modo `-fork` conta a cobertura depois de juntar cada processo filho, e começou de outra coleção (1.150 sementes, com `-keep_seed=1`).
  - Nos primeiros minutos, ficou abaixo da rodada sem `-fork` (11.540 contra 11.694 no início).
  - Aos 30 minutos, passou do maior número da rodada sem `-fork` (11.764 contra 11.729).
  - **Não há rodada sem `-fork` de 30 minutos para comparar:** todas caem antes.
- **Tempo:** a sobra foi de só 14 s, contra 479 s do `diag_probe`. Ao fim do tempo, o libFuzzer espera os processos filhos em andamento, e cada filho roda no máximo `min(300 s, número do filho)`. No censo eram 84 filhos, então a sobra máxima era de 84 s; no `diag_probe` eram mais de 8.000 filhos, então chegava a 300 s.
- **O pânico de `block.rs:305` não apareceu** em 30 minutos, depois de sair aos 231 s na rodada de 2026-11-04. Isso é sorte do sorteio e não prova que ele seja raro.

**Famílias distintas, agrupadas pela pilha:**

| Família | Tipo | Pedido | Pilha (onde nasce) | Entrada guardada | Vezes |
|---|---|---|---|---|---|
| F1 | falta de memória pelo gancho (pedido acima de 1 TiB) | 1.747.498.019.256 bytes | a entrada real de 1,6 TiB: `decode_changes_header`, `block_meta_encode.rs:162` (seção 3) | `oom-c6869029…` (1.091 bytes, a própria semente) | 3 (3 linhas do gancho com esse tamanho exato) |
| F2 | falta de memória pelo limite de 4 GiB do libFuzzer (um pedido só) | 12.884.901.884 bytes (cerca de 12 GiB) | `Vec::with_capacity(n_changes)`, **`block_meta_encode.rs:132`** ← `decode_changes_header` ← `ensure_header` ← `decode_update_block` ← `decode_updates` ← `import_batch` ← `bivar_poc_arquivo::check_bytes` | `oom-d8f611603e5679d75944b87fe7239a356336f783` (955 bytes) | 1 |

- **Nenhuma queda fora da Loro.** As duas famílias nascem em `loro-internal`, chamadas pelo nosso leitor.
- **A entrada real:** executada como semente e contada 3 vezes como falta de memória.
- **F2 é a mesma família da queda de 1,6 TiB,** em outra linha da mesma função: reserva memória pelo número de mudanças lido do arquivo (3.221.225.471 × 4 bytes) antes de ler os dados. A seção 6 tinha listado esse ponto como "lido no código, não testado"; agora está confirmado.
  - **A pilha** foi vista no Windows, com o mesmo teto de 4 GiB do verificador (biblioteca `cap`) e um gancho de falha de alocação (*nightly*), num teste temporário fora do repositório.
  - **Com a Loro pura:** uma atualização sozinha de 190 bytes, com o selo recalculado (`loro-import-alloc-12gib.bin`), faz `LoroDoc::new().import` pedir 12.884.901.884 bytes. O pedido falhou e o processo abortou, 3 de 3, na 1.17.1, em `ea1d383e` e em `c55cebff`.
  - **Uma diferença importante para a 1,6 TiB:** 12 GiB cabem na memória de máquinas grandes. Lá, o pedido pode dar certo e só desperdiçar memória antes do erro.
  - **Pelo verificador** (cópia temporária do `main`, 3 de 3): recusado com `TooMuchMemory { limit: 4 GiB }`, e o programa segue vivo. Mas a mensagem é a enganosa: diz que o limite "pode ser aumentado". Pela regra do pedido absurdo, ela seria corrigida: 12,9 GB para 924 bytes de dados, cerca de 13,9 milhões de vezes.

### 12.4 Defeitos, gravidade e rota

| # | Defeito | Onde | Rota | Arquivo de fora (hoje) | P2P (futuro, no mesmo processo) | Gravidade |
|---|---|---|---|---|---|---|
| D1 | reserva de memória pelo número lido: 1,6 TiB (linha 162) | Loro, `decode_changes_header` | atualizações | contida pelo verificador | **aborta o programa principal**; `catch_unwind` não ajuda | **alta** |
| D1b | o mesmo defeito, linha 132: 12 GiB | Loro, mesma função | atualizações | contida pelo verificador | aborta, ou consome 12 GiB numa máquina grande | **alta** |
| D2 | pânico ao decodificar um bloco SSTable; trava envenenada; aborta | Loro, `loro-kv-store` `block.rs:305` | fotos (inclusive uma foto enviada a um documento já aberto) | contida pelo verificador | **aborta**, se o P2P aceitar fotos; fechado se só aceitar atualizações | **alta** (média, se o P2P recusar fotos) |
| D3 | mensagem enganosa para pedido absurdo ("o limite pode ser aumentado") | nosso código, `Refusal::TooMuchMemory` | D1, D1b | a pessoa é convidada a aumentar um limite que não resolve | — | média |
| D4 | mensagem confusa para pânico: mostra a última linha do erro (nota do Rust ou linha de pilha) | nosso código, `Refusal::Crashed` | D2 | não diz "danificado" nem o que fazer | — | média |
| D5 | o ADR-0024 manda pôr entradas que derrubam em `fuzz/regressions/`, que roda no `cargo test`; D1, D1b e D2 derrubariam o binário de testes inteiro | nosso processo | — | — | — | baixa (de processo) |

- **Observação, não defeito:** `import_batch` com dois ou mais blobs lê o conteúdo sem conferir o selo (seção 11.1). Como o selo é recalculável, isso não muda o impacto.

### 12.5 O `-fork` merece ir para o `main`?

**Recomendação: sim, mas só depois das entradas 1 a 3 do plano (12.6), e sem ignorar quedas.**

**Por que vale:**
- **Sem `-fork`, cada falta de memória da família D1 deixa o Fuzz vermelho.** O censo achou uma entrada nova da família (F2) em cerca de 8,7 milhões de execuções, uma rodada diária de 30 minutos. Então é provável que a rodada diária fique vermelha com frequência por um defeito já conhecido, e um vermelho do Fuzz deixa de dizer algo novo.
- **Com `-fork` e só `-ignore_ooms=1`,** a família D1 é contada e guardada, e a rodada segue até o fim. Quedas e tempos esgotados (por exemplo, o D2) continuam parando a rodada no vermelho.
- **A velocidade não piorou** (12.3).

**O que não deve ir:**
- **`-ignore_crashes=1` e `-ignore_timeouts=1`:** esconderiam o D2 e qualquer queda nova.
- **`-keep_seed=1`:** só serve para o censo; a coleção do `main` não tem entradas que derrubam.

**Como controlar a duração:**
- **Tempo pedido igual ao orçamento menos 300 s.** Cada processo filho roda no máximo 300 s, e o libFuzzer espera os que estão em andamento: 1.500 s na rodada diária, e nos envios, os 120 s atuais (lá os filhos nunca passam de cerca de 20 s). Assim, o fim fica abaixo de 1.800 s mesmo no pior caso. No censo, a sobra foi de 14 s; no `diag_probe`, foi de 479 s.
- **`timeout-minutes` no job como segunda trava** (por exemplo, 45 minutos).
- **O script** (como o de 11.4) aceita o código 71 só quando o libFuzzer diz que o tempo acabou.

**Como controlar o número de arquivos:**
- As entradas vão para uma pasta temporária, não para o artefato.
- Depois da rodada, cada uma é executada sozinha e agrupada pela pilha, como no censo. **Só uma entrada por família** sobe como artefato, com a contagem.
- O libFuzzer guarda um arquivo por entrada distinta. No censo foram 2; no `diag_probe`, 3.438. O agrupamento torna o número indiferente.
- **Família nova de falta de memória** (pilha fora de uma lista aprovada, guardada em `fuzz/`): o job fica vermelho, para que alguém olhe. Família conhecida: aviso, com a contagem.

**Custo:** o gancho de falta de memória usa um recurso *nightly* (`alloc_error_hook`), só em `fuzz/`, que já usa *nightly*. O agrupamento usa Python da máquina da CI, sem dependência nova no projeto.

### 12.6 Plano ajustado (não aplicado; cada item precisa da aprovação do dono)

**1. Entradas, numa pasta própria** (o plano 11.6, ampliado):
- **Pasta:** `fuzz/verifier-refused/bivar_records/`, com:
  - `crash-c6869029ad0329fc11298f77b7f2ea0fe497bf61` (1.091 bytes): D1, 1,6 TiB;
  - `oom-d8f611603e5679d75944b87fe7239a356336f783` (955 bytes): D1b, 12 GiB;
  - **para o D2:** a entrada `crash-0c03624454b91bd7277d8f01e2c3173ac606e1b9`, quando o dono a trouxer. Até lá, as fotos do Bivar com um bloco quebrado, feitas à mão, em formato de registros: uma com o bloco do histórico e outra com o bloco do estado. Elas são entradas construídas, não achadas pelo Fuzz, e isso fica escrito no `fuzz/README.md`.
- **Teste novo** em `verificador/tests/entrada.rs` (`inputs_the_verifier_must_refuse_are_refused_and_the_program_survives`). Para cada arquivo, o teste:
  - monta o `.bivar` como `fuzz_records` monta e abre com `open_checked`;
  - confere a recusa **certa**: "pedido absurdo" para D1 e D1b; "danificado" para D2;
  - confere o texto da mensagem;
  - confere que o processo de teste segue vivo.
- **Uma lista ao lado da pasta** diz, para cada arquivo, a recusa esperada e o defeito da Loro.
- **Regra de passagem para `fuzz/regressions/`:** quando uma versão da Loro, aprovada e atualizada, deixar de cair com a entrada, conferido rodando cada entrada num processo separado.

**2. Mensagens:**
- **Pedido absurdo** (D3). O verificador já lê o tamanho N do pedido que falhou ("memory allocation of N bytes failed"). O programa principal já calcula os dados do documento (registros de foto e de atualizações).
  - **A regra:** N maior que o limite de memória **e** maior que 1.000 vezes os dados.
  - **Medido** (11.3 e 12.3): documentos legítimos, 1,0 vez; bombas de descompressão, no máximo 170,9 vezes (cerca de 5,8 vezes de margem para os 1.000); D1, cerca de 1,6 bilhão de vezes; D1b, cerca de 13,9 milhões de vezes.
  - **A mensagem diz:** o tamanho do arquivo e o pedido; que um documento real não faz isso; que o arquivo está danificado ou foi feito para atacar; e que aumentar o limite **provavelmente não resolve**.
- **Pânico do leitor** (D4). O processo verificador instala o seu próprio tratamento de pânico, que escreve uma linha própria (por exemplo, `DAMAGED`) antes de o processo terminar, como já faz com `TOO_DEEP`. Isso funciona também quando o processo aborta depois, porque o tratamento roda no primeiro pânico.
  - **A recusa nova, "danificado",** diz que o arquivo está danificado ou foi feito para atacar, e que a biblioteca de documento não conseguiu lê-lo.
  - **Ela não fala de memória** e não sugere aumentar limites.
  - **Nunca mostra** a nota do Rust nem linhas de pilha.
- **Testes:**
  - D1 e D1b recebem "pedido absurdo";
  - D2 (histórico e estado) recebe "danificado";
  - as quatro bombas e o documento legítimo acima do limite continuam com a mensagem atual;
  - o teste que trava o texto do Rust continua.

**3. ADR-0024** (Proposto; editável com a aprovação do dono). Textos propostos:
- **Seção 2:** a regra do pedido absurdo (acima do limite e acima de 1.000 vezes os dados), com as medidas e a margem; a mensagem; e a recusa "danificado".
- **Seção 3, a lista de mensagens:** "pedido absurdo" e "danificado".
- **Seção 4, tabela de estresse:** D1 (1,6 TiB), D1b (12 GiB) e D2 (foto com bloco quebrado, histórico e estado), cada um recusado com a mensagem certa, com o programa vivo.
- **Seção 5:** a pasta `verifier-refused/` e a regra de passagem para `regressions/`; o modo `-fork` só se o dono aprovar 12.5.
- **Consequências, "Pendente":**
  - os defeitos D1, D1b e D2 da Loro e o relato à Loro;
  - **a condição de bloqueio do P2P** (texto abaixo).

**4. ADR-0014** (Proposto). Na seção "Consequências", a mesma condição de bloqueio.

**Texto proposto para a condição de bloqueio** (ADR-0024 e ADR-0014):

> **O P2P não abre enquanto as quedas da Loro com dados não confiáveis (D1, D1b e D2, relatório `docs/auditoria/2026-11-04-fuzz-records.md`, seção 12) não forem corrigidas ou isoladas.**
> - **Corrigida:** uma versão da Loro, aprovada e atualizada, deixa de cair com todas as entradas de `fuzz/verifier-refused/`, conferido cada uma num processo separado.
> - **Isolada:** nenhum dado vindo da rede chega à Loro do programa principal sem passar antes por um processo separado, com teto de memória e de tempo, que o decodifique por inteiro. Uma queda desse processo recusa o dado e nunca derruba o programa. O custo desse processo para os pedaços contínuos do P2P precisa ser medido e aprovado (a pendência que já existe).
> - **Não basta:** `catch_unwind` (não pega uma falta de memória, e o documento fica envenenado depois de um pânico, testado), nem conferir o selo (qualquer pessoa o recalcula), nem recusar fotos vindas da rede (fecha o D2, mas não o D1 nem o D1b).

**5. O relato à Loro:** o rascunho foi atualizado com D1, D1b e D2 (veja 12.7). O envio é do dono, depois de pedir um canal privado.

**6. O `-fork` no `main`:** só com a aprovação do dono, depois dos itens 1 a 3, como em 12.5.

**Fica de fora até nova decisão:** qualquer correção na Loro (`[patch]` ou cópia), a atualização da Loro e o desenho do processo de isolamento do P2P.

### 12.7 Rascunho do relato à Loro

- **Arquivos:** `A:\Projetos\bivar-diag\loro-report\`, fora do repositório; nada foi publicado.
- **O que contém:** D1, D1b e D2, cada um com:
  - a entrada mínima: 326, 190 e 256 bytes;
  - a reprodução de 6 linhas;
  - as versões testadas: 1.17.1, `ea1d383e` e `c55cebff`;
  - a pilha;
  - a causa provável e as correções possíveis.
- **Conferido:** cada reprodução do texto foi rodada exatamente como está, 3 de 3.
- **A versão anterior** está em `loro-report-draft-2026-11-04.md`, e o `LEIA-ME.txt` foi atualizado.

### 12.8 Como foi feito (nada ficou no `main`)

- **Ramo temporário `diag/fuzz-censo`**, a partir de `diag/fuzz-fork`, numa pasta de trabalho temporária, com:
  - um workflow e um script de censo;
  - um script em Python que reexecuta e agrupa as entradas;
  - o gancho, que passou a imprimir a pilha só na reexecução.
- **Reprodução com a Loro pura:** `A:\Projetos\bivar-diag\loro-panico\`, com um programa novo e três versões da Loro. O `Cargo.lock` foi copiado das reproduções anteriores, e a pasta `vmain-c55cebf` baixou o commit da Loro de 2026-11-10.
- **Testes temporários** numa cópia do `main` (`git archive`), fora do repositório:
  - `zz_panic.rs`: fotos com bloco quebrado e entradas brutas, pelo verificador e direto no processo;
  - `zz_oom_stack.rs`: *nightly*, com o teto de 4 GiB e o gancho de falha de alocação.
- **Nenhuma dependência nova e nenhum `unsafe`.** A Loro do Bivar não foi atualizada, e nada foi publicado fora do ramo temporário.
- **Limitação:** o artefato da execução `41913641114` não estava na pasta informada. A entrada real do pânico segue sem análise (analisada depois, na seção 13.1).

---

## 13. Quarta rodada: a entrada real do pânico, o plano 12.6 aplicado e o `-fork` no `main` (2026-11-10)

**Decisões do dono:**
- o plano 12.6 está aprovado. As três entradas da Loro vão para a pasta própria, testadas pelo verificador, e a mensagem de pânico não fala de memória;
- o `-fork` vai para o `fuzz.yml` do `main`, depois dos itens 1 a 3 do plano, com estas condições:
  - só `-ignore_ooms=1`;
  - `timeout-minutes: 40`;
  - no máximo uma entrada por família;
  - as faltas de memória contadas no registro.

  Antes, ele precisa ser provado num ramo temporário com uma queda artificial;
- **nenhum relato nem contato com a Loro:** o projeto é 100% independente. O conteúdo do rascunho fica só como registro interno, na seção 13.5. **Isto substitui** o item 5 de 12.6, a seção 12.7 e as decisões 3 da seção 11 e 4 de 12.

### 13.1 A entrada real do pânico (`crash-0c03624454b91bd7277d8f01e2c3173ac606e1b9`)

- **De onde veio:** artefato `diag-real-30min` da execução `41913641114`, em `A:\Projetos\bivar-diag\fuzz-panico\diag-real-30min.zip`. A pasta do dono foi só lida, e o zip foi extraído numa pasta temporária.
- **Integridade:** 886 bytes; o SHA-1 é `0c03624454b91bd7277d8f01e2c3173ac606e1b9`, igual ao nome.
- **Como o libFuzzer achou:** aos 231 s, por mutação (`ChangeASCIIInt` e `CMP`) de uma entrada da coleção (`base unit 66a68e48…`).
- **Registro da CI:** `panicked at …/loro-kv-store-1.17.0/src/block.rs:305:14: validated SSTable block should decode: DecodeError("Invalid bytes")`.

**Cortada como o alvo corta, ela vira cinco registros:**

| Registro | Tipo | Bytes | O que é |
|---|---|---|---|
| 1 | retrato | 367 | **idêntico** à foto original da coleção (`cmp` sem diferença) |
| 2 | marca de gravação | 8 | — |
| 3 | **atualizações** | 367 | um blob da Loro no **modo retrato** (`FastSnapshot`, modo 3), com 6 bytes diferentes da foto. Só o bloco do histórico está malformado; o selo do blob está errado |
| 4 | marca de gravação | 8 | — |
| 5 | atualizações | 121 | um blob de atualizações (`FastUpdates`, modo 4), também com selo errado |

**Reprodução só com a Loro** (`A:\Projetos\bivar-diag\loro-panico\`, 3 de 3 em cada versão: 1.17.1, `main` `ea1d383e` e `main` `c55cebff`):

| Rota | Resultado |
|---|---|
| a rota do Bivar: `from_snapshot`(registro 1) + `import_batch`(registros 3 e 5) | pânico em `block.rs:305`, mesma mensagem da CI (código 101) |
| documento já aberto + `import`(registro 3 com o selo recalculado) | pânico em `block.rs:305`, depois aborta |
| documento novo + `import`(registro 3 com o selo recalculado) | pânico, depois aborta |
| documento já aberto + `import`(registro 5 com o selo recalculado) | recusado com erro, sem pânico |
| qualquer `import` dos registros 3 ou 5 com o selo original | recusado (`DecodeChecksumMismatchError`) |

**Respostas:**
- **É o mesmo pânico da seção 12?** **Sim.** São a mesma linha (`block.rs:305:14`) e a mesma mensagem (`DecodeError("Invalid bytes")`), e o mecanismo é o mesmo: um bloco SSTable malformado, decodificado tarde. A diferença é só o lugar do bloco: aqui o retrato malformado está num **registro de atualizações**, e não no registro de retrato.
- **Chega pela rota de retratos, a mesma de um retrato mandado por um colega no P2P?** **Sim.** O registro 3 é um blob no modo retrato, entregue a um documento **já aberto**:
  - o Bivar abre o registro 1 e depois importa o 3 com `import_batch`;
  - na Loro, um retrato importado num documento que já tem histórico vai para `decode_snapshot_for_updates`, que percorre os blocos do histórico. É a mesma rota de um colega que mande um retrato em vez de atualizações, no P2P (seção 12.2).
  - No `import_batch`, o selo nem é conferido (seção 11.1). Por isso, a entrada original, com o selo errado, chegou lá. Com `import` comum, um selo recalculado basta.
- **No verificador** (depois da mudança 13.2): recusada como **danificada**, sem falar de memória, e o programa segue vivo (teste da seção 13.2).

### 13.2 O plano 12.6, itens 1 a 3, aplicado

**Entradas** (`fuzz/verifier-refused/`):

| Arquivo | Defeito | Recusa esperada |
|---|---|---|
| `bivar_records/crash-c6869029ad0329fc11298f77b7f2ea0fe497bf61` (1.091 bytes) | D1: pedido de 1.747.498.019.256 bytes | pedido absurdo |
| `bivar_records/oom-d8f611603e5679d75944b87fe7239a356336f783` (955 bytes) | D1b: pedido de 12.884.901.884 bytes | pedido absurdo |
| `bivar_records/crash-0c03624454b91bd7277d8f01e2c3173ac606e1b9` (886 bytes) | D2: pânico em `block.rs:305` | danificado |

- **A lista** `fuzz/verifier-refused/bivar_records.txt` diz, para cada arquivo, a recusa esperada e o tamanho do pedido.
- **`.gitattributes`:** as entradas são marcadas como bytes, para nunca terem as quebras de linha convertidas.
- **As entradas de D2 construídas à mão** (seção 12.2) **não entraram**: a entrada real substitui as duas.

**Código** (só o protótipo e `fuzz/`; nenhuma dependência nova, nenhum `unsafe`):
- **`arquivo`:** `file_summary` devolve a impressão digital e os **dados do documento** (bytes dos registros de retrato e de atualizações; os arquivos grandes não contam), lidos sem a Loro. `fingerprint_file` passa a usá-la.
- **`verificador`, biblioteca:**
  - duas recusas novas, `AbsurdRequest { asked, doc_bytes, limit }` e `Damaged { detail }`;
  - a regra `is_absurd_request` (acima do limite **e** acima de `ABSURD_REQUEST_FACTOR` = 1.000 vezes os dados, sem estouro de conta);
  - a leitura do tamanho do pedido que falhou;
  - a resposta `DAMAGED` é conferida **antes** do texto de memória;
  - `fuzz_records_file` monta o `.bivar` exatamente como o alvo `bivar_records` monta.
- **`verificador`, processo:** no primeiro pânico de um leitor, ele escreve `DAMAGED <onde e por quê>` e termina (código 5). Isso acontece antes do segundo pânico da Loro, ao fechar o documento, que abortaria o processo sem texto útil.

**As mensagens, como saem hoje** (copiadas do teste):
- **D1:** "This file was refused: it holds 1.0 KiB of document data, but reading it asks for 1.6 TiB of memory in a single request. A real document does not do that: the file is damaged, or it was made to attack. Raising the memory limit (4 GiB) will probably not help."
- **D1b:** a mesma, com "924 bytes" e "12.0 GiB".
- **D2:** "This file was refused: it is damaged, or it was made to attack. Its contents could not be read." Ela não fala de memória, de limites, de pânico nem de código de saída. O lugar do pânico (`block.rs:305:14`) fica só no campo `detail`, para um registro.

**Testes novos:**
- `verificador/tests/entrada.rs`, `inputs_the_verifier_must_refuse_are_refused_and_the_program_survives`. Para cada entrada da lista, o teste:
  - abre o `.bivar` como arquivo de fora, com o verificador de verdade e o limite normal de 4 GiB;
  - confere a recusa da lista; o **tamanho exato do pedido** (1.747.498.019.256 e 12.884.901.884); que o pedido passa do limite e de 1.000 vezes os dados; e o texto: "probably not help", nunca "can be raised"; para o pânico, sem "memory", "limit", "panicked", "RUST_BACKTRACE" nem "exit code";
  - confere que o arquivo recusado não é anotado como aprovado;
  - no fim, abre um arquivo bom, para provar que o programa continua vivo e verificando;
  - confere também que a lista e a pasta citam os mesmos arquivos, e que os dois tipos de recusa estão cobertos.
- `verificador/src/lib.rs`, testes de unidade:
  - **a regra:** os casos medidos (D1, D1b, documento legítimo, bomba), os dois lados de cada condição e os extremos sem estouro;
  - **a leitura do tamanho** no texto do Rust;
  - **o texto dos tamanhos;**
  - **o que cada mensagem nova diz e não diz.**
- `arquivo/tests/disco.rs`, `the_summary_of_a_file_counts_only_its_document_data`: os dados do documento são exatamente retrato + atualizações; a marca de gravação e um arquivo grande de 3 MB não contam; e a impressão digital é a mesma de antes.
- **Os testes que já existiam continuam passando.** Entre eles:
  - as quatro bombas, com a mensagem do limite;
  - o documento legítimo acima do limite, que abre quando o limite é aumentado;
  - o teste que trava o texto do Rust.

**ADRs** (os dois estão "Proposto"; editados com a aprovação do dono):
- **ADR-0024:**
  - seção 2: pedido absurdo e arquivo danificado;
  - seção 3: as mensagens;
  - seção 4: as entradas da Loro na tabela de estresse;
  - seção 5: o modo `-fork` e a pasta `verifier-refused/`;
  - "Consequências": como as recusas são reconhecidas, a condição do P2P e a decisão de não contatar a Loro.
- **ADR-0014:** a condição do P2P, com o texto do dono: ele só abre quando uma versão nova da Loro, aprovada pelo dono, passar nas três entradas da pasta própria, ou quando os dados de colegas forem isolados num processo separado antes de chegar à Loro, num desenho feito antes do P2P.
- **`fuzz/README.md`:** o modo `-fork` e a pasta nova.

### 13.3 O `-fork` no `fuzz.yml`

**Arquivos:**
- **`fuzz/run-fork.sh`:** roda um alvo com `-fork=2 -ignore_ooms=1 -ignore_timeouts=0 -ignore_crashes=0 -rss_limit_mb=4096 -timeout=60 -max_len=1048576`.
  - Cada linha do registro ganha o segundo em que foi escrita.
  - **O job fica verde só se:** o libFuzzer terminou com 0, ou com 71 (falta de memória ignorada) depois de o tempo acabar; **e** nenhuma entrada de queda ou de tempo esgotado foi escrita. Esta segunda conferência pega uma queda num processo filho que ainda rodava quando o tempo acabou: o modo `-fork` não olha esses filhos.
  - **No fim, escreve um resumo** no registro e na página da execução: tempo pedido e real; segundos entre o fim do tempo e a saída; processos filhos; faltas de memória ignoradas; os três contadores; arquivos por tipo; famílias; entradas guardadas.
- **`fuzz/fork-families.py`** (Python da máquina da CI, só biblioteca padrão; nenhuma dependência no projeto):
  - executa de novo cada entrada guardada e a agrupa pelo lugar onde falhou: o local do pânico, ou os dois primeiros quadros da pilha que são do nosso código ou de uma biblioteca;
  - guarda a menor entrada de cada família, com o texto da execução, e descarta as outras;
  - executa de novo no máximo 300 entradas, por no máximo 150 s.
- **`fuzz/fuzz_targets/oom_hook.rs`:** o gancho de falha de alocação do ramo de diagnóstico, agora nos dois alvos, sem a parte de pilha que servia só ao censo.
- **`.github/workflows/fuzz.yml`:**
  - `timeout-minutes: 40`;
  - um passo de compilação separado;
  - **1.500 s pedidos na rodada diária e 120 s em cada envio.** Um filho roda no máximo 300 s depois do fim do tempo, então o total diário fica abaixo de cerca de 1.800 s. A preparação levou cerca de 4 minutos na execução `41876124671`;
  - o artefato `fuzz-families-<alvo>` sobe sempre: uma entrada por família, mais o registro.

**Prova no ramo temporário `diag/fuzz-fork-prova`:** resultados na seção 13.4.

### 13.4 Prova do `-fork` com falhas artificiais

**Como:**
- **Ramo temporário:** `diag/fuzz-fork-prova`, nunca juntado.
- **O primeiro commit** (`338dd91`) é exatamente o que vai ao `main` (sem o relatório).
- **Depois vem um commit** (`a779878`) que acrescenta duas falhas artificiais aos alvos:
  - `bivar_records` entra em **pânico** quando a entrada começa com `!`;
  - `bivar_container` pede **2 TiB** de uma vez quando a entrada começa com `?` (acima do 1 TiB que o AddressSanitizer atende, então passa pelo gancho).
- **O `fuzz.yml` normal rodou a cada envio,** 120 s por alvo. Os resultados vêm do resumo que `run-fork.sh` escreve na página da execução.

| Execução | Alvo | Falha artificial | Job | Resumo |
|---|---|---|---|---|
| `42867243173` (`a779878`) | `bivar_records` | pânico | **vermelho** | parou aos 25 s; 7 processos filhos; 1 entrada de queda; 1 família; 1 entrada guardada |
| `42867243173` (`a779878`) | `bivar_container` | pedido de 2 TiB | **verde** | 128 s para 120 s pedidos; 20 filhos; **3 faltas de memória ignoradas e contadas** (`oom/timeout/crash: 3/0/0`); 3 arquivos `oom-`; **1 família, 1 entrada guardada** |
| `42868487348` (`33bfd35`, com o resumo corrigido) | `bivar_records` | pânico | **vermelho** | parou aos 11 s; família `panic at fuzz_targets/bivar_records.rs:20: artificial panic (proof of the fuzzing job)` |
| `42868487348` (`33bfd35`) | `bivar_container` | pedido de 2 TiB | **verde** | 124 s para 120 s pedidos; 2 faltas de memória ignoradas e contadas; família `oom-request-failed in …/fuzz_targets/bivar_container.rs:20`; 1 entrada guardada |

**Defeitos do script achados pela prova e corrigidos antes do `main`:**
- **Seção do tempo:** quando a rodada parava antes do fim do tempo, o resumo mostrava um número sem sentido (`-1794332742`). Agora mostra "none".
- **Quadros da pilha:** os arquivos dos alvos (`fuzz_targets/`) eram descartados, e o resumo não listava as famílias. Agora os quadros entram e o resumo lista cada família.
- **Caminhos:** os caminhos das famílias começam na raiz do repositório (`61e6277`).

**Provado:**
- **Uma queda deixa o job vermelho** e guarda uma entrada só.
- **Uma falta de memória acima de 1 TiB não deixa vermelho:** ela é contada no registro e no resumo, e as entradas repetidas da mesma família viram uma só.
- **A duração ficou perto do pedido:** 124 a 128 s para 120 s.

**Não provado aqui:**
- **Tempo esgotado:** a prova da seção 11.4 mostrou o job vermelho com `-ignore_timeouts=0` (código 70), com o mesmo libFuzzer.
- **A rodada diária de 1.500 s:** ela só roda no horário agendado. A espera pelos filhos no fim foi medida no censo (14 s depois de 1.800 s).

### 13.5 Registro interno: os defeitos da Loro (sem contato com a Loro)

**Por decisão do dono (2026-11-10), não haverá relato nem contato com a Loro:** o projeto é 100% independente. O rascunho de `A:\Projetos\bivar-diag\loro-report\` fica guardado só aqui, como registro interno. Nada foi publicado, e a pasta fora do repositório não foi apagada.

| Defeito | Entrada mínima (só com a Loro) | SHA-256 | Onde | O que acontece |
|---|---|---|---|---|
| D1 | `loro-import-alloc.bin`, 326 bytes, atualizações | `9b51e0c92b1c88cca70ee9e10046807833f0f6d482420948b9318428592ff833` | `loro-internal` `block_meta_encode.rs:162` → `serde_columnar` `rle.rs:233` | `LoroDoc::new().import` pede 1.747.498.019.256 bytes e o processo aborta |
| D1b | `loro-import-alloc-12gib.bin`, 190 bytes, atualizações | `5cb8a111d9bb4cae98d51124ac4d38a3c8f5807a2477be4435e039b62f7bd2a0` | `loro-internal` `block_meta_encode.rs:132` (`Vec::with_capacity(n_changes)`) | pede 12.884.901.884 bytes; aborta, ou só gasta memória numa máquina grande |
| D2 | `loro-sstable-panic.bin`, 256 bytes, retrato | `98dcbc3b25b3429dfeba4312d792b98acd95894f178f68944f7c0cc25d54818c` | `loro-kv-store` `block.rs:305` (`expect`) | pânico; a trava do documento fica envenenada; fechar o documento gera outro pânico; o processo aborta (`0xC0000409`) |

**Versões testadas:** Loro 1.17.1, `main` `ea1d383e` (2026-11-03) e `main` `c55cebff` (2026-11-10). Todas caem.

**A reprodução, igual para as três** (o nome do arquivo muda):

```rust
fn main() {
    let bytes = include_bytes!("../loro-import-alloc.bin");
    let doc = loro::LoroDoc::new();
    println!("{:?}", doc.import(bytes).map(|_| ()));
}
```

**As entradas mínimas em base64** (para refazer a prova sem a pasta de fora):

- **D1:** `bG9ybwAAAAAAAAAAAAAAAOr+fh8ABLYA/wMAAP8ACyMBbwFsAABvbwBVcgAAAAAAAAAAd3yF/v/8Sf/6AExPUk8AAAIAAgEQAQEAAAAAAAAAAQEAAAAAAAUBAAABAAYBAAAAAAAAAP//////////sf///////////////////////////////////z8/Pz8/////////////////////////////////////AAAAAAAAAAAAAAAAAAAAAAAAAAAAAACAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAGtvcm8/Pz8/Pz8/Pz8/Pz8/Pz8/AAAAAAEABgCDBHRyZWVAAAAAAwEABAIAAAUAAAAAAQAFAQIAb2NfaWQGc2NoZW1hBG1ldGEADwAAAHeEegEAAQQCBAADA7YDAAA=`
- **D1b:** `bG9ybwAAAAAAAAAAAAAAAB6f28UABLYA/wADAAD/////CyMDAAAA2EKv7a9Py9f//wAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAE9STwAAAgACAQD/CRABAdPT09PT09PT09PT09PT09MAAwMAAA==`
- **D2:** `bG9ybwAAAAAAAAAAAAAAABCMSqUAA4oAAABMT1JPAAALAAsBEAECAAAAAAAAAAEBAAAAAAAFAQAAAQAGAQQBAgAABANvd24ADgEEAgEAAgEAAgEFAgELAAwLb3duIGhpc3RvcnkAAgBmcgECFAACAHZ2AQIWAABGAE4AAABXKS6qAQAAAAUAAAAMAAAAAAAAAAACAAAAAAACAHZ2Cj3IBGcAAABUAAAATE9STwACAQALb3duIGhpc3RvcnkBAgAAAAAAAAADBAIBAAIBAAIBAAIBFgAAAAABAGaIEU8BAAAABQAAAAUAggNvd24ABQCCA293bvGwcN41AAAAAAAAAA==`

**Causas, como lidas no código da Loro:**
- **D1 e D1b:** `decode_changes_header` reserva memória por números lidos do bloco (a soma das dependências na linha 159, e o número de mudanças na linha 132), **antes** de conferir que os bytes existem. Há outras reservas do mesmo tipo na mesma função, não testadas uma a uma, e a soma da linha 159 pode dar a volta numa compilação otimizada.
- **D2:** `MemKvStore::import_all` chama `SsTable::import_all(bytes, false)`, que confere os selos dos blocos, mas não os decodifica. O bloco é decodificado só no primeiro uso, e ali qualquer erro vira pânico (`expect`). Os selos (xxh32 com a semente `"LORO"`) qualquer pessoa recalcula.
- **Observação:** `import_batch` com dois ou mais blobs lê o conteúdo sem conferir o selo (`decode_import_blob_meta(b, false)`).

**Consequência para o Bivar:** só a condição do P2P (ADR-0024 e ADR-0014), as entradas de `fuzz/verifier-refused/` e a regra de passagem para `fuzz/regressions/`. A atualização da Loro, quando houver uma versão nova, segue as regras normais: aprovação do dono, `cargo deny` e `cargo audit`.

### 13.6 Checagens e commit

**Seção 6 do CLAUDE.md, neste PC (Windows):**

| Onde | `cargo fmt --check` | `cargo clippy --all-targets -- -D warnings` | `cargo test` |
|---|---|---|---|
| raiz do repositório | passa | passa | passa (os crates da raiz ainda não têm testes) |
| protótipo (`--release`, como a CI) | passa | passa | **138 passaram, 0 falharam**, 26 ignorados (medições) |

- **Falha do ambiente durante as checagens:** o primeiro `clippy` do protótipo falhou ao ler a biblioteca padrão do Rust ("only metadata stub found", "invalid metadata"), com erros que mudavam a cada tentativa. **A causa era o Windows sem memória:** só 0,1 GB livres do limite de 30,7 GB, com "o arquivo de paginação é muito pequeno". Nenhum processo do projeto estava rodando. Com a memória de volta (8,3 GB livres), o mesmo `clippy` passou sem aviso.

**No Linux (CI do ramo de prova):**
- **Execução `42869999093`,** workflow temporário com os mesmos testes do protótipo: **138 passaram, 0 falharam**, nenhum pânico. Isso inclui os testes novos: as três entradas de `fuzz/verifier-refused/`, os dados do documento e as quatro regras do verificador.
- **O job `poc-crdt` do `CI` falhou nos dois envios de prova** (`42867243176` e `42868487411`), sem que se visse qual teste. O motivo não aparece sem login.
  - **O mesmo job falha no próprio `main`** (`41882245363`, commit `1aa6ee8`, e `41874979352`) e no ramo do censo, que não mudou o protótipo.
  - É a falha intermitente da trava de arquivo no Linux, ainda em diagnóstico (`2026-11-04-falha-ci-trava.md`).
  - **Não é causada por esta mudança:** os mesmos testes passaram inteiros na execução `42869999093`.
  - **Continua pendente:** o `main` segue com essa falha intermitente.

**Dependências:** nenhuma nova, e nenhum `Cargo.lock` mudou. O Python do agrupador é o da máquina da CI, só com a biblioteca padrão.

**Commit no `main`:**
- leva o código, os testes, as entradas, o `fuzz.yml`, os ADRs, o `fuzz/README.md` e este relatório;
- **sem envio ao GitHub:** o dono decide quando enviar.

**Ramos temporários** (nunca juntados), deixados no GitHub para consulta:
- `diag/fuzz-fork`;
- `diag/fuzz-censo`;
- `diag/fuzz-fork-prova`.

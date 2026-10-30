# Ficha das bibliotecas CRDT: entrega 0.6, Parte 1

Coletada em 2026-10-29. Esta ficha serve para o dono aprovar, ou não, o uso das bibliotecas **na prova de conceito descartável**. Ela não aprova o uso no produto: essa decisão vem no ADR do CRDT, depois da comparação.

## Ambiente de medição

- **Máquina de referência:** Windows 11, Ryzen 7 5700X, 16 GB, GTX 1660. O dono confirmou que é este PC.
- **Discos**, conferidos com `Get-PhysicalDisk` e `Get-Partition` (só leitura):

| Unidade | Disco físico | Tipo | Ligação |
| --- | --- | --- | --- |
| A: (o projeto) | Seagate ST1000DM010, 932 GB | HD mecânico | SATA |
| C: | Lexar 128 GB SSD | SSD | SATA |

Como o A: é um HD, os testes de disco da Parte 4 rodam nos dois discos, e os dois resultados são registrados (pedido do dono).

## Como os números foram obtidos

1. **Download:** `cargo vendor` baixou o código-fonte de cada biblioteca, com todas as dependências, numa pasta isolada fora do projeto. Esse comando só baixa e descompacta, sem compilar nem rodar nada.
2. **Pacotes considerados:** só os que de fato entram numa compilação para Windows (`x86_64-pc-windows-msvc`) ou Linux (`x86_64-unknown-linux-gnu`). Inclui as ferramentas que rodam na compilação (*build* e *proc-macro*). Saíram os pacotes que o `cargo vendor` baixa para outras plataformas.
3. **Contagem:** o script [`inventario.sh`](inventario.sh) conta, por pacote:
   - as linhas com a palavra `unsafe` nos arquivos `.rs`, fora de testes, exemplos e comentários de linha inteira (busca de texto, como o dono aprovou);
   - os arquivos `.c`, `.cc`, `.cpp`, `.h`, `.hpp`, `.S`, `.s` e `.asm`;
   - se o pacote tem `build.rs`;
   - se ele depende de `cc`, `cmake`, `bindgen`, `pkg-config`, `vcpkg` ou `cxx-build` para compilar.
4. **Conferências:** `cargo audit` (alertas de segurança do RustSec) e `cargo deny` com o `deny.toml` do projeto (licenças).

As tabelas completas estão em [`inventario-loro.tsv`](inventario-loro.tsv) e [`inventario-automerge.tsv`](inventario-automerge.tsv).

**Limites desta contagem:**

- uma linha com `unsafe` pode ser uma declaração ou um bloco inteiro. O número indica volume, não risco;
- a busca de texto não lê o código, então não diz se o `unsafe` está correto.

## Resumo

| | **Loro** | **Automerge** |
| --- | --- | --- |
| Versão proposta (exata) | `=1.17.1` (de 2026-10-22) | `=0.12.0` (de 2026-09-16) |
| Licença da biblioteca | MIT | MIT |
| Licenças das dependências no `deny.toml` | **falha:** `xxhash-rust` usa **BSL-1.0**, que não está na lista | passa |
| `cargo audit` | **6 alertas**: 2 de memória insegura, 4 de pacote abandonado (detalhes abaixo) | nenhum |
| Pacotes compilados (Windows + Linux) | 122 (92 no programa final, no Windows) | 41 |
| Linhas de Rust nesses pacotes | ~1,23 milhão | ~505 mil |
| Linhas com `unsafe`, total | 17.628 (12.530 no `windows-sys`, que são declarações geradas da API do Windows) | 1.898 |
| `unsafe` na própria biblioteca | 49, espalhadas pelos pacotes `loro-*` e `append-only-bytes` | 1 (`hexane`) |
| Pacotes com `unsafe` | 76 | 26 |
| Arquivos C/assembly | 28: `psm` (24 de assembly), `stacker` (2) e `cc` (2) | 0 |
| Pacotes com `build.rs` | 22 | 7 |
| Ferramenta que compila C | sim: `psm` e `stacker` usam o `cc` (veja abaixo) | não |
| Pessoas que contribuíram | 44 | 62 |
| Quanto pesa a pessoa principal | 75% dos commits numa só pessoa (zxch3n), 16% na segunda | 36% e 31% nas duas principais |
| Versões nos últimos 12 meses | 84 | 19 |
| Último commit | 2026-10-23 | 2026-10-23 |
| Alertas no RustSec para a biblioteca em si | nenhum | nenhum |
| Árvore com movimentação | **sim** (`LoroTree::mov`, `mov_after`, `mov_before`) e lista com movimentação (`LoroMovableList::mov`) | **não**. O movimento precisa ser feito por nós, e o custo é medido na Parte 2 |
| Detectar escrita simultânea no mesmo campo | **não pronto** (o mapa é "a última escrita vence" sem expor o valor que perdeu). Medido na Parte 2 | **sim, pronto:** `get_all` devolve todos os valores simultâneos |
| Desfazer só do próprio autor | **sim** (`UndoManager`) | **não pronto**. Precisa ser feito por nós, e o custo é medido na Parte 2 |
| Cópia sem histórico | candidatos: `ExportMode` com instantâneo "raso" (*shallow snapshot*), a conferir na Parte 2 | a conferir na Parte 2 |

## Os problemas da Loro, em detalhe

### 1. Licença BSL-1.0 (`xxhash-rust`)

- **O que é:** a Boost Software License 1.0. A ferramenta `cargo deny` a classifica como aprovada pela OSI e como licença livre pela FSF. A FSF a lista como compatível com a GPL.
- **O problema:** ela não está na lista de licenças do `CLAUDE.md` §5 nem do `deny.toml`.
- **Na prova:** a licença entra só no `deny.toml` próprio da prova, como já foi feito na prova de texto. A lista do projeto não muda.
- **No produto:** se a Loro vencer, adicionar a BSL-1.0 exige mudar o `CLAUDE.md` e o `deny.toml` juntos, com aprovação do dono.

### 2. Memória insegura em dependências abandonadas: `im` e `sized-chunks`

A Loro usa o pacote `im`, que usa o `sized-chunks`. Os dois foram arquivados pelos autores em 2026-05-03 e nunca terão correção.

- **RUSTSEC-2023-0126 (`im` 15.1.0):** inserir num `im::OrdSet` viola as regras de memória do Rust.
  - **Avaliação:** pela busca no código, a Loro usa só `im::HashMap` e `im::HashSet`, nunca o `OrdSet`. Então este alerta provavelmente não a atinge. É uma **suspeita fundamentada**, não uma prova.
- **RUSTSEC-2026-0255 (`sized-chunks` 0.6.5):** se um item guardado falhar ao ser destruído (um *panic* no `Drop`), a memória pode ser liberada duas vezes.
  - **Avaliação:** **não verifiquei** se o caminho que a Loro usa passa pelos métodos afetados.
- **A correção está em andamento:** o pedido de mudança [loro-dev/loro#1122](https://github.com/loro-dev/loro/pull/1122), aberto em 2026-09-27, troca o `im` pelo `imbl`, a continuação mantida. Ainda não foi aceito, e **por isso a versão 1.17.1, aprovada para a prova, ainda usa o `im` e o `sized-chunks`**, com os dois alertas acima.
- **Outros dois abandonados:** o `bitmaps` (dependência do `im`) e o `atomic-polyfill`, puxado pelo `heapless` 0.7 através do `postcard`. Para o segundo existe o relato de problema (*issue*) [#818](https://github.com/loro-dev/loro/issues/818), aberto desde 2025-09.

#### Verificação de 2026-10-29: a Loro 1.17.1 não usa as partes com defeito

Feita por leitura do código-fonte baixado (condição 3 do dono, opção "a Loro não usa as partes com defeito"):

- **Onde a Loro usa o `im`:** fora dos testes, só em `loro-internal/src/version.rs:163`, `ImVersionVector(im::HashMap<PeerID, Counter, …>)`. Os tipos guardados são `u64` e `i32`. Nenhum outro pacote da Loro (`loro`, `loro-common`, `loro-kv-store`, `serde_columnar` etc.) usa o `im` ou o `sized-chunks`. O `cargo tree -i` confirma que só a `loro-internal` puxa o `im`, e só o `im` puxa o `sized-chunks`.
- **RUSTSEC-2023-0126:** o defeito está no `im::OrdSet`, pelo `sized_chunks::Chunk::force_copy`. A Loro não usa o `OrdSet`, e o `force_copy` não aparece em nenhum lugar do código de tabelas do `im` (`src/hash/`, `src/nodes/hamt.rs`).
- **RUSTSEC-2026-0255:** as funções afetadas são `Chunk::clear`, `drop_left` e `drop_right`, `InlineArray::clear` e `RingBuffer::*`. O `im::HashMap` (`nodes/hamt.rs`) usa só o `SparseChunk`, que não está na lista, e guarda as colisões num `Vec` comum. Além disso, o defeito exige um item cujo `Drop` entre em pânico, e `u64`/`i32` não têm `Drop`.
- **A correção [#1122](https://github.com/loro-dev/loro/pull/1122)** continua aberta e sem aceite (última atualização em 2026-10-15). A versão mais nova da Loro continua sendo a 1.17.1.
- **A troca por `[patch]` não serve:** as continuações mantidas (`imbl` e `imbl-sized-chunks`) têm outro nome e outra numeração de versão (o `im` está na 15.x, e a dependência pede `^15.1`), então o Cargo não aceita a troca direta. Seria preciso manter uma cópia própria do `im`.
- **Limite desta verificação:** ela vale **só para a 1.17.1**, e é leitura de código, não prova automática. A cada atualização da Loro, ela é refeita antes de aceitar a versão nova (ADR-0021).

### 3. Assembly compilado dentro do programa (`psm` e `stacker`)

- **O caminho:** `loro-internal` → `pest` (um leitor de expressões de busca, o *jsonpath*) → `pest_stack_guard` → `stacker` → `psm`.
- **O que entra:** o `psm` traz arquivos em assembly (no Windows, `x86_64_msvc.asm`) que o `cc` monta durante a compilação. Esse código vai para dentro do programa final.
- **Para que serve:** aumentar a pilha de memória quando uma expressão é muito aninhada. Não é um leitor de arquivos.
- **Não dá para desligar:** o `pest` é dependência fixa da `loro-internal`, e a proteção de pilha vem ligada por padrão.
- **O mesmo pacote no projeto:** o `cc` já aparece nas provas de interface e de desenho (`poc/ui-egui` e `poc/render-vello`).
- **Um detalhe:** o `psm` traz um arquivo-objeto já compilado (`wasm32.o`), usado só para WebAssembly. Ele não entra nas compilações para Windows ou Linux.

### 4. Ritmo de versões e concentração

- **Muitas versões:** 84 versões em 12 meses é um ritmo alto. Isso mostra atividade, mas também mais mudanças para acompanhar.
- **Versão muito nova:** a 1.17.1 saiu há 7 dias. Uma alternativa mais testada pela comunidade é a 1.16.2, de 2026-09-21.
- **Dependência de uma pessoa:** 75% dos commits são de uma pessoa só. Se ela parar, o projeto fica em risco.

## Os pontos fracos do Automerge

- **Sem árvore com movimentação:** para mover um objeto, precisamos de uma estrutura própria por cima. O critério da 0.6 já prevê isso: o ADR diz como o movimento é feito e mede o custo de detectar o movimento perdido.
- **Sem desfazer pronto:** o desfazer de um autor só precisa ser feito por nós.
- **Versão 0.x:** a API ainda pode mudar entre versões (0.7 → 0.12 em um ano).
- **Componente em alpha:** o formato interno passou a usar o `hexane` 1.0.0-alpha.5, uma versão de teste.

## Por que o Yrs fica fora

Conferido na documentação oficial do `yrs` 0.28.0 (docs.rs, 2026-10-29):

- **Nada para mover:** a API pública não tem nenhum item com "move" no nome. Os tipos `Array` e `XmlFragment` têm só inserir, remover e ler, e nenhum recurso opcional (`sync`, `weak` ou `small-client`) acrescenta movimentação.
- **O problema:** sem movimentação, mover vira "apagar aqui e inserir lá". Se duas cópias moverem o mesmo objeto ao mesmo tempo, ele aparece duas vezes, e o ADR-0019 (decisão 3) proíbe isso.
- **A comparação:** a Loro tem a movimentação pronta, e o Automerge está na comparação para medir o custo de fazê-la por fora. Incluir o Yrs só repetiria o caso do Automerge, com uma biblioteca de menos versões no último ano (3 contra 19).

## Recomendação

**Aprovar as duas, com versões exatas e só para a prova descartável.** Os problemas da Loro (BSL-1.0, `im`/`sized-chunks` e assembly do `psm`) não impedem medir. Mas eles entram como pontos contra ela na decisão final, e se ela vencer, cada um vira uma condição no ADR.

## Decisão do dono (2026-10-29)

- **Aprovadas só para a prova, com versão exata:** Loro `=1.17.1` e Automerge `=0.12.0`.
- **BSL-1.0:** a exceção vale **só no `deny.toml` da prova**. A lista do projeto (`CLAUDE.md` §5 e `deny.toml` da raiz) não muda.
- **Loro 1.17.1, e não 1.16.2:** fica a 1.17.1. A correção dos pacotes abandonados ([#1122](https://github.com/loro-dev/loro/pull/1122)) ainda não foi aceita, então ela **não** está nessa versão.
- **Pedido para a Parte 2:** para cada recurso que falta numa biblioteca e precisa ser feito por fora (mover com segurança e desfazer no Automerge; detectar escrita simultânea na Loro), fazer uma versão mínima e registrar a velocidade, a **dificuldade** e o **risco de erro**.

---

# Parte 4 e Parte 5: bibliotecas aprovadas depois

## `sha2` `=0.11.1` (Parte 4, aprovada em 2026-10-29)

Resumo SHA-256 dos registros e dos pedaços de arquivos grandes, e base da impressão digital do arquivo. Ficha em [`PARTE4-FORMATO.md`](../PARTE4-FORMATO.md), seção 5:

- MIT OR Apache-2.0;
- grupo RustCrypto;
- 9 pacotes e 110 linhas com `unsafe`, só nas versões aceleradas pelo processador;
- sem C.

## `cap` `=0.1.2` (Parte 5, aprovada em 2026-10-30, só no processo verificador)

| | |
| --- | --- |
| Para quê | limitar a memória do processo verificador (4 GiB) |
| Licença | MIT OR Apache-2.0 |
| **Manutenção** | **sem versão nova desde 2023-03** (condição do dono: registrado aqui). É uma biblioteca de 478 linhas que não precisou mudar, mas está parada. Saída, se um dia for preciso: escrever o contador nós mesmos (cerca de 20 linhas, com `unsafe` e a aprovação do dono) |
| Pacotes que entram | 1 |
| C | nenhum |
| Alertas RustSec | nenhum |
| Onde entra | só no programa verificador (`prototipo/verificador`), nunca no processo principal |

**Conferência das 12 linhas com `unsafe` (pedido do dono)**, por leitura de `src/lib.rs`:

- **7 linhas (253 a 360) só existem com o recurso `nightly` da biblioteca**, que o Bivar não liga. Elas implementam uma interface experimental do Rust (`Alloc`) e não são compiladas.
- **As 5 que entram na compilação** são a implementação de `GlobalAlloc`, a interface que o Rust usa para pedir e devolver memória:
  - `unsafe impl GlobalAlloc` (linha 179);
  - `alloc`, `dealloc`, `alloc_zeroed` e `realloc` (linhas 183 a 249).
- **Todas fazem só duas coisas:**
  1. descontam ou devolvem o tamanho num contador atômico (`remaining`);
  2. repassam o pedido ao gerenciador de memória do sistema (`System`), sem mexer em ponteiros.

  Quando o pedido passa da quota, devolvem "sem memória" (ponteiro nulo), e o Rust encerra o processo verificador.
- **Imprecisão encontrada, sem risco para a memória:** o desconto usa `fetch_sub` e confere o valor *anterior*. Quando um pedido estoura a quota, o contador "dá a volta" (fica com um número enorme) por um instante, até ser devolvido. Outra linha de execução pedindo memória exatamente nesse instante passaria. O limite pode ser ultrapassado por pouco numa corrida entre linhas de execução, mas a memória nunca é usada de forma errada. Para o verificador (uma linha de execução principal), o efeito é desprezível.
- **`realloc`** usa `Layout::from_size_align_unchecked`. O contrato do `GlobalAlloc::realloc` garante que o tamanho novo é válido para o alinhamento, então o uso está correto.

## `libfuzzer-sys` `=0.4.13` (Parte 5, aprovada em 2026-10-30, só em `fuzz/`)

| | |
| --- | --- |
| Para quê | o motor do fuzzing (libFuzzer) |
| Licença | (MIT OR Apache-2.0) AND **NCSA**. A NCSA foi liberada **só no `fuzz/deny.toml`** (aprovação do dono); a raiz não tem NCSA |
| Manutenção | grupo `rust-fuzz`; versão de 2026-06; 20 milhões de downloads recentes |
| Pacotes que entram | 8 |
| `unsafe` | 245 linhas (109 no `getrandom`, 55 no `find-msvc-tools`, 34 no `cc`, 32 no `jobserver`) |
| C / C++ | **o libFuzzer em C++: 31 arquivos `.cpp` e cabeçalhos, cerca de 13 mil linhas**, compilados pelo `cc` |
| Alertas RustSec | nenhum |
| Onde entra | **só na pasta `fuzz/`**, na compilação automática. Nunca no programa |

## `cargo-fuzz` `0.13.2` (Parte 5, aprovada em 2026-10-30)

A ferramenta de linha de comando que roda o fuzzing.

- MIT OR Apache-2.0;
- grupo `rust-fuzz`;
- é instalada só na compilação automática (`.github/workflows/fuzz.yml`), como o `cargo-deny`;
- não entra no código.

# Parte 6 e fechamento da 0.6 (2026-10-30)

## Nenhuma biblioteca nova na Parte 6

- **O sorteio da identidade da cópia e do autor dela usa só a biblioteca padrão do Rust** (`RandomState`, a semente que o sistema dá às tabelas de espalhamento). Serve para identificadores; não serve para criptografia, e não é usado para isso.
- **Versões travadas no protótipo, conferidas com `cargo tree`:** `loro` 1.17.1, `sha2` 0.11.1, `cap` 0.1.2 (só no verificador). O `im` 15.1.0 e o `sized-chunks` 0.6.5 continuam entrando pela Loro, como registrado na seção 2. São 136 pacotes ao todo no protótipo, contando os de compilação.
- **Em `fuzz/`:** `libfuzzer-sys` 0.4.13, com a *nightly* `nightly-2026-10-15` travada só ali.

## Checagens no fechamento

| Onde | `cargo deny check` | `cargo audit` |
|---|---|---|
| Raiz do projeto | passa | passa, sem alertas |
| Protótipo (`prototipo/`) | passa | passa, com os 6 alertas já registrados e liberados só nas provas: `im` (sem manutenção e RUSTSEC-2023-0126), `sized-chunks` (sem manutenção e RUSTSEC-2026-0255), `bitmaps` e `atomic-polyfill` (sem manutenção) |
| Comparação da Parte 2 | passa | - |
| `fuzz/` | passa (NCSA liberada só ali) | - |

## Fuzzing no GitHub

A primeira rodada acontece no primeiro envio ao GitHub. O resultado fica registrado no relatório do fechamento ([`RELATORIO-FECHAMENTO.md`](../RELATORIO-FECHAMENTO.md)).

# Entrega 0.6, Parte 5 (no papel): toda entrada externa, arquivos-bomba e fuzzing

Data: 2026-10-30. Análise antes de qualquer código, como nas outras partes. Trata o achado A7. **Nada aqui está decidido.**

## 1. O que conta como entrada externa

Qualquer dado que chega de fora do próprio programa é **não confiável**:

| Entrada | Hoje | Regra deste ADR |
| --- | --- | --- |
| Arquivo `.bivar` | existe (protótipo da Parte 4) | primeiro leitor a seguir estas regras; prova nesta parte |
| Dados recebidos por P2P (colegas) | depois da Fase 2 | as mesmas regras do `.bivar`: chegam como atualizações da Loro |
| Área de transferência, arrastar e soltar | futuro | o conteúdo é tratado como um arquivo do formato correspondente e passa pelo leitor dele |
| Fontes instaladas no sistema | futuro (a 0.5 usou só fontes embutidas) | leitor de fontes com limites e fuzzing próprios |
| Perfis de cor embutidos em imagens | futuro (entrega 0.8) | leitor de perfis com limites e fuzzing próprios |
| Imagens, SVG, PDF, áudio | futuro | um leitor por formato, cada um com os seus limites |

**Regras para todo leitor:**

1. **Só em Rust** (ADR-0011). A exceção é um processo isolado, com ADR próprio.
2. **Limites obrigatórios**, todos escritos no ADR antes de o leitor existir:
   - tamanho;
   - profundidade (recursão);
   - memória;
   - tempo.
3. **Um critério de estresse:** uma coleção de arquivos feitos para esgotar recursos ("bombas"). Para cada um, o leitor abre dentro dos limites ou recusa com mensagem clara, nunca cai e nunca trava além do tempo-limite.
4. **Um alvo de fuzzing.**
5. **O inventário de `unsafe` e de C** de toda biblioteca de leitura, feito ao aprová-la (como nas fichas desta entrega).

## 2. O problema que mudou o desenho: a Loro descomprime sem teto

- **O que acontece:** ao abrir um documento, a Loro descomprime partes dele (formato LZ4, em `loro-kv-store/src/compress.rs`, função `decompress`) **sem nenhum limite de tamanho**.
- **O risco:** um arquivo `.bivar` pequeno e feito de propósito pode virar muitos gigabytes na memória **dentro da Loro**, onde o nosso código não tem como interromper. O LZ4 chega a cerca de 255 vezes; um registro de 100 MB pode virar 25 GB.
- **O mesmo vale para o tempo:** um dado montado de propósito pode fazer a importação demorar indefinidamente, e o Rust não consegue interromper uma função no meio.
- **Conclusão:** limites de **memória e tempo** só podem ser garantidos se a primeira leitura de um arquivo desconhecido acontecer **num processo separado**, que pode ser encerrado sem derrubar o programa.

## 3. A proposta para o `.bivar`

### 3.1 Limites da caixa (o nosso código os confere antes de entregar qualquer coisa à Loro)

| Limite | Valor proposto | Hoje |
| --- | --- | --- |
| Registro de retrato ou de atualizações | 256 MiB cada | 1 GiB |
| Pedaço de arquivo grande | 4 MiB + 32 bytes (a identidade) | 1 GiB |
| Índice de arquivo grande | 8 MiB (cerca de 260 mil pedaços, ou 1 TB por arquivo grande) | 1 GiB |
| Soma de todos os dados do documento (retrato + atualizações) | 1 GiB | sem limite |
| Número de registros | 50 milhões | sem limite |

### 3.2 Verificação isolada

**O que acontece ao abrir um arquivo desconhecido:**

1. O Bivar o abre primeiro num **processo verificador**: uma cópia do próprio programa, em modo de verificação, sem interface.
2. O verificador lê o arquivo inteiro como o programa leria.
3. O processo principal espera, com limites:
   - **tempo: 60 s** (o documento de 1 milhão de operações abre em cerca de 2,5 s);
   - **memória: 4 GiB** (o mesmo documento usa cerca de 230 MB).
4. Se o verificador passar de um limite, é encerrado, e o Bivar mostra: "Este arquivo foi recusado: ele pede mais memória (ou mais tempo) do que um documento real precisaria." O programa principal nunca chega a ler o arquivo.
5. Se o verificador terminar bem, o processo principal abre o arquivo normalmente.

**Quando a verificação é pulada:** quando este mesmo computador foi o último a gravar o arquivo. O Bivar guarda, fora do documento, o tamanho e o resumo SHA-256 do fim de cada arquivo que ele gravou. Arquivos que você mesmo salvou abrem direto; arquivos que vieram de fora (e-mail, download, outra pessoa) passam pela verificação. Sem isso, abrir o documento de 1 milhão de operações levaria o dobro (cerca de 5 s), acima do limite de 3 s aprovado.

**Como impor o limite de memória** (é a escolha que preciso de você):

| Opção | Como funciona | Custo |
| --- | --- | --- |
| **(a) `cap`** | um "contador de memória" para o processo verificador; passou do limite, o verificador para | biblioteca minúscula: 1 pacote, 12 linhas com `unsafe`, sem C. **Sem versão nova desde 2023** (é pequena e não precisou mudar, mas está parada) |
| (b) contador próprio | o mesmo que (a), escrito por nós (cerca de 20 linhas) | exige **`unsafe` no nosso código** (o Rust exige isso para trocar o gerenciador de memória): sua aprovação, registro em `docs/unsafe.md` e um job de Miri na compilação automática |
| (c) "Job Objects" do Windows (`win32job`) | o próprio Windows impõe o limite | traz o pacote `windows` (327 mil linhas com `unsafe`, geradas automaticamente); só Windows (Linux precisaria de outra) |
| (d) vigiar de fora | o processo principal consulta, a cada 0,2 s, quanta memória o verificador usa, e o encerra se passar | **nenhuma biblioteca nova**; mas é um limite "mole": entre duas consultas, uma bomba pode crescer muito |

**Recomendação: (a) `cap`.** É pequena o bastante para ser lida inteira na revisão, não traz C e funciona igual no Windows e no Linux. A parada desde 2023 fica registrada como risco; a saída, se um dia for preciso, é a opção (b).

### 3.3 Critério de estresse do `.bivar`

Uma coleção de arquivos-bomba, gerada por um programa (não guardada pronta). Cada um precisa ser recusado com mensagem clara, dentro de 60 s, sem derrubar o programa:

- **tamanhos declarados absurdos:** um registro que diz ter 1 TB;
- **muitos registros minúsculos:** 50 milhões e um;
- **bomba de descompressão:** um documento real cujo conteúdo cresce muito ao descomprimir (por exemplo, um texto de 2 GB de letras repetidas, que comprimido fica pequeno);
- **árvore muito funda:** 1 milhão de grupos, um dentro do outro (procura recursão que estoura a pilha, na Loro ou no nosso código);
- **índice de arquivo grande que aponta para pedaços inexistentes ou repetidos milhões de vezes.**

## 4. Fuzzing

| Opção | Como funciona | Exige |
| --- | --- | --- |
| **`cargo-fuzz` + libFuzzer** | o padrão do Rust; um programa gera milhões de arquivos defeituosos, guiado pelo caminho que o código percorre | a versão *nightly* do Rust (só para o fuzzing); a `libfuzzer-sys` traz o libFuzzer em C++ (cerca de 13 mil linhas, 31 arquivos `.cpp`), compilado só para o fuzzing. A licença do libFuzzer é **NCSA**, que não está na nossa lista |
| `bolero` | uma camada por cima de vários "motores" (libFuzzer, AFL); tem um modo simples que roda no `cargo test` normal, sem *nightly* | o modo guiado também usa o libFuzzer, com as mesmas exigências |
| `cargo-afl` (AFL++) | outro motor clássico | código C do AFL++; funciona melhor no Linux |
| só testes de propriedades (`proptest`) | entradas aleatórias, sem guia | nenhuma ferramenta nova, mas encontra muito menos |

**Recomendação: `cargo-fuzz` + libFuzzer.**

- **Onde:** uma pasta só, **`fuzz/`** na raiz (o padrão do `cargo-fuzz`; a pasta já existe). O ADR-0011 diz `tests/fuzz/`; ele ainda está "Proposto", então pode ser corrigido direto, com a sua aprovação.
- **A versão *nightly***: travada numa data, valendo **só dentro de `fuzz/`**. O programa continua sendo compilado com a versão estável 1.99.1.
- **A licença NCSA**: liberada **só no `deny.toml` da pasta `fuzz/`**. O libFuzzer nunca entra no programa entregue ao usuário. A FSF classifica a NCSA como livre e compatível com a GPL.
- **Na compilação automática do GitHub (Linux):**
  - a cada envio, cada alvo roda por 2 minutos;
  - uma vez por dia, 30 minutos por alvo;
  - **os arquivos que já derrubaram o leitor uma vez** (e a coleção que o fuzzing vai juntando) também rodam no `cargo test` normal, na versão estável. Assim, todo envio confere os casos conhecidos, mesmo sem *nightly*.
- **No seu PC:** você não precisa rodar o fuzzing. Ele fica na compilação automática.

## 5. Bibliotecas novas propostas

| | `cap` | `libfuzzer-sys` | `cargo-fuzz` |
| --- | --- | --- | --- |
| Para quê | limite de memória do verificador | motor do fuzzing (só em `fuzz/`) | ferramenta de linha de comando que roda o fuzzing (só na compilação automática) |
| Versão exata | `=0.1.2` | `=0.4.13` | `0.13.2` (instalada na compilação automática, como já é feito com o `cargo-deny`) |
| Licença | MIT OR Apache-2.0 | (MIT OR Apache-2.0) AND **NCSA** | MIT OR Apache-2.0 |
| Manutenção | última versão em 2023-03; 338 mil downloads recentes | grupo `rust-fuzz`; versão de 2026-06; 20 milhões de downloads recentes | grupo `rust-fuzz`; versão de 2026-06 |
| Pacotes que entram | 1 | 8 | (não entra no código) |
| `unsafe` | 12 linhas | 245 linhas (109 no `getrandom`) | — |
| C / C++ | nenhum | **libFuzzer: 31 `.cpp` e cabeçalhos, cerca de 13 mil linhas**, compilados pelo `cc` | — |
| Alertas RustSec | nenhum | nenhum | — |
| Entra no programa do usuário | sim (só no modo verificador) | **não** | **não** |

## 6. O que a prova vai mostrar, depois da aprovação

1. o verificador recusa cada bomba da seção 3.3 com mensagem, dentro de 60 s, com o programa principal vivo;
2. um arquivo gravado neste computador abre sem verificação; o mesmo arquivo alterado por fora passa pela verificação;
3. o alvo de fuzzing do leitor `.bivar` roda na compilação automática, e os casos encontrados viram testes normais;
4. o ADR de entrada externa, com os limites e o critério de estresse de cada tipo de entrada.

# Relatório da prova de conceito do motor de texto (entrega 0.5)

> Prova **descartável**. Nada daqui vira código do produto.

## O que foi testado

- **Versões:**
  - parley 0.12.0, o motor de texto: monta as letras, quebra linhas e alinha;
  - harfrust 0.12.0, que faz o *shaping* (a montagem das letras pelas regras da fonte) e vem junto com o Parley;
  - skrifa 0.44.0, que lê as fontes;
  - vello 0.11.0, que desenha pela placa de vídeo, sobre o wgpu 30.0.1;
  - vello_cpu 0.3.0, que desenha sem placa de vídeo.

  As versões do Vello, do vello_cpu e do wgpu são as mesmas da prova 0.3 (ADR-0018). O Parley usa as mesmas bibliotecas de fonte que o Vello (skrifa 0.44, read-fonts 0.41, peniko 0.6.1), com uma única cópia de cada.
- **Máquina:** Windows 11, Ryzen 7 5700X, NVIDIA GeForce GTX 1660 (Vulkan).
- **Regra do dono:** nenhum `unsafe` no nosso código. O crate usa `unsafe_code = "forbid"`.
- **Fontes, as duas com licença OFL-1.1 e baixadas dos repositórios oficiais da Adobe:**
  - **Source Sans 3 Regular, OTF** (contornos CFF), a mesma da prova 0.3;
  - **Source Serif 4 Regular, TTF** (contornos TrueType), de `adobe-fonts/source-serif`, ramo `release`. SHA-256: `e5a4ee6a3d87bb9024796be390c6771e2a0eb1883dae25effaf57ca01668e24b`.
- **Só as fontes embutidas:** o Parley foi compilado sem acesso às fontes do sistema (sem o recurso `system`). A prova nunca lê as fontes instaladas no computador, e a montagem do texto (quais letras e em que posição) sai igual em qualquer máquina. Os pixels dependem também do modo SIMD; veja o item 6.

## Resultados por critério

### 1. Acentos em português

A frase "Açaí à brasileira — ótimo, pão, avô, ação" foi escrita de dois jeitos:

- com as letras acentuadas prontas (41 caracteres);
- com a letra seguida do acento separado (49 caracteres; são 8 letras acentuadas).

**Passou.** Nas duas fontes e em dois tamanhos, os dois jeitos dão os mesmos desenhos de letra, nas mesmas posições. Esses desenhos são as letras acentuadas prontas da própria fonte: o motor junta "a" + acento agudo no "á" que a fonte já tem. Desenhadas sem placa, as duas linhas saem **pixel a pixel iguais**; com placa, ficam dentro da folga do ADR-0018 (até 3 pixels, 1 nível). A frase não testa acento posicionado sobre uma letra que não tem versão pronta na fonte.

### 2. Kerning

**Passou.** Em "AV", "To" e "Ty", nas duas fontes, a distância entre as letras é a largura da primeira letra mais o ajuste que a fonte define para o par. O teste lê esse ajuste direto da tabela GPOS da fonte, por um código próprio que não passa pelo Parley nem pela lógica do harfrust (mas usa a mesma biblioteca de leitura de tabelas, o `read-fonts`), e confere a posição com folga de 0,001 pixel. O ajuste escala com o tamanho da letra. Com o kerning desligado, cada letra avança exatamente a própria largura.

### 3. Ligaduras

**Passou, com uma ressalva sobre a fonte OTF.** A **Source Sans 3 não tem ligadura "fi" nem "fl"**: as ligaduras dela são "ff", "ft" e "fft". Um teste confirma isso pela tabela GSUB. Por isso, como o critério diz "com uma fonte que as tem":

- **"fi" e "fl"** são testados com a **Source Serif 4**: cada par vira o desenho único da ligadura definido na fonte;
- **ligar e desligar** é testado nas duas fontes ("ff" e "ft" na Source Sans 3; "fi", "fl", "ff" e "ft" na Source Serif 4), também no meio de palavras ("afinal", "afta");
- com as ligaduras desligadas, as letras voltam a aparecer separadas, e o desenho muda nas duas fontes.

O dono aceitou essa solução em 2026-10-28.

### 4. Formatos de fonte

**Passou.** Tudo acima roda com a OTF (contornos CFF) e com a TTF (contornos TrueType). Um teste confere o tipo de cada arquivo e as tabelas de contorno.

### 5. Testes visuais

**Passou.** O dono aprovou 4 imagens de referência em 2026-10-28, em `tests/golden/`: as duas fontes, cada uma desenhada pela placa de vídeo e sem ela. Cada imagem mostra os acentos nos dois jeitos, o kerning ligado e desligado e as ligaduras ligadas e desligadas.

- **Sem placa:** comparação exata.
- **Com placa:** a folga do ADR-0018, de até 3 pixels com diferença de no máximo 1 em 255. Os testes com placa rodam um de cada vez, com a trava da limitação 4 do ADR-0017.
- **Placa × sem placa:** 2,2 a 2,4% dos pixels diferem, todos nas bordas suavizadas das letras, com diferença máxima de 48 a 72 e média de 0,03 a 0,11 em 255. É o comportamento esperado pelo ADR-0018. **Nenhum pixel difere 128 ou mais**, e é isso que o teste exige. Um teste sem placa prova que essa regra pega a falta de uma única letra, até numa legenda de 16 px. (A primeira versão do teste limitava só a média, o que não detectaria nem uma linha inteira faltando; achado 1 da auditoria.)

### 6. Mesma imagem no PC do dono e no GitHub

**Passou.** As imagens sem placa feitas na compilação automática do GitHub (Linux, Ubuntu 24.04) são idênticas, pixel a pixel, às aprovadas no PC do dono (Windows). A etapa `poc-text-cpu` do `ci.yml` faz essa comparação a cada envio; o primeiro resultado foi no commit `a58f6d9`, em 2026-10-28.

| Máquina | Processador | Modo SIMD do vello_cpu |
| --- | --- | --- |
| PC do dono | AMD Ryzen 7 5700X | AVX2 |
| GitHub (`ubuntu-24.04`) | AMD EPYC 7763 64-Core | AVX2 |

**O modo SIMD muda os pixels.** O vello_cpu escolhe sozinho o conjunto de instruções rápidas do processador. Na mesma máquina, forçando cada modo e comparando com as referências feitas em AVX2:

| Modo | Source Sans 3 (OTF) | Source Serif 4 (TTF) |
| --- | --- | --- |
| AVX2 | idêntica | idêntica |
| SSE4.2 | 2 pixels diferem, em 1 nível de 255 | idêntica |
| SSE2 | 2 pixels diferem, em 1 nível de 255 | idêntica |

Os tempos foram parecidos nos três modos, de 3 a 4 ms por página, numa amostra só por modo. O AVX-512 não pôde ser testado, porque o Ryzen 7 5700X não tem esse modo. A medição pode ser repetida com:

```
cargo run --release --manifest-path poc/text-parley/Cargo.toml --example simd_levels
```

**Consequência:** o PC e o GitHub concordaram porque os dois usaram AVX2. Se o GitHub passar a usar uma máquina com AVX-512, ou só com SSE4.2, a comparação exata pode falhar sem que nada tenha mudado no código. O vello_cpu permite **fixar o modo** sem `unsafe`: `RenderContext::new_with` recebe o modo, e `Level::as_avx2()` ou `Level::as_sse4_2()` descem do modo detectado para um mais simples. **Modo fixo aplicado na prova (pedido do dono, 2026-10-29):** os testes de referência sem placa desenham em **AVX2, sem linhas de execução extras** (`cpu::reference_settings()`), e conferem que o mínimo da compilação é SSE2. Assim, o teste não falha quando o GitHub usar outro processador com AVX2 ou AVX-512. Numa máquina sem AVX2 (Mac com chip Apple, Windows em ARM), esses testes são **pulados com uma mensagem visível** e continuam obrigatórios no GitHub e no PC do dono. As 4 referências aprovadas continuaram idênticas com o modo fixo. O programa que gera as imagens para o dono olhar (`cargo run`) continua usando o modo detectado, como o produto faria.

## Imagem extra: parágrafo quebrado em linhas

A pedido do dono, a prova gera `cpu_paragrafo.png` e `gpu_paragrafo.png`, que não são imagens de referência. Elas mostram um parágrafo em português numa coluna de 520 pixels, nas duas fontes, alinhado à esquerda e justificado. Testes de unidade conferem que:

- nenhuma linha passa da largura;
- nenhuma letra se perde na quebra;
- as linhas descem uma altura de linha por vez;
- no justificado, toda linha com mais de uma palavra, menos a última, chega à margem direita.

**O que se vê:** a quebra é limpa, mas o justificado deixa espaços grandes em algumas linhas (por exemplo, "ferramentas próprias, mas todas trabalham sobre o"). O Parley 0.12 quebra linha por linha, de forma gulosa, e não divide palavras em sílabas no fim da linha. Uma palavra sozinha na linha fica alinhada à esquerda, porque não tem espaço para esticar.

## Dependências

- **Peças que o Parley acrescenta:** 34 crates, contados no `Cargo.lock` da prova (todas as plataformas). A maioria (22) é o **ICU4X**, as tabelas e regras do Unicode para quebra de linha, normalização e propriedades de caracteres, com licença Unicode-3.0. Os outros são o próprio Parley (`parley`, `parley_engine`, `parley_data`, `parley_emoji`, `parlance`, `fontique`), o `harfrust`, o `memmap2` e auxiliares.
- **No produto, com o egui:** o eframe 0.37 já traz o `harfrust` em todos os sistemas e o `memmap2` só no Linux. O Parley acrescenta **33 no Windows e no macOS** e **32 no Linux**.
- **Licenças:** todas na lista permitida (MIT, Apache-2.0, Zlib, Unicode-3.0 e combinações). `cargo deny check` passa sem exceções de segurança (e roda também na etapa `poc-text-cpu` do GitHub), e `cargo audit` não encontra falhas conhecidas.
- **Versões duplicadas:** o `cargo deny` avisa 3 bibliotecas auxiliares em duas versões (`hashbrown`, `miniz_oxide`, `syn`), nenhuma ligada a texto ou fontes. A prova 0.3 tinha 22 avisos desse tipo.
- **Fora do Parley:** `png` e `pollster`, os mesmos da prova 0.3.

## Testes automáticos que acompanham a prova

- **36 testes de unidade:** fontes, leitura das tabelas GPOS e GSUB, montagem de linha e de parágrafo, páginas de teste, desenho sem placa, modos SIMD e imagens PNG.
- **9 testes de critério** (`tests/criteria.rs`): acentos, kerning, ligaduras e formatos, conferidos contra as tabelas da fonte.
- **10 testes de desenho** (`tests/render.rs`): as 4 imagens de referência, a repetição exata sem placa, os acentos iguais em pixels (com e sem placa), as ligaduras mudando o desenho, a comparação placa × sem placa e a prova de que essa comparação pega uma letra faltando.

Os testes com placa de vídeo ficam desligados por padrão. No PC do dono:

```
cargo test --manifest-path poc/text-parley/Cargo.toml -- --include-ignored
```

## Como gerar as imagens

```
cargo run --manifest-path poc/text-parley/Cargo.toml
```

As imagens vão para `poc/text-parley/exportacoes/`. Com `-- --sem-placa`, só as imagens sem placa de vídeo.

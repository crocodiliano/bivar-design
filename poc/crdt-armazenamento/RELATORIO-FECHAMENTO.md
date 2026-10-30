# Entrega 0.6: ajustes da Parte 6 e fechamento (Parte 7, sem auditoria)

Data: 2026-10-30. A auditoria será pedida pelo dono numa conversa nova.

## 1. Os cinco ajustes pedidos depois da Parte 6

### 1.1 Camadas ocultas

- **Antes de gravar, a cópia para enviar pergunta.** Se houver camadas ocultas, o programa mostra o aviso "Este arquivo contém N camadas ocultas", com a lista delas:
  - o nome;
  - o tipo;
  - quantas camadas há dentro de cada uma.
- **O dono escolhe:**
  - **manter:** elas vão, ainda ocultas;
  - **remover:** elas ficam fora, com tudo o que há dentro delas.
- **Sem camadas ocultas, não há a pergunta.**
- **No protótipo, uma camada oculta é o parâmetro `visible` igual a 0.** O produto terá um parâmetro de sim/não.
- **Teste:** `hidden_layers_are_listed_and_can_be_left_out`.

### 1.2 Privacidade da cópia

O que a cópia leva além do conteúdo visível:

| Informação | Na cópia | Por quê |
|---|---|---|
| Cabeçalho (marca, versão, opções, conferência) | vai | indispensável para abrir; nada pessoal |
| Versão da organização do documento | vai | indispensável para abrir |
| Identidade do documento | **nova e sorteada** | indispensável: impede a junção com o original |
| Identificador de autor da Loro | **um só, sorteado na hora** | indispensável: a Loro exige um autor em cada operação |
| Hash e pedaços das imagens em uso | vão | são o conteúdo |
| Nome do computador ou da cópia | **não vai** | |
| Nomes dos marcos salvos | **não vai** | |
| Datas (salvamentos, cópias, remoções) | **não vão**; a marca de salvamento da cópia tem data zero | |
| Data de cada mudança na Loro | **não vai** (todas zero, conferido) | |
| Pasta e nome do arquivo das imagens | **nunca guardados**, nem no arquivo de trabalho | a imagem é guardada pelo conteúdo |
| Autores do original | **não vão** | |

**Dois vazamentos encontrados e corrigidos nesta revisão:**
- **A identidade de todo documento levava o número do autor e a hora da criação, em nanossegundos.** Agora é sorteada.
- **A marca de salvamento da cópia levava a data e a hora.** Agora é zero.

**Fora do nosso controle:** o nome do arquivo, que o dono escolhe, e as datas que o Windows guarda fora do conteúdo do arquivo, que um e-mail ou um compactador podem levar.

**Teste:** `the_copy_says_nothing_about_who_where_or_when`. O original grava um nome de computador e um marco com nomes únicos, e importa uma imagem de uma pasta com nome único.
- **Controle:** o teste confere primeiro que o original guarda o nome do computador, o do marco, a identidade, os autores e as datas.
- **Na cópia não aparece nenhum deles,** nem nenhum instante em que o teste rodou, em decimal, em hexadecimal ou em bytes.

### 1.3 A busca nos bytes com o arquivo compactado

**A busca simples nos bytes não basta.** A Loro compacta o retrato do documento:
- um texto curto e único aparece nos bytes;
- um texto repetido vira uma referência ao trecho anterior e some da busca simples.

**Prova:** um texto visível com o marcador repetido 50 vezes não é achado nos bytes da cópia, mas é achado depois de descompactado (`the_search_looks_inside_compressed_data`).

**Por isso todas as buscas dos testes da cópia olham agora quatro formas:**
- os bytes gravados;
- as mudanças na forma sem compactação da Loro;
- o histórico decodificado;
- o estado do documento.

### 1.4 Arquivos `.parcial`

- **O Bivar nunca apaga um `.parcial` que sobrou de uma queda sem perguntar.**
  - **Ao salvar no mesmo nome,** o salvamento para e avisa que há uma cópia gravada pela metade.
  - **Ao abrir um documento com um `.parcial` ao lado,** o relatório da abertura avisa.
  - **O `.parcial` só é apagado depois que o dono manda.**
- **Única exceção:** o `.parcial` que a própria gravação acabou de criar e não conseguiu terminar por erro do disco, na mesma hora. Esse não sobrou de uma queda.
- **Teste:** `a_leftover_partial_copy_is_reported_and_kept`.

### 1.5 Bateria de medidas no repositório

- **Cada rodada grava em `prototipo/resultados/bateria/`, que entra no repositório:**
  - o registro completo da rodada (`<data>_<hora>.txt`);
  - uma linha por medida em `historico.csv`;
  - a referência da próxima comparação (`ultima.json`).
- **O resumo diz de qual versão do código a medida veio.**
- **Os registros trocam a pasta temporária do usuário por `%TEMP%`.** O repositório é público, e o nome da pasta revelaria o nome do usuário do Windows. O mesmo foi feito no registro antigo da Parte 4 (`disco-medidas.txt`).

## 2. Fechamento

### 2.1 Critérios da 0.6 em `docs/fases/fase-0.md`

| Critério | Situação |
|---|---|
| Formato por dentro do arquivo (C1) | **feito:** formato próprio (ADR-0023). Teste novo: um registro de tipo desconhecido injetado é tratado como dano, com mensagem |
| Toda entrada externa (A7) | **feito** (ADR-0024) |
| Onde mora a verdade do documento (A1) | **feito** (ADR-0021) |
| Operações independentes da biblioteca CRDT | **feito** (`check-loro-isolated.sh` na compilação automática) |
| Comparação Loro × Automerge | **feito, com uma ressalva:** antes da cópia, o aviso de pendências ainda não cobre os movimentos que perderam nem as edições escondidas por remoção (item da 0.7) |
| Arquivos grandes em pedaços | **feito** |
| Riscos de armazenamento (A6) | **feito** (ADR-0023) |
| Contingência avaliada | **feito** (decisão do dono de 2026-10-29) |
| Licenças | **feito** |
| **Revisão: ADRs "Revisado por IA"** | **não feito**: os ADRs 0021 a 0024 estão "Proposto", à espera da auditoria |

### 2.2 ADRs criados ou alterados na 0.6

**Criados (os quatro estão "Proposto"):**

| ADR | Assunto |
|---|---|
| [0021](../../docs/adr/0021.md) | Biblioteca CRDT: Loro 1.17.1, com as condições do dono |
| [0022](../../docs/adr/0022.md) | "A última escrita" decidida por relógio lógico (troca a frase do ADR-0019 e do CLAUDE.md) |
| [0023](../../docs/adr/0023.md) | Arquivo `.bivar` em formato próprio, riscos de armazenamento, limpeza, cópia para enviar |
| [0024](../../docs/adr/0024.md) | Toda entrada externa: limites, processo verificador, confiança fora do arquivo, fuzzing |

**Alterados:**
- [0011](../../docs/adr/0011.md) ("Proposto"): a pasta do fuzzing passou a ser `fuzz/`;
- [0014](../../docs/adr/0014.md) ("Proposto"): pendência da proteção contra bombas nos dados de P2P;
- [README dos ADRs](../../docs/adr/README.md): linhas do índice;
- o [ADR-0019](../../docs/adr/0019.md) ("Revisado por IA") **não foi alterado**: a mudança do relógio está no ADR-0022, como o CLAUDE.md exige.

**Fora dos ADRs, também alterados:**
- `CLAUDE.md`: a BSL-1.0 entrou na lista de licenças, e a frase do relógio lógico;
- os arquivos de licenças (`deny.toml`);
- `docs/fases/fase-0.md` e `docs/fases/nucleo.md`;
- `docs/formato/`;
- `fuzz/README.md`.

### 2.3 Checagens no fechamento

**Todas passaram:**
- **Protótipo:**
  - formatação;
  - Clippy sem avisos;
  - 72 testes;
  - os 2.000 + 2.000 cenários;
  - isolamento da Loro;
  - `cargo deny`;
  - `cargo audit`, só com os 6 alertas já registrados.
- **Comparação da Parte 2:** 29 testes e `cargo deny`.
- **Raiz:** formatação, Clippy, testes, `cargo deny` e `cargo audit`.
- **`fuzz/`:** `cargo deny`.

**Bateria de medidas:** 23 medidas, nenhuma acima do limite, nenhuma piorou (`resultados/bateria/2026-10-30_1709.txt`).
- **Desta vez o C: tinha espaço,** e o arquivo de 2 GB foi medido também no SSD:
  - guardar levou 37,7 s, contra 16 s na Parte 4;
  - abrir levou 0,02 s;
  - ler e conferir levou 4,4 s.
- **Não há limite aprovado para guardar,** e esta é a primeira rodada da bateria no SSD, então ainda não há com o que comparar. Com o C: 95% cheio, a escrita lenta é esperada. As próximas rodadas mostram se isso se repete.

### 2.4 Fuzzing no GitHub

Ainda não rodou: a primeira rodada acontece neste envio. O resultado entra aqui depois.

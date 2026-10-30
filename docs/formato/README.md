# Formato do arquivo `.bivar`, versão 1 (rascunho)

> **Situação:** rascunho, escrito na entrega 0.6 a partir do protótipo `poc/crdt-armazenamento/prototipo/arquivo`. Vira a especificação oficial na entrega N.2. Decisão: [ADR-0023](../adr/0023.md). Versão em inglês: [README.en.md](README.en.md).

Esta especificação basta para escrever um leitor do arquivo em qualquer linguagem, sem olhar o código do Bivar. O conteúdo de dois tipos de registro é o formato binário da biblioteca Loro, que é aberto mas pertence à Loro (seção 5).

## 1. Visão geral

- **Um arquivo só,** sem arquivos auxiliares.
- **O arquivo começa com um cabeçalho de 64 bytes,** seguido de uma sequência de **registros**.
- **Registros só são acrescentados no fim.** Um registro já escrito nunca é alterado.
- **Cada registro traz um resumo SHA-256** que cobre o tipo, o tamanho e o conteúdo. Um registro cortado ou alterado é detectado.
- **A compactação** grava um arquivo novo (com o mesmo formato) e o põe no lugar do antigo de uma vez.

Todos os números são inteiros sem sinal, em ordem **little-endian**.

## 2. Cabeçalho (64 bytes)

| Posição | Tamanho | Conteúdo |
| --- | --- | --- |
| 0 | 8 | o texto `BIVARDOC` (ASCII) |
| 8 | 4 | versão do formato: `1` |
| 12 | 4 | opções (*flags*): `0`. O bit 0 está **reservado para criptografia**, adiada por decisão do dono. Um leitor recusa qualquer bit desconhecido |
| 16 | 16 | reservado: zeros (parâmetros da criptografia, no futuro) |
| 32 | 32 | SHA-256 dos bytes 0 a 31 |

**O leitor recusa o arquivo:**

- se o texto inicial não for `BIVARDOC` ("não é um arquivo do Bivar");
- se o resumo do cabeçalho não bater ("cabeçalho danificado");
- se a versão for maior que a que ele conhece ("arquivo de uma versão mais nova do Bivar"). **Nunca tenta "consertar".**

## 3. Registros

| Campo | Tamanho | Conteúdo |
| --- | --- | --- |
| tipo | 1 | um dos tipos da seção 4 |
| tamanho | 8 | tamanho do conteúdo, em bytes; no máximo o limite do tipo (seção 3.1) |
| conteúdo | `tamanho` | depende do tipo |
| resumo | 32 | SHA-256 de `tipo ‖ tamanho ‖ conteúdo` |

### 3.1 Limites ([ADR-0024](../adr/0024.md))

O leitor confere estes limites **antes** de ler ou reservar memória para o conteúdo. Um arquivo acima de um limite é recusado com uma mensagem que diz qual limite foi passado. O usuário pode aumentar um limite nas configurações, só de forma explícita e com aviso.

| O quê | Limite |
| --- | --- |
| um registro de retrato ou de atualizações | 256 MiB |
| todos os registros de retrato e de atualizações juntos (os dados do documento) | 1 GiB |
| um pedaço de arquivo grande | 32 bytes + 4 MiB |
| um índice de arquivo grande | 8 MiB (cerca de 260 mil pedaços, ou 1 TB por arquivo grande) |
| uma marca de salvamento | 8 bytes |
| número de registros | 50 milhões |

**Os arquivos grandes** (imagem, vídeo, áudio) **não entram no 1 GiB:** eles têm os limites próprios do pedaço e do índice. O tamanho total do arquivo não tem limite.

### 3.2 Leitura

**Como ler:**

1. Leia os registros em ordem, a partir do byte 64.
2. Se o arquivo terminar no meio de um registro, ou se o **último** registro tiver o resumo errado, esse registro é uma **gravação cortada**, por exemplo numa queda. Descarte-o; tudo antes dele vale. Um leitor que pode gravar remove esses bytes do fim.
3. Se um registro que **não é o último** tiver o resumo errado, um tipo desconhecido ou um tamanho acima do limite, o arquivo está **danificado** a partir dali. Use só o que veio antes, abra **só para leitura** e avise o usuário.

## 4. Tipos de registro

| Tipo | Nome | Conteúdo |
| --- | --- | --- |
| 1 | Retrato | o documento inteiro, no formato de retrato (*snapshot*) da Loro. É sempre o primeiro registro. A compactação escreve um retrato "raso" (*shallow*), que guarda só o histórico a partir de um ponto |
| 2 | Atualizações | mudanças do documento desde o registro de atualizações anterior, no formato de atualizações (*updates*) da Loro. São aplicadas em ordem, depois do retrato |
| 3 | Pedaço | 32 bytes: a identidade do pedaço (SHA-256 dos dados); depois, até 4 MiB de dados de um arquivo grande (imagem, áudio, vídeo). Um pedaço igual nunca aparece duas vezes. Ao abrir, o leitor pode ler só a identidade e pular os dados (salvo no último registro, sempre conferido inteiro); os dados são conferidos contra a identidade quando lidos |
| 4 | Índice de arquivo grande | 32 bytes: SHA-256 do arquivo inteiro; 8 bytes: tamanho total; depois, 32 bytes por pedaço (as identidades, em ordem) |
| 5 | Marca de salvamento | 8 bytes: segundos desde 1970, ou 0 (sem data, na cópia para enviar). "O usuário salvou aqui." As atualizações depois da última marca são as "edições desde o último salvamento", que o programa oferece manter ou descartar |

**Um tipo desconhecido** dentro da versão 1 é tratado como dano (seção 3). Um tipo novo exige uma versão nova do formato.

## 5. O documento dentro dos registros

O conteúdo dos registros de retrato e de atualizações é o formato binário da **Loro 1.17.1** (biblioteca aberta, licença MIT, [loro.dev](https://loro.dev)). Essa dependência fica registrada: ler o documento exige a Loro, ou uma implementação compatível do formato dela. Como saída legível, o programa pode exportar o histórico em JSON (`export_json_updates` da Loro).

**A organização do documento dentro da Loro** ([ADR-0021](../adr/0021.md)):

- **mapa `meta`:**
  - `doc_id`: a identidade do documento, sorteada (`doc-` e 32 dígitos hexadecimais; não diz quem criou nem quando);
  - `schema`: a versão da organização, hoje `2` (a 2 passou a guardar a marca de removido como entradas, com a hora da remoção). Um documento de outra versão é recusado, nunca lido "do jeito que der".
- **árvore `tree`:** os nós. A ordem dos filhos é a ordem da pilha. O mapa de cada nó guarda:
  - `kind`;
  - parâmetros como entradas `p:<nome>#<contador>.<cópia>` (16 dígitos hexadecimais cada número). O valor do parâmetro é o da **maior** chave;
  - a marca de removido como entradas `removed#<contador>.<cópia>`: um número (segundos desde 1970, quando foi removido) quer dizer "removido", e `false` quer dizer "restaurado";
  - `text`, para nós de texto.
- **mapa `edges`:** uma conexão por chave, no texto `origem␟porta␟destino␟porta` (␟ é o caractere 0x1F).
- **mapa `copies`:** `<cópia>#vv` (a versão que essa cópia já viu) e `<cópia>#at` (quando, em segundos).
- **mapa `checkpoints`:** marcos de tempo, com a chave `<segundos>.<cópia>.<operações>` e a versão naquele momento.
- **mapa `milestones`:** marcos nomeados, com o nome e a versão. **Nunca são limpos.**

## 6. Gravação

- **As edições são acrescentadas** como registros de atualizações enquanto o usuário trabalha.
- **Forçar a gravação no disco** (`fsync`/`sync_data`) acontece:
  - a cada **1 segundo** de edições, no máximo;
  - ao salvar;
  - ao fechar o programa;
  - ao compactar.

  Numa queda de energia, perde-se no pior caso o último segundo.
- **Um programa que abre o arquivo para gravar impede outro de gravar ao mesmo tempo:**
  - no Windows, abrindo o arquivo com compartilhamento só para leitura;
  - nos outros sistemas, com uma trava (*lock*).

  O segundo programa abre só para leitura, e copiar o arquivo continua funcionando.
- **A compactação:**
  1. escreve o arquivo novo como `<nome>.compact-tmp` e força a gravação dele no disco;
  2. renomeia o arquivo antigo para a cópia de segurança `<nome>.anterior`;
  3. renomeia o `.compact-tmp` para `<nome>`;
  4. abre o arquivo novo para conferir;
  5. apaga o `.anterior`. Se o novo não abrir bem, o `.anterior` volta para o lugar.
- **Ao abrir:**
  - se `<nome>` estiver faltando ou estragado e `<nome>.anterior` existir, o leitor oferece recuperar a partir dele. O arquivo estragado nunca é apagado: é guardado como `<nome>.estragado`;
  - se `<nome>` abrir bem e houver um `.anterior` (compactação interrompida depois da troca), o `.anterior` é apagado;
  - um `.compact-tmp` que sobrar é apagado.
- **"Salvar cópia para enviar" e "Salvar como"** gravam primeiro `<nome>.parcial`, forçam no disco e só então renomeiam para `<nome>`. Nunca sobrescrevem um arquivo que já existe. Um `.parcial` que sobrar de uma queda nunca é lido como documento nem apagado sem perguntar: a gravação seguinte no mesmo nome para e avisa, e a abertura de `<nome>` avisa que ele está ao lado.
- **A cópia para enviar é um arquivo comum deste formato,** com uma identidade de documento nova e um histórico que começa nela. Nada no arquivo diz que ele é uma cópia.

## 7. O que não faz parte do formato

- **Scripts:** abrir um arquivo nunca executa código (`CLAUDE.md` §4).
- **Criptografia:** adiada; o espaço está reservado no cabeçalho.

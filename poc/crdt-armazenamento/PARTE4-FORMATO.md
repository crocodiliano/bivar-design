# Entrega 0.6, Parte 4 (no papel): o arquivo no disco

Data: 2026-10-29. Análise antes de qualquer protótipo, como o dono pediu: comparar as três opções no papel e prototipar só a recomendada. **Nada aqui está decidido.**

## 1. As três opções (achado C1)

| | (a) SQLite com proteções | (b) SQLite num processo isolado | (c) Formato próprio, só em Rust |
| --- | --- | --- | --- |
| Código em C dentro do Bivar | **271.131 linhas** (o `sqlite3.c` que vem com o `libsqlite3-sys` 0.39.1) | as mesmas 271.131 linhas, num processo separado | **nenhuma** |
| Regra "ler arquivos externos só em Rust" (ADR-0011, `CLAUDE.md` §4) | **viola**: exige emenda, aprovada pelo dono | cumpre pela exceção do ADR-0011 (processo isolado), com o custo de um segundo programa | **cumpre** |
| Bibliotecas novas | `rusqlite` + `libsqlite3-sys` (14 pacotes, 4.169 linhas com `unsafe` só no `libsqlite3-sys`, alertas antigos no RustSec) | as mesmas, mais a comunicação entre processos | só um resumo criptográfico (`sha2`, ficha abaixo) |
| Arquivo único de verdade | precisa de cuidado: o modo de diário (`-wal`) cria arquivos ao lado; o modo `DELETE` é mais lento | igual a (a) | **sim**: o arquivo só cresce no fim, e a limpeza grava um arquivo novo que substitui o antigo de uma vez |
| Mudar um parâmetro não regrava pedaços grandes | sim (só as páginas mudadas) | sim | sim (acrescenta só um registro pequeno no fim) |
| Queda no meio da gravação | o SQLite cuida (testado no mundo inteiro) | igual | **nós cuidamos**: cada registro tem um resumo; um registro incompleto no fim é ignorado ao abrir |
| Arquivo adulterado (gatilho, visão, tabela extra) | precisa de proteções ativas e uma checagem de esquema; superfície grande | protege o programa, mas não os dados | **não existe**: o formato não tem gatilhos, visões nem consultas |
| Esforço e risco | baixo para gravar, alto para proteger | alto (dois programas, comunicação) | **médio**: o código do formato é nosso e precisa de testes de queda e de fuzzing (Parte 5) |
| Especificação aberta (`docs/formato/`) | o SQLite é documentado; o nosso esquema por cima, também | igual | a nossa caixa é simples de especificar; o conteúdo da Loro segue o formato dela (aberto, mas da Loro) |

**Recomendação: (c), formato próprio em Rust.**

- **É a única opção que cumpre a regra de segurança do projeto sem emenda e sem um segundo programa.**
- **O que o SQLite faria por nós, o desenho abaixo resolve com pouco código:**
  - gravar sem corromper;
  - não regravar pedaços grandes.
- **O que ele traria junto, nenhuma das outras opções evita:**
  - 271 mil linhas de C;
  - a superfície de gatilhos e visões.

## 2. O desenho proposto para (c): um arquivo que só cresce no fim

**Cabeçalho:** um texto fixo, a versão do formato e o `doc_id`.

**Depois dele, uma sequência de registros.** Cada registro tem:

- o tipo;
- o tamanho;
- o conteúdo;
- um resumo SHA-256, que cobre o tipo, o tamanho e o conteúdo.

**Os tipos de registro:**

| Tipo | Conteúdo |
| --- | --- |
| Atualização | mudanças da Loro desde o registro anterior (incremental) |
| Retrato | o documento da Loro inteiro (escrito pela limpeza) |
| Pedaço | até 4 MiB de um arquivo grande; identificado pelo SHA-256 do próprio conteúdo, então um pedaço igual nunca é gravado duas vezes |
| Índice de arquivo grande | o arquivo grande (pelo hash dele) → a lista dos seus pedaços |
| Marca de salvamento | "o usuário salvou aqui" |

**Como cada risco da entrega é tratado:**

- **Salvar e o salvamento automático:** as edições vão sendo acrescentadas no fim, enquanto o usuário trabalha. O trabalho pesado roda fora da interface, e a meta aprovada é uma pausa de no máximo 16 ms.
- **Edições desde o último salvamento:** elas já estão no arquivo, então voltam depois de uma queda. Ao reabrir, o programa diz quantas edições há depois da última marca de salvamento e deixa voltar para ela.
- **Queda no meio da gravação:** um registro cortado ou com o resumo errado no fim é ignorado, e o arquivo abre com tudo o que estava completo.
- **Gravação no disco de verdade:** o programa força a gravação (`sync_data`) a cada lote de edições, no máximo a cada 1 s. Numa queda de energia, perde-se no pior caso o último segundo de edições. O custo disso será medido no HD (A:) e no SSD (C:).
- **Arquivo único:** não há arquivo auxiliar. Copiar o arquivo com o programa aberto dá uma cópia válida, com tudo até o último registro completo.
- **Mesmo arquivo aberto duas vezes:** a primeira abertura tranca o arquivo (`File::try_lock`, que já vem no Rust, sem biblioteca nova). A segunda abre só para leitura, com mensagem.
- **Arquivo corrompido no meio:** o programa abre até o último registro bom, só para leitura, e mostra uma mensagem dizendo até onde conseguiu ler. Nunca cai, e isso é coberto por fuzzing na Parte 5.
- **Migração de versão:**
  - a versão nova é gravada num **arquivo novo**, e o original só é substituído quando o novo está completo e conferido;
  - uma cópia do original fica guardada;
  - um arquivo de versão mais nova é recusado com mensagem.
- **Migração do histórico:** as operações antigas continuam no formato da Loro. Uma mudança na organização dentro da Loro (`schema`) é feita por operações novas, que entram no histórico como qualquer outra. Colegas com versões diferentes da organização não se juntam: a junção é recusada com mensagem até os dois atualizarem.
- **Pastas sincronizadas (OneDrive, Dropbox, unidades de rede):** aviso ao abrir. Elas podem copiar o arquivo no meio de uma gravação e criar duplicatas com conflito. A detecção é pelo caminho e pelas variáveis do Windows (`OneDrive`, `OneDriveCommercial`), sem biblioteca nova.
- **Criptografia opcional:** adiada, com justificativa. Não há pedido concreto ainda, e a escolha de biblioteca exige um ADR (`CLAUDE.md` §4: "criptografia nunca é caseira"). O formato reserva um campo no cabeçalho para isso.
- **Especificação aberta:**
  - a caixa (cabeçalho e registros) fica descrita em `docs/formato/`;
  - o conteúdo dos registros de atualização e de retrato é o formato binário da Loro, que é aberto mas pertence à Loro. Essa dependência fica registrada;
  - como saída legível, o programa pode exportar o histórico em JSON (a Loro tem isso).

## 3. A limpeza: nós removidos, histórico antigo e arquivos grandes (pedido do dono)

"Remover é marcar" deixa o nó no arquivo. O histórico também guarda tudo o que foi apagado.

**A proposta: a limpeza é um passo próprio, chamado "compactar".** Ele grava um arquivo novo com:

- um retrato;
- só os pedaços ainda usados.

Depois, o arquivo novo substitui o antigo de uma vez.

**O que a compactação pode jogar fora, e quando:**

| O quê | Pode ser limpo quando… | Por que isso não quebra nada |
| --- | --- | --- |
| **Nó removido** (vira remoção definitiva na árvore da Loro) | removido há mais de **30 dias**, **e** nenhum conflito ou aviso aberto o cita, **e** nenhuma conexão viva aponta para ele, **e** todos os colaboradores conhecidos do documento já receberam a remoção | o desfazer do Bivar vive só na sessão aberta (não é salvo), então não aponta para nós de dias atrás; e "todos já receberam" impede que um colega edite um nó que não existe mais |
| **Histórico antigo** (retrato "raso" da Loro, que guarda só o histórico a partir de um ponto) | anterior ao ponto que **todos os colaboradores conhecidos** já receberam, **e** com mais de 30 dias | um colega que já recebeu tudo até ali consegue continuar se juntando; os registros de conflito ficam no documento (0.7), não dependem do histórico apagado |
| **Pedaços de arquivo grande** | nenhum nó vivo nem o histórico que ficou citam o hash deles | ninguém mais consegue pedir esse arquivo |

- **Quando a compactação roda:** quando o usuário pede, e com uma sugestão automática quando o lixo passar de **30%** do arquivo. Nunca em silêncio: ela mostra o que vai limpar.
- **Uma pessoa só, sem colaboradores:** a condição "todos já receberam" vale sempre.
- **"Nada se perde sem registro e aviso" continua valendo:** só some o que foi removido, há tempo, sem pendências, e com aviso.

**O que será medido no protótipo:**

- quanto o arquivo cresce com um uso longo, com muitas remoções e muitas trocas de parâmetro;
- quanto encolhe depois da compactação;
- quanto tempo ela leva.

## 4. Ler um nó sem subir até a raiz (pedido do dono)

- **Hoje,** para saber se um nó está visível, a loja sobe pelos pais até a raiz, conferindo a marca de removido.
- **A proposta:** um **índice derivado de "escondidos"**, isto é, os nós removidos e tudo abaixo deles.
  - é reconstruído a partir da Loro ao abrir e depois de cada junção, numa passada só pela árvore;
  - é atualizado na hora quando esta cópia remove, restaura ou move um nó, refazendo só o galho afetado;
  - nunca é escrito de volta no documento: a verdade continua só na Loro (ADR-0021, seção 2).
- **A medida:** um documento com **10.000 níveis de grupos**, um dentro do outro, e mais um documento comum. Compara o tempo de ler um parâmetro do nó mais fundo com e sem o índice, e o custo de reconstruir o índice depois de uma junção.

## 5. Biblioteca nova pedida: `sha2` (resumo SHA-256)

Ela serve para identificar os pedaços dos arquivos grandes pelo conteúdo e para conferir cada registro. A regra "criptografia nunca é caseira" impede escrever um resumo próprio.

| | `sha2` |
| --- | --- |
| Versão proposta (exata) | `=0.11.1` (de 2026-10-12) |
| Licença | MIT OR Apache-2.0 |
| Manutenção | grupo RustCrypto; 96+ pessoas contribuíram, a principal com 45% dos commits; último commit em 2026-10-27; mais de 1,1 bilhão de downloads |
| Pacotes que entram junto | 9: `block-buffer`, `cfg-if`, `const-oid`, `cpufeatures`, `crypto-common`, `digest`, `hybrid-array`, `sha2` e `typenum` |
| Linhas com `unsafe` | 110 no total; no `sha2`, só nas versões aceleradas pelo processador (instruções SHA do x86, NEON do ARM). O código comum é Rust sem `unsafe` |
| Arquivos C ou assembly | **nenhum** |
| `build.rs` / ferramenta que compila C | nenhum / nenhuma |
| Alertas RustSec | RUSTSEC-2021-0100 (resultado errado em processadores ARM na versão 0.9.7, de 2021; corrigido há anos) |
| Alternativas | `blake3` (mais rápida, mas traz C e assembly pelo `cc` por padrão, e tem uma pessoa com 64% dos commits); escrever um resumo próprio (proibido) |

## 6. Testes que o protótipo vai ter (Parte 4, depois da aprovação)

Os testes de disco rodam no **A: (HD)** e no **C: (SSD)**, com os dois resultados registrados:

1. arquivo de 2 GB em pedaços: salva, reabre e sai idêntico (mesmo hash); mudar um parâmetro depois não regrava nenhum pedaço (os bytes dos pedaços no arquivo continuam os mesmos);
2. copiar o arquivo com o programa aberto, em vários momentos de uma gravação: toda cópia abre;
3. segunda abertura: recusada para gravar, aberta só para leitura, com mensagem;
4. o processo é **morto** no meio de uma gravação, muitas vezes, em pontos diferentes: o arquivo sempre reabre;
5. N edições sem "salvar" seguidas da morte do processo: as N edições voltam;
6. tempo de cada gravação forçada ao disco (`sync_data`), no HD e no SSD;
7. pausa da interface no salvamento automático (limite aprovado de 16 ms), medida numa simulação de interface com o salvamento em segundo plano;
8. arquivo cortado e arquivo com bytes trocados: abre o que der, com mensagem, sem cair;
9. migração: falha simulada no meio, e o original fica intacto; arquivo de versão mais nova recusado;
10. crescimento do arquivo e efeito da compactação (seção 3);
11. índice de escondidos com 10.000 níveis (seção 4).

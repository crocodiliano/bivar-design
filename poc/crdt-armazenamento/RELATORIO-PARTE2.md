# Entrega 0.6, Parte 2: comparação Loro × Automerge

Prova de conceito descartável. Este relatório é a base do ADR do CRDT (Parte 3). Data: 2026-10-29.

## O que foi construído

- **Uma "tomada" comum** ([`src/model.rs`](src/model.rs)). São as operações do ADR-0002 (criar e remover nó, conectar e desconectar, mudar parâmetro, reordenar, mover na hierarquia), mais a edição de texto, e um **relatório de junção** com o que o ADR-0019 (decisão 3) exige para avisar:
  - escritas simultâneas no mesmo parâmetro, com os dois valores;
  - movimentos perdidos;
  - nós editados que ficaram escondidos por uma remoção.

  Nenhum tipo das bibliotecas aparece nela.
- **As duas bibliotecas ligadas à mesma tomada:** [`src/loro_doc.rs`](src/loro_doc.rs) e [`src/automerge_doc.rs`](src/automerge_doc.rs).
- **O gerador de carga** ([`src/workload.rs`](src/workload.rs)), com a mistura aprovada pelo dono:

| Operação | Proporção |
| --- | --- |
| Mudar parâmetro | 40% |
| Editar texto | 10% |
| Criar nó | 15% |
| Remover nó | 5% |
| Conectar | 10% |
| Desconectar | 5% |
| Reordenar | 10% |
| Mover na hierarquia | 5% |

  O gerador anota sozinho o que cada cópia quis fazer, sem ler da biblioteca, para conferir as junções contra essa anotação.
- **Os testes** ([`tests/comparison.rs`](tests/comparison.rs)): cada teste é escrito uma vez e roda nas duas bibliotecas.
- **O programa de medição** ([`src/bin/measure.rs`](src/bin/measure.rs)): cada medida roda num processo separado, e a memória é lida do próprio sistema operacional.

Sem `unsafe` no nosso código: a trava `unsafe_code = "forbid"` está ligada.

## Números contra os limites aprovados

Máquina de referência (Ryzen 7 5700X, 16 GB, Windows 11). Os arquivos de 1 milhão de operações ficaram no disco A: (HD mecânico). Cada número é a mediana de 3 execuções, cada uma num processo separado, compilado em `--release`. A saída completa está em [`resultados/medicoes.md`](resultados/medicoes.md).

| Medida | Limite | **Loro** | **Automerge** |
| --- | --- | --- | --- |
| 100.000 operações misturadas | ≤ 5 s | **0,80 s** ✅ | **6,43 s** ❌ (inclui a camada feita por fora; não separada) |
| Junção de 10.000 + 10.000, nos dois sentidos, com o relatório de avisos | ≤ 1 s | **0,20 s** ✅ | **0,97 s** ✅ (por pouco) |
| As duas cópias terminam idênticas | — | sim | sim |
| Abrir 1 milhão de operações, só a biblioteca | ≤ 3 s | 0,015 s ✅ | **3,89 s** ❌ |
| Abrir e ler o documento inteiro, com a camada desta prova | ≤ 3 s | **2,09 s** ✅ | **11,4 s** ❌ |
| Memória a mais ao abrir, só a biblioteca | ≤ 1 GB | 31 MB ✅ | **1,14 GB** ❌ |
| Memória a mais ao abrir e ler tudo, com a camada | ≤ 1 GB | **214 MB** ✅ | **1,14 GB** ❌ |
| Tamanho salvo: 100.000 operações / 1 milhão de operações | só registro | 2,8 MB / 29,6 MB | 1,3 MB / 13,6 MB |
| Tempo para gerar o documento de 1 milhão | só registro | 12,5 s | 94,4 s |

**Detalhes:**

- **"1 milhão de operações"** são 1 milhão de ações do usuário, pela mistura aprovada. O histórico de cada biblioteca conta mais operações (1,48 milhão na Loro, 1,53 milhão no Automerge), porque um texto inserido conta uma operação por letra.
- **A Loro "abre" quase de graça**, porque só decodifica o estado quando ele é usado. Por isso a medida justa é "abrir e ler tudo": 2,09 s, dentro do limite.
- **O Automerge passa dos limites de abertura sozinho**, antes de qualquer código desta prova (3,89 s e 1,14 GB). Não é culpa da versão mínima.
- **Salvar com a interface aberta** fica para a Parte 4. Mesmo assim, salvar as 100.000 operações levou 0,11 s na Loro e 0,07 s no Automerge, fora da interface.

## Carregar só a parte de uma persona

- **Nenhuma das duas** carrega só uma parte de um documento: o documento é uma unidade.
- **A Loro chega perto**, porque guarda cada parte ainda codificada e só a decodifica no primeiro uso. A prova disso são os próprios testes dela, como `snapshot_export_keeps_imported_state_lazy` em `loro-internal/src/state/container_store.rs`. Se cada persona guardar os seus dados em partes ("contêineres") próprias, as partes de uma persona inativa nem chegam a ser decodificadas.
- **O Automerge** decodifica tudo ao abrir.

Isso é uma suspeita bem fundamentada, não medida. Medir é trabalho da Parte 3.

## Comportamentos registrados

- **Desfazer com colaboração.** A muda um objeto, B muda outro, e A desfaz: **nas duas, só a mudança de A some** (teste `undo_only_removes_my_change`).
- **A desfaz algo que B já editou por cima:**
  - **Loro:** o desfazer **passa por cima da edição de B sem aviso**. A cor volta para o valor de antes de A ("green"), e o "blue" de B some. Isso contraria o princípio 5 ("nada se perde sem registro e aviso"). Se a Loro for a escolhida, o desfazer precisa de uma camada que confira antes de desfazer, e o custo dela é avaliado no ADR;
  - **Automerge (versão mínima feita por fora):** o desfazer é pulado e avisado ("color: changed by another copy after this edit"), e o "blue" de B fica.
- **Movimentos que não mudam nada:** a Loro não os grava, e a escrita do mesmo valor que já está lá também não é gravada (nas duas). A versão mínima do Automerge foi ajustada para ignorar movimentos vazios, igual à Loro.
- **Movimentos que formariam ciclo:**
  - **Loro:** ignora o movimento que formaria o ciclo. Uma consequência apareceu no teste: uma remoção que tinha perdido para um movimento **volta a valer** quando esse movimento é anulado por outro;
  - **Automerge (versão mínima):** o movimento mais recente volta para o pai anterior.
- **Cópia para enviar:** "Salvar cópia para enviar" reconstrói o estado visível num documento novo, igual nas duas, sem usar nenhum recurso das bibliotecas.
  - O marcador e o hash da imagem apagada **não aparecem** nem nos bytes do arquivo nem no histórico decodificado.
  - O histórico da cópia só tem a cópia nova como autora.
  - A cópia tem outra identidade, e a junção com o original é **recusada**.
  - O teste de controle confirma que o arquivo original **contém** o marcador e o hash no histórico, nas duas.
  - O aviso de conflitos pendentes antes de gravar fica para a Parte 6, porque depende do painel de conflitos.

## Licenças e segurança da prova

- **`cargo deny check`** passa, com as exceções aprovadas: BSL-1.0 só para o `xxhash-rust`, e os 6 alertas da Loro.
- **`cargo audit`** termina sem erro, com os 6 avisos conhecidos.
- **Uma diferença entre as ferramentas:** o `cargo deny` não encontra os dois alertas de memória insegura (RUSTSEC-2023-0126 e RUSTSEC-2026-0255), mas o `cargo audit` encontra. As exceções ficam na lista mesmo assim.

## Conclusão e recomendação

**Recomendação: Loro.** Ela passa em todos os limites com folga. O Automerge falha em três dos quatro, e falha na abertura do documento grande **sozinho**, sem nenhum código desta prova. Esse é o critério mais difícil de corrigir depois, porque depende da biblioteca, não de nós.

**O que pesa contra a Loro, e vira condição no ADR se ela for escolhida:**

1. **A detecção de escrita simultânea precisa ser feita por nós** (~262 linhas, risco alto). Ela já passa nos 2.000 cenários, mas exige reproduzir a regra de desempate interna da Loro. Se essa regra mudar numa versão nova, a detecção quebra em silêncio. Isso pede um teste que trave a regra.
2. **O desfazer dela passa por cima da edição de outra cópia sem aviso.** Precisa de uma camada própria.
3. **Os problemas da ficha:** licença BSL-1.0, `im`/`sized-chunks` abandonados e com memória insegura, assembly do `psm` e 75% do trabalho numa pessoa só.

**O que vale para as duas, qualquer que seja a escolha:**

- **A detecção de "edição escondida por remoção"** não existe em nenhuma, foi a parte com mais erros, e precisa ser feita pelo Bivar.
- **Os avisos e registros de conflito precisam ser gravados dentro do documento**, para chegarem a todas as cópias (lacuna de três cópias, acima).

**Contingência (ADR-0019, decisão 7):** nenhuma das duas falhou a ponto de reabrir a decisão 2, porque a Loro passa em todos os critérios medidos. A decisão de lançar primeiro uma versão de uma pessoa só é do dono.

## O que foi feito por fora, e quanto custou (pedido do dono)

Para cada recurso que falta numa biblioteca, foi feita uma versão mínima. O tamanho conta as linhas de código, sem linhas vazias nem comentários.

| Recurso | Falta em | Tamanho da versão mínima | Dificuldade | Risco de erro |
| --- | --- | --- | --- | --- |
| **Mover com segurança** (sem duplicar, sem ciclo, com ordem entre irmãos e sabendo qual movimento perdeu) | Automerge | ~363 linhas: registro do movimento, quebra de ciclo, índice da árvore e índice fracionário (36) | **Alta.** É o problema conhecido do "movimento em árvore replicada". A versão mínima guarda "pai, posição e pai anterior" num registro só. Quando dois movimentos formam um ciclo, o mais recente volta ao pai anterior; se isso também falhar, vai para a raiz. Só desfaz ciclos com uma volta; cadeias longas de movimentos cruzados não foram estudadas. Exige um índice da árvore em memória, refeito a cada junção | **Alto.** Dois erros apareceram durante a construção (lista abaixo). A ordem entre irmãos com chaves iguais depende de desempate pelo identificador |
| **Desfazer só o que é seu** | Automerge | ~166 linhas | **Média.** Cada operação local guarda o seu inverso. O desfazer só aplica o inverso se ninguém mudou aquele valor depois. O texto usa "cursores" do Automerge para achar a posição certa depois de edições alheias | **Médio.** Se outra cópia escreveu dentro de um texto inserido, o desfazer do texto é pulado e avisado. Refazer não foi feito |
| **Detectar escrita simultânea no mesmo parâmetro** (e qual movimento perdeu) | Loro | ~262 linhas: leitura do histórico em JSON, cálculo do "passado" de cada operação e comparação dos dois lados | **Alta.** É preciso calcular, operação por operação, o que a outra cópia já tinha visto quando escreveu. Para saber "qual é a última escrita", é preciso reproduzir exatamente a regra de desempate interna da Loro (relógio de Lamport e número da cópia) | **Alto.** Três erros apareceram, entre eles um que escolhia o valor errado como "perdedor" quando havia texto na mesma mudança |
| **Detectar edição escondida por uma remoção** | **as duas** | ~58 linhas, comuns | **Alta.** Nenhuma das duas bibliotecas oferece isso | **Alto.** Cinco erros apareceram, todos achados só pelo teste com milhares de cenários aleatórios |

## Erros encontrados durante a construção

Todos foram achados pelos testes, não por revisão. Isto mede o risco: código assim **só fica confiável com testes de milhares de cenários aleatórios**, e mesmo assim restam casos não cobertos.

| # | Onde | O que acontecia | Como foi achado |
| --- | --- | --- | --- |
| 1 | as duas (comum) | Na segunda direção da junção (B recebendo de A, quando A já tinha as mudanças de B), B não via nada como simultâneo. A regra certa é causal: "o que a outra cópia não tinha visto quando escreveu" | testes de escrita simultânea, movimentos e remoção |
| 2 | Automerge | Um ciclo quebrado só era avisado se o nó estivesse entre os mexidos daquele lado | teste de movimentos cruzados |
| 3 | Automerge | O histórico guarda texto letra por letra. Sem juntar as letras, a busca pelo marcador nunca acharia nada, e o teste da "cópia para enviar" passaria sem provar nada | o **teste de controle** (o arquivo original precisa conter o marcador) |
| 4 | Loro | Índice de movimento dentro da mesma pilha: a Loro conta os irmãos sem o item movido e recusa índices além do fim | gerador de carga |
| 5 | as duas (comum) | Um nó novo de A ficava escondido dentro de um nó que o próprio A apagou, porque B tinha movido o pai dele para lá. A regra "foi A quem apagou, então A sabia" estava errada | 6 cenários aleatórios |
| 6 | as duas (comum) | Com três cópias, "o último movimento do outro lado" mistura o histórico de duas cópias e não quer dizer nada | 100 cenários, 3 cópias |
| 7 | as duas (comum) | Uma remoção que **perdeu** para um movimento ainda era tratada como se valesse | 114 cenários, 3 cópias |
| 8 | Loro | O relógio de cada operação era calculado errado quando havia texto na mesma mudança, e o relatório apontava o valor errado como perdedor | 124 cenários, 3 cópias |
| 9 | as duas (comum) | Um movimento anulado (para evitar ciclo) devolvia um nó para dentro de um nó apagado, sem aviso para o que estava dentro | 2.000 cenários: 2 falhas na Loro e 2 no Automerge |

Também apareceram dois erros **no próprio gerador de testes**: ele anotava como intenção um movimento que não mudava nada, e uma escrita do mesmo valor que já estava lá. As duas bibliotecas ignoram essas operações, então elas não são edições.

## Resultado do teste "nada se perde em silêncio" (achado A12)

Depois das correções, o teste confere cada intenção de cada cópia, anotada pelo gerador. Toda intenção precisa estar no documento final ou no relatório de avisos.

| Cenários aleatórios | Loro | Automerge |
| --- | --- | --- |
| 2.000 com duas cópias (300 operações de base e 150 de cada lado) | 0 falhas | 0 falhas |
| 2.000 com três cópias (100 de cada lado), conferindo a cópia que fez as junções | 0 falhas | 0 falhas |

Registro em [`resultados/contagem-nada-se-perde.txt`](resultados/contagem-nada-se-perde.txt).

**Lacuna conhecida, de desenho e não de biblioteca:** com três cópias, B e C recebem de A mudanças **já juntadas** e não percebem conflitos entre A e C, porque as duas vieram "de fora". Só a cópia que fez a junção avisa. Conclusão para o ADR: **o aviso e o registro precisam ser gravados dentro do próprio documento** pela cópia que detecta, para todas as cópias os receberem. Isso também cumpre o "registro no histórico" do ADR-0019. É preciso um identificador estável para cada conflito, para duas cópias não registrarem o mesmo conflito duas vezes.

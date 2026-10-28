# Auditoria por IA: correções feitas a partir da revisão do ADR-0019 (2026-10-28)

**Escopo:** os 13 commits depois de `b39fb8d`, o commit que trouxe o relatório `docs/auditoria/2026-10-28-revisao-adr-0019.md`. São eles, do mais antigo ao mais novo: `c99b1f9`, `beac26d`, `e56af35`, `06045c6`, `b247588`, `7cebe2b`, `a511b9f`, `36dbd79`, `caca8c4`, `a4c59cb`, `a1e8ca5`, `5395eaa` e `0b061f3`. A faixa foi confirmada com `git log b39fb8d..HEAD`. Os commits só mexem em documentos (`git diff --stat`): nenhum código, nenhum `Cargo.toml`. Não alterei nenhum arquivo.

## Resumo

Os dois achados bloqueantes foram resolvidos.
- **Beco sem saída da Fase 0 (achado 1):** todo achado em aberto ganhou uma entrega, e a 0.4 passou a fechar por último.
- **Contradição da decisão 7 (achado 2):** a versão "de uma pessoa só" agora usa a mesma biblioteca de CRDT (a técnica que junta cópias do documento sem servidor), sem um segundo histórico.

Dos 20 achados, 16 foram corrigidos e 4 só em parte (2, 6, 8 e 16). Nenhum ficou sem tratamento. As correções trouxeram problemas novos de gravidade média, e nenhum é bloqueante. O mais relevante: a biblioteca de CRDT é escolhida na 0.6 sem testar movimentos simultâneos, embora a nova regra dos movimentos dependa dela. O registro de tratamento (`0b061f3`) é fiel no geral, mas apresenta como resolvidos por inteiro os achados 2 e 8, que foram resolvidos só em parte.

## Achado por achado

| # | Situação | Evidência | Observação |
|---|---|---|---|
| 1 (bloqueante) | **Corrigido** | `fase-0.md:66-69`, `:101-107` (A7), `:164-173` (A11 e M15), `:214-219` (M8), `:235-236` (A9 e M13), `:253`; `2026-10-28-fase-0-tratamento.md:60-70` | Conferi os 41 achados da auditoria da Fase 0 (C1–C2, A1–A13, M1–M17, B1–B11): todos aparecem no registro. A divisão do ADR-0014 está prevista. |
| 2 (bloqueante) | **Corrigido**, com uma pendência pequena | `0019.md:80-85` | A contradição acabou: mesma biblioteca e nenhum segundo histórico. Falta uma parte: o "gatilho" definido serve para reabrir a decisão 2, mas ainda não diz quem decide lançar a versão "de uma pessoa só", nem quando. O relatório pedia isso. |
| 3 | **Corrigido** (veja o novo N1) | `0019.md:50-53`; `CLAUDE.md:41`; `fase-0.md:177-178`; `nucleo.md:29` | |
| 4 | **Corrigido** | `0019.md:68-70`; `fase-0.md:120`; `nucleo.md:47-48` | Marcador único, descompressão antes da busca, imagem e hash apagados, aviso no "Salvar como", e a cópia começa um histórico novo. |
| 5 | **Corrigido** | `fase-0.md:127-128`; `nucleo.md:44` | |
| 6 | **Parcialmente** | `nucleo.md:5`, `:28-29`, `:33-35`, `:44-48`, `:94` | Tudo o que o achado listava chegou ao Núcleo. Mas o mesmo problema se repete com critérios novos da 0.6 (veja o novo N3). |
| 7 | **Corrigido** | `0019.md:9`, `:94-108`; `0008.md:9`; `0013.md:9`; `0018.md:9` | Conferi as 11 citações da tabela contra os ADRs: todas existem com essas palavras. |
| 8 | **Parcialmente** | Linhas "Relação" dos ADRs 0002, 0004, 0005, 0007, 0010 e 0018; `fase-0.md:66` | Corrigidas as linhas "Relação" e a lista da 0.4. Não foi tratada a parte do achado sobre os princípios 5 e 6 do `CLAUDE.md:38-43`, que continuam mandando seguir um ADR ainda "Proposto". As correções até ampliaram isso (a linha dos movimentos, `CLAUDE.md:41`). A pendência some quando o ADR-0019 subir de status. |
| 9 | **Corrigido** | `fase-0.md:210-211` | |
| 10 | **Corrigido** | `fase-0.md:133` | |
| 11 | **Corrigido** | `fase-0.md:129-130` | |
| 12 | **Corrigido** | `fase-0.md:177` | |
| 13 | **Corrigido** | `CLAUDE.md:43` × `0018.md:42` | Os números batem. |
| 14 | **Corrigido** | `0019.md:92`; `CLAUDE.md:38` | |
| 15 | **Corrigido** | `0019.md:54-57`; `CLAUDE.md:40` | |
| 16 | **Parcialmente** | `fase-0.md:115`, `:116`, `:121`, `:234`, `:184` | Os critérios da 0.6 e da 0.9 ficaram verificáveis. Mas a checagem de que "só um crate (um módulo do programa) depende da biblioteca de CRDT" vive só na prova, que é descartável. Ela não foi levada ao Núcleo (veja o N3). |
| 17 | **Corrigido**, como encaminhamento | `fase-0.md:116` | O motivo de deixar o Yrs de fora não foi escrito agora; fica para a 0.6. O registro diz isso abertamente. |
| 18 | **Corrigido** | `fase-0.md:166`, `:168-170` | |
| 19 | **Corrigido** | `fase-0.md:84`, `:238` | |
| 20 | **Corrigido** | `fase-0.md:98`; `PLANO.md:85`, `:99`; `CLAUDE.md:129` | Sobraram menções a "SQLite" (veja o N7). |
| Suspeita 5 | **Corrigido** | `0019.md:36`; `fase-0.md:109` | |

## Achados novos introduzidos pelas correções

| # | Gravidade | Onde | Problema | Evidência | Recomendação |
|---|---|---|---|---|---|
| N1 | **Média** | `fase-0.md:116-121` × `0019.md:50-53` | **A biblioteca de CRDT é escolhida na 0.6 sem testar movimentos simultâneos.** A nova regra exige "vence um, com aviso", e para avisar o programa precisa *perceber* que um movimento foi ignorado. Na comparação Loro × Automerge, a detecção só é testada para "o mesmo parâmetro" (`:117`). Os movimentos só são testados na 0.7 (`:177-178`), quando a biblioteca já foi escolhida. Além disso, a frase "na árvore do CRDT, um dos dois movimentos é ignorado" (`0019.md:53`) depende da biblioteca. O próprio ADR-0005 cita a Loro "por ter árvore com movimentação" (`0005.md:18`). Pelo que sei, mas sem conferir nesta sessão, o Automerge não tem uma operação de mover: o movimento vira "apagar e inserir", e isso pode **duplicar** o objeto em vez de fazer "vencer um". | Linhas citadas; o comportamento do Automerge é suspeita. | Na comparação da 0.6, acrescentar o caso "duas cópias movem o mesmo nó / A para dentro de B e B para dentro de A", e exigir que o programa detecte o movimento perdido. No ADR-0019, dizer que a frase sobre a árvore vale para bibliotecas que têm árvore com movimentação. |
| N2 | **Média** | `0019.md:52`; `CLAUDE.md:41`; `fase-0.md:177` | **Não está claro o que "vence um" quer dizer numa reordenação.** "Duas cópias reordenam a mesma pilha" pode querer dizer o mesmo item ou itens diferentes. Se duas pessoas movem itens *diferentes* da mesma pilha, as listas de CRDT costumam aplicar os dois movimentos. Descartar a reordenação inteira de uma delas perderia mais do que o necessário. Os testes da 0.7 e da N.1 vão ser escritos sobre essa regra ambígua. | Texto citado. | O dono decide e o ADR diz se a regra vale por item ou pela pilha inteira. |
| N3 | **Média** | `fase-0.md:101-107`, `:115`, `:125`, `:128-129`, `:133` × `nucleo.md:40-48` | **O mesmo problema do achado 6 aparece em critérios novos.** A 0.6 é descartável, mas estas exigências não têm critério correspondente na N.2 nem na N.1: <br>• a recusa de arquivos feitos para esgotar memória ou tempo (A7); <br>• a checagem de que só um crate depende da biblioteca de CRDT; <br>• o arquivo aberto duas vezes; <br>• a garantia de que os dados chegaram ao disco de verdade; <br>• o salvamento sem pausa perceptível; <br>• a migração do histórico. <br>Elas podem sumir ao virar produto. | Linhas citadas. | Acrescentar esses critérios à N.1 e à N.2. |
| N4 | Baixa | `0b061f3`: `2026-10-28-revisao-adr-0019-tratamento.md:12`, `:18` | **O registro de tratamento é otimista em dois pontos.** No achado 2, apresenta o "gatilho" como se respondesse a "quem dispara a contingência e quando", e não responde. No achado 8, não diz que a parte sobre os princípios 5 e 6 ficou sem tratamento. O resto confere: comparei a lista de arquivos de cada commit com a tabela, e as atribuições batem. | Linhas citadas; `git show --stat` de cada commit. | Ajustar as duas linhas para "em parte". |
| N5 | Baixa | `CLAUDE.md:39`, `:41`; `0019.md:46-50`; `nucleo.md:94` | **Não está claro o que entra no painel de conflitos.** O ADR põe no painel os movimentos e os conflitos estruturais, mas não o "mesmo parâmetro". Já a N.5 diz que o painel mostra "os avisos de 'última escrita vence'". | Linhas citadas. | Dizer no ADR-0019 se o mesmo parâmetro e o raster também entram no painel. |
| N6 | Baixa | `0005.md:20` | **Uma linha do ADR-0005 ficou fora da tabela.** Ela diz que um ciclo depois da junção vira estado de conflito. Agora, o ciclo na hierarquia passou a "vence um". A tabela "O que muda em cada ADR" não lista essa linha. | Texto citado. | Acrescentar a linha à tabela. |
| N7 | Baixa | `PLANO.md:16`, `:422`; `adr/README.md:30`; `0019.md:27`; `fase-0.md:51` | **Sobraram textos desatualizados.** <br>• O PLANO (versão oficial) ainda resume o princípio como "duas pessoas mudam o mesmo valor", sem os movimentos. <br>• O índice ainda chama o ADR-0010 de "Arquivo de projeto em SQLite". <br>• A decisão 1 do ADR-0019 ainda fala no "limite de tamanho do SQLite". <br>• A 0.3 ainda diz "O ADR-0007 fica como está". | Linhas citadas. | Alinhar esses textos, ou anotar que são históricos. |
| N8 | Baixa | `fase-0.md:120`; `nucleo.md:47` | **O teste da cópia sem histórico ainda pode deixar passar um erro.** <br>• Se o marcador for digitado letra por letra, entre outras edições, ele pode ficar guardado em pedaços separados. Aí a busca não o encontra, mesmo com o histórico ainda lá dentro. <br>• O "ou salvo com a compressão desligada" testa um caminho diferente do usado de verdade. | Texto citado. | Inserir o marcador de uma só vez e conferir também, pela própria biblioteca, que o histórico da cópia não tem operações anteriores. Preferir a descompressão. |

## Suspeitas (sem evidência suficiente)

- A falta de uma operação de mover no Automerge (N1) vem do meu conhecimento, sem consulta à documentação nesta sessão.
- **O aviso de "última escrita vence" pode sumir.** Esse aviso depende do histórico. A "Salvar cópia para enviar" e a limpeza dos arquivos grandes podem apagar avisos pendentes e o "valor que perdeu". O próprio registro admite que isso "ainda [está] sem critério próprio" (`revisao-adr-0019-tratamento.md:39`). Talvez a cópia deva avisar quando ainda houver conflitos ou avisos sem resolver.
- **A divisão do ADR-0014** (`fase-0.md:67`) prevê registrar uma justificativa no próprio ADR-0014. Isso é mexer num ADR existente, e o `CLAUDE.md:30` diz que mudar um ADR exige um ADR novo. Até aqui o projeto edita ADRs ainda "Proposto" (por exemplo, no tratamento do A13), então é coerente com a prática, mas a regra escrita é ambígua.

## O que NÃO foi verificado

- As decisões atribuídas ao dono nos achados 2, 3 e 4 e na suspeita 5: não há registro delas no repositório, só a afirmação do registro de tratamento.
- Nada online (Loro, Automerge, Yrs, SQLite, Vello).
- Não rodei `cargo fmt`, `clippy`, `test`, `deny` nem `audit`, porque os commits não tocam em código nem em dependências.
- Não reauditei o plano inteiro. Também não verifiquei se M11 e M12 ("critério da 0.6", hoje na 0.8) estão descritos corretamente no registro antigo.

## Respostas diretas

**Ainda resta algo BLOQUEANTE? Não.** Os achados 1 e 2 estão resolvidos. O que sobra é de gravidade média (N1, N2, N3) ou baixa.

**O ADR-0019 pode subir para "Revisado por IA"?** Do lado do auditor, **sim, está liberado**. Nenhum bloqueante resta, e os achados 3, 7 e 8, que eu tinha posto como condição, foram tratados (o 8 em parte, e a parte que falta se resolve com a própria subida de status). Pela regra do `CLAUDE.md`, ainda faltam duas coisas:
1. O dono **decidir os achados novos que tocam o texto do ADR**: o N2 (o que "vence um" quer dizer numa reordenação) e a frase da árvore no N1. Pode corrigir ou registrar uma justificativa. O N3 e o resto do N1 podem ir para os critérios da `fase-0.md` e da `nucleo.md`, sem mexer no ADR.
2. **A aprovação explícita do dono.** Só então a linha "Status" muda, e as linhas "Relação" dos ADRs 0002, 0004, 0005, 0007, 0008, 0010, 0013 e 0018 passam da marcação provisória para a definitiva.

"Aceito" continua dependendo de uma revisão humana.

Arquivos principais:
- `a:\Projetos\Bivar\docs\adr\0019.md`
- `a:\Projetos\Bivar\CLAUDE.md`
- `a:\Projetos\Bivar\docs\fases\fase-0.md`
- `a:\Projetos\Bivar\docs\fases\nucleo.md`
- `a:\Projetos\Bivar\docs\auditoria\2026-10-28-revisao-adr-0019-tratamento.md`
- `a:\Projetos\Bivar\docs\auditoria\2026-10-28-fase-0-tratamento.md`
- `a:\Projetos\Bivar\docs\adr\0005.md`
- `a:\Projetos\Bivar\docs\PLANO.md`

**Esta auditoria foi feita por IA e não substitui uma revisão humana.**

# Tratamento dos achados: revisão das correções do ADR-0019 (2026-10-28)

Acompanha o relatório [`2026-10-28-revisao-correcoes-adr-0019.md`](2026-10-28-revisao-correcoes-adr-0019.md), que fica como foi escrito. Esta página diz o que foi feito com cada achado e em qual commit.

**Situação em 2026-10-28:** os 8 achados novos (N1 a N8), a pendência do achado 2 e as duas suspeitas que pediam decisão foram tratados. Em N1, N2 e N5, na pendência do achado 2 e nas duas suspeitas, o tratamento segue as decisões do dono. Em N3, N4, N6, N7 e N8, segue a recomendação do auditor. Depois disso, o dono aprovou o ADR-0019 como "Revisado por IA", sem nova rodada de auditoria.

## Tratados

| Achado | O que foi feito | Commit |
| --- | --- | --- |
| N1 | Decisão do dono: no ADR-0019, a frase da árvore vale para bibliotecas com árvore com movimentação. A comparação Loro × Automerge da 0.6 testa "duas cópias movem o mesmo nó" e "A para dentro de B enquanto B para dentro de A", e exige que o programa detecte o movimento perdido sem duplicar o objeto. | `c09471b` |
| N2 | Decisão do dono: a regra vale por item. Mesmo item movido por duas cópias: vence um, com aviso. Itens diferentes da mesma pilha: os dois movimentos valem. Está no ADR-0019, no princípio 5, nos testes da 0.7 e na N.1. | `c09471b` |
| N3 | A N.1 ganhou a checagem de que só um crate depende da biblioteca CRDT. A N.2 ganhou: arquivo feito para esgotar recursos, mesmo arquivo aberto duas vezes, gravação no disco de verdade, salvamento sem pausa perceptível e migração do histórico. | `1dfda08` |
| N4 | No registro anterior, os achados 2 e 8 passaram para "em parte", com o que faltou em cada um. | este commit |
| N5 | Decisão do dono: o painel de conflitos mostra tudo, em duas seções. "Conflitos" exigem ação; "Avisos" (o mesmo parâmetro e o raster) são informativos e podem ser dispensados sem apagar o registro no histórico. Está no ADR-0019, no princípio 5, na 0.7 e na N.5. | `55073c4` |
| N6 | A tabela "O que muda em cada ADR" ganhou a linha da validação pós-junção do ADR-0005: vale para as conexões entre nós; o ciclo na hierarquia segue "vence um". | `0a5c9cf` |
| N7 | O PLANO resume o princípio com os movimentos; o índice dos ADRs e a tabela do PLANO anotam que o formato do ADR-0010 é escolhido na 0.6; a decisão 1 do ADR-0019 não fala mais só do SQLite; a frase "O ADR-0007 fica como está" da 0.3 foi marcada como histórica. | `15850e9` |
| N8 | O marcador é inserido de uma só vez; a cópia é descomprimida do mesmo jeito que o programa a lê (saiu a "compressão desligada"); a biblioteca CRDT confere que o histórico da cópia não tem operações anteriores. Está no ADR-0019, na 0.6 e na N.2. | `2884cc9` |
| Achado 2 (pendência) | Decisão do dono: quem dispara a contingência é o dono; os momentos de avaliar são o fim da 0.6 e o fim da N.1, cada um com um item na lista da entrega. | `13a7e5a` |
| Suspeita: aviso perdido na cópia | Decisão do dono: "Salvar cópia para enviar" avisa quando ainda houver conflitos ou avisos sem resolver. Está no ADR-0019, na 0.6 e na N.2. | `2884cc9` |
| Suspeita: regra dos ADRs | Decisão do dono: ADR "Proposto" pode ser editado diretamente; só "Revisado por IA" e "Aceito" exigem um ADR novo para mudar. Está no `CLAUDE.md` e no índice dos ADRs. | `6530cec` |

## Subida de status

O dono aprovou o ADR-0019 como "Revisado por IA" (commit `ea53d14`). As linhas "Relação" dos ADRs 0002, 0004, 0005, 0007, 0008, 0010, 0013 e 0018 passaram da marcação provisória para a definitiva, e os princípios 5 e 6 do `CLAUDE.md` deixaram de citar o ADR-0019 como "proposto". Com isso se resolve também a parte do achado 8 que faltava. "Aceito" continua dependendo de uma revisão humana.

## Suspeitas que seguem para as entregas

| Suspeita | Onde é verificada |
| --- | --- |
| O Automerge não teria uma operação de mover, e o movimento poderia duplicar o objeto | 0.6 (comparação Loro × Automerge, caso dos movimentos simultâneos). |
| Limpeza dos arquivos grandes apagando o "valor que perdeu" | 0.6 (riscos de armazenamento, limpeza dos arquivos grandes). |

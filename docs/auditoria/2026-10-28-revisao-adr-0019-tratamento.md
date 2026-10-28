# Tratamento dos achados: revisão do ADR-0019 (2026-10-28)

Acompanha o relatório [`2026-10-28-revisao-adr-0019.md`](2026-10-28-revisao-adr-0019.md), que fica como foi escrito. Esta página diz o que foi feito com cada achado e em qual commit.

**Situação em 2026-10-28:** os 20 achados e a suspeita 5 foram tratados. Nos achados 2, 3 e 4 e na suspeita 5, o tratamento segue as decisões do dono. Nos demais, segue a recomendação do próprio auditor. Nenhum status de ADR mudou: o ADR-0019 continua "Proposto" até uma nova revisão do agente auditor e a aprovação do dono.

## Tratados

| Achado | O que foi feito | Commit |
| --- | --- | --- |
| 1 | Todo achado em aberto ganhou entrega: A7 na 0.6; A11 dividido (identidade e autoria, itens a–f, num ADR novo na 0.7; transporte e criptografia, item g, no ADR-0014, para a Fase 2); M15 na 0.7; M8 na 0.8; A9 e M13 na 0.9. A `fase-0.md` diz que a 0.4 é a última entrega a fechar, depois da 0.9. O registro de tratamento da Fase 0 foi atualizado. | `36dbd79` |
| 2 | Decisão do dono, opção (a): a versão de uma pessoa só usa a mesma biblioteca CRDT, sem rede. A contingência adia apenas rede, assinaturas, permissões, presença e interface de colaboração. Gatilho: se nem Loro nem Automerge passarem na 0.6, a decisão 2 é reaberta com um ADR novo. | `c99b1f9` |
| 3 | Decisão do dono: movimentos e reordenações simultâneos seguem a regra do mesmo parâmetro. Vence um, com aviso visível, registro no histórico e registro no painel de conflitos. Está no ADR-0019, no princípio 5 e nos testes da 0.7. | `beac26d` |
| 4 | Decisão do dono: "Salvar como" num arquivo com histórico mostra um aviso discreto sugerindo "Salvar cópia para enviar"; o salvamento normal não muda. A cópia começa um histórico novo e não se junta mais com o original. O teste usa um marcador único, descomprime antes da busca e confere que nenhum pedaço nem o hash de uma imagem apagada fica na cópia. | `e56af35` |
| 5 | 0.6: N edições sem salvar voltam depois de matar o processo, e o ADR diz o que é forçado ao disco e quando. | `caca8c4` |
| 6 | O Núcleo lista o ADR-0019, troca "Arquivo SQLite" por "formato escolhido na 0.6" e ganha critérios na N.1 (aviso da última escrita, movimentos simultâneos), na N.2 (edições desde o último salvamento, pedaços, arquivo adulterado, cópia para enviar, aviso no "Salvar como") e na N.5 (painel de conflitos). | `a1e8ca5` |
| 7 | O ADR-0019 ganhou a tabela "O que muda em cada ADR", cobrindo 0002, 0004, 0005, 0007, 0008, 0010, 0013 e 0018. Os ADRs 0008 e 0013 foram marcados, e a linha "Relação" do ADR-0018 deixou de se contradizer. | `b247588` |
| 8 | As linhas "Relação" passaram a dizer "proposta de substituição em parte", até o ADR-0019 chegar a "Revisado por IA". O ADR-0019 entrou na lista da 0.4. | `b247588`, `36dbd79` |
| 9 | 0.8: o ADR do papel do Vello trata as misturas em luz linear e o perfil de cor do monitor. | `a4c59cb` |
| 10 | 0.6: o ADR diz como as operações antigas do histórico são lidas depois de uma migração e o que acontece numa junção entre versões diferentes. | `caca8c4` |
| 11 | 0.6: tempo de salvamento medido com a interface aberta, com limite numérico fixado no plano da entrega; o ADR diz quando um arquivo grande sem uso pode ser apagado. | `caca8c4` |
| 12 | 0.7: teste de junção "duas pessoas pintam no mesmo bloco de raster". | `a4c59cb` |
| 13 | Princípio 6: a cena inteira (30 a 35%, máxima de 45 a 49, média de cerca de 1,2) e o texto a 800% (máxima de 91) aparecem separados. | `7cebe2b` |
| 14 | A decisão 8 do ADR-0019 manda o ponto "limite do renderizador" para o princípio 5, que passou a citar também o ADR-0001 e o ADR-0005. | `beac26d` |
| 15 | O ADR-0019 define "duas pessoas" (duas cópias: pessoas, computadores ou scripts), "ao mesmo tempo" e "a última" (relógio lógico do CRDT, nunca a hora do computador). | `beac26d` |
| 16 | 0.6: só um crate depende da biblioteca CRDT, com checagem automática; limites de carregamento e velocidade fixados no plano; as 100.000 operações misturam todos os tipos do ADR-0002. 0.9: tamanho das regiões e folga fixados a partir de uma medição. 0.7: o teste de acessibilidade do painel de conflitos foi para a N.5. | `caca8c4`, `a4c59cb`, `a1e8ca5` |
| 17 | 0.6: o ADR do CRDT registra por que o Yrs ficou fora, com o motivo conferido na documentação dele, ou o inclui na comparação. O motivo não foi escrito agora porque não foi conferido. | `caca8c4` |
| 18 | 0.7: a biblioteca de assinatura passa pelo processo de dependências e pela aprovação de criptografia em ADR. Os itens (d), (e) e (f) do A11 foram para o ADR de identidade e autoria da 0.7. | `a4c59cb`, `36dbd79` |
| 19 | A 0.5 e a 0.9 passaram a exigir o status "Revisado por IA", como a 0.6, a 0.7 e a 0.8. | `5395eaa` |
| 20 | 0.6: se a escolha for SQLite com proteções, o ADR vem com uma emenda ao ADR-0011 e ao CLAUDE.md §4. A pedido do dono, o PLANO e o §9 do CLAUDE.md deixaram de dizer "SQLite" e passaram a dizer "formato escolhido na 0.6". | `caca8c4`, `a511b9f` |
| Suspeita 5 | Decisão do dono: o grafo é o modelo lógico do documento e o CRDT é onde ele é armazenado e sincronizado. Está no ADR-0019 (decisão 2) e na 0.6, e o princípio 1 continua valendo. | `06045c6` |

## Suspeitas que seguem para as entregas

| Suspeita | Onde é verificada |
| --- | --- |
| Comportamento da Loro e do Automerge na escrita simultânea, nos movimentos e na compressão | 0.6 (comparação Loro × Automerge) e 0.7 (testes de junção). |
| A cópia sem histórico não se junta com o original | Respondida pelo dono no achado 4: a cópia começa um histórico novo e não se junta mais. |
| O aviso da "última escrita vence" depende do histórico | Em parte respondida pelo dono na revisão seguinte: "Salvar cópia para enviar" avisa quando ainda houver conflitos ou avisos sem resolver (ADR-0019, decisão 5; critérios na 0.6 e na N.2). A limpeza dos arquivos grandes segue para o plano da 0.6. |
| Igualdade exata do vello_cpu entre Windows e Linux | 0.5 (critério "mesma imagem no PC do dono e no GitHub"). |

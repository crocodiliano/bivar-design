# Tratamento dos achados: auditoria da Fase 0 (2026-10-28)

Acompanha o relatório [`2026-10-28-fase-0.md`](2026-10-28-fase-0.md), que fica como foi escrito. Esta página diz o que foi feito com cada achado e em qual commit.

**Situação em 2026-10-28:** foram tratados os achados que não exigem decisão de arquitetura. Os achados C1, C2 e A1 a A12 esperam decisões do dono. M8, M13 e M15 também seguem em aberto.

## Tratados

| Achado | O que foi feito | Commit |
| --- | --- | --- |
| A13 | Toda frase que mandava o auditor decidir passou a dizer: "proposto no plano da entrega, revisado pelo agente auditor e decidido pelo dono" (ADRs 0008, 0013, 0017 e 0018, e PLANO). | `b0fb423` |
| M1 | A folga da placa de vídeo caiu de 16 para 3 pixels, o pior caso medido. Isso revelou que o teste que compara duas montagens da mesma cena difere sempre em 5 pixels; ele ganhou uma folga própria e medida (5 + 3). A N.3 e o PLANO foram alinhados. | `d5cee02` |
| M2 | Commits que mexem em `tests/golden/` precisam da linha `Golden-Approved-By:` com o e-mail do dono. A checagem roda no gancho local e na compilação automática. | `7069582` |
| M3 | O tokio foi atualizado de 1.54.0 para 1.54.1 nas duas provas. `cargo deny` e `cargo audit` voltaram a passar, e os relatórios agora trazem as datas. | `523c87b` |
| M4 | O relatório e o ADR-0018 registram o que o código mede e o que não mede. As colunas "trabalho da placa" e a separação "montar a vista / chamada do Vello" viraram estimativas não verificadas. A N.3 ganhou um critério de medição completa. | `055c961` |
| M5 | O CLAUDE.md (princípio 6) e o ADR-0018 registram a fração de pixels diferentes e a diferença máxima entre o modo sem placa e a placa. | `7b2acb9` |
| M6 | O critério dos 50.000 degradês passou a ser "desenha corretamente ou mostra aviso visível". | `a4d4a5b` |
| M7 | `[workspace.lints]` com `unsafe_code = "deny"`, herdado pelos 10 crates. | `665c3c3` |
| M9 | O ADR-0017 explica por que o Slint não foi testado. A explicação foi conferida na documentação oficial do Slint. | `67f1951` |
| M10 | O repositório é público de forma definitiva. PLANO, roadmap e Fase 1a foram corrigidos. | `7482e0a` |
| M11 | Critério de cor da 0.6: intenção colorimétrica relativa, referência feita com um motor de cor diferente e definição objetiva de "dentro da gama". | `f634fb3` |
| M12 | Critério de scripts da 0.6: limites de memória e de pilha, sem derrubar o programa. | `f634fb3` |
| M14 | README, ADR-0016, PLANO e CLAUDE.md agora dizem: "comportamento testado pelo dono; código revisado por IA e por ferramentas automáticas". | `f26dbd0` |
| M16 | O ADR-0015 separa "verificar compatibilidade" de "tornar compatível". | `72151f4` |
| M17 | Conferido na documentação do Graphite: ele tem grafos aninhados e camadas como vista do grafo. O ADR-0001 e o PLANO foram corrigidos. | `8dfd611` |
| B1 | Critérios da 0.1 marcados, depois do teste do dono, e LEIA-ME-PRIMEIRO atualizado. | `f9ce0b7` |
| B2 | Campo "Prioridade" preenchido nos 18 ADRs. | `f9ce0b7` |
| B3 | A prova do renderizador passou a usar o pollster 1.0, e a duplicata sumiu. | `bff5873` |
| B4 | Contagem real no Windows (139 crates) registrada no ADR-0017. A compilação automática impede que `bivar-core` e `bivar-doc` dependam da interface ou da placa de vídeo. | `70cf93e` |
| B5 | Duas versões do vello_cpu aceitas pelo dono até o egui atualizar (ADR-0018). | `7427553` |
| B6 | OFL saiu do `deny.toml`. O CLAUDE.md lista à parte OFL (só fontes) e CC0 (só dados, como HDRI). | `8eaa0ba` |
| B7 | O terminal do auditor fica liberado: o dono aprova cada comando no modo Manual. Isso está registrado em `auditor.md`. | `7427553` |
| B8 | Canvas azulado da prova registrado no ADR-0017. O produto usa cinza neutro. | `7427553` |
| B9 | O critério de desempenho da 1a ganhou um número: 99% dos quadros abaixo de 16,7 ms. | `7427553` |
| B10 | Critério novo na N.3: sem DirectX 12 nem Vulkan, o programa mostra uma mensagem clara em vez de cair. | `7427553` |
| B11 | Critério novo na N.3: teste automático da tela contra a exportação. | `7427553` |

## Em aberto

| Achado | Por quê |
| --- | --- |
| C1, C2, A1 a A12 | Dependem de decisões de arquitetura do dono. |
| M8 | Pede um ADR novo sobre dependências (cargo-vet, `build.rs`, versões fixas na compilação automática). |
| M13 | Pede investigar a queda dos testes de placa em paralelo e reportá-la ao wgpu. |
| M15 | Pede uma regra sobre a confiança na imagem guardada (cache/proxy) das personas. |

**Status dos ADRs:** nenhum mudou. Subir um ADR para "Revisado por IA" exige que todos os achados que o bloqueiam estejam tratados e que o dono aprove (veja a tabela "Status recomendado dos ADRs" no relatório).

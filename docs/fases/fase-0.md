# Fase 0 — Fundações

**Objetivo:** provar que as escolhas técnicas de maior risco funcionam na máquina de referência e deixar o repositório pronto para o trabalho seguro com o Claude Code.

**Máquina de referência:** Windows 11, Ryzen 7 5700X, 16 GB de RAM, GTX 1660 (6 GB), mesa digitalizadora com pressão.

## Entregas

### 0.1 Repositório e regras

- **Repositório público no GitHub.** Revisores voluntários da comunidade podem entrar como colaboradores mais tarde.
- **Arquivos-base:**
  - `CLAUDE.md`, `docs/adr/`, `docs/fases/` e `docs/PLANO.md`;
  - `LICENSE` (GPL-3.0-or-later);
  - `README.md` com o aviso "feito com auxílio de IA".
- **Workspace Cargo vazio,** com a estrutura de crates proposta no CLAUDE.md.
- **Checagens configuradas:** `rustfmt`, `clippy`, `cargo deny` e `cargo audit`.
- **Compilação automática só em Linux,** a cada envio e a cada pedido de junção (*pull request*). Repositórios públicos não gastam cota de minutos.

**Critérios de aceite:**
- [x] O dono consegue clonar o repositório e rodar `cargo test` no PC dele, com uma mensagem de sucesso.
- [x] Um commit sem `Signed-off-by` é sinalizado.
- [x] Adicionar uma dependência de teste com licença incompatível faz o `cargo deny` falhar.

### 0.2 Prova de conceito da interface (ADR-0009)

Construa um programa mínimo, descartável, com **dois painéis e um canvas vazio**, primeiro em **egui**. Se ele falhar em algum item, repita em **Slint**.

**Critérios de aceite (o dono testa cada um):**
- [x] **Estilo:** os painéis reproduzem o espírito do mockup: fundo escuro, cantos arredondados, tipografia limpa e roxo como destaque. O dono aprova visualmente.
- [x] **Janela separada:** um painel é arrastado para fora da janela principal, vira uma janela própria do Windows e pode ir para um segundo monitor. Depois, volta a se acoplar.
- [x] **Leitor de tela:** com o NVDA ligado, navegar pelos painéis com Tab faz o NVDA ler o nome de cada controle.
- [x] **Teclado:** todos os controles podem ser alcançados e usados sem mouse, com o foco sempre visível.
- [x] **Caneta (pressão):** desenhar no canvas com a mesa digitalizadora mostra na tela, em tempo real, a pressão (0–1).
- [ ] **Caneta (inclinação e borracha):** a inclinação e se a ponta borracha está em uso. **Fora da prova por decisão do dono:** exigiriam código `unsafe`, que não foi permitido nesta prova. Veja a limitação 1 do [ADR-0017](../adr/0017.md).
- [x] **Escala:** com a escala do Windows em 150%, a interface fica nítida e proporcional.

**Resultado esperado:** o framework escolhido e as limitações encontradas são registrados no [ADR-0017](../adr/0017.md), que passa pela revisão do agente auditor (entrega 0.4). O ADR-0009 fica como está.

### 0.3 Prova de conceito do renderizador (ADR-0007)

Um programa mínimo que desenha com **Vello sobre wgpu** na GTX 1660.

**Critérios de aceite:**
- [x] Renderiza 10.000 retângulos com cantos arredondados, traçado e degradê, com navegação (zoom e pan) fluida, sem travadas perceptíveis. **Com ressalva, aceita pelo dono:** a 144 Hz, passa com 256 degradês diferentes, mas não com um degradê diferente por retângulo. Veja a limitação 1 do [ADR-0018](../adr/0018.md).
- [x] Renderiza texto com uma fonte OTF em vários tamanhos, nítido no zoom.
- [x] A mesma cena exportada como PNG é idêntica pixel a pixel ao que aparece na tela, no mesmo zoom (primeiro teste visual). **Com tolerância, aceita pelo dono:** raramente 1 a 3 pixels diferem em 1 nível de 255. Veja a limitação 3 do [ADR-0018](../adr/0018.md).
- [x] A mesma cena renderiza também **sem GPU**, no modo CPU, para PNG.
- [x] O Vello funciona com a mesma versão do wgpu usada pelo eframe 0.37 (wgpu 30).

**Resultado esperado:** o resultado é registrado no [ADR-0018](../adr/0018.md), que confirma o ADR-0007 com ressalvas e passa pela revisão do agente auditor (entrega 0.4). O ADR-0007 fica como está.

> **Auditoria:** por enquanto não há auditor humano. As revisões são feitas pelo agente auditor (`.claude/agents/auditor.md`), por ferramentas automáticas e por voluntários da comunidade. Veja a seção 2 do `CLAUDE.md`.

### 0.4 Revisão por IA dos ADRs

Revisão por IA dos ADRs 0001 a 0018, com três partes:

- **o agente auditor** (`.claude/agents/auditor.md`) revisa todos os ADRs;
- **uma segunda opinião** de uma IA de outra empresa, para reduzir os pontos cegos de IAs parecidas;
- **um pedido público de revisão** à comunidade Rust.

**Critérios de aceite:**
- [ ] O relatório do agente auditor está salvo em `docs/auditoria/`.
- [ ] Todo achado de gravidade Crítica ou Alta foi tratado: ou corrigido, ou com justificativa registrada.
- [ ] Os ADRs de prioridade Alta estão com status "Revisado por IA": 0001, 0002, 0003, 0004, 0005, 0007, 0008, 0009, 0010, 0011, 0014, 0017 e 0018. A prioridade de cada um está no [índice dos ADRs](../adr/README.md).

### 0.5 Prova de conceito do motor de texto

A prova de conceito da 0.3 desenhou texto letra por letra, sem *shaping* (a montagem das letras de acordo com as regras da fonte). Esta prova, também **descartável**, testa um motor de texto em Rust, desenhando com o Vello. A biblioteca é proposta no plano da entrega, com licença, manutenção e alternativas, e precisa da aprovação do dono.

**Regra da prova:** nenhum `unsafe` no nosso código.

**Critérios de aceite:**
- [ ] **Acentos em português:** o texto "Açaí à brasileira — ótimo, pão, avô, ação" aparece com todos os acentos no lugar certo. Isso vale para as letras acentuadas prontas (como "á") e para as montadas com acento separado (letra "a" seguida do acento agudo), que devem ficar iguais.
- [ ] **Kerning:** em pares como "AV", "To" e "Ty", o espaço entre as letras segue o ajuste definido na fonte, e não só a largura de cada letra. Um teste confere a posição contra os valores da fonte.
- [ ] **Ligaduras:** com uma fonte que as tem, "fi" e "fl" viram o desenho único da ligadura. Elas podem ser desligadas, e então as letras voltam a aparecer separadas.
- [ ] **Formatos de fonte:** funciona com uma fonte OTF (contornos CFF) e com uma TTF, as duas com licença OFL.
- [ ] **Testes visuais:** o dono aprova as imagens de referência dos quatro critérios acima, desenhadas pelo Vello na placa de vídeo e pelo vello_cpu.
- [ ] **Mesma imagem no PC do dono e no GitHub:** as imagens sem placa de vídeo (vello_cpu) saem idênticas, pixel a pixel, no PC do dono (Windows) e na compilação automática do GitHub (Linux). Veja "Testes visuais: onde rodam", abaixo. Se não saírem, o Claude Code para e mostra a diferença ao dono, que decide o que fazer.
- [ ] **Registro:** o resultado e a biblioteca escolhida ficam num ADR novo, revisado pelo agente auditor.

### 0.6 Armazenamento e sincronização

Como o documento é guardado no disco e como duas cópias se juntam. Segue as decisões do [ADR-0019](../adr/0019.md): arquivo único com os arquivos grandes dentro, em pedaços; um único histórico de operações, o do CRDT; e "Salvar cópia para enviar", sem histórico. Trata os achados C1, A1, A5 e A6 da [auditoria da Fase 0](../auditoria/2026-10-28-fase-0.md).

As escolhas ficam registradas em **ADRs novos**, com as alternativas comparadas em licença, manutenção, quantidade de código `unsafe` ou em C, tamanho e segurança. Os testes são provas de conceito **descartáveis**.

**Critérios de aceite:**
- [ ] **Formato por dentro do arquivo (achado C1):** um ADR escolhe entre:
  - (a) SQLite com proteções obrigatórias: modo defensivo, `trusted_schema` desligado, verificação das células, carregamento de extensões desligado, limites de tamanho, `quick_check` ao abrir, recusa de tabelas, gatilhos e visões fora da especificação, e validação de todo o conteúdo como se viesse da rede;
  - (b) SQLite aberto num processo isolado, para arquivos de terceiros;
  - (c) um formato próprio com leitor 100% em Rust.

  Na prova: um arquivo com um gatilho ou uma visão injetada é recusado com uma mensagem clara, e o carregador ganha um alvo de fuzzing (ADR-0011).
- [ ] **Onde mora a verdade do documento (achado A1):** o ADR do CRDT diz:
  - se o documento vive na biblioteca CRDT ou no `bivar-core`, e como os dois ficam iguais. Em qualquer caso, o grafo é o modelo lógico do documento e o CRDT é onde ele é armazenado e sincronizado (ADR-0019, decisão 2), para o princípio 1 continuar valendo;
  - como cada nó é identificado de forma única entre computadores;
  - como cada tipo de operação vira estrutura CRDT: árvore para a hierarquia, lista para a pilha, mapa para os parâmetros, e as conexões entre nós, que não formam árvore;
  - a versão do formato das operações.

  Um protótipo modela a cadeia Forma → Traçado → Preenchimento → Saída, com as conexões.
- [ ] **Sistema de operações independente da biblioteca CRDT** (ADR-0019, plano de contingência): no protótipo, só uma camada pequena fala com a biblioteca. O resto usa as operações do ADR-0002, sem tipos da biblioteca.
- [ ] **Comparação Loro × Automerge.** As duas rodam a mesma prova, na máquina de referência, e o ADR registra os números e escolhe uma:
  - **escrita simultânea:** duas cópias mudam o mesmo parâmetro ao mesmo tempo. Depois da junção, o programa sabe que houve escrita simultânea e conhece os dois valores, para avisar e registrar no histórico (ADR-0019). Se a biblioteca não oferecer isso pronto, o ADR mede o custo de fazer por fora;
  - **desfazer em colaboração:** A muda um objeto, B muda outro, e A desfaz. Só a mudança de A some. O ADR registra também o que acontece quando A desfaz algo que B já editou por cima;
  - **carregamento:** tempo e memória para abrir um documento com 1 milhão de operações no histórico. O ADR diz também se dá para carregar só a parte de uma persona;
  - **remoção do histórico (achado A5; ADR-0019, decisão 5):** um texto com um marcador único e uma imagem são apagados, e depois vem "Salvar cópia para enviar". O arquivo da cópia é descomprimido antes da busca (ou salvo com a compressão desligada), e o marcador não aparece em nenhum byte. Nenhum pedaço da imagem apagada, nem o hash dela, está na cópia. A cópia abre com um histórico novo e não se junta com o original;
  - **velocidade:** 100.000 operações num documento de teste, e a junção de duas cópias que receberam 10.000 operações diferentes cada uma, ao mesmo tempo. São medidos o tempo, a memória e o tamanho salvo, e as duas cópias terminam idênticas.
- [ ] **Arquivos grandes em pedaços** (ADR-0019; achado A6): um arquivo de teste de 2 GB, maior que o limite de 1 GB por valor do SQLite, é guardado em pedaços, salvo, reaberto e sai idêntico (mesmo hash). Mudar um parâmetro e salvar não regrava nenhum pedaço.
- [ ] **Riscos de armazenamento (achado A6).** O ADR de armazenamento trata de cada um, e a prova testa:
  - **arquivo único de verdade:** copiar o arquivo com o programa aberto gera uma cópia completa, ou o ADR explica como isso é evitado (por exemplo, os arquivos `-wal` e `-shm` do SQLite);
  - **mesmo arquivo aberto duas vezes:** a segunda abertura é recusada ou abre só para leitura, com uma mensagem clara;
  - **queda no meio da gravação:** o processo é morto durante o salvamento, e o arquivo reabre com a última versão salva;
  - **arquivo corrompido ou cortado:** abre o que for possível ou mostra uma mensagem clara. O programa nunca cai;
  - **migração de versão:** guarda uma cópia antes; uma falha no meio não estraga o original; um arquivo de versão mais nova é recusado com mensagem, nunca "consertado";
  - **pastas sincronizadas** (OneDrive, unidades de rede): o ADR diz o que o programa faz, por exemplo um aviso ao abrir;
  - **criptografia opcional:** o ADR diz como será feita sem biblioteca paga (CLAUDE.md §5), ou adia a decisão com justificativa;
  - **especificação aberta:** o ADR diz como a especificação em `docs/formato/` fica legível sem depender do formato interno da biblioteca CRDT, ou registra essa dependência.
- [ ] **Revisão:** os ADRs novos estão com status "Revisado por IA".
- [ ] **Licenças:** `cargo deny check` e `cargo audit` passam nas provas.

### 0.7 Especificação do modelo de dados, com protótipo

O Núcleo vai implementar o grafo na entrega N.1. Antes disso, as regras do modelo de dados precisam estar escritas e testadas num protótipo **descartável**, construído sobre o resultado da 0.6. Trata os achados A2, A3, A4, A10, A11 (só identidade e autoria), A12, a tradução camadas ↔ grafo e o modelo de recálculo apontados pelo DeepSeek, e o painel de conflitos do ADR-0019.

A especificação fica num documento em `docs/` (o nome é proposto no plano da entrega). As decisões ficam em ADRs novos.

**Critérios de aceite:**
- [ ] **Validação em duas camadas (achado A2):**
  - a validação local (interface, script, plugin) pode rejeitar uma operação;
  - a validação depois da junção nunca rejeita: ela só marca conflito;
  - o conflito é calculado a partir do estado juntado, e dá o mesmo resultado em todas as cópias, qualquer que seja a ordem de chegada das operações.

  **Teste:** uma pessoa liga A em B, outra liga B em A. Depois da junção, as duas cópias mostram o mesmo ciclo em estado de conflito. A Loro impede ciclos só na árvore, não nas conexões entre nós (veja a [comparação das revisões](../auditoria/2026-10-28-comparacao.md)), então o teste vale qualquer que seja a biblioteca.
- [ ] **Ajustes manuais (achado A3):** a especificação diz como um ajuste aponta para o resultado do nó (identificadores estáveis dos pontos ou uma "assinatura" da forma) e o que é "invalidar". **Teste:** na estrela que passa de 5 para 7 pontas, e de 7 para 5, o ajuste nunca é reaplicado no vértice errado. Ou continua no ponto certo, ou entra em estado de conflito.
- [ ] **Dono e referência (achado A4):** a especificação define:
  - o que acontece quando o dono de um nó compartilhado é removido;
  - onde ficam os nós criados no nível do documento, como uma cor-variável;
  - o que acontece quando uma pessoa desvincula e outra edita o original ao mesmo tempo;
  - se "desvincular" copia também os nós que alimentam aquele nó;
  - quando se usa referência e quando se usa componente/instância;
  - se um nó referenciado é calculado no contexto do dono ou de quem o referencia (DeepSeek).

  Cada regra tem um teste de junção que dá o mesmo resultado em todas as cópias.
- [ ] **Tipos de nó desconhecidos (achado A10):** um nó cujo tipo o programa não conhece (plugin ausente, persona dormindo, colega com versão mais nova) é preservado intacto e marcado. A validação de cada tipo é descrita como dados e roda sem acordar a persona. **Teste:** um arquivo com um nó de um plugin ausente abre, salva e reabre sem perder o nó.
- [ ] **Identidade e autoria nas operações (achado A11, só esta parte):** um ADR diz como o autor de cada operação é ligado à chave pública dele, quanto custa assinar cada operação (medido no protótipo) e o que acontece com a operação de um editor malicioso sobre a qual outros já trabalharam. Transporte e criptografia ficam para a Fase 2.
- [ ] **Testes de junção que protegem de verdade (achado A12):**
  - teste de propriedades: as mesmas operações, entregues em qualquer ordem, chegam ao mesmo estado;
  - nenhuma edição simultânea some sem aparecer como conflito, ou como aviso e registro no histórico (ADR-0019);
  - os casos difíceis, um teste cada: mover A para dentro de B enquanto outro move B para dentro de A; remover um nó enquanto outro o edita; reordenar a pilha ao mesmo tempo; o mesmo parâmetro editado por duas pessoas;
  - nos movimentos e reordenações simultâneos, vence um, e o teste confere que todas as cópias mostram o mesmo aviso, o mesmo registro no painel de conflitos e o mesmo movimento perdido no histórico (ADR-0019, decisão 3).
- [ ] **Tradução camadas ↔ grafo (DeepSeek):** a especificação diz como cada painel (camadas, pilha Traçado/Preenchimento/FX) lê o grafo e como cada edição no painel vira operações. **Testes:**
  - mover um efeito para cima de outro na pilha gera exatamente as operações esperadas no grafo;
  - um grafo que o painel não sabe representar ganha o indicador "customizado" e não é alterado só por abrir o painel;
  - ler o grafo no painel e gravar de volta sem mudar nada deixa o grafo idêntico.
- [ ] **Recálculo do grafo (DeepSeek):** a especificação diz como uma mudança se propaga e como o cache evita recalcular o que não mudou, incluindo os nós de ajuste do ADR-0004. **Teste:** mudar um parâmetro recalcula só os nós que dependem dele (verificado por contador).
- [ ] **Painel de conflitos (ADR-0019):** a especificação define os tipos de conflito e de aviso, a gravidade de cada um e as ações próprias de cada tipo (não só "reaplicar/assar/descartar"). O painel é acessível por teclado e leitor de tela, e cor nunca é o único sinal. No protótipo, a lista de conflitos é calculada do estado e sai igual em todas as cópias. A interface do painel é construída no Núcleo.
- [ ] **Revisão:** os ADRs novos estão com status "Revisado por IA".

### 0.8 Cor, scripts e papel do Vello

Duas bibliotecas que o Núcleo precisa e a decisão sobre o que o Vello faz. Cada escolha fica registrada num **ADR próprio**, com as alternativas comparadas em licença, manutenção, quantidade de código `unsafe` ou em C, tamanho e segurança. Os testes de cada item são provas de conceito **descartáveis**.

**Critérios de aceite:**
- [ ] **Motor JS para scripts (ADR-0013):**
  - um ADR compara pelo menos três motores e escolhe um;
  - numa prova mínima, um script chama uma função do Bivar e recebe a resposta;
  - um script com laço infinito é interrompido por limite de tempo, sem travar o programa;
  - um script não consegue abrir arquivos nem acessar a rede, a não ser pelo que o Bivar oferecer;
  - um script que gasta memória sem parar é interrompido por um limite de memória, e um script que chama a si mesmo sem fim é interrompido por um limite de pilha. Nos dois casos, o programa não cai, não trava e mostra uma mensagem clara.
- [ ] **Gerenciamento de cor (ADR-0008):**
  - um ADR compara as opções e escolhe uma;
  - numa prova mínima, um conjunto de cores é convertido de sRGB para CMYK com um perfil ICC e de volta. A diferença é medida em **Delta E 2000** (ΔE00), a medida padrão de diferença de cor percebida pelo olho: ΔE00 = 1 é mais ou menos a menor diferença que uma pessoa atenta consegue notar. Tolerâncias:
    - **intenção de renderização:** todas as conversões da prova usam a intenção **colorimétrica relativa**. Na intenção "perceptual", até as cores dentro da gama mudam, e a ida e volta não faria sentido. O ADR registra se a compensação de ponto preto está ligada ou desligada, e ela é igual na biblioteca e na referência;
    - **referência independente:** as referências são geradas por outro programa de gerenciamento de cor, com o mesmo perfil e a mesma intenção, e com **um motor de cor diferente** do da biblioteca escolhida. Por exemplo, se a biblioteca usar a LittleCMS por dentro, a referência não pode usar a LittleCMS. O ADR registra qual programa e qual motor foram usados;
    - **conversão contra valores de referência:** ΔE00 ≤ 1,0 para cada cor;
    - **o que é "dentro da gama":** uma cor está dentro da gama do perfil quando, **calculado pela referência independente**, sRGB → CMYK → cor de volta muda essa cor no máximo ΔE00 1,0. Assim a biblioteca testada não decide sozinha quais cores entram no teste;
    - **ida e volta (sRGB → CMYK → sRGB):** ΔE00 ≤ 2,0 para cada cor dentro da gama do perfil;
  - **exceção esperada:** cores fora da gama de impressão do perfil CMYK (por exemplo, verdes e azuis muito saturados de tela) não voltam iguais, porque a impressora não consegue reproduzi-las. Para essas cores não vale a tolerância de ida e volta. Elas precisam ser convertidas sempre do mesmo jeito, sem erro, e a prova lista quais saíram da gama e com que ΔE00;
  - a leitura de perfis ICC respeita o ADR-0011 (leitura de arquivos externos só em Rust) ou registra uma exceção justificada.
- [ ] **Papel do Vello (achado A8; ADR-0019):** um ADR diz o que o Vello faz (a tela e a exportação PNG/JPG em 8 bits) e qual caminho cuida de CMYK, 16/32 bits, cores especiais, sobreimpressão, prova de impressão na tela e PDF/X, com as alternativas comparadas. Ele diz também como exportar imagens maiores que o limite de textura da placa (16.384 px na GTX 1660). A escolha conversa com a biblioteca de cor acima.
- [ ] **Revisão:** os três ADRs novos estão com status "Revisado por IA".
- [ ] **Licenças:** `cargo deny check` e `cargo audit` passam nas provas.

### 0.9 Prova de degradês

O Vello 0.11 fica lento com muitos degradês diferentes e cai acima de 16.384 (limitações 1 e 2 do [ADR-0018](../adr/0018.md)). Recursos centrais do Bivar, como a mesclagem, geram muitos degradês. Por isso a solução é provada aqui, numa prova **descartável**, antes do Núcleo e da entrega N.3. O caminho, entre as opções do ADR-0018 (reaproveitar e limitar, blocos, memória configurável no Vello ou versão nova do Vello), é proposto no plano da entrega, revisado pelo agente auditor e decidido pelo dono.

**Regra da prova:** nenhum `unsafe` no nosso código.

**Critérios de aceite:**
- [ ] **Velocidade:** na GTX 1660, com a cena de 10.000 retângulos, cada um com degradê próprio, durante zoom e pan:
  - **obrigatório:** 99% dos quadros abaixo de 16,7 ms (60 Hz);
  - **desejável:** 99% dos quadros abaixo de 6,9 ms (144 Hz).
- [ ] **Estabilidade:** uma cena com 50.000 degradês diferentes abre sem derrubar o programa e **desenha corretamente, ou mostra um aviso visível**. Ela pode ficar lenta. Não pode cair, nem trocar degradês por cor sólida (ou por qualquer simplificação) em silêncio. Nenhum arquivo é recusado por esse limite (ADR-0019).
- [ ] **Imagem correta:** a imagem da solução é comparada com a do vello_cpu da mesma cena, por regiões, para provar que nenhum degradê sumiu ou foi trocado.
- [ ] **Medição registrada:** o relatório diz como cada número foi medido, e o código que mede fica na prova.
- [ ] **Registro:** a solução escolhida fica num ADR novo, revisado pelo agente auditor. A implementação no produto continua sendo critério da N.3.

## Testes visuais: onde rodam

| Tipo de teste visual | No GitHub (Linux, sem placa de vídeo) | No PC do dono (GTX 1660) |
| --- | --- | --- |
| **Sem placa de vídeo** (vello_cpu) | Roda a cada envio, com **comparação exata**: qualquer pixel diferente faz o teste falhar. | Roda também, com a mesma comparação exata. |
| **Com placa de vídeo** (Vello) | **Não roda:** o GitHub não tem placa de vídeo. | Roda, com a folga medida do [ADR-0018](../adr/0018.md): até 3 pixels, com diferença de no máximo 1 em 255. |

- **Antes de um commit que muda o que aparece na tela,** o dono roda os testes com placa no PC dele. O plano da 0.5 diz qual comando faz isso.
- **As imagens sem placa precisam sair iguais nos dois lugares.** A 0.5 confere isso com as primeiras imagens de referência. Se o PC e o GitHub não concordarem, o Claude Code para e o dono decide.
- **Nenhuma imagem de referência muda sem a aprovação do dono** (CLAUDE.md §6).

## Antes do Núcleo

**O Núcleo só começa quando as entregas 0.4, 0.5, 0.6, 0.7, 0.8 e 0.9 estiverem concluídas e revisadas pelo agente auditor.**

## Fora do escopo da Fase 0

Qualquer funcionalidade de produto. As provas de conceito são **descartáveis** e não viram base de código.

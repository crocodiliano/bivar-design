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

**Resultado esperado:** o resultado é registrado no [ADR-0018](../adr/0018.md), que confirma o ADR-0007 com ressalvas e passa pela revisão do agente auditor (entrega 0.4). O ADR-0007 fica como está. *(Texto histórico: depois disso, o [ADR-0019](../adr/0019.md), decisão 6, mudou o ADR-0007, que passou a valer para a tela e a exportação em 8 bits.)*

> **Auditoria:** por enquanto não há auditor humano. As revisões são feitas pelo agente auditor (`.claude/agents/auditor.md`), por ferramentas automáticas e por voluntários da comunidade. Veja a seção 2 do `CLAUDE.md`.

### 0.4 Revisão por IA dos ADRs

Revisão por IA dos ADRs 0001 a 0019, com três partes:

- **o agente auditor** (`.claude/agents/auditor.md`) revisa todos os ADRs;
- **uma segunda opinião** de uma IA de outra empresa, para reduzir os pontos cegos de IAs parecidas;
- **um pedido público de revisão** à comunidade Rust.

**Critérios de aceite:**
- [ ] O relatório do agente auditor está salvo em `docs/auditoria/`.
- [ ] Todo achado de gravidade Crítica ou Alta foi tratado: ou corrigido, ou com justificativa registrada.
- [ ] Os ADRs de prioridade Alta estão com status "Revisado por IA": 0001, 0002, 0003, 0004, 0005, 0007, 0008, 0009, 0010, 0011, 0014, 0017, 0018 e 0019. A prioridade de cada um está no [índice dos ADRs](../adr/README.md). O ADR-0019 chegou a "Revisado por IA" em 2026-10-28.
- [ ] **ADR-0014 dividido em dois** (achado A11): a parte de identidade e autoria passa para o ADR novo da 0.7. O ADR-0014 fica só com transporte e criptografia, que esperam a Fase 2, e registra essa justificativa. Com isso ele pode chegar a "Revisado por IA" sem resolver agora o que é da Fase 2.

**A 0.4 é a última entrega a fechar.** Vários ADRs da lista acima só deixam de estar travados com os resultados da 0.5 à 0.9 (veja o [registro de tratamento](../auditoria/2026-10-28-fase-0-tratamento.md)). Por isso a 0.4 fecha depois da 0.9.

### 0.5 Prova de conceito do motor de texto

A prova de conceito da 0.3 desenhou texto letra por letra, sem *shaping* (a montagem das letras de acordo com as regras da fonte). Esta prova, também **descartável**, testa um motor de texto em Rust, desenhando com o Vello. A biblioteca é proposta no plano da entrega, com licença, manutenção e alternativas, e precisa da aprovação do dono.

**Regra da prova:** nenhum `unsafe` no nosso código.

**Critérios de aceite:**
- [x] **Acentos em português:** o texto "Açaí à brasileira — ótimo, pão, avô, ação" aparece com todos os acentos no lugar certo. Isso vale para as letras acentuadas prontas (como "á") e para as montadas com acento separado (letra "a" seguida do acento agudo), que devem ficar iguais.
- [x] **Kerning:** em pares como "AV", "To" e "Ty", o espaço entre as letras segue o ajuste definido na fonte, e não só a largura de cada letra. Um teste confere a posição contra os valores da fonte.
- [x] **Ligaduras:** com uma fonte que as tem, "fi" e "fl" viram o desenho único da ligadura. Elas podem ser desligadas, e então as letras voltam a aparecer separadas. **Com ressalva, aceita pelo dono:** a Source Sans 3 (OTF) não tem "fi" nem "fl", então esse par foi testado com a Source Serif 4 (TTF), e ligar e desligar foi testado nas duas fontes. Veja o [ADR-0020](../adr/0020.md).
- [x] **Formatos de fonte:** funciona com uma fonte OTF (contornos CFF) e com uma TTF, as duas com licença OFL.
- [x] **Testes visuais:** o dono aprova as imagens de referência dos quatro critérios acima, desenhadas pelo Vello na placa de vídeo e pelo vello_cpu.
- [x] **Mesma imagem no PC do dono e no GitHub:** as imagens sem placa de vídeo (vello_cpu) saem idênticas, pixel a pixel, no PC do dono (Windows) e na compilação automática do GitHub (Linux). Veja "Testes visuais: onde rodam", abaixo. Se não saírem, o Claude Code para e mostra a diferença ao dono, que decide o que fazer.
- [x] **Registro:** o resultado e a biblioteca escolhida ficam num ADR novo, com status "Revisado por IA". Ficaram no [ADR-0020](../adr/0020.md), "Revisado por IA" em 2026-10-29.

### 0.6 Armazenamento e sincronização

Como o documento é guardado no disco e como duas cópias se juntam. Segue as decisões do [ADR-0019](../adr/0019.md): arquivo único com os arquivos grandes dentro, em pedaços; um único histórico de operações, o do CRDT; e "Salvar cópia para enviar", sem histórico. Trata os achados C1, A1, A5, A6 e A7 da [auditoria da Fase 0](../auditoria/2026-10-28-fase-0.md).

As escolhas ficam registradas em **ADRs novos**, com as alternativas comparadas em licença, manutenção, quantidade de código `unsafe` ou em C, tamanho e segurança. Os testes são provas de conceito **descartáveis**.

**Critérios de aceite:**
- [ ] **Formato por dentro do arquivo (achado C1):** um ADR escolhe entre:
  - (a) SQLite com proteções obrigatórias: modo defensivo, `trusted_schema` desligado, verificação das células, carregamento de extensões desligado, limites de tamanho, `quick_check` ao abrir, recusa de tabelas, gatilhos e visões fora da especificação, e validação de todo o conteúdo como se viesse da rede;
  - (b) SQLite aberto num processo isolado, para arquivos de terceiros;
  - (c) um formato próprio com leitor 100% em Rust.

  Se a escolha for (a), o ADR vem com uma emenda registrada ao ADR-0011 e ao CLAUDE.md §4 ("leitura de arquivos externos só em Rust"), aprovada pelo dono, porque o SQLite é escrito em C.

  Na prova: um arquivo com um gatilho ou uma visão injetada é recusado com uma mensagem clara, e o carregador ganha um alvo de fuzzing (ADR-0011).
- [ ] **Toda entrada externa (achado A7):** um ADR novo completa o ADR-0011 para toda entrada externa, não só arquivos: área de transferência, arrastar e soltar, fontes instaladas no sistema, perfis de cor embutidos em imagens, dados recebidos por P2P e o próprio `.bivar`. Ele define:
  - limites obrigatórios de tamanho, recursão, memória e tempo, contra arquivos feitos para esgotar recursos ("bombas");
  - um critério de estresse para cada leitor;
  - um inventário do código `unsafe` e em C de cada biblioteca de leitura, feito ao aprová-la;
  - a ferramenta de fuzzing e onde ela roda: um caminho só (`tests/fuzz/` ou `fuzz/`), como lidar com a exigência da versão *nightly* do Rust e se há um job na compilação automática.

  Na prova: o leitor do `.bivar` é o primeiro a seguir esse ADR. Um arquivo feito para esgotar memória ou tempo é recusado com uma mensagem clara, sem derrubar o programa.
- [ ] **Onde mora a verdade do documento (achado A1):** o ADR do CRDT diz:
  - se o documento vive na biblioteca CRDT ou no `bivar-core`, e como os dois ficam iguais. Em qualquer caso, o grafo é o modelo lógico do documento e o CRDT é onde ele é armazenado e sincronizado (ADR-0019, decisão 2), para o princípio 1 continuar valendo;
  - como cada nó é identificado de forma única entre computadores;
  - como cada tipo de operação vira estrutura CRDT: árvore para a hierarquia, lista para a pilha, mapa para os parâmetros, e as conexões entre nós, que não formam árvore;
  - a versão do formato das operações.

  Um protótipo modela a cadeia Forma → Traçado → Preenchimento → Saída, com as conexões.
- [ ] **Sistema de operações independente da biblioteca CRDT** (ADR-0019, plano de contingência): no protótipo, só um crate depende da biblioteca CRDT, e uma checagem automática na compilação confere isso (como a do achado B4, que impede `bivar-core` e `bivar-doc` de depender da interface). O resto usa as operações do ADR-0002, sem tipos da biblioteca.
- [ ] **Comparação Loro × Automerge.** As duas rodam a mesma prova, na máquina de referência, e o ADR registra os números e escolhe uma. Os limites numéricos de carregamento e velocidade são fixados no plano da entrega e aprovados pelo dono antes da medição. O ADR registra também por que o Yrs, alternativa citada no ADR-0005, ficou fora da comparação, com o motivo conferido na documentação dele, ou o inclui na comparação:
  - **escrita simultânea:** duas cópias mudam o mesmo parâmetro ao mesmo tempo. Depois da junção, o programa sabe que houve escrita simultânea e conhece os dois valores, para avisar e registrar no histórico (ADR-0019). Se a biblioteca não oferecer isso pronto, o ADR mede o custo de fazer por fora;
  - **movimentos simultâneos (ADR-0019, decisão 3):** dois casos, um teste cada: duas cópias movem o mesmo nó para lugares diferentes; uma cópia move A para dentro de B enquanto outra move B para dentro de A. Depois da junção, o objeto está num lugar só, nunca duplicado, nenhum ciclo se forma, e o programa sabe qual movimento perdeu, para avisar e registrar no histórico. Se a biblioteca não tiver árvore com movimentação, o ADR diz como o movimento é feito e mede o custo de detectar o movimento perdido;
  - **desfazer em colaboração:** A muda um objeto, B muda outro, e A desfaz. Só a mudança de A some. O ADR registra também o que acontece quando A desfaz algo que B já editou por cima;
  - **carregamento:** tempo e memória para abrir um documento com 1 milhão de operações no histórico. O ADR diz também se dá para carregar só a parte de uma persona;
  - **remoção do histórico (achado A5; ADR-0019, decisão 5):** um texto com um marcador único, inserido de uma só vez, e uma imagem são apagados, e depois vem "Salvar cópia para enviar". O arquivo da cópia é descomprimido antes da busca, do mesmo jeito que o programa o lê, e o marcador não aparece em nenhum byte. A própria biblioteca confere que o histórico da cópia não tem nenhuma operação anterior à cópia. Nenhum pedaço da imagem apagada, nem o hash dela, está na cópia. A cópia abre com um histórico novo e não se junta com o original. Com um conflito ou um aviso ainda sem resolver, "Salvar cópia para enviar" avisa antes de gravar;
  - **velocidade:** 100.000 operações num documento de teste, misturando todos os tipos de operação do ADR-0002 (criar e remover nó, conectar e desconectar, mudar parâmetro, reordenar e mover na hierarquia), nas proporções definidas no plano da entrega (achado A12), e a junção de duas cópias que receberam 10.000 operações diferentes cada uma, ao mesmo tempo. São medidos o tempo, a memória e o tamanho salvo, e as duas cópias terminam idênticas.
- [ ] **Arquivos grandes em pedaços** (ADR-0019; achado A6): um arquivo de teste de 2 GB, maior que o limite de 1 GB por valor do SQLite, é guardado em pedaços, salvo, reaberto e sai idêntico (mesmo hash). Mudar um parâmetro e salvar não regrava nenhum pedaço.
- [ ] **Riscos de armazenamento (achado A6).** O ADR de armazenamento trata de cada um, e a prova testa:
  - **arquivo único de verdade:** copiar o arquivo com o programa aberto gera uma cópia completa, ou o ADR explica como isso é evitado (por exemplo, os arquivos `-wal` e `-shm` do SQLite);
  - **mesmo arquivo aberto duas vezes:** a segunda abertura é recusada ou abre só para leitura, com uma mensagem clara;
  - **queda no meio da gravação:** o processo é morto durante o salvamento, e o arquivo reabre com a última versão salva;
  - **edições desde o último salvamento:** fazer N edições sem salvar, matar o processo e reabrir: as N edições voltam. Era a função do diário eliminado pelo ADR-0019;
  - **gravação no disco de verdade:** matar o processo não prova que os dados chegaram ao disco, por causa de uma queda de energia. O ADR diz o que é forçado ao disco e quando (por exemplo, a cada operação ou a cada intervalo), e quantas edições podem se perder no pior caso;
  - **salvamento com a interface aberta:** o tempo de salvamento é medido com a interface aberta, e o salvamento automático não causa pausa perceptível. O limite numérico é fixado no plano da entrega e aprovado pelo dono (Gemini);
  - **limpeza dos arquivos grandes:** o ADR diz quando um arquivo grande que ninguém mais usa pode ser apagado do arquivo de projeto, já que o histórico e o desfazer ainda podem precisar dele e a "cópia para enviar" precisa removê-lo (Gemini);
  - **arquivo corrompido ou cortado:** abre o que for possível ou mostra uma mensagem clara. O programa nunca cai;
  - **migração de versão:** guarda uma cópia antes; uma falha no meio não estraga o original; um arquivo de versão mais nova é recusado com mensagem, nunca "consertado";
  - **migração do histórico:** o ADR diz como as operações antigas do histórico do CRDT são lidas depois de uma migração e o que acontece numa junção entre colegas com versões diferentes do formato;
  - **pastas sincronizadas** (OneDrive, unidades de rede): o ADR diz o que o programa faz, por exemplo um aviso ao abrir;
  - **criptografia opcional:** o ADR diz como será feita sem biblioteca paga (CLAUDE.md §5), ou adia a decisão com justificativa;
  - **especificação aberta:** o ADR diz como a especificação em `docs/formato/` fica legível sem depender do formato interno da biblioteca CRDT, ou registra essa dependência.
- [ ] **Contingência avaliada** (ADR-0019, decisão 7): com o resultado da 0.6 em mãos, o dono decide se dispara a versão de uma pessoa só. A decisão fica registrada.
- [ ] **Revisão:** os ADRs novos estão com status "Revisado por IA".
- [ ] **Licenças:** `cargo deny check` e `cargo audit` passam nas provas.

### 0.7 Especificação do modelo de dados, com protótipo

O Núcleo vai implementar o grafo na entrega N.1. Antes disso, as regras do modelo de dados precisam estar escritas e testadas num protótipo **descartável**, construído sobre o resultado da 0.6. Trata os achados A2, A3, A4, A10, A11 (só identidade e autoria), A12, M15, a tradução camadas ↔ grafo e o modelo de recálculo apontados pelo DeepSeek, e o painel de conflitos do ADR-0019.

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
- [ ] **Identidade e autoria nas operações (achado A11, só esta parte):** um ADR novo, que substitui a parte de identidade e autoria do ADR-0014 (veja a 0.4), diz:
  - (a) como o autor de cada operação é ligado à chave pública dele;
  - (b) quanto custa assinar cada operação (medido no protótipo). A biblioteca de assinatura usada na medição passa pelo processo de dependências (CLAUDE.md §5) e, como é criptografia, precisa de aprovação em ADR (CLAUDE.md §4) antes de entrar na prova;
  - (c) o que acontece com a operação de um editor malicioso sobre a qual outros já trabalharam;
  - (d) como o papel de comentarista é imposto, já que a biblioteca CRDT não faz isso;
  - (e) o que acontece quando o acesso de alguém é revogado enquanto ele edita, com o mesmo resultado em todas as cópias;
  - (f) o que acontece com a lista de acesso se o dono do documento perder a chave.

  Transporte e criptografia, o item (g), ficam para a Fase 2, no ADR-0014.
- [ ] **Imagem guardada de personas dormindo (achado M15):** a especificação trata a imagem guardada (cache/proxy) de uma persona que não está carregada como **dado não confiável**: ela é recalculada quando a persona acorda, é invalidada quando chega uma operação que a afeta (inclusive da rede), e a tela indica de forma visível quando a imagem vem do cache. **Teste:** um arquivo cuja imagem guardada não bate com o conteúdo mostra o conteúdo real depois que a persona acorda, e a exportação nunca usa a imagem guardada no lugar do conteúdo.
- [ ] **Testes de junção que protegem de verdade (achado A12):**
  - teste de propriedades: as mesmas operações, entregues em qualquer ordem, chegam ao mesmo estado;
  - nenhuma edição simultânea some sem aparecer como conflito, ou como aviso e registro no histórico (ADR-0019);
  - os casos difíceis, um teste cada: mover A para dentro de B enquanto outro move B para dentro de A; remover um nó enquanto outro o edita; duas cópias mudam a posição do mesmo item da pilha ao mesmo tempo (vence um, com aviso); duas cópias movem itens diferentes da mesma pilha ao mesmo tempo (os dois movimentos valem, sem aviso); o mesmo parâmetro editado por duas pessoas; duas pessoas pintam no mesmo bloco de raster (fica a última pincelada, com aviso e registro, mesmo com os blocos guardados pelo hash, fora do CRDT);
  - nos movimentos e reordenações simultâneos do mesmo item, vence um, nenhum objeto é duplicado, e o teste confere que todas as cópias mostram o mesmo aviso, o mesmo registro no painel de conflitos e o mesmo movimento perdido no histórico (ADR-0019, decisão 3).
- [ ] **Tradução camadas ↔ grafo (DeepSeek):** a especificação diz como cada painel (camadas, pilha Traçado/Preenchimento/FX) lê o grafo e como cada edição no painel vira operações. **Testes:**
  - mover um efeito para cima de outro na pilha gera exatamente as operações esperadas no grafo;
  - um grafo que o painel não sabe representar ganha o indicador "customizado" e não é alterado só por abrir o painel;
  - ler o grafo no painel e gravar de volta sem mudar nada deixa o grafo idêntico.
- [ ] **Recálculo do grafo (DeepSeek):** a especificação diz como uma mudança se propaga e como o cache evita recalcular o que não mudou, incluindo os nós de ajuste do ADR-0004. **Teste:** mudar um parâmetro recalcula só os nós que dependem dele (verificado por contador).
- [ ] **Painel de conflitos (ADR-0019):** o painel tem duas seções, "Conflitos" (exigem ação) e "Avisos" (informativos, podem ser dispensados; incluem o mesmo parâmetro e o raster). Dispensar um aviso não apaga o registro no histórico. A especificação define os tipos de conflito e de aviso, a seção de cada um, a gravidade de cada um e as ações próprias de cada tipo (não só "reaplicar/assar/descartar"). A especificação diz também como o painel é usado por teclado e leitor de tela e como cada tipo é sinalizado sem depender só de cor. No protótipo, a lista de conflitos é calculada do estado e sai igual em todas as cópias. A interface do painel, e o teste de acessibilidade dela, ficam para o Núcleo (entrega N.5).
- [ ] **Revisão:** os ADRs novos estão com status "Revisado por IA".

### 0.8 Cor, scripts e papel do Vello

Duas bibliotecas que o Núcleo precisa, a decisão sobre o que o Vello faz e a política de dependências (achado M8). Cada escolha fica registrada num **ADR próprio**, com as alternativas comparadas em licença, manutenção, quantidade de código `unsafe` ou em C, tamanho e segurança. Os testes de cada item são provas de conceito **descartáveis**.

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
- [ ] **Papel do Vello (achado A8; ADR-0019):** um ADR diz o que o Vello faz (a tela e a exportação PNG/JPG em 8 bits) e qual caminho cuida de CMYK, 16/32 bits, cores especiais, sobreimpressão, prova de impressão na tela e PDF/X, com as alternativas comparadas. Ele diz também:
  - como exportar imagens maiores que o limite de textura da placa (16.384 px na GTX 1660);
  - como a tela faz as misturas de cor em luz linear ou em sRGB, como o ADR-0008 exige (escolha documentada e, quando relevante, configurável), já que o Vello converte cores e degradês para sRGB em 8 bits;
  - como a tela respeita o perfil de cor do monitor (gerenciamento de cor da tela).

  A escolha conversa com a biblioteca de cor acima.
- [ ] **Política de dependências (achado M8):** um ADR novo completa o ADR-0012 antes de o Núcleo começar a somar dependências. Ele define:
  - o que são "poucas" dependências, com um critério verificável;
  - como olhar o código `unsafe` e em C dentro de cada dependência;
  - como tratar os scripts de compilação (`build.rs`) e as macros, que rodam código na máquina durante a compilação;
  - versões fixas para as ferramentas da compilação automática e ações do GitHub fixadas por hash, não por etiqueta;
  - se o projeto adota um registro de quem revisou cada dependência (por exemplo, o cargo-vet), com as alternativas comparadas.
- [ ] **Revisão:** os quatro ADRs novos estão com status "Revisado por IA".
- [ ] **Licenças:** `cargo deny check` e `cargo audit` passam nas provas.

### 0.9 Prova de degradês

O Vello 0.11 fica lento com muitos degradês diferentes e cai acima de 16.384 (limitações 1 e 2 do [ADR-0018](../adr/0018.md)). Recursos centrais do Bivar, como a mesclagem, geram muitos degradês. Por isso a solução é provada aqui, numa prova **descartável**, antes do Núcleo e da entrega N.3. O caminho, entre as opções do ADR-0018 (reaproveitar e limitar, blocos, memória configurável no Vello ou versão nova do Vello), é proposto no plano da entrega, revisado pelo agente auditor e decidido pelo dono. A prova trata também os achados A9 e M13 da [auditoria da Fase 0](../auditoria/2026-10-28-fase-0.md).

**Regra da prova:** nenhum `unsafe` no nosso código.

**Critérios de aceite:**
- [ ] **Velocidade:** na GTX 1660, com a cena de 10.000 retângulos, cada um com degradê próprio, durante zoom e pan:
  - **obrigatório:** 99% dos quadros abaixo de 16,7 ms (60 Hz);
  - **desejável:** 99% dos quadros abaixo de 6,9 ms (144 Hz).
- [ ] **Estabilidade:** uma cena com 50.000 degradês diferentes abre sem derrubar o programa e **desenha corretamente, ou mostra um aviso visível**. Ela pode ficar lenta. Não pode cair, nem trocar degradês por cor sólida (ou por qualquer simplificação) em silêncio. Nenhum arquivo é recusado por esse limite (ADR-0019).
- [ ] **Imagem correta:** a imagem da solução é comparada com a do vello_cpu da mesma cena, por regiões, para provar que nenhum degradê sumiu ou foi trocado. Como o vello_cpu difere da placa em 30 a 35% dos pixels nas bordas (ADR-0018), o tamanho das regiões e a folga de cada uma são fixados no plano da entrega, a partir de uma medição registrada, e aprovados pelo dono.
- [ ] **Limites fixos de memória do Vello (achado A9):** o Vello 0.11 usa áreas de memória de tamanho fixo e só avisa da falha num contador interno. Um teste de estresse passa desses limites de propósito (por exemplo, uma página muito densa ou uma exportação grande) e compara o resultado com o vello_cpu. O programa detecta a falha e repete com mais memória ou divide em blocos. Nunca entrega uma imagem incompleta em silêncio.
- [ ] **Testes de placa em paralelo (achado M13):** a prova usa um único dispositivo de vídeo compartilhado entre a tela e a exportação, como a prova do renderizador já faz. A queda dos testes em paralelo (limitação 4 do [ADR-0017](../adr/0017.md)) vira um exemplo mínimo reportado ao wgpu, com o link registrado no ADR. A trava dos testes continua até o problema ser resolvido.
- [ ] **Medição registrada:** o relatório diz como cada número foi medido, e o código que mede fica na prova.
- [ ] **Registro:** a solução escolhida fica num ADR novo, com status "Revisado por IA". A implementação no produto continua sendo critério da N.3.

## Testes visuais: onde rodam

| Tipo de teste visual | No GitHub (Linux, sem placa de vídeo) | No PC do dono (GTX 1660) |
| --- | --- | --- |
| **Sem placa de vídeo** (vello_cpu) | Roda a cada envio, com **comparação exata**: qualquer pixel diferente faz o teste falhar. | Roda também, com a mesma comparação exata. |
| **Com placa de vídeo** (Vello) | **Não roda:** o GitHub não tem placa de vídeo. | Roda, com a folga medida do [ADR-0018](../adr/0018.md): até 3 pixels, com diferença de no máximo 1 em 255. |

- **Antes de um commit que muda o que aparece na tela,** o dono roda os testes com placa no PC dele. Na prova da 0.5, o comando é `cargo test --manifest-path poc/text-parley/Cargo.toml -- --include-ignored`.
- **As imagens sem placa precisam sair iguais nos dois lugares.** A 0.5 conferiu isso com as primeiras imagens de referência: o PC e o GitHub concordaram, os dois no modo SIMD AVX2. Se o PC e o GitHub não concordarem, o Claude Code para e o dono decide.
- **O modo SIMD muda os pixels** ([ADR-0020](../adr/0020.md)). Na prova da 0.5, o vello_cpu usa o modo que detecta. No produto (entrega N.3), os testes sem placa fixam o modo em AVX2 e o número de linhas de execução. Numa máquina sem AVX2, o teste é pulado com aviso, e continua obrigatório no GitHub e no PC do dono.
- **Nenhuma imagem de referência muda sem a aprovação do dono** (CLAUDE.md §6).

## Antes do Núcleo

**O Núcleo só começa quando as entregas 0.4, 0.5, 0.6, 0.7, 0.8 e 0.9 estiverem concluídas e revisadas pelo agente auditor.** A 0.4 fecha por último, depois da 0.9.

## Fora do escopo da Fase 0

Qualquer funcionalidade de produto. As provas de conceito são **descartáveis** e não viram base de código.

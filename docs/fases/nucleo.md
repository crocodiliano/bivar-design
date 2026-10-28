# Núcleo

**Objetivo:** a plataforma sobre a qual todas as personas são construídas. Ela não tem nenhuma ferramenta de persona, mas tem tudo de que elas dependem.

**ADRs:** 0001, 0002, 0003, 0004, 0005, 0006, 0007, 0008, 0010, 0011, 0012, 0015.

**Pré-requisito:** as entregas 0.4, 0.5 e 0.6 da [Fase 0](fase-0.md) estão concluídas e revisadas pelo agente auditor. O Núcleo não começa sem isso.

## Entregas

### N.1 Grafo e operações (`bivar-core`)

- Nós, portas, conexões e grupos de nós (hierarquia).
- Operações serializáveis: criar/remover nó, conectar/desconectar, mudar parâmetro, reordenar, mover na hierarquia.
- Transações (várias operações = um passo de desfazer).
- Desfazer/refazer **por autor**.
- Validação: tipos de porta compatíveis, sem ciclos.
- Estados de nó: normal, **conflito** e **customizado**.
- Dono + referência para nós compartilhados (ADR-0003).
- Avaliação preguiçosa com cache e invalidação quando uma entrada muda.

**Critérios de aceite:**
- [ ] Um teste cria uma cadeia Forma → Traçado → Preenchimento → Saída, reordena Traçado e Preenchimento e confirma a nova ordem de avaliação.
- [ ] Uma transação com 500 operações é desfeita com um único passo.
- [ ] Tentar criar um ciclo é rejeitado com uma mensagem clara.
- [ ] Testes de propriedades: qualquer sequência aleatória de operações válidas seguida de "desfazer tudo" volta ao estado inicial.
- [ ] Duas cópias do documento recebem operações concorrentes em ordens diferentes e terminam idênticas (prova de compatibilidade com CRDT, ADR-0005).

### N.2 Formato de arquivo (`bivar-doc`)

- Arquivo SQLite, versão do formato, migração de exemplo.
- Salvamento incremental, diário de operações e recuperação de queda.
- Armazenamento de blobs por hash.
- Rascunho da especificação em `docs/formato/`.

**Critérios de aceite:**
- [ ] O dono fecha o programa à força (Gerenciador de Tarefas) no meio de uma edição, reabre, e o documento volta com a última operação salva.
- [ ] Salvar após mudar um parâmetro não regrava blobs grandes (verificado por teste).
- [ ] Um arquivo da "versão 1" de teste abre após uma migração para a "versão 2".

### N.3 Renderização (`bivar-render`)

- Compositor, tiles, cache/proxy e modo sem GPU, a partir do resultado da Fase 0.3.
- Infraestrutura de testes visuais em `tests/golden/`.

**Critérios de aceite:**
- [ ] **Testes visuais com folga medida** (decisão do [ADR-0018](../adr/0018.md)). O teste mostra a imagem de diferença quando falha, e:
  - **sem placa de vídeo (vello_cpu):** falha quando qualquer pixel muda;
  - **com a placa de vídeo:** falha quando mais de 3 pixels mudam, ou quando algum pixel muda mais de 1 nível em 255. Qualquer outra folga precisa vir de uma medição registrada no plano da entrega.
- [ ] Mexer num objeto redesenha só os tiles afetados (verificado por contador).
- [ ] **Muitos degradês, velocidade** (limitação 1 do [ADR-0018](../adr/0018.md); pré-requisito da Fase 1a). Na GTX 1660, com a cena de 10.000 retângulos, cada um com degradê próprio, durante zoom e pan:
  - **obrigatório:** 99% dos quadros abaixo de 16,7 ms (60 Hz);
  - **desejável:** 99% dos quadros abaixo de 6,9 ms (144 Hz).
- [ ] **Muitos degradês, estabilidade** (limitação 2 do [ADR-0018](../adr/0018.md); pré-requisito da Fase 1a). Obrigatório: um arquivo com 50.000 degradês diferentes abre sem derrubar o programa. Ele pode ficar lento, mas não pode cair.

### N.4 Texto, cor e interpolação

- **`bivar-text`:** composição de texto de ponto e em caixa, com quebra de linha e recursos OpenType básicos.
- **`bivar-color`:** cor com espaço de cor (nunca só RGB), perfis ICC e base CMYK.
- **`bivar-interp`:** correspondência de pontos e interpolação de paths (base da mesclagem e das fontes variáveis).

**Critérios de aceite:**
- [ ] O texto "Açaí à brasileira — ótimo" renderiza com acentos corretos e kerning da fonte.
- [ ] Uma cor definida em CMYK é preservada sem conversão ao salvar e reabrir.
- [ ] A interpolação entre um quadrado (4 pontos) e um hexágono (6 pontos) produz passos intermediários sem cruzamentos.

### N.5 Sistema de módulos, interface-base e acessibilidade

- Registro de personas (tipos de nó, ferramentas, painéis).
- Shell da interface com o framework da Fase 0:
  - troca de persona e painéis acopláveis/destacáveis;
  - temas Foco e Conforto via tokens, mais o modo de avaliação de cor;
  - paleta de comandos (Ctrl+K).
- Reprodução de áudio e forma de onda (base para a Animação).
- Uma "persona de exemplo" mínima que registra um nó e um painel, para provar a interface de módulos.

**Critérios de aceite:**
- [ ] A persona de exemplo aparece no seletor, e a troca de aba libera a memória dela (verificado no Gerenciador de Tarefas).
- [ ] Ctrl+K encontra e executa a ação "Trocar para tema Conforto".
- [ ] O tema nunca altera a cor do canvas (teste visual).
- [ ] O NVDA lê o painel de camadas da persona de exemplo.

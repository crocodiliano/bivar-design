# Núcleo

**Objetivo:** a plataforma sobre a qual todas as personas são construídas. Ela não tem nenhuma ferramenta de persona, mas tem tudo de que elas dependem.

**ADRs:** 0001, 0002, 0003, 0004, 0005, 0006, 0007, 0008, 0010, 0011, 0012, 0015, 0019, e os ADRs novos das entregas 0.5 a 0.9.

**Pré-requisito:** as entregas 0.4 a 0.9 da [Fase 0](fase-0.md) estão concluídas e revisadas pelo agente auditor. O Núcleo não começa sem isso.

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
- [ ] **"A última escrita vence", com aviso** (ADR-0019, decisão 3): duas cópias mudam o mesmo parâmetro ao mesmo tempo. Depois da junção, as duas mostram o mesmo valor, o mesmo aviso, e o histórico guarda o valor que perdeu.
- [ ] **Movimentos e reordenações simultâneos** (ADR-0019, decisão 3): duas cópias movem o mesmo nó para lugares diferentes, e outro teste move A para dentro de B enquanto outra cópia move B para dentro de A. Um terceiro teste muda a posição do mesmo item da pilha nas duas cópias. Nos três, vence um, nenhum objeto é duplicado, e todas as cópias mostram o mesmo aviso, o mesmo registro no painel de conflitos e o movimento que perdeu no histórico. Um quarto teste move itens diferentes da mesma pilha nas duas cópias: os dois movimentos valem, e não há aviso.
- [ ] **Nada se perde em silêncio, com milhares de cenários aleatórios** (achado A12; ADR-0021):
  - **2.000 cenários com duas cópias e 2.000 com três**, gerados ao acaso com a mistura de operações da 0.6;
  - **cada intenção de cada cópia**, anotada pelo próprio gerador e não lida da biblioteca, aparece no documento final ou num aviso;
  - **as cópias terminam idênticas**, sem nó duplicado e sem ciclo;
  - **o teste é obrigatório na compilação automática** com um número menor de cenários. Os 2.000 rodam antes de fechar a entrega e antes de aceitar qualquer versão nova da Loro.
- [ ] **Regra de desempate da Loro travada** (ADR-0021): um teste falha se a Loro mudar a forma de escolher a "última escrita" entre duas escritas simultâneas, porque a detecção de escrita simultânea do Bivar reproduz essa regra.
- [ ] **Só um crate depende da biblioteca CRDT** (ADR-0019, decisão 7): uma checagem automática na compilação confere isso, como na prova da 0.6. O resto do programa usa as operações do ADR-0002, sem tipos da biblioteca.
- [ ] **Contingência avaliada** (ADR-0019, decisão 7): com a N.1 pronta, o dono decide se dispara a versão de uma pessoa só. A decisão fica registrada.

### N.2 Formato de arquivo (`bivar-doc`)

- Arquivo único, no formato escolhido na 0.6, com versão do formato e migração de exemplo.
- Arquivos grandes dentro do arquivo, em pedaços (ADR-0019, decisão 1).
- "Salvar cópia para enviar", sem histórico (ADR-0019, decisão 5).
- Salvamento incremental e recuperação de queda pelo histórico do CRDT, o único histórico de operações (ADR-0019).
- Armazenamento de blobs por hash.
- Rascunho da especificação em `docs/formato/`.

**Critérios de aceite:**
- [ ] O dono fecha o programa à força (Gerenciador de Tarefas) no meio de uma edição, reabre, e o documento volta com a última operação salva.
- [ ] Salvar após mudar um parâmetro não regrava blobs grandes (verificado por teste).
- [ ] Um arquivo da "versão 1" de teste abre após uma migração para a "versão 2".
- [ ] **Edições desde o último salvamento:** fazer N edições sem salvar, fechar o programa à força e reabrir: as N edições voltam, na forma definida pelo ADR de armazenamento da 0.6.
- [ ] **Arquivos grandes em pedaços:** um arquivo de 2 GB é guardado em pedaços, salvo, reaberto e sai idêntico (mesmo hash).
- [ ] **Arquivo adulterado recusado:** um arquivo com conteúdo fora da especificação (por exemplo, um gatilho ou uma visão injetada, se o formato for SQLite) é recusado com uma mensagem clara, e o leitor tem um alvo de fuzzing (ADR-0011).
- [ ] **Arquivo feito para esgotar recursos** (achado A7; ADR de entrada externa da 0.6): um arquivo feito para esgotar memória ou tempo é recusado com uma mensagem clara, sem derrubar o programa, dentro dos limites definidos naquele ADR.
- [ ] **Mesmo arquivo aberto duas vezes:** a segunda abertura é recusada ou abre só para leitura, com uma mensagem clara, como definido no ADR de armazenamento da 0.6.
- [ ] **Gravação no disco de verdade:** o que é forçado ao disco, e quando, segue o ADR de armazenamento da 0.6. Um teste confere que essa gravação forçada acontece nos momentos definidos, e o plano da entrega diz quantas edições podem se perder no pior caso, numa queda de energia.
- [ ] **Salvamento sem pausa perceptível:** com a interface aberta, o salvamento automático fica dentro do limite numérico aprovado pelo dono na 0.6.
- [ ] **Migração do histórico:** depois da migração de exemplo da "versão 1" para a "versão 2", as operações antigas do histórico continuam legíveis (o desfazer funciona sobre elas), e a junção com uma cópia de outra versão do formato segue o que o ADR de armazenamento da 0.6 definiu.
- [ ] **"Salvar cópia para enviar":** um texto com um marcador único, inserido de uma só vez, e uma imagem são apagados antes de salvar a cópia. Com o arquivo descomprimido do mesmo jeito que o programa o lê, o marcador não aparece em nenhum byte, e nenhum pedaço nem o hash da imagem apagada está na cópia. A biblioteca CRDT confere que o histórico da cópia não tem nenhuma operação anterior à cópia. A cópia começa um histórico novo.
- [ ] **Pendências antes da cópia:** com um conflito ou um aviso ainda sem resolver no painel de conflitos, "Salvar cópia para enviar" avisa antes de gravar. Sem pendências, não há esse aviso.
- [ ] **Aviso ao compartilhar:** "Salvar como" num arquivo com histórico mostra um aviso discreto sugerindo "Salvar cópia para enviar". O salvamento normal não mostra aviso.

### N.3 Renderização (`bivar-render`)

- Compositor, tiles, cache/proxy e modo sem GPU, a partir do resultado da Fase 0.3.
- Infraestrutura de testes visuais em `tests/golden/`.

**Critérios de aceite:**
- [ ] **Testes visuais com folga medida** (decisão do [ADR-0018](../adr/0018.md)). O teste mostra a imagem de diferença quando falha, e:
  - **sem placa de vídeo (vello_cpu):** falha quando qualquer pixel muda;
  - **com a placa de vídeo:** falha quando mais de 3 pixels mudam, ou quando algum pixel muda mais de 1 nível em 255. Qualquer outra folga precisa vir de uma medição registrada no plano da entrega.
- [ ] **Mesma imagem sem placa em qualquer máquina** (decisão do [ADR-0020](../adr/0020.md)):
  - os testes visuais sem placa desenham com o modo SIMD fixado em AVX2 e com o número de linhas de execução fixo;
  - são compilados sem `target-cpu` nem `target-feature`, e um teste confere que o modo mínimo da compilação é SSE2;
  - numa máquina sem AVX2 (Mac com chip Apple, Windows em ARM), o teste visual sem placa é pulado com uma mensagem visível, que diz o motivo e o modo detectado; ele continua obrigatório no GitHub e no PC do dono;
  - uma cena de referência com desfoque, degradê, traçado e imagem sai idêntica, pixel a pixel, no Windows e no Linux. O plano da entrega avalia o recurso `libm` do vello_cpu.
- [ ] **Placa × sem placa:** a comparação entre as duas não usa a média da diferença. Ela falha quando algum pixel muda 128 ou mais em 255, e um teste prova que isso detecta um único objeto pequeno faltando (achado 1 da [auditoria da entrega 0.5](../auditoria/2026-10-29-fase-0.5.md)).
- [ ] Mexer num objeto redesenha só os tiles afetados (verificado por contador).
- [ ] **Tela × exportação, automático** (achado B11 da auditoria da Fase 0). Um teste automático compara a imagem que vai para a tela com a exportação PNG da mesma vista, dentro da folga da placa de vídeo. Na prova da 0.3, essa comparação era manual (botão "Capturar a tela e comparar").
- [ ] **Computador sem DirectX 12 nem Vulkan** (achado B10 da auditoria da Fase 0). Quando a placa de vídeo não oferece nenhum dos dois, o programa não cai e mostra uma mensagem clara, que diz o que falta. Um teste simula a falta de placa compatível.
- [ ] **Medição de tempo completa** (achado M4 da auditoria da Fase 0). Os números de desempenho de cada quadro trazem, separados: o tempo no processador, o tempo de trabalho da placa de vídeo (medido pela própria placa) e o momento em que a imagem é apresentada na tela. O plano da entrega diz como cada um é medido, e o código que mede fica no repositório.
- [ ] **Muitos degradês, velocidade** (limitação 1 do [ADR-0018](../adr/0018.md); pré-requisito da Fase 1a). A solução é provada antes na entrega 0.9 da [Fase 0](fase-0.md); aqui ela entra no produto. Na GTX 1660, com a cena de 10.000 retângulos, cada um com degradê próprio, durante zoom e pan:
  - **obrigatório:** 99% dos quadros abaixo de 16,7 ms (60 Hz);
  - **desejável:** 99% dos quadros abaixo de 6,9 ms (144 Hz).
- [ ] **Muitos degradês, estabilidade** (limitação 2 do [ADR-0018](../adr/0018.md); pré-requisito da Fase 1a). Obrigatório: um arquivo com 50.000 degradês diferentes abre sem derrubar o programa e **desenha corretamente, ou mostra um aviso visível**. Ele pode ficar lento. Não pode cair, nem trocar degradês por cor sólida (ou por qualquer simplificação) em silêncio.

### N.4 Texto, cor e interpolação

- **`bivar-text`:** composição de texto de ponto e em caixa, com quebra de linha e recursos OpenType básicos.
- **`bivar-color`:** cor com espaço de cor (nunca só RGB), perfis ICC e base CMYK.
- **`bivar-interp`:** correspondência de pontos e interpolação de paths (base da mesclagem e das fontes variáveis).

**Critérios de aceite:**
- [ ] O texto "Açaí à brasileira — ótimo" renderiza com acentos corretos e kerning da fonte.
- [ ] **Fuzzing de fontes** (ADR-0011 e [ADR-0020](../adr/0020.md)): um alvo de fuzzing passa fontes malformadas pelo carregamento, pelo *shaping* e pela extração de contornos (e pelo hinting, se ele for ligado) sem derrubar o programa.
- [ ] **Fonte ou letra faltando avisa** (princípio 5 e ADR-0020): quando a fonte do documento falta e outra é usada, ou quando uma letra não existe na fonte e vira o desenho vazio (`.notdef`), aparece um aviso visível.
- [ ] **Sem fontes do sistema por engano** (ADR-0020): as fontes são carregadas por bytes, e uma checagem automática falha se o `fontique` aparecer com o recurso `system` sem um ADR que o autorize.
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
- [ ] **Painel de conflitos** (ADR-0019; especificado na 0.7): mostra os conflitos e os avisos de um documento de teste em duas seções, "Conflitos" (exigem ação) e "Avisos" (o mesmo parâmetro e o raster, que podem ser dispensados), com as ações de cada tipo. Dispensar um aviso tira o item do painel, e o registro continua no histórico. Ele é usado inteiro só pelo teclado, o NVDA lê cada item e o tipo dele, e cor nunca é o único sinal.

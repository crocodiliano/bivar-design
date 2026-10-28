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
- [ ] O dono consegue clonar o repositório e rodar `cargo test` no PC dele, com uma mensagem de sucesso.
- [ ] Um commit sem `Signed-off-by` é sinalizado.
- [ ] Adicionar uma dependência de teste com licença incompatível faz o `cargo deny` falhar.

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
- [ ] **Registro:** o resultado e a biblioteca escolhida ficam num ADR novo, revisado pelo agente auditor.

### 0.6 Escolha das bibliotecas pendentes

Três escolhas que o Núcleo precisa. Cada uma fica registrada num **ADR próprio**, com as alternativas comparadas em licença, manutenção, quantidade de código `unsafe` ou em C, tamanho e segurança. Os testes de cada item são provas de conceito **descartáveis**.

**Critérios de aceite:**
- [ ] **Motor JS para scripts (ADR-0013):**
  - um ADR compara pelo menos três motores e escolhe um;
  - numa prova mínima, um script chama uma função do Bivar e recebe a resposta;
  - um script com laço infinito é interrompido por limite de tempo, sem travar o programa;
  - um script não consegue abrir arquivos nem acessar a rede, a não ser pelo que o Bivar oferecer.
- [ ] **Gerenciamento de cor (ADR-0008):**
  - um ADR compara as opções e escolhe uma;
  - numa prova mínima, uma cor é convertida de sRGB para CMYK com um perfil ICC e de volta, e o resultado bate com valores de referência dentro de uma tolerância definida no ADR;
  - a leitura de perfis ICC respeita o ADR-0011 (leitura de arquivos externos só em Rust) ou registra uma exceção justificada.
- [ ] **CRDT (ADR-0005), com o teste de velocidade da Loro:**
  - um ADR compara a Loro com pelo menos uma alternativa e escolhe uma;
  - o teste de velocidade aplica 100.000 operações num documento de teste e junta duas cópias que receberam 10.000 operações diferentes cada uma, ao mesmo tempo;
  - são medidos o tempo, a memória e o tamanho salvo, na máquina de referência;
  - as duas cópias terminam idênticas depois da junção;
  - os números ficam registrados no ADR.
- [ ] **Revisão:** os três ADRs novos estão com status "Revisado por IA".
- [ ] **Licenças:** `cargo deny check` e `cargo audit` passam nas provas.

## Antes do Núcleo

**O Núcleo só começa quando as entregas 0.4, 0.5 e 0.6 estiverem concluídas e revisadas pelo agente auditor.**

## Fora do escopo da Fase 0

Qualquer funcionalidade de produto. As provas de conceito são **descartáveis** e não viram base de código.

# Fase 0 — Fundações

**Objetivo:** provar que as escolhas técnicas de maior risco funcionam na máquina de referência e deixar o repositório pronto para o trabalho seguro com o Claude Code.

**Máquina de referência:** Windows 11, Ryzen 7 5700X, 16 GB de RAM, GTX 1660 (6 GB), mesa digitalizadora com pressão.

## Entregas

### 0.1 Repositório e regras

- **Repositório público no GitHub.** O auditor entra como colaborador mais tarde.
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
- [ ] **Estilo:** os painéis reproduzem o espírito do mockup: fundo escuro, cantos arredondados, tipografia limpa e roxo como destaque. O dono aprova visualmente.
- [ ] **Janela separada:** um painel é arrastado para fora da janela principal, vira uma janela própria do Windows e pode ir para um segundo monitor. Depois, volta a se acoplar.
- [ ] **Leitor de tela:** com o NVDA ligado, navegar pelos painéis com Tab faz o NVDA ler o nome de cada controle.
- [ ] **Teclado:** todos os controles podem ser alcançados e usados sem mouse, com o foco sempre visível.
- [ ] **Caneta:** desenhar no canvas com a mesa digitalizadora mostra na tela, em tempo real, a pressão (0–1), a inclinação e se a ponta borracha está em uso.
- [ ] **Escala:** com a escala do Windows em 150%, a interface fica nítida e proporcional.

**Resultado esperado:** o ADR-0009 é atualizado com o framework escolhido e as limitações encontradas, e o auditor o aprova.

### 0.3 Prova de conceito do renderizador (ADR-0007)

Um programa mínimo que desenha com **Vello sobre wgpu** na GTX 1660.

**Critérios de aceite:**
- [ ] Renderiza 10.000 retângulos com cantos arredondados, traçado e degradê, com navegação (zoom e pan) fluida, sem travadas perceptíveis.
- [ ] Renderiza texto com uma fonte OTF em vários tamanhos, nítido no zoom.
- [ ] A mesma cena exportada como PNG é idêntica pixel a pixel ao que aparece na tela, no mesmo zoom (primeiro teste visual).
- [ ] A mesma cena renderiza também **sem GPU**, no modo CPU, para PNG.

**Resultado esperado:** o ADR-0007 é confirmado ou ajustado.

### 0.4 Validação dos ADRs

- [ ] O auditor revisa os ADRs 0001 a 0016 e marca cada um como Aceito ou pede alterações.
- [ ] O auditor recomenda as bibliotecas pendentes: motor JS, gerenciamento de cor e CRDT (com benchmark da Loro).

## Fora do escopo da Fase 0

Qualquer funcionalidade de produto. As provas de conceito são **descartáveis** e não viram base de código.

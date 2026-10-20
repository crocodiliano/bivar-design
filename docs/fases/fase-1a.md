# Fase 1a — Vetor: ilustração

**Objetivo:** ilustração vetorial completa e não destrutiva. Este é o primeiro marco em que o Bivar é útil sozinho, e ao fim dele **o código abre ao público**.

**Referências de uso:** Affinity Designer, Illustrator, Figma (parte de desenho).

**Pré-requisito:** as limitações 1 e 2 do [ADR-0018](../adr/0018.md) estão resolvidas, ou seja, os critérios de degradês da entrega N.3 do [Núcleo](nucleo.md) passaram. A Fase 1a não começa sem isso.

## Escopo

- **Canvas e organização:**
  - canvas infinito, zoom e pan;
  - artboards com os botões "+" nas quatro direções;
  - fundo em hachura fora do artboard.
- **Guias e grades:**
  - réguas e guias em qualquer ângulo (0–360°);
  - guias paramétricas (nós ligadas a objetos) e encaixe;
  - grades predefinidas: áurea, terços, modular, linha de base e isométrica.
- **Ferramentas:**
  - seleção e seleção direta (pontos);
  - caneta Bézier e lápis com suavização e **pressão de caneta** → perfil de largura;
  - formas paramétricas: retângulo (cantos), elipse, polígono, estrela e linha;
  - texto de ponto e em caixa.
- **Operações:**
  - booleanas não destrutivas (união, subtração, interseção, exclusão);
  - alinhar e distribuir;
  - grupos e máscaras.
- **Pilha do painel direito** (projeção dos nós):
  - vários traçados: espessura, perfil de largura, pontas, junções, tracejado, alinhamento interno/central/externo;
  - vários preenchimentos: sólido, degradê linear, radial e angular, e imagem;
  - FX: sombra, desfoque, modos de mesclagem e opacidade;
  - ícones de visibilidade, bloqueio, "customizado" e conflito.
- **Mesclagem entre formas** (nó Mesclagem):
  - passos por quantidade, por distância ou com cor suave;
  - troca de espinha e orientação;
  - mesclagem de degradês e traçados.
- **Nodetree no rodapé:** recolhida e expandida, com trilha Documento › Artboard › Objeto e edição avançada que ativa o indicador "customizado".
- **Painel de camadas:** hierarquia de artboards e objetos, com preview.
- **Exportação:** SVG, PNG e PDF, por artboard ou por recorte, em 1x, 2x e 3x, com **exportação limpa** (aviso de camadas ocultas e metadados).
- **Importação:** SVG e PDF (vetorial simples).
- **Atalhos:** esquema próprio + perfis Figma, Affinity e Illustrator.

**Fora da 1a** (vai para a 1b ou depois): componentes, auto-layout, protótipo, handoff, degradê de malha, vetorização, pincéis artísticos e deformação por malha.

## Critérios de aceite (o dono testa usando o programa)

- [ ] **Pilha = nós:** criar um retângulo, adicionar Traçado 1 e Traçado 2 e arrastar o Traçado 2 para cima do 1. O 2 passa a aparecer por cima no canvas, e a nodetree mostra a nova ordem.
- [ ] **Conflito sem perda:** criar uma estrela de 5 pontas, arrastar um vértice à mão e mudar para 7 pontas. A camada mostra o estado de conflito, e ao abri-la o dono escolhe reaplicar, assar ou descartar. Nada some sem aviso.
- [ ] **Referência:** usar a mesma cor em três objetos como referência e editá-la. O painel avisa "afeta 3 objetos", e "desvincular" separa um deles.
- [ ] **Customizado:** fazer na nodetree uma ligação que os sliders não representam. A camada mostra o indicador "customizado".
- [ ] **Mesclagem:** mesclar um quadrado vermelho e uma estrela azul em 8 passos, depois editar a estrela. Todos os passos se atualizam, sem cruzamentos.
- [ ] **Booleana viva:** subtrair um círculo de um quadrado e mover o círculo depois. O resultado se atualiza.
- [ ] **Caneta:** um traço com o lápis varia de espessura conforme a pressão da caneta.
- [ ] **Guia angular:** criar uma guia a 30° ligada a um objeto, girar o objeto, e a guia acompanha. Objetos encaixam nela.
- [ ] **Exportação limpa:** com uma camada oculta, exportar PDF gera o aviso "este arquivo contém conteúdo oculto".
- [ ] **Desfazer:** cada ação acima se desfaz com Ctrl+Z, na ordem certa.
- [ ] **Queda:** fechar o programa à força no meio da edição e reabrir. O trabalho está lá.
- [ ] **Acessibilidade:** com o NVDA, o dono navega pelas camadas e ouve nome, tipo e estado (oculta, bloqueada, em conflito) de cada uma. Todas as ferramentas acessíveis por teclado.
- [ ] **Desempenho:** um documento com 5.000 objetos navega com fluidez na máquina de referência.
- [ ] **Testes visuais** aprovados pelo dono para: traçados, degradês, booleanas, mesclagem e cada formato de exportação.

## Ao concluir

- Scripts JS, gravador de macros e modo terminal (próxima entrega transversal).
- Checklist de abertura do repositório: nome verificado no INPI, README em português e inglês, CONTRIBUTING com DCO e política de IA, e documentação traduzida.

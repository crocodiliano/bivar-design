# Bivar — Plano de Execução

Oct 20, 2026 · @bruno

> **Esta é a versão oficial do plano.** Ela vive no repositório e é atualizada junto com os ADRs e as especificações das fases.

## Visão geral

Bivar é um programa de design open source (GPL-3.0) que une, numa só interface, vetor e UI, tipografia, animação, diagramação editorial, foto, áudio e 3D leve. Por baixo de tudo há uma nodetree como fonte da verdade, e o usuário trabalha com camadas e painéis comuns. O projeto é construído em fases. Cada fase entrega algo usável antes da próxima começar.

**Princípios que valem para todas as decisões:**

- **O grafo é a fonte da verdade.** Camadas e painéis são uma projeção amigável dele.
- **Tudo é operação.** Cliques, scripts, colegas via P2P e o desfazer passam pelo mesmo sistema de operações no grafo.
- **Personas são módulos.** O núcleo é uma plataforma, e cada aba é um módulo, inclusive as que a comunidade criar.
- **Nada se perde em silêncio.** Conflitos e edições que quebram o procedural ficam visíveis e são resolvidos pelo usuário.
- **Local, offline e privado.** Zero telemetria e nenhum servidor obrigatório.
- **Acessível sempre.** Leitor de tela, teclado completo e cor nunca como único sinal.
- **Ciência com honestidade.** Recursos de ergonomia são "baseados em estudos", com referências, nunca "comprovados".

**Restrições do projeto:**

- **Custo zero na construção:** só bibliotecas, fontes, ícones e formatos livres, com hospedagem e compilação automática gratuitas. O plano do Claude fica fora dessa conta. Por falta de orçamento, não há auditor humano pago por enquanto.
- **Plataforma:** Windows primeiro, com base multiplataforma. A máquina de referência é um Ryzen 7 5700X com 16 GB de RAM e uma GTX 1660 de 6 GB.
- **Linguagem:** Rust em todo o projeto. Código em inglês, documentação em português e inglês.
- **Equipe:** o dono do projeto (que não programa) trabalha com o Claude Code. A revisão é feita por IA (o agente auditor), por ferramentas automáticas e por voluntários da comunidade, até que haja uma auditoria humana. O repositório é público no GitHub desde a Fase 0, de forma definitiva: qualquer pessoa pode ler e revisar. Depois da Fase 1a, a comunidade também entra contribuindo com código.

## Escopo, equipe e papéis

O projeto é de longo prazo, com a arquitetura pensada para as 7 personas e a implementação feita em fases. A ordem segue o uso real do dono: Affinity e Figma são os mais usados, depois FontForge e Rive.

| Papel | Quem | Responsabilidade |
| --- | --- | --- |
| Dono do produto, designer e testador | Você | Decide o quê, aprova planos, testa com critérios de aceite e aprova as imagens de referência |
| Implementação | Claude Code | Escreve código e testes seguindo o CLAUDE.md, os ADRs e as especificações |
| Auditoria por IA | Agente `.claude/agents/auditor.md` | Revisa os ADRs, o código, os testes e as dependências, com prioridade para os módulos críticos: modelo de dados, formato de arquivo, rede e segurança. Leva os ADRs até o status "Revisado por IA", com a aprovação do dono |
| Ferramentas automáticas | Compilação no GitHub | Formatação, Clippy, testes, licenças, vulnerabilidades, cobertura de testes e, sob demanda, testes de mutação |
| Revisão humana | Voluntários da comunidade (por enquanto) | Revisão de quem tem experiência em Rust. Só ela leva um ADR ao status "Aceito" |
| Comunidade | Depois da Fase 1a | Contribui por DCO, inclusive com código feito com IA, desde que declarado e revisado |

**Referências de mercado que calibram o plano:**

- **[Graphite](https://lwn.net/Articles/1051242):** a referência técnica mais próxima. Também é Rust com um grafo de nós procedural. Começou em 2021, já teve mais de 170 contribuidores e ainda está em alpha. Ele já tem grafos aninhados (subgrafos) e um painel de camadas que é uma vista equivalente do grafo ([recursos](https://graphite.art/features/), [redes e nós](https://graphite.art/volunteer/guide/graphene/networks-and-nodes/), conferido em 2026-10-28).
- **[Affinity gratuito e unificado](https://www.footagesecrets.com/news/affinity-by-canva/):** mostra que existe demanda por um app único, e também que esse espaço já tem um concorrente grátis.
- **O diferencial do Bivar:** camadas como vista de um grafo não destrutivo já existem no Graphite. O diferencial é **um mesmo grafo para todas as personas** (vetor, tipografia, editorial, animação, foto, áudio e 3D leve, numa só interface) e a pilha Traçado/Preenchimento/FX do painel direito como projeção amigável dos nós de cada objeto.

Enquanto o Bivar cresce, o dono usa alternativas gratuitas: Affinity, Penpot, Krita, darktable, Fontra/FontForge e Blender.

## Arquitetura do núcleo

O documento é um grafo de nós hierárquico, e toda mudança nele é uma operação. Essa é a decisão mais valiosa do projeto e a primeira a ser revisada.

### Modelo de dados (ponto 1)

- **Cadeia de cada objeto:** Forma → efeitos na ordem da pilha do painel direito → Saída. Reordenar a pilha reordena os nós.
- **Grafo hierárquico:** o grafo do documento contém o grupo de nós de cada objeto. A nodetree alterna entre os dois níveis por uma trilha de navegação, do tipo Documento › Artboard 1 › Quadrado 1.
- **Tudo é operação:** criar nó, conectar, mudar parâmetro, reordenar. A mesma base serve a interface, o desfazer, os scripts e a colaboração P2P.
- **Nós compartilhados:** cada nó tem um dono, que é o objeto onde foi criado. Usá-lo em outro lugar cria uma referência, com ícone de vínculo, aviso "afeta N objetos" e opção de desvincular.
- **Edição manual sobre algo procedural:** fica guardada como ajuste. Se uma mudança anterior a invalidar, o nó entra em **estado de conflito** visível. O usuário então escolhe entre reaplicar, assar ou descartar.
- **Indicador "customizado":** aparece quando o grafo contém algo que o painel visual não representa. O mesmo padrão vale para recursos OpenType, protótipos e o perfil do runtime.
- **Componentes e instâncias:** são grupos de nós com valores sobrescritos. Páginas-mestre e glifos compostos usam o mesmo mecanismo.
- **No núcleo desde o início:** motor de composição de texto, módulo de interpolação (usado na mesclagem e nas fontes variáveis) e reprodução de áudio com forma de onda.

### Personas e memória (ponto 2)

- **Personas são módulos.** Cada uma registra no núcleo os seus tipos de nó, ferramentas e painéis. As oficiais usam a mesma interface interna que os plugins usariam.
- **Memória:** só a persona ativa fica acordada, e a troca de aba pode levar alguns segundos. O que pesa e é liberado são dados, memória de GPU e threads, não os painéis.
- **Renderização sempre disponível:** objetos de personas dormindo aparecem no canvas por cache/proxy na resolução da tela.
- **Painéis emprestados entre personas:** a estrutura permite desde o início, e a implementação vem depois. A exceção é a onda de áudio na timeline, que já vem do núcleo.

### Renderização e cor (ponto 3)

- **Base comum:** wgpu (DirectX 12 ou Vulkan no Windows), com renderizadores para 2D/texto, raster e 3D e um compositor que junta tudo.
- **Renderizador 2D:** [Vello](https://linebender.org/blog/tmil-25/), atrás de uma camada de isolamento. O Skia é o plano B, opcional na compilação e fora da versão padrão.
- **Tela e exportação:** o mesmo motor nas duas, com modo sem GPU para scripts e exportação em lote.
- **Renderização incremental em blocos (tiles),** ligada ao cache.
- **Cor:**
  - perfis ICC, CMYK, cores especiais (sem as bibliotecas Pantone, que são licenciadas), sobreimpressão e soft proofing;
  - exportação PDF/X vetorial;
  - pipeline em 16/32 bits.
- **Imagens grandes:** pixels em blocos, com disco de rascunho no SSD para o que não está na tela.

## Stack, formato de arquivo e segurança

O projeto usa Rust em toda parte. O framework de interface é decidido por uma prova de conceito na Fase 0, e o arquivo de projeto é um SQLite com especificação aberta.

### Stack (ponto 4)

- **Rust:** segurança de memória ao ler arquivos não confiáveis e concorrência sem condições de corrida. O compilador rígido também barra boa parte dos erros de IA antes de o programa rodar.
- **Organização:** um workspace com módulos (crates) separados para o núcleo e para cada persona.
- **Framework de interface:** egui é o primeiro candidato e Slint o segundo. A prova de conceito testa quatro requisitos:
  1. visual bonito e limpo como o mockup;
  2. painel arrastado para fora como janela própria do Windows;
  3. leitor de tela NVDA lendo os painéis, via AccessKit;
  4. pressão, inclinação e borracha da mesa digitalizadora (Windows Ink/WinTab).

### Formato de arquivo (ponto 5)

- **Arquivo único em SQLite:**
  - salvamento atômico e incremental;
  - carregamento parcial por persona;
  - recuperação de queda pelo diário de operações;
  - versões com migração.
- **Especificação aberta e documentada.**
- **Importação e exportação só de formatos abertos ou publicamente documentados, sem engenharia reversa:**

| Área | Formatos |
| --- | --- |
| Vetor e documento | SVG, PDF, PDF/X |
| Imagem | PNG, JPEG, WebP, AVIF, TIFF, PSD (leitura, especificação publicada) |
| Foto RAW | DNG e RAW de câmera via bibliotecas open source |
| Fontes | OTF, TTF, WOFF2, UFO, designspace, .glyphs |
| Animação | Lottie, GIF, APNG, vídeo AV1/VP9, sequência de imagens |
| Áudio | WAV, FLAC, Opus, MP3 |
| 3D | glTF, OBJ, STL |
| Editorial | EPUB 3, PDF acessível (PDF/UA) |
| Tokens | Formato do W3C Design Tokens Community Group, CSS |

- **Caminho de migração:** Affinity exporta SVG, PDF e PSD. Figma exporta SVG e PDF. FontForge exporta UFO.

### Segurança (pontos 3 e 5)

- **Leitura de arquivos:** todo leitor de formato externo é escrito em Rust e testado com fuzzing. O renderizador só recebe comandos de desenho já validados.
- **Abrir arquivo nunca executa código.** Scripts existem só na máquina do usuário, nunca dentro de um arquivo de projeto.
- **Dependências:** poucas, com versões travadas, auditoria automática de vulnerabilidades e checagem de licenças.
- **Código unsafe:** só com justificativa registrada.
- **Criptografia opcional com senha,** desligada por padrão. Só bibliotecas consolidadas, com aviso de "sem a senha, é irrecuperável" e chave de recuperação opcional.
- **Privacidade:** zero telemetria. A verificação de atualizações é opcional e vem desligada.
- **Exportação limpa:** antes de exportar, avisa sobre camadas ocultas, metadados como GPS e autor, e conteúdo fora do artboard.
- **Distribuição:** as primeiras versões saem sem assinatura digital, também pelo winget. A assinatura gratuita (SignPath Foundation) vem quando o projeto se qualificar.

## Colaboração, scripts e plugins

Colaboração, scripts e plugins são três fontes de operações no mesmo grafo. Por isso todos herdam o desfazer, a sincronização e a validação sem código especial.

### Colaboração P2P (ponto 6)

- **Rede:** só rede local (LAN), com descoberta automática dos colegas. Equipes de 5 a 8 pessoas, cada uma conectada diretamente a todas. A parte de rede fica isolada, para que um relay de internet possa ser adicionado no futuro.
- **CRDT:** o modelo de dados é compatível desde o dia 1, mesmo que o P2P chegue só depois da Fase 2. A candidata é a [Loro](https://loro.dev), por ter árvore com movimentação e texto rico. Automerge e Yrs são as alternativas.
- **Arquivos grandes:** não passam pelo CRDT. Viajam à parte, identificados pelo hash do conteúdo e baixados sob demanda.
- **Regras de junção:**
  - depois de cada junção, uma validação marca grafos inválidos em estado de conflito;
  - no raster, a última pincelada vence por bloco;
  - cada pessoa desfaz só as próprias operações.
- **Offline:** cada pessoa tem a cópia completa, e as mudanças se juntam quando ela volta à rede.
- **Segurança:**
  - um par de chaves por usuário, convite por código ou QR code e criptografia de ponta a ponta;
  - operações assinadas e validadas contra a lista de acesso;
  - toda operação recebida é tratada como entrada não confiável.
- **Papéis:** dono, editor, comentarista e visualizador. "Só visualizar" impede edição, mas não garante sigilo: quem recebe os dados pode copiá-los.
- **Histórico:** registra quem fez o quê, com marcos nomeados e compactação opcional do histórico antigo.
- **Presença:** cursores, seleções e em qual persona cada colega está.

### Scripts e automação (ponto 7)

- **Linguagem:** JS/TS, com o motor escolhido na entrega 0.6 da Fase 0.
- **Execução:** um script desfaz com um único Ctrl+Z, sincroniza via P2P e passa pela mesma validação das outras operações.
- **Gravador de macros:** prioridade. Grava ações e gera um script JS editável. Escrever scripts do zero também é suportado.
- **Modo sem interface:** pelo terminal do Windows, para trabalho em lote.
- **Isolamento (sandbox):** acesso só ao documento aberto. Pastas, só as autorizadas. Internet, bloqueada.

### Plugins da comunidade (ponto 7)

- **Tecnologia:** WebAssembly. Qualquer linguagem que compile para Wasm serve, e a API é a mesma dos scripts.
- **API pública versionada,** com política de compatibilidade. Ela só é congelada depois da Fase 3.
- **Permissões:** cada plugin declara as que pede, e o usuário aprova na instalação.
- **Instalação:** a partir de arquivo. Os autores assinam, e plugins sem assinatura geram aviso.
- **Licença:** plugins são obrigatoriamente GPL-3.0. O registro oficial aceita só plugins gratuitos. A GPL não proíbe cobrar; quem mantém essa regra é a política do projeto.
- **Limitação:** plugins com shader próprio de GPU ficam para uma fase avançada, com o shader validado pelo programa.

## Ergonomia, acessibilidade e ferramentas extras

Os temas seguem o que a evidência sustenta (contraste, brilho, ofuscamento), e o canvas nunca é tingido. As ferramentas extras ganham diferenciais próprios: guias paramétricas, notas ligadas a objetos e efeitos compartilháveis como dados.

### Ergonomia e temas (ponto 8)

- **Dois temas:**
  - **Foco:** contraste alto, hierarquia forte, menos elementos visíveis.
  - **Conforto:** brilho geral menor, contraste moderado, sem ofuscamento.
  - Os dois são comunicados como "baseado em estudos", com referências.
- **Evidência considerada:**
  - filtro de luz azul provavelmente não reduz a fadiga ocular ([revisão Cochrane 2023](https://pmc.ncbi.nlm.nih.gov/articles/PMC10436683/));
  - texto escuro sobre fundo claro dá melhor desempenho de leitura ([Piepenbrock et al. 2013](https://documentacion.fundacionmapfre.org/documentacion/bib/143702.do));
  - o modo escuro reduz o ofuscamento em ambiente escuro ([NN/g](https://www.nngroup.com/articles/dark-mode/)).
- **Neutralidade de cor:** o tema nunca tinge o canvas. Um clique ativa o modo de avaliação de cor, com a interface em cinza neutro, por causa do contraste simultâneo.
- **Tokens de design:** sustentam todos os temas, incluindo alto contraste e temas da comunidade, que podem ser importados e exportados como arquivo.
- **Atalhos:** esquema próprio, com perfis Figma, Affinity e Illustrator como opção. A mesma tecla faz a mesma coisa em todas as personas.
- **Contra a fadiga cognitiva:**
  - paleta de comandos (Ctrl+K) e revelação progressiva;
  - modo foco e densidade ajustável;
  - lembrete de pausa opcional, desligado por padrão;
  - layouts de painéis salvos por persona.

### Acessibilidade (pontos 4 e 8)

- **Leitor de tela:** compatível via AccessKit, com o UI Automation do Windows.
- **Teclado:** tudo pode ser feito pelo teclado, com foco sempre visível.
- **Escala:** interface escalável e fontes maiores.
- **Cor nunca é o único sinal:** todo estado tem também forma ou ícone.
- **Movimento reduzido:** opção para desligar animações da interface.
- **O painel de camadas é a representação acessível do canvas.**

### Ferramentas extras (ponto 9)

| Ferramenta | Diferencial | Quando entra |
| --- | --- | --- |
| Guias e grades | Guias paramétricas (nós ligados a objetos), encaixe tipográfico, grades áurea, terços, modular, linha de base e isométrica | Fase 1a |
| Grade de perspectiva | Linhas de 1, 2 ou 3 pontos de fuga com encaixe. "Colar" objetos na perspectiva é uma etapa separada | Fase posterior |
| Organizador de ativos | Biblioteca de componentes, cores, estilos, fontes, pincéis e receitas de efeito (grupos de nós, compartilháveis por serem dados, não código) | Base na 1b, receitas logo depois |
| Comentários e tarefas | Um painel só: o comentário ancorado vira tarefa, com responsável e prazo. Sem calendário nem Gantt | Depois da 1b |
| Wiki do projeto | Markdown com links nos dois sentidos para artboards e componentes. Exporta para Obsidian. Não executa HTML, só carrega imagens locais. Painel acoplável ou janela própria | Depois da 1b |

## Escopo de cada persona

Cada persona tem um escopo de v1 completo, dividido em entregas menores que já são utilizáveis. Qualquer parâmetro de qualquer persona pode ser animado.

### 1 · Vetor (Illustrator, Affinity Designer e Figma)

- **1a, ilustração:**
  - canvas infinito, artboards com os botões "+", guias e grades;
  - seleção e seleção direta, caneta Bézier e lápis com suavização e pressão;
  - formas paramétricas e texto de ponto e em caixa;
  - booleanas não destrutivas;
  - traçado com perfil de largura, pontas, junções, tracejado e alinhamento;
  - preenchimento sólido, degradê linear, radial e angular, e imagem;
  - FX: sombra, desfoque, mesclagem e opacidade;
  - pilha de múltiplos traçados e preenchimentos;
  - camadas, grupos, máscaras, alinhar e distribuir;
  - **mesclagem entre formas**: passos, distância, cor suave e troca de espinha. O ponto de atenção é interpolar formas com quantidades diferentes de pontos;
  - exportação SVG, PNG e PDF por artboard ou recorte, em 1x, 2x e assim por diante.
- **1b, UI:**
  - componentes, instâncias e variantes;
  - auto-layout e restrições (constraints);
  - variáveis e tokens;
  - protótipo simples: links entre telas e transições;
  - biblioteca de componentes;
  - **handoff:** modo de inspeção, CSS e tokens, e um pacote de inspeção em HTML autocontido.
- **Depois:** degradê de malha, vetorização de imagem, pincéis artísticos e deformação por malha.

### 2 · Tipografia (FontLab e FontForge, só alfabeto latino)

- **2a, fontes estáticas:**
  - editor de glifos e mapa Unicode, com conjuntos do Latin Core ao latim estendido;
  - métricas, incluindo as verticais com proteção contra erros;
  - espaçamento e kerning por pares e por classes;
  - componentes e âncoras, com acentos gerados automaticamente;
  - recursos OpenType visuais e automáticos, com editor de código .fea opcional e o indicador "customizado";
  - famílias inteiras num só projeto, com tabelas de nomes automáticas;
  - hinting automático e validador embutido;
  - **fonte viva:** usada direto nas abas Vetor e Interface, sem exportar;
  - exportação OTF, TTF e WOFF2; importação UFO e .glyphs.
- **2b, fontes variáveis:**
  - masters e eixos (peso, largura, tamanho óptico, inclinação e eixos customizados);
  - instâncias nomeadas e designspace;
  - verificador visual de compatibilidade;
  - sliders de eixos ao vivo no documento.
- **Base técnica:** fontc e fontations (Google, em Rust) e kurbo.
- **Depois:** hinting manual e fontes coloridas (COLRv1).

### 3 · Animação (Rive e Cavalry)

- **3a, clássica:**
  - timeline com keyframes e editor de curvas;
  - suavização e física de mola;
  - sincronia com áudio: onda na timeline, marcadores e o nó "áudio → número".
- **3b, interativa:**
  - máquina de estados completa (o protótipo avançado);
  - gatilhos sonoros;
  - rigging com ossos, cinemática inversa, pesos e deformação de malha.
- **3c, procedural:** repetidores, comportamentos (ruído, oscilação, defasagem, seguir caminho) e animação guiada por dados (CSV).
- **Exportação:** vídeo AV1/VP9, GIF/APNG, sequência de imagens e Lottie.
- **Runtime interativo:** projeto à parte, mais tarde. É o mesmo renderizador compilado para WebAssembly, com um "perfil de runtime" de nós suportados.

### 4 · Interface (diagramação editorial: InDesign e Affinity Publisher)

- **4a, base:**
  - páginas, spreads, sangria e marcas de corte;
  - páginas-mestre como componentes;
  - caixas de texto encadeadas;
  - estilos de parágrafo, caractere e objeto;
  - composição Knuth-Plass, hifenização pt-BR (padrões TeX), viúvas e órfãs, capitulares e alinhamento óptico;
  - texto contornando objetos, tabelas e numeração;
  - ortografia pt-BR (Hunspell);
  - imagens vinculadas ou incorporadas, com função "empacotar";
  - preflight e PDF/X.
- **4b, documento longo:** notas de rodapé e de fim, sumário automático, referências cruzadas, cabeçalhos dinâmicos, seções e índice remissivo.
- **4c, digital e dados:**
  - EPUB 3 fixo e refluível;
  - PDF acessível, obrigatório para e-books na UE pelo [European Accessibility Act](https://commission.europa.eu/strategy-and-policy/policies/justice-and-fundamental-rights/disability/union-equality-strategy-rights-persons-disabilities-2021-2030/european-accessibility-act_en);
  - mala direta simples por CSV.

### 5 · Foto (Lightroom e Photoshop)

- **5a, revelação:**
  - revelação RAW como nó inicial: balanço de branco, exposição, curvas, HSL, color grading, ruído e nitidez;
  - correção de lente via lensfun;
  - ajustes e máscaras (pincel, degradê, faixa de luminosidade e de cor);
  - coleção de cerca de 20 fotos por projeto, com marcação, comparação e sincronização de ajustes;
  - exportação em lote.
- **5b, retoque:**
  - pincéis com pressão, carimbo e recuperação (sem IA), subexposição e superexposição;
  - seleções;
  - camadas, máscaras de camada, modos de mesclagem e transformações.
- **5c:** liquify e ferramentas avançadas.
- **Sem IA na v1:** o sistema de máscaras aceita "fontes de máscara" de plugins no futuro.

### 6 · Áudio (referência: Audacity)

- **6a, base:**
  - timeline multipista compartilhada com a Animação;
  - edição de clipes, fades e crossfades;
  - efeitos como nós: equalizador, compressor, limitador, gate, redução de ruído, reverb, delay, tom e velocidade;
  - medidor de loudness (LUFS) e exportação.
- **6b:**
  - espectrograma e detecção de batidas;
  - **gerador de sons procedural:** osciladores, ruído, envelope ADSR, filtros e variação de tom, com predefinições, geração de variações e som vivo ligado à máquina de estados.
- **Usos:** SFX de interface, SFX de animação e trilha. Narração e gravação ficam para o futuro. Sem plugins VST3/CLAP.

### 7 · 3D (referência: Spline)

- **7a, base:**
  - extrusão (com chanfro) e revolução a partir de paths do Vetor, que continuam editáveis;
  - primitivas;
  - materiais realistas (PBR, vidro) e ilustrativos (flat, toon, contorno), alternando como nó;
  - luzes, HDRI CC0, câmeras (incluindo a isométrica) e sombras;
  - integração como camada nas outras abas;
  - importação glTF do Blender;
  - exportação PNG, vídeo e glTF.
- **7b:** nó "3D → Vetor", com paths que continuam ligados ao 3D. Funciona bem com estilos flat e toon.
- **Futuro:** path tracing.

## Roadmap

O projeto tem 11 etapas: fundações, núcleo, as sete personas na ordem de uso real e um bloco futuro. O código é aberto desde a Fase 0, e a comunidade passa a contribuir com código ao fim da Fase 1a. Não há datas: cada etapa termina quando os critérios de aceite das suas entregas passam.

*(Diagrama do roadmap: ver `fases/README.md`.)*

As funções que atravessam o programa entram ao fim da fase indicada à direita. A API de plugins só é congelada depois que três personas provaram que ela é suficiente.

## Método de trabalho com o Claude Code

Você conduz o projeto por documentos, critérios de aceite e testes visuais, sem escrever código. O Claude Code implementa tarefas pequenas, e o agente auditor revisa as decisões e os módulos críticos.

**Documentos que guiam o Claude Code** (todos no repositório):

- **CLAUDE.md:** regras permanentes, lidas em toda sessão. Reúne princípios de arquitetura, regras de segurança, obrigação de testes e convenções.
- **docs/adr/:** um registro de decisão por escolha de arquitetura, com contexto, decisão e alternativas descartadas. Cada um passa por três níveis de status: Proposto → Revisado por IA → Aceito (este, só com revisão humana).
- **docs/fases/:** a especificação de cada entrega, com critérios de aceite em linguagem simples que você testa usando o programa.

**O ciclo de cada tarefa:**

1. **Tarefa pequena:** uma função por vez, nunca uma aba inteira.
2. **Plano antes do código:** o Claude Code apresenta o plano e você aprova.
3. **Implementação com testes,** seguida das checagens automáticas de formatação, erros comuns, vulnerabilidades e licenças.
4. **Você testa** com os critérios de aceite da entrega.
5. **Salvamento no Git** com descrição clara. Cada salvamento é um ponto ao qual dá para voltar.
6. **Revisão do agente auditor** a cada entrega, com prioridade para os módulos críticos. Do Núcleo em diante, o dono também dispara os testes de mutação antes de fechar cada entrega.

**Testes que protegem o projeto:**

- **Testes visuais (golden images):** um arquivo de exemplo é renderizado e comparado com uma imagem de referência que você aprovou. Sem placa de vídeo, se um pixel mudar, o teste avisa. Com a placa, que não repete o desenho bit a bit, o teste aceita só a variação medida no ADR-0018: até 3 pixels, em 1 nível de 255.
- **Testes de unidade e de propriedades** no núcleo.
- **Fuzzing** em todo leitor de formato externo.
- **Scripts JS** usados para testar o programa de ponta a ponta, a partir da Fase 1a.

**Onde os testes rodam** (o repositório é público, e repositórios públicos não gastam cota de minutos do GitHub):

- os testes rodam no seu PC;
- a compilação automática roda em Linux a cada envio e a cada pedido de junção (*pull request*);
- as compilações para Windows e Mac são feitas sob demanda.

**O que você aprende, em nível de leitura:**

- Git: salvar, criar ramificações e voltar atrás;
- ler o resultado de um teste;
- noções de Rust pelo [The Rust Book](https://doc.rust-lang.org/book/), que é gratuito.

## Open source, licença e nome

O projeto usa GPL-3.0, inclusive para plugins, e tem o código aberto desde a Fase 0, num repositório público no GitHub. O nome "Bivar" está livre no registro de pacotes do Rust. Porém, o usuário "bivar" do GitHub já é de outra pessoa, e existe uma empresa de informática com esse nome no Rio de Janeiro. A busca no INPI e na WIPO, feita em 20/10/2026, não encontrou nenhuma marca ativa nas classes 9 e 42. O nome está liberado, com risco baixo.

### Licença e contribuição

- **GPL-3.0 para o programa e para os plugins,** como em Blender, Krita, GIMP e Inkscape.
- **O que a GPL permite e exige:**
  - o uso é livre, inclusive comercial;
  - os arquivos e as artes criados com o programa pertencem ao usuário;
  - quem modificar e distribuir o programa tem que distribuir o código.
- **Compatibilidade:** as licenças das bibliotecas discutidas (MIT, Apache-2.0, MPL-2.0, Slint, VST3 e OFL) são compatíveis.
- **Contribuições:** pelo DCO (Developer Certificate of Origin), sem CLA.
- **Contribuições feitas com IA:** aceitas, com declaração de uso, comportamento testado por uma pessoa e código revisado por IA e por ferramentas automáticas. O aviso "feito com auxílio de IA" fica na interface e no README.
- **Direito autoral:** em algumas jurisdições, a proteção de código gerado majoritariamente por IA é incerta. Por isso, os ADRs registram o papel humano nas decisões. Isto não é aconselhamento jurídico.

### Verificação do nome "Bivar" (feita em 20/10/2026)

| Onde | Resultado | O que fazer |
| --- | --- | --- |
| crates.io (pacotes Rust) | Livre: o pacote "bivar" não existe | Reservar o nome publicando um pacote vazio (o repositório já é público) |
| GitHub, usuário "bivar" | Ocupado por uma conta pessoal ([github.com/bivar](https://github.com/bivar)), de ciência de dados, sem relação com design | Usar uma organização como "bivar-app" ou "bivar-editor". A busca de repositórios não pôde ser automatizada; conferir manualmente |
| Empresas no Brasil | Existe a [Bivar Serviços de Informática Ltda](https://www.oficinadanet.com.br/empresas/bivar-servicos-de-informatica-ltda), no Rio de Janeiro, da área de TI. Não se sabe se tem marca registrada | Verificar no INPI se ela registrou "Bivar" na classe 42 |
| INPI (marcas) | Busca manual, exata e aproximada, nas classes 9 e 42: nenhum resultado | Nada a fazer. Repetir antes de cada lançamento importante em [busca.inpi.gov.br](https://busca.inpi.gov.br/pePI/): "Marca" → pesquisa por radical "bivar", classes 9 e 42 |
| WIPO e TMview (internacional) | Classe 9: só uma marca britânica de 1999, expirada (Aesa Capacitors). Classe 42: nenhum resultado | [branddb.wipo.int](https://branddb.wipo.int) e [tmdn.org/tmview](https://www.tmdn.org/tmview) |
| Busca geral | Nenhum software de design chamado Bivar foi encontrado | — |

**Leitura do resultado:** o nome "Bivar" está liberado. Use um complemento só nos endereços (ex.: organização "bivar-design" no GitHub), porque "bivar" já está ocupado lá. Para o INPI, o complemento não muda nada. A empresa do Rio tem proteção de nome empresarial na área de serviços de TI, o que traz pouco risco para um programa open source gratuito. Isto não é aconselhamento jurídico.

## Registros de decisão e pontos a verificar

Os registros de decisão (ADRs) passam pela revisão por IA (entrega 0.4 da Fase 0) antes de o Núcleo começar. Os marcados como "Alta" são os mais caros de mudar depois. O índice atualizado, com o status de cada um, está em `docs/adr/README.md`.

| ADR | Decisão | Prioridade de revisão |
| --- | --- | --- |
| 0001 | Grafo de nós hierárquico como fonte da verdade; camadas como projeção | Alta |
| 0002 | Toda mudança é uma operação (interface, desfazer, scripts, P2P) | Alta |
| 0003 | Dono + referência para nós compartilhados | Alta |
| 0004 | Edição manual sobre procedural e estado de conflito | Alta |
| 0005 | Modelo de dados compatível com CRDT desde o início (candidata: Loro) | Alta |
| 0006 | Personas como módulos; política de memória e cache/proxy | Média |
| 0007 | wgpu como base, Vello como renderizador 2D e Skia como plano B isolado | Alta |
| 0008 | Gerenciamento de cor: ICC, CMYK, cores especiais, 16/32 bits | Alta |
| 0009 | Framework de interface pela prova de conceito (egui, depois Slint) | Alta |
| 0010 | Arquivo de projeto em SQLite com especificação aberta e migrações | Alta |
| 0011 | Leitura de arquivos externos só em Rust, com fuzzing | Alta |
| 0012 | Política de dependências e de código unsafe | Média |
| 0013 | Scripts em JS/TS isolados; plugins em WebAssembly com a mesma API | Média |
| 0014 | Segurança do P2P: identidade, convite, assinatura e papéis | Alta |
| 0015 | Módulo de interpolação compartilhado (mesclagem e fontes variáveis) | Média |
| 0016 | Licença GPL-3.0, DCO e política de contribuições com IA | Baixa |
| 0017 | Interface em egui (resultado da prova de conceito do ADR-0009) | Alta |
| 0018 | Vello sobre wgpu confirmado, com ressalvas (resultado da prova de conceito do ADR-0007) | Alta |

**Pontos a verificar:**

- [x] Prova de conceito da interface: estilo, janela separada, NVDA e pressão de caneta (entrega 0.2, ADR-0017)
- [x] Prova de conceito do Vello na GTX 1660 (entrega 0.3, ADR-0018)
- [x] Busca manual do nome "Bivar" no INPI, classes 9 e 42 (feita em 20/10/2026, sem conflitos)
- [ ] Se o formato .riv do Rive é documentado o bastante para importar ou exportar
- [ ] Prova de conceito do motor de texto: shaping, kerning, ligaduras e acentos (entrega 0.5)
- [ ] Motor JS para scripts (entrega 0.6)
- [ ] Biblioteca de gerenciamento de cor (entrega 0.6)
- [ ] CRDT, com o teste de velocidade da Loro (entrega 0.6)
- [ ] Biblioteca de leitura de RAW: Rust ou C++ (antes da persona Foto: proposta no plano da entrega, revisada pelo agente auditor e decidida pelo dono)
- [ ] Ferramenta de hinting automático e como isolá-la, se for em C
- [ ] O que acontece num computador sem DirectX 12 nem Vulkan: no mínimo, uma mensagem clara em vez de queda (critério da entrega N.3). Se haverá um modo de reserva, por exemplo com o vello_cpu, fica para o plano da N.3
- [ ] Revisão por IA dos ADRs, segunda opinião e pedido de revisão à comunidade Rust (entrega 0.4)
- [ ] Revisão humana dos ADRs de prioridade Alta, quando houver um revisor com experiência em Rust

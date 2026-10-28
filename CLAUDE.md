# CLAUDE.md — Regras permanentes do projeto Bivar

> Leia este arquivo inteiro no início de toda sessão. Se uma tarefa entrar em conflito com algo daqui ou de um ADR em `docs/adr/`, **pare e pergunte** antes de seguir.

## 1. O que é o Bivar

O Bivar é um programa de design open source (GPL-3.0) escrito em Rust. Ele une numa só interface:

- vetor e UI;
- tipografia;
- animação;
- diagramação editorial;
- foto;
- áudio;
- 3D leve.

Cada uma dessas áreas é uma **persona**: uma aba com as suas próprias ferramentas e painéis. Por baixo de todas elas há uma **nodetree**, que é a fonte da verdade do documento. O usuário trabalha com camadas e painéis comuns, que são uma projeção amigável desse grafo.

O plano completo está em `docs/PLANO.md`, que é a versão oficial do plano. As decisões de arquitetura estão em `docs/adr/`.

## 2. Papéis

- **Dono do produto:** não programa. Ele decide o quê, aprova os seus planos, testa usando o programa e aprova as imagens de referência dos testes visuais. **Explique tudo em português claro, sem jargão desnecessário.**
- **Você (Claude Code):** implementa tarefas pequenas, sempre com testes, seguindo este arquivo, os ADRs e a especificação da entrega em `docs/fases/`.
- **Auditoria:** por enquanto não há auditor humano. A revisão é feita pelo agente `.claude/agents/auditor.md`, por ferramentas automáticas e por revisores voluntários da comunidade.
  - **Os ADRs têm três níveis de status, nesta ordem:**
    1. **Proposto:** escrito, ainda sem revisão.
    2. **Revisado por IA:** o agente auditor revisou, os achados foram tratados e o dono aprovou.
    3. **Aceito:** só depois de revisão humana de alguém experiente em Rust.
  - **Mudar um ADR** exige um novo ADR, revisão pelo agente auditor e aprovação do dono, com status "Revisado por IA". O status "Aceito" só vem depois de revisão humana.

## 3. Princípios de arquitetura (inegociáveis)

1. **O grafo é a fonte da verdade.** Camadas, painéis e a pilha Traçado/Preenchimento/FX apenas leem e editam o grafo. Nenhum estado de documento vive fora dele. (ADR-0001)
2. **Tudo é operação.** Toda mudança no documento é uma operação serializável no grafo, venha ela da interface, de um script, de um colega via P2P ou do desfazer. Nunca altere o documento por outro caminho. (ADR-0002)
3. **Compatível com CRDT desde o dia 1.** Toda operação nova precisa ser mesclável. (ADR-0005)
4. **Personas são módulos.** O núcleo não conhece personas específicas. Cada persona é um crate que registra seus tipos de nó, ferramentas e painéis pela mesma interface que os plugins usarão. (ADR-0006)
5. **Nada se perde em silêncio.** Ajustes invalidados, conflitos de junção e grafos que o painel não sabe representar ficam **visíveis** (estado de conflito ou indicador "customizado"), nunca descartados. (ADR-0004)
6. **O mesmo motor desenha a tela e a exportação.** A exportação normal usa o Vello na placa de vídeo, igual à tela. O modo sem placa de vídeo (vello_cpu) é para scripts e exportação em lote; ele difere da tela nas bordas suavizadas. Na cena de teste da prova, 30 a 35% dos pixels diferem, com diferença máxima de 45 a 91 em 255 e média de cerca de 1,2 em 255, como documentado no ADR-0018. Por isso o modo sem placa tem imagens de referência próprias. (ADR-0007, ADR-0018)
7. **Acessibilidade sempre:**
   - todo controle é acessível por teclado e por leitor de tela (AccessKit);
   - cor nunca é o único sinal;
   - respeite "movimento reduzido".
8. **Offline e privado:** nenhuma conexão de rede sem ação explícita do usuário, e zero telemetria.

## 4. Regras de segurança

- **Leitura de arquivos externos só em Rust.** Isso vale para SVG, fontes, imagens, PDF e qualquer outro formato. Todo leitor novo ganha um alvo de fuzzing. (ADR-0011)
- **Abrir um arquivo nunca executa código.** Arquivos de projeto não carregam scripts.
- **`unsafe` só com justificativa:** um comentário `// SAFETY:` explicando por que é correto, mais o registro em `docs/unsafe.md`. (ADR-0012)
  - O `Cargo.toml` da raiz nega `unsafe_code` em todo o workspace, e todo crate herda essa regra (`[lints] workspace = true`). Liberar num crate exige a aprovação do dono e o registro em `docs/unsafe.md`.
- **Miri:** quando o primeiro bloco `unsafe` for aprovado, um job de Miri (que roda os testes procurando usos errados de memória) passa a ser obrigatório na compilação automática do GitHub.
- **Criptografia nunca é caseira.** Só bibliotecas consolidadas, aprovadas em ADR.
- **Toda operação vinda de script, plugin ou rede é entrada não confiável** e passa pela validação.
- **Sem acesso a rede em testes,** exceto nos testes de P2P em loopback.

## 5. Dependências

- **Nenhuma dependência nova sem aprovação.** Proponha no plano da tarefa, com nome, licença, manutenção e alternativas.
- **Só licenças compatíveis com GPL-3.0.** A lista abaixo é idêntica à do `deny.toml`; mude as duas juntas.
  - GPL-3.0-or-later (a licença do próprio projeto);
  - MIT e Apache-2.0;
  - BSD-2-Clause e BSD-3-Clause;
  - MPL-2.0, ISC e Zlib;
  - LGPL-2.1-only, LGPL-2.1-or-later, LGPL-3.0-only e LGPL-3.0-or-later;
  - OFL-1.1, **só para fontes**: o `cargo deny` não verifica isso, então quem revisa precisa conferir;
  - Unicode-3.0.
- **Proibido:**
  - dependências com licença comercial paga;
  - codecs com royalties (H.264/H.265/AAC na codificação);
  - bibliotecas Pantone.
- **Após mudar dependências,** `cargo deny check` e `cargo audit` precisam passar.

## 6. Testes obrigatórios

- Toda função nova vem com testes.
- O núcleo também recebe testes de propriedades sempre que fizer sentido.
- **Testes visuais (golden images):** toda mudança que afeta o que aparece na tela ou na exportação precisa de um teste visual em `tests/golden/`. **Nunca atualize uma imagem de referência sem a aprovação explícita do dono.** Mostre a ele a diferença antes.
  - **Registro da aprovação:** todo commit que mexe em qualquer pasta `tests/golden/` leva a linha `Golden-Approved-By: Puã <344064416+crocodiliano@users.noreply.github.com>` na mensagem. Escreva essa linha só depois de o dono ver a diferença e escrever que aprova na conversa.
  - **Checagem:** o gancho `.githooks/commit-msg` e a compilação automática do GitHub recusam o commit sem essa linha. É um registro, não uma prova: quem tem acesso de escrita ao repositório poderia digitá-la.
- **Antes de dizer "pronto",** estes comandos precisam passar:
  - `cargo fmt --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test`

## 7. Fluxo de trabalho

1. **Leia** a especificação da entrega em `docs/fases/` e os ADRs relacionados.
2. **Apresente um plano curto, em português:**
   - o que vai fazer;
   - quais arquivos vai tocar;
   - quais testes vai criar;
   - riscos e dependências novas.
   - **Espere a aprovação.**
3. **Implemente em passos pequenos.** Uma tarefa é uma função, nunca uma aba inteira.
4. **Rode as checagens da seção 6.**
5. **Diga ao dono como testar,** citando os critérios de aceite da entrega.
6. **Faça o commit** depois que o dono aprovar.

**Se algo na especificação estiver ambíguo ou contradizer um ADR, pergunte. Não presuma.**

**Ordem das fases:** o Núcleo só começa quando as entregas 0.4, 0.5 e 0.6 da Fase 0 estiverem concluídas e revisadas pelo agente auditor.

**Antes de fechar uma entrega, do Núcleo em diante:** o dono dispara manualmente no GitHub o workflow de testes de mutação (`mutants.yml`), e os resultados são analisados.

## 8. Convenções

- **Código, comentários, nomes e mensagens de commit:** em inglês.
- **Documentação:**
  - para o usuário: em `docs/` (português e inglês);
  - para o dono: explicações em português.
- **Commits** no formato `area: resumo curto` (ex.: `core: add reorder-node operation`), com:
  - `Signed-off-by:` (DCO);
  - indicação de uso de IA no corpo, nesta forma: `AI-assisted: written with Claude Code (<modelo>); behavior tested by the owner; code reviewed by AI and automated tools.` Nunca escreva "reviewed by the project owner": o dono testa o comportamento, mas não revisa o código.
- **Formatação e lints:** `rustfmt` padrão e Clippy sem avisos.
- **Crates:** prefixo `bivar-`.

## 9. Estrutura planejada do repositório (proposta; revisar com o agente auditor)

```
crates/
  bivar-core/        grafo, nós, operações, desfazer, validação
  bivar-doc/         formato de arquivo SQLite, migrações, recuperação
  bivar-render/      wgpu, compositor, tiles, cache/proxy
  bivar-text/        composição de texto
  bivar-color/       gerenciamento de cor
  bivar-interp/      interpolação (mesclagem, fontes variáveis)
  bivar-ui/          shell da interface, painéis, temas/tokens, acessibilidade
  bivar-io-*/        leitores/escritores de formatos (um crate por família)
  bivar-persona-vetor/ … (uma por persona)
  bivar-script/      motor JS/TS e API pública
  bivar-net/         P2P (depois da Fase 2)
  bivar-cli/         modo sem interface
  bivar-app/         executável
docs/  adr/  fases/  PLANO.md
tests/ golden/  fuzz/
```

## 10. Nunca

- Alterar o documento fora do sistema de operações.
- Adicionar dependência, `unsafe` ou acesso a rede sem aprovação.
- Atualizar imagem de referência sem o dono ver.
- Fazer tarefas grandes de uma vez ou "aproveitar para refatorar" sem pedir.
- Implementar recursos fora da entrega atual.

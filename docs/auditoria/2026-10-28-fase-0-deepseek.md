Esta é uma análise crítica da fundação arquitetural do Bivar. A ambição do projeto é alta e louvável, mas a combinação de decisões tomadas revela riscos significativos que precisam ser endereçados antes de uma linha de código do núcleo ser escrita.

🔴 Decisões Erradas ou de Alto Risco
1. O modelo de "Dono + Referência" é uma aposta perigosa (ADR-0003)

Gravidade: Crítica

Análise: A decisão de modelar o compartilhamento de nós como "referências" a um "dono" é emprestada do Figma, mas o Bivar tem uma complexidade maior: um grafo procedural. No Figma, a referência é primariamente para estilos e variáveis (valores), não para a lógica de um nó (comportamento). O ADR não define o que acontece quando um nó referenciado possui upstream (uma cadeia de nós que o alimenta). Se a origem de um nó referenciado mudar, a referência quebra. O modelo de "dono" cria um acoplamento rígido e uma hierarquia de dependências que pode ser frágil.

Recomendação: Antes de codificar, formalize o conceito de "Contexto de Avaliação". Um nó referenciado deve ser avaliado no contexto de seu dono ou no contexto do objeto que o referencia? A resposta a essa pergunta define toda a semântica do grafo. Considere se um modelo de "componentes" (como no Blender/Geometry Nodes) com encapsulamento e interfaces bem definidas não é mais robusto do que "referências" ad-hoc.

2. A escolha do Vello está minada por um problema conhecido e subestimado (ADR-0018)

Gravidade: Crítica

Análise: O ADR-0018 é honesto ao documentar a limitação de 64 gradient ramps no cache do Vello 0.11. No entanto, a conclusão de que isso é um "pré-requisito" para a Fase 1a é um erro de planejamento. O problema não é um detalhe de performance; é um bloqueador funcional para as features centrais do Bivar (mesclagem, repetidores, 3D→Vetor), que por design gerarão centenas ou milhares de degradês diferentes. A solução (a, b, c ou d) não é trivial e pode envolver um trabalho de engenharia significativo (ex: propor uma mudança no upstream do Vello) ou uma mudança de arquitetura no bivar-render (tiles).

Recomendação: Eleve a solução para o problema de degradês à condição de bloqueio absoluto para iniciar a Fase 1a. Não comece a escrever o núcleo de renderização sem um protótipo funcional que resolva esse problema e atenda aos critérios de aceite (60 Hz obrigatório, 144 Hz desejável) para a cena de 10.000 degradês. O risco de retrabalho é altíssimo.

3. A arquitetura CRDT-first pode levar a um "elefante branco" de complexidade (ADR-0005)

Gravidade: Alta

Análise: Projetar todo o modelo de dados e operações em torno de um CRDT (Loro) desde o dia 1 é uma aposta correta para o futuro, mas impõe um custo de complexidade imediato e pesado. O Loro é excelente, mas é uma biblioteca complexa. Toda a lógica de negócio (validação de grafo, undo/redo, conflitos) precisa ser construída sobre ou em conjunto com o CRDT. O ADR menciona "grafo inválido após junção" (ciclos), mas o Loro já rejeita movimentos cíclicos em sua árvore. Isso indica que o modelo de dados pode estar tentando resolver no nível do CRDT problemas que são melhor tratados na camada de aplicação. A complexidade pode atrasar a entrega de um editor funcional (mesmo que single-player) por meses.

Recomendação: Estabeleça um plano de contingência explícito. Defina que, se o cronograma apertar, o projeto pode lançar uma versão 1.0 single-player (sem colaboração P2P) com uma camada de persistência e undo/redo mais simples (ex: event sourcing), mantendo o modelo de operações (ADR-0002) intacto para uma futura migração para CRDT. Isso permite validar o produto e a UX antes de investir pesadamente na complexidade da colaboração.

⚠️ Problemas Prováveis em 6 a 12 Meses
1. O "estado de conflito" se tornará um "cemitério de funcionalidades" (ADR-0004)

Gravidade: Alta

Análise: A decisão de registrar conflitos e resolvê-los depois é correta para scripts e P2P, mas a UX de resolução é uma caixa de Pandora. Em 6 meses, você terá dezenas de tipos de conflito diferentes (ajuste manual vs. procedural, junção CRDT, referência quebrada, ciclo). Sem uma taxonomia e uma interface unificada para gerenciá-los, o usuário será bombardeado com avisos genéricos de "afeta N objetos" ou "nó em conflito", sem saber o que fazer. A experiência será paralisante.

Recomendação: Antes de implementar a resolução, projete um "Painel de Conflitos" central que agregue todos os conflitos, os categorize por tipo e gravidade, e ofereça ações contextualizadas (não apenas "reaplicar/assar/descartar" genéricos). Trate a UX de conflito como uma funcionalidade de primeira classe, não como um detalhe de implementação.

2. A performance do SQLite com assets grandes se tornará um gargalo (ADR-0010)

Gravidade: Média

Análise: Armazenar imagens e áudios de centenas de MB como BLOBs dentro do SQLite é problemático. A documentação do SQLite e fóruns indicam que BLOBs grandes (acima de ~100KB) degradam a performance de queries e podem inflar o arquivo e os backups. Embora o ADR mencione um "diário de operações", ele não detalha como os assets binários serão versionados ou salvos de forma incremental. O salvamento de um documento com um novo vídeo de 200MB pode se tornar lento e propenso a falhas.

Recomendação: Adote um modelo de armazenamento híbrido desde o início: o SQLite armazena apenas metadados, o grafo de nós e as operações. Os assets binários (imagens, áudio) são armazenados como arquivos separados em uma pasta do projeto, referenciados por seu hash de conteúdo (como já é feito no ADR-0005 para o CRDT). Isso mantém o arquivo do projeto leve, o salvamento atômico rápido, e o versionamento de assets mais simples.

❓ O que Falta Decidir Antes do Núcleo
1. A semântica exata da "Projeção" (ADR-0001)

Gravidade: Crítica

Análise: O ADR-0001 afirma que "camadas e pilha são projeções do grafo". Isso é o coração da UX do Bivar, mas é totalmente abstrato. O que exatamente constitui um "grupo de nós" para um objeto? Como um nó que existe no grafo mas não é representável no painel de camadas é tratado? Como a reordenação na pilha (ex: mover um efeito para cima de outro) se traduz em uma operação no grafo? Sem um algoritmo de projeção bidirecional bem definido, você construirá uma interface que pode entrar em estados inconsistentes com o grafo.

Recomendação: Escreva uma especificação formal (ex: um documento de design ou um conjunto de testes) para a "Projeção". Defina as regras de mapeamento entre o grafo e os painéis, incluindo como lidar com nós "customizados" e como a edição em um painel gera operações válidas no grafo.

2. O modelo de avaliação e dependências do grafo

Gravidade: Alta

Análise: Os ADRs descrevem a estrutura do grafo (nós, conexões), mas não como ele é avaliado. Um nó procedural (ex: "Estrela") depende de seus parâmetros e de suas entradas. Se um nó a montante muda, como a avaliação é propagada? Como o sistema evita reavaliar nós que não mudaram? O ADR-0004 introduz "nós de ajuste", mas não explica como eles interagem com o sistema de avaliação.

Recomendação: Defina um modelo de avaliação pull-based com memoização. Cada nó, ao ser solicitado, avalia suas dependências (se ainda não estiverem em cache) e produz um resultado. Isso simplifica a propagação de mudanças e é um pré-requisito para a performance. O "nó de ajuste" (ADR-0004) pode ser implementado como um nó que "envolve" outro, modificando seu resultado.

🔄 Contradições entre os ADRs
1. ADR-0002 (Toda mudança é uma operação) vs. ADR-0005 (Modelo compatível com CRDT)

Gravidade: Alta

Análise: Há uma tensão não resolvida entre "toda mudança é uma operação serializável" (ADR-0002) e "o modelo de dados é projetado para junção CRDT" (ADR-0005). As operações do ADR-0002 parecem ser comandos imperativos (ex: "mover nó X para Y"). Um CRDT como o Loro trabalha com atualizações de estado (ex: "o pai do nó X agora é Y"). A camada de tradução entre esses dois mundos é complexa. O undo (ADR-0002) é definido como "a lista de operações do próprio autor", mas em um sistema CRDT, o undo é inerentemente mais complicado e não pode simplesmente "desfazer" uma operação local sem considerar o estado global.

Recomendação: Esclareça o padrão de arquitetura. O Bivar usará um Command Bus (onde operações são validadas e então aplicadas ao CRDT) ou um modelo de Event Sourcing (onde o CRDT é a fonte da verdade e as operações são derivadas dele)? A primeira opção é mais simples de raciocinar, mas pode ter problemas de concorrência. A segunda é mais robusta, mas mais complexa de implementar. Essa decisão impacta diretamente a implementação do undo e da colaboração.

2. ADR-0018 (Vello) vs. ADR-0001 (Grafo como fonte da verdade)

Gravidade: Média

Análise: O ADR-0018 revela que o Vello tem um limite rígido de 64 degradês em cache e pode travar com mais de 16.384. Isso significa que o renderizador impõe restrições ao modelo de dados. O grafo pode gerar um número ilimitado de degradês (via mesclagem, repetidores), mas o renderizador não pode lidar com isso. Essa é uma contradição de design: o "grafo é a fonte da verdade", mas o renderizador dita o que é possível.

Recomendação: A solução para o problema de degradês (mencionada no ADR-0018) precisa ser tratada como uma restrição de design do produto, não apenas um bug de performance. O bivar-render deve ser capaz de gerenciar a complexidade da cena, potencialmente através de level-of-detail (LOD) ou agrupamento de degradês similares, antes de enviar ao Vello. A arquitetura do renderizador precisa ser pensada para proteger o Vello de cenas excessivamente complexas.

💎 Resumo Executivo
O Bivar está construindo uma catedral sobre fundações que ainda não foram totalmente inspecionadas. As decisões são ambiciosas e tecnicamente sofisticadas, mas os riscos de complexidade e acoplamento são subestimados. O caminho mais seguro é resolver os bloqueadores de renderização (degradês) e de projeção (grafo ↔ camadas) antes de qualquer outra coisa, e considerar seriamente um modo "single-player first" para validar a UX e o núcleo do produto antes de abraçar totalmente a complexidade do CRDT. A honestidade dos ADRs é um excelente sinal; agora, a disciplina de sequenciamento será o fator decisivo entre um produto inovador e um projeto que colapsa sob o próprio peso.

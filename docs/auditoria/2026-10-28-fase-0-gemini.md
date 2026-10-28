# Relatório de Crítica Arquitetural — Projeto Bivar

> **Data da Revisão:** Outubro de 2026  
> **Escopo:** ADR-0001, ADR-0002, ADR-0003, ADR-0004, ADR-0005, ADR-0010 e ADR-0018.  
> **Perfil da Revisão:** Crítica dura e focada em engenharia de sistemas gráficos e CRDTs em Rust.

---

## 1. Decisões Erradas ou Arriscadas

### Risco 1: Misturar Grafo Cíclico/DAG de Nós com Árvore CRDT (Loro) sem Regras de Dono
* **Gravidade:** Crítica
* **Por quê:** O ADR-0001 define um grafo hierárquico, o ADR-0003 impõe que todo nó tem um "dono único" mas permite referências cruzadas, e o ADR-0005 adota o Loro (CRDT). Gerenciar referências e topologias de grafos dinâmicos em um CRDT descentralizado sem uma especificação estrita de ciclo de vida é um convite à corrupção de dados. Se o Usuário A deleta o objeto "dono" de um nó enquanto o Usuário B o referencia remotamente via CRDT, a mesclagem (merge) gerará ponteiros órfãos ou inconsistências topológicas silenciosas.
* **Recomendação Concreta:** Defina formalmente a semântica de propriedade (*ownership*) e de GC (*Garbage Collection*) de referências antes de escrever qualquer código. Estabeleça por contrato que, ao deletar um dono, suas referências externas ou viram cópias locais autônomas instantaneamente ou são convertidas em nós fantasmas inativos.

### Risco 2: Redundância de Camadas de Histórico (SQLite vs. CRDT/Loro)
* **Gravidade:** Alta
* **Por quê:** O ADR-0010 propõe um arquivo de projeto em SQLite contendo um "diário de operações" próprio para recuperação de quedas, enquanto o ADR-0005 adota o Loro desde o primeiro dia. O Loro já é, por definição, um banco de dados de histórico imutável baseado em operações e CRDTs. Manter dois sistemas paralelos de diário de operações (um no SQLite e outro no Loro) é duplicação de engenharia e cria uma brecha gigantesca para dessincronização de estado após um crash.
* **Recomendação Concreta:** Elimine o diário de operações customizado no SQLite. Utilize o formato nativo de exportação/snapshot do Loro armazenado dentro de um BLOB no SQLite (ou use o SQLite apenas como container de ativos pesados), fazendo com que o único log de operações oficial do sistema seja o do próprio CRDT.

### Risco 3: Engessar o Produto por Limitações Técnicas de Terceiros (Vello)
* **Gravidade:** Alta
* **Por quê:** O ADR-0018 estabelece que o produto deve impedir arquivos com mais de 16.384 degradês porque o Vello 0.11 estoura texturas ou degrada severamente a performance (chegando a 156ms por quadro). Restringir as capacidades criativas do software e o modelo de dados de arquivos externos por conta de um gargalo atual de uma biblioteca de renderização em desenvolvimento é um erro grave de arquitetura de produto.
* **Recomendação Concreta:** Trate o limite de degradês estritamente como um problema de otimização interna do `bivar-render` (implementando paginação, *batching* dinâmico ou rasterização de cache em tiles), e **nunca** rejeite arquivos ou bloqueie o modelo de dados por limitações do backend gráfico atual.

---

## 2. Problemas Previstos em 6 a 12 Meses de Desenvolvimento

* **Conflitos Fantasmas e Divergência de Estado no P2P:** Daqui a 8 meses, quando tentarem ativar a colaboração em rede, os testes de concorrência com o Loro vão falhar frequentemente. O grafo hierárquico cruzado com nós de ajuste procedurais (ADR-0004) vai gerar colisões lógicas que o CRDT genérico não sabe resolver sozinho, travando a interface ou gerando estados irrecuperáveis.
* **Testes de Regressão Visual Inviáveis em CI/CD:** Como o Vello na placa de vídeo não é bit a bit determinístico (conforme o próprio ADR-0018 aponta variações de 1 a 3 pixels no Vulkan/DX12), os testes automatizados de exportação de PNG vão quebrar aleatoriamente nos servidores de CI (GitHub Actions) devido a pequenas variações de driver, fazendo com que a equipe desative os testes visuais por exaustão.
* **Gargalos de I/O no Salvamento Incremental:** O acoplamento entre transações do SQLite, serialização de grafos em Rust e o estado do CRDT vai gerar travamentos perceptíveis na interface (*stutters*) durante o salvamento automático em arquivos de tamanho médio.

---

## 3. O Que Está Faltando Decidir Antes de Escrever o Núcleo

1. **A Regra de Exclusão de Dono (Falta no ADR-0003):** O próprio documento admite que a regra para remoção do dono de um nó referenciado "ainda precisa ser definida". Isso precisa estar resolvido matematicamente antes de desenhar as structs em Rust.
2. **Fronteira de Dados Binários:** O ADR-0005 estipula que imagens e áudios ficam fora do CRDT via hash. Falta decidir: Onde esses blobs binários moram no disco durante a edição local? Como o SQLite compacta ou limpa esses arquivos quando deixam de ser referenciados?
3. **Plataforma de Testes de Renderização:** Como o pipeline de CI validará a integridade visual do `bivar-render` em ambientes sem GPU física dedicada, dado que o `vello_cpu` apresenta desvios métricos em relação à placa de vídeo?

---

## 4. Contradições Entre os ADRs

* **Contradição 1 — Fonte da Verdade do Histórico (ADR-0005 vs. ADR-0010):** O **ADR-0005** determina que o histórico e a colaboração residem no log de operações do CRDT (Loro). Já o **ADR-0010** exige um "diário de operações" próprio dentro do SQLite para recuperação de falhas. Se ocorrer uma queda abrupta, qual dos dois diários é a autoridade suprema para reconstruir o estado? Se houver divergência entre eles, o arquivo estará corrompido.
* **Contradição 2 — Grafo Flexível vs. Propriedade Estrita (ADR-0001 vs. ADR-0003):** O **ADR-0001** define o documento como um grafo hierárquico com referências cruzadas (o que configura um Grafo Acíclico Dirigido — DAG), mas o **ADR-0003** impõe que "todo nó tem um dono estrito". Em arquiteturas de grafos onde um nó pode alimentar múltiplos pais, o conceito de "dono único" colapsa, gerando ambiguidades insolúveis na projeção da árvore de camadas da interface.

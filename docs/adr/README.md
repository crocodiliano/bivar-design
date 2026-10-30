# Registros de decisão de arquitetura (ADRs)

Cada ADR registra uma decisão, o contexto e as alternativas descartadas. Para escrever um novo, copie o [modelo](modelo.md).

## Status

Todo ADR passa por três níveis, nesta ordem:

1. **Proposto:** escrito, ainda sem revisão. É o status inicial de todos.
2. **Revisado por IA:** o agente auditor (`.claude/agents/auditor.md`) revisou, os achados foram tratados e o dono aprovou.
3. **Aceito:** reservado para quando houver revisão humana de alguém experiente em Rust.

Por enquanto o projeto não tem auditor humano. Um ADR "Proposto" pode ser editado diretamente, com a aprovação do dono. Para mudar um ADR "Revisado por IA" ou "Aceito", crie um novo ADR que o substitua. Ele passa pela revisão do agente auditor e pela aprovação do dono, e só vira "Aceito" depois de revisão humana.

## Índice

A prioridade indica o quanto custaria mudar a decisão depois. Os ADRs de prioridade Alta são revisados primeiro.

| ADR | Título | Prioridade | Status |
| --- | --- | --- | --- |
| [0001](0001.md) | Grafo de nós hierárquico como fonte da verdade | Alta | Proposto |
| [0002](0002.md) | Toda mudança é uma operação | Alta | Proposto |
| [0003](0003.md) | Dono + referência para nós compartilhados | Alta | Proposto |
| [0004](0004.md) | Edição manual sobre procedural e estado de conflito | Alta | Proposto |
| [0005](0005.md) | Modelo de dados compatível com CRDT desde o início | Alta | Proposto |
| [0006](0006.md) | Personas como módulos; memória por cache/proxy | Média | Proposto |
| [0007](0007.md) | wgpu + Vello; Skia como plano B isolado | Alta | Proposto |
| [0008](0008.md) | Gerenciamento de cor profissional | Alta | Proposto |
| [0009](0009.md) | Framework de interface decidido por prova de conceito | Alta | Proposto |
| [0010](0010.md) | Arquivo de projeto em SQLite com especificação aberta (o formato por dentro foi reaberto pelo ADR-0019 e é escolhido na entrega 0.6) | Alta | Proposto |
| [0011](0011.md) | Leitura de arquivos externos só em Rust, com fuzzing | Alta | Proposto |
| [0012](0012.md) | Política de dependências e de unsafe | Média | Proposto |
| [0013](0013.md) | Scripts em JS/TS e plugins em WebAssembly, com uma única API | Média | Proposto |
| [0014](0014.md) | Segurança do P2P | Alta | Proposto |
| [0015](0015.md) | Módulo de interpolação compartilhado | Média | Proposto |
| [0016](0016.md) | Licença GPL-3.0, DCO e contribuições com IA | Baixa | Proposto |
| [0017](0017.md) | Interface em egui (resultado da prova de conceito do ADR-0009) | Alta | Proposto |
| [0018](0018.md) | Vello sobre wgpu confirmado, com ressalvas (resultado da prova de conceito do ADR-0007) | Alta | Proposto |
| [0019](0019.md) | Decisões do dono após a auditoria da Fase 0 | Alta | Revisado por IA |
| [0020](0020.md) | Motor de texto Parley sobre o Vello (resultado da prova de conceito da entrega 0.5) | Alta | Revisado por IA |
| [0021](0021.md) | Loro como biblioteca CRDT, e como o grafo vive dentro dela (resultado da entrega 0.6) | Alta | Proposto |
| [0022](0022.md) | "A última escrita" é decidida por um relógio lógico, não necessariamente o do CRDT (substitui um trecho do ADR-0019) | Alta | Proposto |
| [0023](0023.md) | Arquivo `.bivar` em formato próprio, só em Rust, que só cresce no fim (resultado da entrega 0.6) | Alta | Proposto |
| [0024](0024.md) | Toda entrada externa: limites, verificação isolada e fuzzing (resultado da entrega 0.6) | Alta | Proposto |

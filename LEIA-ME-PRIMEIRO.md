# Como começar com estes arquivos

Este pacote foi o ponto de partida do repositório do Bivar, com as regras e o plano que o Claude Code segue. Hoje o repositório já tem o workspace Cargo (com os crates ainda vazios) e as provas de conceito das entregas 0.2 e 0.3, em `poc/`.

## Conteúdo

| Arquivo | Para quê |
| --- | --- |
| `CLAUDE.md` | Regras permanentes que o Claude Code lê em toda sessão |
| `docs/PLANO.md` | O plano completo (versão oficial) |
| `docs/adr/` | Registros de decisão, revisados pelo agente auditor (IA) |
| `docs/auditoria/` | Relatórios de revisão por IA e da comunidade |
| `.claude/agents/auditor.md` | O agente auditor, que revisa decisões e código |
| `docs/fases/` | Roadmap e especificações da Fase 0, do Núcleo e da Fase 1a, com critérios de aceite |

## Primeiros passos

> **Instalação completa (VS Code, Git, Rust e Claude Code), passo a passo:** veja o `PASSO-A-PASSO.md`.

1. Crie uma pasta `bivar` no seu PC e copie este conteúdo para dentro dela.
2. Peça ao Claude Code uma revisão de `docs/adr/` e `docs/PLANO.md` com o agente auditor (entrega 0.4 da Fase 0).
3. Faça a busca do nome no INPI (instruções em `docs/PLANO.md`, seção "Verificação do nome").
4. Abra o Claude Code nessa pasta e comece pela Fase 0 com um pedido como este:

   > Leia o CLAUDE.md e docs/fases/fase-0.md. Quero começar pela entrega 0.1 (repositório e regras). Antes de qualquer coisa, me apresente o plano em português e espere minha aprovação.

5. Siga o ciclo do CLAUDE.md: plano → aprovação → implementação com testes → seu teste → commit.

**Dica:** faça uma entrega por vez. Só passe para a próxima quando todos os critérios de aceite da atual estiverem marcados.

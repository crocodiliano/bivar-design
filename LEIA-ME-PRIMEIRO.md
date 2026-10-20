# Como começar com estes arquivos

Este pacote é o ponto de partida do repositório do Bivar. Ainda não há código nele: só as regras e o plano que o Claude Code vai seguir.

## Conteúdo

| Arquivo | Para quê |
| --- | --- |
| `CLAUDE.md` | Regras permanentes que o Claude Code lê em toda sessão |
| `docs/PLANO.md` | O plano completo (cópia do documento online) |
| `docs/adr/` | 16 registros de decisão para o auditor validar |
| `docs/fases/` | Roadmap e especificações da Fase 0, do Núcleo e da Fase 1a, com critérios de aceite |

## Primeiros passos

> **Instalação completa (VS Code, Git, Rust e Claude Code), passo a passo:** veja o `PASSO-A-PASSO.md`.

1. Crie uma pasta `bivar` no seu PC e copie este conteúdo para dentro dela.
2. Mande `docs/adr/` e `docs/PLANO.md` para o auditor revisar.
3. Faça a busca do nome no INPI (instruções em `docs/PLANO.md`, seção "Verificação do nome").
4. Abra o Claude Code nessa pasta e comece pela Fase 0 com um pedido como este:

   > Leia o CLAUDE.md e docs/fases/fase-0.md. Quero começar pela entrega 0.1 (repositório e regras). Antes de qualquer coisa, me apresente o plano em português e espere minha aprovação.

5. Siga o ciclo do CLAUDE.md: plano → aprovação → implementação com testes → seu teste → commit.

**Dica:** faça uma entrega por vez. Só passe para a próxima quando todos os critérios de aceite da atual estiverem marcados.

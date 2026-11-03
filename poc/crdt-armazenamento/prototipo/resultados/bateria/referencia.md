# Referência da bateria de medidas da 0.6

A bateria (`poc/crdt-armazenamento/medir-tudo.ps1`) compara cada rodada com a referência guardada em `ultima.json`. Uma medida "PIOROU" quando passa mais de 25% da referência e mais que a folga dela. Só uma rodada sem medidas que falharam vira a referência seguinte.

## Medidas que pioraram e foram aceitas pelo dono

A rodada de 2026-10-31 06:05 (código `4d9b8ba`) terminou com três "PIOROU" (saída 2). Nenhuma passou de um limite aprovado. As três foram aceitas pelo dono, e por isso esses valores, ou os de uma rodada mais nova, são a referência. A bateria volta a terminar limpa (saída 0) quando nada piora em relação a eles.

| Medida | Antes | Aceito | Por que piorou | Decisão |
|---|---|---|---|---|
| Abrir arquivo de fora, com verificação (1 milhão de operações) | 2,77 s | 14,37 s | Desde o B1, o verificador lê o documento inteiro (nós removidos, todo o histórico) e roda todas as passagens; antes lia só a parte visível. Depois disso, a leitura única por nó (commit `caacb9c`) trouxe a medida para cerca de 10 s | Opções 1 + 2 do dono: acelerar sem mudar regra; o limite de 60 s não muda |
| Tempo de compactar 200 mil operações (SSD C: e HD A:) | 1,0 s | 1,9 s | O tempo a mais é a conferência do resultado da compactação | Aceito pelo dono (auditoria 0.6) |
| Mapa de pedaços: bytes por pedaço | — (medida nova) | 124 bytes | Medida nova do I9; o protótipo guarda o mapa de forma simples | Aceito no protótipo; meta do Núcleo: no máximo 50 bytes por pedaço |

O registro completo das decisões está em `docs/auditoria/2026-10-31-fase-0.6-tratamento.md`.

## Como mudar a referência

- **O script grava toda rodada sem falhas como a nova referência,** mesmo quando algo piorou (saída 2). Por isso uma rodada com "PIOROU" só entra no commit depois que o dono aceita a piora e ela é registrada nesta tabela. Sem a aceitação, a piora é tratada como defeito, e o `ultima.json` volta ao do último commit.
- **Uma medida que melhora** vira referência na própria rodada.

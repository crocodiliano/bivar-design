# Referência da bateria de medidas da 0.6

A bateria (`poc/crdt-armazenamento/medir-tudo.ps1`) compara cada rodada com a referência guardada em `ultima.json`. Uma medida "PIOROU" quando passa mais de 25% da referência e mais que a folga dela. Só uma rodada sem medidas que falharam vira a referência seguinte.

## Medidas que pioraram e foram aceitas pelo dono

A rodada de 2026-10-31 06:05 (código `4d9b8ba`) terminou com três "PIOROU" (saída 2). Nenhuma passou de um limite aprovado. As três foram aceitas pelo dono, e por isso esses valores, ou os de uma rodada mais nova, são a referência. A bateria volta a terminar limpa (saída 0) quando nada piora em relação a eles.

| Medida | Antes | Aceito | Por que piorou | Decisão |
|---|---|---|---|---|
| Abrir arquivo de fora, com verificação (1 milhão de operações) | 2,77 s | 14,37 s | Desde o B1, o verificador lê o documento inteiro (nós removidos, todo o histórico) e roda todas as passagens; antes lia só a parte visível. Depois disso, a leitura única por nó (commit `caacb9c`) trouxe a medida para cerca de 10 s | Opções 1 + 2 do dono: acelerar sem mudar regra; o limite de 60 s não muda |
| Tempo de compactar 200 mil operações (SSD C: e HD A:) | 1,0 s | 1,9 s | O tempo a mais é a conferência do resultado da compactação | Aceito pelo dono (auditoria 0.6) |
| Mapa de pedaços: bytes por pedaço | — (medida nova) | 124 bytes | Medida nova do I9; o protótipo guarda o mapa de forma simples | "O dono aceitou em 2026-11-03 os 124 bytes por pedaço só no protótipo; a meta do Núcleo continua em no máximo 50 bytes." |

## Limites que a bateria confere (pedido do dono, 2026-11-03)

- **O verificador:** 60 s e 4 GiB, nunca mudados. Valem para:
  - a abertura de fora de 1 milhão de operações;
  - o verificador sozinho e a memória máxima dele, com 1 e 2 milhões de operações;
  - a verificação do arquivo com 2 GB de arquivos grandes.

  Acima de qualquer um, a bateria falha (saída 1). Um documento recusado também faz a medida falhar.
- **A memória máxima** é a contagem do próprio limite de memória do verificador, lida a cada 2 ms. Um pico mais curto que isso pode escapar da medida, mas não do limite, que recusa o arquivo.
- **Uma medida pulada** (por exemplo, o arquivo de 2 GB num disco com menos de 6 GB livres) aparece como PULADA, e a rodada termina com a saída 3, nunca limpa. A referência guarda o valor anterior dessa medida.

O registro completo das decisões está em `docs/auditoria/2026-10-31-fase-0.6-tratamento.md`.

## Referência atual: rodada de 2026-11-03 15:00 (código `7dc7c70`)

A bateria rodou 3 vezes seguidas, às 14:49, 15:00 e 15:12, com a regra fixada antes de medir (pedido do dono, 2026-11-03).

- **Para cada medida com limite, vale a mediana das 3 rodadas.** Nenhum limite aprovado muda.
- **Se todas as medianas ficam dentro e não há PULADA nem FALHOU,** a rodada do meio em "abrir e ler 1 milhão" vira a referência. Se não, a referência não muda.
- **O `ultima.json` não mudou entre as rodadas.** As 3 compararam com a referência de 13:13.
- **Todas as 3 foram guardadas** (registros e `historico.csv`).

O resultado: todas as medianas ficaram dentro dos limites, sem PULADA nem FALHOU. "Abrir e ler 1 milhão" deu 2,55, 2,54 e 2,50 s (mediana de 2,54 s, limite de 3 s). Por isso, a rodada de 15:00 é a referência. A tabela completa está no registro de tratamento.

## Como mudar a referência

- **O script grava toda rodada sem falhas como a nova referência,** mesmo quando algo piorou (saída 2). Por isso uma rodada com "PIOROU" só entra no commit depois que o dono aceita a piora e ela é registrada nesta tabela. Sem a aceitação, a piora é tratada como defeito, e o `ultima.json` volta ao do último commit.
- **Uma medida que melhora** vira referência na própria rodada.
- **Quando o valor varia muito entre rodadas** (como "abrir e ler 1 milhão" perto dos 3 s), o dono pode pedir 3 rodadas com a regra da mediana, como na referência atual.

# Parâmetros: "um registro" × "uma entrada por escrita"

Pedido do dono em 2026-10-29, no lugar da proposta não enviada ao projeto Loro. 

> **Decisão do dono (2026-10-29):** entradas para os parâmetros, e também para a marca de removido. Os 2.000 + 2.000 cenários foram rodados de novo depois dessa mudança, sem falhas. A regra do relógio lógico foi ajustada no [ADR-0022](../../docs/adr/0022.md). Registro no [ADR-0021](../../docs/adr/0021.md), seção 5.

## As duas formas

- **Registro (como está no ADR-0021):**
  - cada parâmetro é um valor só, `p:<nome>`;
  - quando duas cópias escrevem ao mesmo tempo, **a Loro escolhe** quem vence;
  - o Bivar descobre que houve disputa comparando o histórico das duas cópias e reproduzindo a regra de desempate da Loro, travada por um teste.
- **Entradas (a alternativa):**
  - cada escrita vira uma entrada própria, `p:<nome>#<contador>.<cópia>`;
  - quem escreve apaga **só as entradas que está vendo** e cria a sua, com o contador igual ao maior visto mais um;
  - escritas simultâneas deixam **as duas entradas no documento**, à vista de todas as cópias;
  - **o Bivar escolhe** quem vence: o maior contador e, empatando, a maior cópia;
  - limpar um parâmetro também é uma entrada, sem valor.

O código está em [`prototipo/loja-loro/src/lib.rs`](prototipo/loja-loro/src/lib.rs) (`Register` e `Entries`). As duas formas passam pelos mesmos testes.

## Resultado

| Critério pedido | Registro | Entradas |
| --- | --- | --- |
| **Depende da regra interna da Loro para os parâmetros** | **sim** (travada por teste) | **não** |
| Linhas de código próprias dos parâmetros | ~79, mais ~106 linhas do teste que trava a regra | ~72, mais ~92 linhas de testes próprios |
| Erros encontrados ao construir | 3 na detecção dos parâmetros (Partes 2 e 3) | **0** (os 7 testes e os 2.000 + 2.000 cenários passaram de primeira) |
| Nada se perde, 2.000 cenários com duas cópias | 0 falhas | 0 falhas |
| Nada se perde, 2.000 com três cópias (na cópia que juntou) | 0 falhas | 0 falhas |
| **Todas as três cópias avisam** das escritas simultâneas nos parâmetros (2.000 cenários) | **1.986 falhas** | **0 falhas** |
| Arquivo depois da junção de 10.000 + 10.000 | 625 KB | 719 KB (**+15%**) |
| Arquivo do documento de 1 milhão de operações | 31,6 MB | 35,6 MB (**+13%**) |
| Junção de 10.000 + 10.000, com o relatório (limite 1 s) | 0,57 s | 0,69 s |
| Abrir e ler o documento de 1 milhão (limite 3 s) | 2,41 s | 2,58 s |
| Memória para isso (limite 1 GB) | 221 MB | 229 MB |

Os tempos são a mediana de 3 execuções na máquina de referência. Registros completos:

- [`resultados/comparacao-parametros-medidas.txt`](prototipo/resultados/comparacao-parametros-medidas.txt)
- [`resultados/comparacao-parametros-cenarios.txt`](prototipo/resultados/comparacao-parametros-cenarios.txt)

## O que os números querem dizer

- **Entradas tira a dependência que motivou o pedido.** Para os parâmetros, nenhuma regra interna da Loro é reproduzida, e o teste que trava a regra deixa de proteger os parâmetros.
- **Entradas resolve, para os parâmetros, a lacuna das três cópias.** A disputa fica gravada no próprio documento, e toda cópia a vê (1.986 falhas contra 0). Isso adianta parte do item da 0.7 ("avisos gravados no documento"), só para os parâmetros.
- **O custo é pequeno e fica dentro de todos os limites:** cerca de 13 a 15% a mais de arquivo e cerca de 0,1 s a mais na junção e na abertura.
- **A quantidade de código é parecida.** O que muda é o risco: a forma por entradas não precisou de nenhuma correção.

## O que continua dependendo da Loro, nas duas formas

- **Os movimentos:** quem vence entre dois movimentos simultâneos continua sendo decidido pela árvore da Loro. O Bivar só confere depois se o movimento de cada cópia valeu, e isso não exige reproduzir a regra. Mas, com três cópias, a escolha do "último movimento de cada lado" usa a mesma ordem da Loro.
- **A marca de "removido":** continua um registro único. Ela poderia passar para entradas do mesmo jeito, com pouco código.
- **Por isso, o teste que trava a regra da Loro continua necessário** nas duas formas, mesmo com entradas: ele protegeria os movimentos e a marca de "removido".

## Problema encontrado durante as medidas (vale para as duas formas)

**Ler o documento inteiro depois de abrir levava cerca de 14 s nos dois modos**, contra 2,09 s da Loro pura na Parte 2.

- **A causa:** a loja do protótipo, a cada nó lido, subia a árvore até a raiz para conferir se algum ancestral estava marcado como removido. É o custo da "remoção por marcação" feita de forma ingênua.
- **A correção:** a leitura completa passou a percorrer a árvore uma vez só, de cima para baixo. O tempo voltou para cerca de 2,4 s, e os 2.000 + 2.000 cenários continuam passando.
- **O risco que fica:** as leituras de **um** nó (por exemplo, na edição normal) ainda sobem até a raiz. Num documento muito fundo isso pode pesar. Isso entra no custo da remoção por marcação, que a Parte 4 vai medir.

## Um ponto de regra para o dono

Na forma por entradas, "a última escrita" passa a ser decidida pelo **relógio lógico do Bivar** (o contador de cada parâmetro), e não pelo relógio lógico da Loro. Os dois são relógios lógicos, e nenhum usa a hora do computador. Mas o `CLAUDE.md` (princípio 5) e o ADR-0019 dizem "**pelo relógio lógico do CRDT**". Se a forma por entradas for escolhida, essa frase precisa virar "**por um relógio lógico, nunca pela hora do computador**". O ADR-0019 está "Revisado por IA", então isso exige um ADR novo.

## Recomendação

**Entradas**, pelos três motivos acima:

1. tira a dependência da regra interna da Loro nos parâmetros;
2. faz todas as cópias verem a disputa;
3. não precisou de nenhuma correção.

O custo (de 13 a 15% de arquivo) cabe nos limites.

Se o dono escolher entradas, sugiro também passar a marca de "removido" para entradas, para que só os movimentos fiquem dependendo da Loro.

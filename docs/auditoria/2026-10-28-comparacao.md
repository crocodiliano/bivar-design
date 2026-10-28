# Comparação das três revisões da Fase 0 (2026-10-28)

Esta página compara as três revisões por IA da Fase 0:

- **Agente auditor:** [`2026-10-28-fase-0.md`](2026-10-28-fase-0.md). Os códigos C1, A1, M1 etc. são dele.
- **Gemini:** [`2026-10-28-fase-0-gemini.md`](2026-10-28-fase-0-gemini.md).
- **DeepSeek:** [`2026-10-28-fase-0-deepseek.md`](2026-10-28-fase-0-deepseek.md).

Os três relatórios ficam como foram escritos. As decisões do dono sobre estes pontos estão no [ADR-0019](../adr/0019.md).

**Escopo diferente.** O agente auditor leu os 18 ADRs, o CLAUDE.md, o PLANO, as fases e o código das provas, e rodou os comandos. O Gemini leu só os ADRs 0001 a 0005, 0010 e 0018. O DeepSeek não diz o escopo, mas cita só ADRs. Por isso muitos achados aparecem só no relatório do agente auditor.

## O que os três apontaram

| Assunto | Agente auditor | Gemini | DeepSeek |
| --- | --- | --- | --- |
| **Regras do dono de um nó compartilhado** (ADR-0003): o que acontece quando o dono é removido enquanto outro nó o referencia | A4 (Alto) | Risco 1 (Crítico) e "falta decidir" 1 | Risco 1 (Crítico) |
| **Conflitos na colaboração:** a junção pode criar um estado inválido ou apagar uma edição sem aviso | C2 (Crítico) e A2 (Alto) | "Conflitos fantasmas" (6 a 12 meses) | "Cemitério de funcionalidades" (Alto) e risco 3 |
| **Onde mora a verdade do documento e o histórico duplicado** | A1 (Alto); A6 cita a especificação dependente do formato da Loro | Risco 2 (Alto) e contradição 1: dois diários, o do SQLite e o da Loro | Contradição 1 (Alta): operações do ADR-0002 × atualizações do CRDT |
| **Limite de degradês do Vello** (ADR-0018) | M6 (Médio); A9 (Alto), buffers fixos | Risco 3 (Alto) | Risco 2 (Crítico) e contradição 2 |
| **Arquivos grandes dentro do arquivo de projeto** | A6 (Alto): limite de 1 GB por valor no SQLite | "Falta decidir" 2: onde moram e como são limpos | Médio: lentidão com valores grandes no SQLite |

Os três chegam ao terceiro assunto por caminhos diferentes. O agente auditor pergunta quem guarda o documento. O Gemini aponta dois históricos paralelos. O DeepSeek aponta a tradução entre operações e CRDT. É o mesmo problema visto de três lados.

## O que dois apontaram

| Assunto | Quem | Detalhe |
| --- | --- | --- |
| **Testes visuais e placa de vídeo na compilação automática** | Agente auditor (M1, M2, M5, B11) e Gemini | O agente auditor trata das folgas e de como a aprovação é registrada. O Gemini prevê testes instáveis no GitHub. |
| **Salvamento e queda do programa** | Agente auditor (A6) e Gemini | O agente auditor lista queda de energia, arquivo cortado e OneDrive. O Gemini prevê travadas no salvamento automático. |
| **Plano de contingência para o CRDT** | DeepSeek (risco 3) e, em parte, o Gemini | O DeepSeek propõe lançar primeiro para uma pessoa só. O Gemini só alerta para a complexidade. |

## O que só um apontou

**Só o agente auditor:**

- C1: o SQLite é escrito em C e pode trazer código SQL guardado no arquivo (gatilhos e visões);
- A3: um ajuste invalidado pode ser reaplicado no vértice errado sem aviso;
- A5: o histórico guardado no arquivo deixa recuperar o que foi apagado;
- A7: entradas externas além de arquivos, e "bombas" de memória e tempo;
- A8: o Vello trabalha só em 8 bits sRGB, sem CMYK;
- A10: tipos de nó desconhecidos e personas "dormindo";
- A11: identidade e autoria nas operações;
- A12: os testes de CRDT previstos não protegem contra perda de edições;
- os achados médios e baixos (M1 a M17, B1 a B11).

**Só o DeepSeek:**

- a tradução entre o painel de camadas e o grafo, nos dois sentidos (Crítico);
- como o grafo é recalculado quando algo muda, com cache (Alto);
- um painel central de conflitos, com tipos e ações próprias (Alto);
- o desfazer em colaboração, que não é só "desfazer a minha última operação";
- o "contexto de avaliação" de um nó referenciado: no dono ou em quem referencia.

**Só o Gemini:**

- a contradição entre "um nó pode alimentar vários outros" (ADR-0001) e "todo nó tem um único dono" (ADR-0003).

## Onde discordam

1. **Arquivos grandes.** Cada um propõe um caminho:
   - o agente auditor: dividir em pedaços dentro do arquivo;
   - o Gemini: o histórico da Loro num bloco do SQLite, ou o SQLite só como recipiente dos arquivos grandes;
   - o DeepSeek: uma pasta separada ao lado do projeto.

   **Decisão do dono (ADR-0019):** arquivo único, com os arquivos grandes dentro dele, em pedaços.
2. **Gravidade dos degradês.**
   - DeepSeek: Crítico, um bloqueio que precisa de protótipo antes de começar;
   - Gemini: Alto, mas um problema de otimização interna, que nunca pode limitar o que o arquivo contém;
   - agente auditor: Médio (M6), mais a suspeita de imagem incompleta (A9).

   **Decisão do dono:** prova na entrega 0.9, antes do Núcleo, e nenhum arquivo é recusado por limite do renderizador.
3. **"O ADR-0018 manda recusar arquivos."** O Gemini lê assim. O texto diz "o produto precisa impedir que uma cena passe desse limite", que é ambíguo, mas o critério de aceite logo abaixo já pede "desenha corretamente, ou mostra um aviso visível". O ADR-0019 tira a ambiguidade.
4. **Testes da placa de vídeo no GitHub.** O Gemini prevê que eles vão falhar ao acaso por diferenças de driver. Duas ressalvas:
   - a compilação automática do GitHub roda em Linux sem placa de vídeo, então esses testes nem rodam lá;
   - a variação de 1 a 3 pixels foi medida repetindo a cena na mesma máquina, não entre drivers.

   O risco real está em outro lugar: a comparação exata do modo sem placa entre o PC do dono (Windows) e o GitHub (Linux). A Fase 0 reorganizada pede essa verificação na 0.5.
5. **"Um nó com vários usuários contradiz o dono único."** O Gemini vê uma contradição. O agente auditor não: o ADR-0003 é justamente a regra para um nó usado em vários lugares (um dono, várias referências). O que falta são as regras em aberto (A4), não uma escolha entre os dois modelos.
6. **Operações × CRDT.** O DeepSeek chama de contradição. O agente auditor trata como decisão que falta (A1). O ADR-0019 decide que haverá um único histórico, o do CRDT, e que o sistema de operações continua independente da biblioteca.
7. **Ciclos e a Loro.** O DeepSeek diz que a Loro "já rejeita movimentos cíclicos em sua árvore", o que sugere que o problema dos ciclos está resolvido. **Não está.** A Loro impede ciclos **só na árvore** (a hierarquia de grupos). As **conexões entre nós** (a saída de um nó ligada à entrada de outro) não formam árvore, e a Loro não as verifica. Uma pessoa liga A em B, outra liga B em A, e as duas ligações, válidas sozinhas, formam um ciclo depois da junção. O achado A2 continua de pé, e a entrega 0.7 trata dele.

## Diferenças de gravidade

| Assunto | Agente auditor | Gemini | DeepSeek |
| --- | --- | --- | --- |
| Dono de nó compartilhado | Alto | Crítico | Crítico |
| Degradês | Médio (M6) | Alto | Crítico |
| Arquivos grandes | Alto (dentro de A6) | sem gravidade | Médio |
| Tradução camadas ↔ grafo | não apontou | não apontou | Crítico |

**Esta comparação foi feita por IA e não substitui uma revisão humana.**

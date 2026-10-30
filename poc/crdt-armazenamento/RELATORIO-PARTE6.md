# Entrega 0.6, Parte 6: "Salvar cópia para enviar" completo, e a bateria de medidas

Data: 2026-10-30. Decisões no [ADR-0023, seção 7](../../docs/adr/0023.md); formato em [`docs/formato/`](../../docs/formato/README.md).

> **Atualizado no fechamento:** o dono pediu cinco ajustes depois desta parte (camadas ocultas, privacidade, busca nos dados compactados, `.parcial` nunca apagado sem perguntar, bateria no repositório). Eles estão no [relatório do fechamento](RELATORIO-FECHAMENTO.md); onde este relatório diz que o `.parcial` "é substituído", vale a regra nova.

## O que foi feito

### 1. "Salvar cópia para enviar"

- **Monta um arquivo novo só com o que está visível agora.** Ficam de fora:
  - o histórico;
  - os nós removidos;
  - os arquivos grandes que só eles usavam.
- **A cópia ganha uma identidade nova** e nunca se junta com o original.
- **Com avisos ainda sem resolver,** nada é gravado até o dono confirmar. O programa mostra a lista antes.

### 2. "Salvar como"

- Grava o arquivo inteiro, com o histórico, e continua sendo o mesmo documento.
- Pede ao programa o aviso discreto sugerindo "Salvar cópia para enviar".

### 3. As duas gravações só dão o nome final a um arquivo completo

- Elas escrevem primeiro em `<nome>.parcial` e só trocam o nome no fim.
- Uma queda no meio nunca deixa um arquivo pela metade que pareça pronto.
- Nenhuma das duas sobrescreve um arquivo que já existe.

### 4. Bateria de medidas (pedido do dono)

É um comando único que roda de novo todas as medições da 0.6, compara com os limites aprovados e com a rodada anterior, e grava o resultado em `prototipo/resultados/bateria/`. A partir da pasta do projeto, no PowerShell:

```
powershell -ExecutionPolicy Bypass -File poc/crdt-armazenamento/medir-tudo.ps1
```

- **Duração:** cerca de 4 minutos.
- **Situação de cada medida:**
  - **ok;**
  - **ACIMA DO LIMITE:** passou de um limite aprovado;
  - **PIOROU:** ficou mais de 25% pior que a rodada anterior, e mais que a variação normal daquela medida.
- **Resultado do comando:**
  - 0 = tudo bem;
  - 1 = limite passado ou medida falhou;
  - 2 = algo piorou.
- **Aviso de piora testado:** com a rodada anterior alterada de propósito para "100 mil operações em 0,5 s", a bateria marcou PIOROU e terminou com 2. As medidas que variaram pouco continuaram "ok".
- **A única medida nova é a de 100 mil operações no protótipo.** A da Parte 2 era do pacote de comparação.
- **O teste do arquivo de 2 GB é pulado** num disco com menos de 6 GB livres, com aviso. Hoje o C: tem 3,5 GB livres, então esse teste rodou só no A:.

## Resultado da bateria no fim da Parte 6

Todas as medidas ficaram dentro dos limites e nenhuma piorou.

| Medida | Valor | Limite aprovado |
|---|---|---|
| 100 mil operações | 1,5 s | 5 s |
| Junção de 10 mil + 10 mil operações | 0,33 s | 1 s |
| Abrir e ler 1 milhão de operações | 2,5 s | 3 s |
| Memória para isso | 229 MB | 1 GB |
| Pausa do salvamento automático, pior caso (SSD / HD) | 1,2 / 1,3 ms | 16 ms |
| Abrir arquivo deste computador | 0,11 s | - |
| Abrir arquivo de fora, com verificação (só abrir) | 2,9 s | aceito pelo dono |
| Ler um nó a 10 mil níveis, com o índice | 1,4 µs | - |
| Forçar gravação no disco, mediana (SSD / HD) | 0,35 / 22 ms | - |
| Compactar 200 mil operações (SSD / HD) | 1,0 s, arquivo de 6,0 para 3,1 MB | - |
| Arquivo de 2 GB no HD: guardar / abrir / ler e conferir | 12,4 / 0,01 / 4,4 s | - |

## Testes (68 no protótipo, todos passam)

Quatro testes são novos (`arquivo/tests/copia.rs`):

1. **A cópia não leva nada apagado nem histórico.** Um texto com um marcador único e uma imagem de 9 MB são apagados antes da cópia. O teste procura o marcador e a imagem de três jeitos:
   - nos bytes do arquivo;
   - no arquivo lido como o programa lê;
   - no histórico que a própria Loro mostra.

   Não acha nada: nem o marcador, nem o hash da imagem, nem a identidade de um pedaço dela, nem os dados dela. A Loro confirma que o histórico da cópia tem um autor só, o da própria cópia. A junção com o original é recusada. Antes de olhar a cópia, o teste confere que o arquivo de trabalho guarda os dois (controle).
2. **Pendências avisadas antes de gravar.** Com um parâmetro editado em duas cópias, nada é gravado sem a confirmação. Depois dela, a cópia abre limpa.
3. **"Salvar como" guarda o histórico** e pede o aviso.
4. **Uma cópia só aparece completa.** Um `.parcial` que sobrou é substituído, e nenhuma das duas sobrescreve um arquivo existente.

**Os 2.000 + 2.000 cenários continuam passando.**

**Checagens:** formatação, Clippy sem avisos, isolamento da Loro, licenças (`cargo deny`) no protótipo, na comparação, na raiz e em `fuzz/`. O `cargo audit` da raiz passa. No protótipo, ele mostra só os 6 alertas já registrados na ficha (`im` e `sized-chunks`, dentro da Loro 1.17.1).

## Problemas encontrados e corrigidos nesta parte

1. **A primeira versão das duas gravações escrevia direto no nome final.** Uma queda no meio deixaria um arquivo pela metade com cara de cópia pronta. Corrigido com o `.parcial`.
2. **Os testes deixavam arquivos acumulando na pasta temporária do C:.** Cada rodada criava nomes novos (cerca de 460 MB já acumulados). Agora cada teste reusa o próprio nome e apaga as sobras da rodada anterior.
3. **A primeira rodada da bateria não leu 18 das 20 medidas,** porque o Rust às vezes escreve o resultado na mesma linha do nome do teste. Corrigido; a rodada seguinte leu todas.
4. **Dois testes novos usavam o mesmo nome de arquivo** e se atrapalhavam quando rodavam juntos. Cada um ganhou um nome próprio.

## O que fica pendente

- **A lista de pendências antes da cópia** cobre o mesmo parâmetro editado em duas cópias e a remoção. Ainda não cobre:
  - os movimentos que perderam;
  - as edições escondidas por uma remoção.

  Esses dois chegam com o registro de conflitos gravado no documento, na entrega 0.7.
- **Espaço no C::** só 3,5 GB livres. Para o teste de 2 GB rodar também no SSD, o C: precisa de 6 GB livres. As sobras antigas dos testes podem ser apagadas à mão: `%TEMP%\bivar-poc-arquivo`, `%TEMP%\bivar-poc-copia` e `%TEMP%\bivar-poc-verificador`.
- **A primeira rodada do fuzzing no GitHub** continua esperando o primeiro envio.

## Como conferir, no PowerShell, a partir da pasta do projeto

```
cargo test --release --manifest-path poc/crdt-armazenamento/prototipo/Cargo.toml -p bivar-poc-arquivo --test copia
```

Deve mostrar `4 passed`. Depois, a bateria de medidas (comando acima) deve terminar com "Acima do limite: 0".

# Falha da CI em `dd7d0af`: arquivo aberto "só leitura" no teste do backup (2026-11-04)

> **Situação:** causa **não provada**, pela regra fixada antes de medir. Nenhuma correção foi aplicada ou proposta. Este registro diz o que foi provado, o que continua hipótese e qual é a próxima medida.

## 1. A falha

**Onde falhou.** CI do commit `dd7d0af`, execução `41693430250`, job `poc-crdt`, passo "Prototype tests". O teste foi `an_old_backup_restored_in_place_gets_a_new_name`, em `poc/crdt-armazenamento/prototipo/verificador/tests/copias.rs`.

**O que ele protege.** Quando um backup antigo volta para o lugar do arquivo, a sessão que o abre ganha um nome de cópia novo. A junção com um colega que tinha a versão posterior não perde nenhuma das duas escritas.

**A mensagem.**
- **Primeira falha:** `PermissionDenied: "read-only"` ao salvar. O teste descartava o relatório da abertura e, por isso, não dizia por que o arquivo tinha aberto só leitura.
- **Desde o passo 1** (commit `4b5aa28`, só no teste), uma gravação que falha mostra o motivo.

**O teste não usa sorteio,** então não há semente.

**O commit `dd7d0af`** mudou só documentos e dados da bateria (o tratamento, 3 registros, `historico.csv`, `referencia.md` e `ultima.json`). Nenhum teste lê esses arquivos: os testes usam só a pasta temporária ou a pasta em `BIVAR_TEST_DIR`.

**No Windows** (este computador, sem Linux), a falha não se repetiu:
- 50 vezes com o teste sozinho;
- 50 vezes com o arquivo de testes inteiro;
- 400 vezes sob carga.

O mecanismo suspeito é o da trava de arquivo do Linux (`flock`), que o Windows não usa.

## 2. Como foi medido (ramo `diag/trava-linux`, nunca juntado ao `main`)

- **Registro (commit `a1d47df`).** Com `BIVAR_DIAG_LOCK_LOG` definida, o programa registra cada abertura para escrita, cada fechamento, cada lançamento do verificador (início e fim) e cada trava recusada. Cada linha leva a hora em nanossegundos, o processo e o teste, porque a linha de execução tem o nome do teste.
- **Na trava recusada,** o registro guarda, no Linux:
  - as linhas de `/proc/locks` do arquivo;
  - todo processo com o arquivo aberto em `/proc/*/fd`, com o PID, o processo pai, o nome, as linhas de trava e o comando.

  Isso é lido duas vezes: logo depois da recusa e 50 ms depois. Sem a variável, nada muda.
- **Job "Diag lock (temporary)"** (execução `41863941502`). O arquivo de testes `copias` é compilado uma vez e roda em laço:
  - **A:** em paralelo, como na CI, por 30 minutos, parando ao chegar a 10 falhas;
  - **B:** com `--test-threads=1`, por 30 minutos.

  Ficam guardados os registros das execuções que falharam ou que tiveram trava recusada. O artefato `diag-trava-logs` foi baixado pelo dono para `A:\Projetos\bivar-diag`, fora do repositório, e lido sem alteração.

## 3. Resultados

| Configuração | Execuções | Tempo | Falhas | Execuções com trava recusada | Travas recusadas |
|---|---|---|---|---|---|
| A, em paralelo | 81 | 9 s (parou nas 10 falhas) | **10** | 48 | 58 |
| B, `--test-threads=1` | 7.471 | 1.800 s | **0** | 0 | 0 |

**Os testes que rodaram.** As 48 execuções guardadas de A rodaram os 5 testes do arquivo `copias`:
- `a_file_copied_to_another_computer_gets_a_new_name`;
- `a_file_duplicated_on_this_computer_gets_a_new_name`;
- `an_old_backup_restored_in_place_gets_a_new_name`;
- `a_file_in_a_synced_folder_gets_a_name_per_computer`;
- `two_sessions_with_the_same_name_still_lose_nothing`.

As 7.471 execuções de B passaram todas, e o laço apaga o registro de uma execução sem falha e sem trava recusada. Por isso não há lista de B no artefato. Que B rodou os mesmos 5 testes **vem da construção do job, não de uma lista guardada:** o mesmo executável, só com `--test-threads=1`, que não filtra testes.

### 3.1 Cada trava recusada

As 58 recusas, agrupadas pelo teste que tentou abrir, pelo arquivo e por qual abertura daquele arquivo era. A 1ª abertura é a da criação; a 2ª é a que vem depois de fechar o arquivo criado.

| Teste que tentou abrir | Arquivo | Abertura | Vezes | Deu falha? |
|---|---|---|---|---|
| `an_old_backup_restored_in_place_gets_a_new_name` | `backup-false.bivar` / `backup-true.bivar` | 2ª (logo antes de um `save`) | 5 + 5 | **sim, as 10 falhas** |
| `an_old_backup_restored_in_place_gets_a_new_name` | `backup-false.bivar` / `backup-true.bivar` | 3ª (o backup restaurado; nada é salvo depois) | 9 + 1 | não |
| `a_file_duplicated_on_this_computer_gets_a_new_name` | `duplicado-true.bivar` / `duplicado-false.bivar` | 2ª | 10 + 6 | não |
| `a_file_copied_to_another_computer_gets_a_new_name` | `outro-pc-false.bivar` / `outro-pc-true.bivar` | 2ª | 8 + 6 | não |
| `a_file_in_a_synced_folder_gets_a_name_per_computer` | `pasta-sincronizada-false.bivar` | 2ª (5) e 3ª (1) | 6 | não |
| `two_sessions_with_the_same_name_still_lose_nothing` | `mesmo-nome-false.bivar` / `mesmo-nome-true-clone.bivar` | 2ª | 1 + 1 | não |

**Quem segurava a trava:**
- **Nos processos:** em nenhuma das 58 recusas a varredura achou um processo com o arquivo aberto, nem o próprio processo de testes nem outro.
- **Em `/proc/locks`:**
  - em 55 recusas não havia nenhuma linha de trava do arquivo;
  - em 3 havia uma linha (`FLOCK ADVISORY WRITE`) com o PID do próprio processo de testes (5356 e 6355).
- **Esse PID não identifica quem segura a trava.** Em `/proc/locks`, o PID de uma trava `flock` é o de quem a criou, e continua o mesmo quando a trava é herdada por outro processo.
- **Na releitura, 50 ms depois,** não havia mais nenhuma trava.

**Nenhum outro teste usou o mesmo caminho.** Em todas as 48 execuções guardadas, cada arquivo foi aberto, fechado ou recusado só pelo seu próprio teste.

### 3.2 As 48 recusas contra as 10 falhas

- **As 10 execuções que falharam** tiveram 1 recusa cada. Foi sempre a 2ª abertura do arquivo do backup, que é a única seguida de um `save` naquele arquivo. O `save` encontra o arquivo só leitura e o teste falha.
- **As outras 38 execuções** tiveram 48 recusas e passaram:

  | Teste que recebeu a recusa | Recusas |
  |---|---|
  | `a_file_duplicated_on_this_computer_gets_a_new_name` | 16 |
  | `a_file_copied_to_another_computer_gets_a_new_name` | 14 |
  | `an_old_backup_restored_in_place_gets_a_new_name` (3ª abertura) | 10 |
  | `a_file_in_a_synced_folder_gets_a_name_per_computer` | 6 |
  | `two_sessions_with_the_same_name_still_lose_nothing` | 2 |

- **Esses testes passaram com o arquivo só leitura, sem perceber.**
  - O que eles prometem (nome de cópia novo e nenhuma escrita perdida na junção) é conferido na memória e não depende do arquivo poder ser gravado. Por isso continuou certo.
  - Nenhum deles confere que a abertura foi para escrita, nem lê o motivo do "só leitura" ("already open for writing elsewhere"), que era falso: nenhum outro teste usava o arquivo.
  - Num programa de verdade, essa sessão não poderia salvar e diria que o arquivo está aberto em outro lugar.

### 3.3 Por que B não teve nenhuma recusa

Com uma linha de execução só, nunca há um teste fechando um arquivo enquanto outro lança o verificador. Os lançamentos acontecem na mesma linha de execução, que fica parada até o verificador começar a rodar.

Isso combina com a hipótese da herança da trava, mas combina também com qualquer causa que dependa de testes em paralelo. Não distingue uma da outra.

### 3.4 Coincidência no tempo (indício, não prova)

| Abertura logo depois de fechar o mesmo arquivo | Total | Com o fechamento durante um lançamento do verificador por outro teste |
|---|---|---|
| Recusada | 58 | **55 (95%)** |
| Deu certo | 867 | 16 (1,8%) |

- **Entre fechar e a recusa,** passaram de 57 a 736 µs (média de 196 µs).
- **Nos 3 casos restantes,** o arquivo foi fechado antes de qualquer lançamento começar, e mesmo assim a trava foi recusada de 706 a 736 µs depois. São as execuções 13, 37 e 55, todas na 2ª abertura de `outro-pc-false.bivar`.
- **A herança, como descrita, não explica esses 3 casos.** Por ela, um processo filho só herda a trava se for criado enquanto o arquivo ainda está aberto.

## 4. Aplicação da regra fixada antes de medir

> "A causa só conta como provada se a falha mostrar quem segurava a trava e essa informação explicar o mecanismo. Se A não falhar nenhuma vez, a causa não está provada. Se o responsável for outro, descarte a hipótese da herança da trava."

- **A falhou:** 10 falhas em 81 execuções.
- **A captura não mostrou quem segurava a trava** em nenhuma das 58 recusas. Por isso, **a causa não está provada**, e nenhuma correção é proposta.
- **Os dados não apontam para outro responsável:**
  - nenhum outro teste usou o mesmo caminho;
  - nenhum processo tinha o arquivo aberto no momento da leitura;
  - as 3 linhas de `/proc/locks` têm o PID de quem criou a trava, que é o mesmo numa trava herdada.

  Por isso **a hipótese da herança da trava não é descartada**, mas continua incompleta por causa dos 3 casos sem lançamento em andamento.

### Provado

1. **A falha não vem do commit `dd7d0af`, de tempo-limite nem de sorteio.**
2. **A falha só acontece com testes em paralelo:** 10 falhas em 81 execuções em A, contra 0 em 7.471 em B.
3. **O motivo do "só leitura" é a trava recusada** ("already open for writing elsewhere"), sem nenhum outro teste usando o mesmo caminho.
4. **A trava sumiu em menos de 50 ms em todas as recusas.** Em 55 das 58, já tinha sumido no momento da primeira leitura.
5. **A recusa atinge os 5 testes do arquivo.** Só o teste do backup falha, porque é o único que salva depois da abertura recusada. Os outros passam com o arquivo só leitura sem perceber.
6. **A falha do `main` em `4b5aa28`** (execução `41863553288`) **é igual às do ramo:** mesmo teste, `backup-false.bivar`, mesma linha (`copias.rs:72`) e mesmo motivo mostrado pelo teste ("opened read-only: the file is already open for writing elsewhere").

### Hipótese (não provada)

1. **A herança da trava.** No Linux, um processo filho criado por outro teste começa com uma cópia dos arquivos abertos e segura a trava até passar a rodar o verificador. Indícios:
   - 55 das 58 recusas aconteceram com o fechamento durante um lançamento por outro teste, contra 1,8% das reaberturas que deram certo;
   - a trava some em microssegundos.

   Contra ela: os 3 casos sem lançamento em andamento.
2. **Os 3 casos** podem ter outra explicação, ainda não identificada. Uma possibilidade: o registro marca "fechar" pouco antes de o arquivo ser realmente fechado. Também não está provada.
3. **Se a hipótese 1 estiver certa, o defeito também é do programa, e não só do teste.** No Linux e no macOS, abrir o verificador para um arquivo enquanto outro documento é fechado e reaberto faria esse documento abrir "só leitura, já aberto em outro lugar" sem motivo real.

## 5. Próxima medida (proposta, não aplicada)

No mesmo ramo `diag/trava-linux`, sem dependência nova, sem `unsafe` e sem mudar o comportamento do programa:

1. **Marcar a hora do fechamento depois de o arquivo ser realmente fechado,** e não antes, e marcar a hora exata em que a trava é pedida. Isso confere os 3 casos fora do padrão.
2. **Na recusa, antes de qualquer outra leitura, olhar só os filhos do processo de testes** (`/proc/self/task/*/children`). Para cada filho, ler o estado, o comando e se ele tem o arquivo aberto (`/proc/<filho>/fd`). Essa leitura leva microssegundos, e não milissegundos como a varredura de todos os processos.
3. **Repetir a tentativa da trava num arquivo aberto à parte, em laço curto, até ela ficar livre,** registrando quando ficou livre e o que cada filho estava fazendo em cada volta. A trava pedida à parte é solta logo em seguida.
4. **A mesma configuração A:** 30 minutos ou 10 falhas.

**Regra proposta, para fixar antes de medir:**
- **A causa fica provada** se, nas recusas, a leitura achar um filho do processo de testes com o arquivo aberto e com o comando do próprio executável de testes (ainda antes de passar a rodar o verificador), e se a trava ficar livre quando esse filho passa a ser o verificador (o mesmo PID do "SPAWN-END").
- **A hipótese é descartada** se a leitura achar outro responsável, ou se as recusas continuarem sem nenhum filho com o arquivo aberto.

## 6. CI e Fuzz

| Commit | CI | `cargo deny` e `cargo audit` das provas | Fuzz |
|---|---|---|---|
| `4b5aa28` (`main`, passo 1) | falhou em `poc-crdt`: este mesmo teste | pulados, porque o passo anterior falhou | passou |
| `a1d47df` (ramo `diag/trava-linux`) | passou nas 5 partes | **rodaram e passaram** | passou |

O ramo `diag/trava-linux` continua no GitHub. Não foi apagado e não será juntado ao `main`.

## 7. Segunda medida (aprovada pelo dono em 2026-11-04)

> **Resultado:** a regra fixada antes de medir **não fechou**. A configuração C teve recusas, e, pela regra, isso quer dizer que a hipótese da herança da trava não explica tudo. A causa continua **não provada**, nenhuma correção é proposta e nenhuma medida nova foi inventada.

**Regra fixada antes de medir:**
- a causa só conta como provada se:
  - M1 tiver recusas;
  - M2 tiver zero;
  - A continuar com recusas;
  - C tiver zero recusas;
  - e houver execuções suficientes para a comparação fazer sentido;
- qualquer recusa em C ou em M2 quer dizer que a hipótese não explica tudo;
- se a regra não fechar, nenhuma medida nova é inventada.

### 7.1 Como foi medido

Ramo `diag/trava-linux`, commit `1e97bc7`, execução `41876124751`, com os dois jobs no mesmo envio.

- **Reprodução mínima (`poc/diag-trava-repro`).** É um programa separado, sem código do Bivar e sem dependências.
  - Uma linha de execução abre um arquivo para leitura e escrita, pede a mesma trava do protótipo (`File::try_lock`), fecha o arquivo e o abre de novo, contando as recusas.
  - Em M1, outras 4 linhas de execução lançam `/bin/true` sem parar, do mesmo jeito que o protótipo lança o verificador (entrada vazia, saídas por tubo).
  - Em M2, não há lançamentos.
  - Cada uma roda 5 minutos.
- **Intervenção (`copias`, compilado uma vez).** Execuções alternadas, uma de A, uma de C, até 500 de cada ou 30 minutos, sem parar antes por falha.
  - **A:** sem mudanças, como controle.
  - **C:** os mesmos testes e o mesmo paralelismo, com `BIVAR_DIAG_SERIALIZE=1`. Essa variável liga uma trava global, só de diagnóstico e dentro do processo de testes, que fica presa durante o `spawn()` do verificador e durante o `try_lock` de um arquivo. Assim, as duas coisas nunca acontecem ao mesmo tempo.
  - Sem a variável, nada fica preso, então A roda o mesmo código de antes.

### 7.2 Resultados

**Reprodução mínima:**

| Medida | Tempo | Tentativas de travar | Recusas | Lançamentos |
|---|---|---|---|---|
| M1, com lançamentos | 300 s | 145.354.573 | **44.933.426 (30,9%)** | 1.886.561 |
| M2, sem lançamentos | 300 s | 242.501.000 | **0** | 0 |

**Intervenção:**

| Configuração | Execuções | Falhas | Recusas | Execuções com recusa | Tempo somado das execuções | Lista de testes |
|---|---|---|---|---|---|---|
| A, controle | 500 | **87** | **318** | 267 | 53 s | os 5 testes, igual nas 500 |
| C, com a trava global | 500 | **44** | **175** | 156 | 52 s | os 5 testes, igual nas 500 |

- **A lista de testes foi conferida em cada execução** e é a mesma nas 1.000:
  - `a_file_copied_to_another_computer_gets_a_new_name`;
  - `a_file_duplicated_on_this_computer_gets_a_new_name`;
  - `a_file_in_a_synced_folder_gets_a_name_per_computer`;
  - `an_old_backup_restored_in_place_gets_a_new_name`;
  - `two_sessions_with_the_same_name_still_lose_nothing`.
- **As 1.000 execuções levaram 114 s** e chegaram às 500 de cada antes dos 30 minutos.

### 7.3 Aplicação da regra

| Condição | Esperado | Medido | Cumpriu? |
|---|---|---|---|
| M1 tem recusas | > 0 | 44.933.426 | sim |
| M2 tem zero | 0 | 0 | sim |
| A continua com recusas | > 0 | 318 | sim |
| C tem zero recusas | 0 | **175** | **não** |
| Execuções suficientes | — | 500 + 500, e M1/M2 com mais de 100 milhões de tentativas cada | sim |

**A regra não fechou.**

### 7.4 Provado

1. **No Linux, lançar processos enquanto outra linha de execução fecha e reabre um arquivo travado faz a trava ser recusada, sem nenhum código do Bivar.** Isso aconteceu em 31% das tentativas em M1, e nunca em mais de 242 milhões de tentativas sem lançamentos (M2).
2. **A trava global de C reduz as recusas e as falhas na suíte,** mas não acaba com elas: de 318 para 175 recusas e de 87 para 44 falhas, nas mesmas 500 execuções e com os mesmos testes.
3. **Portanto, impedir que o lançamento do verificador e o pedido da trava aconteçam ao mesmo tempo não basta.** Na suíte, parte das recusas tem outra origem, ou acontece fora do intervalo que a trava global de C cobre.

### 7.5 Hipótese (não provada)

- **O que segura a trava nas 175 recusas de C** não foi identificado. Também não está provado se é o mesmo mecanismo de M1 fora do intervalo coberto, ou outra coisa.
- **As 3 recusas sem verificador em andamento** da primeira medida (seção 3.4) **continuam sem explicação.** As 175 recusas de C mostram que há recusas que a hipótese, como foi descrita, não cobre. Essas 3 podem ser do mesmo grupo, mas isso não foi verificado.

### 7.6 Contagem correta de recusas por teste (primeira medida, configuração A)

- **58** é o total de recusas, em **48** execuções.
- **10** dessas recusas estão nas 10 execuções que falharam, uma em cada. Foi sempre a 2ª abertura do arquivo do backup, logo antes de um `save`.
- **As outras 48 recusas** estão nas outras **38** execuções, que passaram.
- **O número 48 aparece duas vezes por coincidência:** 48 execuções com recusa e 48 recusas sem falha.

| Teste que recebeu a recusa | Com falha | Sem falha | Total |
|---|---|---|---|
| `an_old_backup_restored_in_place_gets_a_new_name` | 10 (2ª abertura) | 10 (3ª abertura) | 20 |
| `a_file_duplicated_on_this_computer_gets_a_new_name` | 0 | 16 | 16 |
| `a_file_copied_to_another_computer_gets_a_new_name` | 0 | 14 | 14 |
| `a_file_in_a_synced_folder_gets_a_name_per_computer` | 0 | 6 | 6 |
| `two_sessions_with_the_same_name_still_lose_nothing` | 0 | 2 | 2 |
| **Total** | **10** | **48** | **58** |

### 7.7 Testes que precisam passar a conferir que o arquivo abriu para escrita

Nenhum foi alterado agora. Todos estão em `poc/crdt-armazenamento/prototipo/verificador/tests/copias.rs`.

- **Os 5 testes do arquivo, em cada abertura:**

  | Teste | Aberturas que precisam conferir |
  |---|---|
  | `a_file_copied_to_another_computer_gets_a_new_name` | `p1` e `p2` |
  | `a_file_duplicated_on_this_computer_gets_a_new_name` | `p` e `dup` |
  | `an_old_backup_restored_in_place_gets_a_new_name` | a abertura antes do `save`, que hoje só falha no `save`, e a do backup restaurado, que hoje passa sem perceber |
  | `a_file_in_a_synced_folder_gets_a_name_per_computer` | `p1`, `p2` e a reabertura de `p1` |
  | `two_sessions_with_the_same_name_still_lose_nothing` | `p1` e `p2`, e a abertura direta com `DocFile::open` |

- **A criação, em `original()`, usada pelos 5 testes:** `DocFile::create` não informa se o arquivo ficou aberto para escrita.
- **Não foi verificado** se os testes de outros arquivos (`disco.rs`, `copia.rs`, `entrada.rs`) também abrem arquivos sem conferir isso. Neste diagnóstico, só o `copias` rodou em laço.

### 7.8 Observações que não fazem parte da regra

- **No Windows,** o executável do verificador compilado com a trava global de C foi recusado pelo sistema ao ser lançado: "Acesso negado", inclusive rodando direto pelo terminal. A versão anterior rodava normalmente. Isso parece uma política de segurança do Windows contra executáveis novos sem reputação, e não o código, mas não foi investigado. A medida rodou no Linux da CI, onde o código compilou e rodou.
- **O Fuzz do ramo falhou** em `1e97bc7`, no alvo `bivar_records`, no passo "Fuzz bivar_records" (execução `41876124671`). O GitHub guardou uma entrada que causou o erro (artefato `fuzz-artifacts-bivar_records`, 569 bytes).
  - O registro e o artefato exigem login e não foram lidos.
  - O Fuzz passou no `main` em `343e453` e em `f420b9d`, e no ramo em `a1d47df`.
  - Não se sabe se a falha vem do código de diagnóstico do ramo ou se é um achado real do sorteio do fuzzing.

## 8. CI e Fuzz (atualizado)

| Commit | CI | `cargo deny` e `cargo audit` das provas | Fuzz |
|---|---|---|---|
| `343e453` (`main`, relatório) | passou | rodaram e passaram | passou |
| `f420b9d` (`main`, a CI roda deny e audit mesmo depois de falha) | falhou em `poc-crdt`, no passo "Prototype tests" (código 101; o teste não foi identificado, porque o registro exige login) | **rodaram e passaram depois da falha** | passou |
| `1e97bc7` (ramo `diag/trava-linux`) | passou | rodaram e passaram | **falhou em `bivar_records`** (veja 7.8) |

O ramo `diag/trava-linux` continua no GitHub, sem ser apagado nem juntado ao `main`.

# Entrega 0.6, Parte 4: o arquivo no disco

Data: 2026-10-29. Prova descartável. As decisões estão no [ADR-0023](../../docs/adr/0023.md), e a especificação em [`docs/formato/`](../../docs/formato/README.md).

## O que foi feito

- **Um pacote novo, `bivar-poc-arquivo`** ([`prototipo/arquivo/`](prototipo/arquivo/)): o formato `.bivar`. Ele não depende da Loro; fala com o documento só pela interface `Store`, e a checagem automática agora confere esse pacote também.
- **A loja** ganhou:
  - versões e atualizações incrementais;
  - o registro do que cada cópia já viu;
  - marcos de tempo e marcos nomeados;
  - o plano de compactação e a compactação;
  - a data de cada remoção;
  - o índice de nós escondidos e vivos.
- **Biblioteca nova:** só a `sha2` `=0.11.1`, aprovada. O `cargo deny` passa.
- **Especificação escrita do formato,** em português e em inglês (condição 1 do dono).

## Os testes (todos passam)

Os 15 testes do arquivo rodaram **nos dois discos**, com **30 mortes de processo** em cada teste de queda ([registro](prototipo/resultados/disco-testes.txt)):

| Teste | O que prova |
| --- | --- |
| `save_and_reopen_keep_the_document` | salvar e reabrir dá o mesmo documento |
| `edits_not_saved_are_back_after_reopening` | edições sem salvar voltam |
| `edits_since_the_last_save_survive_a_kill` | 40 edições sem salvar, processo morto: as 40 voltam |
| `killing_the_program_while_writing_never_breaks_the_file` | processo morto 30 vezes no meio da gravação: o arquivo sempre abre |
| `a_copy_made_while_writing_always_opens` | 60 cópias feitas durante a gravação: todas abrem |
| `killing_during_compaction_leaves_the_old_file_or_the_new_one` | **condição 3 do dono:** processo morto 30 vezes durante a compactação: sempre há um arquivo válido |
| `a_cut_last_write_is_dropped_and_everything_before_stays` | todos os cortes possíveis do último registro: o arquivo abre com o que estava completo |
| `damage_in_the_middle_opens_what_it_can_read_only` | dano no meio: abre o que dá, só para leitura, com mensagem |
| `random_damage_never_crashes_the_reader` | 300 arquivos com bytes trocados ou cortados ao acaso: nunca cai. O fuzzing de verdade é da Parte 5 |
| `a_second_opening_is_read_only_with_a_reason` | a segunda abertura é só para leitura |
| `large_files_are_stored_once_and_a_parameter_change_rewrites_no_piece` | arquivo grande guardado uma vez; mudar um parâmetro não regrava nada já escrito |
| `migration_keeps_a_copy_never_harms_the_original_and_newer_files_are_refused` | migração: cópia guardada, original intacto se falhar, versão mais nova recusada, opção de criptografia recusada |
| `compaction_cleans_only_what_the_rules_allow` | antes de 30 dias nada é limpo; depois, só o que as regras permitem; os pedaços da imagem limpa saem |
| `milestones_and_open_conflicts_are_never_cleaned` | marcos nomeados e conflitos não resolvidos ficam |
| `a_copy_offline_longer_than_the_kept_history_is_told_not_merged_silently` | a cópia offline aparece no plano antes; ao voltar, a junção é recusada nos dois sentidos, e o trabalho dela fica intacto |

**Os 2.000 + 2.000 cenários** continuam passando com todas as mudanças da loja ([registro](prototipo/resultados/contagem-nada-se-perde.txt)).

## As medições ([registro](prototipo/resultados/disco-medidas.txt))

| Medida | SSD (C:) | HD (A:) |
| --- | --- | --- |
| Forçar a gravação no disco (mediana / pior de 100) | 0,35 ms / 0,69 ms | 24 ms / 64 ms |
| **Pausa da interface no salvamento automático** (limite 16 ms), documento de 100.000 operações | **1,03 ms** | **1,02 ms** |
| Gravação forçada, fora da interface (pior caso) | 0,7 ms | 35 ms |
| Arquivo de 2 GB: gravar e salvar | 16,7 s | 12,5 s |
| Arquivo de 2 GB: **abrir** | **< 0,01 s** | **< 0,01 s** |
| Arquivo de 2 GB: ler tudo e conferir | 4,3 s | 4,2 s |
| Mudar um parâmetro depois e salvar | 0,5 ms, +333 bytes | 17,7 ms, +333 bytes |
| 200.000 edições, salvamento automático a cada 100 | 6,0 MB | 6,0 MB |
| Compactação 31 dias depois (6.441 nós limpos, histórico cortado) | 3,1 MB, em 1,3 s | 3,1 MB, em 1,3 s |

**O índice de nós escondidos** (pedido do dono), com 10.000 grupos um dentro do outro:

- **sem o índice, uma leitura leva 962 ms;**
- **com o índice, leva 0,6 µs;**
- reconstruir o índice leva 6 ms nesse documento e 14 ms num documento comum de 100.000 operações.

**Atenção ao medir o HD:** o arquivo de 2 GB tinha acabado de ser gravado, então parte dele ainda estava na memória do Windows. Uma leitura "fria", depois de reiniciar o computador, seria mais lenta no HD.

## Problemas encontrados e corrigidos nesta parte

1. **Abrir um arquivo com muitos registros pequenos era lento:** 3 rodadas de teste levavam 17 s, porque cada registro era importado separadamente. As atualizações passaram a ser importadas todas de uma vez, e caiu para 1,6 s.
2. **Abrir o arquivo com uma imagem de 2 GB relia os 2 GB:** 3 s, mesmo com o arquivo na memória do sistema. Cada pedaço passou a trazer a própria identidade no começo, e a abertura lê só ela. Caiu para menos de 0,01 s. A especificação foi atualizada.
3. **A Loro aceitava, sem reclamar, juntar uma cópia antiga a um documento compactado.** A junção agora é **recusada antes de importar**, nos dois sentidos.
4. **A Loro não abre um documento compactado "no passado"** (`fork_at` não existe para documentos rasos). A junção agora usa uma cópia movida no tempo (`checkout`).
5. **Dois marcos de tempo gravados no mesmo segundo** se sobrescreviam. A chave passou a incluir o número de operações.
6. **A primeira versão do índice** ainda perguntava à Loro se o nó tinha sido apagado de vez (0,22 ms por leitura). O índice passou a guardar também os nós vivos (0,6 µs).

Houve também erros nos próprios testes: um esperava limpar só um nó, num documento que já tinha outros removidos; outro procurava o sinal de "pronto" do processo filho no lugar errado da linha.

## Riscos que ficam (registrados no ADR-0023)

- **A troca de nome no fim da compactação** é atômica na prática no NTFS, mas sem garantia formal numa queda de **energia** naquele exato instante. Os testes matam o processo, não desligam o computador.
- **O código do formato é nosso.** Precisa do fuzzing da Parte 5 e de testes de queda na N.2.
- **A migração do histórico** entre organizações diferentes do documento (`schema`) está descrita, mas a prova fica para a N.2.

## Como conferir, no PowerShell, a partir da pasta do projeto

1. Rode os testes do protótipo (cerca de 1 minuto). Devem terminar sem falhas:

   ```
   cargo test --release --manifest-path poc/crdt-armazenamento/prototipo/Cargo.toml
   ```

2. Para rodar os testes do arquivo no HD, com 30 mortes de processo:

   ```
   $env:BIVAR_TEST_DIR="A:\Projetos\Bivar\poc\crdt-armazenamento\prototipo\resultados\tmp-hd"; $env:BIVAR_KILLS=30; cargo test --release --manifest-path poc/crdt-armazenamento/prototipo/Cargo.toml -p bivar-poc-arquivo --test disco -- --test-threads=1
   ```

3. Depois, feche e abra o PowerShell, para os ajustes do passo 2 não valerem para os próximos testes.

# Medições da entrega 0.6

Mediana de 3 execuções, cada uma num processo separado. Compilação `--release`.

## loro

- **100.000 operações misturadas:** 0.80 s (limite 5 s: passa), memória a mais 57.1 MB, salvar 0.11 s, arquivo 2.8 MB.
  - mistura feita: `[(SetParam,40049),(EditText,10100),(Create,14977),(Remove,4761),(Connect,9938),(Disconnect,5129),(Reorder,10082),(Move,4964)]`
- **Junção 10.000 + 10.000 (nos dois sentidos):** 0.196 s (limite 1 s: passa), cópias idênticas: true, avisos na cópia A (escritas/movimentos/escondidos): 422w/224m/928h, memória a mais na tarefa 47.5 MB, arquivo 0.6 MB.
- **Documento com 1 milhão de operações:** gerado em 12.5 s; 1483122 operações no histórico; arquivo 29.6 MB.
- **Abrir esse documento:** 0.01 s (limite 3 s: passa), memória a mais 214.2 MB (limite 1 GB: passa); primeira leitura completa depois de abrir 2.08 s; leitura do arquivo no disco 0.01 s; 101424 nós visíveis.
- **Desfazer algo que outra cópia já editou por cima:** `UndoOutcome_{_undone:_true,_skipped:_[]_}_->_Some(Str("green"))`

## automerge

- **100.000 operações misturadas:** 6.43 s (limite 5 s: **não passa**), memória a mais 75.3 MB, salvar 0.07 s, arquivo 1.3 MB.
  - mistura feita: `[(SetParam,40049),(EditText,10100),(Create,14977),(Remove,4761),(Connect,9938),(Disconnect,5129),(Reorder,10082),(Move,4964)]`
- **Junção 10.000 + 10.000 (nos dois sentidos):** 0.971 s (limite 1 s: passa), cópias idênticas: true, avisos na cópia A (escritas/movimentos/escondidos): 422w/141m/1557h, memória a mais na tarefa 71.9 MB, arquivo 0.3 MB.
- **Documento com 1 milhão de operações:** gerado em 94.4 s; 1527922 operações no histórico; arquivo 13.6 MB.
- **Abrir esse documento:** 7.62 s (limite 3 s: **não passa**), memória a mais 1138.3 MB (limite 1 GB: **não passa**); primeira leitura completa depois de abrir 3.74 s; leitura do arquivo no disco 0.00 s; 101424 nós visíveis.
- **Desfazer algo que outra cópia já editou por cima:** `UndoOutcome_{_undone:_false,_skipped:_["color:_changed_by_another_copy_after_this_edit"]_}_->_Some(Str("blue"))`


## Abrir só com a biblioteca (sem a camada desta prova)

3 execuções cada, mesmo arquivo de 1 milhão de operações:

- **Loro** (`LoroDoc::from_snapshot`): 0,015 s; memória a mais 31,4 MB. A Loro só decodifica o estado no primeiro uso. A primeira leitura completa leva 2,08 s, e o total com essa leitura fica em 214,2 MB.
- **Automerge** (`AutoCommit::load`): 3,87 a 3,89 s; memória a mais 1,11 a 1,14 GB. **Já passa dos dois limites sem nenhum código desta prova.** O índice da árvore feito por fora soma cerca de 3,7 s.

# Proposal: report concurrent writes (the losing values) on import

> **RASCUNHO NÃO ENVIADO.** Em 2026-10-29, o dono decidiu **não enviar** esta proposta, para o Bivar não depender de outros projetos (registrado no [ADR-0021](../../docs/adr/0021.md), seção 5). O texto fica guardado só como registro.

*Draft written for the Bivar project to post on [loro-dev/loro](https://github.com/loro-dev/loro). Not sent, by the owner's decision.*

---

**Title:** Expose concurrent writes (winner and losing values) to the same map key / tree node after `import`

Hi! We are building [Bivar](https://github.com/crocodiliano/bivar-design), an open-source (GPL-3.0) design application in Rust, and we chose Loro 1.17.1 as the CRDT under our document graph. Thank you for the movable tree and the performance: in our benchmark, Loro opened a 1-million-operation document in 2.1 s (state read included) and merged 10,000 + 10,000 concurrent operations in 0.2 s.

### What we need

Our product rule is "nothing is lost without a warning": when two peers set the same parameter concurrently, last-writer-wins is fine, but the user must see a warning and the history must keep the value that lost. The same applies to two peers moving the same tree node concurrently, and to a move that is ignored because it would create a cycle.

Today, after `doc.import(...)`, Loro tells us the new state (and events with the new values), but not:

1. that a write to a `LoroMap` key was **concurrent** with another write to the same key;
2. the **losing value(s)**;
3. for `LoroTree`, which concurrent `move` lost, and which moves were ignored because they would form a cycle.

### How we do it now (and why it is fragile)

We reconstruct this outside Loro:

- `export_json_updates_without_peer_compression` for both sides;
- the causal past of each incoming change (`frontiers_to_vv` on its `deps`);
- and Loro's internal winner rule, so we know which value won: higher Lamport timestamp, then higher peer id.

This works (it passes 2,000 randomized two-peer and 2,000 three-peer scenarios), but it costs about 260 lines on our side. It also depends on an internal rule that is not part of Loro's public contract, so we had to add a test that fails if the rule changes. It also took several iterations to get right. For example, the Lamport time of each op is `change.lamport + (op.counter - change.id.counter)`, not the op's index in the change, because a text insert spans several counters.

### What would help

Any of these, in order of preference:

1. **An import report.** For example, `import_with_report(bytes) -> ImportReport` listing, per container and key, the winning and losing `(ID, value)` of concurrent map writes, and for trees the losing and ignored moves.
2. **A query on map keys.** Something like `LoroMap::get_concurrent(key) -> Vec<(ID, LoroValue)>`, returning all values that are not causally overwritten. Automerge's `get_all` is similar.
3. **A documented, stable winner rule** for map keys and tree moves, so code like ours relies on a contract and not on an implementation detail.

We would be glad to help with tests or a PR if one of these directions fits Loro's design.

Thanks again for the library!

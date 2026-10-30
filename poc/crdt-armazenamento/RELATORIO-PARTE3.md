# Entrega 0.6, Parte 3: o protótipo do grafo na Loro

Data: 2026-10-29. Prova descartável. As decisões estão no [ADR-0021](../../docs/adr/0021.md).

## O que foi feito

**Um protótipo em três pacotes** ([`prototipo/`](prototipo/)):

| Pacote | O que tem | Depende da Loro? |
| --- | --- | --- |
| `bivar-poc-modelo` | Operações do ADR-0002 (`Op`, versão `bivar-op/1`), a interface `Store`, o desfazer (`Document`), a detecção de conflitos e o kit de testes | **não** |
| `bivar-poc-loja-loro` | A loja na Loro: o grafo dentro da árvore, os mapas e os textos da Loro, e a junção com o relatório de avisos | **sim, o único** (`=1.17.1`) |
| `bivar-poc-cadeia` | A cadeia Forma → Traçado → Preenchimento → Saída, reordenação da pilha, conexões entre objetos e o indicador "customizado" | **não** (a Loro só entra nos testes dele) |

**A checagem automática** ([`prototipo/check-loro-isolated.sh`](prototipo/check-loro-isolated.sh)) faz duas conferências:

- nenhum pacote além da loja depende da Loro;
- a loja usa exatamente a 1.17.1.

Ela foi testada nos dois sentidos: passa no protótipo e falha, com mensagem clara, quando a Loro é posta no pacote `cadeia`. Entrou na compilação automática do GitHub (job `poc-crdt`), junto com os testes e o `cargo deny` das duas provas.

## Testes (todos passam)

| Onde | Testes |
| --- | --- |
| `modelo` | ida e volta de todas as operações pelo texto serializado; outra versão do formato recusada; entrada malformada recusada sem derrubar o programa |
| `loja-loro/tests/tie_break.rs` | **trava da regra de desempate da Loro** (condição 1): mesmo relógio, vence a maior cópia; relógio maior vence mesmo com cópia menor; movimentos seguem a mesma regra |
| `loja-loro/tests/store.rs` | escrita simultânea com os dois valores; movimentos sem duplicar nem formar ciclo, sabendo o perdedor; nó removido enquanto editado (avisado e restaurável); disputa entre remover e restaurar; versão de organização diferente recusada; cópia para enviar sem histórico e sem nada removido; desfazer só o próprio; desfazer nunca sobrescreve outra cópia; transação de 500 operações num passo; desfazer de remoção traz o mesmo nó com as conexões; desfazer de texto acompanha edições alheias; nada se perde (duas e três cópias) |
| `cadeia/tests/chain.rs` | a cadeia avalia na ordem da pilha; reordenar Traçado e Preenchimento muda a ordem e se desfaz; Forma e Saída não se movem na pilha; reordenar e acrescentar efeito em cópias diferentes valem os dois; reordenar o mesmo efeito em duas cópias deixa um, sem duplicar, com aviso; conexão para um objeto removido aparece como referência quebrada em todas as cópias; pilha mexida fora do formato ganha "customizado" |

**Os milhares de cenários aleatórios** (condição 6): 2.000 com duas cópias e 2.000 com três, **sem nenhuma falha** ([registro](prototipo/resultados/contagem-nada-se-perde.txt)).

**Tempo de junção** de 10.000 + 10.000 operações com o relatório completo: **0,57 s** (mediana de 3; limite 1 s). Teste sob pedido: `loja-loro/tests/merge_time.rs`.

## Erros encontrados nesta parte

Todos foram achados pelos testes:

1. **Posição de movimento contada com irmãos removidos.** Com a remoção por marcação, os nós removidos continuam na árvore da Loro, e ela os conta ao posicionar. Um movimento caía no lugar errado, ou no lugar onde o nó já estava, e então não era gravado. **Achado só com 2.000 cenários: 7% falhavam, e os 6 rápidos passavam.**
2. **A regra antiga de "nó escondido"** ("caminhar até o nó removido e perguntar quem removeu") falhou com três cópias, quando uma junção reativou um movimento antes anulado para evitar ciclo. Ela foi trocada pela regra "visível onde foi editado, escondido depois da junção".
3. **A regra nova, na primeira versão,** olhava a outra cópia **depois** de ela já ter juntado as mudanças desta. A versão certa olha a outra cópia como ela era quando editou (`fork_at`).

Houve também um erro no próprio teste da trava: ele lia o relógio da mudança inteira, e não o de cada operação.

## Pontos para o dono confirmar

Estes três diferem do texto da entrega ou da condição. Os motivos estão no ADR-0021:

1. **A pilha é a ordem dos filhos do objeto,** e não uma lista separada. A entrega citava "lista para a pilha" como exemplo.
2. **O desfazer é uma camada do Bivar que substitui o da Loro,** e não uma camada por cima dele. Ele cumpre a condição: confere antes de desfazer e avisa.
3. **No produto, o pacote que depende da Loro é o `bivar-doc`**, e o `bivar-core` fica com as operações e a interface.

## Como conferir

```
cargo test --release --manifest-path poc/crdt-armazenamento/prototipo/Cargo.toml
sh poc/crdt-armazenamento/prototipo/check-loro-isolated.sh
```

O primeiro comando deve terminar sem falhas, e o segundo deve dizer "Loro is isolated in bivar-poc-loja-loro (1.17.1)".

Para os 2.000 + 2.000 cenários (cerca de 1 minuto):

```
BIVAR_SEEDS=2000 BIVAR_COUNT=1 cargo test --release --manifest-path poc/crdt-armazenamento/prototipo/Cargo.toml -p bivar-poc-loja-loro --test store nothing
```

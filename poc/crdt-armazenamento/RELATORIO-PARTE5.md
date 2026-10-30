# Entrega 0.6, Parte 5: toda entrada externa, arquivos-bomba e fuzzing

Data: 2026-10-30. Decisões e números completos no [ADR-0024](../../docs/adr/0024.md); análise em [`PARTE5-ENTRADA.md`](PARTE5-ENTRADA.md).

## O que foi feito

- **Antes da Parte 5, a troca da compactação ganhou a cópia de segurança `.anterior`** (pedido do dono). O processo morre em cada um dos 4 passos da troca, e o documento sempre volta. Isso está no ADR-0023 e em `docs/formato/`.
- **Limites do `.bivar`**, conferidos antes de ler ou reservar memória. A mensagem diz qual limite foi passado e que ele pode ser aumentado nas configurações.
- **Impressão digital do arquivo** e **lista local de arquivos confiáveis**, fora de qualquer documento.
- **Processo verificador** (`prototipo/verificador`): abre arquivos de fora com 60 s e 4 GiB (`cap`), sem gravar nada.
- **A Loro só recebe os dados depois de a leitura inteira terminar** e de a impressão digital bater com a verificada.
- **Fuzzing** em `fuzz/` (`cargo-fuzz` + libFuzzer, *nightly* travada só ali, NCSA só ali), na compilação automática (`.github/workflows/fuzz.yml`), com a coleção reproduzida no `cargo test` normal.

## Testes (64 no protótipo, todos passam)

**Os 12 testes de entrada externa** (`verificador/tests/entrada.rs`):

- arquivo deste computador abre sem verificador;
- cópia de um arquivo confiável, vinda de fora, passa pela verificação;
- arquivo confiável alterado por fora passa pela verificação;
- confiança vive só fora do arquivo;
- arquivo trocado depois da verificação não abre;
- registro de 1 TB recusado antes de qualquer leitura;
- registros demais recusados, e o limite aumentado explicitamente abre o arquivo;
- dados do documento acima do limite recusados, e arquivos grandes não contam;
- bomba de descompressão recusada pelo limite de memória;
- verificador lento encerrado;
- árvore funda abre ou é recusada, nunca derruba;
- índice de arquivo grande hostil é inofensivo.

**A reprodução da coleção do fuzzing** roda no `cargo test` normal.

**Os 2.000 + 2.000 cenários** continuam passando.

## Problemas encontrados e corrigidos nesta parte

1. **A abertura de arquivos confiáveis passou do limite de 3 s** (3,2 a 3,4 s) por causa do índice de escondidos montado inteiro ao abrir, da Parte 4. Ele forçava a Loro a decodificar todos os nós. O índice passou a ser **sob demanda**: abrir volta a 0,11 s, e abrir e ler tudo a 2,47 s. Uma leitura a 10.000 níveis leva 1,2 µs.
2. **Um documento gerado antes da mudança da marca de removido abriu sem reclamar e errado** (151 mil nós visíveis em vez de 101 mil), porque a versão da organização não tinha sido aumentada. Ela passou para 2, e a regra "toda mudança de organização aumenta a versão" ficou no ADR-0021.
3. **Dois erros nos próprios testes:**
   - o documento do teste de limite era pequeno demais para passar o limite;
   - a primeira versão do teste do tempo-limite não fazia nada, porque mudar o ambiente do próprio processo exige `unsafe`. Ela foi trocada por uma opção que vale só para o processo verificador.

## O que fica pendente

- **A primeira rodada de fuzzing de verdade** só acontece no primeiro envio ao GitHub: o libFuzzer não roda no Windows deste PC. Localmente: o pacote compila, as licenças passam, e 300 mil mutações não derrubaram nada.
- **Arquivos de fora levam cerca do dobro do tempo para abrir** (5 s no documento de 1 milhão de operações, contra 2,5 s). O dono decide se isso é aceitável.
- **Gerar uma árvore de 1 milhão de níveis levaria cerca de 3 horas** (criar grupos aninhados pela Loro tem custo quadrático), então foi medida até 100 mil. Verificar e abrir cresce em linha reta.

## Como conferir, no PowerShell, a partir da pasta do projeto

```
cargo test --release --manifest-path poc/crdt-armazenamento/prototipo/Cargo.toml -p bivar-poc-verificador
```

Deve terminar sem falhas, em cerca de 30 s.

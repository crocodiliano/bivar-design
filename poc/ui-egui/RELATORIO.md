# Relatório — prova de conceito da interface (entrega 0.2)

Programa **descartável** para decidir o framework de interface ([ADR-0009](../../docs/adr/0009.md)). A decisão está no [ADR-0017](../../docs/adr/0017.md). O código não vira base do produto.

## O que foi construído

- **Painel "Camadas":** uma lista de camadas com botões de visível e de bloqueada. O estado aparece pela forma (bolinha cheia ou vazia), não só pela cor.
- **Painel "Propriedades do Objeto":** três controles deslizantes, Protusão, Peso e Borrão, cada um com um campo numérico.
- **Canvas:** fundo listrado com o artboard branco. O desenho com caneta usa espessura proporcional à pressão. O mouse também desenha, mas com espessura fixa.
- **Mostrador da caneta:** pressão de 0 a 1, ponta encostada ou no ar, inclinação e borracha, estas duas marcadas como "não disponível".
- **Painéis destacáveis:** cada painel vira uma janela do Windows quando é arrastado para fora ou pelo botão "Destacar". Ele volta ao ser arrastado sobre a janela principal, pelo botão "Acoplar" ou ao fechar a janela.
- **Visual:**
  - fonte Poppins (OFL), com o arquivo da licença em `assets/fonts/OFL.txt`;
  - tema escuro, cantos arredondados e roxo como destaque;
  - contorno de foco grosso;
  - nenhuma animação.
- **Sem `unsafe`:** o compilador recusa qualquer bloco `unsafe` neste crate.

## Como rodar

```
cd poc\ui-egui
cargo run --release
```

## Testes

```
cd poc\ui-egui
cargo test
```

- **30 testes de lógica:**
  - pressão sempre entre 0 e 1;
  - espessura do traço;
  - regras de desacoplar e acoplar;
  - contraste WCAG das cores;
  - tamanho do artboard.
- **8 testes de interface (`tests/ui.rs`, com egui_kittest):**
  - 3 testes visuais com imagens de referência aprovadas pelo dono em `tests/golden/`: a janela a 100%, a janela a 150% e o contorno de foco;
  - nomes e papéis acessíveis de todos os controles, como o NVDA os lê;
  - a tecla Tab alcança todos os controles;
  - Espaço liga e desliga a visibilidade, as setas movem o controle deslizante, Enter destaca o painel e "Acoplar" traz de volta.

**Imagens de referência:** quando falta uma, o teste falha e grava `<nome>.new.png`. Ela só vira referência com a aprovação do dono (CLAUDE.md, seção 6).

## Resultado do teste do dono (2026-10-20)

Na máquina de referência, todos os critérios passaram: estilo, janela separada no 2º monitor com volta ao acoplamento, NVDA, teclado, pressão da caneta e escala de 150%.

## Limitações

Os detalhes estão no ADR-0017.

1. **Inclinação e borracha da caneta não estão disponíveis.** O winit 0.30 as descarta no Windows, e lê-las diretamente exigiria `unsafe`, que esta prova não permite.
2. **O arrasto não continua na janela recém-destacada.** Para movê-la, usa-se a barra de título do Windows.
3. **Movimento reduzido:** as animações estão sempre desligadas. A preferência do Windows não é lida, porque isso exigiria `unsafe`. O produto terá uma opção própria nas configurações do Bivar.
4. **Testes visuais em paralelo derrubam o processo** dentro de uma dependência de GPU. Por isso eles rodam um de cada vez, com uma trava.
   - **Ambiente:** GeForce GTX 1660, driver NVIDIA **617.14**, conferido com `nvidia-smi` em 2026-10-20.
   - **Depois de atualizar o driver para a versão 617.42 (2026-10-20):** sem a trava, ainda houve 7 quedas em 10 rodadas. Com a trava, foram 10 de 10 sem falha. A trava continua necessária.
5. **Exceções de dependência só nesta pasta:** avisos de "sem manutenção" do zbus, usado só no Linux (`deny.toml` e `.cargo/audit.toml` locais).

## Dependências

| Crate | Uso | Licença |
| --- | --- | --- |
| `eframe` 0.37.0 (com `accesskit` e `wgpu`) | Janela, interface e acessibilidade | MIT OR Apache-2.0 |
| `egui_kittest` 0.37.0 (só testes; `snapshot` e `wgpu`) | Testes visuais e de acessibilidade | MIT OR Apache-2.0 |

`cargo deny` e `cargo audit` passam com a configuração local.

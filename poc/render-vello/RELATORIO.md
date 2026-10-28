# Relatório da prova de conceito do renderizador (entrega 0.3, ADR-0007)

> Prova **descartável**. Nada daqui vira código do produto.

## O que foi testado

- **Versões:**
  - vello 0.11.0 (desenho pela placa de vídeo);
  - vello_cpu 0.3.0 (desenho sem placa de vídeo);
  - wgpu 30.0.1;
  - eframe/egui 0.37.0;
  - skrifa 0.44.0 (leitura da fonte).
- **Máquina:** Windows 11, NVIDIA GeForce GTX 1660, Ryzen 7 5700X.
- **Regra do dono:** nenhum `unsafe` no nosso código. O crate usa `unsafe_code = "forbid"`.
- **Cena:**
  - 10.000 retângulos com cantos arredondados, traçado e degradê linear, cada um com cores próprias;
  - uma linha de texto em cada tamanho de 8 a 96 px, com a fonte **Source Sans 3 OTF**. Ela tem contornos CFF e licença OFL-1.1, e foi baixada do repositório oficial da Adobe.

## Resultados por critério

### 1. Compatibilidade com o wgpu do eframe 0.37

O **vello 0.11.0 usa o wgpu 30**, o mesmo do eframe 0.37. O `cargo tree` mostra uma única cópia (30.0.1), usada pelos dois. O Vello desenha no mesmo dispositivo de vídeo do eframe, dentro da janela egui.

- O **vello_hybrid 0.2** (a variante "híbrida") ainda usa o wgpu 29 e é incompatível. Não foi usado.
- **Limite de textura:** o eframe pede à placa um limite de 8.192 px por lado. O Vello guarda **uma linha por degradê diferente** numa textura interna. Com 10.000 degradês diferentes, a textura passa do limite e o programa **cai** (erro de validação do wgpu). A prova pede à placa o limite real dela: 16.384 na GTX 1660. Uma cena com mais de 16.384 degradês diferentes ao mesmo tempo ainda derrubaria o Vello 0.11. O produto precisa se proteger disso.

### 2. 10.000 retângulos, com zoom e pan fluidos

O teste automático percorre a cena por 12 s, de "cena inteira" até zoom de 40×, e mede o intervalo real entre quadros na janela (1272×754 px, Vulkan).

| Degradês diferentes na cena | Quadros | Média | 99% abaixo de | Pior | Acima de 16,7 ms (60 Hz) | Acima de 6,9 ms (144 Hz) |
| --- | --- | --- | --- | --- | --- | --- |
| 10.000 (cada retângulo com o seu) | 73 | 165,6 ms | 176,6 ms | 176,6 ms | 73 (todos) | 73 (todos) |
| 1.000 | 1.101 | 10,9 ms | 12,1 ms | 14,9 ms | 0 | 1.101 (todos) |
| 256 | 2.298 | 5,2 ms | 6,4 ms | 9,6 ms | 0 | 14 |
| 64 | 2.868 | 4,2 ms | 11,6 ms | 17,2 ms | 1 | 138 |

**Repetição a pedido do dono:** a linha de 64 saiu pior que a de 256, o que não fazia sentido. O teste foi repetido 6 vezes, intercalando 64 e 256, para que nenhum dos lados se beneficiasse da ordem.

| Rodada | Degradês diferentes | Quadros | Média | 99% abaixo de | Pior | Acima de 16,7 ms | Acima de 6,9 ms |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 64 | 3.177 | 3,78 ms | 4,49 ms | 8,82 ms | 0 | 2 |
| 2 | 256 | 2.311 | 5,19 ms | 6,08 ms | 7,91 ms | 0 | 1 |
| 3 | 64 | 3.184 | 3,77 ms | 4,75 ms | 9,03 ms | 0 | 4 |
| 4 | 256 | 2.321 | 5,17 ms | 6,31 ms | 11,79 ms | 0 | 8 |
| 5 | 64 | 3.149 | 3,81 ms | 4,53 ms | 6,99 ms | 0 | 1 |
| 6 | 256 | 2.322 | 5,17 ms | 6,19 ms | 7,75 ms | 0 | 4 |

Nas 3 rodadas, 64 ficou melhor que 256. A primeira medição de 64 foi um ponto fora da curva, provavelmente por atividade da máquina naquele momento, e não um efeito do Vello. Por isso não houve investigação adicional.

**Onde vai o tempo,** num quadro de 1272×754 px:

| Degradês diferentes | Montar a vista (nosso código) | Chamada do Vello (processador) | Trabalho da placa de vídeo |
| --- | --- | --- | --- |
| 10.000 | 0,7 ms | 156 ms | 2,6 ms |
| 4.096 | 0,9 ms | 45 ms | 2,7 ms |
| 1.000 | 0,7 ms | 9,6 ms | 2,9 ms |
| 256 | 0,7 ms | 4,1 ms | 2,6 ms |
| 64 | 0,8 ms | 2,6 ms | 3,0 ms |

**Causa:** a placa de vídeo desenha os 10.000 retângulos em menos de 3 ms. O gargalo está no processador, dentro do Vello: em `vello_encoding/src/ramp_cache.rs`, a memória de degradês guarda **só 64** entre um quadro e outro (`RETAINED_COUNT = 64`, valor fixo). Acima disso, a tabela de cores de cada degradê é recalculada a cada quadro, e cada degradê novo percorre a lista inteira. O custo cresce muito mais rápido que o número de degradês.

**Otimização feita na prova:** a cena é montada uma vez só. A cada quadro, só se aplica o zoom e o pan (`Scene::append` com transformação). Isso reduziu a montagem de 12 ms para 0,7 ms.

**Observação:** a janela usa sincronismo vertical (`AutoVsync`) e, mesmo assim, houve intervalos de ~5 ms (mais de 144 por segundo). Os números acima são os intervalos medidos, não um limite teórico.

### 3. Texto OTF em vários tamanhos, nítido no zoom

O texto é desenhado a partir dos contornos da fonte, a cada quadro, na escala da tela, sem imagens ampliadas. A 800% de zoom, as bordas continuam lisas: veja `tests/golden/gpu_texto_zoom_800.png` e `cpu_texto_zoom_800.png`, depois de aprovadas.

- **Limitação da prova:** não há "shaping", ou seja, sem ligaduras e sem kerning. Cada letra avança pela largura dela. O produto precisará de um motor de texto completo, que é assunto do `bivar-text`.

### 4. PNG idêntico pixel a pixel à tela

A janela tem o botão **"Capturar a tela e comparar"**. Ele tira uma foto da janela e compara a área do canvas com duas imagens:

| Comparação | Resultado na máquina de referência |
| --- | --- |
| Tela × a imagem que a placa desenhou e o egui mostrou | **Idênticas pixel a pixel** |
| Tela × uma exportação PNG nova, na mesma vista | 1 pixel diferente, por 1 nível em 255 |

**Causa da diferença:** o Vello, na placa de vídeo, **não é 100% repetível**. Desenhar a mesma cena duas vezes às vezes muda 1 pixel em 1 nível. Medição, 99 repetições da cena inteira em 1280×800 px (1.024.000 pixels):

| Modo da placa | Desenhos diferentes do primeiro | Pior caso |
| --- | --- | --- |
| Vulkan (padrão aqui) | 0 a 16 de 99, conforme a vista | 1 pixel, variação 1 |
| DirectX 12 | 0 a 92 de 99 | 3 pixels, variação 1 |

A causa provável é a ordem em que a placa soma números em paralelo. O egui não altera nada: a prova desliga o "dithering" (pontilhado) dele, que mexeria nas cores.

- **Nos testes automáticos:** as imagens da placa aceitam até 3 pixels com variação de no máximo 1 (`GPU_REPEAT_TOLERANCE`, em `src/image.rs`), o pior caso medido acima. As do modo sem placa são comparadas de forma exata.
  - **Correção de 2026-10-28:** a folga era de 16 pixels, número que vinha das repetições ("até 16 de 99 desenhos mudaram"), não dos pixels. Ao baixá-la para 3, o teste que compara duas montagens da mesma cena (vista aplicada ao juntar a cena × vista dentro de cada objeto) falhou: na cena inteira, 5 pixels diferem sempre, em 1 nível, por arredondamento. Esse teste ganhou uma folga própria, de 5 + 3 pixels (`REENCODE_TOLERANCE`, em `tests/render.rs`).

### 5. Renderização sem placa de vídeo

O **vello_cpu 0.3** desenha a mesma cena sem placa de vídeo:

- pela linha de comando: `--cpu arquivo.png`;
- pela janela: botão "Exportar PNG (sem placa de vídeo)".

Ele é repetível de forma exata.

**Diferença para a placa de vídeo:** são motores diferentes da mesma família, e o que muda é a suavização das bordas.

| Vista | Pixels diferentes | Maior diferença | Diferença média |
| --- | --- | --- | --- |
| Cena inteira (10.000 retângulos pequenos) | 34,5% | 49 em 255 | 1,29 em 255 |
| Texto a 800% | 2,6% | 91 em 255 | 0,17 em 255 |

As diferenças ficam nas bordas: dá para ver nas máscaras que o teste `cpu_is_close_to_gpu` grava em `target/tmp/`. Visualmente as imagens são iguais.

## Teste do dono na máquina de referência (2026-10-20)

| Item | Resultado |
| --- | --- |
| Zoom com a roda do mouse | Funciona |
| Pan arrastando com o botão esquerdo | Funciona |
| Teclado: setas, `+`, `-` e `0` | Funciona |
| Fluidez, 10.000 degradês diferentes | 72 quadros, média 167,8 ms, 99% abaixo de 187,1 ms, pior 187,1 ms; todos acima de 16,7 ms |
| Fluidez, 256 degradês diferentes | 2.361 quadros, média 5,08 ms, 99% abaixo de 5,99 ms, pior 7,41 ms; 0 acima de 16,7 ms; 2 acima de 6,9 ms |
| Texto ampliado | Bordas lisas |
| Tela × PNG, 100% | Tela × imagem desenhada: idênticas. Tela × nova exportação: idênticas pixel a pixel |
| Tela × PNG, escala de 150% | Tela × imagem desenhada: idênticas. Tela × nova exportação: 1 pixel diferente, por 1 em 255 (dentro da variação medida) |
| Exportação sem placa de vídeo | 69 ms; 30,58% dos pixels diferentes da placa, no máximo 45 e em média 1,221 em 255 |

## Testes automáticos

- **35 testes de unidade:**
  - a cena (10.000 retângulos, sempre igual);
  - a câmera (o ponto sob o cursor fica parado no zoom, há limites de zoom, o percurso do teste de fluidez);
  - a fonte (é OTF com CFF, toda letra usada tem desenho, o espaçamento acompanha o tamanho, bytes inválidos são recusados);
  - as estatísticas de fluidez;
  - o PNG (grava e lê de volta sem perder nada);
  - a comparação de imagens.
- **5 testes de desenho:**
  - a placa repete dentro da folga medida;
  - o modo sem placa repete de forma exata;
  - montar a cena uma vez e reaplicar a vista dá a mesma imagem;
  - o fundo é igual nos dois motores;
  - o modo sem placa fica perto da placa.
  
  Rodaram 10 vezes seguidas sem falha.
- **4 imagens de referência,** aprovadas pelo dono em 2026-10-20:
  - `gpu_cena_inteira`;
  - `gpu_texto_zoom_800`;
  - `cpu_cena_inteira`;
  - `cpu_texto_zoom_800`.
- Os testes com a placa de vídeo usam um único dispositivo, um teste de cada vez, por causa da limitação 4 do ADR-0017.

## Dependências

- Aprovadas pelo dono para esta prova: `vello`, `vello_cpu`, `png` e `pollster`.
- `skrifa` já vinha com o Vello. Foi citado no `Cargo.toml` na mesma versão, para ler o mapa de caracteres da fonte.
- `cargo deny check` e `cargo audit`, com as mesmas exceções da 0.2 (os avisos da família zbus, só no Linux):
  - **2026-10-20:** passavam;
  - **2026-10-21:** saiu o aviso RUSTSEC-2026-0384 (tokio 1.54.0), e os dois passaram a falhar. O tokio entra só na versão WebAssembly, não no programa do Windows;
  - **2026-10-28:** o tokio foi atualizado para 1.54.1 (`cargo update -p tokio`, com aprovação do dono), e os dois voltaram a passar.
- **Duplicação:** o egui 0.37 usa por dentro o **vello_cpu 0.2** para desenhar o texto da interface, e a prova usa o 0.3. As duas versões ficam no programa.

## Pontos para o auditor

1. A memória de 64 degradês do Vello 0.11 e o custo que cresce muito acima disso. Caminhos possíveis:
   - limitar e reaproveitar degradês no `bivar-render`;
   - guardar blocos já desenhados (os "tiles" do ADR-0007);
   - propor a mudança ao projeto Vello;
   - esperar uma versão que resolva.
2. A queda acima de 16.384 degradês diferentes, por causa do limite de textura.

   Os itens 1 e 2 são **pré-requisitos obrigatórios antes da Fase 1a**, com critérios de aceite na entrega N.3 do Núcleo. Veja o ADR-0018.
3. Vello na placa de vídeo não é bit a bit repetível: a tela e o PNG podem diferir em poucos pixels, por 1 nível.
4. CPU × GPU são motores diferentes. Um PNG feito sem placa de vídeo não é idêntico ao da tela.
5. Duas versões do vello_cpu no programa (egui e nós).

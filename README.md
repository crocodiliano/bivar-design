# Bivar

> **Feito com auxílio de IA.** · **Made with AI assistance.**

**Português** · [English](#english)

O Bivar é um programa de design open source, escrito em Rust. Ele reúne numa só interface:

- vetor e UI;
- tipografia;
- animação;
- diagramação editorial;
- foto;
- áudio;
- 3D leve.

O projeto está na **Fase 0 (fundações)** e ainda não tem funcionalidades. Veja o plano em [`docs/PLANO.md`](docs/PLANO.md) e as decisões de arquitetura em [`docs/adr/`](docs/adr/).

## Como compilar

1. Instale o Rust pelo [rustup.rs](https://rustup.rs).
2. Rode:

```
git clone <url-do-repositório>
cd bivar
git config core.hooksPath .githooks
cargo test
```

- O comando `git config core.hooksPath .githooks` liga a checagem de assinatura (veja abaixo). Rode-o uma vez em cada clone.
- O comando `cargo test` compila tudo e roda os testes.

## Como contribuir

- **Licença:** toda contribuição é aceita sob a licença do projeto.
- **Assinatura obrigatória:** todo commit precisa da linha `Signed-off-by` (o [DCO](https://developercertificate.org/)). Com ela, você declara que tem o direito de enviar aquele código. Para incluí-la, faça o commit com `git commit -s`.
- **Uso de IA:** contribuições feitas com auxílio de IA são bem-vindas, desde que:
  - o uso seja declarado no corpo da mensagem do commit;
  - uma pessoa tenha revisado o código.
- **Revisão:** o projeto **ainda não passou por auditoria humana**. As decisões e o código foram revisados apenas por IA e por ferramentas automáticas. Revisões da comunidade, sobretudo de quem tem experiência em Rust, são muito bem-vindas: os relatórios ficam em [`docs/auditoria/`](docs/auditoria/).

## Licença

GPL-3.0-or-later. Veja o arquivo [`LICENSE`](LICENSE).

---

## English

Bivar is an open source design application written in Rust. It brings together, in a single interface:

- vector and UI design;
- typography;
- animation;
- editorial layout;
- photo;
- audio;
- light 3D.

The project is in **Phase 0 (foundations)** and has no features yet. See the plan in [`docs/PLANO.md`](docs/PLANO.md) and the architecture decisions in [`docs/adr/`](docs/adr/). Both are written in Portuguese.

### Building

1. Install Rust from [rustup.rs](https://rustup.rs).
2. Run:

```
git clone <repository-url>
cd bivar
git config core.hooksPath .githooks
cargo test
```

- `git config core.hooksPath .githooks` enables the sign-off check (see below). Run it once per clone.
- `cargo test` builds everything and runs the tests.

### Contributing

- **License:** all contributions are accepted under the project license.
- **Sign-off required:** every commit needs a `Signed-off-by` line (the [DCO](https://developercertificate.org/)). It certifies that you have the right to submit the code. To add it, commit with `git commit -s`.
- **AI use:** AI-assisted contributions are welcome, as long as:
  - the AI use is declared in the commit message body;
  - a human has reviewed the code.
- **Review:** the project **has not had a human audit yet**. Decisions and code have been reviewed only by AI and automated tools. Community reviews, especially from experienced Rust developers, are very welcome: reports live in [`docs/auditoria/`](docs/auditoria/).

### License

GPL-3.0-or-later. See [`LICENSE`](LICENSE).

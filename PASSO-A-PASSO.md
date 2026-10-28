# Passo a passo: do zero até a primeira sessão no Claude Code

Siga na ordem. Cada etapa termina com um **teste rápido**: só avance quando ele der certo.

> **Como abrir o terminal do VS Code:** com o VS Code aberto, use o menu **Terminal → Novo Terminal** (ou o atalho **Ctrl + '**). Ele aparece na parte de baixo da janela. É ali que você digita os comandos deste guia e aperta **Enter**.

---

## Parte 1 — Instalar os programas (uma vez só)

### 1. VS Code

1. Acesse **code.visualstudio.com** e baixe a versão para Windows.
2. Na instalação, deixe marcadas as opções **"Adicionar ao PATH"**. Marque também as duas opções **"Abrir com Code"** do menu do Explorer, que ajudam a abrir pastas com o botão direito.
3. Termine a instalação e abra o VS Code.

**Teste:** o VS Code abre normalmente.

### 2. Git

1. Acesse **git-scm.com/downloads/win** e baixe o instalador.
2. Na instalação, aceite tudo como está, com duas exceções:
   - **"Choosing the default editor"** → escolha **"Use Visual Studio Code as Git's default editor"**.
   - **"Adjusting the name of the initial branch"** → escolha **"Override the default branch name"** e deixe escrito **main**.
3. **Feche e abra o VS Code de novo** (para ele "enxergar" o Git).
4. No terminal do VS Code, digite:
   ```
   git --version
   ```

**Teste:** aparece algo como `git version 2.xx`.

### 3. Conta no GitHub

1. Crie uma conta gratuita em **github.com**.
2. **Ative a verificação em duas etapas:** Settings → Password and authentication → Two-factor authentication. É importante, porque o código do projeto vai morar lá.
3. **Proteja o seu e-mail:** Settings → Emails → marque **"Keep my email addresses private"**. Copie o e-mail que aparece, no formato `números+usuario@users.noreply.github.com`. Quando o repositório for público, é esse e-mail que vai aparecer no histórico, e não o seu e-mail real.
4. Volte ao terminal do VS Code e digite, **um comando por vez** (troque pelos seus dados):
   ```
   git config --global user.name "Seu Nome"
   git config --global user.email "o-email-noreply-que-voce-copiou"
   ```

**Teste:** digite `git config --global --list`. O seu nome e o e-mail noreply aparecem na lista.

### 4. Rust

1. Acesse **rustup.rs** e baixe o **rustup-init.exe** (64 bits).
2. Abra o arquivo. Uma janela preta (o terminal) vai aparecer.
3. Se aparecer um aviso dizendo que faltam as ferramentas do Visual Studio (algo como *"Rust requires the Microsoft C++ build tools"*), escolha a opção que **instala o Visual Studio Community automaticamente**. Quando o instalador do Visual Studio abrir:
   - confirme que o item **"Desenvolvimento para desktop com C++"** está marcado;
   - deixe instalar (é grande, pode demorar);
   - volte para a janela do Rust quando terminar.
4. Quando o Rust perguntar o tipo de instalação, escolha **1) Proceed with standard installation** (a opção padrão) e aperte Enter.
5. **Feche e abra o VS Code de novo.**
6. No terminal do VS Code, digite:
   ```
   rustc --version
   cargo --version
   ```

**Teste:** os dois comandos mostram um número de versão.

### 5. Extensão do Claude Code no VS Code

1. No VS Code, aperte **Ctrl + Shift + X** (abre a aba de Extensões).
2. Pesquise **"Claude Code"**, confira que o autor é a **Anthropic** e clique em **Install**.
3. Na barra lateral esquerda aparece um ícone novo, em forma de faísca. Clique nele.
4. Na primeira vez, aparece uma tela de login: clique em **Sign in** e autorize no navegador com a mesma conta do Claude que você já paga.

**Teste:** o painel do Claude Code abre e mostra a caixa para digitar mensagens.

> Se a extensão não aparecer depois de instalar: aperte **Ctrl + Shift + P**, digite **"Developer: Reload Window"** e aperte Enter.

---

## Parte 2 — Montar a pasta do projeto

### 6. Criar a pasta no lugar certo

1. Crie a pasta **`C:\Projetos\bivar`**.
   - **Não use** Documentos, Área de Trabalho ou qualquer pasta sincronizada com OneDrive, Google Drive ou Dropbox. A sincronização atrapalha o Git e a compilação do Rust, que gera milhares de arquivos.
2. Descompacte o zip **bivar-repo-inicial.zip** e copie o **conteúdo** da pasta `bivar` de dentro dele para `C:\Projetos\bivar`.

**Teste:** o arquivo **`C:\Projetos\bivar\CLAUDE.md`** existe. Se você vir `C:\Projetos\bivar\bivar\CLAUDE.md`, a pasta ficou duplicada: mova o conteúdo um nível acima.

### 7. Abrir a pasta no VS Code

1. No VS Code: **Arquivo → Abrir Pasta…** → escolha `C:\Projetos\bivar`.
2. Se o VS Code perguntar se você confia nos autores dos arquivos, clique em **"Sim, eu confio nos autores"**. São os seus arquivos.

**Teste:** na barra lateral esquerda (Explorer) aparecem `CLAUDE.md`, `LEIA-ME-PRIMEIRO.md`, `PASSO-A-PASSO.md` e a pasta `docs`.

---

## Parte 3 — A primeira sessão com o Claude Code

### 8. Ajustar o modo de permissão

1. Abra o painel do Claude Code (ícone da faísca).
2. Na parte de baixo da caixa de mensagem há um indicador do **modo de permissão**. Clique nele e escolha **Manual**.
   - **Manual:** o Claude pede a sua permissão antes de editar arquivos e rodar comandos. **Use este modo no começo**, porque você vê e aprova cada passo.
   - **Plan:** o Claude só planeja e espera a sua aprovação antes de mexer em qualquer coisa. Bom para tarefas grandes.
   - **Evite** "Auto" e "Edit automatically" enquanto você ainda está aprendendo.

### 9. Enviar o primeiro pedido

Copie e cole na caixa do Claude Code:

> Leia o CLAUDE.md e o docs/fases/fase-0.md. Quero começar pela entrega 0.1 (repositório e regras). Antes de qualquer coisa, me apresente o plano em português e espere minha aprovação. Me explique cada comando que for rodar como se eu nunca tivesse programado.

Depois:

1. **Leia o plano.** Se algo não estiver claro, pergunte ali mesmo, por exemplo: "o que é o cargo deny?"
2. **Aprove** quando estiver tudo claro.
3. Quando ele pedir permissão para rodar um comando ou editar um arquivo, leia a explicação e aprove. **Na dúvida, pergunte antes de aprovar.**

### 10. Fechar a entrega

1. Quando o Claude disser que terminou, abra `docs/fases/fase-0.md` e confira cada **critério de aceite** da entrega 0.1. Teste os que você consegue testar.
2. Se tudo passou, peça: **"Faça o commit desta entrega."**
3. Só então passe para a próxima entrega (0.2), de preferência **numa conversa nova** (botão de nova conversa no painel do Claude). Cada entrega numa conversa própria mantém o Claude focado.

---

## Regras de segurança para não errar

- **Nunca** cole senhas, tokens ou códigos de verificação na conversa com o Claude.
- **Se algo der errado:** peça "volte para o último commit" ou "desfaça o que você acabou de fazer". O Git guarda tudo.
- **Se o Claude sugerir instalar uma dependência nova:** pela regra do CLAUDE.md, ele precisa justificar antes. Peça a justificativa se ele não der.
- **Se o Claude quiser mudar algo que contradiz um ADR:** pare e traga a dúvida para a conversa de planejamento (ou peça uma revisão ao agente auditor) antes de aprovar.
- **Se um comando der erro e você não entender:** copie a mensagem de erro inteira e cole para o Claude, pedindo "me explique o que aconteceu antes de tentar consertar".

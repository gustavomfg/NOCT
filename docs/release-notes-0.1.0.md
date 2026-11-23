# NOCT 0.1.0: release notes

[Português do Brasil](#português-do-brasil)

The first release of NOCT rewritten in Rust (Tauri 2 shell, Leptos interface, SQLite). It is a beta for developers who
want to try it on their own projects. The installers are **not signed by the operating system**; README.md, "Install",
says how to check their SHA256 and how to get past the SmartScreen and Gatekeeper warnings.

## What works

- **Projects.** Add a project folder; the sidebar lists the projects, with a favourite, and says when a folder is missing
  or must be chosen again.
- **OpenRouter.** The key is typed once and kept in the operating system's credential store; the interface and the logs
  never see it. Each project asks for permission before anything is sent, and has its own model, chosen from
  OpenRouter's list.
- **Revisar (review).** Ask about the project. The answer streams in and is drawn as Markdown without HTML: links are not
  clickable and images are not loaded. An answer can be cancelled, copied and saved in a conversation.
- **Construir and Documentar (build and docs).** An answer can propose file changes. Each one waits, pending, until you
  approve it; it can be rejected, and an applied change can be undone. Markdown files go to Documentar. A change to Git,
  CI or self-running files, to a device name or to an encoded path is refused; a file that runs code on build or install
  is flagged on its card.
- **Ajustes (settings).** Theme (light, dark), the glass layer, language, approval policy, diagnostics, and the
  OpenRouter key, permissions and chat switch.
- **Safety of the window.** A strict Content-Security-Policy, commands granted to the main window only, navigation
  limited to the app, and in this build no page menu, no reload or print shortcut and no inspector.

## What does not work yet

- OpenRouter is the only provider. Local models (Ollama, LM Studio) and other APIs are not available.
- The assistant has no tools: it reads the top-level text files of the project that the context rules allow, and does
  not run commands or search the tree.
- Saved conversations are not listed in the interface.
- The updater is configured but no update has been published through it; install new versions by hand.
- Rolling back file operations through the native worker works on Linux only.

## Known limitations

- **Unsigned installers.** Windows shows SmartScreen and macOS shows Gatekeeper on the first start.
- **Checked mostly on Linux.** The interface is checked in Chromium on every change, and the app has been used in a real
  window on Linux. No window has been checked on Windows or macOS; those builds are compiled and tested in CI only.
- **Rounded corners on Linux.** Some panes may show jagged corners with WebKitGTK. Ajustes › Aparência › "Bordas sem
  camadas (teste)" is a workaround.
- **Whole-file changes.** A proposed change replaces the whole file and is limited to 32 KiB.
- **Windows file permissions.** The data folder gets no extra ACL on Windows.
- **Data from the previous product.** The Rust app opens the same database schema (32), but provider keys are not
  migrated: type the OpenRouter key again.

---

## Português do Brasil

A primeira versão do NOCT reescrito em Rust (shell Tauri 2, interface Leptos, SQLite). É uma beta para desenvolvedores que
querem experimentar nos próprios projetos. Os instaladores **não são assinados pelo sistema operacional**; o README.pt-BR.md,
"Instalar", explica como conferir o SHA256 e como passar pelos avisos do SmartScreen e do Gatekeeper.

### O que funciona

- **Projetos.** Adicione a pasta de um projeto; a barra lateral lista os projetos, com favorito, e avisa quando uma pasta
  sumiu ou precisa ser escolhida de novo.
- **OpenRouter.** A chave é digitada uma vez e guardada no cofre de credenciais do sistema; a interface e os logs nunca a
  veem. Cada projeto pede permissão antes de enviar qualquer coisa e tem seu próprio modelo, escolhido na lista do
  OpenRouter.
- **Revisar.** Pergunte sobre o projeto. A resposta chega aos poucos e é desenhada como Markdown sem HTML: links não são
  clicáveis e imagens não são carregadas. A resposta pode ser cancelada, copiada e salva numa conversa.
- **Construir e Documentar.** Uma resposta pode propor alterações de arquivos. Cada uma espera, pendente, até você aprovar;
  pode ser rejeitada, e uma alteração aplicada pode ser desfeita. Arquivos Markdown vão para Documentar. Alterações em
  arquivos do Git, do CI ou que rodam sozinhos, em nomes de dispositivo ou em caminhos codificados são recusadas; um
  arquivo que roda código no build ou na instalação é sinalizado no cartão.
- **Ajustes.** Tema (claro, escuro), camada de vidro, idioma, política de aprovação, diagnóstico, e a chave, as permissões
  e o interruptor do chat do OpenRouter.
- **Segurança da janela.** Content-Security-Policy estrita, comandos liberados só para a janela principal, navegação
  limitada ao app e, nesta build, sem menu da página, sem atalho de recarregar ou imprimir e sem inspetor.

### O que ainda não funciona

- O OpenRouter é o único provedor. Modelos locais (Ollama, LM Studio) e outras APIs não estão disponíveis.
- O assistente não tem ferramentas: lê os arquivos de texto do topo do projeto que as regras de contexto permitem, e não
  roda comandos nem busca na árvore.
- As conversas salvas não aparecem listadas na interface.
- O atualizador está configurado, mas nenhuma atualização foi publicada por ele; instale as novas versões à mão.
- O rollback de operações de arquivo pelo worker nativo funciona só no Linux.

### Limitações conhecidas

- **Instaladores sem assinatura.** O Windows mostra o SmartScreen e o macOS mostra o Gatekeeper na primeira abertura.
- **Conferido sobretudo no Linux.** A interface é conferida no Chromium a cada mudança, e o app foi usado em janela real
  no Linux. Nenhuma janela foi conferida no Windows nem no macOS; essas builds são só compiladas e testadas no CI.
- **Cantos arredondados no Linux.** Alguns painéis podem mostrar cantos serrilhados no WebKitGTK. Ajustes › Aparência ›
  "Bordas sem camadas (teste)" contorna o problema.
- **Alterações de arquivo inteiro.** Uma alteração proposta substitui o arquivo inteiro e é limitada a 32 KiB.
- **Permissões de arquivo no Windows.** A pasta de dados não recebe ACL extra no Windows.
- **Dados do produto anterior.** O app em Rust abre o mesmo esquema de banco (32), mas as chaves de provedores não são
  migradas: digite de novo a chave do OpenRouter.

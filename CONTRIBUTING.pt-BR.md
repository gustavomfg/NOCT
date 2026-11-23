# Contribuindo

[English](CONTRIBUTING.md)

## Antes de começar

Antes de abrir um Pull Request:

- procure issues e discussões existentes;
- abra uma issue para mudanças significativas de comportamento ou novas
  funcionalidades;
- mantenha o Pull Request focado em um único objetivo;
- respeite a fronteira entre a interface (`crates/noct-ui`), os comandos do shell
  (`crates/noct-app`) e a persistência (`crates/noct-db`).

# Diretrizes de desenvolvimento

## Arquitetura

O NOCT é escrito em Rust: um shell Tauri (`crates/noct-app`), uma
interface Leptos compilada para WebAssembly (`crates/noct-ui`) e uma camada SQLite
(`crates/noct-db`). Preserve a separação: a interface chama só os comandos que o
shell declara, os comandos conferem seus argumentos e a janela que os chama, e a
persistência é usada só pelos comandos. Não crie uma ponte genérica nem acople
capacidades nativas à interface.

## Segurança

Nunca faça commit de chaves de API, tokens, bancos locais, logs sensíveis ou
credenciais pessoais. A chave do provedor vive só no cofre de credenciais do
sistema: nunca deve chegar à interface, aos logs nem ao repositório. Arquivos de um
projeto são escritos só dentro dele, e só depois de a pessoa aprovar a alteração.
Preserve a validação dos comandos, a autorização de workspace e o modo Revisar
somente leitura. Uma mudança numa fronteira de confiança leva `[security-review]`
no commit e é registrada em `docs/status.md`.

## Qualidade do código

Antes de abrir um Pull Request, execute:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p noct-ui --target wasm32-unknown-unknown -- -D warnings
cargo test --workspace
(cd crates/noct-ui && trunk build --release)
npm ci                              # uma vez: instala o Playwright para as duas checagens abaixo
node scripts/check-shell-ui.mjs     # a interface no Chromium, com um shell simulado
node scripts/check-shell-csp.mjs    # a Content-Security-Policy no Chromium
```

As duas checagens do Chromium leem a build de `crates/noct-ui/dist`, então rode-as depois do `trunk build --release`. O
CI roda os mesmos passos (`.github/workflows/rust-shell-validation.yml`) e, no Linux, também os testes Node dos scripts.
Para usar o app numa janela, rode `node scripts/run-rust-shell.mjs dev --data-dir <pasta temporária vazia>`; nunca aponte
para uma pasta de dados com dados reais.

Descreva no Pull Request os riscos, os comandos executados e qualquer validação
manual.

# Mensagens de commit

Commits Conventional Commits são recomendados, por exemplo:

```text
fix(files): refuse a path that leaves the project
docs(readme): update architecture overview
```

# Pull Requests

Um bom Pull Request explica a mudança, a motivação e os riscos, além dos passos
manuais de validação. Inclua screenshots para mudanças visuais e mantenha
alterações pequenas e revisáveis.

# Ambiente de desenvolvimento

O ambiente usa Rust 1.97.1 (fixado em `rust-toolchain.toml`, com o alvo
`wasm32-unknown-unknown`), trunk 0.22.1, Node.js 24 com npm 11 para as checagens do
Playwright e os testes Node dos scripts, e, no Linux, os pacotes de sistema do Tauri
listados no README (seção 5). Qualquer editor serve: WebStorm (o do mantenedor) ou qualquer um com `rust-analyzer`.

# Filosofia do projeto

Cada contribuição deve respeitar estas prioridades:

1. segurança antes de conveniência;
2. workspace antes da IA;
3. conhecimento antes das conversas;
4. controle humano antes da automação;
5. extensibilidade antes de implementações específicas de Provider.

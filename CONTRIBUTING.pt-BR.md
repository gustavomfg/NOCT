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

O produto deste branch é escrito em Rust: um shell Tauri (`crates/noct-app`), uma
interface Leptos compilada para WebAssembly (`crates/noct-ui`) e uma camada SQLite
(`crates/noct-db`). Preserve a separação: a interface chama só os comandos que o
shell declara, os comandos conferem seus argumentos e a janela que os chama, e a
persistência é usada só pelos comandos. Não crie uma ponte genérica nem acople
capacidades nativas à interface.

## Segurança

Nunca faça commit de chaves de API, tokens, bancos locais, logs sensíveis ou
credenciais pessoais. Dados de autenticação nunca devem ser expostos ao
interface. Preserve a validação dos comandos, a autorização de workspace, Review
somente leitura e escritas de Build limitadas.

## Qualidade do código

Antes de abrir um Pull Request, execute:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast
(cd crates/noct-ui && trunk build --release)
npm ci
npm test
npm run check:csp
npm run check:ui
```

Descreva no Pull Request os riscos, os comandos executados e qualquer validação
manual.

# Mensagens de commit

Commits Conventional Commits são recomendados, por exemplo:

```text
fix(ipc): validate renderer origin
docs(readme): update architecture overview
```

# Pull Requests

Um bom Pull Request explica a mudança, a motivação e os riscos, além dos passos
manuais de validação. Inclua screenshots para mudanças visuais e mantenha
alterações pequenas e revisáveis.

# Ambiente de desenvolvimento

O ambiente recomendado usa WebStorm, Rust 1.97.1 (fixado em `rust-toolchain.toml`,
com o alvo `wasm32-unknown-unknown`), trunk 0.22.1 e Node.js 24 com npm 11 para as
checagens do Playwright e os testes Node dos scripts. O launcher `webstorm` deve
estar no `PATH` quando usado pelos fluxos de desenvolvimento.

# Filosofia do projeto

Cada contribuição deve respeitar estas prioridades:

1. segurança antes de conveniência;
2. workspace antes da IA;
3. conhecimento antes das conversas;
4. controle humano antes da automação;
5. extensibilidade antes de implementações específicas de Provider.

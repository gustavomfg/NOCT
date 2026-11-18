## Resumo

## Motivação

## Validação

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace --no-fail-fast`
- [ ] `trunk build --release` em `crates/noct-ui`
- [ ] `npm test`, `npm run check:ui` e `npm run check:csp`
- [ ] Teste manual relevante

## Segurança e compatibilidade

- [ ] Não inclui credenciais, logs ou dados locais
- [ ] Preserva a fronteira dos comandos do shell e a validação de origem e de payload
- [ ] Documenta mudanças visuais ou de protocolo

## Resumo

## Motivação

## Validação

Marque o que rodou. "Não se aplica" é uma resposta válida: escreva ao lado o motivo (por exemplo, só documentação).

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `trunk build --release` em `crates/noct-ui`
- [ ] `node scripts/check-shell-ui.mjs` e `node scripts/check-shell-csp.mjs` (depois do `trunk build --release`)
- [ ] Teste manual na janela real (sistema: ___ ) ou não se aplica
- [ ] Não se aplica (motivo: ___ )

## Segurança e compatibilidade

Para cada item: marque, ou escreva "não se aplica".

- [ ] Não inclui credenciais, chaves, logs, bancos ou dados locais
- [ ] A chave do provedor continua só no cofre do sistema e não chega à interface nem aos logs
- [ ] Preserva a fronteira dos comandos do shell (lista de comandos, capability, validação do payload, teste de ACL)
- [ ] Escrita em arquivos do projeto só dentro do workspace e só depois da aprovação
- [ ] A Content-Security-Policy continua estrita (sem script nem estilo inline)
- [ ] Muda uma fronteira de confiança: o commit leva `[security-review]` e a revisão está em `docs/status.md`
- [ ] Documenta mudanças visuais, de protocolo ou de dados (migração, formato salvo)

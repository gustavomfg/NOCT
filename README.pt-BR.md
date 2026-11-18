# NOCT: estado da migração para Rust

[English](README.md)

> Este é o branch `migration/rust`. Ele reúne o trabalho que move o shell desktop do NOCT e a persistência local do
> Electron e do TypeScript para Rust. Não é uma versão. O produto estável é o aplicativo Electron na `main` (versão
> v1.0.5), e o README dele está na `main`.

Este arquivo descreve o branch como ele foi conferido em 2026-11-17, contra o código do commit `de7dba4`. Os commits
depois dele mudam só a documentação. Cada afirmação diz o que foi conferido. O que não foi conferido está listado como
não conferido.

## 1. O que é o NOCT e o que a migração faz

O NOCT é um workspace desktop local de engenharia. Ele mantém o estado do projeto de um desenvolvedor e coordena o
desenvolvedor com os provedores de IA que o usuário escolhe. A aprovação e o julgamento ficam com o desenvolvedor. O
aplicativo registra planos, aprovações e resultados de validação (`AGENTS.md`, "Product identity").

A migração substitui três partes do aplicativo Electron:

- o processo principal e o IPC do Electron, por um shell Tauri 2 em Rust (`noct-app`);
- a camada de persistência em TypeScript, por uma camada SQLite em Rust (`noct-db`) que abre o mesmo schema, versão 32;
- o renderer em React, por uma interface Leptos compilada para WebAssembly (`noct-ui`), que reaproveita o CSS da
  interface Electron.

As credenciais dos provedores passam para um cofre em Rust (`noct-secrets`), que guarda os segredos no cofre de
credenciais do sistema operacional. As decisões de arquitetura estão na [ADR 0001](docs/adr/0001-rust-migration.md) e na
[ADR 0002](docs/adr/0002-envelope-provider-credentials.md).

O que este branch ainda não faz:

- não substitui o aplicativo Electron na `main`;
- não produz instalador nem pacote assinado;
- não chama nenhum provedor de IA;
- não mostra conversas na interface;
- nenhuma janela foi conferida no Windows nem no macOS (seção 3).

## 2. Estado atual

### Fases

A coluna Status é o status que a ADR 0001 dá na tabela de fases (depois do commit `e86b24b`). A última coluna diz o que o
repositório mostra, quando isso difere da ADR ou precisa de uma nota. A seção 10 lista os conflitos.

| Fase | Escopo na ADR | Status (ADR 0001) | O que o repositório mostra |
| --- | --- | --- | --- |
| 0. Fundação | Workspace, `noct-native` (Linux), limites em `noct-core`, shell Tauri | **Feita** no branch | A seção de evidências da ADR. O workspace compila no Linux, no Windows e no macOS, no CI. |
| 1. Contratos | `shared/` para `noct-core` | Não revisada nesta atualização | Existem módulos de contrato em `noct-core`, e uma guarda lê `shared/constants.ts`. A ADR não registra conclusão. |
| 2. Persistência | `electron/database` e `persistence` para `noct-db` | Em andamento | Feito: o runtime, as migrações 1 a 32, os repositórios e os itens de fechamento (caminho público de abertura, nove fixtures de linhas de release, catálogo de operações do observador). Aberto: a conferência com uma cópia de dados reais antes de qualquer lançamento público, e o observador ainda não tem pontos de chamada (fase 6). |
| 3. Rollback nativo | Backends de macOS (clone APFS) e Windows | Parcialmente feita, só Linux | `crates/noct-native/src/linux.rs` é o único backend. O worker Rust responde `UNSUPPORTED` no macOS e no Windows. O que falta está listado em [o documento de backends](docs/rust-native-rollback-backends.md). |
| 4. Serviços de workspace | Indexador, controle de mudanças, validação, engenharia | Parcialmente feita | Existem repositórios, na fase 2, para workspaces, change sets, índice do projeto, índice semântico e validação. Os serviços, como o `ProjectIndexService`, não foram portados. |
| 5. IA | `electron/ai`, `security` | Parcialmente feita | Feito: o cofre de credenciais em `noct-secrets`. O seu ciclo de ida e volta com o cofre do sistema operacional passa no CI nos três sistemas. Não portados: os adaptadores de provedores e o loop de ferramentas. Os crates Rust não têm cliente HTTP para provedores. |
| 6. IPC e shell | `electron/ipc`, `main.ts`, `runtime` para Tauri | Parcialmente feita | A primeira fatia está feita: cinco comandos, liberados só para a janela `main`. Conferido numa janela em execução só pela verificação manual de configurações da seção 3. |
| 7. Interface | `src/` para Leptos | Parcialmente feita | A área de configurações, a lista de projetos na barra lateral, a camada de vidro e os temas. Conferido no Chromium com um shell simulado. Não conferido numa janela real. |
| 8. Atualizações e empacotamento | `electron/updates`, `electron-builder`, workflows de release | Não iniciada no lado Rust | Bloqueada. Veja abaixo. |
| 9. Aposentadoria | Remover `electron/`, TypeScript, Vite, npm e o requisito de Node | Não iniciada | Depende da fase 8 e das fases anteriores. |

A fase 0 é a única que a ADR marca como feita. Nas fases 2, 5, 6 e 7, os itens de fechamento ou as primeiras fatias
estão feitos, e o resto de cada fase está aberto.

### O que bloqueia a fase 8

O critério da ADR para a fase 8 é "pacotes assinados validados em três plataformas". No repositório:

- **Nenhum empacotamento está configurado.** `bundle.active` é `false` em `crates/noct-app/tauri.conf.json`.
- **Nenhum atualizador.** Nenhum crate de atualizador está no `Cargo.lock`.
- **Nenhuma configuração de assinatura** existe nos crates Rust nem no `tauri.conf.json`.
- **Nenhum responsável pelas chaves de assinatura.** A decisão aberta 1 da ADR 0001 não foi tomada.
- **Nenhum equivalente dos fuses do Electron nem da integridade ASAR.** A ADR registra isso como uma lacuna que "precisa
  de uma decisão explícita".
- **O shell não rodou numa janela no Windows nem no macOS**, então o critério "validado em três plataformas" ainda não
  pode ser cumprido.
- **Os pisos de plataforma estão decididos: Windows 10 ou posterior, macOS 10.15 ou posterior, e Ubuntu 22.04 ou posterior para Linux.**
  A decisão 4 da ADR 0001 está resolvida para eles. Nenhum job do CI roda em Ubuntu 22.04.
  [`docs/compatibility.pt-BR.md`](docs/compatibility.pt-BR.md#versões-mínimas-de-plataforma) tem a decisão e as
  afirmações do Tauri em que se apoiam. O Tauri não informa a versão do WebKitGTK, e as páginas de macOS dele dão dois
  valores diferentes.

## 3. Verificação

Os resultados abaixo são do commit `de7dba4`, o código que este README descreve. Este README não acrescenta código. As
conferências desta máquina rodaram no Linux. Nada foi rodado à mão no Windows nem no macOS.

### O que o CI cobre

O resultado de um job é `success` quando todas as etapas que rodaram passaram. As etapas só de Linux rodam dentro do job
do Ubuntu, então a tabela lê o resultado do job para elas.

| Verificação | Linux (`ubuntu-latest`) | Windows (`windows-latest`) | macOS (`macos-latest`) |
| --- | --- | --- | --- |
| O workspace compila, e `cargo test --workspace --no-fail-fast` passa (testes ignorados excluídos) | Passou, `de7dba4` | Passou, `de7dba4` | Passou, `de7dba4` |
| O shell de release compila com a interface embutida (o job confere se o nome do arquivo wasm está no binário) | Passou, `de7dba4` | Passou, `de7dba4` | Passou, `de7dba4` |
| Ida e volta no cofre de chaves do sistema, `os_store` (última execução, em `1d89435`) | Passou | Passou | Passou |
| As permissões dos manifestos do Tauri batem com os arquivos commitados | Passou, `de7dba4` | não rodou | não rodou |
| Diferencial de saídas contra o TypeScript, nove linhas de release | Passou, `de7dba4` | não rodou | não rodou |
| Chromium: Content-Security-Policy, e as áreas de configurações e interface (14 cenários, shell simulado) | Passou, `de7dba4` | não rodou | não rodou |
| Fixture de redação de log e amostras de contrato IPC, contra o TypeScript e o zod | Passou, `de7dba4` | não rodou | não rodou |
| Testes do launcher (`node --test scripts/run-rust-shell.test.mjs`) | Passou, `de7dba4` | não rodou | não rodou |
| Row gate: 10 testes contra os bancos dos repositórios TypeScript | Passou, `de7dba4` (`row-gate.yml`) | não rodou | não rodou |
| Política de dependências (`cargo deny check`, `cargo audit`) | Passou, `1d89435` (`rust-audit.yml`, última execução) | não rodou | não rodou |

O ciclo de ida e volta do cofre e a política de dependências são workflows separados. Cada um roda só quando os próprios
caminhos mudam, então a última execução deles é de um commit anterior.

Os testes de estatística acrescentados pelo `c0748e0` (`a_failed_analysis_after_a_large_round_is_logged_and_the_round_still_updates`
e `a_failed_statistics_step_at_close_still_runs_the_optimize_and_the_close_succeeds`) aparecem como `ok` nos logs dos três
jobs do shell do `de7dba4`.

### Vista numa janela real

| Plataforma | O que foi visto | Onde está registrado |
| --- | --- | --- |
| Linux, WebKitGTK | Os modos dev e release foram abertos à mão. Os dois mostraram a área de configurações com os valores padrão, e o `settings_get` rodou de ponta a ponta. Era a interface anterior ao layout atual. | `docs/rust-shell.md`, "Manual check, Linux (WebKitGTK)" |
| Windows, WebView2 | Nada. | nenhum |
| macOS, WKWebView | Nada. | nenhum |

Não visto em nenhuma janela, em nenhuma plataforma:

- a barra lateral com workspaces reais, e o layout atual;
- o tamanho mínimo da janela, como o gerenciador de janelas o aplica;
- a política de navegação numa webview em execução;
- os comandos chamados a partir de uma janela em execução (`settings_get`, `settings_set` para o tema, `workspaces_list`,
  `conversations_list` e `workspace_set_favorite`). A conferência no navegador usa um shell simulado, e os testes em Rust
  usam o runtime de teste do Tauri;
- o custo da camada de vidro, que não foi medido;
- o aviso para uma pasta de dados fora do perfil do usuário, num perfil real do Windows.

### O que o CI não cobre

- Nenhum job abre uma janela. A janela é conferida à mão.
- As conferências no Chromium, o diferencial de saídas e o row gate rodam só no Linux.
- O Windows não aplica ACL de arquivo à pasta de dados (seção 6).
- O workflow de rollback nativo, `native-rollback-validation.yml`, roda na `main` e em `stabilization/native-rollback-v1`.
  Ele não roda neste branch, então o comportamento de rollback no Windows e no macOS não é testado aqui.

## 4. Como rodar

### Requisitos

- Rust 1.97.1, que o `rust-toolchain.toml` fixa, com o alvo `wasm32-unknown-unknown`.
- trunk 0.22.1: `cargo install --locked trunk@0.22.1`.
- Node.js 24 (`>=24.18 <25`) e npm 11, com `npm ci` na raiz do repositório. O Node roda o launcher, a opção de fixture e
  as conferências que comparam com o TypeScript.
- No Linux, os pacotes de sistema que o CI instala para o Tauri: `build-essential`, `libssl-dev`,
  `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libxdo-dev`, `librsvg2-dev` e `libayatana-appindicator3-dev`.
- A opção de fixture precisa das tags de release no clone, então use um clone completo com as tags.

### Comandos

O launcher é `scripts/run-rust-shell.mjs`. As opções são `--data-dir <caminho absoluto>` ou
`--from-fixture <tag de release>`, que não podem ser combinadas, e `--check`.

| Comando | O que faz | Rodado para este README |
| --- | --- | --- |
| `node scripts/run-rust-shell.mjs dev --check` | Mostra o plano do modo de desenvolvimento e confere as ferramentas. Não inicia nada. | Sim. Passou. |
| `node scripts/run-rust-shell.mjs release --check` | O mesmo plano para o modo de release. | Sim. Passou. |
| `node scripts/run-rust-shell.mjs dev --data-dir <pasta temporária vazia> --check` | O mesmo plano com a pasta de dados informada. | Sim. Passou. |
| `node scripts/run-rust-shell.mjs release --from-fixture v1.0.2 --check` | Copia o banco de release v1.0.2 para uma pasta nova em `target/shell-fixtures/`, mostra o caminho e não inicia nada. | Sim. Passou. A cópia fica ali até ser apagada. |
| `cargo build -p noct-app --release --features custom-protocol` | Compila o shell de release com a interface embutida. | Sim. Passou. |
| `node scripts/run-rust-shell.mjs dev` | Inicia o `trunk serve` na porta 1420 e roda `cargo run -p noct-app`. Isso abre a janela. | Não rodou. As conferências deste README não abrem janelas. O mesmo modo foi aberto à mão no Linux (seção 3). |
| `node scripts/run-rust-shell.mjs release` | Roda o `trunk build --release` e o build de release, e então abre a janela. | Não rodou, pelo mesmo motivo. A etapa de build foi rodada como mostrado acima. |
| `node scripts/run-rust-shell.mjs dev\|release --data-dir <caminho absoluto>` | Abre o shell nessa pasta, que é definida como `NOCT_DATA_DIR`. | Não rodou. Abre uma janela. |
| `node scripts/run-rust-shell.mjs dev\|release --from-fixture <tag>` | Abre o shell numa cópia de um banco de release fixado. As tags são as nove de `scripts/fixtures/release-lines.json`. | Não rodou. Abre uma janela. A etapa de cópia rodou com `--check`. |

> **Nunca aponte o shell para uma pasta de dados que contenha dados reais.** Sem `--data-dir`, o shell usa a pasta de
> dados padrão dele, e é ali que ficam os dados de um usuário real do shell Rust. Para testes, use `--data-dir` com uma
> pasta vazia criada para o teste e apagada depois. Ou use `--from-fixture`: ele copia um banco para dentro de `target/` e
> nunca abre o original. No Windows, uma pasta de dados fora do perfil do usuário é registrada no log como aviso, e o
> shell a abre mesmo assim.

## 5. Arquitetura por crate

| Crate | Responsabilidade |
| --- | --- |
| `noct-app` | O shell Tauri 2. Abre a pasta de dados e o log, registra os cinco comandos, define a política da janela e confere o acesso aos workspaces. |
| `noct-core` | Os contratos, limites e tipos que espelham o `shared/`, e a redação do log. |
| `noct-db` | Persistência SQLite: o runtime, as migrações 1 a 32, os repositórios, backups e pontos de recuperação, escritas atômicas, o log de arquivo e o observador de operações. |
| `noct-native` | Um binário separado, um worker de rollback para operações de sistema de arquivos. Tem só um backend, para Linux. |
| `noct-secrets` | O cofre de credenciais dos provedores: um documento cifrado, um índice sem segredos e uma chave-mestra no cofre de credenciais do sistema operacional. O shell ainda não o abre. |
| `noct-ui` | A interface Leptos, compilada para WebAssembly pelo Trunk. As fontes Geist estão vendorizadas sob a SIL Open Font License 1.1. |

## 6. Modelo de segurança

### Em vigor

- **Política da webview.** `withGlobalTauri` é `false`. A Content-Security-Policy é
  `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; img-src 'self' data: blob:; connect-src 'self' ipc: http://ipc.localhost`.
  O Chromium confere essa política (`scripts/check-shell-csp.mjs`). Nenhuma webview a aplicou ainda.
- **Comandos.** Cinco comandos, `settings_get`, `settings_set`, `workspaces_list`, `conversations_list` e
  `workspace_set_favorite`, são declarados em
  `crates/noct-app/build.rs`. `crates/noct-app/capabilities/default.json` os libera só para a janela `main`. Os testes com
  o runtime simulado do Tauri recusam um comando que não foi liberado e um pedido vindo de uma segunda janela.
- **Navegação.** A janela principal aceita navegação só para a interface empacotada e, nos builds de depuração, para o
  servidor de desenvolvimento (`crates/noct-app/src/navigation.rs`, com os testes do próprio arquivo). Não verificado
  numa webview em execução.
- **Permissões do navegador.** A webview não concede nada além de `core:default` (ADR 0001, tabela de segurança). Não
  verificado numa webview em execução.
- **Credenciais.** O banco guarda só uma referência UUID para cada configuração de provedor. O cofre cifra as credenciais
  com XChaCha20-Poly1305 e guarda a chave-mestra no cofre de credenciais do sistema operacional (ADR 0002). O shell não
  abre o cofre, então nenhuma credencial chega à interface.
- **Logs.** As linhas são redigidas antes de entrar na fila do log de arquivo (`crates/noct-core/src/log_redaction.rs`).
  A fixture de redação é comparada com o TypeScript no Linux, no CI.
- **Arquivos locais.** No Unix, o banco, os backups, os logs e as escritas atômicas são criados com modo `0600`, e as
  pastas com `0700`. No Windows, nenhuma ACL é aplicada (veja a lista abaixo).
- **Workspaces restaurados.** Um backup restaurado grava `authorized` como `0`. O workspace precisa ser selecionado de
  novo antes que o shell confie nele (`crates/noct-db/src/repositories/backup.rs`).

### Ainda faltando

- **Allowlist de provedores.** Nenhuma está implementada. O repositório tem um desenho, [`docs/provider-allowlist-design.md`](docs/provider-allowlist-design.md),
  com seis perguntas em aberto e sem código. Ele registra uma lacuna do produto Electron: um endpoint de fonte local aceita
  HTTPS para qualquer host, inclusive endereços privados, e pula a conferência do endereço resolvido que as fontes remotas
  fazem. As regras do produto Electron são estas: provedores remotos de Chat Completions precisam usar HTTPS, recusam
  redirecionamentos e conferem cada endereço resolvido antes de conectar; HTTP sem TLS só é permitido para serviços de
  loopback (`docs/security.md`). O ambiente dos processos filhos do Electron tem uma allowlist própria
  (`electron/runtime/childEnvironment.ts`). O branch Rust não tem cliente de provedores, então nenhuma dessas regras foi
  portada. Qual política se aplica é uma decisão do responsável pelo projeto.
- **ACL no Windows.** A pasta de dados não recebe DACL. Para definir uma, é preciso `windows-sys` como dependência direta,
  que o responsável ainda não aprovou. A decisão foi esperar a integração do cofre, com o responsável presente. A análise
  está em `docs/rust-windows-file-permissions.md`.
- **Fuses do Electron e integridade ASAR.** O Tauri não tem equivalente. A ADR 0001 marca isso como lacuna que precisa de
  decisão.
- **Conferência de acesso ao workspace.** Ela aproxima `access(R_OK | X_OK)` sem uma chamada nativa. Os testes não cobrem
  ACLs, outros usuários nem acesso por grupo (`docs/rust-shell.md`, "Known approximations").
- **Dependências do npm.** `npm audit --omit=dev` não reporta vulnerabilidades. A árvore completa tem 13 achados moderados,
  todos de um único advisory, GHSA-g3hv-75wg-m249 (`minimatch`), em ferramentas de desenvolvimento. Nenhum está no caminho
  do build Rust. O `npm audit fix` não foi aplicado, porque a saída dele instala `electron-builder@25.1.0`, que ele chama de
  mudança incompatível. As opções estão em [`docs/npm-audit-triage.md`](docs/npm-audit-triage.md). Nenhuma foi decidida.

## 7. Compatibilidade e rollback

- **Schema.** A versão do schema é 32 (`shared/constants.ts`, `crates/noct-core/src/limits.rs`), e um teste de paridade
  confere os dois. Este branch não acrescenta migração depois da 32. A última migração em
  `crates/noct-db/src/migrations/generated.rs` é a 32, como nas migrações TypeScript.
- **Bancos já na versão 32.** Abrem sem mudança de schema. O row gate confere as linhas que o código Rust lê e escreve
  contra os repositórios TypeScript.
- **Bancos antigos.** As nove linhas de release fixadas vão da 0.5.0-beta à 1.0.2. Quando o runtime Rust abre um banco
  antigo, ele grava primeiro um backup, mantém até três, e então roda as migrações dentro de transações. O row gate e o
  diferencial de saídas comparam o resultado com o TypeScript para cada linha.
- **Bancos mais novos.** O runtime recusa um schema mais novo que 32 antes de alterar qualquer coisa.
- **Rollback para o aplicativo Electron.** As duas implementações usam o schema 32, então um arquivo escrito por uma deve
  abrir na outra. Este branch não testa isso. O row gate lê arquivos que o TypeScript escreveu, e compara as escritas do
  Rust com os resultados esperados do TypeScript. A outra direção também é conferida: `scripts/verify-rust-written-gate.mjs`
  faz o Rust escrever um banco, o TypeScript lê e escreve nele, e o Rust lê de volta, comparando as linhas lógicas a cada
  etapa. Ele roda com o row gate no CI. Cobre espaços de trabalho, configurações e conversas, não as linhas de release,
  o índice semântico nem as tabelas de engenharia. Os acessos do TypeScript também escrevem no banco (`PRAGMA optimize` e
  checkpoints do WAL), então a comparação usa linhas lógicas, não bytes do arquivo.
- **Rollback no macOS e no Windows.** O worker Rust responde `UNSUPPORTED` nessas plataformas. O worker C++ que o produto
  Electron usa tem backends para as duas. O que falta está listado em
  [o documento de backends](docs/rust-native-rollback-backends.md).
- **Credenciais dos provedores não são migradas.** A decisão 6 da ADR 0001 propõe que o usuário digite de novo as chaves
  dos provedores uma vez, depois da migração. Isso está pendente de confirmação.
- **Não é o mesmo rollback.** O `noct-native` faz rollback de operações de sistema de arquivos. Ele não faz rollback do
  banco.

## 8. Testes e CI

### Rodado para este README (Linux, commit `de7dba4`)

| Comando | Resultado |
| --- | --- |
| `cargo fmt --all -- --check` | Passou |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passou |
| `cargo clippy -p noct-ui --target wasm32-unknown-unknown -- -D warnings` | Passou |
| `cargo test --workspace --no-fail-fast` | 460 passaram, 0 falharam, 19 ignorados |
| `cargo tree -p noct-app -e normal -i rusqlite -f '{p} features={f}'`, e `cargo tree --workspace -e normal -f '{p} {f}' \| grep -c hooks` | `rusqlite` 0.42.0 sem `hooks`. O grep conta 0. A feature `hooks` é dev-dependency do `noct-db` (commit `c0748e0`), então não entra no grafo de release. |
| `node --test scripts/run-rust-shell.test.mjs` | 7 passaram |
| `node scripts/verify-row-gate.mjs` | 10 passaram, depois `scripts/verify-rust-written-gate.mjs` passou, saída 0 |
| `node scripts/generate-output-differential.mjs`, e depois `cargo test -p noct-app --test output_differential -- --ignored` | 1 passou |
| `trunk build --release` em `crates/noct-ui` | Passou |
| `cargo build -p noct-app --release --features custom-protocol` | Passou. O binário embute o arquivo wasm. |
| `node scripts/check-shell-csp.mjs` | Passou |
| `node scripts/check-shell-ui.mjs` | 17 de 17 cenários passaram |
| `node scripts/generate-log-redaction-fixture.mjs --check` | Passou |
| `node scripts/generate-ipc-samples.mjs --check` | Passou |
| `cargo deny check` | Passou (advisories, bans, licenses, sources) |
| `cargo audit` | Nenhuma vulnerabilidade. Seis avisos, todos listados com motivo em `deny.toml`: cinco crates sem manutenção e um crate unsound, `glib` (RUSTSEC-2024-0429) |
| `npm ci` | Instalado |
| `npm audit --omit=dev` | `found 0 vulnerabilities`. A árvore completa tem 13 achados moderados. Veja [a triagem](docs/npm-audit-triage.md). |

Não rodados aqui, com o motivo:

- `cargo test -p noct-secrets --test os_store -- --ignored --nocapture`. Ele grava uma entrada de teste no cofre de
  credenciais do sistema operacional da máquina onde roda. Usa um nome de serviço próprio e não toca na chave-mestra de um
  aplicativo instalado (ADR 0002). O CI o roda em cada sistema.
- Os benchmarks do índice semântico e a medição de estatísticas semânticas, que são testes ignorados (listados abaixo).
  São medições, sem orçamento definido, e não fazem parte de nenhuma barreira.

### Testes ignorados

| Teste | Quantidade | Como roda | Por que está ignorado | Rodado aqui? |
| --- | --- | --- | --- | --- |
| `crates/noct-db/tests/row_gate.rs` | 10 | `node scripts/verify-row-gate.mjs` | Precisa de `NOCT_ROW_GATE_DIR`, que o script define | Sim, 10 passaram |
| `crates/noct-db/tests/rust_written_gate.rs` | 2 | `node scripts/verify-rust-written-gate.mjs`, que o `verify-row-gate.mjs` também roda | Precisa de `NOCT_RUST_WRITTEN_DIR`, que o script define | Sim, passaram |
| `crates/noct-app/tests/output_differential.rs` | 1 | `node scripts/generate-output-differential.mjs`, e depois o teste com `--ignored` | Precisa das bases das linhas de release | Sim, passou |
| `crates/noct-secrets/tests/os_store.rs` | 1 | `cargo test -p noct-secrets --test os_store -- --ignored --nocapture` | Precisa do cofre de credenciais do sistema operacional | Não, veja acima |
| Benchmarks em `crates/noct-db/src/repositories/semantic_index.rs` | 4 | Modo release, `--ignored --nocapture` | São medições | Não |
| `crates/noct-db/tests/semantic_statistics.rs` | 1 | Modo release, `--ignored --nocapture` | É uma medição | Não |

### Workflows em `migration/rust`

| Workflow | Roda em push para `migration/rust` | Runners | O que roda |
| --- | --- | --- | --- |
| `rust-shell-validation.yml` | Sim, e por disparo manual. Não tem gatilho de pull request. | `ubuntu-latest`, `windows-latest`, `macos-latest` | Build, testes do workspace, build de release com a interface embutida. As etapas só de Linux estão listadas abaixo. |
| `row-gate.yml` | Sim. Também em pull requests para a `main`, e por disparo manual. | `ubuntu-latest` | O row gate. Não é uma verificação obrigatória: torná-la obrigatória é uma configuração do repositório. |
| `rust-audit.yml` | Sim, quando os próprios caminhos mudam. Também em pull requests, em agenda e por disparo manual. | `ubuntu-24.04` | `cargo deny check` e `cargo audit`. |
| `keyring-round-trip.yml` | Sim, quando mudam `crates/noct-secrets/**`, `Cargo.toml`, `Cargo.lock` ou o próprio workflow. | `ubuntu-24.04`, `windows-latest`, `macos-latest` | Os testes unitários, e a ida e volta de `os_store` com o cofre real em cada sistema. |

Os outros workflows não rodam em push para este branch. `package-validation.yml` (chamado "CI · source, renderer and
packages") e `dependency-security.yml` rodam em push para a `main`, em pull requests e por disparo manual.
`native-rollback-validation.yml` roda em push para a `main` e para `stabilization/native-rollback-v1`. `stable-release.yml`
roda em tags de versão. `updater-rehearsal.yml` e `packaged-recovery.yml` rodam só por disparo manual.

### Etapas que rodam só no Linux

Em `rust-shell-validation.yml`, estas etapas têm `if: matrix.os == 'ubuntu-latest'`:

- os pacotes de Linux para o Tauri, o Node 24, o `npm ci`, e as bases das linhas de release e as saídas TypeScript do
  diferencial;
- a regeneração das permissões do Tauri, que falha se houver qualquer diferença dos arquivos commitados;
- o diferencial de saídas contra o TypeScript;
- a instalação do Chromium, e as conferências no Chromium da Content-Security-Policy e da interface;
- a fixture de redação de log e as amostras de contrato IPC;
- os testes do launcher.

Os jobs de Windows e macOS rodam o build, os testes do workspace e o build de release com a interface embutida.

## 9. Assinatura de código e o que isso significa para instaladores

- Este branch não tem assinatura de código: nem certificado, nem notarização, nem artefato de atualização assinado.
- `bundle.active` é `false`, então este branch não produz instalador.
- Um binário compilado a partir deste branch é não assinado. Windows e macOS não têm uma identidade de publicador para
  conferir nele. A única forma de verificar a origem é compilar a partir do commit por conta própria.
- O produto Electron na `main` tem política própria. O README dele diz que os checksums do Linux são assinados com GPG e
  que os pacotes de Windows e macOS são publicados sem assinatura de plataforma nem notarização. Essa política não se
  aplica a este branch ainda, e a escolha de quem guarda as chaves de assinatura está aberta (ADR 0001, decisão aberta 1).

## 10. Índice da documentação e próximos passos

### Documentos da migração

- [ADR 0001: migração para Rust](docs/adr/0001-rust-migration.md). Fases, progresso, invariantes de segurança e decisões
  abertas. O status é "Proposed".
- [ADR 0002: cifragem por envelope das credenciais dos provedores](docs/adr/0002-envelope-provider-credentials.md).
  Accepted. Substitui em parte a decisão 6 da ADR 0001.
- [Rodando o shell Rust localmente](docs/rust-shell.md). Os modos de execução, o que foi conferido, a verificação manual,
  as lacunas do CI e as aproximações conhecidas.
- [Versões mínimas de plataforma](docs/compatibility.pt-BR.md#versões-mínimas-de-plataforma). Os pisos decididos, as páginas do
  Tauri que eles citam, e o que não está confirmado.
- [Row gate no CI](docs/rust-row-gate-workflow.md). O workflow, as decisões, o teste em que o TypeScript lê e escreve o que o
  Rust escreveu, e o que não foi conferido.
- [Backends de rollback em Rust](docs/rust-native-rollback-backends.md). O que os backends de macOS e Windows precisam
  reproduzir, e o que falta.
- [Permissões de arquivo no Windows](docs/rust-windows-file-permissions.md). A análise, o aviso, e a ACL que não está
  implementada.
- [Desenho da allowlist de provedores](docs/provider-allowlist-design.md). Uma proposta, com perguntas em aberto. Sem código.
- [Triagem do npm audit](docs/npm-audit-triage.md). Os achados, o escopo, e as opções. Nada foi corrigido.
- [Índice da documentação](docs/README.md). Os outros documentos. Os guias de usuário dele descrevem o produto Electron.
  Por exemplo, `docs/security.md` descreve o IPC do Electron.
- [`AGENTS.md`](AGENTS.md). As regras para quem contribui e os comandos de validação.

### Próximos passos

1. **Responsável pelo projeto.** Decidir a allowlist de provedores, usando o desenho em
   `docs/provider-allowlist-design.md`, ou confirmar a política de provedores do Electron como regra para o Rust (seção 6).
2. **Responsável pelo projeto, com ele presente.** Decidir entre `windows-sys` ou declarações no próprio repositório, e
   sobre a ACL do Windows, durante a integração do cofre (seção 6 e `docs/rust-native-rollback-backends.md`).
3. **Janelas reais.** Abrir o shell no Windows (WebView2) e no macOS (WKWebView). No Linux, conferir o layout atual e a
   barra lateral com workspaces reais, usando `--from-fixture`, e conferir o tamanho mínimo da janela.
4. **Fase 2.** Ligar o observador de operações aos pontos de chamada da fase 6. Antes de qualquer lançamento público,
   rodar a comparação entre TypeScript e Rust numa cópia de dados reais, mantida em `target/` e nunca commitada.
5. **Fase 3.** Escrever os backends de rollback para macOS (clone APFS) e Windows, ou decidir que o worker C++ continua
   cuidando dessas plataformas. Corrigir o comentário sobre o `#else` em `crates/noct-native/src/main.rs`. Os detalhes estão
   em `docs/rust-native-rollback-backends.md`.
6. **Fase 5.** Portar os adaptadores de provedores e o loop de ferramentas.
7. **Decisões da fase 8.** Quem guarda as chaves de assinatura. Como substituir os fuses do Electron e a integridade ASAR.
   As versões mínimas das plataformas, que estão decididas em `docs/compatibility.md`; a configuração do bundle para macOS ainda será escrita.
8. **Configuração do repositório, responsável pelo projeto.** Decidir se o row gate vira verificação obrigatória na
   `main`. Ele já passou neste branch.
9. **Responsável pelo projeto.** Decidir a opção de npm audit (`docs/npm-audit-triage.md`).

### Conflitos entre as fontes

- A ADR 0001 dizia "não iniciadas: fases 3 a 9" e não tinha coluna de status. O commit `e86b24b` acrescentou a coluna de
  status, e este README a usa. A ADR 0001 dá à fase 1 o status "Não revisada nesta atualização", e este README não a
  revisa de novo.
- A ADR 0001 listava "ligação do gate no CI" como pendente na fase 2. A ADR agora diz que ela está feita em
  `row-gate.yml`, embora o job não seja obrigatório.
- A ADR 0001 está "Proposed", enquanto a ADR 0002 está "Accepted" e a substitui em parte.
- A decisão 4 da ADR 0001 pedia as versões mínimas das plataformas em `docs/compatibility.md`. O arquivo tem os valores
  decididos e as fontes deles, e o texto da ADR agora registra a decisão (Windows 10 ou posterior, macOS 10.15 ou
  posterior, Ubuntu 22.04 ou posterior). A versão do WebKitGTK no Ubuntu 22.04 e a configuração do macOS no pacote
  continuam em aberto.
- Nenhuma allowlist de provedores está implementada. `docs/provider-allowlist-design.md` é só um desenho (seção 6).
- A ADR 0002 agora também registra texto sobre estatísticas e versão do schema (commit `c0748e0`) que não trata de
  credenciais. A descrição do cofre não mudou.

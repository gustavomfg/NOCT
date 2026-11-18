# Chat com o provedor (beta 1)

Este documento descreve o chat da seção **Revisar** que pergunta sobre um projeto ao OpenRouter. O chat vem **ligado**
desde que o dono testou a chave na janela real (2026-11-18), e pode ser desligado em **Ajustes**.

## Estado

- Padrão: `PROVIDER_CHAT_DEFAULT = true` em `crates/noct-app/src/provider_chat.rs`. A escolha da pessoa fica na chave
  `providerChatEnabled` da tabela `settings` (`"true"` ou `"false"`); sem ela, vale o padrão.
- `provider_chat_status` lê o interruptor; `provider_chat_set_enabled` (`{ enabled }`) troca e responde com o valor salvo.
- Com o interruptor desligado, `provider_chat_send` recusa com `chat-disabled` antes de olhar o projeto, e a linha sobre a
  caixa de pergunta diz "O chat está desligado. Nada é enviado." com o botão **Ligar**.
- A caixa de pergunta aceita texto sempre que há um projeto. O envio só fica disponível com chave, modelo e chat ligado; a
  linha sobre a caixa diz o que falta (chave, modelo, permissão) e oferece o botão que resolve.
- A asserção de compilação em `crates/noct-app/tests/provider_chat_flow.rs` agora fixa o padrão ligado.

## O que uma pergunta envia

Uma pergunta só é enviada se todas as verificações passarem, nesta ordem:

1. o interruptor está ligado;
2. a pergunta tem texto e até 8000 caracteres;
3. o projeto está salvo, disponível e **autorizado** (um projeto restaurado precisa ser escolhido de novo);
4. o projeto tem **consentimento** para o OpenRouter na versão atual do texto de consentimento;
5. o projeto tem um **modelo padrão** do OpenRouter;
6. há uma **chave** salva no cofre do sistema.

Só então o projeto é lido e a chamada é feita. O envio contém:

- a pergunta;
- o nome do modelo padrão;
- os arquivos de texto do **topo** do projeto que `noct_core::ai::context::select` aceita: pastas ignoradas, nomes
  sensíveis, arquivos acima de 32 KiB e o total acima de 128 KiB ficam de fora; só UTF-8 é lido; subpastas não são lidas;
  links simbólicos não são seguidos.

A chave é lida no processo principal e vai no cabeçalho `Authorization` da chamada. Ela não entra no corpo, nos eventos
enviados à interface, nos erros nem nos logs.

## Somente leitura

O chat não executa comandos e não escreve arquivos do projeto. A resposta é mostrada na tela. Ela só é guardada quando a
pessoa clica em **Salvar resposta**, que cria (ou reaproveita, por projeto) uma conversa com `conversations_create` e
grava o texto da resposta com `ai_save_assistant`. A pergunta não é guardada por esse caminho.

## Comandos

| Comando | Entrada | Resposta |
| --- | --- | --- |
| `provider_chat_status` | nenhuma | `{ enabled }`; `false` nesta versão |
| `provider_chat_send` | `{ input: { workspace, question }, onEvent }` | nada, quando a chamada começou; os eventos chegam no canal |
| `provider_chat_cancel` | nenhuma | nada |

Os eventos do canal são `delta` (com `text`), `done` e `failed` (com `code`). Uma chamada termina com exatamente um
`done` ou um `failed`, salvo cancelamento. Uma resposta cancelada não envia mais eventos. Só uma pergunta roda por vez; uma
segunda recebe `chat-busy`.

## Limites e falhas

- Pergunta: 8000 caracteres.
- Evento do fluxo: 64 KiB. Resposta completa: 64 KiB de caracteres.
- Tempo: conexão, leitura e total com limites; um fluxo parado por 30 segundos é interrompido.
- Redirecionamentos são recusados, e o endereço é fixado aos endereços aprovados pela política de rede.
- Sem proxy do sistema (`no_proxy`); o atualizador habilita proxy no workspace, e o chat não herda isso.
- Falhas de texto fixo: `provider-unauthorized`, `provider-rate-limited`, `provider-unavailable`, `timeout`,
  `stream-truncated`, `stream-malformed`, `response-too-large`, `provider-error`, `provider-rejected`, `transport`. Nenhum
  texto do provedor é mostrado; cada falha diz que nada foi salvo.

## Interface

A seção fica em **Revisar**, abaixo dos achados de exemplo. A resposta chega em ordem mesmo quando os eventos chegam fora
de ordem (a interface reordena pelo índice). Cancelar mantém o texto já recebido, não o salva, e ignora o que chegar depois.

## Testes

- `crates/noct-app/tests/provider_chat_flow.rs`: ordem das verificações, interruptor, contexto (sensíveis, subpastas,
  binários e limite de tamanho), corpo sem chave, streaming, truncamento, JSON inválido, redirecionamento, IP privado, erro
  sem chave, cancelamento e exclusão mútua.
- `crates/noct-app/tests/provider_transport.rs`: transporte contra servidor de loopback (19 testes).
- `crates/noct-ui/src/chat.rs`: sessão de uma pergunta, execuções antigas, cancelamento e salvamento (testes nativos).
- `scripts/check-shell-ui.mjs`: cenário `chat-off-by-default` e cinco cenários com o canal simulado (ordem, salvamento,
  cancelamento, recusa e falha).

**Não verificado:** uma chamada real ao OpenRouter. Todos os testes usam servidor de loopback. Também não foi verificado no
Windows nem no macOS.

## O que ainda falta verificar

1. Uma chamada real com chave de teste em Windows e macOS (o dono testou a chave no Linux).

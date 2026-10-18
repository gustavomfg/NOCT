# Providers e modelos

[English](providers.md)

## Escolhendo a IA

O Nocturne não depende de uma IA específica. Em Configurações → IA, conecte um
serviço, escolha um modelo para o workspace e a seleção passa a ser o motor de
Review, Build e Docs. O indicador abaixo da caixa de mensagem mostra o serviço e
o modelo ativos e abre a mesma página. Sem seleção, a execução é recusada com
uma mensagem clara em vez de recorrer a outra IA.

## Conexões OpenAI-compatible

Os presets cobrem OpenAI, Google Gemini, OpenRouter, DeepSeek, Groq, Mistral,
xAI, Ollama e LM Studio. **Outro** aceita qualquer endpoint que implemente os
recursos compatíveis `/models` e `/chat/completions` (vLLM, llama.cpp,
LiteLLM…); a chave de API é opcional e uma URL de loopback é tratada como
local. Endpoints remotos exigem HTTPS. HTTP simples é aceito apenas para
Providers locais em loopback.

O adapter oferece descoberta de modelos, streaming, cancelamento, function
calling e o recurso OpenAI-compatible `/embeddings` quando o modelo selecionado
declara a capacidade `embeddings`. Catálogos que não declaram capacidades são
tratados como "não declarado", não como "sem suporte". Use o botão de modelo de
uma conexão para atualizar o catálogo e trocar de modelo. Modelos de embeddings
são associados separadamente do modelo de conversa.

Embeddings remotos exigem consentimento explícito do workspace. Sem modelo de
embeddings, sem consentimento ou diante de falha do adapter, o índice semântico
continua disponível por recuperação lexical e estrutural local.

## Ferramentas do agente

Execuções com Provider usam um ciclo limitado de ferramentas executado no
processo principal:

| Ferramenta | Modos | Observações |
| --- | --- | --- |
| `list_directory`, `read_file`, `search_text` | Review, Build, Docs | Caminhos limitados à raiz autorizada; diretórios gerados e prováveis segredos são ignorados ou recusados. |
| `write_file`, `edit_file` | Build | Escrita atômica; `.git`, `.nocturne` e diretórios ignorados são recusados. Aprovação por alteração opcional. |
| `run_command` | Build | Sem shell, timeout de 120 s, saída limitada, aprovação explícita sempre; elevação de privilégio e comandos destrutivos são bloqueados. |

Build exige um modelo com function calling; se um Provider recusar uma
solicitação de Build que ofereceu ferramentas, o erro informa isso. Review e
Docs repetem uma vez sem ferramentas e respondem com o contexto selecionado
pelo Nocturne. A saída das ferramentas é dado não confiável e é enviada ao
Provider escolhido como qualquer outro contexto.

## Conta ChatGPT (opcional)

Uma conta ChatGPT pode ser conectada por um Codex CLI instalado. O Nocturne não
recebe senha ou token da conta: usa o App Server autenticado do Codex e lista os
modelos retornados para aquela conta. Escolher um modelo Codex torna o Codex o
motor de todos os modos até que outro modelo seja escolhido. Uma assinatura
ChatGPT não fornece créditos da OpenAI Platform API.

## Credenciais e diagnóstico

Chaves de Provider são cifradas pelo armazenamento seguro do sistema
operacional. No SQLite existem apenas referências opacas; elas nunca retornam
ao renderer e ficam fora de backups e relatórios de diagnóstico.

O painel de diagnóstico diferencia endpoint indisponível, credencial recusada,
modelo ausente, timeout, rate limit, resposta inválida e créditos insuficientes.
Uma falha de Provider é recuperável e não significa que o processo principal
falhou.

## Casos comuns

- **Créditos insuficientes:** adicione saldo na conta da API ou escolha outro
  Provider; plano ChatGPT e cobrança de API são separados.
- **Chave inválida:** substitua a credencial.
- **Modelo ausente:** atualize o catálogo e escolha um modelo disponível.
- **Rate limit:** aguarde a janela indicada pelo serviço.
- **Endpoint local offline:** inicie Ollama ou LM Studio e confirme o endereço
  de loopback.

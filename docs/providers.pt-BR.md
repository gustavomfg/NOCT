# Providers e modelos

[English](providers.md)

O NOCT não depende de nenhuma empresa de IA. Você conecta o Provider
que preferir, escolhe um modelo para cada workspace, e esse modelo executa
Review, Build e Docs. Não é preciso conta em fornecedor, CLI de fornecedor nem
assinatura.

## Escolhendo a IA

Em **Configurações → IA**, conecte um serviço e escolha o modelo do workspace. O
indicador abaixo da caixa de mensagem mostra o serviço e o modelo ativos e abre
a mesma página. Sem modelo associado, a execução é recusada com uma mensagem
clara, sem recorrer a outra IA.

## Opções de conexão

| Opção | Credencial | Onde o modelo roda |
| --- | --- | --- |
| Gemini, OpenRouter, DeepSeek, Groq, Mistral, xAI | sua chave de API | servidores do Provider |
| Ollama, LM Studio | nenhuma | sua máquina (loopback) |
| **Outro** | chave opcional e seu endereço | qualquer endpoint com API de chat completions (vLLM, llama.cpp, LiteLLM, servidor próprio) |

O OpenRouter dá acesso a modelos de diferentes fornecedores com uma única chave. Use-o quando quiser um modelo que não
tem preset direto.

Não há login por assinatura. Os planos de consumo dos fornecedores de IA não
oferecem um login suportado e geral para aplicativos de terceiros; por isso a
conexão é feita por chave de API ou modelo local. O uso de uma chave de API é
cobrado pelo próprio Provider; um plano de chat é um produto separado.

Nos presets Ollama e LM Studio, o campo **Endereço do serviço** permite usar
outra porta local; em branco, mantém o endereço padrão mostrado. Depois de
salvar a conexão, **Atualizar modelos** repete a descoberta no mesmo serviço,
inclusive após uma falha, sem cadastrar outra conexão. A chave digitada é
limpa do formulário assim que a conexão é salva.

## Regras de conexão

O adapter oferece descoberta de modelos, streaming, cancelamento, function
calling e o recurso `/embeddings` quando o modelo selecionado declara a
capacidade `embeddings`. Endpoints remotos exigem HTTPS; HTTP simples é aceito
apenas para Providers locais em loopback. Catálogos que não declaram capacidades
são tratados como "não declarado", não como "sem suporte". Use o botão de modelo
de uma conexão para atualizar o catálogo e trocar de modelo. Modelos de
embeddings são associados separadamente do modelo de conversa.

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

O Build exige um modelo com function calling; se um Provider recusar uma
solicitação de Build que ofereceu ferramentas, o erro informa isso. Review e
Docs repetem uma vez sem ferramentas e respondem com o contexto selecionado
pelo Nocturne. A saída das ferramentas é dado não confiável e é enviada ao
Provider escolhido como qualquer outro contexto.

## Credenciais e diagnóstico

Chaves de Provider são cifradas pelo armazenamento seguro do sistema
operacional. No SQLite existem apenas referências opacas; elas nunca retornam
ao renderer e ficam fora de backups e relatórios de diagnóstico.

O painel de diagnóstico diferencia endpoint indisponível, credencial recusada,
modelo ausente, timeout, rate limit, resposta inválida e créditos insuficientes.
Uma falha de Provider é recuperável e não significa que o processo principal
falhou.

## Casos comuns

- **Créditos insuficientes:** adicione saldo na conta do Provider ou escolha outro.
- **Chave inválida:** substitua a credencial.
- **Modelo ausente:** atualize o catálogo e escolha um modelo disponível.
- **Rate limit:** aguarde a janela indicada pelo serviço.
- **Endpoint local offline:** inicie Ollama ou LM Studio e confirme o endereço de loopback.
- **Build recusado com ferramentas:** o modelo não suporta function calling; escolha outro modelo ou use o Review.


## Falhas de conexão

O OpenRouter usa `https://openrouter.ai/api/v1`; o Nocturne acrescenta `/models`
e `/chat/completions`. Conexões remotas validam todos os resultados DNS e deixam
o socket selecionar um IPv4 ou IPv6 acessível desse conjunto validado, sem nova
resolução do hostname nem seguir redirecionamentos.

Se a chave não puder ser salva, desbloqueie o chaveiro do sistema. No Linux,
é necessário Secret Service ou KWallet; a chave nunca é salva em texto puro.
Em desktops sem detecção nativa pelo Chromium (como Hyprland e Sway), o Nocturne
procura serviços D-Bus instalados e seleciona Secret Service ou KWallet antes de
inicializar o Electron. Desktops reconhecidos e uma opção explícita
`--password-store` mantêm sua seleção. Instalar ou desbloquear o cofre pode
exigir reiniciar o app; se o KWallet pedir autorização, permita o acesso ao
Nocturne. A ausência de cofre seguro continua impedindo salvar a chave.
O catálogo público não comprova que uma chave tenha acesso à inferência ou créditos.
HTTP 402 significa créditos insuficientes; erros recebidos depois de iniciar o
stream também interrompem a execução, mantendo a resposta parcial como incompleta.

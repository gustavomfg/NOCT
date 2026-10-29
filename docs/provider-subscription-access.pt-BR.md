# Acesso aos Providers e assinaturas

[English](provider-subscription-access.md)

O NOCT conecta-se a IAs por chave de API e por runtimes locais. Não
oferece login com uma assinatura de chat de consumo.

- **Chave de API:** funciona com qualquer Provider configurado; o uso é cobrado
  pelo próprio Provider e é separado de qualquer plano de chat.
- **Runtime local** (Ollama, LM Studio): sem chave e sem conta de fornecedor; o
  modelo roda na sua máquina.
- **Agregadores** (por exemplo OpenRouter): uma chave dá acesso a modelos de
  vários fornecedores, cobrados pelo agregador.

Assinaturas de consumo não oferecem um login suportado e geral para aplicativos
de terceiros; por isso o Nocturne não pede, não guarda e não reutiliza uma
assinatura. Veja [Providers](providers.pt-BR.md) para as opções de conexão.

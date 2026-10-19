# Nocturne Studio

> Um workspace desktop local-first para entender, revisar e evoluir projetos de software reais com a IA que você escolher.

[English](README.md)

Nocturne Studio é software livre sob a licença MIT. Ele mantém juntos o workspace
de um projeto, suas conversas, achados de engenharia e conhecimento durável.
É um workspace desktop de engenharia, não uma IDE, uma substituição autônoma do
desenvolvedor nem um produto oficial de qualquer fornecedor de IA. Ele não depende
de uma IA, empresa ou CLI específico: baixe o app, conecte uma API ou modelo local,
escolha o modelo do workspace e comece a trabalhar.

Ele enfrenta a fragmentação de contexto das ferramentas baseadas apenas em
prompts: estado do projeto, evidências de revisão, decisões e conhecimento aprovado
continuam conectados em vez de serem reconstruídos a cada conversa.

![Workspace do Nocturne Studio com IA ativa, conversa e painel do agente](docs/images/workspace-overview.png)

As capturas mostram a interface atual em português com dados de demonstração.

## O que ele faz

- **Workspaces:** seleciona e autoriza explicitamente uma pasta de projeto;
  workspaces movidos ou restaurados continuam sem autorização até serem
  selecionados novamente.
- **Review Mode:** análise somente leitura com sugestões baseadas em evidências
  e reconciliação de achados novos, persistentes e resolvidos.
- **Build Mode:** alterações assistidas por agente dentro do workspace
  autorizado com qualquer modelo que suporte ferramentas, com aprovação de
  comandos, progresso, diff visível e rollback protegido quando suas
  precondições são atendidas.
- **Docs Mode:** preview, comparação e aplicação incremental de Markdown com
  confirmação, verificação de concorrência e escrita atômica.
- **Segundo Cérebro e Awareness:** memórias locais estruturadas com aprovação,
  escopo, atualidade e explicação do contexto selecionado para cada execução.
- **Conversas e Git:** conversas persistentes, históricos paginados, estado Git
  do workspace e preparação de commits.
- **Camada de Providers:** Google Gemini, OpenRouter, DeepSeek, Groq, Mistral,
  xAI, Ollama, LM Studio ou qualquer endpoint com chat completions. O Nocturne
  não depende de fornecedor de IA, CLI nem assinatura.

O desenvolvedor continua responsável pela intenção, aprovação, revisão e
alteração final no projeto.

## Artefatos de release suportados

A validação oficial de release cobre atualmente:

| Plataforma | Artefato configurado pelo build |
| --- | --- |
| Windows 10/11 | instalador NSIS x64 (`.exe`) |
| Linux desktop | AppImage e `tar.gz` (vale a arquitetura do build publicado) |
| macOS | DMG e ZIP do atualizador (vale a arquitetura do build publicado) |

A validação de pacotes sem assinatura roda em Linux, Windows e macOS. Releases
estáveis publicam os alvos configurados: Linux tem manifesto de checksum
assinado por GPG no ambiente protegido, enquanto Windows e macOS atualmente
publicam checksums sem assinatura de plataforma nem notarização; consulte
[instalação](docs/installation.pt-BR.md) e [compatibilidade](docs/compatibility.pt-BR.md).

## Escolha sua IA

1. Abra **Configurações → IA** (ou clique no indicador de IA abaixo da caixa de
   mensagem).
2. Clique em **Conectar uma IA** e escolha um serviço: chave de API (Gemini,
   OpenRouter, DeepSeek, Groq, Mistral, xAI), runtime local (Ollama, LM Studio)
   ou **Outro** para qualquer endpoint com chat completions (vLLM, llama.cpp,
   LiteLLM, servidor próprio).
3. Escolha o modelo do workspace atual. O indicador no compositor mostra qual
   serviço e modelo a próxima execução usará.

![Providers de API disponíveis em Configurações → IA](docs/images/ai-providers.png)

<details>
<summary>Modelos locais, endpoints próprios e modelo ativo</summary>

Ollama, LM Studio e endpoints próprios usam o mesmo fluxo de conexão.

![Opções de runtimes locais e endpoint próprio](docs/images/local-models.png)

A conexão ativa mostra o Provider e o modelo vinculados ao workspace.

![OpenRouter e seu modelo selecionado nas configurações de IA](docs/images/ai-connections.png)

</details>

Review, Build e Docs passam todos pelo modelo selecionado. O Nocturne oferece
ao modelo um pequeno conjunto de ferramentas executadas no processo principal:
`list_directory`, `read_file` e `search_text` em todos os modos, além de
`write_file`, `edit_file` e `run_command` apenas no Build. Todo caminho fica
limitado à raiz autorizada, diretórios gerados e prováveis segredos (`.env`,
chaves, credenciais) são recusados, todo comando aguarda sua aprovação e roda
sem shell, e toda alteração do Build é capturada em BEFORE/AFTER para revisão e
rollback. Configurações → IA também pode pedir aprovação antes de cada alteração
de arquivo. O Build exige um modelo com function calling; Review e Docs usam
apenas o contexto selecionado pelo Nocturne quando um modelo só de chat recusa
ferramentas.

Não há login por assinatura: os planos de consumo não oferecem login suportado
para aplicativos de terceiros. Use uma chave de API, um agregador como o
OpenRouter (uma chave para modelos de vários fornecedores) ou um modelo local.
Veja [providers](docs/providers.pt-BR.md).

As credenciais de API dos Providers são cifradas pelo armazenamento seguro do
sistema operacional, ficam no processo principal e não entram em backups ou
diagnósticos.

No Linux, é necessário ter Secret Service ou KWallet disponível e desbloqueado
para salvar chaves de API. O Nocturne detecta os cofres instalados no Hyprland e
em desktops semelhantes. Veja [falhas de conexão](docs/providers.pt-BR.md#falhas-de-conexão).

## Dados locais e recuperação

Conversas, configurações, catálogo de modelos e conhecimento estruturado ficam
em um banco SQLite local. Arquivos de contexto do workspace usam escritas
limitadas e atômicas. O aplicativo valida o banco, cria snapshots antes de
operações destrutivas e pode colocar um banco corrompido em quarentena antes de
restaurar um candidato válido com confirmação do usuário. Backups não incluem
arquivos do projeto nem credenciais de Providers. Esses mecanismos reduzem o
risco de recuperação; não são promessa de durabilidade absoluta contra toda
falha de hardware ou filesystem.

Leia [backup e recuperação](docs/backup-and-recovery.pt-BR.md) antes de mover
ou restaurar um workspace.

## Segurança e privacidade

O renderer usa isolamento, sandbox e nenhuma integração Node.js. Capacidades
nativas atravessam APIs nomeadas do preload e handlers IPC validados. Caminhos
do workspace são contidos e rechecados, Review permanece somente leitura e
Providers remotos usam HTTPS com validação de endereços. O Nocturne é local-first,
mas prompts, contexto, anexos e a saída das ferramentas chamadas pelo modelo são
enviados ao Provider escolhido; um runtime local como o Ollama os mantém na
máquina.
Consulte [segurança](docs/security.pt-BR.md), [privacidade](docs/privacy.pt-BR.md)
e [SECURITY.md](SECURITY.pt-BR.md).

## Requisitos para desenvolvimento

- Node.js `>=24.18 <25`
- npm `>=11 <12`
- ferramentas nativas compatíveis com `better-sqlite3`

```bash
npm ci
npm run dev
```

A lista completa está em [desenvolvimento](docs/development.pt-BR.md).

## Estado atual

O `v1.0.2` usa `gustavomfg/nocturne-studio` como repositório de publicação e de
atualização. Uma tag de versão inicia o workflow de release no GitHub, que valida
o código-fonte, o inventário de todas as plataformas e os assets publicados.
Checksums do Linux são assinados por GPG; Windows e macOS continuam sem
assinatura, e o macOS sem notarização, conforme a política atual.

Limitações conhecidas: agentes dependem da qualidade de function calling do
modelo, comandos rodam no host sem sandbox do sistema operacional (a aprovação é
a fronteira) e uma execução para após um número limitado de etapas de
ferramenta. Adapters nativos (fora do protocolo Chat Completions) para Anthropic e
GitHub Copilot, marketplace, colaboração em nuvem e orquestração multiagente não
fazem parte deste contrato de release.

## Documentação

- [Índice da documentação](docs/README.pt-BR.md) · [English](docs/README.md)
- [Instalação](docs/installation.pt-BR.md) · [English](docs/installation.md)
- [Primeiro uso](docs/getting-started.pt-BR.md) · [English](docs/getting-started.md)
- [Providers](docs/providers.pt-BR.md) · [English](docs/providers.md)
- [Configuração](docs/configuration.pt-BR.md) · [English](docs/configuration.md)
- [Review, Build e Docs](docs/modes.pt-BR.md) · [English](docs/modes.md)
- [Segundo Cérebro e Awareness](docs/second-brain.pt-BR.md) · [English](docs/second-brain.md)
- [Backup e recuperação](docs/backup-and-recovery.pt-BR.md) · [English](docs/backup-and-recovery.md)
- [Atualizações](docs/updates.pt-BR.md) · [English](docs/updates.md)
- [Segurança](docs/security.pt-BR.md) · [English](docs/security.md)
- [Privacidade](docs/privacy.pt-BR.md) · [English](docs/privacy.md)
- [Solução de problemas](docs/troubleshooting.pt-BR.md) · [English](docs/troubleshooting.md)
- [Desenvolvimento](docs/development.pt-BR.md) · [English](docs/development.md)
- [Arquitetura](docs/architecture.pt-BR.md) · [English](docs/architecture.md)
- [Notas do candidato 1.0.2](docs/releases/v1.0.2.pt-BR.md) · [English](docs/releases/v1.0.2.md)
- [Prontidão da release 1.0.2](docs/release-readiness-1.0.2.md)
- [Notas históricas da versão 1.0.1](docs/releases/v1.0.1.pt-BR.md) · [English](docs/releases/v1.0.1.md)
- [Checklist do candidato de release](docs/release-rc-checklist.pt-BR.md) · [English](docs/release-rc-checklist.md)
- [Prontidão histórica da 1.0.1](docs/release-readiness-1.0.md)

English é a fonte pública canônica. As traduções em português usam o sufixo
`.pt-BR.md` e preservam títulos, comandos, caminhos e nomes técnicos.

## Licença

Nocturne Studio é distribuído sob a [licença MIT](LICENSE).

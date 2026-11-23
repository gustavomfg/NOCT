# Política de segurança

[English](SECURITY.md)

O NOCT é um aplicativo desktop local, mas lida com arquivos de projeto, uma chave de provedor e comunicação com um serviço
de IA externo. Relatos de segurança são bem-vindos.

## Como relatar uma vulnerabilidade

Relate em particular, por **GitHub › Security › Report a vulnerability** (um security advisory privado) neste
repositório. Não abra issue pública para uma vulnerabilidade, e não inclua segredos, arquivos de projeto ou prompts
privados em nenhum relato. Se o relato privado não aparecer na página do repositório, abra uma issue mínima dizendo só que
você tem um relato de segurança, sem detalhes, e peça ao mantenedor um canal privado.

Inclua, quando for seguro:

- a versão do NOCT (o nome do instalador a traz) ou o commit;
- o sistema operacional, a versão e a arquitetura;
- o impacto e uma reprodução mínima;
- o comportamento esperado e o observado;
- uma mitigação ou contorno, se conhecido.

Antes de compartilhar logs, remova chaves de API, tokens, arquivos de banco, conteúdo de projetos e dados pessoais.

## Escopo

A política cobre o aplicativo desktop: o shell Tauri e seus comandos, a interface (Leptos, WebAssembly) e sua
Content-Security-Policy, a autorização e o confinamento dos workspaces, as alterações de arquivos e sua aprovação, a
persistência e a recuperação, a integração com o OpenRouter e sua política de rede, o armazenamento da credencial, os
instaladores e a configuração do atualizador. O OpenRouter e os provedores de modelos por trás dele têm suas próprias
políticas de segurança.

O NOCT está em beta. Só a versão mais nova recebe correções. Os instaladores não são assinados pelo sistema operacional
(README.pt-BR.md, "Instalar").

## Desenho de segurança

- **A chave fica no cofre do sistema operacional.** A chave do OpenRouter é gravada uma vez no cofre
  (`crates/noct-secrets`), cuja chave-mestra está no cofre de credenciais do sistema. Ela nunca volta para a interface e
  nunca vai para os logs; as linhas do log são redigidas antes de gravadas.
- **Arquivos são escritos só dentro do projeto, e só depois da aprovação.** O assistente propõe alterações; cada uma espera
  até a pessoa aprovar, e pode ser desfeita. As regras de caminho recusam qualquer coisa fora do projeto, links simbólicos
  que saem dele, arquivos do Git e do CI, arquivos que rodam sozinhos, nomes de dispositivo e caminhos codificados
  (`noct_core::file_change`). O modo Revisar nunca escreve.
- **Comandos estreitos.** A interface chega ao código nativo só pelos comandos que o shell declara, liberados apenas para a
  janela principal. Cada comando confere a entrada e devolve uma mensagem de erro fixa; causas e caminhos ficam no shell.
- **Webview estrito.** Sem script nem estilo inline, sem origem remota, navegação limitada ao app; numa build release, sem
  menu da página, sem atalho de recarregar ou imprimir, sem inspetor. As respostas são desenhadas como texto: Markdown sem
  HTML, links não clicáveis, imagens não carregadas.
- **Política de rede estreita.** As chamadas ao provedor vão só para o OpenRouter, por HTTPS, sem seguir redirecionamento,
  conferindo cada endereço resolvido, sem proxy, com limite de tempo e de tamanho.
- **Consentimento por projeto.** Nada é enviado ao provedor por um projeto até a pessoa permitir.
- **Workspaces restaurados não são confiáveis** até a pessoa escolher a pasta de novo.

Estas são mitigações implementadas, não uma certificação formal de segurança. O registro de cada revisão de segurança está
em [docs/status.md](docs/status.md).

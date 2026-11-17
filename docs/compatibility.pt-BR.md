# Compatibilidade

A restauração protegida tem contrato mais restrito que a aplicação:
[rollback V1](native-rollback-v1.md) suporta Linux tmpfs/ext4/Btrfs local,
macOS APFS e Windows NTFS em volume fixo, com restrições de tipo/permissão/tamanho.
Outros ambientes recusam a mutação com `UNSUPPORTED`; captura de checkpoints,
inspeção de diffs e aceitação continuam disponíveis. O worker nativo exige
toolchain C++17 de desenvolvimento, sem alterar o ABI SQLite/Electron.

O shell Rust em `migration/rust` tem só o backend de rollback de Linux. Em macOS e
Windows ele recusa a restauração com `UNSUPPORTED`. Veja
[backends de rollback em Rust](rust-native-rollback-backends.md).

[English](compatibility.md)

## Alvos oficiais do aplicativo

| Plataforma | Artefato de release |
| --- | --- |
| Windows 10/11 | instalador NSIS x64 |
| Linux desktop | AppImage ou `tar.gz` da arquitetura publicada |
| macOS | DMG e ZIP do atualizador da arquitetura publicada |

Somente artefatos publicados pelo workflow automatizado de release após seus
gates de validação são releases oficiais. Pacotes locais e builds de outra arquitetura são artefatos
de desenvolvimento até passarem pelos mesmos smoke checks e gates.

## Compatibilidade de desenvolvimento

- Node.js `>=24.18 <25`;
- npm `>=11 <12`;
- Electron 43.x;
- `better-sqlite3` 13.x com N-API, verificado no runtime Electron adotado e no
  Node.js de desenvolvimento.

Use `npm run test:abi` para verificar o carregamento do módulo nativo no
runtime empacotado antes da suíte completa. A compatibilidade N-API não
substitui o smoke do pacote.

## Versões mínimas de plataforma

O item 4 da [ADR 0001](adr/0001-rust-migration.md#open-decisions) está parcialmente resolvido:
os valores abaixo são propostas, e o responsável pelo projeto ainda não as aceitou. O Tauri documenta
o que exige e qual é o padrão da configuração dele. Ele não define o piso de suporte do NOCT, então cada
linha mostra a afirmação do Tauri ao lado da proposta do NOCT.

Fontes, lidas em 2026-11-17:

- [Pré-requisitos](https://v2.tauri.app/start/prerequisites/), "Last updated: Aug 20, 2026";
- [Referência de configuração](https://v2.tauri.app/reference/config/), "Last updated: Jul 23, 2023";
- [Instalador do Windows](https://v2.tauri.app/distribute/windows-installer/), "Last updated: Nov 5, 2026".

As páginas foram lidas por uma ferramenta de busca, não por um navegador. O texto entre aspas foi copiado da
página buscada. O restante é paráfrase.

| Plataforma | Piso proposto | O que a documentação do Tauri afirma | Status |
| --- | --- | --- | --- |
| Windows | Windows 10 e Windows 11, x64 (o alvo oficial acima) | A página de pré-requisitos lista "Windows 7 and later" em System Dependencies. A página do instalador diz "By default, the Microsoft Installer (`.msi`) does not work on Windows 7" e tem uma seção "Supporting Windows 7". Nenhuma das duas define um mínimo para um aplicativo distribuído. | Decisão do projeto. As duas afirmações tratam de coisas diferentes: a página de pré-requisitos cobre a toolchain de desenvolvimento, e a nota do instalador cobre o bootstrap do MSI no Windows 7. O NOCT não tem o Windows 7 como alvo. Se o piso precisa de número de build, está em aberto. |
| WebView2 (Windows) | Runtime obrigatório. Nenhuma versão mínima definida. | A dica do Windows diz "WebView 2 is already installed on Windows 10 (from version 1803 onward) and later versions of Windows." O bloco padrão de `bundle > windows` define `minimumWebview2Version` como `null`, e `webviewInstallMode` tem como padrão um `downloadBootstrapper` silencioso. A página do instalador diz "Your application WILL NOT work if the user does not have the runtime installed." | Confirmado: o runtime é obrigatório. O número 1803 descreve a instalação prévia, não um piso de suporte. O NOCT não define versão do WebView2. |
| macOS | Proposto: macOS 10.15 (Catalina) | A página de pré-requisitos lista "macOS Catalina (10.15) and later" em System Dependencies. A referência de configuração define `minimumSystemVersion` como "A version string indicating the minimum macOS X version that the bundled application supports. Defaults to `10.13`." | Os dois valores são diferentes. 10.15 é o pré-requisito de desenvolvimento, e 10.13 é o padrão do bundler para o aplicativo. Defina `bundle.macOS.minimumSystemVersion` explicitamente na fase 8, com o valor que o responsável aceitar. O `electron-builder.json5` também não define mínimo para macOS. Versão do WKWebView: o Tauri não informa, não conferido. |
| Linux | WebKitGTK 4.1, família de pacotes `webkit2gtk-4.1`. Nenhuma versão mínima definida. | A página de pré-requisitos dá os pacotes de desenvolvimento 4.1 por distribuição, por exemplo `libwebkit2gtk-4.1-dev` (Debian), `webkit2gtk-4.1` (Arch), `webkit2gtk4.1-devel` (Fedora) e `webkit2gtk-4.1-dev` (Alpine). Não informa a versão do WebKitGTK. A aba do openSUSE indica `webkit2gtk3-devel`. | A família de pacotes está confirmada. A versão mínima **não é informada pelo Tauri**, e este documento não define uma. O spike rodou com WebKitGTK 2.54.2 (ADR 0001). Isso é uma medição, não um piso. Defina o piso pelas distribuições para as quais os artefatos Linux são feitos, na fase 8. Não conferi se o `webkit2gtk3-devel` do openSUSE fornece a API 4.1. |
| Toolchain Rust | 1.97.1 | As páginas do Tauri não informam versão do Rust. | Uma fixação do projeto em `rust-toolchain.toml`, feita pelo commit `751894e`. Não é um requisito do Tauri. |

Não conferido: qualquer afirmação do Tauri sobre a versão do WKWebView e os campos de iOS da referência de
configuração. A página de configuração foi lida inteira, exceto um trecho, lido só por um resumo. Os campos usados
acima foram lidos literalmente de outras partes da página.

## Compatibilidade de IA

- nenhum fornecedor de IA específico é exigido; o mínimo é um endpoint que liste
  modelos e implemente chat completions com streaming;
- o Build exige, além disso, um modelo com function calling;
- endpoints remotos exigem HTTPS;
- Ollama e LM Studio locais usam loopback.

Não há adapters nativos dedicados para Anthropic, Gemini ou GitHub Copilot no
contrato atual; esses modelos são acessados por um gateway compatível ou por um
preset de Provider.

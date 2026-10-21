# Instalação e requisitos

[English](installation.md)

## Artefatos oficiais da v1.0.5

Instale somente artefatos publicados para a release desejada:

- Windows 10/11 x64: `Nocturne-Studio-Windows-1.0.5-Setup.exe`;
- Linux desktop: `Nocturne.Studio-Linux-1.0.5.AppImage` ou `tar.gz`;
- macOS ARM64: `Nocturne-Studio-Mac-1.0.5-Installer.dmg`; o atualizador usa
  o ZIP correspondente.

O repositório canônico e a fonte do atualizador são
`gustavomfg/nocturne-studio`. A arquitetura é a indicada pelo asset da
release; não trate um artefato do workflow de package-validation como release
estável.

Compare o manifesto específico da plataforma antes de instalar:

- Linux: `SHA256SUMS-Linux` e sua assinatura GPG destacada
  `SHA256SUMS-Linux.sig`;
- Windows: `SHA256SUMS-Windows`;
- macOS: `SHA256SUMS-macOS`.

Os checksums Linux são assinados no environment protegido de stable-release.
Os pacotes Windows e macOS atualmente não possuem assinatura nem notarização;
o sistema operacional pode mostrar um aviso adicional de confiança. A presença
apenas de checksum não significa que exista assinatura de plataforma.

O cliente publicado `v1.0.0` continua compatível com o slug histórico
`gustavomfg/Nocturne-Codex` pela redirect de rename do GitHub. Os novos pacotes
`v1.0.1` usam diretamente o repositório canônico.

## Requisitos de IA

Nenhum fornecedor de IA nem programa de linha de comando é exigido. Review,
Build e Docs funcionam com qualquer Provider configurado (veja
[Providers](providers.pt-BR.md)); o Build exige um modelo com function calling.
O Nocturne não instala nem depende de outro programa de IA.

## Ambiente de desenvolvimento

- Node.js `>=24.18 <25`;
- npm `>=11 <12`;
- ferramentas nativas compatíveis com `better-sqlite3`.

```bash
npm ci
npm run dev
```

Veja [desenvolvimento](development.pt-BR.md) para validação e empacotamento.

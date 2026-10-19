# Desenvolvimento

[English](development.md)

## Ambiente

- Node.js `>=24.18 <25`;
- npm `>=11 <12`;
- WebStorm é recomendado e o launcher `webstorm` deve estar no `PATH`;
- ferramentas nativas compatíveis com o ABI do Electron e `better-sqlite3`.

## Comandos diários

```bash
npm ci
npm run dev
npm run typecheck
npm run lint
npm test
npm run build
```

Validações adicionais:

```bash
npm run test:abi
npm run test:renderer
npm run benchmark:sqlite
```

`npm run test:renderer` executa Playwright. `npm run test:abi` verifica o módulo
SQLite nativo dentro do runtime Electron adotado.

O renderer depende da ponte de preload do Electron (`window.nocturne`). Use
`npm run dev` para abrir o shell desktop suportado; acessar a URL do Vite em um
navegador independente não fornece as capacidades nativas e cai
intencionalmente na tela de recuperação. O Playwright usa um mock local da
ponte para a cobertura exclusiva do renderer.

## Empacotamento e smoke checks

```bash
npm run package -- --publish never
npm run package:dir -- --publish never
npm run smoke:package
npm run rehearse:packaged-recovery
```

Os rehearsals usam dados temporários isolados e são verificações de engenharia
de release, não substitutos da validação de uma release assinada.

Execute `git diff --check` antes de enviar uma alteração. Mantenha testes,
contratos, APIs do preload e documentação alinhados ao alterar uma capacidade
nativa.

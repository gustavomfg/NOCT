# Design do NOCT

Referência visual: `docs/design/mockup.html` (abre com duplo clique) e as capturas
`docs/design/{review,build,docs}-{light,dark}.png`. O mockup NÃO é código de produção.

## Princípios

- Limpo e minimalista, inspirado em produtos Apple: só o importante na tela.
- Uma janela, três modos (Revisar, Construir, Documentar), uma caixa de pergunta fixa embaixo.
- Nada some sem confirmação: comandos esperam aprovação, alterações têm "Reverter".
- Tema claro e escuro, com o claro como padrão. O escuro segue o sistema só se o usuário não escolheu.

## Regras de implementação (obrigatórias, por causa da CSP)

A CSP é `script-src 'self' 'wasm-unsafe-eval'; style-src 'self'`.

- PROIBIDO `style=`, `<style>`, `attr:style`, `style:` e `.style(` no `noct-ui` (a guarda
  `the_interface_sets_no_inline_style` falha o teste).
- Todo o CSS vai em arquivo externo servido pelo Trunk (por exemplo `crates/noct-ui/styles/app.css`).
- Tema por atributo: `<html data-theme="light|dark">`. As cores são variáveis CSS declaradas
  em `[data-theme="..."]`; os componentes só usam `var(--...)`.
- Estados por atributos ARIA (`aria-pressed`, `aria-current`) estilizados por seletor de atributo.
- Fonte: Inter se estiver instalada, com fallback para a fonte do sistema. Não carregar fonte
  de CDN (a CSP e o modo offline não permitem). Se quiser embutir a Inter, vai no pacote do app.

## Tokens de cor

| Token | Claro | Escuro |
|---|---|---|
| `--bg` | `#FBFBFC` | `#101012` |
| `--side` | `#F3F3F5` | `#161618` |
| `--surface` | `#FFFFFF` | `#1C1C1F` |
| `--text` | `#1D1D1F` | `#F4F4F5` |
| `--muted` | `#5F6068` | `#A0A0AB` |
| `--line` | `rgba(0,0,0,.09)` | `rgba(255,255,255,.10)` |
| `--accent` | `#4F46E5` (índigo) | `#9B9CFF` |
| `--on-accent` | `#FFFFFF` | `#101012` |
| `--sel` | `rgba(79,70,229,.10)` | `rgba(155,156,255,.16)` |
| `--hover` | `rgba(0,0,0,.045)` | `rgba(255,255,255,.06)` |
| `--track` | `rgba(0,0,0,.06)` | `rgba(255,255,255,.08)` |
| `--code` | `#F3F3F5` | `#141416` |
| `--add` / `--add-text` | `rgba(22,128,60,.10)` / `#0F6B33` | `rgba(74,222,128,.12)` / `#7EE2A3` |
| `--del` / `--del-text` | `rgba(180,35,24,.09)` / `#9B2318` | `rgba(248,113,113,.12)` / `#FCA5A5` |
| `--sev-high` / `--sev-mid` / `--sev-low` | `#B42318` / `#8A4B08` / `#475467` | `#FCA5A5` / `#F5C26B` / `#A0A0AB` |

Sombras: `--shadow` (cartões flutuantes e caixa de pergunta) e `--shadow-sm` (cartões comuns).
Os valores exatos estão no `mockup.html`, que é a fonte da verdade.

## Tipografia

- Corpo 14px, linha 1.5. Título de tela 28px/600, espaçamento -0.02em. Título de cartão 15px/600.
- Rótulos pequenos 12px; "eyebrow" em caixa alta, 12px/600, espaçamento .04em.
- Código e caminhos: `ui-monospace, Menlo, Consolas`, 12.5–13px.

## Forma e espaçamento

- Raios: cartões 14px, caixa de pergunta 16px, botões e itens 10px, controle segmentado 12px.
- Alvo de toque mínimo 40px de altura (botões, itens de lista, campo).
- Coluna de conteúdo centralizada com no máximo 720px. Barra lateral de 248px (mínimo 200px).
- Espaçamento base de 2, 6, 10, 12, 14, 18, 20, 24, 28, 32px.

## Estrutura da janela

1. Barra lateral: marca NOCT + botão de tema; lista de projetos (item atual com `aria-current`);
   no rodapé, cartão com o modelo ativo e o estado do cofre de chaves, e o botão Ajustes.
2. Topo do conteúdo: controle segmentado centralizado com os três modos.
3. Área principal: título e uma linha de explicação, depois cartões.
4. Rodapé fixo: caixa "Pergunte sobre o projeto" com botão de enviar (seta para cima).

## Os três modos

- **Revisar**: somente leitura. Cartões de achados com severidade (Alta/Média/Baixa, cor + texto,
  nunca só cor), título, caminho do arquivo e descrição.
- **Construir**: cartão "Aguardando aprovação" com o comando e os botões Negar/Aprovar; depois do
  Aprovar mostra "Aprovado. O comando está rodando." Abaixo, cartão de diff com botão Reverter.
- **Documentar**: cartão com diff em prosa (linha removida em vermelho, adicionada em verde) e
  botões Descartar/Confirmar alteração. Só grava depois da confirmação.

## Acessibilidade

- Contraste mínimo AA nos dois temas (os tokens acima foram escolhidos para isso).
- Foco visível em todo controle (`outline: 2px solid currentColor`).
- Diff e severidade nunca dependem só de cor: usam sinal (+/−) e rótulo.
- `prefers-reduced-motion` desliga as transições.
- Campo com `label` associado (visualmente oculto) e botões de ícone com `aria-label`.

## Segurança na interface

- A UI nunca recebe uma chave de API: só "configurada: sim/não" (o cofre vive no backend Rust).
- O estado "Cofre de chaves desbloqueado" vem de um comando do backend, não de estado local.
- Nenhuma URL externa é carregada pela janela (navegação limitada pelo shell).

## Vidro (decisão: oficial, mas como camada opcional, a ser testada em webview real)

Referência: `docs/design/mockup-vidro-painel.html` e `docs/design/painel-*.png`.
A janela do app continua OPACA (`#0b0b0e`); o vidro é CSS dentro da página, sobre um fundo de degradê.

- Ativação por atributo: `<html data-glass="on|off">`. Padrão `on`. Com `off` (ou sem suporte, ou
  `prefers-reduced-transparency`), tudo cai nas cores sólidas dos tokens acima.
- Fundo: três manchas radiais (índigo, rosa, azul) sobre um degradê de 135°. Tokens `--bg-a/b/c` e
  `--blob1/2/3` (valores no mockup).
- Onde aplicar: barra lateral, painel de conteúdo (placa flutuante, raio 26px, margem 14px),
  controle segmentado, caixa de pergunta e cartões curtos.
- Onde NÃO aplicar: blocos de comando, diffs e documentos longos (`--code`, `--add`, `--del`
  permanecem sólidos). Texto longo sempre sobre fundo suficientemente opaco (contraste AA).
- Receita: `backdrop-filter: blur(22px a 34px) saturate(1.5 a 1.7)`; fundo em degradê translúcido
  (`--pane-a/b`, `--card-a/b`); borda clara (`--pane-edge`); brilho de borda superior (`--pane-hi`);
  faixa de luz e brilho no canto por `::before` (`--pane-sheen`, `--pane-streak`).
- Cartões sobem 2px no hover; sem transição com `prefers-reduced-motion`.
- Fallbacks obrigatórios: `@supports (backdrop-filter: blur(1px))` e `prefers-reduced-transparency`.
- Desempenho: o blur grande sobre área grande é pesado para a GPU. Medir no WebKitGTK real
  (Linux), WebView2 e WKWebView antes de fixar os valores. Se pesar, reduzir o blur ou limitar o
  efeito à barra lateral e ao topo. Registrar o resultado em `docs/rust-shell.md`.

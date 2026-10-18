---
version: 1
slug: "src-app-tsx"
primary_target: "src/App.tsx"
related_targets: ["src/styles/tokens.css","src/styles/studio.css"]
---

# Desktop workspace redesign

Mode: Operate. Renderer, assets, tests and frontend documentation only.
The user rejected the previous workshop scene and owl as AI-looking. They explicitly selected a small, carefully drawn pixel mark, with no large illustration, after the proposed Zed / Linear / Raycast references. This pins the direction and takes precedence over concept seed 3deb5843 (index 4). No generated comp is authoritative; implementation follows actual desktop UI references.

## Direction contract

THESIS: A precise desktop workspace whose hierarchy comes from alignment and useful content. Replace the illustrated landing-page treatment with a practical start screen.

OWN-WORLD: Neutral charcoal, pale controls, restrained peach details, quiet separators, Geist UI and Geist Mono for code. A small generated pixel-art N image is the only pixel-art motif. No mascot or decorative scene.

STORY: Choose the project, prepare the task, choose Build / Review / Docs, then inspect the visible work and approvals. All existing commands and safeguards retain their behavior.

FIRST VIEWPORT: A 256px quiet sidebar, a flexible conversation surface, and a 344px evidence panel. A compact brand and subdued new-conversation action recede behind work. The start screen presents an open-project action and a command list. Mode choices sit directly above a compact message composer. Inspector idle states use useful text instead of another logo.

FORM: The user confirmed the competitor-referenced desktop grammar. Grounded sources considered: Zed docking; Linear navigation; Raycast commands; native command palettes; Git clients; terminal multiplexers; digital notebooks. The script's fourth assignment is native command palettes. Catalog motifs cannot override the user's confirmed references; retain their information-priority and state-discipline lessons without importing costumes.

Signature interaction: keyboard-operable mode selection and stable row actions. Focus appears immediately without movement; reduced motion remains respected.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance

## Reference and REA evidence

- Zed: https://zed.dev/ — inspected the public page, app imagery and rendered DOM. REA `inspect_web_page`, Evidence `ev_5e11e9d70133029c4a554e7d7683d0e2cedec4291ad08b1d9f7309bf9fc7161d`. Passive, origin-filtered, attach-limited observation; does not establish private desktop implementation.
- Linear: https://linear.app/now/behind-the-latest-design-refresh — official account and comparison images support a dimmer navigation region, reduced icon decoration and softer separators.
- Raycast: https://www.raycast.com/ — product imagery informed compact command rows and secondary actions. No proprietary code or external brand assets are shipped.
- Nocturne built frontend: REA `analyze_javascript_application` on `dist/`; parent Evidence `ev_1e9680b8382cc4d6c2334482ff3e0216b5f80f1d4283ed703c8301351821c736`, artifact SHA-256 `c534b5cc28309249cf10cf28b62ca24ba6200b26ddd6dd4b665c677d0ab0cc1a`. 13 relevant files, 11 JavaScript files parsed, zero parse failures. Semantic coverage is partial and bounded by analysis budgets; this is static evidence, not an execution or security audit. Source inspection establishes the actual React component edit boundaries.

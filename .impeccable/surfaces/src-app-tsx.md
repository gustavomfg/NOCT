---
version: 1
slug: "src-app-tsx"
primary_target: "src/App.tsx"
related_targets: ["src/styles/task-workspace.css","src/domains/chat/Composer.tsx","src/domains/agent/AgentPanel.tsx","src/domains/settings/AIConnectionPage.tsx"]
---

# Task workspace

Mode: Operate. Renderer, assets, tests and frontend documentation only.
The user rejected the previous workshop scene and owl as AI-looking. They explicitly selected a small, carefully drawn pixel mark, with no large illustration, after the proposed Zed / Linear / Raycast references. This pins the direction and takes precedence over concept seed 3deb5843 (index 4). No generated comp is authoritative; implementation follows actual desktop UI references.

## Direction contract

THESIS: The request anchors the work. Plans, decisions and results remain alongside it; project context can recede when reading or writing.

OWN-WORLD: Neutral charcoal, pale controls, restrained peach details, quiet separators, Geist UI and Geist Mono for code. A small generated pixel-art N image is the only pixel-art motif. No mascot or decorative scene.

STORY: Choose the project, write a goal, select Analyze / Implement / Document, inspect the proposal and decide explicitly. Templates prepare a request without executing it. The approval notice leads directly to the pending decision.

FIRST VIEWPORT: A 236px project/task sidebar, flexible central history and request surface, and a 336px progress panel. At compact desktop widths these become 224px and 304px. The request surface contains the textarea, explained modes, AI selector and send action. Recent projects and request templates are disclosures; canonical narrow drawers retain focus containment.

FORM: User selected task flow (candidate 3, seed 753961b8) over conversation-first and project-overview compositions. Code-led composition; the decision wireframe is a comparison reference, not an approved visual comp. The existing neutral desktop world remains pinned.

Signature interaction: the pending-decision action opens the activity panel and focuses the actual decision without accepting it. Mode selection uses arrow keys; disclosures support keyboard use. Existing short state transitions preserve geometry and reduced motion.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance

## Reference and REA evidence

- Zed: https://zed.dev/ — inspected the public page, app imagery and rendered DOM. REA `inspect_web_page`, Evidence `ev_5e11e9d70133029c4a554e7d7683d0e2cedec4291ad08b1d9f7309bf9fc7161d`. Passive, origin-filtered, attach-limited observation; does not establish private desktop implementation.
- Linear: https://linear.app/now/behind-the-latest-design-refresh — official account and comparison images support a dimmer navigation region, reduced icon decoration and softer separators.
- Raycast: https://www.raycast.com/ — product imagery informed compact command rows and secondary actions. No proprietary code or external brand assets are shipped.
- Nocturne built frontend: REA `analyze_javascript_application` on `dist/`; parent Evidence `ev_1e9680b8382cc4d6c2334482ff3e0216b5f80f1d4283ed703c8301351821c736`, artifact SHA-256 `c534b5cc28309249cf10cf28b62ca24ba6200b26ddd6dd4b665c677d0ab0cc1a`. 13 relevant files, 11 JavaScript files parsed, zero parse failures. Semantic coverage is partial and bounded by analysis budgets; this is static evidence, not an execution or security audit. Source inspection establishes the actual React component edit boundaries.

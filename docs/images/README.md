# Current product screenshots

Captured on 2026-10-29 from the real React renderer using Playwright/Chromium,
the dark theme and Portuguese locale. The workspace, conversation and AI binding
are demonstration data supplied through the mock preload bridge from
`tests/renderer/mockNocturne.ts`; no account, key or private project was used.
The example conversation and AI binding come directly from the renderer fixture.

| Image | View |
| --- | --- |
| `workspace-overview.png` | Workspace at 1440 × 900 with the task, progress panel and active AI |
| `ai-providers.png` | Settings dialog with remote API presets |
| `local-models.png` | Settings dialog showing local runtime choices above the API group |
| `ai-connections.png` | Settings dialog with the provider and model bound to the workspace |

The dialog images are native element screenshots at a 1440 × 900 viewport.
They have no image edits. Review captures before replacing them, and keep raw
browser output in the ignored `output/playwright/` directory.

Only the screenshots used by the current READMEs are kept here. Older captures
remain available in Git history and historical version tags.

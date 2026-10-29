---
name: "NOCT"
description: "A neutral, compact desktop engineering workspace with a small generated pixel-art N image."
colors:
  canvas: "#18191c"
  panel: "#141518"
  surface: "#202125"
  surface-raised: "#28292e"
  surface-hover: "#303137"
  border-subtle: "#303137"
  border: "#3d3f47"
  border-strong: "#61636f"
  text: "#ededef"
  text-secondary: "#c4c5ce"
  text-muted: "#a0a2ae"
  text-subtle: "#9395a2"
  accent: "#efac87"
  accent-strong: "#f5bea1"
  accent-surface: "#342a25"
  success: "#8bc5a2"
  warning: "#e5bc7a"
  danger: "#f19b9c"
  info: "#9dbce3"
  on-accent: "#261e19"
  control: "#ededef"
  on-control: "#202127"
  selection: "#554137"
  overlay: "rgb(8 9 12 / 76%)"
  light-canvas: "#fafafa"
  light-panel: "#f1f1f3"
  light-surface: "#ffffff"
  light-surface-raised: "#e8e8ed"
  light-surface-hover: "#dedfe6"
  light-border-subtle: "#e1e2e8"
  light-border: "#c3c5cf"
  light-border-strong: "#9497a6"
  light-text: "#202127"
  light-text-secondary: "#484a56"
  light-text-muted: "#626573"
  light-text-subtle: "#6a6d7b"
  light-accent: "#88502e"
  light-accent-strong: "#794325"
  light-accent-surface: "#f5e9df"
  light-success: "#306b49"
  light-warning: "#83591c"
  light-danger: "#b33d48"
  light-info: "#3e6396"
  light-on-accent: "#fffaf7"
  light-control: "#292a31"
  light-on-control: "#ffffff"
  light-selection: "#edd7c8"
  light-overlay: "rgb(20 21 28 / 42%)"
typography:
  display:
    fontFamily: "'Geist Variable', 'Geist', ui-sans-serif, system-ui, -apple-system, sans-serif"
    fontSize: "28px"
    fontWeight: 550
    lineHeight: 1.25
    letterSpacing: "-.03em"
  headline:
    fontFamily: "'Geist Variable', 'Geist', ui-sans-serif, system-ui, -apple-system, sans-serif"
    fontSize: "20px"
    fontWeight: 550
    lineHeight: 1.25
  title:
    fontFamily: "'Geist Variable', 'Geist', ui-sans-serif, system-ui, -apple-system, sans-serif"
    fontSize: "16px"
    fontWeight: 550
    lineHeight: 1.4
  body:
    fontFamily: "'Geist Variable', 'Geist', ui-sans-serif, system-ui, -apple-system, sans-serif"
    fontSize: "14px"
    fontWeight: 450
    lineHeight: 1.65
  label:
    fontFamily: "'Geist Variable', 'Geist', ui-sans-serif, system-ui, -apple-system, sans-serif"
    fontSize: "13px"
    fontWeight: 500
    lineHeight: 1.4
  code:
    fontFamily: "'Geist Mono', 'SFMono-Regular', Consolas, 'Liberation Mono', monospace"
    fontSize: "13px"
    fontWeight: 450
    lineHeight: 1.65
rounded:
  xs: "2px"
  sm: "6px"
  md: "8px"
  lg: "12px"
  xl: "12px"
  pill: "6px"
spacing:
  2xs: "4px"
  xs: "6px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "24px"
  2xl: "32px"
  3xl: "48px"
components:
  button-primary:
    backgroundColor: "{colors.control}"
    textColor: "{colors.on-control}"
    typography: "{typography.body}"
    rounded: "{rounded.sm}"
    padding: "8px 16px"
    height: "38px"
  button-primary-hover:
    backgroundColor: "{colors.control}"
    textColor: "{colors.on-control}"
    rounded: "{rounded.sm}"
  button-secondary:
    backgroundColor: "transparent"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.sm}"
    padding: "8px 16px"
    height: "38px"
  search-field:
    backgroundColor: "transparent"
    textColor: "{colors.text}"
    typography: "{typography.label}"
    rounded: "{rounded.sm}"
    padding: "0 8px"
    height: "38px"
  navigation-active:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.text}"
    typography: "{typography.label}"
    rounded: "{rounded.sm}"
    padding: "0 8px"
    height: "38px"
  branch-chip:
    backgroundColor: "transparent"
    textColor: "{colors.text-muted}"
    typography: "{typography.label}"
    rounded: "{rounded.sm}"
    padding: "4px"
    height: "30px"
  activity-container:
    backgroundColor: "transparent"
    textColor: "{colors.text-secondary}"
    typography: "{typography.label}"
    rounded: "{rounded.md}"
  mode-selected:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.text}"
    typography: "{typography.label}"
    rounded: "{rounded.sm}"
    padding: "8px 12px"
    height: "52px"
  command-row:
    backgroundColor: "transparent"
    textColor: "{colors.text-secondary}"
    typography: "{typography.body}"
    rounded: "{rounded.md}"
    padding: "8px 12px"
    height: "64px"
  composer:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.lg}"
  workspace-selector:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    typography: "{typography.label}"
    rounded: "{rounded.md}"
    padding: "12px"
    height: "64px"
  provider-option:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.md}"
    padding: "12px"
    height: "84px"
  settings-field:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.sm}"
    padding: "8px 12px"
    height: "40px"
---

# Design System: NOCT

## Overview

**Creative North Star: "The Quiet Desktop"**

A precise desktop workspace whose hierarchy comes from alignment, readable text, and useful content. Neutral charcoal surfaces, pale primary controls, quiet separators, and restrained peach details support long engineering sessions. The light theme follows the same neutral structure with dark primary controls.

Zed, Linear, and Raycast inform the compact desktop grammar. Nocturne retains its own small generated raster N in ivory and muted peach in the brand and assistant identity. The user rejected the illustrated workshop and owl; the confirmed identity has no mascot or large illustration. The request anchors the working area, plans and decisions sit alongside it, and project context can recede during reading or writing.

**Key Characteristics:**

- Neutral dark and light surfaces with clear reading contrast.
- Neutral primary controls; peach marks focus and selected semantic details.
- Compact rows, restrained corners, and flat working surfaces.
- A central request surface, adjacent progress and decisions, and collapsible context.
- One small generated pixel-art N image; conventional interface and code typography.

## Colors

The palette is neutral charcoal in dark mode and clean gray in light mode, with one restrained peach accent. The frontmatter records the actual shared colors; matching light roles use the `light-` prefix. Runtime components keep using the unprefixed CSS custom properties, which the active theme remaps.

### Primary

- **Peach detail:** `accent`, `accent-strong`, and `accent-surface` identify focus, selected semantic details, and checked settings controls. Their light equivalents deepen to warm brown for contrast.
- **Neutral action pair:** `control` and `on-control` make primary actions pale with dark text in dark mode, and dark with white text in light mode. Open-project, send, connect, and shared save actions use this pair.
- **State colors:** `success`, `warning`, `danger`, and `info` retain their functional meanings. They are status colors, not supporting brand palettes; pair them with text or icons.

### Neutral

- **Canvas and panel:** `canvas` carries the conversation and content area; the dimmer `panel` separates the sidebar and inspector.
- **Surface layers:** `surface`, `surface-raised`, and `surface-hover` distinguish inputs, selected rows, and interactive states without decorative panels.
- **Separators:** `border-subtle` divides regions; `border` frames controls; `border-strong` clarifies hover or stronger boundaries.
- **Reading hierarchy:** `text`, `text-secondary`, `text-muted`, and `text-subtle` separate main content, supporting content, and metadata. Small text remains readable.
- **Selection and overlays:** `selection` is the shared selection surface; `overlay` dims modal backgrounds. The light-theme stylesheet retains its more specific selection foreground/accent pairing.

**The Control Pair Rule.** Use color-control with color-on-control for neutral primary actions in both themes. Use color-accent with color-on-accent for the semantic filled accent controls that remain, such as checked settings boxes.

## Typography

**Display and Body Font:** Geist Variable, with Geist and the existing sans-serif system fallbacks.

**Code and Shortcut Font:** Geist Mono, with the existing platform monospace fallbacks.

The interface uses conventional, compact letterforms. Medium weights establish hierarchy without heavy promotional headings; pixel art is confined to the small identity mark.

- **Display:** the effective display token comes from the studio stylesheet, overriding the earlier base value. It is used by the start heading with tight leading and slight negative tracking.
- **Headline:** settings page headings and major section titles.
- **Title:** dialog and compact page titles; the brand wordmark uses its own established medium weight.
- **Body:** task descriptions, conversation text, and form controls. Long assistant prose uses the existing more generous line height of 1.8.
- **Label:** compact navigation, metadata, modes, and controls. Small, caption, label, and code roles share the established legibility floor (13px).
- **Code:** code blocks, inline code, paths where already monospace, and keyboard shortcuts. Do not introduce pixel fonts.

**The Small Mark Rule.** Keep the generated pixel-art N image at the small brand and assistant sizes. Do not expand it into a mascot, scene, or large illustration.

## Layout

The application fills the desktop window and keeps independently scrollable navigation, task history, and progress regions. The task-workspace layer sets a (236px) sidebar, a (336px) progress panel, and a flexible central column whose reading and request surfaces are capped at (840px). The sidebar begins with the project selector and a prominent new-task action; recent tasks remain available while recent projects use a disclosure. The topbar is (64px) high. It shows the task title and only the actions that act on it (branch, editor, terminal, memory, settings, help, progress panel). The project name lives in the sidebar, and the agent state lives in the progress panel, so the header never repeats them. Assistant prose is bounded at (72ch).

From (981px) through (1320px), the sidebar and progress panel become (224px) and (304px), including the canonical (1120px) compact-desktop range. At (980px) and below, existing left and right drawers replace inline side panels: left width `min(310px, 88vw)`, right width `min(390px, 92vw)`. Preserve their backdrops, dismissal behavior, focus containment and restoration. The renderer keeps its minimum window content width (360px). These are task-workspace overrides of the base shell dimensions, not new shared primitive tokens.

At (720px) and below, topbar and composer side padding use the compact (16px) inset, the branch chip stays out of the header at (520px) and below, and settings navigation becomes a horizontal strip with visible section names. At (520px) and below, welcome actions stack, the composer footer can wrap, the send action keeps its icon and accessible name, and provider choices become one column. The three mode choices and current mode description remain visible. Preserve the canonical (520px), (720px), (980px), (1120px), and (1320px) responsive ranges.

The start surface uses a short statement of the working contract and, only when no project is open, one project action. Its content is capped at (680px), with (32px) padding on desktop and (24px) at compact sizes. It scrolls independently above the persistent request composer. Keep async and error messages in normal layout flow, at the same width as the start content. Request modes are the only way to choose what a request does; there are no separate quick-start commands or templates.

Settings use a bounded dialog up to (1000px) wide and (760px) high, constrained by a (48px) total viewport inset. The navigation column is (220px), then (180px) at the drawer range. At (720px) and below, the dialog uses a (24px) total viewport inset and horizontal navigation; content scrolls while header and save/cancel feedback remain available. Provider selection groups local models, API-key services, and custom connections under explanatory headings, with a two-column grid that becomes one column at (520px).

The small spacing rhythm starts on a (4px) grid; the frontmatter preserves the actual named scale, including the compact (6px) step.

## Elevation & Depth

Working regions are flat. Neutral surface steps and separators carry most depth; the composer and primary actions have no decorative shadow. The inherited small, medium, and large shadow tokens remain available for structural overlays. Their exact dark and light values, focus rings, and the existing drawer shadows are recorded in the sidecar rather than added to the frontmatter schema.

**The Flat Workspace Rule.** Use tonal layers and quiet separators for working regions. Keep the composer, command rows, and primary actions flat; reserve structural elevation for overlays and drawers.

Motion uses the existing short state transitions and easing. Focus appears immediately; hover, focus, and press preserve geometry. Keep reduced-motion overrides, including the theme-transition fallback. Do not animate an empty state or the brand mark to attract attention.

## Shapes

Use the actual radius scale in the frontmatter. Small controls, mode choices, and task-history rows use the compact small radius; the project selector, user messages, provider choices, and request command rows use the medium radius; the composer uses the large radius. The extra-large token currently resolves to the same corner size as large. The legacy pill token resolves to the small radius in the studio layer, so a token name does not imply a capsule silhouette.

The mark is a generated raster N with interlocking ivory and muted-peach strokes, preserved as the canonical PNG in assets/logo/nocturne-logo.png. It occupies a transparent brand box (28px); the assistant mark is drawn at (24px) inside a (28px) identity slot. The light theme darkens the same mark through its existing filter. Use the generated image directly; preserve its alpha, proportions, and native edge detail rather than redrawing the artwork in code. Small instances use normal image sampling. Status dots retain circular geometry.

## Components

### Buttons

Primary actions are compact and neutral. Shared dialog primary actions use the frontmatter's small radius, body typography, and standard control sizing; start project actions use the established medium radius and label typography. The new-task action uses the neutral control pair in dark mode; its existing light treatment remains a white surface with dark text. Send pairs its icon with a text label, hiding only that visible label at the narrow breakpoint while preserving the accessible name. Disabled controls remain visibly inactive; the send control changes to a separate stop/cancel state during execution.

Secondary actions use text, transparent or surface fills, and quiet borders. Hover changes color or border treatment without translation or scale. All actionable targets preserve the minimum (32px) interaction floor. The header path action explicitly uses that minimum even though its text looks secondary.

### Inputs / Fields

Search rests on a transparent surface and gains a stronger surface/border on focus. The request composer is one box with a visible label outside it: the textarea sits above a single footer that holds the attachment control, the AI selector and the send/stop action. The mode switch and its one-line description sit above the box, never inside it. The textarea begins at (72px) and grows to the existing (220px) bound before scrolling. Missing-AI guidance is the AI selector itself, in the accent color, not a separate line. Preserve disabled, cancellation, and saving states.

Settings use the same shared tokens rather than a separate theme: form fields have the small radius, panel fill, body text, and the current field minimum (40px). Hover clarifies the border; focus uses the shared peach outline and focus ring. Checkbox visuals stay compact inside labelled interaction rows; checked state uses the semantic accent pair. Inline errors remain textually clear and connected to their context.

### Navigation

Sidebar tools and task-history rows use neutral selected surfaces with visible text states; the selected task also has an inset border in dark mode. The project selector has a bounded surface, while recent projects recede into a disclosure. New-task and search controls share the compact sidebar rhythm. Settings is a labelled footer action. Stable row actions must remain keyboard reachable. Settings navigation reuses the neutral selected surface and medium radius, with larger rows suited to its title and description.

The progress panel has a compact heading, execution state, and four text tabs with a quiet selected underline. The tabs keep a (44px) minimum height. Empty activity states use a useful sentence and supporting hint beneath a separator, aligned with the working content. Other existing inspector views may retain small functional icons; do not replace idle guidance with another brand illustration.

### Chips

Branch metadata is a quiet text treatment on a transparent background, with compact corners and muted text. Its frontmatter size describes the presentational chip, not permission to shrink actionable targets. Attachment and capability tags reuse the existing compact scale and keep readable text and explicit remove actions where applicable.

### Cards / Containers

Activity containers use the medium radius, transparent working surface, and subtle border. Their summary row keeps the established large control height (44px); opened sections gain a tonal heading surface. Plans, files, diffs, artifacts, and documents remain flat, bounded working regions. Preserve progressive disclosure and independent scrolling for long content.

### Task Modes and Decisions

Analyze, Implement, and Document appear as one segmented radio group above the request box, sized to its labels. Each choice shows only its action name; the one-line description beneath the group states what the selected mode does to the project (reads and suggests, edits files and asks for approval of commands, writes documentation). The internal mode names Review, Build, and Docs are not shown. The active choice uses a neutral selected surface; arrow keys and Home/End move between modes. The placeholder of the request box follows the selected mode.

The pending-approval notice pairs warning color with explicit text and a review action. That action opens the progress panel, selects Activity, and focuses the pending decision. It never accepts the decision. Plans, activities, suggestions, artifacts, and validation retain their distinct views and existing approval boundaries.

The pending-approval notice sits above the mode group and leads to the pending decision in the progress panel; it never accepts the decision.

### AI Connections

Settings retain the shared field and navigation grammar. The active connection is a flat medium-radius surface, followed by existing connection controls and a neutral add action. Connection selection separates local models, services using an API key, and custom endpoints with plain explanatory copy. Provider options use a panel fill, medium radius, subtle icon block, and (84px) minimum height; hover clarifies the border and surface. Descriptions wrap at narrow widths, and each choice exposes its provider name with associated supporting text. Authentication, model selection, feedback, and saved-state behavior remain explicit.

## Do's and Don'ts

### Do:

- Do use the shared semantic tokens and inspect both dark and light themes.
- Do keep interface text at or above the 13px floor and actionable pointer targets at or above 32px.
- Do preserve immediate visible focus, keyboard navigation, dialog focus containment and restoration, and reduced-motion behavior.
- Do keep the small generated N mark crisp and the interface text in Geist, with Geist Mono for code and shortcuts.
- Do preserve canonical responsive drawers, independent scrolling, bounded histories, and stable control geometry.
- Do keep the selected mode description visible and associated with the current mode control.
- Do keep the request separate from sending, and decision navigation separate from approval.

### Don't:

- Don't bring back the owl, a mascot, a large illustration, or the rejected workshop scene.
- Don't introduce a sage theme or decorative accent washes across the neutral workspace.
- Don't use peach as the fill for every primary action; preserve the neutral control pair.
- Don't encode mode, approval, status, or errors through color alone.
- Don't turn isolated legacy hardcoded colors or uppercase metadata into new system tokens.
- Don't move or resize controls on hover, focus, or click.

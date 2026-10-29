# NOCT

<!-- impeccable:product-schema 1 -->

## Platform

web

React renderer inside a local Electron desktop application.

## Users

Developers working with a real local software project. Product facts and constraints here are supplied by the project's AGENTS.md and the user's redesign request.

## Product Purpose

Keep project context, conversations, plans, approvals, changes, and validation together. The developer retains intent, judgment, and accountability.

## Operating Context

Analyze → Plan → Explain → Suggest → Approve → Implement → Validate. Review is read-only; Build exposes modifications and approvals; Docs focuses on documentation. Long sessions, large histories, keyboard navigation, and resizable desktop windows are supported.

## Capabilities and Constraints

The redesign is frontend-only. Preserve IPC contracts, persistence, security boundaries, existing actions, localization, viewport rendering, and performance isolation. Keep the canonical responsive breakpoints and 13px typography / 32px interaction floors. Dark is the primary identity; the existing light theme remains usable.

## Brand Commitments

The user requests a complete visual reformulation with a personal character and less generic AI styling. The illustrated night workshop and pixel owl were rejected. The confirmed direction uses Zed, Linear and Raycast as references for a neutral, compact desktop interface. Pixel art is limited to a small logo; no large illustration. The latest user instruction selects a generated raster image as the sole source of the logo, replacing code-drawn SVG attempts. The NOCT name remains.

## Accessibility & Inclusion

Visible keyboard focus, dialog focus containment and restoration, reduced-motion support, readable contrast, clear errors and async status, and stable control geometry.

## Evidence on Hand

Existing product implementation in src/, bilingual interface copy, and renderer regression captures in tests/renderer/__screenshots__/. These fixtures are test data, not customer claims.

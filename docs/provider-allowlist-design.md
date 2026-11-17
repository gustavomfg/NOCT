# Provider endpoint allowlist: design

- **Status:** Proposed. Nothing is implemented. Waiting for maintainer review.
- **Date:** 2026-11-17, reviewed against `migration/rust` at `0ae0fb8`.
- **Scope:** the endpoints of the Chat Completions adapter (built-in presets and "Other"), in the TypeScript implementation that ships today and in the Rust port that replaces it.
- **Related:** [security.md](security.md), [providers.md](providers.md), [ADR 0001](adr/0001-rust-migration.md), [ADR 0002](adr/0002-envelope-provider-credentials.md).

## 1. Policy being designed

From the maintainer's brief:

- allowed hosts are a fixed built-in list plus hosts the user approves explicitly;
- HTTPS is required, except loopback for local models;
- every resolved address and every redirect is checked, so no request reaches an internal network or a loopback service the user did not authorize;
- provider keys stay in the main process, and the renderer sees only "configured: yes or no".

The last point already holds. The first three hold only in part (section 3).

## 2. Threat model

Assets:

- **A1** provider API keys, which live only in the main process and are encrypted as in ADR 0002;
- **A2** workspace context, which is sent to the endpoint on every run (source excerpts, conversation, tool output);
- **A3** internal network services and cloud metadata endpoints reachable from the user's machine.

Threats:

- **T1** Mistake: the user types an internal address, an unexpected port, or plain HTTP for a remote provider.
- **T2** DNS answers mix public and private addresses, or change between the check and the connection (rebinding).
- **T3** Redirect: an approved host answers with a 3xx pointing at an internal or loopback target.
- **T4** Forged configuration: a compromised or buggy renderer calls the configuration IPC with an endpoint or approval the user did not see.
- **T5** Indirect: workspace files or model output try to change the endpoint or the approvals. No agent tool may reach the configuration.
- **T6** Credential misdirection: a key is sent to a host other than the one it was saved for, for example after the endpoint is edited.

Out of scope: a malicious host the user approved; a compromised operating system or user account; certificate pinning to vendors; a process already running as the user that impersonates a local preset.

## 3. Current behavior (TypeScript, the reference)

| Concern | Where | Behavior today |
| --- | --- | --- |
| Built-in endpoints | `shared/ai/providerCatalog.ts`, `providerCatalog` | Eight presets with a fixed `baseUrl`: six remote, two loopback (Ollama, LM Studio). |
| URL syntax | `electron/ai/providers/chat-completions/config.ts`, `validateBaseUrl` | Rejects credentials, query, fragment and schemes other than HTTP and HTTPS. |
| Plain HTTP | same, `validateBaseUrl` | Allowed only when `source` is `local` and the host is loopback. |
| Name and literal address checks | same, `isUnsafeRemoteHost` | For `remote` only: refuses `localhost`, `metadata.google.internal` and literal IP addresses outside the public ranges. |
| Transport choice | `electron/ai/providers/chat-completions/adapter.ts`, constructor | `remote` uses `createRemoteProviderTransport()`. `local` uses the global `fetch` with no address check. |
| Remote DNS and connection | `adapter.ts`, `createRemoteProviderTransport`, `resolveProviderHost`, `requestPinnedAddress` | Requires HTTPS, resolves every address, refuses if any address is not public, and connects through a pinned lookup over that same snapshot. The socket never resolves the name again. |
| Redirects | `adapter.ts`, `redirect: 'error'` on the fetch calls; `requestPinnedAddress` does not follow redirects | Refused for both transports. |
| Consent | `ProviderConfigurationService` create and update | Saving the connection is the only consent. No record names the approved host. |
| Tests | `tests/chatCompletionsProviderAdapter.test.ts` | Cover the HTTP loopback rule, the refusal of mixed or private DNS answers, and the IPv4 fallback when DNS lists IPv6 first. No test title names redirects; the word appears in the file, but the refusal is not confirmed by a named test. |

## 4. Gaps against the policy

- **G1. Local endpoints skip the address check.** `source: 'local'` accepts `https://` to any host, including `https://10.0.0.5`, or to a name that resolves to a private address. The request uses the global `fetch` with no DNS check. No approval is involved. This is the main gap.
- **G2. No built-in list and no per-host approval.** "Other" accepts any public HTTPS host, and saving the form is the only consent.
- **G3. No port rule.** Remote sources accept `https://host:8443` or any other port.
- **G4. Documentation wording.** [security.md](security.md) says remote providers "validate all resolved addresses". That is true for remote sources only. Until G1 is closed, the sentence should say "remote sources".

## 5. Built-in host list (proposal)

Each preset maps to one host on port 443 (loopback presets use their local port). Selecting a preset is the approval for its host. The sources are the pages read on 2026-11-17.

| Preset | Host | Base URL used by NOCT | Vendor source | Status |
| --- | --- | --- | --- | --- |
| Gemini | `generativelanguage.googleapis.com` | `https://generativelanguage.googleapis.com/v1beta/openai` | Google AI Studio, OpenAI compatibility page (`https://aistudio.google.com/docs/openai`; the `ai.google.dev` address redirects there). The page quotes the base URL with a trailing slash. | Confirmed |
| OpenRouter | `openrouter.ai` | `https://openrouter.ai/api/v1` | `https://openrouter.ai/docs/quickstart`, section "Using the OpenAI SDK". Its examples use the Responses API, not Chat Completions. | Base URL confirmed; the Chat Completions path is not shown on that page |
| DeepSeek | `api.deepseek.com` | `https://api.deepseek.com` | `https://api-docs.deepseek.com/`, parameter table, `base_url (OpenAI)` | Confirmed |
| Groq | `api.groq.com` | `https://api.groq.com/openai/v1` | `https://console.groq.com/docs/openai`: "change the `base_url` to `https://api.groq.com/openai/v1`" | Confirmed |
| Mistral | `api.mistral.ai` | `https://api.mistral.ai/v1` | `https://docs.mistral.ai/api/`. The page read was truncated. The only URL on it is the example `https://api.mistral.ai/v1/chat/completions`; no base URL is labelled. | **Not confirmed** as a base URL |
| xAI | `api.x.ai` | `https://api.x.ai/v1` | `https://docs.x.ai/developers/rest-api-reference`, "Base URLs" table: `https://api.x.ai`, with no path. The page read also used a different name for the API in one sentence, which must be checked by hand before citing it. | Host confirmed; the `/v1` path is **not** confirmed. Read the Chat Completions reference first |
| Ollama | `127.0.0.1`, port 11434 | `http://127.0.0.1:11434/v1` | Product default in `providerCatalog.ts`. No vendor page read. | **Unconfirmed** against the Ollama documentation |
| LM Studio | `127.0.0.1`, port 1234 | `http://127.0.0.1:1234/v1` | Product default in `providerCatalog.ts`. No vendor page read. | **Unconfirmed** against the LM Studio documentation |

Not in the list: regional hosts such as `eu.openrouter.ai`. They need an explicit approval.

## 6. Approval flow (proposal)

**Presets.** Selecting a preset approves its host and port. The host is fixed in code, so the user cannot change it through the form.

**Other endpoints.**

1. The user enters a URL in Settings, under "Other".
2. Before the save is accepted, the UI shows the host, the scheme, the port and the address scope (section 7), and says that the workspace context and the key will be sent to that host. A host without an approval is refused with `host-not-approved`.
3. On confirmation, the main process records an approval: host, port, scope and the time of approval. The renderer reads the list of approvals only to show them in Settings.
4. Approvals are per host and port. Changing either needs a new approval. An approval never names an IP address: every request still resolves the name and applies section 8.
5. The user can revoke an approval. Connections on that host are disabled with `host-not-approved` and kept, not deleted.
6. A saved key is bound to the endpoint it was entered for. Changing the host requires entering the key again (T6).

**Scopes.**

- `public` (default): every resolved address must be public, as today.
- `private-network`: a local-source endpoint on RFC 1918 or IPv6 unique-local addresses, such as another machine on the LAN. It needs its own confirmation text.
- `loopback`: a loopback host on a port that is not a preset port.

**Always refused,** even with an approval: link-local `169.254.0.0/16` and `fe80::/10` (where cloud metadata lives), `0.0.0.0/8`, `::`, multicast (`224.0.0.0/4`, `ff00::/8`), and the names `localhost` for remote sources and `metadata.google.internal`. Other reserved ranges stay refused unless a scope above names them.

## 7. Address scope and ports

| Source | Scheme | Addresses allowed | Port | Needs approval |
| --- | --- | --- | --- | --- |
| `remote` | HTTPS only | `public` only | 443 by default | A preset, or an approval for another port |
| `local` | HTTPS | `public`, `private-network` or `loopback`, as approved | Any | A preset, or an approval with the matching scope |
| `local` | HTTP | `loopback` only | Any | A preset, or an approval with scope `loopback` |

## 8. DNS and redirect handling

For every request, not only when the connection is saved:

1. Parse the URL once and refuse credentials, query and fragment, as today.
2. If the host is an IP literal, that address is the only one. Otherwise resolve all A and AAAA records once (`lookup` with `all: true`, as today).
3. Check every address against the scope of the approval. One disallowed address refuses the whole request. Today this rule runs only for `remote`; it applies to every source under this design, which closes G1.
4. Connect only to the checked snapshot, through a pinned lookup (`createPinnedLookup`, as today). The socket must not resolve the name again during the request, so an answer that changes after the check is never used (T2).
5. Never follow a redirect. A 3xx response is a protocol error, as today (`redirect: 'error'` and the pinned request, which does not follow redirects) (T3).
6. Keep the existing timeout and `AbortSignal` behaviour.

The check runs on every request, so a host whose DNS changes between two runs is checked again on the second. The pinning closes the gap inside one request.

## 9. Changes to the IPC contract

The renderer keeps its rule: it sees `credentialConfigured` and never the key. The proposed changes, which must land together (AGENTS.md, "native capability" rule):

- **Shared contracts** (`shared/ai/providerConfiguration.ts`, `shared/ai/providerConfigurationSchemas.ts`):
  - a new error code `host-not-approved` in `ProviderConfigurationErrorCode`; the message must not reveal internal addresses;
  - a `ProviderHostApproval` type with `id`, `host`, `port`, `scope` and `approvedAt`;
  - Zod schemas for the approval input: host length, port from 1 to 65535, and scope from an enum.
- **Main process:** `ProviderConfigurationService` create, update and diagnose check the approval before the adapter is built. A new approval store supports list, approve and revoke.
- **Preload:** named methods only, following the existing rule: `providerHostApprovals.list()`, `.approve(input)`, `.revoke(id)`.
- **IPC handler:** checks the origin (main window only), validates the payload with Zod, and applies the same rate limit as the other provider calls. Approval input from any other origin is refused.
- **Error behaviour:** `host-not-approved` is returned without an address, and diagnostics do not record resolved addresses.
- **Rust port:** the same error code and schemas in `noct-core`, the handler in `noct-app`, and the DNS and pinned connection in the Rust adapter when phase 5 starts. The choice of a crate for the pinned connection needs the dependency review AGENTS.md requires; this document does not make that choice.
- **Database:** an approval table is a schema change. It would make the first migration after 32 and would raise the version that the Electron runtime refuses (`electron/database/DatabaseRuntime.ts`). See question Q4 before choosing.

## 10. Test cases

Existing, to keep:

- plain HTTP outside loopback is refused for both sources (`tests/chatCompletionsProviderAdapter.test.ts`, the loopback test);
- a mixed or private DNS answer is refused before connecting (the remote transport test);
- an IPv6-first DNS answer falls back to IPv4 through the socket (the same file).

New, written with the implementation:

1. `local` with `https://10.0.0.5` and no approval: `host-not-approved`, and no connection attempt.
2. `local` with an approved `private-network` host: connects. The same host answering `169.254.169.254` is refused even with the approval.
3. `remote` on port 8443 without an approval: refused. With an approval: connects.
4. An approved host answers 302 to `http://127.0.0.1` or to a private address: the redirect is not followed and the request fails.
5. Rebinding: the resolver returns a public address at the check and a private one on a second lookup. The socket uses only the snapshot, and the private address is never contacted.
6. Mixed answer for a `local` source: refused. Today such an answer goes through `fetch` unchecked.
7. A revoked approval: the next request is refused, and the connection is shown as disabled.
8. IPC: an approval from a non-main origin, or with an unknown field, is refused. The list response contains no credential.
9. A saved connection whose host is changed needs its key again.
10. A loopback port that is not a preset, without a `loopback` approval: refused.
11. Rust side, when the approval table lands: a schema-32 database upgrades inside one transaction, and the rollback path follows the rules of ADR 0002.

## 11. Questions for review

- **Q1.** Should a loopback port that is not a preset need an approval (proposed: yes), or stay open as it is today?
- **Q2.** Should `private-network` exist in the first version? The alternative is to refuse every non-loopback address for local sources, which closes G1 without a new scope.
- **Q3.** Is the approval granularity host and port (proposed), or host only?
- **Q4.** Where do approvals live? A new table needs a migration, which the Electron runtime refuses above version 32. Storing them in the existing settings store needs no migration, but first confirm that the settings store can hold a list.
- **Q5.** The Mistral and xAI base URLs must be confirmed (section 5) before the built-in list is final.
- **Q6.** Should "Other" keep a single form, or add a separate confirmation step that writes the host out in full?

## 12. What this document does not change

No code, no schema, no workflow and no existing document changes with this proposal. The embedding consent flow in [providers.md](providers.md) is outside its scope.

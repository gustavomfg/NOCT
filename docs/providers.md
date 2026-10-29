# Providers and models

[Português do Brasil](providers.pt-BR.md)

NOCT does not depend on a specific AI company. You connect the
provider you prefer, choose a model for each workspace and that model runs
Review, Build and Docs. Nothing else is required: no vendor account, no vendor
CLI and no subscription.

## Choosing the AI

In **Settings → AI**, connect a service and choose its model for the workspace.
The chip under the message box shows the active service and model and opens the
same page. With no model bound, a run is refused with a clear message instead of
falling back to another AI.

## Connection options

| Option | Credential | Where the model runs |
| --- | --- | --- |
| Gemini, OpenRouter, DeepSeek, Groq, Mistral, xAI | your API key | the provider's servers |
| Ollama, LM Studio | none | your machine (loopback) |
| **Other** | optional key and your address | any endpoint that implements chat completions (vLLM, llama.cpp, LiteLLM, your own server) |

OpenRouter gives access to many vendors' models, from different companies
models, through one key. Use it when you want a model that has no direct preset.

Subscription login is not offered. Consumer plans of AI vendors do not expose a
supported, general-purpose login for third-party applications, so an API key or a
local model is the way to connect. Usage of an API key is billed by that
provider; a chat subscription is a separate product.

For Ollama and LM Studio, **Service address** lets you use a different local
port; leaving it blank uses the displayed default. After saving a connection,
**Refresh models** repeats discovery on that same service, including after a
failure, without creating another connection. The entered key is cleared from
the form as soon as the connection is saved.

## Connection rules

The adapter supports model discovery, streaming, cancellation, function calling
and the `/embeddings` resource when the selected model advertises the
`embeddings` capability. Remote endpoints require HTTPS; plain HTTP is accepted
only for local loopback providers. Catalogs that do not advertise model
capabilities are treated as "not declared" rather than "unsupported". Use the
model button on a connection to refresh its catalog and switch models. Embedding
models are bound separately from the chat model.

Remote embedding use requires explicit workspace consent. If no embedding model
is bound, consent is absent, or the adapter fails, the semantic index remains
available through local lexical and structural retrieval.

## Agent tools

Provider runs use a bounded tool loop executed in the main process:

| Tool | Modes | Notes |
| --- | --- | --- |
| `list_directory`, `read_file`, `search_text` | Review, Build, Docs | Paths constrained to the authorized root; generated directories and likely secret files are skipped or refused. |
| `write_file`, `edit_file` | Build | Atomic writes; `.git`, `.nocturne` and ignored directories are refused. Optional per-change approval. |
| `run_command` | Build | No shell, 120 s timeout, bounded output, explicit approval every time; privilege escalation and destructive commands are blocked. |

Build requires a model that supports function calling; if a provider rejects a
Build request that offered tools, the error says so. Review and Docs retry once
without tools and answer from the context Nocturne selected. Tool output is
untrusted data and is sent to the selected provider like any other context.

## Credentials and diagnostics

Provider keys are encrypted with the operating-system secure storage. They are
represented in SQLite by opaque references, never returned to the renderer, and
excluded from backups and diagnostic reports.

The diagnostic panel distinguishes unavailable endpoints, rejected credentials,
missing models, timeouts, rate limits, invalid responses and insufficient API
credits. A provider failure is recoverable and does not imply that the main
application has crashed.

## Common cases

- **Insufficient credits:** add balance to the provider account or choose another provider.
- **Invalid key:** replace the provider credential.
- **Missing model:** refresh the catalog and select an available model.
- **Rate limit:** wait for the provider's limit window.
- **Local endpoint offline:** start Ollama or LM Studio and verify its loopback address.
- **Build refused with tools:** the model does not support function calling; choose another model or use Review.


## Connection troubleshooting

OpenRouter uses `https://openrouter.ai/api/v1`; Nocturne appends `/models`
and `/chat/completions`. Remote connections validate all DNS results and let the
socket select a reachable IPv4 or IPv6 address from that validated snapshot,
without resolving the hostname again or following redirects.

If saving a key fails, unlock the operating system keychain. On Linux, Nocturne
requires Secret Service or KWallet; it never falls back to plaintext storage.
On desktops without Chromium's native keyring selection (such as Hyprland or
Sway), Nocturne detects installed D-Bus services and selects Secret Service or
KWallet before Electron initializes. Recognized desktops and explicit
`--password-store` switches keep their selection. Installing or unlocking the
keyring may require restarting the app; allow Nocturne access if KWallet prompts.
Without secure storage, saving credentials remains unavailable.
The public model catalog does not prove that a key has inference access or credits.
HTTP 402 means insufficient credits; errors sent after streaming begins also
fail the run, preserving any partial response as incomplete.

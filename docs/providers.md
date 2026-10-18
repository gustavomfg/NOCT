# Providers and models

[Português do Brasil](providers.pt-BR.md)

## Choosing the AI

Nocturne does not depend on a specific AI. In Settings → AI, connect a service,
choose a model for the workspace and the selection becomes the engine for
Review, Build and Docs. The chip under the message box shows the active service
and model and opens the same page. With no selection, a run is refused with a
clear message instead of falling back to another AI.

## OpenAI-compatible connections

Presets cover OpenAI, Google Gemini, OpenRouter, DeepSeek, Groq, Mistral, xAI,
Ollama and LM Studio. **Other** accepts any endpoint that implements the
compatible `/models` and `/chat/completions` resources (vLLM, llama.cpp,
LiteLLM…); its API key is optional and a loopback URL is treated as local.
Remote endpoints require HTTPS. Plain HTTP is accepted only for local loopback
providers.

The adapter supports model discovery, streaming, cancellation, function calling
and the OpenAI-compatible `/embeddings` resource when the selected model
advertises the `embeddings` capability. Catalogs that do not advertise model
capabilities are treated as "not declared" rather than "unsupported". Use the
model button on a connection to refresh its catalog and switch models.
Embedding models are bound separately from the chat model.

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
without tools and answer from the context Nocturne selected. Tool output is untrusted data and is sent to the selected
provider like any other context.

## ChatGPT account (optional)

A ChatGPT account can be connected through an installed Codex CLI. Nocturne does
not receive the account password or token: it uses the authenticated Codex App
Server and lists the models returned for that account. Choosing a Codex model
makes Codex the engine for all modes until another model is chosen. A ChatGPT
subscription does not provide OpenAI Platform API credits.

## Credentials and diagnostics

Provider keys are encrypted with the operating-system secure storage. They are
represented in SQLite by opaque references, never returned to the renderer, and
excluded from backups and diagnostic reports.

The diagnostic panel distinguishes unavailable endpoints, rejected credentials,
missing models, timeouts, rate limits, invalid responses and insufficient API
credits. A provider failure is recoverable and does not imply that the main
application has crashed.

## Common cases

- **Insufficient credits:** add balance to the API account or choose another
  provider; a ChatGPT plan and API billing are separate.
- **Invalid key:** replace the provider credential.
- **Missing model:** refresh the catalog and select an available model.
- **Rate limit:** wait for the provider's limit window.
- **Local endpoint offline:** start Ollama or LM Studio and verify its loopback
  address.

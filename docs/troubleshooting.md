# Troubleshooting

[Português do Brasil](troubleshooting.pt-BR.md)

## The app or project does not open

Confirm that the project folder exists and is readable. If it moved, choose
**Locate folder** and authorize the new root. History remains local and readable
while the old root is unavailable, but access to files, Git and AI stays blocked
until explicit reselection.

## No AI is selected or a model is missing

Open **Settings → AI**, connect a provider and choose its model for the
workspace. Nocturne refuses a run with a clear message while no model is bound,
rather than falling back to another AI. If the provider lists no models, check
its key and endpoint, then refresh the model list with the model button on its
connection.

## A provider fails

Check the endpoint, HTTPS/loopback rule, credential and model catalog. For
Ollama or LM Studio, start the local service. The diagnostic distinguishes
authentication, credits, rate limits, timeout, unavailable endpoint and invalid
response.

## A run was interrupted

Read the error summary for what was preserved and use **Try again** when the
operation is eligible. Build rollback is offered only when its snapshot and
reported file boundaries make a safe rollback possible.

## Recovery or update failed

Do not delete local database or recovery artifacts. Use the guided recovery or
import a verified backup. For an interrupted update, resume the download; the
current installed version and local data remain the recovery baseline until the
new package is validated.

## Diagnostics

Use **Settings > Diagnostics** to copy or export a sanitized report. Review any
log before sharing it. Reports must not contain credentials, prompts, project
contents, diffs or private paths.

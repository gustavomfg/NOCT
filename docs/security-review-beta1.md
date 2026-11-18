# Security review of beta 1 (OpenRouter provider)

This document reviews every `[security-review]` commit of the beta 1 round. For each commit it states what the commit
does, the threat it covers, the tests that prove it, and the code that carries the control. It also lists the decisions
taken without the owner (D1 to D9), the answers to the network questions, and the findings of this review with their
state. It is written before the code of the next items, so each finding names the commit that closes it, or says that it
is open.

Scope: the Rust shell (`crates/noct-core`, `crates/noct-db`, `crates/noct-secrets`, `crates/noct-app`, `crates/noct-ui`) on
branch `migration/rust`. Nothing in the Electron product or its TypeScript was reviewed or changed.

## Commits of the round

| Commit | Summary | Threat it covers |
| --- | --- | --- |
| `ef26329` | Exact-key read of a hidden setting | A key stored under `maintenance.` is read by the shell without showing up in the settings read |
| `8919c48` | Provider key contract, write-only input | A key reaching a debug log, a reply or the page through its type |
| `2cd2350` | Save, check and remove the key through the vault | A key kept in the database, returned to the page, or written before validation |
| `5105ceb` | Network policy for the provider endpoint (pure) | Calls to hosts and ports that are not approved, to private addresses, through redirects |
| `c68771e` | Context rules: ignored folders, sensitive files, size budget | Sending secrets, credentials or large files to a provider |
| `f5adb0f` | Per-project consent contract, version `openrouter-v1` | Sending a project's context without an explicit, recorded consent |
| `c1056fc` | Consent commands for a saved project | Consent recorded for a project the shell does not know, or read from the settings page |
| `0df77cb` | Key and consent section in settings | The key staying in the page after a save, or echoed back in the DOM |

## Per commit

### `ef26329` read one hidden setting key

- **What it does.** `Settings::value(key)` runs `SELECT value FROM settings WHERE key = ?1`. `Settings::get()` still
  hides the `maintenance.` prefix, so the exact-key read is the only way the key reference is read back.
- **Threat.** The reference is not secret, but the settings read is shown to the page. If the reference used a visible key
  it would appear there. The exact read keeps the hiding rule in one place.
- **Tests.** `settings_value_reads_a_hidden_key_that_the_settings_read_does_not_return` (`crates/noct-db/tests/repositories.rs`).
- **Essential code.** `crates/noct-db/src/repositories/settings.rs`, `value`.

### `8919c48` provider key contract with a write-only input

- **What it does.** `ProviderKeyInput { provider, key }` is `deny_unknown_fields`. Its `Debug` prints `"<redacted>"` for the
  key. `validated()` trims the key and accepts only visible ASCII with a length of 8 to 512 characters
  (`limits::provider_key`). Its error names the field `key`, never the value. `ProviderKeyStatus` has no key field.
- **Threat.** A key that reaches a log, a panic message or a reply, or an input that carries another provider's field.
- **Tests.** `debug_output_never_contains_the_key`, `a_key_with_control_characters_or_inner_spaces_is_refused`,
  `a_key_outside_the_length_bounds_is_refused`, `input_rejects_unknown_fields_and_other_providers`,
  `the_status_serializes_without_any_key_field`, `a_visible_key_is_accepted_without_its_surrounding_spaces`
  (`crates/noct-core/tests/provider_key.rs`).
- **Essential code.** `crates/noct-core/src/ai/provider_key.rs`:

  ```rust
  impl fmt::Debug for ProviderKeyInput {
      fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
          formatter
              .debug_struct("ProviderKeyInput")
              .field("provider", &self.provider)
              .field("key", &"<redacted>")
              .finish()
      }
  }
  ```

### `2cd2350` save, check and remove the key through the vault

- **What it does.** `credentials::save` validates the input before it opens the vault, wraps the key in `SecretString`, and
  writes the secret first and its reference second. `credentials::remove` deletes the secret first and then clears the
  reference. `credentials::status` opens the vault only when a reference is saved. `map_vault` maps every vault failure to a
  fixed code, so the vault's own text never reaches the page.
- **Threat.** A key stored in the database, a key returned by status, a write that leaves a reference without its secret, a
  remove that reports success while the secret remains, and a vault message that names the key.
- **Tests.** `saving_keeps_the_secret_in_the_vault_and_only_its_reference_in_settings`,
  `an_invalid_key_is_refused_before_the_vault_opens_and_nothing_is_saved`, `a_failed_secret_write_leaves_no_reference_behind`,
  `status_without_a_saved_key_answers_not_configured_and_never_opens_the_vault`,
  `the_status_reads_the_vault_and_reports_a_secret_that_was_lost`, `removing_deletes_the_secret_and_clears_the_reference`,
  `removing_without_a_saved_key_does_not_open_the_vault`, `no_reply_or_error_carries_the_key`,
  `vault_failures_map_to_the_codes_the_interface_shows` (`crates/noct-app/tests/credentials.rs`).
  ACL: `the_provider_key_commands_are_granted_and_refuse_bad_requests_before_the_store` and
  `the_provider_key_commands_are_refused_to_another_window` (`crates/noct-app/tests/commands_acl.rs`).
- **Essential code.** `crates/noct-app/src/credentials.rs`, `save`:

  ```rust
  let ProviderKeyInput { provider, key } =
      input.validated().map_err(|_| CommandError::InvalidInput)?;
  let secret = SecretString::from(key);
  let vault = open()?;
  match reference(store)? {
      Some(current) if vault.has(&current).map_err(map_vault)? => {
          vault.replace(&current, &secret).map_err(map_vault)?;
      }
      _ => {
          // The secret is written first, then its reference.
          let created = vault.create(&secret).map_err(map_vault)?;
          write_reference(store, &created.to_string())?;
      }
  }
  ```

  `remove`:

  ```rust
  if let Some(current) = reference(store)? {
      open()?.delete(&current).map_err(map_vault)?;
      write_reference(store, "")?;
  }
  ```

- **Vault design.** Unchanged. The envelope, the master key in the operating system keyring and the index are those of
  ADR 0002. This round only calls the existing API.

### `5105ceb` network policy for the provider endpoint

- **What it does.** A pure module in `noct-core`. `check_endpoint` allows only the approved host, port and scheme
  (`BUILTIN_APPROVALS`, OpenRouter on 443). `verify_addresses` resolves the name once through an injected
  `AddressResolver`, or uses the IP literal, and checks every returned address against the class allowed for the source.
  `check_status` refuses every 3xx reply.
- **Threat.** A call to an unapproved host or port, a name that resolves to a private, loopback, link-local or metadata
  address, a rebinding between the check and the connection, and a redirect into a private network.
- **Tests.** `only_the_approved_host_and_port_are_called`, `a_url_with_credentials_a_query_a_fragment_or_a_bad_host_or_port_is_refused`,
  `the_address_classes_of_ipv4_follow_the_routable_ranges`, `the_address_classes_of_ipv6_follow_the_routable_ranges`,
  `a_remote_source_accepts_public_addresses_only_and_one_bad_address_refuses_the_set`,
  `a_name_is_resolved_once_and_the_first_answer_is_the_snapshot`, `an_ip_literal_is_its_own_answer_and_the_resolver_is_not_called`,
  `every_redirect_is_refused_and_the_other_statuses_reach_the_adapter`, `the_refusal_codes_are_stable_and_name_no_address`
  (`crates/noct-core/tests/provider_network.rs`).
- **Essential code.** `crates/noct-core/src/ai/network.rs`:

  ```rust
  pub fn allowed_class(source: ProviderSource) -> AddressClass {
      match source {
          ProviderSource::Remote => AddressClass::Public,
          ProviderSource::Local => AddressClass::Loopback,
      }
  }

  pub fn check_addresses(addresses: &[IpAddr], source: ProviderSource) -> Result<(), EndpointError> {
      if addresses.is_empty() {
          return Err(EndpointError::NoAddress);
      }
      let allowed = allowed_class(source);
      if addresses.iter().all(|address| classify(*address) == allowed) {
          Ok(())
      } else {
          Err(EndpointError::AddressNotAllowed)
      }
  }

  pub fn check_status(status: u16) -> Result<(), EndpointError> {
      if (300..=399).contains(&status) {
          Err(EndpointError::RedirectRefused)
      } else {
          Ok(())
      }
  }
  ```

- **Limits of this commit.** It is a policy. No connection exists yet, so the pinned connection is not in this commit. See
  the transport section below for the commit that adds it.

### `c68771e` context rules for what a provider call may carry

- **What it does.** `is_inside_project` refuses absolute paths, drive letters, `..` and empty segments.
  `is_ignored_relative_path` skips dependency and build folders on any segment. `is_potentially_sensitive_path` marks a file
  sensitive when its name is in the fixed list, contains `secret`, `credential` or `token`, or ends with `.pem`, `.key`,
  `.p12`, `.pfx`, `.crt` or `.cer`. `select` includes files in order, excludes sensitive, ignored or oversized files, and
  stops at the first file that does not fit the budget.
- **Threat.** A key file, a token file, a file outside the project, a large file, or a file that is sent because the budget
  was already exceeded.
- **Tests.** `a_sensitive_file_is_never_included_whatever_its_size`, `a_sensitive_extension_is_sensitive_including_a_file_named_after_it`,
  `a_name_that_contains_secret_credential_or_token_is_sensitive`, `the_fixed_secret_names_are_sensitive_whatever_their_case_or_folder`,
  `a_path_must_name_a_place_inside_the_project`, `a_file_over_the_per_file_cap_is_excluded_and_does_not_use_the_budget`,
  `the_selection_never_passes_the_budget_and_stops_at_the_first_file_that_does_not_fit`,
  `a_smaller_later_file_is_not_sent_once_the_budget_has_stopped_the_selection` (`crates/noct-core/tests/provider_context.rs`).
- **Essential code.** `crates/noct-core/src/ai/context.rs`:

  ```rust
  pub fn is_potentially_sensitive_path(relative: &str) -> bool {
      let normalized = normalize(relative);
      let base = normalized
          .trim_end_matches('/')
          .rsplit('/')
          .next()
          .unwrap_or_default()
          .to_lowercase();
      SENSITIVE_NAMES.contains(&base.as_str())
          || SENSITIVE_PARTS.iter().any(|part| base.contains(part))
          || SENSITIVE_EXTENSIONS
              .iter()
              .any(|extension| base.ends_with(extension))
  }
  ```

- **Deliberate divergence from TypeScript.** A file named exactly `.pem` (or another listed extension) is sensitive in the
  Rust port. The TypeScript rule in `electron/semantic-index/SemanticPrivacyPolicy.ts` takes the extension with
  `path.posix.extname`, which is empty for `.pem`, so the same name is not sensitive there. This is a bug in the TypeScript
  side, recorded in the section "Findings" below. The TypeScript code is not changed by this review.

### `f5adb0f` per-project consent contract

- **What it does.** `CONSENT_VERSION = "openrouter-v1"`. `ProjectConsentRequest` takes one path, checked for length in
  characters (1 to 4000). `ProjectConsentStatus` carries the path, the provider and one flag, and nothing else.
- **Threat.** A consent that names a provider or a version it does not cover, a path counted in bytes that lets a longer
  path through, and a reply that carries project content.
- **Tests.** `the_consent_version_names_the_beta_one_scope`, `a_path_of_one_to_the_maximum_characters_is_accepted`,
  `a_path_is_counted_in_characters_not_bytes`, `the_reply_carries_the_path_the_provider_and_the_flag_only`
  (`crates/noct-core/tests/provider_consent.rs`).

### `c1056fc` consent commands for a saved project

- **What it does.** `consent::status`, `grant` and `revoke` store one setting per project under
  `maintenance.consent.openrouter.<path>`. Revoking writes an empty value. `ensure_saved` refuses a path that is not a saved
  project with `unknown-workspace`.
- **Threat.** A consent for a project the shell does not know, and a consent that the settings read exposes.
- **Tests.** `the_consent_commands_follow_a_saved_project_and_stay_out_of_the_settings_read`
  (`crates/noct-app/tests/commands_acl.rs`).
- **Essential code.** `crates/noct-app/src/consent.rs`:

  ```rust
  fn ensure_saved(store: &Store, workspace: &str) -> Result<(), CommandError> {
      let saved = services::workspace_rows(store)?
          .iter()
          .any(|row| row.path == workspace);
      if saved {
          Ok(())
      } else {
          Err(CommandError::UnknownWorkspace)
      }
  }
  ```

- **Finding.** `ensure_saved` does not check that the project is authorized. See "Findings", F1.

### `0df77cb` key and consent section in settings

- **What it does.** The key field is `type="password"` with `autocomplete="off"`. Its draft is emptied before the save call
  is sent, whatever the reply. The page shows only "Chave configurada." or "Nenhuma chave configurada.". The consent asks
  for confirmation, and the confirmation text names what may be sent and the budget.
- **Threat.** The key staying in the DOM, the input or the page after a save, and a consent recorded without the person
  reading what it covers.
- **Tests.** Browser scenarios `provider-key-not-configured`, `provider-key-configured`, `provider-key-save-refused` and
  `provider-reply-outside-the-contract` in `scripts/check-shell-ui.mjs`. Their assertions include: the field is a password
  field, the field is emptied when the key is sent, the key is in no attribute, text or script of the page after a save,
  a refused save leaves the status unchanged, and consent is granted and revoked once each, after confirmation. The
  inline-style guard `the_interface_sets_no_inline_style` runs in `crates/noct-app/tests/shell_policy.rs`.
- **Essential code.** `crates/noct-ui/src/provider_panel.rs`, `save_key`:

  ```rust
  let save_key = move || {
      let key = draft.get_untracked();
      draft.set(String::new());
      key_busy.set(true);
      key_error.set(None);
      ipc::call_with(provider::KEY_SAVE, &provider::key_save_payload(&key), move |outcome| {
          // ...
      });
  };
  ```

## Redaction: where a key could appear, and what stops it

| Place | Control | Evidence |
| --- | --- | --- |
| `Debug` of the input | `<redacted>` | `debug_output_never_contains_the_key` |
| The vault secret in memory | `SecretString` from `secrecy` | `noct-secrets` re-export (decision D3) |
| Replies of the key commands | No key field in `ProviderKeyStatus` | `the_status_serializes_without_any_key_field`, `no_reply_or_error_carries_the_key` |
| Shell error messages | Fixed texts in `crates/noct-app/src/error.rs`; no value is interpolated | `vault_failures_map_to_the_codes_the_interface_shows` |
| Vault error messages | `VaultError::message()` returns fixed text | `crates/noct-secrets/src/error.rs` |
| Deserialization errors of the key input | `serde_json` names the field, not the value, and the input is `deny_unknown_fields` | `input_rejects_unknown_fields_and_other_providers` |
| The page | The draft is cleared before the call; the DOM is searched after a save | `provider-key-configured` scenario |
| Logs | The key is never passed to a log call in the round's code | Review of the call sites of `credentials::save` |

## Findings

- **F1. Consent for an unauthorized project. Open, fixed in this batch.** `ensure_saved` accepts a saved path whose
  `authorized` flag is false. The mission requires that restored projects stay unauthorized until reselected. The consent
  itself reads no file, so this is not a data leak, but a consent recorded for a project the person has not reselected is
  not allowed. Fix: require `authorized` and an available project, with a test.
- **F2. Orphan secrets after a crash. Open, fixed in this batch (D9).** A crash between `vault.create` and the reference
  write leaves a secret that no reference names. `vault.prune` removes it. Calling `prune` with the references the settings
  name, under the provider-key lock, is safe with the vault's own design.
- **F3. Race between two saves. Open, fixed in this batch.** Two concurrent saves can both see no reference and each create
  a secret. One reference is then overwritten and its secret becomes an orphan (F2). Fix: one lock around the key commands.
- **F4. `.pem` divergence. Recorded as a bug of the TypeScript side.** See `c68771e`. The Rust port keeps the stricter
  rule. The TypeScript code is not changed in this round.
- **F5. No HTTP client yet. Open, addressed by the transport commit of this batch.** The policy of `5105ceb` is not
  enforced by any connection until the transport exists. Until then no request to a provider is possible.

## Decisions taken without the owner (D1 to D9)

- **D1.** The key reference is stored at `maintenance.secret.openrouter.reference`. Both settings readers hide the
  `maintenance.` prefix (`electron/database/SettingsRepository.ts` and `Settings::get`), so the reference never reaches
  the settings the page reads. `providers.openrouter.reference` was rejected because the TypeScript reader would list it.
- **D2.** `noct-app` depends on `noct-secrets` by path. No external crate was added for this.
- **D3.** `noct-secrets` re-exports `SecretString` and `ExposeSecret`, so `noct-app` has no `secrecy` dependency of its own.
- **D4.** A status read with no saved reference does not open the vault, so it never asks the operating system for the
  master key.
- **D5.** Save writes the secret before the reference. Remove deletes the secret before it clears the reference. If clearing
  the reference fails, the reference stays and a retry finishes the removal.
- **D6.** The key is 8 to 512 characters of visible ASCII after trimming. An error names the field, never the value.
- **D7.** The ACL tests never send a valid save, so they never touch the keyring of the machine that runs them. Service
  tests use an in-memory vault.
- **D8.** `provider_key_status`, `provider_key_save` and `provider_key_remove` are granted to the `main` window only.
- **D9.** Orphan secrets are removed by `vault.prune` (see F2). This round implements that call, under the key lock.

## Answers to the network questions (Q1 to Q6)

The most restrictive answer was taken for each, as section 13 of `docs/provider-allowlist-design.md` records.

- **Q1.** No loopback endpoint is in the list, so a loopback port that is not approved is refused.
- **Q2.** A local source reaches loopback only. The `private-network` scope is not implemented. This closes G1 in the Rust port.
- **Q3.** Approvals are per host and per port. A host approved on 443 does not reach 8443.
- **Q4.** No approval table and no migration. The approvals are the built-in list in `network.rs`. The per-project consent is
  stored under `maintenance.consent.openrouter.<path>`.
- **Q5.** The built-in list holds OpenRouter alone.
- **Q6.** Beta 1 has no "other" endpoints. The form that would accept one does not exist.

## Dependency decision: provider HTTP client

Approved by the owner: `reqwest` with the same version the lockfile already resolves (`=0.13.5`), `default-features = false`,
features `rustls-no-provider`, `json` and `stream`, and `ring` as the TLS provider. `aws-lc` is not used.

Checks, run on the tree of this commit:

- `cargo tree -i aws-lc-rs` and `cargo tree -i aws-lc-sys`: no match. No `aws-lc` crate is in the graph.
- `cargo tree -i ring`: one provider, `ring 0.17.14`, under `rustls 0.23.46`. `cargo tree -d` shows no duplicate of `rustls`,
  `reqwest`, `hyper-rustls`, `tokio-rustls` or `rustls-webpki`.
- `cargo deny check`: advisories, bans, licences and sources all ok.
- `cargo audit`: exit 0. The seven allowed warnings are the advisories already listed in `docs/rust-dependency-advisories.md`.
- The lockfile gained two edges, `noct-app` to `reqwest` and `noct-app` to `rustls`, and no package.

Two consequences the code must handle:

- **`rustls-no-provider` needs an installed provider.** Without one, reqwest panics when it builds a client. The provider
  client installs `ring` as the process default once, and ignores the error that means it is already installed.
- **System proxy.** `tauri-plugin-updater` turns on reqwest's `system-proxy` feature, and Cargo unifies features across the
  workspace, so the feature is on for the provider client too. A system proxy would carry the request to an address the
  policy never checked. The provider client therefore calls `no_proxy()`.

## Status of this document

The findings and decisions above are the state at the time the document was first published. This section records what
the continuation of beta 1 changed, with the commit of each change. The mission report is in `docs/status.md`.

### Findings

| Finding | State | Commit | Test or evidence |
| --- | --- | --- | --- |
| F1. Consent for an unauthorized project | Closed | `fa86fef` | `consent_is_refused_for_a_saved_project_that_is_not_authorized` (`tests/commands_acl.rs`) |
| F2. Orphan secrets after a crash (D9) | Closed | `3c74b97` | `an_orphan_left_by_a_stopped_save_is_removed_by_the_next_save`, `removing_a_key_also_removes_its_leftovers`, `a_failed_cleanup_does_not_fail_the_save` (`tests/credentials.rs`) |
| F3. Race between two saves | Closed | `628e799` | `two_key_saves_at_the_same_time_run_one_after_the_other` (`tests/credentials.rs`); the lock was removed in a mutation check and the test failed |
| F4. `.pem` divergence | Open, a bug of the TypeScript side. Not fixed here. The Rust port keeps the stricter rule. | `c68771e` (record) | Recorded only; see the note below |
| F5. No HTTP client | Closed | `89b81bc` (dependency), `5907f25` (transport), `f1a386e` (strict SSE and protocol) | 19 transport tests against a loopback server (`tests/provider_transport.rs`) |

**F4, for the TypeScript side.** The TypeScript code accepts a `.pem` value that the Rust port refuses. This is a bug of the
TypeScript side: the stricter rule of the Rust port is the one that applies, and the TypeScript code was not changed in
this continuation, as the mission requires.

### Decisions taken in the continuation (D10 to D17)

- **D10.** `models_set_bindings` keeps its full-row replace, because the TypeScript side depends on it. The new commands
  `models_bindings_get` and `models_set_default_model` are separate and write only the default, in one transaction that
  keeps the embedding binding (`saving_the_default_model_keeps_the_other_bindings_of_the_row`).
- **D11.** The model identifier is checked in the core before a project is read: its size, its characters and its
  `vendor/name` shape. `:free` is a filter and a suggestion in the interface only. Beta 1 uses a text field.
- **D12.** The transport uses no system proxy (`no_proxy`), because the updater enables a system proxy for the whole
  workspace. Redirects are refused. The host is pinned to the addresses the policy approved. Connect, read and total timeouts
  apply, with a 30 s limit for a stalled stream and a 64 KiB limit for each event. An environment-based proxy test is not
  written: edition 2024 makes `set_var` unsafe, and the setting is a builder call.
- **D13.** The chat stays off: `PROVIDER_CHAT_ENABLED = false`. A compile-time assertion in
  `tests/provider_chat_flow.rs` fails the build when the switch is turned on, until that line is changed on purpose. See
  `docs/provider-chat.md`.
- **D14.** The beta 1 context is the text files at the top level of the project that `context::select` accepts: UTF-8,
  32 KiB per file and 128 KiB in total. Subfolders and symbolic links are not read.
- **D15.** Saving an answer stores only its text, through `ai_save_assistant`, in a conversation of the project. The
  conversation is created on the first save and reused for that project. The question is not stored by this path.
- **D16.** The stub composer at the bottom of the window ("the questions are not answered in this slice") was replaced by
  the chat section in Revisar, and its styles were removed. The owner should check the design of that area.
- **D17.** A channel's callback is kept for the life of the page, one small closure per question. The shell can send the end
  of a channel after the last answer event, so the callback cannot be dropped at that point.

### Stops, not done

- **GET /models (the provider's model list).** Not implemented. It is a provider network call in the production flow, which
  needs owner review first. The model field is a text field in beta 1.
- **The chat in production.** Not enabled. It needs the owner's review of what is sent and of the consent text, and a real
  call with a test key on each system.
- **Windows and macOS.** Not verified in a window. Nothing in this continuation was run on those systems by hand.

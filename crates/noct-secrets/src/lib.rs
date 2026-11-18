//! Provider credentials for NOCT.
//!
//! The secrets are sealed in an encrypted file (XChaCha20-Poly1305). The operating system credential
//! store holds only the 32-byte master key that opens it: Secret Service on Linux, Keychain on macOS
//! and Credential Manager on Windows. The application keeps two plain things: opaque UUID
//! references, which SQLite may store, and a non-secret index of those references on disk, so that
//! `has` and the pruning of unused secrets never need the operating system store.
//!
//! Secrets are never written to SQLite, to logs or to the renderer. The file holds only
//! ciphertext, and the store never receives a secret. There is no plaintext fallback. When the
//! operating system store cannot be opened, the vault cannot be used.
//!
//! Operations block on the operating system store and on the filesystem. Call them from a blocking
//! task, never directly from an async command handler.

mod document;
mod envelope;
mod error;
mod file;
mod index;
mod platform;
mod reference;
mod vault;

pub use error::VaultError;
pub use keyring_core::CredentialStore;
pub use platform::default_store;
pub use reference::CredentialReference;
pub use secrecy::{ExposeSecret, SecretString};
pub use vault::{MASTER_KEY_ACCOUNT, ProviderCredentialVault, SERVICE_NAME};

//! Provider credentials for NOCT.
//!
//! Secrets live only in the operating system credential store: Secret Service on
//! Linux, Keychain on macOS and Credential Manager on Windows. They are never written
//! to SQLite, to files, to logs or to the renderer. The application keeps two things:
//! opaque UUID references, which SQLite may store, and a non-secret index of those
//! references on disk, so that unused secrets can be found and removed.
//!
//! There is no plaintext or application-encrypted fallback. When the operating system
//! store cannot be opened, the vault cannot be created.
//!
//! Operations block on the operating system store. Call them from a blocking task,
//! never directly from an async command handler.

mod error;
mod index;
mod platform;
mod reference;
mod vault;

pub use error::VaultError;
pub use keyring_core::CredentialStore;
pub use platform::default_store;
pub use reference::CredentialReference;
pub use vault::{ProviderCredentialVault, SERVICE_NAME};

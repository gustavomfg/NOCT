use std::sync::Arc;

use keyring_core::CredentialStore;

use crate::VaultError;

/// Opens the credential store of the operating system.
///
/// Linux uses the freedesktop Secret Service over D-Bus (GNOME Keyring, KWallet or any
/// compatible provider). There is no plaintext store on any platform.
pub fn default_store() -> Result<Arc<CredentialStore>, VaultError> {
    open_platform_store()
}

#[cfg(target_os = "linux")]
fn open_platform_store() -> Result<Arc<CredentialStore>, VaultError> {
    let store: Arc<CredentialStore> = zbus_secret_service_keyring_store::Store::new()
        .map_err(|_| VaultError::SecureStorageUnavailable)?;
    Ok(store)
}

#[cfg(target_os = "macos")]
fn open_platform_store() -> Result<Arc<CredentialStore>, VaultError> {
    let store: Arc<CredentialStore> = apple_native_keyring_store::keychain::Store::new()
        .map_err(|_| VaultError::SecureStorageUnavailable)?;
    Ok(store)
}

#[cfg(target_os = "windows")]
fn open_platform_store() -> Result<Arc<CredentialStore>, VaultError> {
    let store: Arc<CredentialStore> = windows_native_keyring_store::Store::new()
        .map_err(|_| VaultError::SecureStorageUnavailable)?;
    Ok(store)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn open_platform_store() -> Result<Arc<CredentialStore>, VaultError> {
    Err(VaultError::SecureStorageUnavailable)
}

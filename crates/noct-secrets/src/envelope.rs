//! Envelope encryption for the provider credential document.
//!
//! The operating system store holds one 32-byte master key. The secrets are sealed with it, so
//! the store never receives a secret and the file holds only ciphertext. The cipher is
//! XChaCha20-Poly1305. Its 192-bit nonce is long enough to draw at random for every write, so no
//! counter has to be kept, and its pure Rust implementation does not depend on the CPU's AES
//! instructions, which makes its behavior the same on the three platforms.
//!
//! Sealed layout: one version byte, the 24-byte nonce, then the ciphertext and its 16-byte tag.
//! The associated data is the purpose string followed by the version byte. A changed version, a
//! changed purpose, or bytes read as another file all fail authentication.

use std::fmt;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use secrecy::{ExposeSecret, SecretBox};
use zeroize::Zeroizing;

use crate::VaultError;

pub(crate) const KEY_BYTES: usize = 32;
const VERSION: u8 = 1;
const NONCE_BYTES: usize = 24;
const TAG_BYTES: usize = 16;
const PURPOSE: &[u8] = b"noct/provider-credentials";
/// Bytes that a sealed document adds to its plaintext.
pub(crate) const OVERHEAD_BYTES: usize = 1 + NONCE_BYTES + TAG_BYTES;

/// The master key. Its bytes are zeroed when it is dropped, and its `Debug` output is redacted.
pub(crate) struct MasterKey(SecretBox<[u8; KEY_BYTES]>);

impl MasterKey {
    /// A new key from the operating system random source. The key is written straight into its
    /// heap allocation, so no copy of it is left on the stack.
    pub(crate) fn generate() -> Result<Self, VaultError> {
        let mut bytes = Box::new([0_u8; KEY_BYTES]);
        getrandom::fill(&mut bytes[..]).map_err(|_| VaultError::WriteFailed)?;
        Ok(Self(SecretBox::new(bytes)))
    }

    /// A key read back from the operating system. Any other length is corrupt.
    pub(crate) fn from_stored(bytes: Zeroizing<Vec<u8>>) -> Result<Self, VaultError> {
        if bytes.len() != KEY_BYTES {
            return Err(VaultError::CorruptStore);
        }
        let mut key = Box::new([0_u8; KEY_BYTES]);
        key.copy_from_slice(&bytes);
        Ok(Self(SecretBox::new(key)))
    }

    /// The key bytes, for writing to the operating system store and for sealing.
    pub(crate) fn expose(&self) -> &[u8; KEY_BYTES] {
        self.0.expose_secret()
    }
}

impl fmt::Debug for MasterKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MasterKey([REDACTED])")
    }
}

/// Seals `plaintext` under `key`. Every call draws a new nonce.
pub(crate) fn seal(key: &MasterKey, plaintext: &[u8]) -> Result<Vec<u8>, VaultError> {
    seal_as(key, plaintext, VERSION)
}

/// Seals with `aad_version` in the associated data. [`seal`] always passes the format version;
/// the tests pass another value to show that the version is authenticated.
fn seal_as(key: &MasterKey, plaintext: &[u8], aad_version: u8) -> Result<Vec<u8>, VaultError> {
    let mut nonce = [0_u8; NONCE_BYTES];
    getrandom::fill(&mut nonce).map_err(|_| VaultError::WriteFailed)?;
    let cipher = cipher(key)?;
    let aad = associated_data(aad_version);
    let ciphertext = cipher
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| VaultError::WriteFailed)?;
    let mut sealed = Vec::with_capacity(OVERHEAD_BYTES + plaintext.len());
    sealed.push(VERSION);
    sealed.extend_from_slice(&nonce);
    sealed.extend_from_slice(&ciphertext);
    Ok(sealed)
}

/// Opens bytes sealed by [`seal`]. A wrong key, a changed byte, an unknown version or a truncated
/// file all fail closed as corrupt; the error never says which check failed. The plaintext comes
/// back in a buffer that is zeroed when it is dropped.
pub(crate) fn open(key: &MasterKey, sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    if sealed.len() < OVERHEAD_BYTES || sealed[0] != VERSION {
        return Err(VaultError::CorruptStore);
    }
    let nonce: [u8; NONCE_BYTES] = sealed[1..1 + NONCE_BYTES]
        .try_into()
        .map_err(|_| VaultError::CorruptStore)?;
    let cipher = cipher(key)?;
    let aad = associated_data(VERSION);
    let plaintext = cipher
        .decrypt(
            &XNonce::from(nonce),
            Payload {
                msg: &sealed[1 + NONCE_BYTES..],
                aad: &aad,
            },
        )
        .map_err(|_| VaultError::CorruptStore)?;
    Ok(Zeroizing::new(plaintext))
}

fn cipher(key: &MasterKey) -> Result<XChaCha20Poly1305, VaultError> {
    XChaCha20Poly1305::new_from_slice(key.expose()).map_err(|_| VaultError::CorruptStore)
}

/// The purpose string, then the format version. Changing either one fails authentication.
fn associated_data(version: u8) -> [u8; PURPOSE.len() + 1] {
    let mut data = [0_u8; PURPOSE.len() + 1];
    data[..PURPOSE.len()].copy_from_slice(PURPOSE);
    data[PURPOSE.len()] = version;
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_bytes_open_with_the_same_key_only() {
        let key = MasterKey::generate().expect("key");
        let other = MasterKey::generate().expect("other key");
        let sealed = seal(&key, b"segredo").expect("seal");

        assert_eq!(open(&key, &sealed).expect("open").as_slice(), b"segredo");
        assert!(matches!(
            open(&other, &sealed),
            Err(VaultError::CorruptStore)
        ));
    }

    #[test]
    fn every_byte_is_authenticated() {
        let key = MasterKey::generate().expect("key");
        let sealed = seal(&key, b"segredo").expect("seal");

        for index in 0..sealed.len() {
            let mut changed = sealed.clone();
            changed[index] ^= 0x01;
            assert!(
                matches!(open(&key, &changed), Err(VaultError::CorruptStore)),
                "byte {index} changed"
            );
        }
        assert!(matches!(
            open(&key, &sealed[..OVERHEAD_BYTES - 1]),
            Err(VaultError::CorruptStore)
        ));
    }

    #[test]
    fn the_format_version_is_part_of_the_associated_data() {
        let key = MasterKey::generate().expect("key");
        // The header still says VERSION, so only the authenticated copy of the version differs.
        let sealed = seal_as(&key, b"segredo", VERSION + 1).expect("seal");

        assert!(matches!(open(&key, &sealed), Err(VaultError::CorruptStore)));
    }

    #[test]
    fn the_nonce_changes_on_every_seal() {
        let key = MasterKey::generate().expect("key");
        let first = seal(&key, b"mesmo").expect("seal");
        let second = seal(&key, b"mesmo").expect("seal");

        assert_ne!(first[1..1 + NONCE_BYTES], second[1..1 + NONCE_BYTES]);
        assert_ne!(first, second);
    }

    #[test]
    fn a_key_of_the_wrong_length_is_corrupt() {
        let stored = Zeroizing::new(vec![7_u8; KEY_BYTES - 1]);
        assert!(matches!(
            MasterKey::from_stored(stored),
            Err(VaultError::CorruptStore)
        ));
    }

    #[test]
    fn the_master_key_is_redacted_in_debug_output() {
        let key = MasterKey::generate().expect("key");
        let rendered = format!("{key:?}");

        assert_eq!(rendered, "MasterKey([REDACTED])");
    }
}

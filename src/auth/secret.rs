//! Protection of the stored Personal Access Token.
//!
//! The PAT is encrypted so that it can only be decrypted by azdocli on the
//! machine where it was entered. Copying the credentials file to another
//! machine, or reading the PAT out of it, does not yield a usable token.
//!
//! On Windows the PAT is sealed with DPAPI for the current user, using an
//! app- and machine-specific entropy. Elsewhere it is sealed with AES-256-GCM
//! using a key derived from the machine id.

use anyhow::{anyhow, Result};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

const PREFIX: &str = "enc:v1:";
const APP_CONTEXT: &[u8] = b"azdocli-pat-v1";

/// Returns true when a stored PAT value is encrypted rather than plaintext.
pub fn is_protected(stored: &str) -> bool {
    stored.starts_with(PREFIX)
}

pub(crate) fn protect_for(binding: &str, pat: &str) -> Result<String> {
    let sealed = seal(binding, pat.as_bytes())?;
    Ok(format!("{}{}", PREFIX, BASE64.encode(sealed)))
}

pub(crate) fn unprotect_for(binding: &str, stored: &str) -> Result<String> {
    let encoded = stored
        .strip_prefix(PREFIX)
        .ok_or_else(|| anyhow!("Stored PAT is not encrypted"))?;
    let sealed = BASE64.decode(encoded).map_err(|_| undecryptable())?;
    let plain = open(binding, &sealed).map_err(|_| undecryptable())?;
    String::from_utf8(plain).map_err(|_| undecryptable())
}

fn undecryptable() -> anyhow::Error {
    anyhow!("Stored PAT cannot be decrypted on this machine. Run 'azdocli login' again.")
}

#[cfg(windows)]
fn seal(binding: &str, data: &[u8]) -> Result<Vec<u8>> {
    dpapi::seal(&entropy(binding), data)
}

#[cfg(windows)]
fn open(binding: &str, sealed: &[u8]) -> Result<Vec<u8>> {
    dpapi::open(&entropy(binding), sealed)
}

#[cfg(windows)]
fn entropy(binding: &str) -> Vec<u8> {
    [APP_CONTEXT, binding.as_bytes()].concat()
}

#[cfg(not(windows))]
fn seal(binding: &str, data: &[u8]) -> Result<Vec<u8>> {
    aead::seal(binding, data)
}

#[cfg(not(windows))]
fn open(binding: &str, sealed: &[u8]) -> Result<Vec<u8>> {
    aead::open(binding, sealed)
}

#[cfg(windows)]
mod dpapi {
    use anyhow::{anyhow, Result};
    use std::ptr;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        }
    }

    fn empty_blob() -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        }
    }

    /// Copies a DPAPI-allocated output blob into a Vec and frees it.
    fn take(output: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        unsafe {
            let data = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
            LocalFree(output.pbData as _);
            data
        }
    }

    pub fn seal(entropy: &[u8], data: &[u8]) -> Result<Vec<u8>> {
        let input = blob(data);
        let entropy = blob(entropy);
        let mut output = empty_blob();
        let ok = unsafe {
            CryptProtectData(
                &input,
                ptr::null(),
                &entropy,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(anyhow!("Failed to encrypt the PAT"));
        }
        Ok(take(output))
    }

    pub fn open(entropy: &[u8], sealed: &[u8]) -> Result<Vec<u8>> {
        let input = blob(sealed);
        let entropy = blob(entropy);
        let mut output = empty_blob();
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                &entropy,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(anyhow!("Failed to decrypt the PAT"));
        }
        Ok(take(output))
    }
}

/// AES-256-GCM sealing with a key derived from the machine binding.
///
/// Compiled on every platform so it is tested everywhere, but only used
/// outside Windows.
#[cfg_attr(windows, allow(dead_code))]
mod aead {
    use super::APP_CONTEXT;
    use aes_gcm::aead::rand_core::RngCore;
    use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
    use aes_gcm::{Aes256Gcm, Nonce};
    use anyhow::{anyhow, Result};
    use hkdf::Hkdf;
    use sha2::Sha256;

    const SALT_LEN: usize = 16;
    const NONCE_LEN: usize = 12;

    fn cipher(binding: &str, salt: &[u8]) -> Aes256Gcm {
        let mut key = [0u8; 32];
        Hkdf::<Sha256>::new(Some(salt), binding.as_bytes())
            .expand(APP_CONTEXT, &mut key)
            .expect("32 bytes is a valid HKDF-SHA256 output length");
        Aes256Gcm::new_from_slice(&key).expect("key is 32 bytes")
    }

    /// Returns `salt || nonce || ciphertext`.
    pub fn seal(binding: &str, data: &[u8]) -> Result<Vec<u8>> {
        let mut salt = [0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher(binding, &salt)
            .encrypt(&nonce, data)
            .map_err(|_| anyhow!("Failed to encrypt the PAT"))?;
        Ok([&salt[..], &nonce[..], &ciphertext[..]].concat())
    }

    pub fn open(binding: &str, sealed: &[u8]) -> Result<Vec<u8>> {
        if sealed.len() < SALT_LEN + NONCE_LEN {
            return Err(anyhow!("Encrypted PAT is truncated"));
        }
        let (salt, rest) = sealed.split_at(SALT_LEN);
        let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
        cipher(binding, salt)
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|_| anyhow!("Failed to decrypt the PAT"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn aead_round_trips() {
            let sealed = seal("machine-a", b"my-secret-pat").unwrap();
            assert_eq!(open("machine-a", &sealed).unwrap(), b"my-secret-pat");
        }

        #[test]
        fn aead_rejects_another_machine() {
            let sealed = seal("machine-a", b"my-secret-pat").unwrap();
            assert!(open("machine-b", &sealed).is_err());
        }

        #[test]
        fn aead_rejects_truncated_input() {
            assert!(open("machine-a", &[0u8; 4]).is_err());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MACHINE_A: &str = "machine-a";

    #[test]
    fn protect_round_trips() {
        let stored = protect_for(MACHINE_A, "my-secret-pat").unwrap();
        assert_eq!(unprotect_for(MACHINE_A, &stored).unwrap(), "my-secret-pat");
    }

    #[test]
    fn protected_value_is_marked_and_hides_the_pat() {
        let stored = protect_for(MACHINE_A, "my-secret-pat").unwrap();
        assert!(is_protected(&stored));
        assert!(!stored.contains("my-secret-pat"));
    }

    #[test]
    fn pat_protected_on_another_machine_cannot_be_read() {
        let stored = protect_for(MACHINE_A, "my-secret-pat").unwrap();
        let err = unprotect_for("machine-b", &stored).unwrap_err();
        assert!(err.to_string().contains("azdocli login"));
    }

    #[test]
    fn tampered_value_cannot_be_read() {
        let stored = protect_for(MACHINE_A, "my-secret-pat").unwrap();
        let mut sealed = BASE64.decode(&stored[PREFIX.len()..]).unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0xff;
        let tampered = format!("{}{}", PREFIX, BASE64.encode(sealed));
        assert!(unprotect_for(MACHINE_A, &tampered).is_err());
    }

    #[test]
    fn protecting_twice_gives_different_values() {
        let first = protect_for(MACHINE_A, "my-secret-pat").unwrap();
        let second = protect_for(MACHINE_A, "my-secret-pat").unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn plaintext_pat_is_not_protected() {
        assert!(!is_protected("my-secret-pat"));
    }
}

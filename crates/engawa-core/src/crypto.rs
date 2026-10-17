//! Randomness, authenticated encryption and constant-time comparison, through aws-lc-rs, the one
//! cryptographic library Engawa ships (it is already rustls's provider).
//!
//! # References
//!
//! - [rfc5116] D. `McGrew`, "An Interface and Algorithms for Authenticated Encryption," RFC 5116,
//!   2008. <https://www.rfc-editor.org/rfc/rfc5116>
//! - [sp800-38d] M. Dworkin, "Recommendation for Block Cipher Modes of Operation:
//!   Galois/Counter Mode (GCM) and GMAC," NIST SP 800-38D, 2007.
//!   <https://doi.org/10.6028/NIST.SP.800-38D>
//!
//! [rfc5116]: https://www.rfc-editor.org/rfc/rfc5116
//! [sp800-38d]: https://doi.org/10.6028/NIST.SP.800-38D

use aws_lc_rs::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use aws_lc_rs::constant_time::verify_slices_are_equal;

/// The system random number generator failed. aws-lc-rs reports no detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the system random number generator failed")]
pub struct RandomError;

/// `N` bytes from the system CSPRNG.
///
/// # Errors
///
/// When the random number generator fails.
pub fn random<const N: usize>() -> Result<[u8; N], RandomError> {
    let mut bytes = [0_u8; N];
    aws_lc_rs::rand::fill(&mut bytes).map_err(|_unspecified| RandomError)?;
    Ok(bytes)
}

/// A random token of `N` bytes, as lower-case hex (`2 * N` characters).
///
/// # Errors
///
/// When the random number generator fails.
pub fn random_token<const N: usize>() -> Result<String, RandomError> {
    Ok(hex(&random::<N>()?))
}

/// Lower-case hex.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        for nibble in [byte >> 4, byte & 0x0F] {
            if let Some(&digit) = DIGITS.get(usize::from(nibble)) {
                out.push(char::from(digit));
            }
        }
    }
    out
}

/// Encrypts and authenticates `plaintext` with AES-256-GCM [sp800-38d] under `key`.
///
/// `aad` is bound as associated data [rfc5116]. The result is `nonce ‖ ciphertext ‖ tag`: a
/// fresh random 96-bit nonce (the length SP 800-38D §8.2.2 allows for random nonces), the
/// ciphertext, and the 16-byte tag. Only a holder of `key` can read it or make one that [`open`]
/// accepts.
///
/// [rfc5116]: https://www.rfc-editor.org/rfc/rfc5116
/// [sp800-38d]: https://doi.org/10.6028/NIST.SP.800-38D
///
/// # Errors
///
/// When the random number generator fails, the only way sealing fails with a 32-byte key.
pub fn seal(key: &[u8; 32], aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, RandomError> {
    let nonce = random::<NONCE_LEN>()?;
    let key = aead_key(key).ok_or(RandomError)?;
    let mut in_out = plaintext.to_vec();
    key.seal_in_place_append_tag(
        Nonce::assume_unique_for_key(nonce),
        Aad::from(aad),
        &mut in_out,
    )
    .map_err(|_unspecified| RandomError)?;
    let mut out = Vec::with_capacity(NONCE_LEN + in_out.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&in_out);
    Ok(out)
}

/// The plaintext of a [`seal`]ed value, or `None` when it was altered, was sealed under another
/// key or with other associated data, or is not a sealed value at all.
#[must_use]
pub fn open(key: &[u8; 32], aad: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
    let (nonce, ciphertext) = sealed.split_at_checked(NONCE_LEN)?;
    let nonce = Nonce::try_assume_unique_for_key(nonce).ok()?;
    let key = aead_key(key)?;
    let mut in_out = ciphertext.to_vec();
    let plaintext = key.open_in_place(nonce, Aad::from(aad), &mut in_out).ok()?;
    Some(plaintext.to_vec())
}

fn aead_key(key: &[u8; 32]) -> Option<LessSafeKey> {
    UnboundKey::new(&AES_256_GCM, key)
        .ok()
        .map(LessSafeKey::new)
}

/// The SHA-256 digest of `data`.
#[must_use]
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let digest = aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, data);
    let mut out = [0_u8; 32];
    out.copy_from_slice(digest.as_ref());
    out
}

/// Whether `a` and `b` are equal, in time that does not depend on where they differ.
#[must_use]
pub fn equal(a: &[u8], b: &[u8]) -> bool {
    verify_slices_are_equal(a, b).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_hex_of_the_requested_length_and_differ() {
        let a = random_token::<32>().unwrap();
        let b = random_token::<32>().unwrap();
        assert_eq!(a.len(), 64);
        assert!(
            a.bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_ne!(a, b);
    }

    #[test]
    fn sha256_matches_the_fips_180_example() {
        // FIPS 180-4 example "abc".
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sealed_values_open_only_unaltered_under_their_key_and_context() {
        let key = random::<32>().unwrap();
        let sealed = seal(&key, b"context", b"a secret value").unwrap();
        assert_eq!(
            open(&key, b"context", &sealed).as_deref(),
            Some(&b"a secret value"[..])
        );
        // The plaintext is not visible in the sealed bytes.
        assert!(!sealed.windows(14).any(|w| w == b"a secret value"));
        // Two seals of the same value differ (fresh nonces).
        assert_ne!(sealed, seal(&key, b"context", b"a secret value").unwrap());
        // Other associated data, another key, a flipped bit, or truncation: refused.
        assert_eq!(open(&key, b"other", &sealed), None);
        assert_eq!(open(&random::<32>().unwrap(), b"context", &sealed), None);
        let mut flipped = sealed.clone();
        if let Some(byte) = flipped.last_mut() {
            *byte ^= 1;
        }
        assert_eq!(open(&key, b"context", &flipped), None);
        assert_eq!(open(&key, b"context", &sealed[..5]), None);
    }
}

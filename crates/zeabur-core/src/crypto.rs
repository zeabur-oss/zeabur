//! Randomness, authenticated encryption, constant-time comparison and RS256 signatures, through
//! aws-lc-rs, the one cryptographic library Zeabur ships (it is already rustls's provider).
//!
//! # References
//!
//! - [rfc5116] D. `McGrew`, "An Interface and Algorithms for Authenticated Encryption," RFC 5116,
//!   2008. <https://www.rfc-editor.org/rfc/rfc5116>
//! - [sp800-38d] M. Dworkin, "Recommendation for Block Cipher Modes of Operation:
//!   Galois/Counter Mode (GCM) and GMAC," NIST SP 800-38D, 2007.
//!   <https://doi.org/10.6028/NIST.SP.800-38D>
//! - [rfc7518] M. Jones, "JSON Web Algorithms (JWA)," RFC 7518, 2015, section 3.3. <https://www.rfc-editor.org/rfc/rfc7518#section-3.3>
//!
//! [rfc5116]: https://www.rfc-editor.org/rfc/rfc5116
//! [sp800-38d]: https://doi.org/10.6028/NIST.SP.800-38D
//! [rfc7518]: https://www.rfc-editor.org/rfc/rfc7518#section-3.3

use aws_lc_rs::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use aws_lc_rs::constant_time::verify_slices_are_equal;
use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::{RSA_PKCS1_SHA256, RsaKeyPair};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
pub use zeabur_model::kernel::digest::hex;

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

/// Why an RSA private key cannot sign.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    /// It is not a PEM RSA private key, PKCS#1 (`RSA PRIVATE KEY`) or PKCS#8 (`PRIVATE KEY`).
    #[error("the key is not a PEM RSA private key")]
    NotPem,
    /// aws-lc-rs refuses it, such as a key shorter than 2048 bits.
    #[error("the key is refused: {0}")]
    Refused(String),
}

/// Signs `message` with RSASSA-PKCS1-v1_5 and SHA-256, the JWS algorithm `RS256` [rfc7518],
/// under the PEM private key `pem`.
///
/// [rfc7518]: https://www.rfc-editor.org/rfc/rfc7518#section-3.3
///
/// # Errors
///
/// When `pem` is not an RSA private key aws-lc-rs accepts, or signing fails.
pub fn sign_rs256(pem: &str, message: &[u8]) -> Result<Vec<u8>, KeyError> {
    let label = |l: &str| pem.contains(&format!("-----BEGIN {l}-----"));
    let pkcs1 = label("RSA PRIVATE KEY");
    if !pkcs1 && !label("PRIVATE KEY") {
        return Err(KeyError::NotPem);
    }
    let body: String = pem
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("-----"))
        .collect();
    let der = STANDARD.decode(body).map_err(|_error| KeyError::NotPem)?;
    let key = if pkcs1 {
        RsaKeyPair::from_der(&der)
    } else {
        RsaKeyPair::from_pkcs8(&der)
    }
    .map_err(|e| KeyError::Refused(e.to_string()))?;
    let mut signature = vec![0; key.public_modulus_len()];
    key.sign(
        &RSA_PKCS1_SHA256,
        &SystemRandom::new(),
        message,
        &mut signature,
    )
    .map_err(|_unspecified| KeyError::Refused("signing failed".to_owned()))?;
    Ok(signature)
}

#[cfg(test)]
mod tests {
    use aws_lc_rs::encoding::AsDer as _;
    use aws_lc_rs::rsa::KeySize;
    use aws_lc_rs::signature::{KeyPair as _, RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};

    use super::*;

    #[test]
    fn rs256_signatures_verify_with_the_public_key() {
        let key = RsaKeyPair::generate(KeySize::Rsa2048).unwrap();
        let der = STANDARD.encode(key.as_der().unwrap().as_ref());
        let pem = format!("-----BEGIN PRIVATE KEY-----\n{der}\n-----END PRIVATE KEY-----\n");
        let signature = sign_rs256(&pem, b"header.claims").unwrap();
        UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, key.public_key().as_ref())
            .verify(b"header.claims", &signature)
            .unwrap();
        assert_eq!(sign_rs256("not a key", b"x"), Err(KeyError::NotPem));
        assert!(
            sign_rs256(
                "-----BEGIN RSA PRIVATE KEY-----\nAAAA\n-----END RSA PRIVATE KEY-----",
                b"x"
            )
            .is_err()
        );
    }

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

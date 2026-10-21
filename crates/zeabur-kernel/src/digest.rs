//! SHA-256 for names derived from other names, such as a label value for a user name.
//!
//! These digests make names short and stable; they protect nothing. The server's cryptography is
//! `zeabur-core`'s, through aws-lc-rs; this module uses the pure-Rust `sha2`, so the crates that
//! name things build without a C library, WebAssembly included.

use sha2::{Digest as _, Sha256};

/// The SHA-256 digest of `data` [fips180-4].
///
/// [fips180-4]: https://doi.org/10.6028/NIST.FIPS.180-4
#[must_use]
pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The examples of FIPS 180-4 (NIST's "Example Algorithms", SHA-256).
    #[test]
    fn sha256_gives_the_standard_digests() {
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}

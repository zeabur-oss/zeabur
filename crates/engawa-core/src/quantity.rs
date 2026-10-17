//! Kubernetes quantities [k8s-quantity]: `250m`, `2`, `512Mi`, `1.5Gi`, `1e3`.
//!
//! [k8s-quantity]: https://kubernetes.io/docs/reference/kubernetes-api/common-definitions/quantity/

/// A quantity in `unit`s (10^`exponent` of the base: −3 for millicores, 0 for bytes), rounded up.
/// `None` when it is not a Kubernetes quantity [k8s-quantity].
///
/// [k8s-quantity]: https://kubernetes.io/docs/reference/kubernetes-api/common-definitions/quantity/
fn parse(quantity: &str, exponent: i32) -> Option<u64> {
    let text = quantity.trim();
    let split = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '+'))
        .unwrap_or(text.len());
    let (number, suffix) = text.split_at_checked(split)?;
    let number = number.strip_prefix('+').unwrap_or(number);
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    let digits = format!("{whole}{fraction}");
    let mantissa: u128 = digits.parse().ok()?;
    let fraction_digits = i32::try_from(fraction.len()).ok()?;
    let (multiplier, power): (u128, i32) = match suffix {
        "" => (1, 0),
        "n" => (1, -9),
        "u" => (1, -6),
        "m" => (1, -3),
        "k" => (1, 3),
        "M" => (1, 6),
        "G" => (1, 9),
        "T" => (1, 12),
        "P" => (1, 15),
        "E" => (1, 18),
        "Ki" => (1 << 10, 0),
        "Mi" => (1 << 20, 0),
        "Gi" => (1 << 30, 0),
        "Ti" => (1 << 40, 0),
        "Pi" => (1 << 50, 0),
        "Ei" => (1 << 60, 0),
        other => {
            // A decimal exponent: `1e3`, `5E-1`.
            let exp = other
                .strip_prefix(['e', 'E'])
                .and_then(|e| e.parse::<i32>().ok())?;
            (1, exp)
        }
    };
    let value = mantissa.checked_mul(multiplier)?;
    let shift = power.checked_sub(fraction_digits)?.checked_sub(exponent)?;
    let scaled = if shift >= 0 {
        value.checked_mul(10_u128.checked_pow(u32::try_from(shift).ok()?)?)?
    } else {
        let divisor = 10_u128.checked_pow(u32::try_from(-shift).ok()?)?;
        value.div_ceil(divisor)
    };
    u64::try_from(scaled).ok()
}

/// CPU in millicores (`250m` → 250, `2` → 2000, `250000000n` → 250).
#[must_use]
pub fn millicores(quantity: &str) -> Option<u64> {
    parse(quantity, -3)
}

/// Memory or storage in bytes (`512Mi`, `1G`).
#[must_use]
pub fn bytes(quantity: &str) -> Option<u64> {
    parse(quantity, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantities_parse_in_every_suffix_and_round_up() {
        assert_eq!(millicores("250m"), Some(250));
        assert_eq!(millicores("2"), Some(2000));
        assert_eq!(millicores("0.5"), Some(500));
        assert_eq!(millicores("250000000n"), Some(250));
        assert_eq!(millicores("1n"), Some(1), "rounded up");
        assert_eq!(millicores("1e3"), Some(1_000_000));
        assert_eq!(bytes("512Mi"), Some(512 << 20));
        assert_eq!(bytes("1.5Gi"), Some(3 << 29));
        assert_eq!(bytes("1G"), Some(1_000_000_000));
        assert_eq!(bytes("128974848"), Some(128_974_848));
        assert_eq!(bytes("12Kix"), None);
        assert_eq!(bytes(""), None);
        assert_eq!(bytes("Mi"), None);
    }
}

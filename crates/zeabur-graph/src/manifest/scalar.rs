//! How a string is quoted: the way `kubectl get -o yaml` quotes it.
//!
//! kubectl writes YAML with `sigs.k8s.io/yaml`, which hands the object to go-yaml v2, a YAML 1.1
//! writer. go-yaml writes a string bare only when its own reader would read the bare text back
//! as a string; otherwise it writes it in double quotes ([encode.go], `stringv`). Its reader
//! takes the YAML 1.1 booleans and nulls, numbers in YAML 1.1 and Go spellings, and timestamps
//! for something else ([resolve.go]), and the writer quotes sexagesimal numbers (`1:20`) as
//! well. A string that would break the syntax when bare (`*`, `- item`, `a: b`) is written in
//! single quotes by the emitter ([emitterc.go], `yaml_emitter_analyze_scalar`).
//!
//! serde-saphyr already quotes most of these, but writes `1:20` and `2024-01-01` bare and uses
//! double quotes everywhere, so the decision is made here and handed to it with its
//! [`DoubleQuoted`] and [`SingleQuoted`] wrappers. Where this module leaves a string to
//! serde-saphyr, it may still quote one that go-yaml writes bare (`inf`, `nan`): more quoting
//! than kubectl, never less.
//!
//! [encode.go]: https://github.com/go-yaml/yaml/blob/v2.4.0/encode.go#L283-L335
//! [resolve.go]: https://github.com/go-yaml/yaml/blob/v2.4.0/resolve.go
//! [emitterc.go]: https://github.com/go-yaml/yaml/blob/v2.4.0/emitterc.go#L984-L1130

use serde::ser::{Serialize, Serializer};
use serde_saphyr::{DoubleQuoted, SingleQuoted};

/// A string written with kubectl's quoting.
pub(super) struct Scalar<'a>(pub(super) &'a str);

impl Serialize for Scalar<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match quote(self.0) {
            Some(Quote::Double) => DoubleQuoted(self.0).serialize(serializer),
            Some(Quote::Single) => SingleQuoted(self.0).serialize(serializer),
            None => self.0.serialize(serializer),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Quote {
    Single,
    Double,
}

/// The quotes go-yaml puts around `s`, or `None` to leave the choice to serde-saphyr.
fn quote(s: &str) -> Option<Quote> {
    // A line break or a character that needs an escape: serde-saphyr writes a block scalar or
    // double quotes, as go-yaml does.
    if s.chars()
        .any(|c| c.is_control() || matches!(c, '\u{FEFF}' | '\u{2028}' | '\u{2029}'))
    {
        return None;
    }
    if resolves(s) || sexagesimal(s) {
        Some(Quote::Double)
    } else if !plain_allowed(s.as_bytes()) {
        Some(Quote::Single)
    } else {
        None
    }
}

/// The bare words go-yaml reads as a boolean, a null or a special float (`resolveMapList`).
#[rustfmt::skip]
const WORDS: [&str; 39] = [
    "y", "Y", "yes", "Yes", "YES", "true", "True", "TRUE", "on", "On", "ON",
    "n", "N", "no", "No", "NO", "false", "False", "FALSE", "off", "Off", "OFF",
    "", "~", "null", "Null", "NULL",
    ".nan", ".NaN", ".NAN", ".inf", ".Inf", ".INF",
    "+.inf", "+.Inf", "+.INF", "-.inf", "-.Inf", "-.INF",
];

/// Whether go-yaml's reader takes bare `s` for something other than a string (`resolve`).
fn resolves(s: &str) -> bool {
    if WORDS.contains(&s) {
        return true;
    }
    match s.as_bytes().first() {
        // Go's `strconv.ParseFloat`, which allows `_` between digits.
        Some(b'.') => underscores_between_digits(s) && float(&s.replace('_', "")),
        Some(b'+' | b'-' | b'0'..=b'9') => {
            let plain = s.replace('_', "");
            timestamp(s.as_bytes()) || prefixed_int(&plain) || float(&plain)
        }
        _ => false,
    }
}

fn underscores_between_digits(s: &str) -> bool {
    let b = s.as_bytes();
    b.iter().enumerate().all(|(i, c)| {
        *c != b'_'
            || (i
                .checked_sub(1)
                .and_then(|p| b.get(p))
                .is_some_and(u8::is_ascii_digit)
                && b.get(i + 1).is_some_and(u8::is_ascii_digit))
    })
}

/// `0x1F`, `-0o17`, `0b1010`: Go's `strconv.ParseInt(s, 0, 64)`, then `ParseUint` for an
/// unsigned value. Decimal and `0`-prefixed octal spellings are all caught by [`float`].
fn prefixed_int(s: &str) -> bool {
    let (sign, unsigned) = match s.as_bytes().first() {
        Some(b'-') => (Some(true), s.get(1..).unwrap_or_default()),
        Some(b'+') => (Some(false), s.get(1..).unwrap_or_default()),
        _ => (None, s),
    };
    let radix = match unsigned.as_bytes() {
        [b'0', b'x' | b'X', ..] => 16,
        [b'0', b'o' | b'O', ..] => 8,
        [b'0', b'b' | b'B', ..] => 2,
        _ => return false,
    };
    let digits = unsigned.get(2..).unwrap_or_default();
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return false;
    }
    let Ok(value) = u64::from_str_radix(digits, radix) else {
        return false;
    };
    match sign {
        Some(true) => value <= 1 << 63,
        Some(false) => i64::try_from(value).is_ok(),
        None => true,
    }
}

/// go-yaml's `yamlStyleFloat`, `^[-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?$`, for a
/// value that does not overflow.
fn float(s: &str) -> bool {
    let digits = |part: &str| part.bytes().all(|c| c.is_ascii_digit());
    let unsigned = s.strip_prefix(['+', '-']).unwrap_or(s);
    let (mantissa, exponent) = unsigned
        .split_once(['e', 'E'])
        .map_or((unsigned, None), |(m, e)| (m, Some(e)));
    let (whole, fraction) = mantissa
        .split_once('.')
        .map_or((mantissa, None), |(w, f)| (w, Some(f)));
    let number = match fraction {
        Some(fraction) if whole.is_empty() => !fraction.is_empty() && digits(fraction),
        Some(fraction) => digits(whole) && digits(fraction),
        None => !whole.is_empty() && digits(whole),
    };
    let exponent = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && digits(e)
    });
    number && exponent && s.parse::<f64>().is_ok_and(f64::is_finite)
}

/// go-yaml's `isBase60Float`, `^[-+]?[0-9][0-9_]*(?::[0-5]?[0-9])+(?:\.[0-9_]*)?$`.
fn sexagesimal(s: &str) -> bool {
    let s = s.strip_prefix(['+', '-']).unwrap_or(s);
    let (whole, fraction) = s.split_once('.').unwrap_or((s, ""));
    let mut parts = whole.split(':');
    let first = parts.next().unwrap_or_default().as_bytes();
    let sixty = |part: &str| match part.as_bytes() {
        [d] => d.is_ascii_digit(),
        [t, d] => (b'0'..=b'5').contains(t) && d.is_ascii_digit(),
        _ => false,
    };
    first.first().is_some_and(u8::is_ascii_digit)
        && first.iter().all(|c| c.is_ascii_digit() || *c == b'_')
        && whole.contains(':')
        && parts.all(sixty)
        && fraction.bytes().all(|c| c.is_ascii_digit() || c == b'_')
}

/// The timestamps go-yaml reads (`allowedTimestampFormats`, through Go's `time.Parse`):
/// `2006-1-2`, `2006-1-2 15:4:5.999999999` and `2006-1-2T15:4:5.999999999Z07:00`, with `t`
/// for `T` too.
fn timestamp(b: &[u8]) -> bool {
    let [y1, y2, y3, y4, b'-', rest @ ..] = b else {
        return false;
    };
    let year = [y1, y2, y3, y4].iter().try_fold(0_u32, |n, d| {
        d.is_ascii_digit().then(|| n * 10 + u32::from(**d - b'0'))
    });
    let Some(year) = year else {
        return false;
    };
    let mut rest = rest;
    let Some(month) = number(&mut rest).filter(|m| (1..=12).contains(m)) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    let [b'-', after @ ..] = rest else {
        return false;
    };
    rest = after;
    if number(&mut rest).is_none_or(|d| d == 0 || d > days) {
        return false;
    }
    // A space in Go's layout matches any run of spaces.
    let (zone, time) = match rest {
        [] => return true,
        [b'T' | b't', after @ ..] => (true, after),
        [b' ', ..] => (false, rest.trim_ascii_start()),
        _ => return false,
    };
    rest = time;
    for (limit, separator) in [(24, true), (60, true), (60, false)] {
        if number(&mut rest).is_none_or(|n| n >= limit) {
            return false;
        }
        if separator {
            let [b':', after @ ..] = rest else {
                return false;
            };
            rest = after;
        }
    }
    // Fractional seconds, of any length, after `.` or `,`.
    if let [b'.' | b',', fraction @ ..] = rest
        && fraction.first().is_some_and(u8::is_ascii_digit)
    {
        let end = fraction
            .iter()
            .position(|c| !c.is_ascii_digit())
            .unwrap_or(fraction.len());
        rest = fraction.get(end..).unwrap_or_default();
    }
    if !zone {
        return rest.is_empty();
    }
    match rest {
        [b'Z'] => true,
        [b'+' | b'-', h1, h2, b':', m1, m2] => {
            let two = |a: u8, b: u8| {
                (a.is_ascii_digit() && b.is_ascii_digit()).then(|| (a - b'0') * 10 + (b - b'0'))
            };
            two(*h1, *h2).is_some_and(|h| h <= 24) && two(*m1, *m2).is_some_and(|m| m <= 60)
        }
        _ => false,
    }
}

/// One or two digits, as Go's `time.Parse` reads `1`, `2`, `15`, `4` and `5`.
fn number(rest: &mut &[u8]) -> Option<u32> {
    let digit = |c: Option<&u8>| {
        c.filter(|c| c.is_ascii_digit())
            .map(|c| u32::from(c - b'0'))
    };
    let first = digit(rest.first())?;
    let (value, used) = digit(rest.get(1)).map_or((first, 1), |second| (first * 10 + second, 2));
    *rest = rest.get(used..).unwrap_or_default();
    Some(value)
}

/// Whether the emitter may write `s` bare in block context (`block_plain_allowed`).
fn plain_allowed(b: &[u8]) -> bool {
    let blank = |c: Option<&u8>| c.is_none_or(|c| matches!(c, b' ' | b'\t'));
    if b.starts_with(b"---") || b.starts_with(b"...") {
        return false;
    }
    if b.first() == Some(&b' ') || b.last() == Some(&b' ') {
        return false;
    }
    let first = match b.first() {
        Some(
            b'#' | b',' | b'[' | b']' | b'{' | b'}' | b'&' | b'*' | b'!' | b'|' | b'>' | b'\''
            | b'"' | b'%' | b'@' | b'`',
        ) => false,
        Some(b'?' | b':' | b'-') => !blank(b.get(1)),
        _ => true,
    };
    first
        && b.windows(2).enumerate().all(|(i, pair)| match pair {
            [_, b':'] => !blank(b.get(i + 2)),
            [b' ' | b'\t', b'#'] => false,
            _ => true,
        })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    /// Each value as `kubectl create configmap x --from-literal=k=<value> --dry-run=client -o
    /// yaml` writes it (kubectl v1.37.1), and as this writer does.
    #[test]
    fn strings_are_quoted_as_kubectl_quotes_them() {
        #[rustfmt::skip]
        let table: &[(&str, &str)] = &[
            // YAML 1.1 booleans, in the spellings go-yaml reads; others stay bare.
            ("true", r#""true""#), ("True", r#""True""#), ("FALSE", r#""FALSE""#),
            ("yes", r#""yes""#), ("Yes", r#""Yes""#), ("NO", r#""NO""#),
            ("y", r#""y""#), ("N", r#""N""#), ("on", r#""on""#),
            ("OFF", r#""OFF""#),
            // Nulls.
            ("", r#""""#), ("~", r#""~""#), ("null", r#""null""#), ("NULL", r#""NULL""#),
            // Sexagesimal numbers.
            ("1:20", r#""1:20""#), ("190:20:30", r#""190:20:30""#),
            ("190:20:30.15", r#""190:20:30.15""#), ("-1:20", r#""-1:20""#),
            ("1_0:20", r#""1_0:20""#), ("1:60", "1:60"), ("1:2_0", "1:2_0"),
            // Timestamps.
            ("2024-01-01", r#""2024-01-01""#), ("2024-1-1", r#""2024-1-1""#),
            ("2001-12-14t21:59:43.10-05:00", r#""2001-12-14t21:59:43.10-05:00""#),
            ("2001-12-14T21:59:43.10Z", r#""2001-12-14T21:59:43.10Z""#),
            ("2001-12-14 21:59:43.10", r#""2001-12-14 21:59:43.10""#),
            ("2024-01-01  10:00:00", r#""2024-01-01  10:00:00""#),
            ("2024-01-01 10:00:00,5", r#""2024-01-01 10:00:00,5""#),
            ("2024-01-01T10:00:00+24:00", r#""2024-01-01T10:00:00+24:00""#),
            ("2024-01-01T10:00:00+05:60", r#""2024-01-01T10:00:00+05:60""#),
            ("2024-02-29", r#""2024-02-29""#), ("2000-02-29", r#""2000-02-29""#),
            ("2024-01-01T10:00:00+0530", "2024-01-01T10:00:00+0530"),
            ("2024-01-01 10:00", "2024-01-01 10:00"),
            ("2024-001-01", "2024-001-01"), ("12024-01-01", "12024-01-01"),
            // Integers in YAML 1.1 and Go spellings.
            ("0x1F", r#""0x1F""#), ("+0x1F", r#""+0x1F""#), ("0X1f", r#""0X1f""#),
            ("0o17", r#""0o17""#), ("0O17", r#""0O17""#), ("017", r#""017""#),
            ("08", r#""08""#), ("0b1010", r#""0b1010""#), ("-0b11", r#""-0b11""#),
            ("1_000", r#""1_000""#), ("1__0", r#""1__0""#), ("1_", r#""1_""#),
            ("0x_1F", r#""0x_1F""#), ("-1", r#""-1""#), ("+1", r#""+1""#),
            ("0xFFFFFFFFFFFFFFFF", r#""0xFFFFFFFFFFFFFFFF""#),
            ("-0x8000000000000000", r#""-0x8000000000000000""#),
            ("99999999999999999999", r#""99999999999999999999""#),
            ("0x", "0x"), ("0b2", "0b2"), ("0x1p3", "0x1p3"),
            // Floats.
            (".inf", r#"".inf""#), ("-.Inf", r#""-.Inf""#), (".NaN", r#"".NaN""#),
            ("1e3", r#""1e3""#), ("1E3", r#""1E3""#), ("1.", r#""1.""#), (".5", r#"".5""#),
            ("+.5", r#""+.5""#), ("1.e5", r#""1.e5""#), ("6.8523015e+5", r#""6.8523015e+5""#),
            (".5_0", r#"".5_0""#), ("1_0.5", r#""1_0.5""#), ("1e-400", r#""1e-400""#),
            // Strings that would break the syntax bare.
            ("@here", "'@here'"), ("%x", "'%x'"), ("-", "'-'"), ("- item", "'- item'"),
            ("? x", "'? x'"), ("a: b", "'a: b'"), ("ends:", "'ends:'"),
            (" leading", "' leading'"), ("trailing ", "'trailing '"),
            ("#comment", "'#comment'"), ("a #b", "'a #b'"), ("*", "'*'"),
            ("&anchor", "'&anchor'"), ("!tag", "'!tag'"), ("|x", "'|x'"), (">x", "'>x'"),
            ("'q", "'''q'"), ("\"q", "'\"q'"), ("`q", "'`q'"), (",x", "',x'"),
            ("[x", "'[x'"), ("{x", "'{x'"), ("--- x", "'--- x'"), ("...", "'...'"),
            ("a\tb", r#""a\tb""#),
            // Ordinary strings stay bare.
            ("-x", "-x"), ("?x", "?x"), ("nginx:1.27", "nginx:1.27"),
            ("/data", "/data"), ("100m", "100m"), ("v1.2.3", "v1.2.3"),
            ("https://example.com/a", "https://example.com/a"),
        ];
        for (value, kubectl) in table {
            let yaml = serde_saphyr::to_string(&BTreeMap::from([("k", Scalar(value))])).unwrap();
            assert_eq!(yaml, format!("k: {kubectl}\n"), "{value:?}");
        }
    }

    /// Bare in kubectl's output, quoted in ours: serde-saphyr's own rule is stricter here (it
    /// reads booleans, nulls and special floats in any case, and anything number-like).
    #[test]
    fn serde_saphyr_quotes_a_few_more() {
        for value in [
            "tRUE",
            "yEs",
            "oN",
            "nULL",
            "inf",
            "+inf",
            "nan",
            ".nAn",
            "1e400",
            ".",
            "2001-12-14 21:59:43.10 -5",
            "2024-01-01T10:00:00",
            "2024-01-01T10:00:00+25:00",
            "2024-01-01T24:00:00Z",
            "2024-01-01 10:00:00.",
            "2024-01-01 10:00:00Z",
            "2024-02-30",
            "1900-02-29",
            "2024-13-01",
            "_1",
            "._5",
            "1.2.3",
            "<<",
            "0x10000000000000000",
            "+0x8000000000000000",
        ] {
            assert_eq!(quote(value), None, "{value:?}");
            let yaml = serde_saphyr::to_string(&Scalar(value)).unwrap();
            assert!(yaml.starts_with(['"', '\'']), "{value:?} => {yaml}");
            assert_eq!(serde_saphyr::from_str::<String>(&yaml).unwrap(), value);
        }
    }

    /// Whatever the quoting, the text reads back as the same string.
    #[test]
    fn what_is_written_reads_back_the_same() {
        for value in [
            "yes",
            "no",
            "on",
            "off",
            "y",
            "n",
            "true",
            "null",
            "~",
            "",
            "0123",
            "0",
            "00",
            "1e3",
            "1E3",
            "0x1F",
            "1_000",
            ".5",
            "-1",
            "+1",
            "1.0",
            "1.2.3",
            "1:20",
            ":",
            "a:b",
            "a: b",
            "ends:",
            ":x",
            "#",
            "a#b",
            "a #b",
            "#a",
            "-",
            "- a",
            "?",
            "? a",
            "&a",
            "*a",
            "!a",
            "%a",
            "@a",
            "'a'",
            "\"a\"",
            "a'b",
            "a\"b",
            " a",
            "a ",
            "a\tb",
            "a\nb",
            "a\n",
            "\n",
            "[a]",
            "{a}",
            "a,b",
            "|",
            ">",
            "<<",
            "=",
            "inf",
            ".inf",
            "nan",
            ".nan",
            "2024-01-01",
            "2024-01-01T10:00:00Z",
            "12:30",
            "v1.27",
            "1.27",
            "nginx:1.27",
            "https://example.com/a#b",
            "false ",
            "Yes",
            "NO",
            "Null",
            "NULL",
            "é",
            "日本語",
        ] {
            let yaml = serde_saphyr::to_string(&BTreeMap::from([("k", Scalar(value))])).unwrap();
            let back: BTreeMap<String, String> = serde_saphyr::from_str(&yaml).unwrap();
            assert_eq!(back["k"], value, "{value:?} => {yaml:?}");
        }
    }
}

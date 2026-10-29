//! How far before its first point a metrics query may read samples, bounded from its text.
//!
//! VictoriaMetrics reads samples before the time it evaluates: a range selector's window, a
//! subquery's window and step, an `offset`, and for each selector and each subquery the step and
//! 5 minutes more (`-search.minStalenessInterval` unset). A rollup function over an expression
//! that is not a selector is a subquery too. [`reach`] adds every duration in the query, and the
//! steps and 5 minutes once for each level a subquery may nest in, without telling rollup
//! functions from others. It refuses what it cannot bound: `@`, `WITH`, comments, durations that
//! are not whole literals.

use crate::reject::Reject;

/// `maxSilenceInterval`: VictoriaMetrics reads this much more before each selector and subquery.
const SILENCE: i64 = 5 * 60 * 1000;

/// `-search.latencyOffset`: VictoriaMetrics may move an instant query's time this much earlier.
const LATENCY: i64 = 30 * 1000;

/// Aggregates, which read no samples themselves.
const AGGREGATES: &[&str] = &[
    "avg",
    "bottomk",
    "count",
    "count_values",
    "group",
    "max",
    "min",
    "quantile",
    "stddev",
    "stdvar",
    "sum",
    "topk",
];

/// Binary operators that are words.
const WORD_OPERATORS: &[&str] = &["and", "or", "unless", "if", "ifnot", "default", "atan2"];

/// Words before a list of labels.
const MODIFIERS: &[&str] = &[
    "by",
    "without",
    "on",
    "ignoring",
    "group_left",
    "group_right",
];

/// A parenthesis not yet closed.
#[derive(Default)]
struct Frame {
    /// An aggregate's: never a subquery.
    plain: bool,
    /// Holds a parenthesis or a binary operator, so a rollup function over it is a subquery.
    nested: bool,
    /// The most subqueries nested in it.
    depth: i64,
    /// For a list of labels after an aggregate's `by`: the aggregate's parenthesis is next.
    aggregate_next: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Last {
    Selector,
    Aggregate,
    /// `by`, `on` and the like; `aggregate` for an aggregate's `by` or `without` before its
    /// parenthesis.
    Modifier {
        aggregate: bool,
    },
    /// The labels of an aggregate's `by` or `without`, before its parenthesis.
    Labels,
    /// A window or subquery.
    Window,
    Other,
}

/// The milliseconds before its first point that `query` may read samples from, when VictoriaMetrics
/// evaluates it every `step` milliseconds (an instant query's step for one).
///
/// # Errors
/// [`Reject::InvalidQuery`] when the query uses what this cannot bound, or is not `MetricsQL`.
pub fn reach(query: &str, step: i64) -> Result<i64, Reject> {
    let mut scan = Scan {
        durations: 0,
        steps: step.max(1),
        subqueries: 0,
        frames: vec![Frame {
            plain: true,
            ..Frame::default()
        }],
        braces: false,
        last: Last::Other,
    };
    let mut i = 0;
    while let Some(&b) = query.as_bytes().get(i) {
        let rest = query.get(i..).ok_or(Reject::InvalidQuery)?;
        i += scan.token(b, rest)?;
    }
    let [root] = scan.frames.as_slice() else {
        return Err(Reject::InvalidQuery);
    };
    if scan.braces {
        return Err(Reject::InvalidQuery);
    }
    let level = scan.steps.saturating_mul(3).saturating_add(SILENCE);
    Ok(scan
        .durations
        .saturating_add(
            root.depth
                .saturating_add(scan.subqueries)
                .saturating_mul(level),
        )
        .saturating_add(scan.steps.saturating_mul(2))
        .saturating_add(SILENCE)
        .saturating_add(LATENCY))
}

struct Scan {
    /// Every duration so far.
    durations: i64,
    /// The longest step: the query's or a subquery's.
    steps: i64,
    /// Windows and subqueries over what is not a selector.
    subqueries: i64,
    frames: Vec<Frame>,
    /// Inside a selector's `{}`.
    braces: bool,
    last: Last,
}

impl Scan {
    /// Reads the token at the start of `rest`, whose first byte is `b`; its length.
    fn token(&mut self, b: u8, rest: &str) -> Result<usize, Reject> {
        let mut next = Last::Other;
        let len = match b {
            b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r' => return Ok(1),
            b'"' | b'\'' | b'`' => string(rest)?,
            b'{' if !self.braces => {
                self.braces = true;
                1
            }
            b'}' if self.braces => {
                self.braces = false;
                next = Last::Selector;
                1
            }
            b',' => 1,
            b'(' if !self.braces => {
                self.frame()?.nested = true;
                let (plain, aggregate_next) = match self.last {
                    Last::Aggregate | Last::Labels => (true, false),
                    Last::Modifier { aggregate } => (false, aggregate),
                    Last::Selector | Last::Window | Last::Other => (false, false),
                };
                self.frames.push(Frame {
                    plain,
                    aggregate_next,
                    ..Frame::default()
                });
                1
            }
            b')' if !self.braces => {
                let closed = self.frames.pop().ok_or(Reject::InvalidQuery)?;
                let own = i64::from(closed.nested && !closed.plain);
                let parent = self.frame()?;
                parent.depth = parent.depth.max(closed.depth.saturating_add(own));
                if closed.aggregate_next {
                    next = Last::Labels;
                }
                1
            }
            b'[' if !self.braces => {
                let end = rest.find(']').ok_or(Reject::InvalidQuery)?;
                self.window(rest.get(1..end).ok_or(Reject::InvalidQuery)?)?;
                next = Last::Window;
                end + 1
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' | b':' => return self.word(rest),
            b'0'..=b'9' | b'.' => rest
                .bytes()
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.'))
                .count(),
            b'+' | b'-' | b'*' | b'/' | b'%' | b'^' | b'>' | b'<' | b'=' | b'!' | b'~' => {
                if !self.braces {
                    self.frame()?.nested = true;
                }
                1
            }
            _ => return Err(Reject::InvalidQuery),
        };
        self.last = next;
        Ok(len)
    }

    fn frame(&mut self) -> Result<&mut Frame, Reject> {
        self.frames.last_mut().ok_or(Reject::InvalidQuery)
    }

    /// `[window]` or `[window:step]`, without the brackets.
    fn window(&mut self, content: &str) -> Result<(), Reject> {
        let (window, step) = match content.split_once(':') {
            Some((window, step)) => (window.trim(), Some(step.trim())),
            None => (content.trim(), None),
        };
        if !window.is_empty() {
            self.durations = self.durations.saturating_add(duration(window)?);
        } else if step.is_none() {
            return Err(Reject::InvalidQuery);
        }
        if let Some(step) = step.filter(|s| !s.is_empty()) {
            let step = duration(step)?;
            self.durations = self.durations.saturating_add(step);
            self.steps = self.steps.max(step);
        }
        if step.is_some() || self.last != Last::Selector {
            self.subqueries = self.subqueries.saturating_add(1);
        }
        Ok(())
    }

    /// The word at the start of `rest`, with its duration for `offset`; their length.
    fn word(&mut self, rest: &str) -> Result<usize, Reject> {
        let len = rest
            .bytes()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b':' | b'.'))
            .count();
        let word = rest
            .get(..len)
            .ok_or(Reject::InvalidQuery)?
            .to_ascii_lowercase();
        let word = word.as_str();
        let last = std::mem::replace(&mut self.last, Last::Other);
        if self.braces {
        } else if word == "with" {
            return Err(Reject::InvalidQuery);
        } else if word == "offset" {
            if !matches!(last, Last::Selector | Last::Window) {
                self.frame()?.nested = true;
            }
            let tail = rest.get(len..).ok_or(Reject::InvalidQuery)?;
            let literal = tail.trim_start();
            let literal = literal.strip_prefix('-').unwrap_or(literal).trim_start();
            let digits = literal
                .bytes()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == b'.')
                .count();
            let offset = duration(literal.get(..digits).ok_or(Reject::InvalidQuery)?)?;
            self.durations = self.durations.saturating_add(offset);
            return Ok(len + (tail.len() - literal.len()) + digits);
        } else if WORD_OPERATORS.contains(&word) {
            self.frame()?.nested = true;
        } else if MODIFIERS.contains(&word) {
            self.last = Last::Modifier {
                aggregate: matches!(word, "by" | "without") && last == Last::Aggregate,
            };
        } else if AGGREGATES.contains(&word) {
            self.last = Last::Aggregate;
        } else if !matches!(word, "inf" | "nan" | "keep_metric_names") {
            self.last = Last::Selector;
        }
        Ok(len)
    }
}

/// The length of the string at the start of `s`, as `MetricsQL`'s `scanString` finds it: up to
/// the first quote like the opening one that follows an even number of backslashes.
fn string(s: &str) -> Result<usize, Reject> {
    let bytes = s.as_bytes();
    let quote = *bytes.first().ok_or(Reject::InvalidQuery)?;
    let mut i = 1;
    loop {
        let n = bytes
            .get(i..)
            .and_then(|tail| tail.iter().position(|&c| c == quote))
            .ok_or(Reject::InvalidQuery)?;
        i += n;
        let backslashes = bytes.get(..i).map_or(0, |head| {
            head.iter().rev().take_while(|&&c| c == b'\\').count()
        });
        if backslashes % 2 == 0 {
            return Ok(i + 1);
        }
        i += 1;
    }
}

/// A duration literal in milliseconds: whole seconds, or whole numbers each with `ms`, `s`, `m`,
/// `h`, `d`, `w` or `y` (`1h30m`).
fn duration(text: &str) -> Result<i64, Reject> {
    if !text.is_empty() && text.bytes().all(|c| c.is_ascii_digit()) {
        return text
            .parse::<i64>()
            .map(|seconds| seconds.saturating_mul(1000))
            .map_err(|_overflow| Reject::InvalidQuery);
    }
    let mut total: i64 = 0;
    let mut rest = text;
    if rest.is_empty() {
        return Err(Reject::InvalidQuery);
    }
    while !rest.is_empty() {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        let number: i64 = rest
            .get(..digits)
            .filter(|n| !n.is_empty())
            .and_then(|n| n.parse().ok())
            .ok_or(Reject::InvalidQuery)?;
        let tail = rest.get(digits..).ok_or(Reject::InvalidQuery)?;
        let (unit, len) = if tail.starts_with("ms") {
            (1, 2)
        } else {
            let unit = match tail.bytes().next() {
                Some(b's') => 1000,
                Some(b'm') => 60 * 1000,
                Some(b'h') => 3600 * 1000,
                Some(b'd') => 86_400 * 1000,
                Some(b'w') => 7 * 86_400 * 1000,
                Some(b'y') => 365 * 86_400 * 1000,
                _ => return Err(Reject::InvalidQuery),
            };
            (unit, 1)
        };
        total = total.saturating_add(number.saturating_mul(unit));
        rest = tail.get(len..).ok_or(Reject::InvalidQuery)?;
    }
    Ok(total)
}

/// Queries of every shape [`reach`] bounds, for tests.
#[cfg(test)]
pub(crate) fn query() -> impl proptest::strategy::Strategy<Value = String> {
    use proptest::prelude::*;
    let duration = prop_oneof![
        (0_u32..7200).prop_map(|n| format!("{n}s")),
        (0_u32..600).prop_map(|n| format!("{n}m")),
        (0_u32..72).prop_map(|n| format!("{n}h")),
        (0_u32..9).prop_map(|n| format!("{n}d")),
        (0_u32..7200).prop_map(|n| n.to_string()),
        (0_u32..9, 0_u32..60).prop_map(|(h, m)| format!("{h}h{m}m")),
        (0_u32..90_000).prop_map(|n| format!("{n}ms")),
    ];
    let selector = prop_oneof![
        Just("up".to_owned()),
        Just("x{a=\"b\",c!~\"d\"}".to_owned()),
        Just("{__name__=\"up\" or a=\"[1y] offset 1y\"}".to_owned()),
        Just("y{a='x[2y]', b=`(`}".to_owned()),
    ];
    let leaf = prop_oneof![
        selector.clone(),
        (selector, duration.clone()).prop_map(|(s, d)| format!("{s}[{d}]")),
        Just("0.9".to_owned()),
        Just("inf".to_owned()),
        (0_u32..600).prop_map(|n| format!("0.9[{n}s]")),
        Just("time()".to_owned()),
    ];
    leaf.prop_recursive(6, 48, 2, move |inner| {
        let d = duration.clone();
        prop_oneof![
            (inner.clone(), d.clone(), prop::option::of(d.clone()))
                .prop_map(|(e, w, s)| format!("({e})[{w}:{}]", s.unwrap_or_default())),
            (inner.clone(), d.clone(), d.clone()).prop_map(|(e, w, s)| format!("{e}[{w}:{s}]")),
            (inner.clone(), d.clone()).prop_map(|(e, w)| format!("({e})[{w}]")),
            (inner.clone(), d.clone()).prop_map(|(e, w)| format!("{e}[{w}]")),
            (inner.clone(), d.clone()).prop_map(|(e, w)| format!("{e} OFFSET {w}")),
            inner.clone().prop_map(|e| format!("-{e}")),
            inner.clone().prop_map(|e| format!("SUM BY (pod) ({e})")),
            (inner.clone(), any::<bool>(), d).prop_map(|(e, minus, o)| format!(
                "{e} offset {}{o}",
                if minus { "-" } else { "" }
            )),
            (
                prop::sample::select(vec!["rate", "max_over_time", "abs", "increase"]),
                inner.clone()
            )
                .prop_map(|(f, e)| format!("{f}({e})")),
            inner
                .clone()
                .prop_map(|e| format!("quantile_over_time(0.9, {e})")),
            inner
                .clone()
                .prop_map(|e| format!("rate({e}) keep_metric_names")),
            inner.clone().prop_map(|e| format!("sum({e})")),
            inner.clone().prop_map(|e| format!("sum by (pod) ({e})")),
            inner.clone().prop_map(|e| format!("max({e}) without (a)")),
            inner.clone().prop_map(|e| format!("({e})")),
            (inner.clone(), inner.clone()).prop_map(|(a, b)| format!("{a} + {b}")),
            (inner.clone(), inner.clone()).prop_map(|(a, b)| format!("{a} and {b}")),
            (inner.clone(), inner.clone())
                .prop_map(|(a, b)| format!("{a} / on(a) group_left(b) {b}")),
            inner.clone().prop_map(|e| format!("{e} > bool 0")),
            (inner.clone(), inner).prop_map(|(a, b)| format!("({a}, {b})")),
        ]
    })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const MINUTE: i64 = 60 * 1000;
    const STEP: i64 = MINUTE;
    /// What every query reads: a selector's step and 5 minutes, the step its start may be aligned
    /// down by, and the latency offset.
    const BASE: i64 = 2 * STEP + SILENCE + LATENCY;
    /// What each level a subquery may nest in adds.
    const LEVEL: i64 = 3 * STEP + SILENCE;

    #[test]
    fn reach_adds_every_duration_and_a_level_for_each_subquery() {
        let cases: &[(&str, &str, i64)] = &[
            ("a bare selector", "up", BASE),
            (
                "Zeabur's CPU query",
                "sum by (pod) (rate(container_cpu_usage_seconds_total{namespace=\"a\",pod=~\"^web-[a-z0-9]{1,10}-[a-z0-9]{5}$\"}[120s]))",
                BASE + 2 * MINUTE,
            ),
            ("an offset", "up offset 1d", BASE + 1440 * MINUTE),
            ("a negative offset", "up offset -1h", BASE + 60 * MINUTE),
            ("a combined duration", "rate(up[1h30m])", BASE + 90 * MINUTE),
            ("seconds without a unit", "rate(up[300])", BASE + 5 * MINUTE),
            (
                "every duration, also in other operands",
                "rate(up[1h]) + up offset 1h",
                BASE + 120 * MINUTE,
            ),
            (
                "a subquery's window and step",
                "max_over_time(rate(up[5m])[1h:10m])",
                2 * 10 * MINUTE + SILENCE + LATENCY + 75 * MINUTE + 2 * (30 * MINUTE + SILENCE),
            ),
            (
                "a window over a parenthesis is a subquery",
                "max_over_time((up)[1h])",
                BASE + 60 * MINUTE + 2 * LEVEL,
            ),
            ("a rollup over an expression", "rate(sum(up))", BASE + LEVEL),
            ("nested expressions", "abs(abs(up + up))", BASE + 2 * LEVEL),
            (
                "brackets and words in strings",
                "up{a=\"[1y] offset 1y @ #\", b='x[2y]', c=`[3y]`}",
                BASE,
            ),
            ("an escaped quote", r#"up{a="x\"[1y]"}"#, BASE),
            (
                "an escaped backslash ends the string",
                r#"up{a="x\\"}[1y]"#,
                BASE + 365 * 1440 * MINUTE,
            ),
        ];
        for (name, query, read) in cases {
            assert_eq!(reach(query, STEP), Ok(*read), "{name}: {query}");
        }
    }

    #[test]
    fn what_cannot_be_bounded_is_refused() {
        for query in [
            "up @ 1700000000",
            "rate(up[5m]) @ start()",
            "WITH (w = 5m) rate(up[w])",
            "rate(up[w])",
            "rate(up[1i])",
            "rate(up[$__interval])",
            "rate(up[1.5h])",
            "rate(up[1h-5m])",
            "rate(up[])",
            "up offset w",
            "up offset 1.5h",
            "up offset",
            "up # [1y]\n",
            "up\\x",
            "métrique",
            "up{a=\"b\"",
            "up{a=(\"b\")}",
            "up[5m",
            "sum(up",
            "up )",
        ] {
            assert_eq!(reach(query, STEP), Err(Reject::InvalidQuery), "{query}");
        }
    }

    proptest! {
        #[test]
        fn never_panics(query in "[ -~]{0,200}") {
            if let Ok(read) = reach(&query, STEP) {
                prop_assert!(read >= BASE);
            }
        }

        /// Every generated query is bounded.
        #[test]
        fn every_generated_query_is_bounded(query in query(), step in 1_i64..3_600_000) {
            prop_assert!(reach(&query, step).is_ok(), "{}", query);
        }
    }
}

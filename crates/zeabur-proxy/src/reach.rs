//! How far before its first point a metrics query reads samples.
//!
//! VictoriaMetrics (v1.152.0, `app/vmselect/promql/eval.go`) reads samples before the time it
//! evaluates: a range selector's window, a subquery's window and step, an `offset`, and for every
//! selector and subquery the step and 5 minutes more (`maxSilenceInterval` while
//! `-search.minStalenessInterval` is unset). A rollup function over an expression that is not a
//! selector is an implicit subquery with the step as window and step. [`reach`] bounds the sum
//! along the deepest path of the query and refuses what it cannot bound: `@`, `WITH`, comments,
//! durations that are not literals, deeper nesting than [`MAX_DEPTH`].
//!
//! The lexer follows `MetricsQL`'s (`github.com/VictoriaMetrics/metricsql` v0.87.4) for strings,
//! so a bracket or `offset` inside a string is never counted and one outside is never missed.
//! Anything it does not know is refused.

use crate::reject::Reject;

/// The deepest nesting accepted.
pub const MAX_DEPTH: usize = 32;

/// `maxSilenceInterval`: VictoriaMetrics reads this much more before each selector and subquery.
const SILENCE: i64 = 5 * 60 * 1000;

/// `-search.latencyOffset`: VictoriaMetrics may move an instant query's time this much earlier.
const LATENCY: i64 = 30 * 1000;

/// `MetricsQL`'s aggregate functions: none reads samples itself.
const AGGREGATES: &[&str] = &[
    "any",
    "avg",
    "bottomk",
    "bottomk_avg",
    "bottomk_last",
    "bottomk_max",
    "bottomk_median",
    "bottomk_min",
    "count",
    "count_values",
    "distinct",
    "geomean",
    "group",
    "histogram",
    "limitk",
    "mad",
    "max",
    "median",
    "min",
    "mode",
    "outliers_iqr",
    "outliers_mad",
    "outliersk",
    "quantile",
    "quantiles",
    "share",
    "stddev",
    "stdvar",
    "sum",
    "sum2",
    "topk",
    "topk_avg",
    "topk_last",
    "topk_max",
    "topk_median",
    "topk_min",
    "zscore",
];

/// Binary operators that are words.
const WORD_OPERATORS: &[&str] = &["and", "or", "unless", "if", "ifnot", "default", "atan2"];

/// The milliseconds before its first point that `query` may read samples from, when VictoriaMetrics
/// evaluates it every `step` milliseconds (an instant query's step for one).
///
/// # Errors
/// [`Reject::InvalidQuery`] when the query uses what this cannot bound, or is not `MetricsQL`.
pub fn reach(query: &str, step: i64) -> Result<i64, Reject> {
    let tokens = lex(query)?;
    let mut parser = Parser { tokens, at: 0 };
    let ast = parser.expr(0)?;
    if parser.at != parser.tokens.len() {
        return Err(Reject::InvalidQuery);
    }
    Ok(eval(&ast, step)
        .saturating_add(step)
        .saturating_add(LATENCY))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token<'a> {
    Ident(&'a str),
    Text,
    Number(&'a str),
    Punct(u8),
    Op(&'a str),
    /// `[window]` or `[window:step]`, in milliseconds; `subquery` when there is a `:`.
    Window {
        window: Option<i64>,
        step: Option<i64>,
        subquery: bool,
    },
}

fn lex(query: &str) -> Result<Vec<Token<'_>>, Reject> {
    let bytes = query.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        let rest = query.get(i..).ok_or(Reject::InvalidQuery)?;
        let (token, len) = match b {
            b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r' => {
                i += 1;
                continue;
            }
            b'{' | b'}' | b'(' | b')' | b',' => (Token::Punct(b), 1),
            b'[' => {
                let end = rest.find(']').ok_or(Reject::InvalidQuery)?;
                (
                    window(rest.get(1..end).ok_or(Reject::InvalidQuery)?)?,
                    end + 1,
                )
            }
            b'"' | b'\'' | b'`' => (Token::Text, string(rest)?),
            b'a'..=b'z' | b'A'..=b'Z' | b'_' | b':' => {
                let len = rest
                    .bytes()
                    .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b':' | b'.'))
                    .count();
                (
                    Token::Ident(rest.get(..len).ok_or(Reject::InvalidQuery)?),
                    len,
                )
            }
            b'0'..=b'9' | b'.' => {
                let len = rest
                    .bytes()
                    .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.'))
                    .count();
                (
                    Token::Number(rest.get(..len).ok_or(Reject::InvalidQuery)?),
                    len,
                )
            }
            _ => {
                let len = ["==", "!=", ">=", "<=", "=~", "!~"]
                    .iter()
                    .find(|op| rest.starts_with(*op))
                    .map(|op| op.len())
                    .or_else(|| {
                        matches!(
                            b,
                            b'+' | b'-' | b'*' | b'/' | b'%' | b'^' | b'>' | b'<' | b'='
                        )
                        .then_some(1)
                    })
                    .ok_or(Reject::InvalidQuery)?;
                (Token::Op(rest.get(..len).ok_or(Reject::InvalidQuery)?), len)
            }
        };
        tokens.push(token);
        i += len;
    }
    Ok(tokens)
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

fn window(content: &str) -> Result<Token<'static>, Reject> {
    let (window, step, subquery) = match content.split_once(':') {
        Some((window, step)) => (window.trim(), Some(step.trim()), true),
        None => (content.trim(), None, false),
    };
    let window = if window.is_empty() {
        None
    } else {
        Some(duration(window)?)
    };
    if window.is_none() && !subquery {
        return Err(Reject::InvalidQuery);
    }
    let step = match step {
        None | Some("") => None,
        Some(step) => Some(duration(step)?),
    };
    Ok(Token::Window {
        window,
        step,
        subquery,
    })
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

#[derive(Debug)]
enum Ast {
    Scalar,
    Selector,
    /// Several expressions whose reach is the largest of theirs: operands, a union, arguments.
    All(Vec<Self>),
    Aggregate(Vec<Self>),
    Call(Vec<Self>),
    Window {
        inner: Box<Self>,
        window: Option<i64>,
        step: Option<i64>,
        subquery: bool,
    },
    Offset(Box<Self>, i64),
}

impl Ast {
    /// Whether a rollup function reads this directly rather than through an implicit subquery.
    fn is_series(&self) -> bool {
        match self {
            Self::Selector | Self::Window { .. } => true,
            Self::Offset(inner, _) => inner.is_series(),
            Self::Scalar | Self::All(_) | Self::Aggregate(_) | Self::Call(_) => false,
        }
    }
}

fn eval(ast: &Ast, step: i64) -> i64 {
    match ast {
        Ast::Scalar => 0,
        Ast::Selector => step.saturating_add(SILENCE),
        Ast::All(items) | Ast::Aggregate(items) => {
            items.iter().map(|item| eval(item, step)).max().unwrap_or(0)
        }
        Ast::Call(args) => args
            .iter()
            .map(|arg| {
                if matches!(arg, Ast::Scalar) {
                    0
                } else if arg.is_series() {
                    eval(arg, step)
                } else {
                    eval(arg, step)
                        .saturating_add(step.saturating_mul(3))
                        .saturating_add(SILENCE)
                }
            })
            .max()
            .unwrap_or(0),
        Ast::Window {
            inner,
            window,
            step: own,
            subquery,
        } => {
            if !subquery && matches!(**inner, Ast::Selector) {
                return window.unwrap_or(0).max(step).saturating_add(SILENCE);
            }
            let inner_step = own.filter(|own| *own > 0).unwrap_or(step).max(1);
            let window = window.unwrap_or(0).max(step).max(inner_step);
            eval(inner, inner_step)
                .saturating_add(window)
                .saturating_add(inner_step.saturating_mul(2))
                .saturating_add(SILENCE)
        }
        Ast::Offset(inner, offset) => eval(inner, step).saturating_add(*offset),
    }
}

struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    at: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Token<'a>> {
        self.tokens.get(self.at)
    }

    fn peek_ident(&self) -> Option<String> {
        match self.peek() {
            Some(Token::Ident(name)) => Some(name.to_ascii_lowercase()),
            _ => None,
        }
    }

    fn next(&mut self) -> Result<Token<'a>, Reject> {
        let token = self
            .tokens
            .get(self.at)
            .cloned()
            .ok_or(Reject::InvalidQuery)?;
        self.at += 1;
        Ok(token)
    }

    fn eat_punct(&mut self, punct: u8) -> bool {
        if self.peek() == Some(&Token::Punct(punct)) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expect_punct(&mut self, punct: u8) -> Result<(), Reject> {
        if self.eat_punct(punct) {
            Ok(())
        } else {
            Err(Reject::InvalidQuery)
        }
    }

    fn eat_word(&mut self, words: &[&str]) -> bool {
        if self
            .peek_ident()
            .is_some_and(|w| words.contains(&w.as_str()))
        {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expr(&mut self, depth: usize) -> Result<Ast, Reject> {
        if depth > MAX_DEPTH {
            return Err(Reject::InvalidQuery);
        }
        let mut operands = vec![self.unary(depth)?];
        while self.binary_operator() {
            self.eat_word(&["bool"]);
            if self.eat_word(&["on", "ignoring"]) {
                self.labels()?;
            }
            if self.eat_word(&["group_left", "group_right"]) {
                if self.peek() == Some(&Token::Punct(b'(')) {
                    self.labels()?;
                }
                if self.eat_word(&["prefix"]) && self.next()? != Token::Text {
                    return Err(Reject::InvalidQuery);
                }
            }
            operands.push(self.unary(depth)?);
        }
        Ok(if operands.len() == 1 {
            operands.pop().ok_or(Reject::InvalidQuery)?
        } else {
            Ast::All(operands)
        })
    }

    fn binary_operator(&mut self) -> bool {
        match self.peek() {
            Some(Token::Op(op)) if !matches!(*op, "=" | "=~" | "!~") => {
                self.at += 1;
                true
            }
            _ => self.eat_word(WORD_OPERATORS),
        }
    }

    fn unary(&mut self, depth: usize) -> Result<Ast, Reject> {
        while matches!(self.peek(), Some(Token::Op("+" | "-"))) {
            self.at += 1;
        }
        let mut ast = self.primary(depth)?;
        loop {
            if let Some(Token::Window {
                window,
                step,
                subquery,
            }) = self.peek().cloned()
            {
                self.at += 1;
                ast = Ast::Window {
                    inner: Box::new(ast),
                    window,
                    step,
                    subquery,
                };
            } else if self.eat_word(&["offset"]) {
                if matches!(self.peek(), Some(Token::Op("-"))) {
                    self.at += 1;
                }
                let Token::Number(text) = self.next()? else {
                    return Err(Reject::InvalidQuery);
                };
                ast = Ast::Offset(Box::new(ast), duration(text)?);
            } else if !self.eat_word(&["keep_metric_names"]) {
                return Ok(ast);
            }
        }
    }

    fn primary(&mut self, depth: usize) -> Result<Ast, Reject> {
        match self.next()? {
            Token::Number(_) | Token::Text => Ok(Ast::Scalar),
            Token::Punct(b'(') => Ok(Ast::All(self.arguments(depth)?)),
            Token::Punct(b'{') => {
                self.matchers()?;
                Ok(Ast::Selector)
            }
            Token::Ident(name) => {
                let name = name.to_ascii_lowercase();
                if matches!(name.as_str(), "offset" | "with" | "bool") {
                    return Err(Reject::InvalidQuery);
                }
                let grouped = matches!(self.peek_ident().as_deref(), Some("by" | "without"));
                let called = self.peek() == Some(&Token::Punct(b'('));
                if AGGREGATES.contains(&name.as_str()) && (grouped || called) {
                    if self.eat_word(&["by", "without"]) {
                        self.labels()?;
                    }
                    self.expect_punct(b'(')?;
                    let args = self.arguments(depth)?;
                    if self.eat_word(&["by", "without"]) {
                        self.labels()?;
                    }
                    if self.eat_word(&["limit"]) && !matches!(self.next()?, Token::Number(_)) {
                        return Err(Reject::InvalidQuery);
                    }
                    return Ok(Ast::Aggregate(args));
                }
                if called {
                    self.at += 1;
                    return Ok(Ast::Call(self.arguments(depth)?));
                }
                if matches!(name.as_str(), "inf" | "nan") {
                    return Ok(Ast::Scalar);
                }
                if self.eat_punct(b'{') {
                    self.matchers()?;
                }
                Ok(Ast::Selector)
            }
            Token::Punct(_) | Token::Op(_) | Token::Window { .. } => Err(Reject::InvalidQuery),
        }
    }

    /// Expressions separated by commas up to `)`; the `(` is already read.
    fn arguments(&mut self, depth: usize) -> Result<Vec<Ast>, Reject> {
        let mut args = Vec::new();
        if self.eat_punct(b')') {
            return Ok(args);
        }
        loop {
            args.push(self.expr(depth + 1)?);
            if self.eat_punct(b')') {
                return Ok(args);
            }
            self.expect_punct(b',')?;
        }
    }

    /// `(label, …)`.
    fn labels(&mut self) -> Result<(), Reject> {
        self.expect_punct(b'(')?;
        loop {
            match self.next()? {
                Token::Punct(b')') => return Ok(()),
                Token::Ident(_) | Token::Text => {}
                Token::Number(_) | Token::Punct(_) | Token::Op(_) | Token::Window { .. } => {
                    return Err(Reject::InvalidQuery);
                }
            }
            if !self.eat_punct(b',') {
                return self.expect_punct(b')');
            }
        }
    }

    /// `label op "value"`, separated by `,` or `or`, up to `}`; the `{` is already read.
    fn matchers(&mut self) -> Result<(), Reject> {
        loop {
            match self.next()? {
                Token::Punct(b'}') => return Ok(()),
                Token::Ident(_) | Token::Text => {}
                Token::Number(_) | Token::Punct(_) | Token::Op(_) | Token::Window { .. } => {
                    return Err(Reject::InvalidQuery);
                }
            }
            if let Some(Token::Op("=" | "!=" | "=~" | "!~")) = self.peek() {
                self.at += 1;
                if self.next()? != Token::Text {
                    return Err(Reject::InvalidQuery);
                }
            }
            if !self.eat_punct(b',') && !self.eat_word(&["or"]) {
                return self.expect_punct(b'}');
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const MINUTE: i64 = 60 * 1000;
    const STEP: i64 = MINUTE;
    /// What every query adds: the step VictoriaMetrics may align the start down by, and its
    /// latency offset.
    const EDGE: i64 = STEP + LATENCY;

    /// The reach of each query at a 1-minute step, as VictoriaMetrics reads it.
    #[test]
    fn reach_follows_how_victoriametrics_reads() {
        let cases: &[(&str, &str, i64)] = &[
            ("a bare selector", "up", STEP + SILENCE),
            (
                "Zeabur's CPU query",
                "sum by (pod) (rate(container_cpu_usage_seconds_total{namespace=\"a\",pod=~\"^web-[a-z0-9]{1,10}-[a-z0-9]{5}$\"}[120s]))",
                2 * MINUTE + SILENCE,
            ),
            (
                "a window shorter than the step",
                "rate(up[30s])",
                STEP + SILENCE,
            ),
            ("an offset", "up offset 1d", STEP + SILENCE + 1440 * MINUTE),
            (
                "a negative offset still counts",
                "up offset -1h",
                STEP + SILENCE + 60 * MINUTE,
            ),
            (
                "a combined duration",
                "rate(up[1h30m])",
                90 * MINUTE + SILENCE,
            ),
            (
                "seconds without a unit",
                "rate(up[300])",
                5 * MINUTE + SILENCE,
            ),
            (
                "the deeper of two operands",
                "up + rate(up[1h])",
                60 * MINUTE + SILENCE,
            ),
            (
                "a subquery",
                "max_over_time(rate(up[5m])[1h:10m])",
                10 * MINUTE + SILENCE + 60 * MINUTE + 20 * MINUTE + SILENCE,
            ),
            (
                "a subquery with the step inherited",
                "max_over_time(up[1h:])",
                STEP + SILENCE + 60 * MINUTE + 2 * STEP + SILENCE,
            ),
            (
                "a zero subquery step is the step inherited",
                "max_over_time(up[1h:0s])",
                STEP + SILENCE + 60 * MINUTE + 2 * STEP + SILENCE,
            ),
            (
                "an implicit subquery",
                "rate(sum(up))",
                STEP + SILENCE + 3 * STEP + SILENCE,
            ),
            (
                "an offset on a subquery",
                "max_over_time(up[1h:]) offset 1h",
                STEP + SILENCE + 60 * MINUTE + 2 * STEP + SILENCE + 60 * MINUTE,
            ),
            (
                "a scalar argument",
                "quantile_over_time(0.9, up[1h])",
                60 * MINUTE + SILENCE,
            ),
            (
                "brackets and words in strings",
                "up{a=\"[1y] offset 1y @ #\", b='x[2y]', c=`[3y]`}",
                STEP + SILENCE,
            ),
            ("an escaped quote", r#"up{a="x\"[1y]"}"#, STEP + SILENCE),
            (
                "an escaped backslash ends the string",
                r#"up{a="x\\"}[1y]"#,
                365 * 1440 * MINUTE + SILENCE,
            ),
            (
                "modifiers",
                "sum(rate(up[5m])) by (a) limit 3 / on(a) group_left(b) prefix \"p_\" count without (c) (up) > bool 0",
                5 * MINUTE + SILENCE,
            ),
            ("a union", "(up, rate(up[2h]))", 120 * MINUTE + SILENCE),
            (
                "a quoted metric name",
                "{\"up\", a=\"b\" or c!~\"d\"}",
                STEP + SILENCE,
            ),
        ];
        for (name, query, read) in cases {
            assert_eq!(reach(query, STEP), Ok(read + EDGE), "{name}: {query}");
        }
    }

    /// What cannot be bounded is refused, not guessed.
    #[test]
    fn what_cannot_be_bounded_is_refused() {
        let deep = format!(
            "{}up{}",
            "abs(".repeat(MAX_DEPTH + 2),
            ")".repeat(MAX_DEPTH + 2)
        );
        for query in [
            "up @ 1700000000",
            "up @ end()",
            "rate(up[5m]) @ start()",
            "WITH (w = 5m) rate(up[w])",
            "with (f(x) = x) f(up)",
            "rate(up[w])",
            "rate(up[1i])",
            "rate(up[$__interval])",
            "rate(up[1.5h])",
            "rate(up[1h-5m])",
            "rate(up[5H])",
            "rate(up[])",
            "up offset w",
            "up offset 1.5h",
            "up # [1y]\n",
            "up\\x",
            "métrique",
            "up{a=\"b\"",
            "up[5m",
            "sum(up",
            "up )",
            "offset",
            deep.as_str(),
        ] {
            assert_eq!(reach(query, STEP), Err(Reject::InvalidQuery), "{query}");
        }
    }

    proptest! {
        /// Nothing panics, and a query that is accepted reaches back at least a selector's read.
        #[test]
        fn never_panics(query in "[ -~]{0,200}") {
            if let Ok(read) = reach(&query, STEP) {
                prop_assert!(read >= EDGE);
            }
        }

        /// Wrapping a query in a window, a subquery or an offset never makes it reach less.
        #[test]
        fn wrapping_never_reaches_less(
            minutes in 1_i64..10_000,
            inner in prop::sample::select(vec!["up", "rate(up[5m])", "sum(up)", "max_over_time(up[1h:5m])"]),
            wrap in 0_usize..4,
        ) {
            let wrapped = match wrap {
                0 => format!("({inner}) offset {minutes}m"),
                1 => format!("max_over_time(({inner})[{minutes}m:])"),
                2 => format!("rate({inner})"),
                _ => format!("sum(({inner}) + up)"),
            };
            prop_assert!(reach(&wrapped, STEP)? >= reach(inner, STEP)?);
        }
    }
}

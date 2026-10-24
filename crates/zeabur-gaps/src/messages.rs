//! The words for the sentences this crate sends, in each language an interface shows.
//!
//! The catalogs in `messages/` hold one template per key; the web renders the same templates. The
//! format is described in `docs/internal/wording.md`.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde_json::Value;
use zeabur_kernel::message::Message;

/// A language the catalogs are written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    /// English.
    En,
    /// Traditional Chinese, as written in Taiwan.
    ZhTw,
    /// Simplified Chinese.
    ZhCn,
}

impl Language {
    /// Every language, in the order the interfaces list them.
    pub const ALL: [Self; 3] = [Self::En, Self::ZhTw, Self::ZhCn];

    /// Its BCP 47 tag: `en`, `zh-TW`, `zh-CN`.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::ZhTw => "zh-TW",
            Self::ZhCn => "zh-CN",
        }
    }

    /// The language with the tag `tag`.
    #[must_use]
    pub fn from_tag(tag: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|language| language.tag() == tag)
    }
}

/// `message` in `language`; its key when the catalog has no sentence for it.
#[must_use]
pub fn render(message: &Message, language: Language) -> String {
    let catalog = catalog(language);
    match catalog.messages.get(message.key.as_str()) {
        Some((_, Ok(nodes))) => write(nodes, &message.params, catalog, ""),
        _ => message.key.clone(),
    }
}

/// The template for `key` in `language`.
#[must_use]
pub fn template(key: &str, language: Language) -> Option<&'static str> {
    catalog(language)
        .messages
        .get(key)
        .map(|(source, _)| source.as_str())
}

/// Every key the catalog for `language` has a sentence for.
pub fn keys(language: Language) -> impl Iterator<Item = &'static str> {
    catalog(language).messages.keys().map(String::as_str)
}

/// The templates in `language` that are not well-formed, with why.
#[must_use]
pub fn malformed(language: Language) -> Vec<(&'static str, String)> {
    let catalog = catalog(language);
    let format = [&catalog.seconds, &catalog.minutes];
    catalog
        .messages
        .iter()
        .map(|(key, (_, nodes))| (key.as_str(), nodes))
        .chain(format.into_iter().map(|(_, nodes)| ("format", nodes)))
        .filter_map(|(key, nodes)| nodes.as_ref().err().map(|why| (key, why.clone())))
        .collect()
}

type Parsed = (String, Result<Vec<Node>, String>);

struct Catalog {
    list_separator: String,
    list_and: String,
    seconds: Parsed,
    minutes: Parsed,
    messages: BTreeMap<String, Parsed>,
}

fn catalog(language: Language) -> &'static Catalog {
    static EN: LazyLock<Catalog> = LazyLock::new(|| load(include_str!("../messages/en.json")));
    static ZH_TW: LazyLock<Catalog> =
        LazyLock::new(|| load(include_str!("../messages/zh-TW.json")));
    static ZH_CN: LazyLock<Catalog> =
        LazyLock::new(|| load(include_str!("../messages/zh-CN.json")));
    match language {
        Language::En => &EN,
        Language::ZhTw => &ZH_TW,
        Language::ZhCn => &ZH_CN,
    }
}

fn load(json: &str) -> Catalog {
    let json: Value = serde_json::from_str(json).unwrap_or_default();
    let text = |name: &str| {
        json.get("format")
            .and_then(|format| format.get(name))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let parsed = |source: String| {
        let nodes = parse(&source);
        (source, nodes)
    };
    Catalog {
        list_separator: text("listSeparator"),
        list_and: text("listAnd"),
        seconds: parsed(text("seconds")),
        minutes: parsed(text("minutes")),
        messages: json
            .get("messages")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .map(|(key, source)| {
                let source = source.as_str().unwrap_or_default().to_owned();
                (key.clone(), parsed(source))
            })
            .collect(),
    }
}

#[derive(Debug)]
enum Node {
    Text(String),
    Hash,
    Placeholder(Placeholder),
}

#[derive(Debug)]
struct Placeholder {
    name: String,
    kind: String,
    style: String,
    cases: BTreeMap<String, Vec<Node>>,
}

fn parse(source: &str) -> Result<Vec<Node>, String> {
    let mut parser = Parser {
        chars: source.chars().collect(),
        at: 0,
    };
    parser.nodes(false, false)
}

struct Parser {
    chars: Vec<char>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn nodes(&mut self, in_case: bool, hash: bool) -> Result<Vec<Node>, String> {
        let mut nodes = Vec::new();
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c == '$' && self.chars.get(self.at + 1) == Some(&'{') {
                flush(&mut nodes, &mut text);
                self.at += 2;
                nodes.push(Node::Placeholder(self.placeholder(hash)?));
            } else if c == '}' && in_case {
                flush(&mut nodes, &mut text);
                return Ok(nodes);
            } else if c == '#' && hash {
                flush(&mut nodes, &mut text);
                nodes.push(Node::Hash);
                self.at += 1;
            } else {
                text.push(c);
                self.at += 1;
            }
        }
        if in_case {
            return Err("a case is not closed".to_owned());
        }
        flush(&mut nodes, &mut text);
        Ok(nodes)
    }

    fn space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.at += 1;
        }
    }

    fn word(&mut self) -> String {
        self.space();
        let mut word = String::new();
        while let Some(c) = self
            .peek()
            .filter(|c| !c.is_whitespace() && !matches!(c, ',' | '{' | '}'))
        {
            word.push(c);
            self.at += 1;
        }
        word
    }

    fn placeholder(&mut self, hash: bool) -> Result<Placeholder, String> {
        let mut placeholder = Placeholder {
            name: self.word(),
            kind: String::new(),
            style: String::new(),
            cases: BTreeMap::new(),
        };
        self.space();
        if self.peek() == Some(',') {
            self.at += 1;
            placeholder.kind = self.word();
            if !matches!(
                placeholder.kind.as_str(),
                "select" | "plural" | "items" | "first" | "list" | "duration"
            ) {
                return Err(format!("unknown kind {}", placeholder.kind));
            }
            let hash = hash || placeholder.kind == "plural";
            loop {
                self.space();
                if self.peek() == Some(',') {
                    self.at += 1;
                }
                let key = self.word();
                if key.is_empty() {
                    break;
                }
                self.space();
                if self.peek() == Some('{') {
                    self.at += 1;
                    let body = self.nodes(true, hash)?;
                    self.at += 1;
                    placeholder.cases.insert(key, body);
                } else {
                    placeholder.style = key;
                }
            }
        }
        self.space();
        if self.peek() != Some('}') {
            return Err(format!("placeholder {} is not closed", placeholder.name));
        }
        self.at += 1;
        Ok(placeholder)
    }
}

fn flush(nodes: &mut Vec<Node>, text: &mut String) {
    if !text.is_empty() {
        nodes.push(Node::Text(std::mem::take(text)));
    }
}

fn write(
    nodes: &[Node],
    params: &BTreeMap<String, String>,
    catalog: &Catalog,
    hash: &str,
) -> String {
    nodes
        .iter()
        .map(|node| match node {
            Node::Text(text) => text.clone(),
            Node::Hash => hash.to_owned(),
            Node::Placeholder(placeholder) => fill(placeholder, params, catalog, hash),
        })
        .collect()
}

fn pick<'a>(cases: &'a BTreeMap<String, Vec<Node>>, key: &str) -> &'a [Node] {
    cases
        .get(key)
        .or_else(|| cases.get("other"))
        .map_or(&[], Vec::as_slice)
}

fn fill(
    placeholder: &Placeholder,
    params: &BTreeMap<String, String>,
    catalog: &Catalog,
    hash: &str,
) -> String {
    let value = params.get(&placeholder.name).map(String::as_str);
    let items: Vec<&str> = value.unwrap_or_default().split(',').collect();
    let cases = &placeholder.cases;
    let write = |nodes: &[Node], hash: &str| write(nodes, params, catalog, hash);
    match placeholder.kind.as_str() {
        "select" => {
            let key = match value {
                Some(value) if !value.is_empty() => value,
                _ if cases.contains_key("none") => "none",
                _ => "other",
            };
            write(pick(cases, key), hash)
        }
        "plural" => {
            let one = (number(value) - 1.0).abs() < f64::EPSILON;
            write(
                pick(cases, if one { "one" } else { "other" }),
                value.unwrap_or_default(),
            )
        }
        "items" => write(
            pick(cases, if items.len() == 1 { "one" } else { "other" }),
            hash,
        ),
        "first" => write(
            pick(cases, items.first().copied().unwrap_or_default()),
            hash,
        ),
        "list" => {
            let words: Vec<String> = if cases.is_empty() {
                items.iter().map(|item| (*item).to_owned()).collect()
            } else {
                items
                    .iter()
                    .filter_map(|item| {
                        let chosen = cases.get(*item).or_else(|| cases.get("other"))?;
                        Some(write(chosen, item))
                    })
                    .collect()
            };
            match words.split_last() {
                Some((last, rest)) if placeholder.style == "and" && !rest.is_empty() => {
                    format!(
                        "{}{}{last}",
                        rest.join(&catalog.list_separator),
                        catalog.list_and
                    )
                }
                _ => words.join(&catalog.list_separator),
            }
        }
        "duration" => {
            let seconds = number(value);
            let (unit, n) = if seconds >= 60.0 && seconds % 60.0 == 0.0 {
                (&catalog.minutes, seconds / 60.0)
            } else {
                (&catalog.seconds, seconds)
            };
            let params = BTreeMap::from([("n".to_owned(), show(n))]);
            unit.1.as_ref().map_or_else(
                |_| String::new(),
                |nodes| self::write(nodes, &params, catalog, ""),
            )
        }
        _ => value.unwrap_or_default().to_owned(),
    }
}

/// A parameter as JavaScript's `Number` reads it.
fn number(value: Option<&str>) -> f64 {
    match value.map(str::trim) {
        None => f64::NAN,
        Some("") => 0.0,
        Some(value) => value.parse().unwrap_or(f64::NAN),
    }
}

/// A number as JavaScript's `String` writes it, for the values a duration takes.
fn show(n: f64) -> String {
    if n.is_nan() {
        "NaN".to_owned()
    } else if n.is_infinite() {
        if n > 0.0 { "Infinity" } else { "-Infinity" }.to_owned()
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_parses() {
        for language in Language::ALL {
            assert_eq!(malformed(language), vec![], "{}", language.tag());
        }
    }

    #[test]
    fn every_language_has_the_same_keys() {
        let en: Vec<_> = keys(Language::En).collect();
        assert_ne!(en, Vec::<&str>::new());
        for language in Language::ALL {
            assert_eq!(keys(language).collect::<Vec<_>>(), en, "{}", language.tag());
        }
    }

    #[test]
    fn renders_the_examples() {
        let examples: Value =
            serde_json::from_str(include_str!("../messages/examples.json")).expect("examples");
        let examples = examples.as_array().expect("an array");
        assert_ne!(examples.len(), 0);
        for example in examples {
            let mut message = Message::new(example["key"].as_str().expect("a key"));
            for (name, value) in example["params"].as_object().expect("params") {
                message = message.with(name, value.as_str().expect("a string"));
            }
            for language in Language::ALL {
                assert_eq!(
                    render(&message, language),
                    example[language.tag()].as_str().expect("a sentence"),
                    "{} {:?}",
                    language.tag(),
                    message
                );
            }
        }
    }

    #[test]
    fn an_unknown_key_renders_as_itself() {
        assert_eq!(
            render(&Message::new("gap.nope"), Language::ZhTw),
            "gap.nope"
        );
    }

    #[test]
    fn parses_plural_hash_only_in_plural_cases() {
        let nodes = parse("#${n, plural, one {# a} other {# b}}").expect("parses");
        assert!(matches!(&nodes[0], Node::Text(text) if text == "#"));
        assert!(parse("${n, select, a {x}").is_err());
        assert!(parse("${n, nope}").is_err());
    }
}

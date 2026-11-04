//! Renaming old component input names in the text of a configuration file.
//!
//! The edit works on lines and changes only the key. Its result is parsed and compared with the
//! parsed original, so a text edit that changes anything else is dropped.

use serde_json::Value;

use crate::config::RENAMED_INPUTS;

const SECTIONS: [&str; 2] = ["inputs", "secretFiles"];

/// One old input name that a configuration file uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputRename {
    /// `inputs` or `secretFiles`.
    pub section: &'static str,
    /// The component ID.
    pub component: &'static str,
    /// The name in the file.
    pub old: &'static str,
    /// The name the catalog uses.
    pub new: &'static str,
}

impl std::fmt::Display for InputRename {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: input {} is now {}",
            self.component, self.old, self.new
        )
    }
}

/// The old input names of a file and the file text with the new ones.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Rewrite {
    /// The new file text; `None` when no rename could be placed.
    pub text: Option<String>,
    /// The renames made in `text`.
    pub done: Vec<InputRename>,
    /// The renames that could not be placed safely. Reading the file still uses the new names.
    pub left: Vec<InputRename>,
}

/// Finds the old input names under `inputs` and `secretFiles`, and renames each key in the text
/// where that is safe.
///
/// A key is left alone when the new name is already in the same mapping or
/// the key is not written as a plain `old: value` entry of a block or one-line flow mapping.
#[must_use]
pub fn rename_input_keys(yaml: &str) -> Rewrite {
    let Ok(value) = serde_saphyr::from_str::<Value>(yaml) else {
        return Rewrite::default();
    };
    let found = &value;
    let wanted: Vec<InputRename> = SECTIONS
        .iter()
        .flat_map(|section| {
            RENAMED_INPUTS
                .iter()
                .filter(move |(component, old, _)| {
                    found
                        .get(section)
                        .and_then(|s| s.get(component))
                        .is_some_and(|m| m.get(old).is_some())
                })
                .map(|(component, old, new)| InputRename {
                    section,
                    component,
                    old,
                    new,
                })
        })
        .collect();
    if wanted.is_empty() {
        return Rewrite::default();
    }
    let mut lines: Vec<String> = yaml.split_inclusive('\n').map(str::to_owned).collect();
    let (done, mut left): (Vec<_>, Vec<_>) = wanted.into_iter().partition(|r| edit(&mut lines, r));
    if done.is_empty() {
        return Rewrite {
            text: None,
            done,
            left,
        };
    }
    let text = lines.concat();
    let mut expected = value;
    for r in &done {
        if let Some(map) = expected
            .get_mut(r.section)
            .and_then(|s| s.get_mut(r.component))
            .and_then(Value::as_object_mut)
            && let Some(v) = map.remove(r.old)
        {
            map.insert(r.new.to_owned(), v);
        }
    }
    if serde_saphyr::from_str::<Value>(&text).is_ok_and(|v| v == expected) {
        Rewrite {
            text: Some(text),
            done,
            left,
        }
    } else {
        left.extend(done);
        Rewrite {
            text: None,
            done: Vec::new(),
            left,
        }
    }
}

/// A line `key: rest` with its indent. `key_at` is the byte range of the key without quotes.
struct Entry<'a> {
    indent: usize,
    key: &'a str,
    key_at: (usize, usize),
    rest: &'a str,
}

fn at(lines: &[String], i: usize) -> &str {
    lines.get(i).map_or("", String::as_str)
}

fn meaningful(line: &str) -> bool {
    let t = line.trim_start();
    !t.is_empty() && !t.starts_with('#')
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn entry(line: &str) -> Option<Entry<'_>> {
    let indent = indent_of(line);
    let body = line.get(indent..)?;
    let (key_at, after) = match body.chars().next()? {
        q @ ('"' | '\'') => {
            let close = body.get(1..)?.find(q)? + 1;
            ((indent + 1, indent + close), body.get(close + 1..)?)
        }
        '#' | '{' | '[' | '-' | '?' | '&' | '*' | '!' | '|' | '>' | '\t' | '%' | '@' | '`' => {
            return None;
        }
        _ => {
            let colon = plain_colon(body)?;
            ((indent, indent + colon), body.get(colon..)?)
        }
    };
    let after = after.trim_start_matches(' ').strip_prefix(':')?;
    if !(after.is_empty() || after.starts_with([' ', '\n', '\r'])) {
        return None;
    }
    Some(Entry {
        indent,
        key: line.get(key_at.0..key_at.1)?,
        key_at,
        rest: after.trim(),
    })
}

/// The offset of the `:` that ends a plain key.
fn plain_colon(text: &str) -> Option<usize> {
    text.char_indices()
        .find(|&(i, c)| {
            c == ':'
                && text
                    .as_bytes()
                    .get(i + 1)
                    .is_none_or(|b| b" \r\n".contains(b))
        })
        .map(|(i, _)| i)
}

fn edit(lines: &mut [String], r: &InputRename) -> bool {
    let Some(start) = lines
        .iter()
        .position(|l| entry(l).is_some_and(|e| e.indent == 0 && e.key == r.section))
    else {
        return false;
    };
    if entry(at(lines, start)).is_none_or(|e| !is_empty_rest(e.rest)) {
        return false;
    }
    let end = (start + 1..lines.len())
        .find(|&i| meaningful(at(lines, i)) && indent_of(at(lines, i)) == 0)
        .unwrap_or(lines.len());
    let Some(child) = (start + 1..end)
        .find(|&i| meaningful(at(lines, i)))
        .map(|i| indent_of(at(lines, i)))
    else {
        return false;
    };
    let Some(comp) = unique(start + 1..end, lines, |e| {
        e.indent == child && e.key == r.component
    }) else {
        return false;
    };
    let Some(e) = entry(at(lines, comp)) else {
        return false;
    };
    if is_empty_rest(e.rest) {
        edit_block(lines, comp, end, child, r)
    } else if e.rest.starts_with('{') {
        lines.get_mut(comp).is_some_and(|l| edit_flow(l, r))
    } else {
        false
    }
}

fn is_empty_rest(rest: &str) -> bool {
    rest.is_empty() || rest.starts_with('#')
}

/// The only line in `range` that `matches`, if there is exactly one.
fn unique(
    range: impl Iterator<Item = usize>,
    lines: &[String],
    matches: impl Fn(&Entry<'_>) -> bool,
) -> Option<usize> {
    let mut found = None;
    for i in range {
        if entry(at(lines, i)).is_some_and(|e| matches(&e)) {
            if found.is_some() {
                return None;
            }
            found = Some(i);
        }
    }
    found
}

fn edit_block(
    lines: &mut [String],
    comp: usize,
    section_end: usize,
    parent: usize,
    r: &InputRename,
) -> bool {
    let end = (comp + 1..section_end)
        .find(|&i| meaningful(at(lines, i)) && indent_of(at(lines, i)) <= parent)
        .unwrap_or(section_end);
    let Some(inner) = (comp + 1..end)
        .find(|&i| meaningful(at(lines, i)))
        .map(|i| indent_of(at(lines, i)))
    else {
        return false;
    };
    let mut old_line = None;
    for i in
        (comp + 1..end).filter(|&i| meaningful(at(lines, i)) && indent_of(at(lines, i)) == inner)
    {
        let Some(e) = entry(at(lines, i)) else {
            return false;
        };
        if e.key == r.new {
            return false;
        }
        if e.key == r.old {
            if old_line.is_some() {
                return false;
            }
            old_line = Some((i, e.key_at));
        }
    }
    let Some((i, (from, to))) = old_line else {
        return false;
    };
    lines
        .get_mut(i)
        .is_some_and(|l| replace(l, from, to, r.new))
}

fn replace(line: &mut String, from: usize, to: usize, with: &str) -> bool {
    if line.get(from..to).is_none() {
        return false;
    }
    line.replace_range(from..to, with);
    true
}

/// Renames the key in `{a: 1, old: 2}` written on one line.
fn edit_flow(line: &mut String, r: &InputRename) -> bool {
    let Some(open) = line.find('{') else {
        return false;
    };
    let mut depth = 0_usize;
    let mut quote = None;
    let mut closed = false;
    // The entries at depth one, as byte ranges of `line`.
    let mut entries = Vec::new();
    let mut from = open + 1;
    let mut prev = '{';
    for (i, c) in line.char_indices().skip_while(|(i, _)| *i <= open) {
        if let Some(q) = quote {
            if c == q {
                quote = None;
                prev = c;
            }
            continue;
        }
        match c {
            '"' | '\'' if matches!(prev, '{' | ',' | ':' | '[' | ' ') => quote = Some(c),
            '#' if prev == ' ' => return false,
            '{' | '[' => depth += 1,
            ']' | '}' if depth > 0 => depth -= 1,
            '}' => {
                entries.push((from, i));
                closed = true;
                break;
            }
            ',' if depth == 0 => {
                entries.push((from, i));
                from = i + 1;
            }
            _ => {}
        }
        prev = c;
    }
    if !closed || quote.is_some() {
        return false;
    }
    let mut target = None;
    for (a, b) in entries {
        let Some(part) = line.get(a..b) else {
            return false;
        };
        let lead = part.len() - part.trim_start().len();
        let text = part.trim();
        if text.is_empty() {
            continue;
        }
        let key_at = if let Some(q @ ('"' | '\'')) = text.chars().next() {
            let Some(len) = text.get(1..).and_then(|t| t.find(q)) else {
                return false;
            };
            (a + lead + 1, a + lead + 1 + len)
        } else {
            let end = plain_colon(text).unwrap_or(text.len());
            let len = text.get(..end).map_or(0, |k| k.trim_end().len());
            (a + lead, a + lead + len)
        };
        let Some(key) = line.get(key_at.0..key_at.1) else {
            return false;
        };
        if key == r.new {
            return false;
        }
        if key == r.old {
            if target.is_some() {
                return false;
            }
            target = Some(key_at);
        }
    }
    let Some((from, to)) = target else {
        return false;
    };
    replace(line, from, to, r.new)
}

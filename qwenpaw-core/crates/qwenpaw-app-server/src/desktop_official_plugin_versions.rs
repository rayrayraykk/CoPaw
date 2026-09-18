//! Preserve decimal ordering beyond u64 while retaining the PEP 440 parser.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::str::FromStr;

use pep440_rs::Version;

enum Part<'a> {
    Text(&'a str),
    Number(&'a str),
}

pub(super) fn trim(value: &str) -> &str {
    // Python str.strip/re \s also include these four information separators.
    value.trim_matches(|ch: char| ch.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&ch))
}

fn number(value: &str) -> Part<'_> {
    let value = value.trim_start_matches('0');
    Part::Number(if value.is_empty() { "0" } else { value })
}

fn parts(value: &str) -> Vec<Part<'_>> {
    let value = trim(value);
    let (public, local) = value
        .split_once('+')
        .map_or((value, None), |(public, local)| (public, Some(local)));
    let mut parts = Vec::new();
    let mut start = 0;
    let mut numeric = false;
    for (index, ch) in public.char_indices() {
        if index > start && numeric != ch.is_ascii_digit() {
            let piece = &public[start..index];
            parts.push(if numeric {
                number(piece)
            } else {
                Part::Text(piece)
            });
            start = index;
        }
        numeric = ch.is_ascii_digit();
    }
    let piece = &public[start..];
    parts.push(if numeric {
        number(piece)
    } else {
        Part::Text(piece)
    });
    if let Some(local) = local {
        parts.push(Part::Text("+"));
        for piece in local.split_inclusive(['.', '_', '-']) {
            let end = piece.len() - usize::from(piece.ends_with(['.', '_', '-']));
            let segment = &piece[..end];
            parts.push(
                if !segment.is_empty() && segment.bytes().all(|byte| byte.is_ascii_digit()) {
                    number(segment)
                } else {
                    Part::Text(segment)
                },
            );
            parts.push(Part::Text(&piece[end..]));
        }
    }
    parts
}

pub(super) fn compare(left: &str, right: &str) -> Option<Ordering> {
    let left = parts(left);
    let right = parts(right);
    // A single strictly increasing mapping is shared by both versions. Zero must
    // stay zero: missing epoch/pre/post/dev numbers and release padding use it.
    let mut ranks = BTreeMap::from([((1, "0"), 0)]);
    for part in left.iter().chain(&right) {
        if let Part::Number(value) = part {
            ranks.entry((value.len(), *value)).or_insert(0);
        }
    }
    for (rank, value) in ranks.values_mut().enumerate() {
        *value = rank;
    }
    let encode = |parts: &[Part<'_>]| {
        let mut encoded = String::new();
        for part in parts {
            match part {
                Part::Text(value) => encoded.push_str(value),
                Part::Number(value) => encoded.push_str(&ranks[&(value.len(), *value)].to_string()),
            }
        }
        Version::from_str(&encoded).ok()
    };
    Some(encode(&left)?.cmp(&encode(&right)?))
}

pub(super) fn valid(value: &str) -> bool {
    compare(value, value).is_some()
}

pub(super) fn decimal(value: &str) -> bool {
    let value = trim(value);
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

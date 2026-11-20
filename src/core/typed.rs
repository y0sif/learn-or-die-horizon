//! Typed answers (Java `TypedAnswer`, the path for packs without readings, which is German's):
//! how close a typed answer is to the question's text.
//!
//! Both sides are made plain first: composed, lower case, letters and digits kept, whitespace and
//! hyphens one space, other punctuation dropped. Equal plain texts are EXACT, and so is a typed
//! text equal to the target with its apostrophes read as spaces. Spaces count. Otherwise both are
//! folded (`ß` as `ss`, `œ` as `oe`, `æ` as `ae`, `ø` as `o`, then accents dropped) and compared
//! word by word: the word counts must match (articles are required), and a word that differs may
//! be off by 0 edits at 1 to 3 letters, 1 at 4 to 7 and 2 from 8, unless the typed word is itself
//! a word of the pack. Any such difference is CLOSE, which counts as right.
//!
//! Unicode: there is no normaliser here, so composing and folding cover the letters of Latin-1
//! Supplement and Latin Extended-A that have a canonical decomposition (the German pack uses
//! `ä ö ü Ä Ö ß`). Letters outside those blocks are kept as typed; marks in the combining
//! diacritical blocks are dropped.
use std::collections::HashSet;

/// How a typed answer compares with the question's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Exact,
    /// Right, with a spelling slip: an accent, `ss` for `ß`, or a small typo.
    Close,
    Wrong,
}

impl Verdict {
    /// EXACT and CLOSE both count as right.
    pub fn is_right(self) -> bool {
        self != Verdict::Wrong
    }
}

/// Every word of the pack's texts, folded: a typo that is itself one of them is wrong.
#[derive(Clone, Debug, Default)]
pub(crate) struct PackWords {
    latin: HashSet<String>,
}

impl PackWords {
    pub(crate) fn new<'a>(texts: impl IntoIterator<Item = &'a str>) -> PackWords {
        let mut latin = HashSet::new();
        for text in texts {
            for source in [text.to_string(), apostrophes_as_spaces(text)] {
                latin.extend(fold(&plain(&source)).split(' ').map(str::to_string));
            }
        }
        PackWords { latin }
    }
}

/// `Character.isWhitespace`: no non-breaking spaces, but the C0 separators.
pub(crate) fn java_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t'..='\r'
            | '\u{1C}'..='\u{1F}'
            | ' '
            | '\u{1680}'
            | '\u{2000}'..='\u{2006}'
            | '\u{2008}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{205F}'
            | '\u{3000}'
    )
}

/// `String.strip()`.
pub(crate) fn java_strip(text: &str) -> &str {
    text.trim_matches(java_whitespace)
}

fn apostrophes_as_spaces(text: &str) -> String {
    text.replace(['\'', '\u{2019}'], " ")
}

/// Judges `typed` against the question's `text`.
pub(crate) fn judge(typed: &str, text: &str, words: &PackWords) -> Verdict {
    let got = plain(typed);
    if got.is_empty() {
        return Verdict::Wrong;
    }
    let want = plain(text);
    let spaced = plain(&apostrophes_as_spaces(text));
    if got == want || got == spaced {
        return Verdict::Exact;
    }
    let got = fold(&got);
    let typed: Vec<&str> = got.split(' ').collect();
    let mut target = fold(&want);
    if target.split(' ').count() != typed.len() {
        target = fold(&spaced);
        if target.split(' ').count() != typed.len() {
            return Verdict::Wrong;
        }
    }
    for (typed, wanted) in typed.iter().zip(target.split(' ')) {
        if *typed == wanted {
            continue;
        }
        let allowed = match utf16(wanted).len() {
            0..=3 => 0,
            4..=7 => 1,
            _ => 2,
        };
        if distance(typed, wanted) > allowed || words.latin.contains(*typed) {
            return Verdict::Wrong;
        }
    }
    Verdict::Close
}

/// Composed, lower case, letters and digits only, words split by single spaces.
pub(crate) fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in compose(text).to_lowercase().chars() {
        if is_letter_or_digit(c) || c == '\u{30FC}' {
            out.push(c);
        } else if java_whitespace(c) || matches!(c, '-' | '\u{2010}' | '\u{2013}' | '\u{2014}') {
            out.push(' ');
        }
    }
    out.split(' ')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A plain text with `ß œ æ ø` spelled out and accents dropped.
pub(crate) fn fold(plain: &str) -> String {
    let mut out = String::with_capacity(plain.len());
    for c in plain.chars() {
        match c {
            'ß' => out.push_str("ss"),
            'œ' => out.push_str("oe"),
            'æ' => out.push_str("ae"),
            'ø' => out.push('o'),
            c if is_mark(c) => {}
            c => out.push(base_of(c).unwrap_or(c)),
        }
    }
    out
}

/// `Character.isLetterOrDigit` near enough: no combining marks, and no superscript digits or
/// vulgar fractions of Latin-1.
fn is_letter_or_digit(c: char) -> bool {
    (c.is_alphabetic() || c.is_numeric())
        && !is_mark(c)
        && !matches!(c, '\u{B2}' | '\u{B3}' | '\u{B9}' | '\u{BC}'..='\u{BE}')
}

/// The combining diacritical blocks.
fn is_mark(c: char) -> bool {
    matches!(
        c,
        '\u{300}'..='\u{36F}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{FE20}'..='\u{FE2F}'
    )
}

/// Each combining mark with the letters it composes with and what they compose to, in step.
const COMPOSED: &[(char, &str, &str)] = &[
    ('\u{300}', "AEIOUaeiou", "ÀÈÌÒÙàèìòù"),
    (
        '\u{301}',
        "AEIOUYaeiouyCcLlNnRrSsZz",
        "ÁÉÍÓÚÝáéíóúýĆćĹĺŃńŔŕŚśŹź",
    ),
    (
        '\u{302}',
        "AEIOUaeiouCcGgHhJjSsWwYy",
        "ÂÊÎÔÛâêîôûĈĉĜĝĤĥĴĵŜŝŴŵŶŷ",
    ),
    ('\u{303}', "ANOanoIiUu", "ÃÑÕãñõĨĩŨũ"),
    ('\u{304}', "AaEeIiOoUu", "ĀāĒēĪīŌōŪū"),
    ('\u{306}', "AaEeGgIiOoUu", "ĂăĔĕĞğĬĭŎŏŬŭ"),
    ('\u{307}', "CcEeGgIZz", "ĊċĖėĠġİŻż"),
    ('\u{308}', "AEIOUaeiouyY", "ÄËÏÖÜäëïöüÿŸ"),
    ('\u{30A}', "AaUu", "ÅåŮů"),
    ('\u{30B}', "OoUu", "ŐőŰű"),
    ('\u{30C}', "CcDdEeLlNnRrSsTtZz", "ČčĎďĚěĽľŇňŘřŠšŤťŽž"),
    ('\u{327}', "CcGgKkLlNnRrSsTt", "ÇçĢģĶķĻļŅņŖŗŞşŢţ"),
    ('\u{328}', "AaEeIiUu", "ĄąĘęĮįŲų"),
];

/// The letter `base` with the mark on it, when one letter writes both.
fn composite(base: char, mark: char) -> Option<char> {
    let (_, bases, composites) = COMPOSED.iter().find(|(m, _, _)| *m == mark)?;
    let index = bases.chars().position(|b| b == base)?;
    composites.chars().nth(index)
}

/// The letter under a composed letter's accent.
fn base_of(letter: char) -> Option<char> {
    COMPOSED.iter().find_map(|(_, bases, composites)| {
        let index = composites.chars().position(|c| c == letter)?;
        bases.chars().nth(index)
    })
}

/// NFC for the letters in [`COMPOSED`]: a letter followed by a mark becomes one letter.
fn compose(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if is_mark(c)
            && let Some(last) = out.pop()
        {
            match composite(last, c) {
                Some(joined) => out.push(joined),
                None => {
                    out.push(last);
                    out.push(c);
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// A Java string's UTF-16 units: lengths and distances count them.
fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// Levenshtein distance over UTF-16 units.
fn distance(a: &str, b: &str) -> usize {
    let (a, b) = (utf16(a), utf16(b));
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let same = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (cur[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + same);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

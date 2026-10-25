//! Turning source HTML into readable plain text, and splitting text into terms.

use std::{collections::HashSet, sync::LazyLock};

use regex::Regex;

static BLOCK_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<\s*(/?)\s*(p|div|br|li|ul|ol|h[1-6]|tr|table|section|article|header|footer|blockquote|pre)\b[^>]*>").unwrap()
});
static ANY_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<[^>]*>").unwrap());
static DROP_BLOCKS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<(script|style)\b.*?</(script|style)\s*>").unwrap());
static ENTITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"&(#[0-9]{1,7}|#[xX][0-9a-fA-F]{1,6}|[a-zA-Z]{2,8});").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t\u{a0}]+").unwrap());
static BLANK_LINES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").unwrap());

/// Plain text from an HTML fragment: paragraphs and list items become lines.
pub fn html_to_text(html: &str) -> String {
    let html = DROP_BLOCKS.replace_all(html, "");
    let html = BLOCK_TAG.replace_all(&html, |c: &regex::Captures| {
        let tag = c[2].to_ascii_lowercase();
        match (tag.as_str(), &c[1]) {
            ("li", "") => "\n• ",
            ("li", _) => "",
            ("br", _) => "\n",
            _ => "\n\n",
        }
    });
    let text = ANY_TAG.replace_all(&html, "");
    tidy(&decode_entities(&text))
}

/// Collapses runs of spaces and blank lines, trims each line.
pub fn tidy(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    let lines: Vec<String> = text
        .lines()
        .map(|l| SPACES.replace_all(l, " ").trim().to_string())
        .collect();
    let joined = lines.join("\n");
    BLANK_LINES.replace_all(&joined, "\n\n").trim().to_string()
}

pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    ENTITY
        .replace_all(s, |c: &regex::Captures| {
            let e = &c[1];
            let ch = if let Some(hex) = e.strip_prefix("#x").or_else(|| e.strip_prefix("#X")) {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            } else if let Some(dec) = e.strip_prefix('#') {
                dec.parse().ok().and_then(char::from_u32)
            } else {
                named_entity(e)
            };
            ch.map_or_else(|| c[0].to_string(), |ch| ch.to_string())
        })
        .into_owned()
}

fn named_entity(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "ndash" => '–',
        "mdash" => '—',
        "hellip" => '…',
        "rsquo" => '’',
        "lsquo" => '‘',
        "rdquo" => '”',
        "ldquo" => '“',
        "bull" => '•',
        "middot" => '·',
        "euro" => '€',
        "pound" => '£',
        "yen" => '¥',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "times" => '×',
        _ => return None,
    })
}

/// Repairs UTF-8 text that was decoded as Latin-1 somewhere upstream ("Ã©" → "é").
pub fn fix_mojibake(s: &str) -> String {
    if !s.contains(['Ã', 'â', 'Â', 'ð']) {
        return s.to_string();
    }
    let bytes: Option<Vec<u8>> = s
        .chars()
        .map(|c| match c as u32 {
            n @ 0..=0xFF => Some(n as u8),
            // Windows-1252 punctuation that shows up in double-encoded text.
            _ => cp1252_byte(c),
        })
        .collect();
    bytes
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_else(|| s.to_string())
}

fn cp1252_byte(c: char) -> Option<u8> {
    Some(match c {
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8A,
        '‹' => 0x8B,
        'Œ' => 0x8C,
        'Ž' => 0x8E,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '˜' => 0x98,
        '™' => 0x99,
        'š' => 0x9A,
        '›' => 0x9B,
        'œ' => 0x9C,
        'ž' => 0x9E,
        'Ÿ' => 0x9F,
        _ => return None,
    })
}

pub fn word_count(text: &str) -> usize {
    text.split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count()
}

/// Lowercase words of `text`, keeping the characters tech terms use (c++, c#, node.js).
pub fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || matches!(c, '+' | '#' | '.' | '-' | '/')))
        .flat_map(|w| w.split('/'))
        .map(|w| w.trim_matches(|c: char| matches!(c, '.' | '-')))
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Every 1–3 word phrase in a text, for fast term lookups.
pub struct Phrases {
    lower: HashSet<String>,
    /// Single words as written, for short terms like "Go" that are also English words.
    exact: HashSet<String>,
}

pub const MAX_PHRASE_WORDS: usize = 3;

impl Phrases {
    pub fn new(text: &str) -> Self {
        let w = words(text);
        let mut lower = HashSet::with_capacity(w.len() * MAX_PHRASE_WORDS);
        for n in 1..=MAX_PHRASE_WORDS {
            for win in w.windows(n) {
                lower.insert(win.join(" "));
            }
        }
        let exact = text
            .split(|c: char| !(c.is_alphanumeric() || matches!(c, '+' | '#')))
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        Self { lower, exact }
    }

    /// `phrase` must already be normalized with [`normalize_phrase`].
    pub fn contains(&self, phrase: &str) -> bool {
        self.lower.contains(phrase)
    }

    pub fn contains_exact(&self, word: &str) -> bool {
        self.exact.contains(word)
    }
}

/// Normalizes a keyword the same way [`Phrases`] splits text.
pub fn normalize_phrase(s: &str) -> String {
    words(s).join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_becomes_lines() {
        let t = html_to_text(
            "<p>Hello&nbsp;<b>world</b></p><ul><li>React &amp; TS</li><li>Go</li></ul><script>x()</script>",
        );
        assert_eq!(t, "Hello world\n\n• React & TS\n• Go");
    }

    #[test]
    fn decodes_double_escaped_html() {
        // Greenhouse sends escaped HTML inside JSON.
        let t = html_to_text(&decode_entities(
            "&lt;p&gt;Caf&#233; &amp;amp; co&lt;/p&gt;",
        ));
        assert_eq!(t, "Café & co");
    }

    #[test]
    fn repairs_mojibake() {
        assert_eq!(fix_mojibake("Ciudad de MÃ©xico"), "Ciudad de México");
        assert_eq!(fix_mojibake("teamâ€™s hours"), "team’s hours");
        assert_eq!(fix_mojibake("plain text"), "plain text");
        // Real Latin-1 text that isn't double-encoded stays.
        assert_eq!(fix_mojibake("Â"), "Â");
    }

    #[test]
    fn phrases_find_tech_terms() {
        let p =
            Phrases::new("We use Node.js, C++ and React/TypeScript. Machine learning a plus. Go!");
        for t in ["node.js", "c++", "react", "typescript", "machine learning"] {
            assert!(p.contains(&normalize_phrase(t)), "{t}");
        }
        assert!(p.contains_exact("Go"));
        assert!(!p.contains("java"));
    }
}

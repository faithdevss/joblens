//! "Can I apply?" from a posting's location field and description.
//!
//! Only explicit statements count: a location field, or a sentence like "must be based
//! in the US" or "UTC−3 to UTC+3". Anything else is "not stated". We never infer
//! eligibility from a company's HQ, a city, or where its customers are.

use std::{collections::BTreeSet, sync::LazyLock};

use regex::{Captures, Regex};

// Word boundaries are ASCII-only (`(?-u:\b)`): a Unicode `\b` stops the regex engine's
// fast path on any non-ASCII text (curly quotes, emoji), which made the feed ~50× slower.

use super::{
    Eligibility, EligibilityStatus,
    geo::{self, country_name},
};

/// Where the user is, from search settings.
#[derive(Debug, Clone, Default)]
pub struct UserPlace {
    /// ISO code, e.g. "PT".
    pub country: Option<String>,
    /// Minutes east of UTC.
    pub tz_offset: Option<i32>,
}

impl UserPlace {
    pub fn new(country: &str, timezone: &str) -> Self {
        Self {
            country: Some(country.trim().to_uppercase()).filter(|c| !c.is_empty()),
            tz_offset: parse_utc_offset(timezone),
        }
    }
}

/// "UTC+05:30" → 330.
pub fn parse_utc_offset(tz: &str) -> Option<i32> {
    let rest = tz.trim().strip_prefix("UTC")?;
    if rest.is_empty() {
        return Some(0);
    }
    parse_offset(rest)
}

/// "+5", "-03:30", "−3", "+0530" → minutes.
fn parse_offset(s: &str) -> Option<i32> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let (sign, digits) = match s.chars().next()? {
        '+' => (1, &s[1..]),
        '-' => (-1, &s[1..]),
        '−' | '–' => (-1, &s['−'.len_utf8()..]),
        _ => (1, s.as_str()),
    };
    let (h, m) = match digits.split_once(':') {
        Some((h, m)) => (h.parse::<i32>().ok()?, m.parse::<i32>().ok()?),
        None if digits.len() > 2 => (
            digits[..digits.len() - 2].parse().ok()?,
            digits[digits.len() - 2..].parse().ok()?,
        ),
        None => (digits.parse().ok()?, 0),
    };
    (h <= 14 && m < 60).then_some(sign * (h * 60 + m))
}

fn format_offset(min: i32) -> String {
    let sign = if min < 0 { "−" } else { "+" };
    let abs = min.abs();
    if abs % 60 == 0 {
        format!("UTC{sign}{}", abs / 60)
    } else {
        format!("UTC{sign}{}:{:02}", abs / 60, abs % 60)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Claim {
    Worldwide {
        quote: String,
    },
    Countries {
        codes: BTreeSet<&'static str>,
        quote: String,
        /// "US, UK + 35 other countries": the list isn't complete.
        partial: bool,
    },
    Timezone {
        min: i32,
        max: i32,
    },
}

enum Verdict {
    Pass,
    Fail,
    /// We can't tell: the user hasn't set a country/timezone, or the list is partial.
    Unknown,
}

const FRAGMENT: &str = r"(?P<w>[^.;:!?\n()]{2,120})";

static RESTRICTIONS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        // "open to candidates based in the United States"
        r"(?i)(?-u:\b)(?:open to|hiring|hire|accepting|considering)\s+(?:applicants|candidates|people|talent|team members)?\s*(?:who are\s+)?(?:based|located|living|residing|resident)\s+(?:in|within)\s+",
        // "must be located in", "must reside in"
        r"(?i)(?-u:\b)must\s+(?:be\s+)?(?:currently\s+)?(?:based|located|living|residing|reside|live)\s+(?:in|within)\s+",
        // "must be eligible to live and work in the US"
        r"(?i)(?-u:\b)(?:must|need to|required to)\s+(?:be\s+|have\s+)?(?:legally\s+)?(?:eligible|authori[sz]ed|permitted|the right)\s+to\s+(?:live\s+and\s+)?work\s+(?:in|within|from)\s+",
        // "only able to hire candidates who are legally eligible to work in Canada"
        r"(?i)(?-u:\b)(?:only|exclusively)\s+(?:able\s+to\s+)?(?:hire|hiring|consider|considering|accept|accepting|open to)\s+(?:candidates\s+|applicants\s+)?(?:who\s+are\s+)?(?:legally\s+eligible\s+to\s+work\s+in\s+|based\s+in\s+|located\s+in\s+|from\s+|in\s+)?",
        // "can be located anywhere in the US or Canada", "work from anywhere in the Philippines"
        r"(?i)(?-u:\b)(?:remote|anywhere)\s+(?:position\s+|role\s+)?(?:in|within|across)\s+",
        // "This role is based in New Zealand"
        r"(?i)(?-u:\b)(?:this|the)\s+(?:role|position|job|opportunity)\s+is\s+(?:fully\s+remote\s+and\s+|remote\s+and\s+)?(?:based|located|open)\s+(?:in|to candidates in|to)\s+",
        // "a nearshore delivery role, based in Mexico or Brazil"
        r"(?i)(?-u:\b)(?:role|position|job)\s*,?\s+(?:which\s+is\s+|is\s+)?(?:based|located)\s+(?:in|within)\s+",
        // "Location: Philippines (Remote)", "Location: Remote, US"
        r"(?i)(?-u:\b)location\s*:\s*(?:remote\s*[,\-–—]?\s*)?",
    ]
    .iter()
    .map(|p| Regex::new(&format!("{p}{FRAGMENT}")).unwrap())
    .collect()
});

/// "US only", "EU residents only", "Remote — US and Canada only".
static ONLY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?P<w>[^.;:!?\n()"“”]{1,80}?)(?-u:\b)only(?-u:\b)"#).unwrap()
});
static ONLY_TAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^[\s,\-–—/]*(?:(?:based|residents?|candidates?|applicants?|citizens?|time\s*zones?|hires?|remote)[\s,\-–—/]*)*$").unwrap()
});
static WORLDWIDE_FIELD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?-u:\b)(?:worldwide|anywhere|global(?:ly)?|international|all countries|world\s*wide)(?-u:\b)",
    )
    .unwrap()
});
static WORLDWIDE_TEXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?-u:\b)(?:anywhere in the world|hire (?:globally|worldwide|from anywhere)|hiring (?:globally|worldwide|from anywhere)|open to (?:candidates|applicants|people) (?:from )?(?:anywhere|worldwide|all over the world)|fully remote,? worldwide|(?:be based|live|located|work from|work remotely from) anywhere)(?-u:\b)").unwrap()
});
static AFTER_ANYWHERE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\s+(?:in|within|across)\s+(?:the\s+)?(?P<w>\w+)").unwrap());
static PARTIAL_LIST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:\+\s*\d+\s*(?:other|more)|and\s+\d+\s+(?:other|more)|(?-u:\b)more(?-u:\b)|(?-u:\b)etc(?-u:\b))")
        .unwrap()
});

const OFFSET: &str = r"[+\-−–]\s*\d{1,2}(?::?\d{2})?";
const ZONES: &str = r"UTC|GMT|CEST|CET|EET|WET|BST|EST|EDT|ET|CST|CDT|CT|MST|MDT|MT|PST|PDT|PT|IST|AEST|JST|SGT|(?:US\s+)?(?:Eastern|Central|Mountain|Pacific)(?:\s+time)?";

static TZ_RANGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)(?-u:\b)(?:UTC|GMT)\s*(?P<a>{OFFSET})?\s*(?:to|-|–|—|and|through)\s*(?:(?:UTC|GMT)\s*(?P<b>{OFFSET})?|(?P<c>{OFFSET}))"
    ))
    .unwrap()
});
static TZ_PLUS_MINUS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)(?-u:\b)(?P<z>{ZONES})\s*(?P<o>{OFFSET})?\s*(?:\+/-|\+/−|±|\+-|plus\s+or\s+minus|plus/minus)\s*(?P<h>\d{{1,2}})\s*(?:h|hours?|hrs?)(?-u:\b)"
    ))
    .unwrap()
});
static TZ_WITHIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)(?-u:\b)within\s+(?P<h>\d{{1,2}}|one|two|three|four|five|six)\s+hours?\s+(?:of|from)\s+(?:the\s+)?(?P<z>{ZONES})\s*(?P<o>{OFFSET})?"
    ))
    .unwrap()
});
static TZ_REGION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?-u:\b)(?P<r>USA?|U\.S\.|North American?|European?|EU|EMEA|LATAM|Latin American?|APAC|Americas?|Asian?|Australian?)\s+time\s*zones?(?-u:\b)").unwrap()
});

fn zone_offset(zone: &str) -> Option<i32> {
    let z = zone.to_lowercase();
    let z = z.trim_start_matches("us ").trim_end_matches(" time");
    Some(match z {
        "utc" | "gmt" | "wet" => 0,
        "bst" | "cet" => 60,
        "cest" | "eet" => 120,
        "est" | "et" | "eastern" => -300,
        "edt" | "cdt" => -300,
        "cst" | "ct" | "central" => -360,
        "mdt" => -360,
        "mst" | "mt" | "mountain" => -420,
        "pdt" => -420,
        "pst" | "pt" | "pacific" => -480,
        "ist" => 330,
        "sgt" => 480,
        "jst" => 540,
        "aest" => 600,
        _ => return None,
    })
}

fn region_tz(region: &str) -> Option<(i32, i32)> {
    let r = region.to_lowercase();
    let h = |a: f32, b: f32| Some(((a * 60.0) as i32, (b * 60.0) as i32));
    match r.trim_end_matches('n') {
        "us" | "usa" | "u.s." => h(-10.0, -4.0),
        "north america" => h(-10.0, -3.5),
        "europea" | "europe" | "eu" => h(-1.0, 3.0),
        "emea" => h(-1.0, 4.0),
        "latam" | "latin america" => h(-8.0, -3.0),
        "america" | "americas" => h(-10.0, -3.0),
        "apac" => h(5.0, 13.0),
        "asia" => h(5.0, 9.0),
        "australia" => h(8.0, 11.0),
        _ => None,
    }
}

fn word_number(s: &str) -> Option<i32> {
    s.parse().ok().or(match s.to_lowercase().as_str() {
        "one" => Some(1),
        "two" => Some(2),
        "three" => Some(3),
        "four" => Some(4),
        "five" => Some(5),
        "six" => Some(6),
        _ => None,
    })
}

fn timezone_claims(text: &str) -> Vec<Claim> {
    let mut out = Vec::new();
    for c in TZ_RANGE.captures_iter(text) {
        let a = c.name("a").map_or(Some(0), |m| parse_offset(m.as_str()));
        let b = c
            .name("b")
            .or(c.name("c"))
            .map_or(Some(0), |m| parse_offset(m.as_str()));
        if let (Some(a), Some(b)) = (a, b)
            && a != b
        {
            out.push(Claim::Timezone {
                min: a.min(b),
                max: a.max(b),
            });
        }
    }
    let around = |c: &Captures| -> Option<Claim> {
        let base = zone_offset(&c["z"])?;
        let base = base + c.name("o").map_or(Some(0), |o| parse_offset(o.as_str()))?;
        let h = word_number(&c["h"])? * 60;
        Some(Claim::Timezone {
            min: base - h,
            max: base + h,
        })
    };
    out.extend(TZ_PLUS_MINUS.captures_iter(text).filter_map(|c| around(&c)));
    out.extend(TZ_WITHIN.captures_iter(text).filter_map(|c| around(&c)));
    out.extend(TZ_REGION.captures_iter(text).filter_map(|c| {
        let (min, max) = region_tz(&c["r"])?;
        Some(Claim::Timezone { min, max })
    }));
    out
}

fn quote(s: &str) -> String {
    let s = s.trim().trim_end_matches([',', '-', '–', '—']).trim();
    if s.chars().count() > 140 {
        let cut: String = s.chars().take(137).collect();
        format!("{}…", cut.trim_end())
    } else {
        s.to_string()
    }
}

/// Claims from a structured location field ("USA, Canada", "Anywhere in the World").
fn location_claims(location: &str) -> Vec<Claim> {
    let loc = location.trim();
    if loc.is_empty() {
        return Vec::new();
    }
    let codes = geo::countries_in(loc);
    let mut out = timezone_claims(loc);
    if !codes.is_empty() {
        out.push(Claim::Countries {
            codes,
            quote: quote(loc),
            partial: PARTIAL_LIST.is_match(loc),
        });
    } else if WORLDWIDE_FIELD.is_match(loc) {
        out.push(Claim::Worldwide { quote: quote(loc) });
    }
    out
}

/// Claims from sentences in the title or description.
fn text_claims(text: &str) -> Vec<Claim> {
    let mut out = Vec::new();
    for re in RESTRICTIONS.iter() {
        for c in re.captures_iter(text) {
            let w = &c["w"];
            let codes = geo::countries_in(w);
            if !codes.is_empty() {
                out.push(Claim::Countries {
                    codes,
                    quote: quote(&c[0]),
                    partial: false,
                });
            }
        }
    }
    for c in ONLY.captures_iter(text) {
        let w = &c["w"];
        let places = geo::places(w);
        // The place names must sit right before "only", e.g. "US-based candidates only".
        let Some(first) = places.first() else {
            continue;
        };
        let last = places.last().unwrap();
        let between_ok = places.windows(2).all(|p| {
            let gap = &w[p[0].end..p[1].start];
            gap.trim().is_empty()
                || matches!(
                    gap.trim().to_lowercase().as_str(),
                    "," | "/" | "&" | "and" | "or" | "+"
                )
        });
        if between_ok && ONLY_TAIL.is_match(&w[last.end..]) {
            let codes = places
                .iter()
                .flat_map(|p| p.codes.iter().copied())
                .collect();
            out.push(Claim::Countries {
                codes,
                quote: quote(&format!("{} only", &w[first.start..last.end])),
                partial: false,
            });
        }
    }
    for m in WORLDWIDE_TEXT.find_iter(text) {
        // "anywhere in the US" is a country list (handled above), not worldwide.
        if let Some(after) = AFTER_ANYWHERE.captures(&text[m.end()..])
            && !after["w"].eq_ignore_ascii_case("world")
        {
            continue;
        }
        out.push(Claim::Worldwide {
            quote: quote(m.as_str()),
        });
    }
    out.extend(timezone_claims(text));
    out
}

fn verdict(claim: &Claim, user: &UserPlace) -> Verdict {
    match claim {
        Claim::Worldwide { .. } => Verdict::Pass,
        Claim::Countries { codes, partial, .. } => match &user.country {
            Some(c) if codes.contains(c.as_str()) => Verdict::Pass,
            Some(_) if *partial => Verdict::Unknown,
            Some(_) => Verdict::Fail,
            None => Verdict::Unknown,
        },
        Claim::Timezone { min, max } => match user.tz_offset {
            Some(o) if (*min..=*max).contains(&o) => Verdict::Pass,
            Some(_) => Verdict::Fail,
            None => Verdict::Unknown,
        },
    }
}

fn user_country(user: &UserPlace) -> String {
    user.country
        .as_deref()
        .map(|c| country_name(c).to_string())
        .unwrap_or_default()
}

fn range(min: i32, max: i32) -> String {
    format!("{} to {}", format_offset(min), format_offset(max))
}

fn failed_note(claim: &Claim, user: &UserPlace) -> String {
    match claim {
        Claim::Countries { quote, .. } => {
            format!("“{quote}” You can’t apply from {}.", user_country(user))
        }
        Claim::Timezone { min, max } => format!(
            "Requires working hours in {}. You’re in {}.",
            range(*min, *max),
            format_offset(user.tz_offset.unwrap_or(0))
        ),
        Claim::Worldwide { .. } => unreachable!("worldwide never fails"),
    }
}

fn unknown_note(claim: &Claim, user: &UserPlace) -> (EligibilityStatus, String) {
    match claim {
        Claim::Countries {
            quote,
            partial: true,
            ..
        } if user.country.is_some() => (
            EligibilityStatus::NotStated,
            format!(
                "Lists “{quote}”. Check the posting to see if {} is one of them.",
                user_country(user)
            ),
        ),
        Claim::Countries { quote, .. } => (
            EligibilityStatus::Restricted,
            format!("“{quote}” Set your country in Settings to check if you can apply."),
        ),
        Claim::Timezone { min, max } => (
            EligibilityStatus::Restricted,
            format!(
                "Requires working hours in {}. Set your timezone in Settings to check.",
                range(*min, *max)
            ),
        ),
        Claim::Worldwide { .. } => unreachable!("worldwide is never unknown"),
    }
}

fn passed_note(claims: &[&Claim], user: &UserPlace) -> String {
    let countries = claims.iter().find_map(|c| match c {
        Claim::Countries { quote, .. } => Some(quote),
        _ => None,
    });
    if let Some(quote) = countries {
        return format!("Hiring in “{quote}”. {} is included.", user_country(user));
    }
    let tz = claims.iter().find_map(|c| match c {
        Claim::Timezone { min, max } => Some((*min, *max)),
        _ => None,
    });
    if let Some((min, max)) = tz {
        return format!(
            "Open to anyone working {}. You’re in {}.",
            range(min, max),
            format_offset(user.tz_offset.unwrap_or(0))
        );
    }
    match claims.first() {
        Some(Claim::Worldwide { quote }) => format!("Open worldwide (“{quote}”)."),
        _ => "Open worldwide.".into(),
    }
}

/// Classifies one posting for one user. `location` is the source's location field;
/// `text` is the title and description.
pub fn classify(location: &str, text: &str, user: &UserPlace) -> Eligibility {
    let mut claims = location_claims(location);
    for c in text_claims(text) {
        if !claims.contains(&c) {
            claims.push(c);
        }
    }

    if claims.is_empty() {
        let loc = location.trim();
        let note = if loc.is_empty() || loc.eq_ignore_ascii_case("remote") {
            "The posting doesn’t say which countries can apply. Ask before applying.".to_string()
        } else {
            format!(
                "The posting says “{}” but doesn’t say which countries can apply. Ask before applying.",
                quote(loc)
            )
        };
        return Eligibility {
            status: EligibilityStatus::NotStated,
            note,
        };
    }

    let verdicts: Vec<(Verdict, &Claim)> = claims.iter().map(|c| (verdict(c, user), c)).collect();
    if let Some((_, c)) = verdicts.iter().find(|(v, _)| matches!(v, Verdict::Fail)) {
        return Eligibility {
            status: EligibilityStatus::Restricted,
            note: failed_note(c, user),
        };
    }
    if let Some((_, c)) = verdicts.iter().find(|(v, _)| matches!(v, Verdict::Unknown)) {
        let (status, note) = unknown_note(c, user);
        return Eligibility { status, note };
    }
    let passed: Vec<&Claim> = verdicts.iter().map(|(_, c)| *c).collect();
    Eligibility {
        status: EligibilityStatus::Eligible,
        note: passed_note(&passed, user),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use EligibilityStatus::*;

    #[test]
    fn parses_offsets() {
        assert_eq!(parse_utc_offset("UTC+05:30"), Some(330));
        assert_eq!(parse_utc_offset("UTC-03:00"), Some(-180));
        assert_eq!(parse_utc_offset("UTC+00:00"), Some(0));
        assert_eq!(parse_utc_offset(""), None);
        assert_eq!(parse_offset("−3"), Some(-180));
        assert_eq!(format_offset(-180), "UTC−3");
        assert_eq!(format_offset(330), "UTC+5:30");
    }

    /// One labeled case from the fixture corpus.
    #[derive(serde::Deserialize)]
    struct Case {
        note: String,
        #[serde(default)]
        location: String,
        #[serde(default)]
        text: String,
        country: String,
        #[serde(default)]
        timezone: String,
        expect: EligibilityStatus,
        #[serde(default)]
        note_contains: Option<String>,
    }

    /// Labeled postings: location fields and sentences copied from real listings
    /// (Remotive, Remote OK, We Work Remotely, Greenhouse, Lever, Ashby).
    #[test]
    fn fixture_corpus() {
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("../../fixtures/eligibility.json")).unwrap();
        let mut failures = Vec::new();
        for case in &cases {
            let user = UserPlace::new(&case.country, &case.timezone);
            let got = classify(&case.location, &case.text, &user);
            let note_ok = case
                .note_contains
                .as_ref()
                .is_none_or(|n| got.note.contains(n.as_str()));
            if got.status != case.expect || !note_ok {
                failures.push(format!(
                    "{}: expected {:?}, got {:?} — {}",
                    case.note, case.expect, got.status, got.note
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} cases failed:\n{}",
            failures.len(),
            cases.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn notes_explain_the_badge() {
        let pt = UserPlace::new("PT", "UTC+00:00");
        let e = classify("", "Remote — US and Canada only.", &pt);
        assert_eq!(e.status, Restricted);
        assert_eq!(
            e.note,
            "“US and Canada only” You can’t apply from Portugal."
        );

        let e = classify("", "Open to anyone working UTC−3 to UTC+3.", &pt);
        assert_eq!(e.status, Eligible);
        assert_eq!(
            e.note,
            "Open to anyone working UTC−3 to UTC+3. You’re in UTC+0."
        );

        let e = classify("EU, UK", "", &pt);
        assert_eq!(e.note, "Hiring in “EU, UK”. Portugal is included.");
    }
}

//! Salary text like "$120k – $150k / year" → numbers, currency and period.
//!
//! Anything we can't read with confidence keeps only its text; it is never guessed.

use std::sync::LazyLock;

use regex::Regex;

use super::{Salary, SalaryPeriod};

static NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d[\d,.\s]*\d|\d)\s*([kKmM])?(?-u:\b)").unwrap());
static PER_HOUR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(/\s*h(ou)?r(?-u:\b)|per\s+hour|hourly|an\s+hour|/\s*h(?-u:\b))").unwrap()
});
static PER_MONTH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(/\s*mo(nth)?(?-u:\b)|per\s+month|monthly|a\s+month)").unwrap()
});
static PER_YEAR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(/\s*y(ea)?r(?-u:\b)|per\s+(year|annum)|annual(ly)?|yearly|a\s+year|(?-u:\b)p\.?a\.?(?-u:\b)|/\s*y(?-u:\b))",
    )
    .unwrap()
});
static OTHER_UNIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(per|/)\s*(task|project|word|article|day|week|hit|call|lesson|session|piece)(?-u:\b)|(?-u:\b)equity\s+only(?-u:\b)")
        .unwrap()
});

const CODES: &[&str] = &[
    "USD", "EUR", "GBP", "CAD", "AUD", "NZD", "CHF", "SEK", "NOK", "DKK", "PLN", "CZK", "INR",
    "PKR", "BRL", "MXN", "ARS", "ZAR", "SGD", "JPY", "HUF", "RON", "ILS", "AED", "PHP", "IDR",
    "COP", "CLP", "TRY", "UAH", "HKD", "BDT",
];

fn currency(text: &str) -> Option<&'static str> {
    let upper = text.to_uppercase();
    if let Some(code) = CODES.iter().find(|c| {
        upper.match_indices(*c).any(|(i, _)| {
            !upper[..i].ends_with(char::is_alphabetic)
                && !upper[i + 3..].starts_with(char::is_alphabetic)
        })
    }) {
        return Some(code);
    }
    for (prefix, code) in [
        ("CA$", "CAD"),
        ("C$", "CAD"),
        ("A$", "AUD"),
        ("AU$", "AUD"),
        ("NZ$", "NZD"),
        ("S$", "SGD"),
        ("R$", "BRL"),
        ("HK$", "HKD"),
        ("MX$", "MXN"),
    ] {
        if upper.contains(prefix) {
            return Some(code);
        }
    }
    [
        ('€', "EUR"),
        ('£', "GBP"),
        ('₹', "INR"),
        ('¥', "JPY"),
        ('$', "USD"),
        ('₱', "PHP"),
    ]
    .iter()
    .find(|(sym, _)| text.contains(*sym))
    .map(|(_, c)| *c)
}

/// Reads "87,069", "35,3", "1.5", "120 000" as a number. Groups of three after a separator are thousands.
fn read_number(raw: &str) -> Option<f64> {
    let raw: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    let parts: Vec<&str> = raw.split([',', '.']).collect();
    if parts.len() == 1 {
        return raw.parse().ok();
    }
    let (last, head) = parts.split_last()?;
    if last.len() == 3 {
        return parts.concat().parse().ok();
    }
    let whole = head.concat();
    format!("{whole}.{last}").parse().ok()
}

/// Parses free salary text. Returns `None` when there's no text at all.
pub fn parse(text: &str) -> Option<Salary> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut salary = Salary {
        text: text.to_string(),
        ..Default::default()
    };
    if OTHER_UNIT.is_match(text) {
        return Some(salary);
    }
    let mut values: Vec<(f64, Option<char>)> = NUMBER
        .captures_iter(text)
        .filter_map(|c| {
            let n = read_number(&c[1])?;
            let unit = c
                .get(2)
                .map(|m| m.as_str().to_ascii_lowercase().chars().next().unwrap());
            Some((n, unit))
        })
        .collect();
    values.truncate(2);
    if values.is_empty() {
        return Some(salary);
    }
    // "£60–75k": the suffix on the second number applies to both.
    if values.len() == 2 && values[0].1.is_none() && values[1].1.is_some() && values[0].0 < 1000.0 {
        values[0].1 = values[1].1;
    }
    let scaled: Vec<f64> = values
        .iter()
        .map(|(n, unit)| match unit {
            Some('k') => n * 1_000.0,
            Some('m') => n * 1_000_000.0,
            _ => *n,
        })
        .collect();
    let (min, max) = match scaled.as_slice() {
        [a] => (*a, *a),
        [a, b] => (a.min(*b), a.max(*b)),
        _ => unreachable!(),
    };
    salary.currency = currency(text).map(str::to_string);
    salary.period = if PER_HOUR.is_match(text) {
        Some(SalaryPeriod::Hour)
    } else if PER_MONTH.is_match(text) {
        Some(SalaryPeriod::Month)
    } else if PER_YEAR.is_match(text) || min >= 10_000.0 {
        // Five-figure amounts with no unit are annual salaries.
        Some(SalaryPeriod::Year)
    } else {
        None
    };
    salary.min = Some(min);
    salary.max = Some(max);
    Some(salary)
}

/// Structured salary from a source that gives numbers (Remote OK, Lever).
pub fn from_range(
    min: Option<f64>,
    max: Option<f64>,
    currency: Option<&str>,
    period: Option<SalaryPeriod>,
) -> Option<Salary> {
    let min = min.filter(|n| *n > 0.0);
    let max = max.filter(|n| *n > 0.0);
    if min.is_none() && max.is_none() {
        return None;
    }
    let currency = currency.map(str::to_uppercase);
    let fmt = |n: f64| {
        if n >= 1000.0 {
            format!("{}k", (n / 1000.0).round())
        } else {
            format!("{n}")
        }
    };
    let range = match (min, max) {
        (Some(a), Some(b)) if a != b => format!("{}–{}", fmt(a), fmt(b)),
        (Some(a), _) | (None, Some(a)) => fmt(a),
        (None, None) => unreachable!("checked above"),
    };
    let text = match &currency {
        Some(c) => format!("{c} {range}"),
        None => range,
    };
    Some(Salary {
        min: min.or(max),
        max: max.or(min),
        currency,
        period,
        text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(
        s: &str,
    ) -> (
        Option<f64>,
        Option<f64>,
        Option<String>,
        Option<SalaryPeriod>,
    ) {
        let s = parse(s).unwrap();
        (s.min, s.max, s.currency, s.period)
    }

    #[test]
    fn reads_common_formats() {
        use SalaryPeriod::*;
        assert_eq!(
            p("$170k - $200k"),
            (
                Some(170_000.0),
                Some(200_000.0),
                Some("USD".into()),
                Some(Year)
            )
        );
        assert_eq!(
            p("$35,3k- $52k"),
            (
                Some(35_300.0),
                Some(52_000.0),
                Some("USD".into()),
                Some(Year)
            )
        );
        assert_eq!(
            p("$50-$75 /hour"),
            (Some(50.0), Some(75.0), Some("USD".into()), Some(Hour))
        );
        assert_eq!(
            p("€150K – €250K • Offers Equity"),
            (
                Some(150_000.0),
                Some(250_000.0),
                Some("EUR".into()),
                Some(Year)
            )
        );
        assert_eq!(
            p("£60–75k"),
            (
                Some(60_000.0),
                Some(75_000.0),
                Some("GBP".into()),
                Some(Year)
            )
        );
        assert_eq!(
            p("USD 87,069 - 124,385"),
            (
                Some(87_069.0),
                Some(124_385.0),
                Some("USD".into()),
                Some(Year)
            )
        );
        assert_eq!(
            p("CA$90,000 per year"),
            (
                Some(90_000.0),
                Some(90_000.0),
                Some("CAD".into()),
                Some(Year)
            )
        );
        assert_eq!(
            p("€4.500 - €5.500 / month"),
            (
                Some(4_500.0),
                Some(5_500.0),
                Some("EUR".into()),
                Some(Month)
            )
        );
    }

    #[test]
    fn keeps_unreadable_salary_as_text_only() {
        let s = parse("$1000 per task").unwrap();
        assert_eq!((s.min, s.period), (None, None));
        assert_eq!(s.text, "$1000 per task");
        let s = parse("Competitive").unwrap();
        assert_eq!(s.min, None);
        // Small numbers with no unit: we don't know the period.
        assert_eq!(parse("$40-60").unwrap().period, None);
        assert!(parse("  ").is_none());
    }

    #[test]
    fn builds_from_numbers() {
        let s = from_range(
            Some(50_000.0),
            Some(150_000.0),
            Some("usd"),
            Some(SalaryPeriod::Year),
        )
        .unwrap();
        assert_eq!(s.text, "USD 50k–150k");
        assert!(from_range(Some(0.0), Some(0.0), None, None).is_none());
    }
}

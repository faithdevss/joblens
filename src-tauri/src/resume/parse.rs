//! Rules-based resume text → profile. Best effort: the user reviews the result in the
//! setup wizard, so when unsure we leave a field empty rather than guess.

use std::sync::LazyLock;

use regex::Regex;

use crate::profile::{
    Achievement, Certification, Contact, Education, Language, Profile, Project, Role, Skill,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Summary,
    Experience,
    Skills,
    Education,
    Projects,
    Languages,
    Certifications,
    /// A heading we recognise but don't import (interests, references, …).
    Other,
}

const MONTH: &str = r"(?:jan|feb|mar|apr|may|jun|jul|aug|sep|sept|oct|nov|dec)[a-z]*\.?";

static DATE_POINT: LazyLock<String> = LazyLock::new(|| {
    format!(r"(?:{MONTH}\s*'?\d{{2,4}}|\d{{1,2}}/\d{{4}}|\d{{4}}-\d{{2}}|\d{{4}})")
});

static DATE_RANGE: LazyLock<Regex> = LazyLock::new(|| {
    let p = &*DATE_POINT;
    Regex::new(&format!(
        r"(?i)\b{p}\s*(?:-|–|—|to|until)\s*(?:{p}|present|current|now|today)\b"
    ))
    .unwrap()
});

static YEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(?:19|20)\d{2}\b").unwrap());

static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\w.+-]+@[\w-]+(?:\.[\w-]+)+").unwrap());

static PHONE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\+?\(?\d[\d\s().-]{7,}\d").unwrap());

static LINKEDIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:https?://)?(?:[a-z]{2,3}\.)?linkedin\.com/[^\s|,]+").unwrap()
});

static GITHUB: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:https?://)?(?:www\.)?github\.com/[^\s|,]+").unwrap());

static URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:https?://)?(?:www\.)?[a-z0-9-]+(?:\.[a-z0-9-]+)*\.(?:com|dev|io|me|net|org|co|app|xyz|tech|site|page|info|design|codes)(?:/[^\s|,]*)?",
    )
    .unwrap()
});

static BULLET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:[•·▪●◦‣○■►➢✓→*]|[-–—](?:\s|$))\s*").unwrap());

/// Separators people use between title, company, location and dates on one line.
static SEPARATOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*\|\s*|\s+[—–\-·•@]\s+|\s+at\s+").unwrap());

static LOCATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?i:remote(?:\s*\(.*\))?|hybrid|on-?site)$|^\p{Lu}[\p{L} .'-]+,\s*\p{Lu}[\p{L} .'-]+$",
    )
    .unwrap()
});

static TITLE_WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:engineer|developer|programmer|manager|designer|lead|analyst|architect|consultant|director|intern|scientist|specialist|administrator|officer|head|vp|president|founder|co-founder|cto|ceo|coo|cfo|coordinator|associate|assistant|sre|devops|writer|editor|researcher|owner|technician|support|accountant|teacher|recruiter|strategist|principal|staff|senior|junior|freelance|contractor)\b",
    )
    .unwrap()
});

static DEGREE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:bachelor|master|ph\.?\s?d|doctorate|b\.?\s?sc|m\.?\s?sc|b\.?\s?tech|m\.?\s?tech|b\.?\s?e\.|m\.?\s?e\.|b\.?s\.|m\.?s\.|b\.?a\.|m\.?a\.|mba|b\.?eng|m\.?eng|bs|ms|ba|ma|associate|diploma|degree|licentiate|hnd|a-levels?)\b",
    )
    .unwrap()
});

static SCHOOL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:university|college|institute|school|academy|polytechnic|universidad|universidade|université|universität|hochschule|école|ecole)\b",
    )
    .unwrap()
});

/// "Remote", "Lisbon, Portugal" — but not "Globex, Staff Designer".
fn is_location(s: &str) -> bool {
    LOCATION.is_match(s) && !TITLE_WORD.is_match(s)
}

fn heading(line: &str) -> Option<Section> {
    if line.len() > 40 {
        return None;
    }
    // "E X P E R I E N C E" → "EXPERIENCE"
    let collapsed: String = if line.split(' ').all(|w| w.chars().count() <= 1) {
        line.chars().filter(|c| !c.is_whitespace()).collect()
    } else {
        line.to_string()
    };
    let key = collapsed
        .trim_end_matches(':')
        .to_lowercase()
        .replace(" and ", " & ");
    let key = key.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(match key.as_str() {
        "summary"
        | "professional summary"
        | "profile"
        | "professional profile"
        | "about"
        | "about me"
        | "objective"
        | "career objective"
        | "overview" => Section::Summary,
        "experience"
        | "work experience"
        | "professional experience"
        | "employment"
        | "employment history"
        | "work history"
        | "career history"
        | "relevant experience"
        | "experience & employment" => Section::Experience,
        "skills"
        | "technical skills"
        | "core skills"
        | "key skills"
        | "skills & tools"
        | "tools"
        | "technologies"
        | "tech stack"
        | "core competencies"
        | "competencies"
        | "skills & technologies"
        | "skills & expertise" => Section::Skills,
        "education" | "academic background" | "education & training" => Section::Education,
        "projects"
        | "personal projects"
        | "side projects"
        | "open source"
        | "projects & open source"
        | "selected projects" => Section::Projects,
        "languages" | "language skills" | "spoken languages" => Section::Languages,
        "certifications"
        | "certificates"
        | "licenses & certifications"
        | "certifications & licenses"
        | "courses & certifications" => Section::Certifications,
        "interests"
        | "hobbies"
        | "references"
        | "awards"
        | "publications"
        | "volunteering"
        | "volunteer experience"
        | "achievements"
        | "honors & awards"
        | "additional information" => Section::Other,
        _ => return None,
    })
}

/// The section a line heads, e.g. "Experience", when it's a heading we recognise.
pub fn section_heading(line: &str) -> Option<&'static str> {
    Some(match heading(line.trim())? {
        Section::Summary => "Summary",
        Section::Experience => "Experience",
        Section::Skills => "Skills",
        Section::Education => "Education",
        Section::Projects => "Projects",
        Section::Languages => "Languages",
        Section::Certifications => "Certifications",
        Section::Other => "Other",
    })
}

/// Splits raw extracted text into the clean, non-empty lines the profile cites by index.
pub fn to_lines(text: &str) -> Vec<String> {
    static WIDE_GAP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\t+| {3,}").unwrap());
    static BULLET_GAP: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^([•·▪●◦‣○■►➢✓→*\-–])(?:\t|\s{2,})+").unwrap());
    static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
    static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[A-Za-z]+\b").unwrap());
    text.lines()
        .map(|l| {
            let l = l
                .replace('\u{a0}', " ")
                .replace('\u{fb01}', "fi")
                .replace('\u{fb02}', "fl")
                .replace('\u{2019}', "'")
                // Hyphen, non-breaking hyphen, figure dash and minus all read as "-".
                .replace(['\u{2010}', '\u{2011}', '\u{2012}', '\u{2212}'], "-");
            let l = WORD.replace_all(&l, |c: &regex::Captures| small_caps_fix(&c[0]));
            // A tab after a bullet is just indentation.
            let l = BULLET_GAP.replace(l.trim(), "$1 ");
            // Wide gaps usually separate columns, e.g. "Engineer        Mar 2021 – Now".
            let l = WIDE_GAP.replace_all(&l, " | ");
            SPACES
                .replace_all(&l, " ")
                .trim_matches(['|', ' '])
                .to_string()
        })
        .filter(|l| !l.is_empty())
        .collect()
}

/// Some fonts map small-caps "I" to "i", so "SENIOR" is extracted as "SENiOR". Short
/// mixed words like "iOS" are left alone.
fn small_caps_fix(word: &str) -> String {
    let upper = word.chars().filter(char::is_ascii_uppercase).count();
    let only_i = word.chars().all(|c| c.is_ascii_uppercase() || c == 'i');
    if upper >= 3 && only_i && word.contains('i') {
        word.to_ascii_uppercase()
    } else {
        word.to_string()
    }
}

struct Ids {
    next: u32,
}

impl Ids {
    fn make(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}{}", self.next)
    }
}

pub fn parse(lines: &[String]) -> Profile {
    let mut ids = Ids { next: 0 };
    let mut profile = Profile::default();

    // Split into header + sections.
    let mut sections: Vec<(Section, Vec<usize>)> = Vec::new();
    let mut header: Vec<usize> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(s) = heading(line) {
            sections.push((s, Vec::new()));
        } else if let Some((_, body)) = sections.last_mut() {
            body.push(i);
        } else {
            header.push(i);
        }
    }

    // No headings at all: everything after the first few header lines is probably roles.
    if sections.is_empty() && header.len() > 4 {
        let body = header.split_off(4);
        sections.push((Section::Experience, body));
    }

    parse_contact(lines, &header, &mut profile.contact);

    for (section, body) in &sections {
        match section {
            Section::Summary => {
                profile.summary = join(lines, body);
                profile.summary_lines = to_u32(body);
            }
            Section::Experience => profile.roles.extend(parse_roles(lines, body, &mut ids)),
            Section::Skills => parse_skills(lines, body, &mut profile.skills, &mut ids),
            Section::Education => profile
                .education
                .extend(parse_education(lines, body, &mut ids)),
            Section::Projects => profile
                .projects
                .extend(parse_projects(lines, body, &mut ids)),
            Section::Languages => profile
                .languages
                .extend(parse_languages(lines, body, &mut ids)),
            Section::Certifications => profile
                .certifications
                .extend(parse_certifications(lines, body, &mut ids)),
            Section::Other => {}
        }
    }

    // Contact details are sometimes in a sidebar or footer.
    if profile.contact.email.is_empty() || profile.contact.linkedin.is_empty() {
        let all: Vec<usize> = (0..lines.len()).collect();
        let mut extra = Contact::default();
        find_links(lines, &all, &mut extra);
        let c = &mut profile.contact;
        for (field, found) in [
            (&mut c.email, extra.email),
            (&mut c.phone, extra.phone),
            (&mut c.linkedin, extra.linkedin),
            (&mut c.github, extra.github),
            (&mut c.website, extra.website),
        ] {
            if field.is_empty() {
                *field = found;
            }
        }
    }

    // No summary heading: a long sentence in the header is likely one.
    if profile.summary.is_empty() {
        let long: Vec<usize> = header
            .iter()
            .copied()
            .filter(|&i| word_count(&lines[i]) >= 12 && !EMAIL.is_match(&lines[i]))
            .collect();
        profile.summary = join(lines, &long);
        profile.summary_lines = to_u32(&long);
    }

    profile
}

fn to_u32(idx: &[usize]) -> Vec<u32> {
    idx.iter().map(|&i| i as u32).collect()
}

fn join(lines: &[String], idx: &[usize]) -> String {
    idx.iter()
        .map(|&i| strip_bullet(&lines[i]))
        .collect::<Vec<_>>()
        .join(" ")
}

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

fn strip_bullet(line: &str) -> &str {
    match BULLET.find(line) {
        Some(m) => &line[m.end()..],
        None => line,
    }
}

fn is_bullet(line: &str) -> bool {
    BULLET.is_match(line)
}

fn segments(line: &str) -> Vec<String> {
    SEPARATOR
        .split(line)
        .map(|s| s.trim_matches([',', ' ', '(', ')']).to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn find_links(lines: &[String], idx: &[usize], c: &mut Contact) {
    for &i in idx {
        let line = &lines[i];
        if c.email.is_empty()
            && let Some(m) = EMAIL.find(line)
        {
            c.email = m.as_str().to_string();
        }
        if c.linkedin.is_empty()
            && let Some(m) = LINKEDIN.find(line)
        {
            c.linkedin = m.as_str().trim_end_matches('/').to_string();
        }
        if c.github.is_empty()
            && let Some(m) = GITHUB.find(line)
        {
            c.github = m.as_str().trim_end_matches('/').to_string();
        }
        if c.phone.is_empty()
            && let Some(m) = PHONE.find(line)
            && m.as_str().chars().filter(char::is_ascii_digit).count() >= 9
            && !DATE_RANGE.is_match(m.as_str())
        {
            c.phone = m.as_str().trim().to_string();
        }
        if c.website.is_empty() {
            let without_email = EMAIL.replace_all(line, "");
            if let Some(m) = URL.find_iter(&without_email).find(|m| {
                let s = m.as_str().to_lowercase();
                !s.contains("linkedin.com") && !s.contains("github.com")
            }) {
                c.website = m.as_str().trim_end_matches('/').to_string();
            }
        }
    }
}

fn is_contact_bit(s: &str) -> bool {
    EMAIL.is_match(s)
        || URL.is_match(s)
        || LINKEDIN.is_match(s)
        || GITHUB.is_match(s)
        || PHONE
            .find(s)
            .is_some_and(|m| m.as_str().chars().filter(char::is_ascii_digit).count() >= 9)
}

fn looks_like_name(s: &str) -> bool {
    let words = word_count(s);
    (2..=5).contains(&words)
        && s.chars().all(|c| c.is_alphabetic() || " .'-".contains(c))
        && s.split_whitespace()
            .all(|w| w.chars().next().is_some_and(char::is_uppercase))
}

fn parse_contact(lines: &[String], header: &[usize], c: &mut Contact) {
    let header: Vec<usize> = header.iter().copied().take(12).collect();
    find_links(lines, &header, c);
    for &i in &header {
        let mut used = false;
        for seg in segments(&lines[i]) {
            if is_contact_bit(&seg) {
                used = true;
            } else if c.name.is_empty() && looks_like_name(&seg) && !TITLE_WORD.is_match(&seg) {
                c.name = seg;
                used = true;
            } else if c.location.is_empty() && is_location(&seg) {
                c.location = seg;
                used = true;
            } else if c.headline.is_empty() && word_count(&seg) <= 10 && TITLE_WORD.is_match(&seg) {
                c.headline = seg;
                used = true;
            }
        }
        if used {
            c.lines.push(i as u32);
        }
    }
}

/// Splits role header text into (title, company), dropping locations.
fn title_company(parts: &[String]) -> (String, String) {
    let mut segs: Vec<String> = parts
        .iter()
        .flat_map(|p| segments(p))
        .filter(|s| !is_location(s))
        .collect();
    if segs.len() == 1
        && let Some((a, b)) = segs[0].clone().split_once(',')
    {
        segs = vec![a.trim().to_string(), b.trim().to_string()];
    }
    match segs.iter().position(|s| TITLE_WORD.is_match(s)) {
        Some(t) => {
            let title = segs.remove(t);
            (title, segs.into_iter().next().unwrap_or_default())
        }
        None => {
            let mut it = segs.into_iter();
            (it.next().unwrap_or_default(), it.next().unwrap_or_default())
        }
    }
}

fn parse_roles(lines: &[String], body: &[usize], ids: &mut Ids) -> Vec<Role> {
    let mut roles: Vec<Role> = Vec::new();
    // Non-bullet, short lines seen since the last achievement: header text for the next role.
    let mut pending: Vec<usize> = Vec::new();
    let mut last_was_ach = false;

    let new_role = |ids: &mut Ids, header: &[usize], extra: Option<String>, dates: String| {
        let mut parts: Vec<String> = header.iter().map(|&i| lines[i].clone()).collect();
        parts.extend(extra);
        let (title, company) = title_company(&parts);
        Role {
            id: ids.make("r"),
            title,
            company,
            dates,
            lines: to_u32(header),
            ..Default::default()
        }
    };

    for &i in body {
        let line = &lines[i];
        if !is_bullet(line)
            && let Some(m) = DATE_RANGE.find(line)
        {
            let rest = format!("{} {}", &line[..m.start()], &line[m.end()..]);
            let rest = rest.trim_matches(|c: char| c.is_whitespace() || "|,–—-·()".contains(c));
            let extra = (!rest.is_empty()).then(|| rest.to_string());
            let mut role = new_role(ids, &pending, extra, m.as_str().to_string());
            role.lines.push(i as u32);
            roles.push(role);
            pending.clear();
            last_was_ach = false;
            continue;
        }

        let text = strip_bullet(line).to_string();
        let continues = last_was_ach
            && !is_bullet(line)
            && (text.starts_with(|c: char| c.is_lowercase() || c.is_ascii_digit())
                || (word_count(&text) > 8
                    && roles
                        .last()
                        .and_then(|r| r.achievements.last())
                        .is_some_and(|a| !a.text.ends_with(['.', '!', '?']))));
        if continues {
            let ach = roles
                .last_mut()
                .and_then(|r| r.achievements.last_mut())
                .expect("last_was_ach implies an achievement");
            ach.text = format!("{} {text}", ach.text);
            ach.lines.push(i as u32);
            continue;
        }

        let sentence = word_count(&text) > 10 || text.ends_with('.');
        if is_bullet(line) || (sentence && !roles.is_empty()) {
            let unfinished = roles
                .last()
                .filter(|r| r.company.is_empty() && r.achievements.is_empty());
            if let Some(role) = unfinished
                && !pending.is_empty()
                && pending.len() <= 2
            {
                // "Title  Dates" then "Company" on its own line: finish that role.
                let mut parts: Vec<String> = (!role.title.is_empty())
                    .then(|| role.title.clone())
                    .into_iter()
                    .collect();
                parts.extend(pending.iter().map(|&p| lines[p].clone()));
                let (title, company) = title_company(&parts);
                let role = roles.last_mut().unwrap();
                (role.title, role.company) = (title, company);
                role.lines.extend(to_u32(&pending));
                pending.clear();
            } else if roles.is_empty() || (!pending.is_empty() && pending.len() <= 2) {
                // Header lines with no dates: still a role, the date check will flag it.
                roles.push(new_role(ids, &pending, None, String::new()));
                pending.clear();
            }
            for p in pending.drain(..) {
                push_ach(roles.last_mut().unwrap(), ids, lines[p].clone(), p);
            }
            push_ach(roles.last_mut().unwrap(), ids, text, i);
            last_was_ach = true;
        } else {
            pending.push(i);
            last_was_ach = false;
        }
    }

    // Trailing short lines with no role after them: keep anything with substance.
    if let Some(role) = roles.last_mut() {
        for p in pending {
            if word_count(&lines[p]) > 4 {
                push_ach(role, ids, strip_bullet(&lines[p]).to_string(), p);
            }
        }
    }
    roles
}

fn push_ach(role: &mut Role, ids: &mut Ids, text: String, line: usize) {
    role.achievements.push(Achievement {
        id: ids.make("a"),
        text,
        lines: vec![line as u32],
    });
}

fn parse_skills(lines: &[String], body: &[usize], skills: &mut Vec<Skill>, ids: &mut Ids) {
    for &i in body {
        let mut line = strip_bullet(&lines[i]);
        // "Languages: Go, Rust" → "Go, Rust"
        if let Some((label, rest)) = line.split_once(':')
            && word_count(label) <= 4
        {
            line = rest;
        }
        for item in line.split([',', ';', '|', '•', '·']) {
            let name = item.trim().trim_end_matches('.').trim();
            if name.is_empty() || name.len() > 40 || word_count(name) > 5 {
                continue;
            }
            if skills.iter().any(|s| s.name.eq_ignore_ascii_case(name)) {
                continue;
            }
            skills.push(Skill {
                id: ids.make("s"),
                name: name.to_string(),
                lines: vec![i as u32],
            });
        }
    }
}

fn take_dates(s: &str) -> (String, String) {
    let found = DATE_RANGE.find(s).or_else(|| YEAR.find_iter(s).last());
    match found {
        Some(m) => {
            let rest = format!("{}{}", &s[..m.start()], &s[m.end()..]);
            let rest = rest.trim_matches(|c: char| c.is_whitespace() || "|,–—-·()".contains(c));
            (rest.to_string(), m.as_str().to_string())
        }
        None => (s.to_string(), String::new()),
    }
}

fn parse_education(lines: &[String], body: &[usize], ids: &mut Ids) -> Vec<Education> {
    let mut out: Vec<Education> = Vec::new();
    for &i in body {
        let (text, dates) = take_dates(strip_bullet(&lines[i]));
        let mut touched = false;
        // "BSc Computer Science, University of Lisbon" → two parts.
        let segs = segments(&text)
            .into_iter()
            .flat_map(|seg| match seg.split_once(", ") {
                Some((a, b)) if SCHOOL.is_match(&seg) && DEGREE.is_match(&seg) => {
                    vec![a.to_string(), b.to_string()]
                }
                _ => vec![seg],
            });
        for seg in segs {
            let is_school = SCHOOL.is_match(&seg);
            let is_degree = !is_school && DEGREE.is_match(&seg);
            if !is_school && !is_degree {
                continue;
            }
            let full = out.last().is_none_or(|e| {
                if is_school {
                    !e.school.is_empty()
                } else {
                    !e.degree.is_empty()
                }
            });
            if full {
                out.push(Education {
                    id: ids.make("e"),
                    ..Default::default()
                });
            }
            let e = out.last_mut().unwrap();
            if is_school {
                e.school = seg;
            } else {
                e.degree = seg;
            }
            touched = true;
        }
        if let Some(e) = out.last_mut() {
            if !dates.is_empty() && e.dates.is_empty() {
                e.dates = dates;
                touched = true;
            }
            if touched {
                e.lines.push(i as u32);
            }
        }
    }
    out
}

fn parse_projects(lines: &[String], body: &[usize], ids: &mut Ids) -> Vec<Project> {
    let mut out: Vec<Project> = Vec::new();
    for &i in body {
        let line = &lines[i];
        let text = strip_bullet(line);
        let is_name = !is_bullet(line)
            && (SEPARATOR.is_match(text) || (word_count(text) <= 6 && !text.ends_with('.')));
        if is_name || out.is_empty() {
            let link = URL
                .find(text)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let rest = URL.replace(text, "");
            let mut segs = segments(&rest).into_iter();
            out.push(Project {
                id: ids.make("p"),
                name: segs.next().unwrap_or_default(),
                link,
                description: segs.collect::<Vec<_>>().join(" — "),
                lines: vec![i as u32],
            });
        } else {
            let p = out.last_mut().unwrap();
            p.description = if p.description.is_empty() {
                text.to_string()
            } else {
                format!("{} {text}", p.description)
            };
            p.lines.push(i as u32);
        }
    }
    out
}

fn language_level(s: &str) -> Option<&'static str> {
    let s = s.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| s.contains(w));
    if has(&["native", "mother tongue", "bilingual"]) {
        Some("Native")
    } else if has(&["fluent", "c2", "c1", "full professional", "advanced"]) {
        Some("Fluent")
    } else if has(&["professional", "working", "b2", "b1", "intermediate"]) {
        Some("Professional")
    } else if has(&[
        "basic",
        "elementary",
        "beginner",
        "a1",
        "a2",
        "conversational",
    ]) {
        Some("Basic")
    } else {
        None
    }
}

fn parse_languages(lines: &[String], body: &[usize], ids: &mut Ids) -> Vec<Language> {
    static NAME_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*[(:–—-]").unwrap());
    let mut out = Vec::new();
    for &i in body {
        for item in strip_bullet(&lines[i]).split([',', ';', '|', '•', '·']) {
            let item = item.trim();
            let name = NAME_END.split(item).next().unwrap_or("").trim();
            if name.is_empty() || word_count(name) > 3 {
                continue;
            }
            out.push(Language {
                id: ids.make("l"),
                name: name.to_string(),
                level: language_level(item).unwrap_or("").to_string(),
                lines: vec![i as u32],
            });
        }
    }
    out
}

fn parse_certifications(lines: &[String], body: &[usize], ids: &mut Ids) -> Vec<Certification> {
    body.iter()
        .filter_map(|&i| {
            let text = strip_bullet(&lines[i]);
            let year = YEAR.find_iter(text).last();
            let rest = match year {
                Some(m) => format!("{}{}", &text[..m.start()], &text[m.end()..]),
                None => text.to_string(),
            };
            let mut segs = segments(&rest);
            if segs.len() == 1
                && let Some((a, b)) = segs[0].clone().rsplit_once(", ")
            {
                segs = vec![a.to_string(), b.to_string()];
            }
            let mut segs = segs.into_iter();
            let name = segs.next()?;
            Some(Certification {
                id: ids.make("c"),
                name,
                issuer: segs.next().unwrap_or_default(),
                year: year.map(|m| m.as_str().to_string()).unwrap_or_default(),
                lines: vec![i as u32],
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> (Vec<String>, Profile) {
        let lines = to_lines(text);
        let p = parse(&lines);
        (lines, p)
    }

    const CLASSIC: &str = "\
Sam Rivera
Senior Frontend Engineer · Lisbon, Portugal
sam.rivera@example.com | +351 912 345 678 | linkedin.com/in/samrivera | github.com/samr | samrivera.dev

SUMMARY
Frontend engineer with eight years building design systems and fast React apps
for B2B products.

EXPERIENCE
Senior Frontend Engineer — Brook          Mar 2021 – Present
• Led the migration of a 200k-line React app to TypeScript across four teams.
• Cut bundle size by 40% by splitting routes and removing unused
  dependencies.
• Mentored three engineers.
Frontend Engineer | Halcyon Labs | Remote
2018 – 2021
- Built the component library used by 12 product teams.
- Shipped offline mode for the field app.

SKILLS
Languages: TypeScript, JavaScript, Go
Frameworks: React, Next.js, Node.js; GraphQL
CI/CD, Testing Library

EDUCATION
BSc Computer Science, University of Lisbon, 2014 – 2017

PROJECTS
tinyforms — github.com/samr/tinyforms
A 2 kB form library for React with 3k stars.

LANGUAGES
Portuguese (Native), English (Fluent), Spanish – basic

CERTIFICATIONS
AWS Certified Developer — Amazon Web Services (2022)
Certified Kubernetes Administrator, CNCF, 2023

INTERESTS
Climbing
";

    #[test]
    fn reads_contact_block() {
        let (lines, p) = run(CLASSIC);
        let c = &p.contact;
        assert_eq!(c.name, "Sam Rivera");
        assert_eq!(c.headline, "Senior Frontend Engineer");
        assert_eq!(c.location, "Lisbon, Portugal");
        assert_eq!(c.email, "sam.rivera@example.com");
        assert_eq!(c.phone, "+351 912 345 678");
        assert_eq!(c.linkedin, "linkedin.com/in/samrivera");
        assert_eq!(c.github, "github.com/samr");
        assert_eq!(c.website, "samrivera.dev");
        assert_eq!(c.lines, vec![0, 1, 2]);
        assert_eq!(lines[0], "Sam Rivera");
    }

    #[test]
    fn reads_summary_with_source_lines() {
        let (lines, p) = run(CLASSIC);
        assert!(p.summary.starts_with("Frontend engineer with eight years"));
        assert!(p.summary.ends_with("for B2B products."));
        assert_eq!(p.summary_lines.len(), 2);
        assert_eq!(
            lines[p.summary_lines[0] as usize],
            "Frontend engineer with eight years building design systems and fast React apps"
        );
    }

    #[test]
    fn reads_roles_dates_and_achievements() {
        let (lines, p) = run(CLASSIC);
        assert_eq!(p.roles.len(), 2, "{:#?}", p.roles);

        let brook = &p.roles[0];
        assert_eq!(brook.title, "Senior Frontend Engineer");
        assert_eq!(brook.company, "Brook");
        assert_eq!(brook.dates, "Mar 2021 – Present");
        assert_eq!(brook.achievements.len(), 3);
        // Wrapped bullet is joined and cites both lines.
        let cut = &brook.achievements[1];
        assert_eq!(
            cut.text,
            "Cut bundle size by 40% by splitting routes and removing unused dependencies."
        );
        assert_eq!(cut.lines.len(), 2);
        assert_eq!(lines[cut.lines[1] as usize], "dependencies.");

        let halcyon = &p.roles[1];
        assert_eq!(halcyon.title, "Frontend Engineer");
        assert_eq!(halcyon.company, "Halcyon Labs");
        assert_eq!(halcyon.dates, "2018 – 2021");
        assert_eq!(halcyon.achievements.len(), 2);
        assert_eq!(halcyon.lines.len(), 2);
    }

    #[test]
    fn every_cited_line_contains_the_text() {
        let (lines, p) = run(CLASSIC);
        for role in &p.roles {
            for a in &role.achievements {
                let first = strip_bullet(&lines[a.lines[0] as usize]);
                assert!(a.text.starts_with(first), "{} vs {}", a.text, first);
            }
        }
        for s in &p.skills {
            assert!(lines[s.lines[0] as usize].contains(&s.name), "{}", s.name);
        }
    }

    #[test]
    fn reads_skills_without_labels_or_duplicates() {
        let (_, p) = run(CLASSIC);
        let names: Vec<&str> = p.skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "TypeScript",
                "JavaScript",
                "Go",
                "React",
                "Next.js",
                "Node.js",
                "GraphQL",
                "CI/CD",
                "Testing Library"
            ]
        );
    }

    #[test]
    fn reads_education_projects_languages_certs() {
        let (_, p) = run(CLASSIC);
        assert_eq!(p.education.len(), 1);
        assert_eq!(p.education[0].degree, "BSc Computer Science");
        assert_eq!(p.education[0].school, "University of Lisbon");
        assert_eq!(p.education[0].dates, "2014 – 2017");

        assert_eq!(p.projects.len(), 1);
        assert_eq!(p.projects[0].name, "tinyforms");
        assert_eq!(p.projects[0].link, "github.com/samr/tinyforms");
        assert_eq!(
            p.projects[0].description,
            "A 2 kB form library for React with 3k stars."
        );

        let langs: Vec<(&str, &str)> = p
            .languages
            .iter()
            .map(|l| (l.name.as_str(), l.level.as_str()))
            .collect();
        assert_eq!(
            langs,
            [
                ("Portuguese", "Native"),
                ("English", "Fluent"),
                ("Spanish", "Basic")
            ]
        );

        let certs: Vec<(&str, &str, &str)> = p
            .certifications
            .iter()
            .map(|c| (c.name.as_str(), c.issuer.as_str(), c.year.as_str()))
            .collect();
        assert_eq!(
            certs,
            [
                ("AWS Certified Developer", "Amazon Web Services", "2022"),
                ("Certified Kubernetes Administrator", "CNCF", "2023")
            ]
        );
    }

    #[test]
    fn ignores_sections_we_dont_import() {
        let (_, p) = run(CLASSIC);
        assert!(!p.skills.iter().any(|s| s.name == "Climbing"));
        assert!(
            !p.roles
                .iter()
                .flat_map(|r| &r.achievements)
                .any(|a| a.text == "Climbing")
        );
    }

    #[test]
    fn reads_roles_without_bullets_and_title_after_company() {
        let text = "\
Ana Costa
ana@example.com

Work Experience
Acme Corp
Product Designer
Jan 2020 - Dec 2022
Redesigned onboarding, which lifted activation by 18 percent in two quarters.
Ran weekly research sessions with customers and shared findings with product.

Globex, Staff Designer, 06/2017 to 12/2019
Owned the design system.
";
        let (_, p) = run(text);
        assert_eq!(p.roles.len(), 2, "{:#?}", p.roles);
        assert_eq!(p.roles[0].title, "Product Designer");
        assert_eq!(p.roles[0].company, "Acme Corp");
        assert_eq!(p.roles[0].dates, "Jan 2020 - Dec 2022");
        assert_eq!(p.roles[0].achievements.len(), 2);
        assert_eq!(p.roles[1].title, "Staff Designer");
        assert_eq!(p.roles[1].company, "Globex");
        assert_eq!(p.roles[1].dates, "06/2017 to 12/2019");
        assert_eq!(p.roles[1].achievements[0].text, "Owned the design system.");
    }

    #[test]
    fn handles_word_list_tabs_and_comma_titles() {
        let lines = to_lines("•\tShipped the thing to production.\n-    Another one.\n");
        assert_eq!(
            lines,
            ["• Shipped the thing to production.", "- Another one."]
        );
        let (title, company) = title_company(&["Staff Engineer , Razorleaf".to_string()]);
        assert_eq!(
            (title.as_str(), company.as_str()),
            ("Staff Engineer", "Razorleaf")
        );
    }

    #[test]
    fn reads_indian_degree_names() {
        let lines =
            to_lines("Education\nB.Tech in Computer Science, Anna University, 2010 – 2014\n");
        let p = parse(&lines);
        assert_eq!(p.education[0].degree, "B.Tech in Computer Science");
        assert_eq!(p.education[0].school, "Anna University");
    }

    #[test]
    fn handles_spaced_and_colon_headings() {
        assert_eq!(heading("E X P E R I E N C E"), Some(Section::Experience));
        assert_eq!(heading("Skills:"), Some(Section::Skills));
        assert_eq!(heading("Skills and Tools"), Some(Section::Skills));
        assert_eq!(heading("Languages: Go, Rust"), None);
    }

    #[test]
    fn leaves_unknown_fields_empty() {
        let (_, p) = run("Just Some Text\nwith nothing useful in it at all\n");
        assert_eq!(p.contact.email, "");
        assert_eq!(p.contact.phone, "");
        assert!(p.roles.is_empty());
        assert!(p.skills.is_empty());
    }

    #[test]
    fn reads_dates_with_typographic_dashes_and_small_caps() {
        // PDFs often use U+2011 / U+2212 dashes, and some fonts turn small-caps "I" into "i".
        let text = "\
Sam Rivera
sam@example.com

Experience
SENiOR SOFTWARE ENGiNEER October 2021 \u{2011} Present
Acme Corp
Led the move to a typed API layer used by every team.

Software Engineer, Globex, 2018 \u{2212} 2021
Built the billing service on iOS and web.
";
        let (_, p) = run(text);
        assert_eq!(p.roles.len(), 2, "{:#?}", p.roles);
        assert_eq!(p.roles[0].title, "SENIOR SOFTWARE ENGINEER");
        assert_eq!(p.roles[0].company, "Acme Corp");
        assert_eq!(p.roles[0].dates, "October 2021 - Present");
        assert_eq!(p.roles[1].title, "Software Engineer");
        assert_eq!(p.roles[1].dates, "2018 - 2021");
        assert_eq!(
            p.roles[1].achievements[0].text,
            "Built the billing service on iOS and web."
        );
    }

    #[test]
    fn date_ranges_are_not_phone_numbers() {
        let (_, p) = run("Jo Bloggs\nEngineer, Initech 2019 - 2021\n");
        assert_eq!(p.contact.phone, "");
    }

    #[test]
    fn ids_are_unique() {
        let (_, p) = run(CLASSIC);
        let mut ids: Vec<&str> = p
            .roles
            .iter()
            .flat_map(|r| {
                std::iter::once(r.id.as_str()).chain(r.achievements.iter().map(|a| a.id.as_str()))
            })
            .chain(p.skills.iter().map(|s| s.id.as_str()))
            .chain(p.education.iter().map(|e| e.id.as_str()))
            .collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }
}

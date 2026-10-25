//! Rules-only match score and must-have / avoid keyword filters.
//!
//! The score ranks the feed without AI. It compares the posting with the profile on
//! four factors (skills, experience, role, seniority). Where the posting or profile
//! doesn't say, the factor gets a neutral value instead of a guess.

use std::{collections::BTreeSet, sync::LazyLock};

use regex::Regex;

use super::{
    MatchBreakdown, MatchedSkill, Weights,
    text::{MAX_PHRASE_WORDS, Phrases, normalize_phrase, word_count},
};
use crate::profile::{Profile, SearchSettings, Seniority};

/// Value used when there isn't enough information to compare.
const NEUTRAL: u32 = 60;
pub const SHORT_DESCRIPTION_WORDS: usize = 150;

/// Known skills: display name and how postings write it (normalized). Used to find what
/// a posting asks for. Profile skills outside this list are still matched by name.
const SKILLS: &[(&str, &[&str])] = &[
    ("JavaScript", &["javascript", "js", "es6"]),
    ("TypeScript", &["typescript", "ts"]),
    ("React", &["react", "react.js", "reactjs"]),
    ("React Native", &["react native"]),
    ("Vue", &["vue", "vue.js", "vuejs"]),
    ("Angular", &["angular", "angularjs"]),
    ("Svelte", &["svelte", "sveltekit"]),
    ("Next.js", &["next.js", "nextjs"]),
    ("Nuxt", &["nuxt", "nuxt.js"]),
    ("Node.js", &["node.js", "nodejs", "node"]),
    ("Deno", &["deno"]),
    ("Express", &["express.js", "expressjs"]),
    ("NestJS", &["nestjs", "nest.js"]),
    ("GraphQL", &["graphql"]),
    ("REST APIs", &["rest api", "rest apis", "restful"]),
    ("HTML", &["html", "html5"]),
    ("CSS", &["css", "css3"]),
    ("Sass", &["sass", "scss"]),
    ("Tailwind", &["tailwind", "tailwindcss", "tailwind css"]),
    ("Redux", &["redux"]),
    ("Webpack", &["webpack"]),
    ("Vite", &["vite"]),
    ("Jest", &["jest"]),
    ("Playwright", &["playwright"]),
    ("Cypress", &["cypress"]),
    ("Storybook", &["storybook"]),
    ("Accessibility", &["accessibility", "a11y", "wcag"]),
    ("Python", &["python"]),
    ("Django", &["django"]),
    ("Flask", &["flask"]),
    ("FastAPI", &["fastapi"]),
    ("Ruby", &["ruby"]),
    ("Rails", &["rails", "ruby on rails"]),
    ("PHP", &["php"]),
    ("Laravel", &["laravel"]),
    ("Java", &["java"]),
    ("Kotlin", &["kotlin"]),
    ("Spring Boot", &["spring boot"]),
    ("Scala", &["scala"]),
    ("Go", &["golang"]),
    ("Rust", &["rust"]),
    ("C++", &["c++", "cpp"]),
    ("C#", &["c#", "csharp"]),
    (".NET", &["dotnet", "asp.net"]),
    ("Swift", &["swift"]),
    ("SwiftUI", &["swiftui"]),
    ("Objective-C", &["objective-c"]),
    ("iOS", &["ios"]),
    ("Android", &["android"]),
    ("Flutter", &["flutter"]),
    ("Dart", &["dart"]),
    ("Elixir", &["elixir"]),
    ("Erlang", &["erlang"]),
    ("Haskell", &["haskell"]),
    ("Clojure", &["clojure"]),
    ("SQL", &["sql"]),
    ("PostgreSQL", &["postgresql", "postgres"]),
    ("MySQL", &["mysql"]),
    ("SQLite", &["sqlite"]),
    ("MongoDB", &["mongodb", "mongo"]),
    ("Redis", &["redis"]),
    ("Elasticsearch", &["elasticsearch"]),
    ("Kafka", &["kafka"]),
    ("RabbitMQ", &["rabbitmq"]),
    ("Snowflake", &["snowflake"]),
    ("BigQuery", &["bigquery"]),
    ("dbt", &["dbt"]),
    ("Spark", &["spark", "apache spark", "pyspark"]),
    ("Airflow", &["airflow"]),
    ("Pandas", &["pandas"]),
    ("NumPy", &["numpy"]),
    ("TensorFlow", &["tensorflow"]),
    ("PyTorch", &["pytorch"]),
    ("scikit-learn", &["scikit-learn", "sklearn"]),
    ("Machine learning", &["machine learning", "ml"]),
    ("Deep learning", &["deep learning"]),
    ("NLP", &["nlp", "natural language processing"]),
    ("LLMs", &["llm", "llms", "large language models"]),
    ("Computer vision", &["computer vision"]),
    ("Data engineering", &["data engineering"]),
    ("AWS", &["aws", "amazon web services"]),
    ("GCP", &["gcp", "google cloud"]),
    ("Azure", &["azure"]),
    ("Docker", &["docker"]),
    ("Kubernetes", &["kubernetes", "k8s"]),
    ("Terraform", &["terraform"]),
    ("Ansible", &["ansible"]),
    ("Linux", &["linux"]),
    ("CI/CD", &["ci cd"]),
    ("GitHub Actions", &["github actions"]),
    ("Jenkins", &["jenkins"]),
    ("Git", &["git"]),
    ("Microservices", &["microservices"]),
    ("Serverless", &["serverless"]),
    ("Figma", &["figma"]),
    ("Sketch", &["sketch"]),
    ("User research", &["user research", "ux research"]),
    ("Agile", &["agile"]),
    ("Scrum", &["scrum"]),
    ("Jira", &["jira"]),
    ("SEO", &["seo"]),
    ("Salesforce", &["salesforce"]),
    ("HubSpot", &["hubspot"]),
    ("Excel", &["excel"]),
    ("Tableau", &["tableau"]),
    ("Power BI", &["power bi"]),
    ("Looker", &["looker"]),
    ("Unity", &["unity"]),
    ("Unreal", &["unreal", "unreal engine"]),
    ("WebGL", &["webgl"]),
    ("Three.js", &["three.js", "threejs"]),
    ("D3", &["d3", "d3.js"]),
    ("Electron", &["electron"]),
    ("gRPC", &["grpc"]),
    ("Solidity", &["solidity"]),
    ("Shopify", &["shopify"]),
    ("WordPress", &["wordpress"]),
];

/// Skills that are also everyday words: only count them written exactly like this.
const EXACT_CASE: &[(&str, &str)] = &[
    ("go", "Go"),
    ("r", "R"),
    ("c", "C"),
    (".net", "NET"),
    ("net", "NET"),
    ("express", "Express"),
    ("rest", "REST"),
];

struct Term {
    name: String,
    keys: Vec<String>,
    exact: Option<&'static str>,
    lines: Vec<u32>,
}

impl Term {
    fn found(&self, p: &Phrases) -> bool {
        self.keys.iter().any(|k| p.contains(k)) || self.exact.is_some_and(|e| p.contains_exact(e))
    }
}

fn exact_for(key: &str) -> Option<&'static str> {
    EXACT_CASE.iter().find(|(k, _)| *k == key).map(|(_, e)| *e)
}

fn dictionary_term(name: &str, aliases: &[&str]) -> Term {
    let key = normalize_phrase(name);
    let exact = exact_for(&key).or_else(|| exact_for(&name.to_lowercase()));
    let mut keys: Vec<String> = aliases.iter().map(|a| normalize_phrase(a)).collect();
    if exact.is_none() && !keys.contains(&key) {
        keys.push(key);
    }
    Term {
        name: name.to_string(),
        keys,
        exact,
        lines: Vec::new(),
    }
}

static DICTIONARY: LazyLock<Vec<Term>> =
    LazyLock::new(|| SKILLS.iter().map(|(n, a)| dictionary_term(n, a)).collect());

/// Title words that say nothing about the role itself.
const TITLE_NOISE: &[&str] = &[
    "senior",
    "sr",
    "junior",
    "jr",
    "staff",
    "principal",
    "lead",
    "head",
    "mid",
    "level",
    "i",
    "ii",
    "iii",
    "iv",
    "remote",
    "the",
    "of",
    "and",
    "for",
    "a",
    "an",
    "to",
    "in",
    "at",
    "with",
    "contract",
    "full",
    "time",
    "part",
    "fulltime",
    "us",
    "eu",
    "emea",
    "latam",
    "apac",
    "only",
    "hybrid",
    "global",
    "worldwide",
    "m",
    "f",
    "d",
    "x",
];

fn title_tokens(title: &str) -> BTreeSet<String> {
    let t = title
        .to_lowercase()
        .replace("front-end", "frontend")
        .replace("front end", "frontend")
        .replace("back-end", "backend")
        .replace("back end", "backend")
        .replace("full-stack", "fullstack")
        .replace("full stack", "fullstack");
    t.split(|c: char| !c.is_alphanumeric() && c != '+' && c != '#')
        .filter(|w| !w.is_empty() && !TITLE_NOISE.contains(w))
        .map(|w| match w {
            "developer" | "dev" | "eng" | "engineers" | "developers" => "engineer".to_string(),
            "programmer" => "engineer".to_string(),
            w => w.to_string(),
        })
        .collect()
}

fn rank(s: Seniority) -> i32 {
    match s {
        Seniority::Junior => 0,
        Seniority::Mid => 1,
        Seniority::Senior => 2,
        Seniority::Staff | Seniority::Lead => 3,
    }
}

pub fn seniority_label(s: Seniority) -> &'static str {
    match s {
        Seniority::Junior => "Junior",
        Seniority::Mid => "Mid",
        Seniority::Senior => "Senior",
        Seniority::Staff => "Staff",
        Seniority::Lead => "Lead",
    }
}

/// Seniority stated in a job title, if any.
pub fn title_seniority(title: &str) -> Option<Seniority> {
    let lower = title.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let has = |opts: &[&str]| words.iter().any(|w| opts.contains(w));
    if has(&[
        "intern",
        "internship",
        "graduate",
        "entry",
        "junior",
        "jr",
        "trainee",
    ]) {
        Some(Seniority::Junior)
    } else if has(&["staff", "principal", "distinguished"]) {
        Some(Seniority::Staff)
    } else if has(&["lead", "head", "manager", "director", "vp"]) {
        Some(Seniority::Lead)
    } else if has(&["senior", "sr", "iii"]) {
        Some(Seniority::Senior)
    } else if has(&["mid", "intermediate", "ii"]) {
        Some(Seniority::Mid)
    } else {
        None
    }
}

static YEARS_REQUIRED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?-u:\b)(\d{1,2})\s*\+?\s*(?:(?:-|–|to)\s*\d{1,2}\s*\+?\s*)?(?:years?|yrs?)(?:['’]s?)?\s+(?:of\s+)?(?:[\w\-/]+\s+){0,4}?(?:experience|exp)(?-u:\b)").unwrap()
});
static YEAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?-u:\b)(19[5-9]\d|20\d{2})(?-u:\b)").unwrap());
static PRESENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?-u:\b)(present|current|now|today|ongoing)(?-u:\b)").unwrap()
});

pub fn years_required(text: &str) -> Option<u32> {
    YEARS_REQUIRED
        .captures_iter(text)
        .filter_map(|c| c[1].parse::<u32>().ok())
        .filter(|y| (1..=15).contains(y))
        .max()
}

/// Years from the earliest role start to the latest end (or now).
pub fn years_on_profile(profile: &Profile, this_year: i32) -> Option<u32> {
    let mut first = None::<i32>;
    let mut last = None::<i32>;
    for role in &profile.roles {
        let years: Vec<i32> = YEAR
            .find_iter(&role.dates)
            .filter_map(|m| m.as_str().parse().ok())
            .collect();
        let Some(start) = years.first().copied() else {
            continue;
        };
        let end = if PRESENT.is_match(&role.dates) {
            this_year
        } else {
            years.last().copied().unwrap_or(start)
        };
        first = Some(first.map_or(start, |f| f.min(start)));
        last = Some(last.map_or(end, |l| l.max(end)));
    }
    Some((last? - first?).max(0) as u32)
}

/// Experience factor from the years a posting asks for and the years the profile shows.
pub fn experience_factor(required: Option<u32>, have: Option<u32>) -> u32 {
    match (required, have) {
        (None, _) => NEUTRAL + 10,
        (Some(_), None) => NEUTRAL - 10,
        (Some(req), Some(have)) if have >= req => 100,
        (Some(req), Some(have)) => 100u32.saturating_sub(20 * (req - have)).max(20),
    }
}

/// Everything about the user the score and filters need, prepared once per feed.
pub struct MatchContext {
    weights: Weights,
    skills: Vec<Term>,
    targets: Vec<BTreeSet<String>>,
    seniority: Vec<Seniority>,
    years: Option<u32>,
    must_have: Vec<String>,
    avoid: Vec<String>,
}

fn keyword(k: &str) -> Option<String> {
    let n = normalize_phrase(k);
    (!n.is_empty()).then_some(n)
}

impl MatchContext {
    pub fn new(
        profile: &Profile,
        search: &SearchSettings,
        weights: Weights,
        this_year: i32,
    ) -> Self {
        let mut skills: Vec<Term> = Vec::new();
        for s in &profile.skills {
            let key = normalize_phrase(&s.name);
            if key.is_empty() || skills.iter().any(|t| t.keys.contains(&key)) {
                continue;
            }
            // Use the dictionary's spellings when it knows this skill.
            let known = DICTIONARY
                .iter()
                .find(|t| t.keys.contains(&key) || normalize_phrase(&t.name) == key);
            let term = match known {
                Some(t) => Term {
                    name: s.name.clone(),
                    keys: t.keys.clone(),
                    exact: t.exact,
                    lines: s.lines.clone(),
                },
                None => Term {
                    name: s.name.clone(),
                    exact: exact_for(&key),
                    keys: if exact_for(&key).is_some() {
                        Vec::new()
                    } else {
                        vec![key]
                    },
                    lines: s.lines.clone(),
                },
            };
            skills.push(term);
        }
        let targets = search
            .titles
            .iter()
            .chain(profile.roles.iter().map(|r| &r.title))
            .map(|t| title_tokens(t))
            .filter(|t| !t.is_empty())
            .collect();
        Self {
            weights,
            skills,
            targets,
            seniority: search.seniority.clone(),
            years: years_on_profile(profile, this_year),
            must_have: search.must_have.iter().filter_map(|k| keyword(k)).collect(),
            avoid: search.avoid.iter().filter_map(|k| keyword(k)).collect(),
        }
    }

    fn has_keyword(p: &Phrases, lower_text: &str, k: &str) -> bool {
        if k.split(' ').count() > MAX_PHRASE_WORDS {
            lower_text.contains(k)
        } else {
            p.contains(k)
        }
    }

    /// False when the job lacks a must-have keyword or mentions one to avoid.
    pub fn passes_keywords(&self, p: &Phrases, lower_text: &str) -> bool {
        self.must_have
            .iter()
            .all(|k| Self::has_keyword(p, lower_text, k))
            && !self
                .avoid
                .iter()
                .any(|k| Self::has_keyword(p, lower_text, k))
    }

    pub fn breakdown(
        &self,
        title: &str,
        description: &str,
        tags: &[String],
        p: &Phrases,
    ) -> MatchBreakdown {
        let tag_phrases = Phrases::new(&tags.join(", "));
        let in_job = |t: &Term| t.found(p) || t.found(&tag_phrases);

        let matched: Vec<&Term> = self.skills.iter().filter(|t| in_job(t)).collect();
        let mut job_terms: Vec<&str> = matched.iter().map(|t| t.name.as_str()).collect();
        let mut missing = Vec::new();
        for t in DICTIONARY.iter() {
            let on_profile = self
                .skills
                .iter()
                .any(|s| s.keys == t.keys && s.exact == t.exact);
            if !on_profile && in_job(t) {
                job_terms.push(&t.name);
                missing.push(t.name.clone());
            }
        }
        let skills = if job_terms.is_empty() {
            NEUTRAL
        } else {
            let denom = job_terms.len().min(12) as f64;
            ((matched.len() as f64).min(denom) / denom * 100.0).round() as u32
        };

        let years_required = years_required(description);
        let experience = experience_factor(years_required, self.years);

        let job_tokens = title_tokens(title);
        let role = if self.targets.is_empty() || job_tokens.is_empty() {
            NEUTRAL
        } else {
            self.targets
                .iter()
                .map(|t| {
                    let common = t.intersection(&job_tokens).count() as f64;
                    (common / t.len() as f64 * 100.0).round() as u32
                })
                .max()
                .unwrap_or(0)
        };

        let job_level = title_seniority(title);
        let seniority = match job_level {
            _ if self.seniority.is_empty() => NEUTRAL,
            None => NEUTRAL,
            Some(l) if self.seniority.contains(&l) => 100,
            Some(l) => match self
                .seniority
                .iter()
                .map(|s| (rank(*s) - rank(l)).abs())
                .min()
            {
                Some(0) => 80,
                Some(1) => 60,
                _ => 20,
            },
        };

        let score = self.weights.score(skills, experience, role, seniority);

        let words = word_count(description);
        missing.truncate(10);
        MatchBreakdown {
            score,
            skills,
            experience,
            role,
            seniority,
            matched_skills: matched
                .iter()
                .map(|t| MatchedSkill {
                    name: t.name.clone(),
                    lines: t.lines.clone(),
                })
                .collect(),
            missing_skills: missing,
            years_required,
            years_on_profile: self.years,
            job_seniority: job_level.map(|s| seniority_label(s).to_string()),
            word_count: words as u32,
            short_description: words < SHORT_DESCRIPTION_WORDS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Role, Skill};

    fn profile() -> Profile {
        Profile {
            roles: vec![
                Role {
                    title: "Senior Frontend Engineer".into(),
                    dates: "Mar 2021 – Present".into(),
                    ..Default::default()
                },
                Role {
                    title: "Frontend Developer".into(),
                    dates: "2018 – 2021".into(),
                    ..Default::default()
                },
            ],
            skills: ["React", "TypeScript", "GraphQL", "Go"]
                .iter()
                .enumerate()
                .map(|(i, n)| Skill {
                    id: format!("s{i}"),
                    name: n.to_string(),
                    lines: vec![8],
                })
                .collect(),
            ..Default::default()
        }
    }

    fn search() -> SearchSettings {
        SearchSettings {
            titles: vec!["Senior Frontend Engineer".into()],
            seniority: vec![Seniority::Senior, Seniority::Staff],
            must_have: vec!["React".into()],
            avoid: vec!["PHP".into(), "on-site".into()],
            ..Default::default()
        }
    }

    fn run(title: &str, desc: &str) -> MatchBreakdown {
        let ctx = MatchContext::new(&profile(), &search(), Weights::default(), 2026);
        ctx.breakdown(title, desc, &[], &Phrases::new(&format!("{title}\n{desc}")))
    }

    #[test]
    fn strong_match_scores_high() {
        let b = run(
            "Senior Front-End Developer",
            "We use React.js and TypeScript with a GraphQL API. 5+ years of professional frontend experience.",
        );
        assert_eq!(b.skills, 100);
        assert_eq!(b.role, 100);
        assert_eq!(b.seniority, 100);
        assert_eq!(b.years_on_profile, Some(8));
        assert_eq!(b.years_required, Some(5));
        assert_eq!(b.experience, 100);
        assert_eq!(b.score, 100);
        assert_eq!(
            b.matched_skills[0],
            MatchedSkill {
                name: "React".into(),
                lines: vec![8]
            }
        );
        assert!(b.short_description);
    }

    #[test]
    fn weak_match_lists_missing_skills() {
        let b = run(
            "Junior Backend Engineer",
            "Python, Django and PostgreSQL on AWS. 10+ years of backend experience.",
        );
        assert_eq!(b.skills, 0);
        assert_eq!(b.missing_skills, ["Python", "Django", "PostgreSQL", "AWS"]);
        assert_eq!(b.seniority, 20);
        assert_eq!(b.experience, 60);
        assert!(b.score < 40, "{}", b.score);
    }

    #[test]
    fn go_only_counts_as_written() {
        let b = run("Engineer", "Ready to go? We write Go services.");
        assert!(b.matched_skills.iter().any(|s| s.name == "Go"));
        let b = run("Engineer", "Ready to go the extra mile.");
        assert!(!b.matched_skills.iter().any(|s| s.name == "Go"));
    }

    #[test]
    fn keyword_filters() {
        let ctx = MatchContext::new(&profile(), &search(), Weights::default(), 2026);
        let check = |t: &str| ctx.passes_keywords(&Phrases::new(t), &t.to_lowercase());
        assert!(check("React and TypeScript, fully remote"));
        assert!(!check("Vue and TypeScript"));
        assert!(!check("React with a PHP backend"));
        assert!(!check("React. This role is on-site in Berlin."));
    }

    #[test]
    fn reads_seniority_and_years() {
        assert_eq!(
            title_seniority("Sr. Software Engineer"),
            Some(Seniority::Senior)
        );
        assert_eq!(title_seniority("Staff UI Engineer"), Some(Seniority::Staff));
        assert_eq!(
            title_seniority("Engineering Manager - EU"),
            Some(Seniority::Lead)
        );
        assert_eq!(title_seniority("Frontend Engineer"), None);
        assert_eq!(
            years_required("3-5 years of experience with React"),
            Some(3)
        );
        assert_eq!(
            years_required("At least 4 yrs of hands-on Rust experience"),
            Some(4)
        );
        assert_eq!(years_required("Founded 25 years ago"), None);
    }
}

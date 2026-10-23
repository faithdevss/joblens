mod extract;
mod parse;

use crate::{error::AppResult, profile::Profile};

pub struct ParsedResume {
    pub profile: Profile,
    pub lines: Vec<String>,
    pub pages: Option<u32>,
}

/// Reads a PDF or DOCX resume into a draft profile. Runs fully offline.
pub fn read(file_name: &str, bytes: &[u8]) -> AppResult<ParsedResume> {
    let extracted = extract::extract(file_name, bytes)?;
    let lines = parse::to_lines(&extracted.text);
    let profile = parse::parse(&lines);
    Ok(ParsedResume {
        profile,
        lines,
        pages: extracted.pages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parser tuning aid: `RESUME=path/to/cv.pdf cargo test dump_resume -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dump_resume() {
        let path = std::env::var("RESUME").expect("set RESUME to a resume file");
        let bytes = std::fs::read(&path).unwrap();
        let r = read(&path, &bytes).unwrap();
        for (i, l) in r.lines.iter().enumerate() {
            println!("{i:>3} | {l}");
        }
        println!("{}", serde_json::to_string_pretty(&r.profile).unwrap());
    }

    #[test]
    fn pdf_end_to_end() {
        let pdf = extract::tests::make_pdf(&[
            "Sam Rivera",
            "sam@example.com",
            "Experience",
            "Senior Frontend Engineer | Brook | Mar 2021 - Present",
            "- Led the migration of a large React app to TypeScript across four teams.",
            "- Cut bundle size by 40 percent by splitting routes and removing dead code.",
            "Skills",
            "React, TypeScript, Node.js, GraphQL",
        ]);
        let r = read("resume.pdf", &pdf).unwrap();
        assert_eq!(r.pages, Some(1));
        assert_eq!(r.profile.contact.name, "Sam Rivera");
        assert_eq!(r.profile.roles.len(), 1);
        assert_eq!(r.profile.roles[0].company, "Brook");
        assert_eq!(r.profile.roles[0].achievements.len(), 2);
        assert_eq!(r.profile.skills.len(), 4);
    }

    #[test]
    fn docx_end_to_end() {
        let docx = extract::tests::make_docx(&[
            "Sam Rivera",
            "sam@example.com",
            "Experience",
            "Senior Frontend Engineer — Brook\tMar 2021 – Present",
            "- Led the migration of a large React app to TypeScript across four teams.",
            "- Cut bundle size by 40 percent by splitting routes and removing dead code.",
            "Skills",
            "React, TypeScript, Node.js, GraphQL",
        ]);
        let r = read("resume.docx", &docx).unwrap();
        assert_eq!(r.pages, None);
        assert_eq!(r.profile.roles[0].title, "Senior Frontend Engineer");
        assert_eq!(r.profile.roles[0].dates, "Mar 2021 – Present");
        assert_eq!(r.profile.roles[0].achievements.len(), 2);
    }
}

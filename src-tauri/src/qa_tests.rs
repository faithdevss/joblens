//! End-to-end check of the v1 loop on one machine and then another:
//! add a job → gap check → write a letter → honesty check → export → mark applied →
//! back up → restore elsewhere.

use crate::{
    apply::{
        export,
        gap::{self, EditAction},
        writer::{self, DraftKind, DraftLength},
    },
    data, db,
    db::store,
    jobs::{RawJob, refresh},
    profile::{Achievement, Contact, Profile, Role, Skill},
    tracker::{self, AppStatus},
};

fn profile() -> Profile {
    Profile {
        contact: Contact {
            name: "Sam Rivera".into(),
            email: "sam@example.com".into(),
            phone: "+351 912 000 000".into(),
            ..Default::default()
        },
        roles: vec![Role {
            id: "r1".into(),
            title: "Senior Frontend Engineer".into(),
            company: "Brook".into(),
            dates: "2019 – Present".into(),
            achievements: vec![Achievement {
                id: "a1".into(),
                text: "Led the migration of a 200k-line React app to TypeScript.".into(),
                lines: vec![4],
            }],
            lines: vec![3],
            ..Default::default()
        }],
        skills: vec![Skill {
            id: "s1".into(),
            name: "React".into(),
            lines: vec![7],
        }],
        ..Default::default()
    }
}

fn lines() -> Vec<String> {
    [
        "Sam Rivera",
        "sam@example.com | +351 912 000 000",
        "EXPERIENCE",
        "Senior Frontend Engineer, Brook · 2019 – Present",
        "Led the migration of a 200k-line React app to TypeScript.",
        "SKILLS",
        "",
        "React",
        "EDUCATION",
    ]
    .map(String::from)
    .to_vec()
}

#[test]
fn v1_loop_end_to_end() {
    let a = tempfile::tempdir().unwrap();
    let db_path = a.path().join(db::DB_FILE_NAME);
    let conn = db::open(&db_path).unwrap();
    store::save_import(&conn, &profile(), &lines(), "cv.pdf", 1000, Some(1)).unwrap();
    std::fs::create_dir_all(a.path().join("resume")).unwrap();
    std::fs::write(a.path().join("resume/cv.pdf"), b"%PDF-1.4").unwrap();

    // A pasted job is saved and tracked straight away.
    let id = refresh::add_manual(
        &conn,
        &RawJob {
            external_id: "pasted-1".into(),
            title: "Frontend Engineer".into(),
            company: "Acme".into(),
            description:
                "Remote, worldwide. We build with React and TypeScript. 5+ years of experience."
                    .into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(tracker::get(&conn, id).unwrap().status, AppStatus::Saved);

    // Gap check: TypeScript is in the resume text but not the skills list.
    let g = gap::check(&conn, id).unwrap();
    assert!(g.checks.iter().all(|c| c.ok), "{:?}", g.checks);
    let edit = g
        .edits
        .iter()
        .find(|e| e.after == "TypeScript")
        .expect("suggestion")
        .clone();
    gap::decide(
        &conn,
        id,
        &edit.id,
        EditAction::Accept {
            text: "TypeScript".into(),
        },
    )
    .unwrap();

    // Letter from templates is fully backed; a typed claim must be resolved.
    let ctx = writer::Context::load(&conn, id).unwrap();
    let mut d = writer::from_rules(
        &ctx,
        DraftKind::CoverLetter,
        DraftLength::Standard,
        &["a1".into()],
    );
    assert_eq!(d.open_flags(), 0);
    let last = d.paragraphs.len() - 1;
    writer::edit_paragraph(
        &ctx,
        &mut d,
        last,
        "I have shipped 12 apps to the App Store. Thank you for your time.",
    )
    .unwrap();
    assert_eq!(d.open_flags(), 1);
    writer::resolve(&mut d, last, 0, false).unwrap();
    assert_eq!(d.open_flags(), 0);
    writer::save(&conn, id, &mut d).unwrap();
    let text = export::text(&d);
    assert!(text.contains("At Brook, I led the migration"), "{text}");
    assert!(!text.contains("App Store"));
    assert!(!export::pdf(&d).is_empty());
    assert!(!export::docx(&d).unwrap().is_empty());

    // Applying records what was sent.
    let app = tracker::mark_applied(&conn, id, "2026-10-25").unwrap();
    assert_eq!(app.status, AppStatus::Applied);
    assert_eq!(app.follow_up_at.as_deref(), Some("2026-11-01"));
    assert!(app.letter.is_some() && app.resume_version.is_some());

    // Back up, restore on a fresh install, and everything is there.
    let file = a.path().join("backup.zip");
    data::backup(&conn, a.path(), &file, "1.0.0").unwrap();
    let b = tempfile::tempdir().unwrap();
    let b_db = b.path().join(db::DB_FILE_NAME);
    let mut other = db::open(&b_db).unwrap();
    let r = data::read_backup(&std::fs::read(&file).unwrap(), &b.path().join("restore")).unwrap();
    data::swap(&mut other, b.path(), &b_db, Some(r)).unwrap();
    let restored = tracker::list(&other).unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].status, AppStatus::Applied);
    let p = store::get_profile(&other).unwrap().unwrap();
    assert!(p.profile.skills.iter().any(|s| s.name == "TypeScript"));
    assert_eq!(
        writer::load(&other, id, DraftKind::CoverLetter).unwrap(),
        Some(d)
    );
    assert!(b.path().join("resume/cv.pdf").exists());
}

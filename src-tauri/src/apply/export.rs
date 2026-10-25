//! Turning a draft into something to send: plain text, Word (.docx) or PDF, all made
//! on this computer.

use std::io::{Cursor, Write};

use super::writer::{Draft, DraftKind};
use crate::error::{AppError, AppResult};

/// The paragraphs as shown, one string each; empty paragraphs are left out.
fn paragraphs(d: &Draft) -> Vec<String> {
    d.paragraphs
        .iter()
        .map(|p| {
            p.shown()
                .sentences
                .iter()
                .map(|s| s.text.trim())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|p| !p.trim().is_empty())
        .collect()
}

/// Blocks of text in order: subject (emails), greeting, paragraphs, sign-off.
/// Lines inside a block are separated by `\n`.
fn blocks(d: &Draft) -> Vec<String> {
    let mut out = Vec::new();
    if d.kind == DraftKind::Email && !d.subject.trim().is_empty() {
        out.push(format!("Subject: {}", d.subject.trim()));
    }
    if !d.greeting.trim().is_empty() {
        out.push(d.greeting.trim().to_string());
    }
    out.extend(paragraphs(d));
    if !d.sign_off.trim().is_empty() {
        out.push(d.sign_off.trim().to_string());
    }
    out
}

pub fn text(d: &Draft) -> String {
    blocks(d).join("\n\n")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A minimal Word document: one paragraph per block, Calibri 11pt, 1" margins.
pub fn docx(d: &Draft) -> AppResult<Vec<u8>> {
    let mut body = String::new();
    for block in blocks(d) {
        body.push_str(r#"<w:p><w:pPr><w:spacing w:after="200"/></w:pPr>"#);
        for (i, line) in block.lines().enumerate() {
            if i > 0 {
                body.push_str("<w:r><w:br/></w:r>");
            }
            body.push_str(&format!(
                r#"<w:r><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:cs="Calibri"/><w:sz w:val="22"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
                xml_escape(line)
            ));
        }
        body.push_str("</w:p>");
    }
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let files = [
        (
            "[Content_Types].xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_string(),
        ),
        (
            "_rels/.rels",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_string(),
        ),
        ("word/document.xml", document),
    ];
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, content) in files {
        zip.start_file(name, opts)
            .map_err(|e| AppError::Other(e.to_string()))?;
        zip.write_all(content.as_bytes())?;
    }
    Ok(zip
        .finish()
        .map_err(|e| AppError::Other(e.to_string()))?
        .into_inner())
}

// ---- PDF ----

const PAGE_W: f64 = 595.0; // A4
const PAGE_H: f64 = 842.0;
const MARGIN: f64 = 72.0;
const FONT_SIZE: f64 = 11.0;
const LEADING: f64 = 15.0;
const BLOCK_GAP: f64 = 10.0;

/// Helvetica widths (per 1000 em) for ASCII 32–126, from the standard AFM.
const HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

/// The WinAnsi byte for a character; anything the built-in font can't show becomes "?".
fn win_ansi(c: char) -> u8 {
    match c {
        ' '..='~' => c as u8,
        '\u{a0}'..='\u{ff}' => c as u32 as u8,
        '€' => 0x80,
        '‚' => 0x82,
        '„' => 0x84,
        '…' => 0x85,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '™' => 0x99,
        _ => b'?',
    }
}

fn width(b: u8) -> f64 {
    let w = match b {
        32..=126 => HELVETICA[(b - 32) as usize],
        0x91 | 0x92 | 0x82 => 222,
        0x93 | 0x94 | 0x84 => 333,
        0x95 => 350,
        0x96 => 556,
        0x97 | 0x85 | 0x99 => 1000,
        _ => 556,
    };
    f64::from(w) * FONT_SIZE / 1000.0
}

/// Wraps one line of text to the page width.
fn wrap(line: &[u8], max: f64) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let mut cur_w = 0.0;
    for word in line.split(|b| *b == b' ').filter(|w| !w.is_empty()) {
        let w: f64 = word.iter().map(|b| width(*b)).sum();
        let space = if cur.is_empty() { 0.0 } else { width(b' ') };
        if !cur.is_empty() && cur_w + space + w > max {
            out.push(std::mem::take(&mut cur));
            cur_w = 0.0;
        }
        if !cur.is_empty() {
            cur.push(b' ');
            cur_w += width(b' ');
        }
        cur.extend_from_slice(word);
        cur_w += w;
    }
    out.push(cur);
    out
}

fn pdf_string(line: &[u8]) -> Vec<u8> {
    let mut out = vec![b'('];
    for b in line {
        if matches!(b, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        out.push(*b);
    }
    out.push(b')');
    out
}

/// A plain A4 PDF in Helvetica. Characters outside Western European text show as "?".
pub fn pdf(d: &Draft) -> Vec<u8> {
    // Lay out lines into pages: each entry is (y, bytes).
    let mut pages: Vec<Vec<(f64, Vec<u8>)>> = vec![Vec::new()];
    let mut y = PAGE_H - MARGIN;
    for block in blocks(d) {
        for line in block.lines() {
            let bytes: Vec<u8> = line.chars().map(win_ansi).collect();
            for l in wrap(&bytes, PAGE_W - 2.0 * MARGIN) {
                if y < MARGIN {
                    pages.push(Vec::new());
                    y = PAGE_H - MARGIN;
                }
                pages.last_mut().expect("a page").push((y, l));
                y -= LEADING;
            }
        }
        y -= BLOCK_GAP;
    }

    // Objects: 1 catalog, 2 pages, 3 font, then a page + content pair per page.
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        Vec::new(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    ];
    let mut kids = Vec::new();
    for lines in &pages {
        let mut stream = Vec::new();
        for (y, l) in lines {
            stream
                .extend_from_slice(format!("BT /F1 {FONT_SIZE} Tf {MARGIN} {y:.1} Td ").as_bytes());
            stream.extend(pdf_string(l));
            stream.extend_from_slice(b" Tj ET\n");
        }
        let page_id = objects.len() + 1;
        kids.push(format!("{page_id} 0 R"));
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                page_id + 1
            )
            .into_bytes(),
        );
        let mut content = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
        content.extend(stream);
        content.extend_from_slice(b"\nendstream");
        objects.push(content);
    }
    objects[1] = format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.join(" "),
        kids.len()
    )
    .into_bytes();

    let mut out = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// A file name like "Cover letter - Acme.pdf", safe on every OS.
pub fn file_name(d: &Draft, company: &str, ext: &str) -> String {
    let company: String = company
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '.' | '&') {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let company: String = company.chars().take(60).collect();
    if company.is_empty() {
        format!("{}.{ext}", d.kind.label())
    } else {
        format!("{} - {company}.{ext}", d.kind.label())
    }
}

/// `dir/name`, or `dir/name (2)` etc. when that exists.
pub fn free_path(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}).{ext}")))
        .find(|p| !p.exists())
        .expect("a free name")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::writer::{self, Context, DraftLength};
    use crate::profile::{Achievement, Contact, Profile, Role};

    fn draft(kind: DraftKind) -> Draft {
        let profile = Profile {
            contact: Contact {
                name: "Sam Rivera".into(),
                ..Default::default()
            },
            roles: vec![Role {
                id: "r1".into(),
                title: "Engineer".into(),
                company: "Brook".into(),
                dates: "2020 – 2024".into(),
                achievements: vec![Achievement {
                    id: "a1".into(),
                    text: "Built the café’s “order (beta)” flow — 40% faster".into(),
                    lines: vec![1],
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let ctx = Context::new(
            "Developer".into(),
            "Acme & Co".into(),
            String::new(),
            profile,
            &[],
        );
        writer::from_rules(&ctx, kind, DraftLength::Short, &["a1".into()])
    }

    #[test]
    fn text_has_every_part() {
        let t = text(&draft(DraftKind::Email));
        assert!(
            t.starts_with(
                "Subject: Application: Developer — Sam Rivera\n\nHi [hiring manager name],\n\n"
            ),
            "{t}"
        );
        assert!(t.ends_with("Best,\nSam Rivera"), "{t}");
        assert!(t.contains("40% faster"), "{t}");
    }

    #[test]
    fn docx_reads_back() {
        let d = draft(DraftKind::CoverLetter);
        let bytes = docx(&d).unwrap();
        let back = crate::resume::read("letter.docx", &bytes).unwrap();
        let all = back.lines.join("\n");
        assert!(all.contains("Dear [hiring manager name],"), "{all}");
        assert!(all.contains("“order (beta)” flow"), "{all}");
    }

    #[test]
    fn pdf_reads_back() {
        let d = draft(DraftKind::CoverLetter);
        let bytes = pdf(&d);
        let doc = lopdf::Document::load_mem(&bytes).expect("valid pdf");
        assert_eq!(doc.get_pages().len(), 1);
        let back = pdf_extract::extract_text_from_mem(&bytes).unwrap();
        assert!(back.contains("hiring manager name"), "{back}");
        assert!(back.contains("Sam Rivera"), "{back}");
    }

    #[test]
    fn long_text_wraps_onto_pages() {
        let mut d = draft(DraftKind::CoverLetter);
        let p = d.paragraphs[0].clone();
        for _ in 0..60 {
            d.paragraphs.push(p.clone());
        }
        let bytes = pdf(&d);
        let doc = lopdf::Document::load_mem(&bytes).unwrap();
        assert!(doc.get_pages().len() > 1);
        let words = "aaaa ".repeat(80);
        let w = wrap(words.as_bytes(), PAGE_W - 2.0 * MARGIN);
        assert!(w.len() > 1);
        assert!(
            w.iter()
                .all(|l| l.iter().map(|b| width(*b)).sum::<f64>() <= PAGE_W - 2.0 * MARGIN)
        );
    }

    #[test]
    fn file_names_are_safe_and_unique() {
        let d = draft(DraftKind::CoverLetter);
        assert_eq!(
            file_name(&d, "Acme/Co: Inc", "pdf"),
            "Cover letter - Acme Co Inc.pdf"
        );
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.pdf"), b"x").unwrap();
        assert_eq!(free_path(dir.path(), "a.pdf"), dir.path().join("a (2).pdf"));
    }
}

//! Resume file → plain text. PDF uses the text layer only (no OCR); DOCX reads
//! `word/document.xml` directly.

use std::io::{Cursor, Read};

use quick_xml::{Reader, events::Event};

use crate::error::{AppError, AppResult};

const MAX_FILE_BYTES: usize = 15 * 1024 * 1024;

#[derive(Debug, PartialEq)]
pub struct Extracted {
    pub text: String,
    pub pages: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Pdf,
    Docx,
}

fn detect(file_name: &str, bytes: &[u8]) -> AppResult<Kind> {
    let lower = file_name.to_lowercase();
    if bytes.starts_with(b"%PDF") {
        return Ok(Kind::Pdf);
    }
    if bytes.starts_with(b"PK") && !lower.ends_with(".pdf") {
        return Ok(Kind::Docx);
    }
    if lower.ends_with(".doc") || bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        return Err(AppError::Other(
            "Old Word (.doc) files aren't supported. Save it as .docx or PDF and try again.".into(),
        ));
    }
    Err(AppError::Other(
        "This isn't a PDF or Word (.docx) file.".into(),
    ))
}

pub fn extract(file_name: &str, bytes: &[u8]) -> AppResult<Extracted> {
    if bytes.is_empty() {
        return Err(AppError::Other("This file is empty.".into()));
    }
    if bytes.len() > MAX_FILE_BYTES {
        return Err(AppError::Other(
            "This file is over 15 MB. A resume should be much smaller — export it again without images.".into(),
        ));
    }
    let out = match detect(file_name, bytes)? {
        Kind::Pdf => extract_pdf(bytes)?,
        Kind::Docx => extract_docx(bytes)?,
    };
    if out.text.split_whitespace().count() < 20 {
        return Err(AppError::Other(match out.pages {
            Some(_) => {
                "This PDF has almost no text we can read. It looks scanned or saved as an image. \
                        Export your resume as a text PDF or a .docx and try again."
                    .into()
            }
            None => "This Word file has almost no text in it.".into(),
        }));
    }
    Ok(out)
}

fn extract_pdf(bytes: &[u8]) -> AppResult<Extracted> {
    // pdf-extract can panic on malformed files; contain it so one bad PDF can't take
    // the app down. Requires `panic = "unwind"` in the release profile.
    let result = std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem_by_pages(bytes));
    let pages = match result {
        Ok(Ok(pages)) => pages,
        Ok(Err(e)) => {
            return Err(AppError::Other(format!(
                "Couldn't read this PDF ({e}). If it's password-protected, remove the password and try again."
            )));
        }
        Err(_) => {
            return Err(AppError::Other(
                "Couldn't read this PDF. Try exporting it again, or upload a .docx.".into(),
            ));
        }
    };
    Ok(Extracted {
        pages: Some(pages.len() as u32),
        text: pages.join("\n"),
    })
}

fn extract_docx(bytes: &[u8]) -> AppResult<Extracted> {
    let bad = |_| AppError::Other("Couldn't open this Word file. Is it a .docx?".into());
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(bad)?;
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .map_err(bad)?
        .read_to_string(&mut xml)?;
    Ok(Extracted {
        text: docx_xml_to_text(&xml)?,
        pages: None,
    })
}

/// Paragraphs become lines; list paragraphs get a "• " prefix so the parser can see bullets.
fn docx_xml_to_text(xml: &str) -> AppResult<String> {
    let mut reader = Reader::from_str(xml);
    let mut out = String::new();
    let mut para = String::new();
    let mut is_list = false;
    let mut in_text = false;
    loop {
        let event = reader
            .read_event()
            .map_err(|e| AppError::Other(format!("Couldn't read this Word file: {e}")))?;
        match event {
            Event::Start(e) | Event::Empty(e) if e.name().as_ref() == "w:numPr" => is_list = true,
            Event::Start(e) if e.name().as_ref() == "w:t" => in_text = true,
            Event::End(e) if e.name().as_ref() == "w:t" => in_text = false,
            Event::Empty(e) if e.name().as_ref() == "w:tab" => para.push('\t'),
            Event::Empty(e) if matches!(e.name().as_ref(), "w:br" | "w:cr") => para.push('\n'),
            Event::Text(t) if in_text => para.push_str(&t.xml10_content()),
            Event::GeneralRef(r) if in_text => {
                // Entity and character references (&amp;, &#233;) arrive as separate events.
                if let Ok(Some(c)) = r.resolve_char_ref() {
                    para.push(c);
                } else {
                    para.push_str(match r.into_inner().as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => "",
                    });
                }
            }
            Event::End(e) if e.name().as_ref() == "w:p" => {
                let text = para.trim_start();
                if is_list && !text.is_empty() && !text.starts_with(['•', '-', '*', '–', '·'])
                {
                    out.push_str("• ");
                }
                out.push_str(&para);
                out.push('\n');
                para.clear();
                is_list = false;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::Write;

    use super::*;

    const RESUME: &[&str] = &[
        "Sam Rivera",
        "Senior Frontend Engineer",
        "sam@example.com | Lisbon, Portugal",
        "Experience",
        "Senior Frontend Engineer - Brook  Mar 2021 - Present",
        "Led the migration of a large React app to TypeScript across four teams of engineers.",
        "Cut bundle size by 40 percent by splitting routes and removing unused dependencies.",
        "Skills",
        "React, TypeScript, Node.js, GraphQL",
    ];

    /// Minimal one-page PDF with one text line per entry.
    pub(crate) fn make_pdf(lines: &[&str]) -> Vec<u8> {
        use lopdf::{
            Document, Object, Stream,
            content::{Content, Operation},
            dictionary,
        };
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let resources_id =
            doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font_id } });
        let mut ops = vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 11.into()]),
            Operation::new("TL", vec![16.into()]),
            Operation::new("Td", vec![50.into(), 780.into()]),
        ];
        for line in lines {
            ops.push(Operation::new("Tj", vec![Object::string_literal(*line)]));
            ops.push(Operation::new("T*", vec![]));
        }
        ops.push(Operation::new("ET", vec![]));
        let content = Content { operations: ops };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => content_id,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    /// Minimal .docx; lines starting with "- " become list paragraphs.
    pub(crate) fn make_docx(lines: &[&str]) -> Vec<u8> {
        let body: String = lines
            .iter()
            .map(|l| match l.strip_prefix("- ") {
                Some(item) => format!(
                    r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>{item}</w:t></w:r></w:p>"#
                ),
                None => format!(r#"<w:p><w:r><w:t xml:space="preserve">{l}</w:t></w:r></w:p>"#),
            })
            .collect();
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#
        );
        let mut buf = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("word/document.xml", opts).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
        zip.finish().unwrap();
        buf.into_inner()
    }

    #[test]
    fn reads_pdf_text_layer() {
        let out = extract("resume.pdf", &make_pdf(RESUME)).unwrap();
        assert_eq!(out.pages, Some(1));
        assert!(out.text.contains("Sam Rivera"));
        assert!(out.text.contains("React, TypeScript"));
    }

    #[test]
    fn rejects_pdf_without_text() {
        let err = extract("scan.pdf", &make_pdf(&[])).unwrap_err();
        assert!(err.to_string().contains("looks scanned"), "{err}");
    }

    #[test]
    fn rejects_garbage_pdf_without_panicking() {
        let err = extract("x.pdf", b"%PDF-1.5 not really a pdf").unwrap_err();
        assert!(err.to_string().contains("Couldn't read this PDF"), "{err}");
    }

    #[test]
    fn reads_docx_paragraphs_and_lists() {
        let out = extract(
            "resume.docx",
            &make_docx(&[
                "Sam Rivera &amp; Co",
                "Experience",
                "- Led the migration of a large React app to TypeScript across four teams.",
                "Skills",
                "React, TypeScript, Node.js, GraphQL, Rust, Go, Python, SQL, CSS, HTML",
            ]),
        )
        .unwrap();
        assert_eq!(out.pages, None);
        assert!(out.text.contains("Sam Rivera & Co\n"));
        assert!(out.text.contains("• Led the migration"));
    }

    #[test]
    fn rejects_old_doc_and_unknown_files() {
        let doc = extract("cv.doc", &[0xD0, 0xCF, 0x11, 0xE0, 0, 0]).unwrap_err();
        assert!(doc.to_string().contains(".doc"));
        let txt = extract("cv.txt", b"hello").unwrap_err();
        assert!(txt.to_string().contains("isn't a PDF"));
    }
}

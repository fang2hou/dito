//! The text of a file the user chose: Markdown and plain text as they are, Word and PowerPoint from the XML inside
//! their archives.

use std::io::{Cursor, Read};
use std::path::Path;

use quick_xml::Reader;
use quick_xml::events::Event;
use zip::ZipArchive;

/// The file name extensions [`read_source`] takes, lower case.
pub const EXTENSIONS: &[&str] = &["md", "markdown", "txt", "docx", "pptx"];

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("unsupported file type")]
    Unsupported,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("the file could not be read as text: {0}")]
    Unreadable(String),
}

fn unreadable(e: impl std::fmt::Display) -> SourceError {
    SourceError::Unreadable(e.to_string())
}

pub fn read_source(path: &Path) -> Result<String, SourceError> {
    let extension = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    match extension.as_str() {
        "md" | "markdown" | "txt" => String::from_utf8(std::fs::read(path)?).map_err(unreadable),
        "docx" => docx_text(std::fs::read(path)?),
        "pptx" => pptx_text(std::fs::read(path)?),
        _ => Err(SourceError::Unsupported),
    }
}

fn docx_text(bytes: Vec<u8>) -> Result<String, SourceError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(unreadable)?;
    xml_text(&entry(&mut archive, "word/document.xml")?, b"w:t", b"w:p")
}

/// Slides in their numbered order, a blank line between two of them.
fn pptx_text(bytes: Vec<u8>) -> Result<String, SourceError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(unreadable)?;
    let mut slides: Vec<(u32, String)> = archive
        .file_names()
        .filter_map(Result::ok)
        .filter_map(|name| {
            let number = name.strip_prefix("ppt/slides/slide")?.strip_suffix(".xml")?.parse().ok()?;
            Some((number, name.into_owned()))
        })
        .collect();
    if slides.is_empty() {
        return Err(unreadable("no slides"));
    }
    slides.sort_unstable();
    let mut out = String::new();
    for (_, name) in slides {
        out.push_str(&xml_text(&entry(&mut archive, &name)?, b"a:t", b"a:p")?);
        out.push('\n');
    }
    Ok(out)
}

fn entry(archive: &mut ZipArchive<Cursor<Vec<u8>>>, name: &str) -> Result<Vec<u8>, SourceError> {
    let mut xml = Vec::new();
    archive.by_name(name).map_err(unreadable)?.read_to_end(&mut xml)?;
    Ok(xml)
}

/// The character data of every `text` element, with a line break where a `paragraph` element ends.
fn xml_text(xml: &[u8], text: &[u8], paragraph: &[u8]) -> Result<String, SourceError> {
    let mut reader = Reader::from_reader(xml);
    let mut out = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event().map_err(unreadable)? {
            Event::Start(e) if e.name().as_ref() == text => in_text = true,
            Event::End(e) if e.name().as_ref() == text => in_text = false,
            Event::End(e) if e.name().as_ref() == paragraph => out.push('\n'),
            Event::Text(t) if in_text => out.push_str(&t.decode().map_err(unreadable)?),
            Event::GeneralRef(r) if in_text => {
                if let Some(c) = r.resolve_char_ref().map_err(unreadable)? {
                    out.push(c);
                } else {
                    match &*r.decode().map_err(unreadable)? {
                        "amp" => out.push('&'),
                        "lt" => out.push('<'),
                        "gt" => out.push('>'),
                        "quot" => out.push('"'),
                        "apos" => out.push('\''),
                        _ => {}
                    }
                }
            }
            Event::Eof => return Ok(out),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::*;

    fn archive(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in entries {
            writer.start_file(*name, SimpleFileOptions::default()).expect("start an entry");
            writer.write_all(body.as_bytes()).expect("write an entry");
        }
        writer.finish().expect("finish the archive").into_inner()
    }

    #[test]
    fn a_word_document_gives_its_paragraphs_with_split_runs_joined_and_entities_resolved() {
        let xml = "<w:document><w:body>\
            <w:p><w:r><w:t>Fabri</w:t></w:r><w:r><w:t xml:space=\"preserve\">kam Cloud &amp; Co</w:t></w:r></w:p>\
            <w:p><w:r><w:instrText>HYPERLINK</w:instrText><w:t>second</w:t></w:r></w:p>\
            </w:body></w:document>";
        let text = docx_text(archive(&[("word/document.xml", xml)])).expect("a readable document");
        assert_eq!(text, "Fabrikam Cloud & Co\nsecond\n");
    }

    #[test]
    fn a_presentation_gives_its_slides_in_numbered_order() {
        let slide = |text: &str| format!("<p:sld><a:p><a:r><a:t>{text}</a:t></a:r></a:p></p:sld>");
        let bytes = archive(&[
            ("ppt/slides/slide10.xml", &slide("ten")),
            ("ppt/slides/slide2.xml", &slide("two")),
            ("ppt/slides/_rels/slide2.xml.rels", "<Relationships/>"),
            ("ppt/slides/slide1.xml", &slide("one")),
        ]);
        assert_eq!(pptx_text(bytes).expect("a readable presentation"), "one\n\ntwo\n\nten\n\n");
    }

    #[test]
    fn a_file_that_is_not_what_its_extension_says_is_an_error_not_empty_text() {
        assert!(matches!(docx_text(b"plain text".to_vec()), Err(SourceError::Unreadable(_))));
        assert!(matches!(
            pptx_text(archive(&[("word/document.xml", "<w:document/>")])),
            Err(SourceError::Unreadable(_))
        ));
        assert!(matches!(read_source(Path::new("notes.pdf")), Err(SourceError::Unsupported)));
    }
}

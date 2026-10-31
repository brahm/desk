// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The converter end to end: an ODT made here becomes a PDF and an ODT.
//! The PDF is read back with the Poppler tools, which CI installs.

use std::path::{Path, PathBuf};
use std::process::Command;

use desk_model::{Document, Metadata, Op, Paragraph, Run};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("desk-convert-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn sample_odt(dir: &Path) -> PathBuf {
    let mut document = Document::new_word_processing();
    let style = document.styles().default_paragraph();
    let first = document.body().get(0).unwrap().id();
    for (index, text) in ["Primeiro parágrafo.", "Segundo parágrafo."]
        .iter()
        .enumerate()
    {
        let id = document.new_id();
        let paragraph =
            Paragraph::from_runs(id, style, Default::default(), [Run::new(text)]).unwrap();
        document
            .apply(Op::InsertParagraph {
                index: index + 1,
                paragraph,
            })
            .unwrap();
    }
    document
        .apply(Op::RemoveParagraph { paragraph: first })
        .unwrap();
    document
        .apply(Op::SetMetadata {
            metadata: Metadata {
                title: "Conversão".into(),
                ..Metadata::default()
            },
        })
        .unwrap();
    let path = dir.join("in.odt");
    std::fs::write(&path, desk_filters::write_odt(&document).unwrap()).unwrap();
    path
}

fn convert(input: &Path, output: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_desk-convert"))
        .arg(input)
        .arg(output)
        .output()
        .unwrap()
}

#[test]
fn odt_becomes_pdf() {
    let dir = scratch("pdf");
    let input = sample_odt(&dir);
    let output = dir.join("out.pdf");
    let run = convert(&input, &output);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let text = Command::new("pdftotext")
        .arg(&output)
        .arg("-")
        .output()
        .expect("pdftotext is needed for this test (poppler-utils)");
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(text.contains("Primeiro parágrafo."), "{text}");
    assert!(text.contains("Segundo parágrafo."), "{text}");
    let info = Command::new("pdfinfo").arg(&output).output().unwrap();
    let info = String::from_utf8(info.stdout).unwrap();
    assert!(info.contains("Title:           Conversão"), "{info}");
    // A4, the page size of a new document.
    assert!(info.contains("595 x 842 pts"), "{info}");
}

#[test]
fn odt_becomes_odt() {
    let dir = scratch("odt");
    let input = sample_odt(&dir);
    let output = dir.join("out.odt");
    let run = convert(&input, &output);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let document = desk_filters::read_odt(&std::fs::read(&output).unwrap()).unwrap();
    let texts: Vec<_> = document.body().iter().map(|p| p.text()).collect();
    assert_eq!(texts, ["Primeiro parágrafo.", "Segundo parágrafo."]);
}

#[test]
fn errors_are_reported() {
    let dir = scratch("errors");
    let missing = convert(&dir.join("missing.odt"), &dir.join("out.pdf"));
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("cannot read"));

    let input = sample_odt(&dir);
    let unknown = convert(&input, &dir.join("out.docx"));
    assert!(!unknown.status.success());

    let garbage = dir.join("garbage.odt");
    std::fs::write(&garbage, b"not a zip").unwrap();
    let broken = convert(&garbage, &dir.join("out.pdf"));
    assert!(!broken.status.success());
    assert!(String::from_utf8_lossy(&broken.stderr).contains("zip"));
}

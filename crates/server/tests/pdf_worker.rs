//! The real binary as the PDF worker: a file that hurts the parser must hurt only the child.
#![allow(clippy::unwrap_used)]

use std::time::{Duration, Instant};

use lopdf::{Document, Object, Stream, dictionary};
use opensign_pdf::{Limits, PdfError, Probe, Stamp, StampContent};

fn worker() {
    opensign_pdf::enable_worker(env!("CARGO_BIN_EXE_opensign").into()).unwrap();
}

fn sample() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
    let content = doc.add_object(Stream::new(dictionary! {}, b"BT /F1 24 Tf 72 720 Td (Agreement {{Name;type=text}}) Tj ET".to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()], "Contents" => content,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    doc.objects.insert(pages, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

#[test]
fn every_operation_gives_the_same_answer_through_the_worker() {
    worker();
    let clean = opensign_pdf::sanitize(&sample(), Limits::default()).unwrap();
    assert_eq!(clean.pages.len(), 1);
    assert!(!clean.pdf.is_empty());
    assert_eq!(opensign_pdf::page_sizes(&clean.pdf).unwrap().len(), 1);
    assert!(opensign_pdf::extract_text(&clean.pdf).unwrap()[0].contains("Agreement"));
    assert!(opensign_pdf::hidden_text(&clean.pdf).unwrap().is_empty());
    assert_eq!(opensign_pdf::page_text(&clean.pdf).unwrap().len(), 1);
    opensign_pdf::detect_fields(&clean.pdf).unwrap();
    let scan = opensign_pdf::take_text_tags(&clean.pdf).unwrap();
    assert_eq!(scan.tags.len(), 1, "{:?}", scan.problems);
    assert!(!scan.pdf.is_empty());
    let merged = opensign_pdf::merge(&[clean.pdf.clone(), clean.pdf.clone()]).unwrap();
    assert_eq!(opensign_pdf::page_sizes(&merged).unwrap().len(), 2);
    let stamp = Stamp { page: 0, x: 0.1, y: 0.1, width: 0.2, height: 0.05, content: StampContent::Text("Signed".into()) };
    let stamped = opensign_pdf::stamp(&clean.pdf, &[stamp]).unwrap();

    let identity = opensign_pdf::generate_seal_identity("Acme AB", "SE", 1).unwrap();
    let options = opensign_pdf::SealOptions {
        name: "Acme".into(),
        reason: "Signed".into(),
        location: String::new(),
        signing_time: chrono::Utc::now(),
    };
    let sealed = opensign_pdf::seal(&stamped, &identity, &options, |_| Ok(None)).unwrap();
    let checks = opensign_pdf::verify_signatures(&sealed).unwrap();
    assert!(checks[0].intact && checks[0].kind == "signature", "{checks:?}");
}

#[test]
fn a_refusal_keeps_its_message() {
    worker();
    let error = opensign_pdf::sanitize(b"not a pdf at all", Limits::default()).unwrap_err();
    assert!(matches!(&error, PdfError::Rejected(message) if message == "This is not a PDF file"), "{error:?}");
}

#[test]
fn a_child_that_dies_fails_only_its_request() {
    worker();
    assert!(opensign_pdf::probe(Probe::Abort, Duration::from_secs(10)).is_err());
    opensign_pdf::probe(Probe::Ping, Duration::from_secs(10)).unwrap();
}

#[test]
fn a_child_that_never_answers_is_stopped() {
    worker();
    let started = Instant::now();
    assert!(opensign_pdf::probe(Probe::Spin, Duration::from_secs(1)).is_err());
    assert!(started.elapsed() < Duration::from_secs(5));
    opensign_pdf::probe(Probe::Ping, Duration::from_secs(10)).unwrap();
}

#[cfg(unix)]
#[test]
fn a_child_that_takes_all_the_memory_dies_of_its_own_limit() {
    worker();
    let started = Instant::now();
    assert!(opensign_pdf::probe(Probe::Allocate, Duration::from_secs(60)).is_err());
    assert!(started.elapsed() < Duration::from_secs(50), "the memory limit stops it, not the deadline");
    opensign_pdf::probe(Probe::Ping, Duration::from_secs(10)).unwrap();
}

#[test]
fn a_hostile_file_costs_one_failed_request() {
    worker();
    let entries = 3000;
    let mut header = String::new();
    for n in 0..entries {
        header.push_str(&format!("{} 0 ", 100 + n));
    }
    let first = header.len();
    let mut content = header.into_bytes();
    content.push(b'(');
    content.extend(std::iter::repeat_n(b'a', 8 * 1024 * 1024));
    content.push(b')');

    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    let mut object = |id: u32, body: &[u8], pdf: &mut Vec<u8>| {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    };
    object(1, b"<</Type/Catalog/Pages 2 0 R>>", &mut pdf);
    object(2, b"<</Type/Pages/Kids[3 0 R]/Count 1>>", &mut pdf);
    object(3, b"<</Type/Page/Parent 2 0 R/MediaBox[0 0 600 800]>>", &mut pdf);
    let mut bomb = format!("<</Type/ObjStm/N {entries}/First {first}/Length {}>>\nstream\n", content.len()).into_bytes();
    bomb.extend_from_slice(&content);
    bomb.extend_from_slice(b"\nendstream");
    object(4, &bomb, &mut pdf);
    let start = pdf.len();
    pdf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
    for offset in &offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<</Size 5/Root 1 0 R>>\nstartxref\n{start}\n%%EOF\n").as_bytes());

    let started = Instant::now();
    assert!(opensign_pdf::sanitize(&pdf, Limits::default()).is_err());
    assert!(opensign_pdf::verify_signatures(&pdf).is_err());
    assert!(started.elapsed() < Duration::from_secs(50));
    opensign_pdf::probe(Probe::Ping, Duration::from_secs(10)).unwrap();
}

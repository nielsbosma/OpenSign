#![allow(clippy::unwrap_used)]

use chrono::Utc;
use cms::content_info::{CmsVersion, ContentInfo};
use cms::signed_data::{EncapsulatedContentInfo, SignedData, SignerInfos};
use const_oid::ObjectIdentifier;
use const_oid::db::rfc5911::ID_SIGNED_DATA;
use const_oid::db::rfc5912::ID_SHA_256;
use der::asn1::{Any, GeneralizedTime, Int, OctetString, SetOfVec};
use der::{Decode, Encode, Sequence};
use lopdf::{Document, Object, Stream, StringFormat, dictionary};
use sha2::{Digest, Sha256};
use spki::AlgorithmIdentifierOwned;

use crate::*;

fn sample_pdf(malicious: bool) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
    let content = doc.add_object(Stream::new(dictionary! {}, b"BT /F1 24 Tf 72 720 Td (Agreement) Tj ET".to_vec()));
    let mut page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        "Contents" => content,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    };
    if malicious {
        let js =
            doc.add_object(dictionary! { "S" => "JavaScript", "JS" => Object::String(b"app.alert('x')".to_vec(), StringFormat::Literal) });
        let link = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Link",
            "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
            "A" => dictionary! { "S" => "Launch", "F" => Object::String(b"calc.exe".to_vec(), StringFormat::Literal) },
        });
        page.set("AA", dictionary! { "O" => js });
        page.set("Annots", vec![Object::Reference(link)]);
    }
    let page_id = doc.add_object(page);
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 }));
    let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => pages_id };
    if malicious {
        let js = doc.add_object(
            dictionary! { "S" => "JavaScript", "JS" => Object::String(b"this.exportDataObject()".to_vec(), StringFormat::Literal) },
        );
        let file = doc.add_object(Stream::new(dictionary! { "Type" => "EmbeddedFile" }, b"MZ payload".to_vec()));
        let spec = doc.add_object(dictionary! { "Type" => "Filespec", "F" => Object::String(b"x.exe".to_vec(), StringFormat::Literal), "EF" => dictionary! { "F" => file } });
        catalog.set("OpenAction", js);
        catalog.set("Names", dictionary! { "EmbeddedFiles" => dictionary! { "Names" => vec![Object::String(b"x.exe".to_vec(), StringFormat::Literal), spec.into()] } });
        catalog.set(
            "AcroForm",
            dictionary! { "Fields" => Vec::<Object>::new(), "XFA" => Object::String(b"<xdp/>".to_vec(), StringFormat::Literal) },
        );
    }
    let catalog_id = doc.add_object(catalog);
    doc.trailer.set("Root", catalog_id);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

fn all_objects(pdf: &[u8]) -> String {
    let doc = Document::load_mem(pdf).unwrap();
    format!("{:?}", doc.objects)
}

fn tiny_png() -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, 4, 2);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[0u8, 0, 0, 255].repeat(8)).unwrap();
    writer.finish().unwrap();
    out
}

fn options() -> SealOptions {
    SealOptions {
        name: "Acme (Seal) \\ Ö".into(),
        reason: "Signed with OpenSign".into(),
        location: "Stockholm".into(),
        signing_time: Utc::now(),
    }
}

#[test]
fn sanitize_strips_active_content() {
    let input = sample_pdf(true);
    assert!(all_objects(&input).contains("JavaScript"));
    let clean = sanitize(&input, Limits::default()).unwrap();
    let objects = all_objects(&clean.pdf);
    for needle in ["JavaScript", "Launch", "EmbeddedFile", "OpenAction", "calc.exe", "AcroForm", "XFA", "MZ payload"] {
        assert!(!objects.contains(needle), "{needle} survived: {objects}");
    }
    let doc = Document::load_mem(&clean.pdf).unwrap();
    let page = *doc.get_pages().values().next().unwrap();
    let content = doc.get_page_content(page);
    assert!(content.windows(9).any(|w| w == b"Agreement"));
    assert_eq!(clean.pages.len(), 1);
    assert!(!clean.removed.is_empty());
}

#[test]
fn sanitize_rejects_non_pdf_and_oversize() {
    assert!(matches!(sanitize(b"<html>hello</html>", Limits::default()), Err(PdfError::Rejected(_))));
    let input = sample_pdf(false);
    let tiny = Limits { max_bytes: 10, ..Limits::default() };
    assert!(matches!(sanitize(&input, tiny), Err(PdfError::Rejected(_))));
}

#[test]
fn stamps_fields_and_rejects_out_of_page() {
    let input = sanitize(&sample_pdf(false), Limits::default()).unwrap().pdf;
    let stamps = vec![
        Stamp { page: 0, x: 0.1, y: 0.1, width: 0.3, height: 0.04, content: StampContent::Text("Anna Ångström (CEO)".into()) },
        Stamp { page: 0, x: 0.1, y: 0.2, width: 0.03, height: 0.02, content: StampContent::Check(true) },
        Stamp {
            page: 0,
            x: 0.5,
            y: 0.8,
            width: 0.35,
            height: 0.1,
            content: StampContent::Signature {
                png: Some(tiny_png()),
                name: "Anna Ångström".into(),
                caption: "Signed 2026-10-28 with BankID".into(),
            },
        },
    ];
    let out = stamp(&input, &stamps).unwrap();
    let doc = Document::load_mem(&out).unwrap();
    assert_eq!(doc.get_pages().len(), 1);
    let bad = Stamp { page: 0, x: 0.9, y: 0.1, width: 0.3, height: 0.1, content: StampContent::Check(false) };
    assert!(stamp(&input, &[bad]).is_err());
    let wrong_page = Stamp { page: 3, x: 0.1, y: 0.1, width: 0.1, height: 0.1, content: StampContent::Check(false) };
    assert!(stamp(&input, &[wrong_page]).is_err());
}

fn report() -> EvidenceReport {
    EvidenceReport {
        document_id: "doc_1".into(),
        title: "Agreement".into(),
        organisation: "Acme AB".into(),
        original_sha256: "ab".repeat(32),
        signed_sha256: "cd".repeat(32),
        completed_at: "2026-10-28 12:00 UTC".into(),
        verify_url: "https://sign.example.com/verify/doc_1".into(),
        parties: vec![EvidenceParty {
            name: "Anna Ångström".into(),
            email: Some("anna@example.com".into()),
            role: "Signer".into(),
            authentication: "Email link".into(),
            signed_at: Some("2026-10-28 11:59 UTC".into()),
            ip: Some("192.0.2.1".into()),
            identity: None,
        }],
        events: (0..80)
            .map(|i| EvidenceEvent { at: format!("2026-10-28 11:{:02} UTC", i % 60), text: format!("Event number {i}") })
            .collect(),
        audit_head: Some("ef".repeat(32)),
    }
}

#[test]
fn evidence_adds_pages_and_attachment() {
    let input = sanitize(&sample_pdf(false), Limits::default()).unwrap().pdf;
    let out = append_evidence(&input, &report()).unwrap();
    let doc = Document::load_mem(&out).unwrap();
    assert!(doc.get_pages().len() >= 3, "80 events should spill onto a second evidence page");
    let names = doc.catalog().unwrap().get(b"Names").unwrap().as_dict().unwrap();
    assert!(names.has(b"EmbeddedFiles"));
}

#[test]
fn seal_and_verify_round_trip() {
    let identity = generate_seal_identity("Acme AB", "SE", 2).unwrap();
    let input = append_evidence(&sanitize(&sample_pdf(false), Limits::default()).unwrap().pdf, &report()).unwrap();
    let sealed = seal(&input, &identity, &options(), |_| Ok(None)).unwrap();
    let checks = verify_signatures(&sealed).unwrap();
    assert_eq!(checks.len(), 1);
    let check = &checks[0];
    assert!(check.is_intact(), "{check:?}");
    assert!(check.signer.as_deref().unwrap().contains("Acme AB Document Seal"));
    assert!(check.timestamp.is_none());
    assert!(sanitize(&sealed, Limits::default()).is_ok(), "a sealed file is still a valid PDF");
}

#[test]
fn verify_detects_tampering_and_appended_updates() {
    let identity = generate_seal_identity("Acme AB", "SE", 2).unwrap();
    let input = sanitize(&sample_pdf(false), Limits::default()).unwrap().pdf;
    let sealed = seal(&input, &identity, &options(), |_| Ok(None)).unwrap();

    let mut tampered = sealed.clone();
    let at = tampered.windows(9).position(|w| w == b"Agreement").unwrap_or(20);
    tampered[at] ^= 0x01;
    if let Ok(checks) = verify_signatures(&tampered) {
        assert!(checks.iter().all(|c| !c.is_intact()), "{checks:?}");
        assert!(checks.iter().all(|c| !c.digest_matches));
    }

    let mut appended = sealed.clone();
    appended.extend_from_slice(b"\n% appended\n");
    let checks = verify_signatures(&appended).unwrap();
    assert!(!checks[0].covers_whole_file);
    assert!(checks[0].digest_matches);

    let other = generate_seal_identity("Mallory", "SE", 1).unwrap();
    let prepared = prepare(&input, &options()).unwrap();
    let digest = prepared.digest;
    let mut signed = sign(&other, &digest).unwrap();
    let last = signed.signature.len() - 1;
    signed.signature[last] ^= 0x01;
    let forged = finish(prepared, &identity, signed, None).unwrap();
    let checks = verify_signatures(&forged).unwrap();
    assert_eq!(checks[0].signature_valid, Some(false));
}

#[derive(Sequence)]
struct Imprint {
    hash_algorithm: AlgorithmIdentifierOwned,
    hashed_message: OctetString,
}

#[derive(Sequence)]
struct TstInfo {
    version: u8,
    policy: ObjectIdentifier,
    message_imprint: Imprint,
    serial_number: Int,
    gen_time: GeneralizedTime,
    nonce: Int,
}

#[derive(Sequence)]
struct Status {
    status: u8,
}

#[derive(Sequence)]
struct Response {
    status: Status,
    token: ContentInfo,
}

/// A response shaped like a real TSA's, without the TSA's signature (which OpenSign leaves to readers).
fn fake_tsa(request: &[u8], data: &[u8], echo_nonce: bool) -> Vec<u8> {
    let fields: Vec<Any> = Vec::from_der(request).unwrap();
    let mut nonce: Int = fields[2].decode_as().unwrap();
    if !echo_nonce {
        nonce = Int::new(&[0x01, 0x02]).unwrap();
    }
    let info = TstInfo {
        version: 1,
        policy: ObjectIdentifier::new_unwrap("1.2.3.4"),
        message_imprint: Imprint {
            hash_algorithm: AlgorithmIdentifierOwned { oid: ID_SHA_256, parameters: None },
            hashed_message: OctetString::new(Sha256::digest(data).to_vec()).unwrap(),
        },
        serial_number: Int::new(&[7]).unwrap(),
        gen_time: GeneralizedTime::from_unix_duration(std::time::Duration::from_secs(1_800_000_000)).unwrap(),
        nonce,
    };
    let signed = SignedData {
        version: CmsVersion::V3,
        digest_algorithms: SetOfVec::try_from(vec![AlgorithmIdentifierOwned { oid: ID_SHA_256, parameters: None }]).unwrap(),
        encap_content_info: EncapsulatedContentInfo {
            econtent_type: ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.1.4"),
            econtent: Some(Any::encode_from(&OctetString::new(info.to_der().unwrap()).unwrap()).unwrap()),
        },
        certificates: None,
        crls: None,
        signer_infos: SignerInfos::try_from(vec![]).unwrap(),
    };
    let token = ContentInfo { content_type: ID_SIGNED_DATA, content: Any::encode_from(&signed).unwrap() };
    Response { status: Status { status: 0 }, token }.to_der().unwrap()
}

#[test]
fn seals_with_a_time_stamp() {
    let identity = generate_seal_identity("Acme AB", "SE", 2).unwrap();
    let input = sanitize(&sample_pdf(false), Limits::default()).unwrap().pdf;
    let nonce = [0xA5u8; 16];
    let sealed = seal(&input, &identity, &options(), |signature| {
        let request = build_timestamp_request(signature, &nonce);
        let response = fake_tsa(&request, signature, true);
        Ok(Some(parse_timestamp_response(&response, signature, &nonce).map_err(|e| PdfError::Timestamp(e.to_string()))?))
    })
    .unwrap();
    let check = &verify_signatures(&sealed).unwrap()[0];
    assert!(check.is_intact(), "{check:?}");
    assert_eq!(check.timestamp.unwrap().timestamp(), 1_800_000_000);

    let request = build_timestamp_request(b"sig", &nonce);
    assert!(parse_timestamp_response(&fake_tsa(&request, b"sig", false), b"sig", &nonce).is_err(), "wrong nonce");
    assert!(parse_timestamp_response(&fake_tsa(&request, b"other", true), b"sig", &nonce).is_err(), "wrong imprint");
}

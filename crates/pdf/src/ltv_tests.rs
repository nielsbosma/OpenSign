#![allow(clippy::unwrap_used)]

use std::str::FromStr;
use std::time::Duration;

use chrono::Utc;
use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::content_info::{CmsVersion, ContentInfo};
use cms::signed_data::{CertificateSet, EncapsulatedContentInfo, SignedData, SignerIdentifier, SignerInfo, SignerInfos};
use const_oid::ObjectIdentifier;
use const_oid::db::rfc5911::{ID_CONTENT_TYPE, ID_MESSAGE_DIGEST, ID_SIGNED_DATA};
use const_oid::db::rfc5912::{ECDSA_WITH_SHA_256, ID_AD_OCSP, ID_KP_TIME_STAMPING, ID_SHA_256};
use const_oid::db::rfc6960::ID_PKIX_OCSP_BASIC;
use der::asn1::{Any, BitString, GeneralizedTime, Ia5String, Int, Null, OctetString, SetOfVec, UtcTime};
use der::{Decode, Encode, Sequence};
use lopdf::{Document, IncrementalDocument, Object, Stream, StringFormat, dictionary};
use sha2::{Digest, Sha256};
use spki::{AlgorithmIdentifierOwned, SubjectPublicKeyInfoOwned};
use x509_cert::Certificate;
use x509_cert::attr::Attribute;
use x509_cert::builder::{Builder, CertificateBuilder, Profile};
use x509_cert::crl::{CertificateList, RevokedCert, TbsCertList};
use x509_cert::ext::pkix::crl::dp::DistributionPoint;
use x509_cert::ext::pkix::name::{DistributionPointName, GeneralName};
use x509_cert::ext::pkix::{AccessDescription, AuthorityInfoAccessSyntax, CrlDistributionPoints, ExtendedKeyUsage};
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::time::{Time, Validity};
use x509_ocsp::{
    BasicOcspResponse, CertStatus, OcspGeneralizedTime, OcspRequest, OcspResponse, OcspResponseStatus, ResponderId, ResponseBytes,
    ResponseData, SingleResponse,
};

use crate::cert::SealKey;
use crate::*;

const ID_CT_TST_INFO: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.1.4");

struct Ca {
    key: SealKey,
    cert: Certificate,
}

fn ecdsa() -> AlgorithmIdentifierOwned {
    AlgorithmIdentifierOwned { oid: ECDSA_WITH_SHA_256, parameters: None }
}

fn root(name: &str) -> Ca {
    let key = SealKey::random();
    let subject = Name::from_str(&format!("CN={name}")).unwrap();
    let spki = SubjectPublicKeyInfoOwned::from_key(key.verifying_key()).unwrap();
    let validity = Validity::from_now(Duration::from_secs(3650 * 86400)).unwrap();
    let cert = CertificateBuilder::new(Profile::Root, SerialNumber::new(&[1]).unwrap(), validity, subject, spki, &key)
        .unwrap()
        .build::<p256::ecdsa::DerSignature>()
        .unwrap();
    Ca { key, cert }
}

/// A time stamp authority certificate issued by `ca`, with OCSP and CRL locations.
fn issue(ca: &Ca, name: &str, serial: u8) -> Ca {
    let key = SealKey::random();
    let subject = Name::from_str(&format!("CN={name}")).unwrap();
    let spki = SubjectPublicKeyInfoOwned::from_key(key.verifying_key()).unwrap();
    let validity = Validity::from_now(Duration::from_secs(365 * 86400)).unwrap();
    let profile =
        Profile::Leaf { issuer: ca.cert.tbs_certificate.subject.clone(), enable_key_agreement: false, enable_key_encipherment: false };
    let mut builder = CertificateBuilder::new(profile, SerialNumber::new(&[serial]).unwrap(), validity, subject, spki, &ca.key).unwrap();
    let uri = |u: &str| GeneralName::UniformResourceIdentifier(Ia5String::new(u).unwrap());
    builder
        .add_extension(&AuthorityInfoAccessSyntax(vec![AccessDescription {
            access_method: ID_AD_OCSP,
            access_location: uri("http://ocsp.test"),
        }]))
        .unwrap();
    builder
        .add_extension(&CrlDistributionPoints(vec![DistributionPoint {
            distribution_point: Some(DistributionPointName::FullName(vec![uri("http://crl.test/ca.crl")])),
            reasons: None,
            crl_issuer: None,
        }]))
        .unwrap();
    builder.add_extension(&ExtendedKeyUsage(vec![ID_KP_TIME_STAMPING])).unwrap();
    let cert = builder.build::<p256::ecdsa::DerSignature>().unwrap();
    Ca { key, cert }
}

/// A certificate fit for anything but time stamping.
fn issue_other(ca: &Ca, name: &str, serial: u8) -> Ca {
    let key = SealKey::random();
    let subject = Name::from_str(&format!("CN={name}")).unwrap();
    let spki = SubjectPublicKeyInfoOwned::from_key(key.verifying_key()).unwrap();
    let validity = Validity::from_now(Duration::from_secs(365 * 86400)).unwrap();
    let profile =
        Profile::Leaf { issuer: ca.cert.tbs_certificate.subject.clone(), enable_key_agreement: false, enable_key_encipherment: false };
    let builder = CertificateBuilder::new(profile, SerialNumber::new(&[serial]).unwrap(), validity, subject, spki, &ca.key).unwrap();
    let cert = builder.build::<p256::ecdsa::DerSignature>().unwrap();
    Ca { key, cert }
}

fn attribute(oid: ObjectIdentifier, value: Any) -> Attribute {
    Attribute { oid, values: SetOfVec::try_from(vec![value]).unwrap() }
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
    #[asn1(optional = "true")]
    nonce: Option<Int>,
}

/// A signed RFC 3161 token over `digest`, as a real TSA returns it.
fn token(tsa: &Ca, chain: &[&Certificate], digest: &[u8; 32]) -> Vec<u8> {
    token_with_nonce(tsa, chain, digest, None)
}

fn token_with_nonce(tsa: &Ca, chain: &[&Certificate], digest: &[u8; 32], nonce: Option<Int>) -> Vec<u8> {
    let info = TstInfo {
        version: 1,
        policy: ObjectIdentifier::new_unwrap("1.2.3.4"),
        message_imprint: Imprint {
            hash_algorithm: AlgorithmIdentifierOwned { oid: ID_SHA_256, parameters: None },
            hashed_message: OctetString::new(digest.to_vec()).unwrap(),
        },
        serial_number: Int::new(&[9]).unwrap(),
        gen_time: GeneralizedTime::from_unix_duration(Duration::from_secs(Utc::now().timestamp() as u64)).unwrap(),
        nonce,
    }
    .to_der()
    .unwrap();
    let attrs = SetOfVec::try_from(vec![
        attribute(ID_CONTENT_TYPE, Any::encode_from(&ID_CT_TST_INFO).unwrap()),
        attribute(ID_MESSAGE_DIGEST, Any::encode_from(&OctetString::new(Sha256::digest(&info).to_vec()).unwrap()).unwrap()),
    ])
    .unwrap();
    let signature = tsa.key.sign(&attrs.to_der().unwrap()).unwrap();
    let cert = &tsa.cert.tbs_certificate;
    let signer = SignerInfo {
        version: CmsVersion::V1,
        sid: SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
            issuer: cert.issuer.clone(),
            serial_number: cert.serial_number.clone(),
        }),
        digest_alg: AlgorithmIdentifierOwned { oid: ID_SHA_256, parameters: None },
        signed_attrs: Some(attrs),
        signature_algorithm: ecdsa(),
        signature: OctetString::new(signature.as_bytes().to_vec()).unwrap(),
        unsigned_attrs: None,
    };
    let signed = SignedData {
        version: CmsVersion::V3,
        digest_algorithms: SetOfVec::try_from(vec![AlgorithmIdentifierOwned { oid: ID_SHA_256, parameters: None }]).unwrap(),
        encap_content_info: EncapsulatedContentInfo {
            econtent_type: ID_CT_TST_INFO,
            econtent: Some(Any::encode_from(&OctetString::new(info).unwrap()).unwrap()),
        },
        certificates: Some(
            CertificateSet::try_from(chain.iter().map(|c| CertificateChoices::Certificate((*c).clone())).collect::<Vec<_>>()).unwrap(),
        ),
        crls: None,
        signer_infos: SignerInfos::try_from(vec![signer]).unwrap(),
    };
    ContentInfo { content_type: ID_SIGNED_DATA, content: Any::encode_from(&signed).unwrap() }.to_der().unwrap()
}

fn sample() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let content = doc.add_object(Stream::new(dictionary! {}, b"BT /F1 24 Tf 72 720 Td (Agreement) Tj ET".to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        "Contents" => content,
    });
    doc.objects.insert(pages, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    sanitize(&out, Limits::default()).unwrap().pdf
}

struct Setup {
    ca: Ca,
    tsa: Ca,
    sealed: Vec<u8>,
}

fn sealed_with_timestamp() -> Setup {
    let ca = root("Test Root CA");
    let tsa = issue(&ca, "Test TSA", 7);
    let identity = generate_seal_identity("Acme AB", "SE", 2).unwrap();
    let options = SealOptions { name: "Acme".into(), reason: "Signed".into(), location: "SE".into(), signing_time: Utc::now() };
    let sealed = seal(&sample(), &identity, &options, |digest| Ok(Some(token(&tsa, &[&tsa.cert, &ca.cert], digest)))).unwrap();
    Setup { ca, tsa, sealed }
}

fn ocsp_response(signer: &Ca, request: &[u8], status: CertStatus, next_update_days: i64) -> Vec<u8> {
    let request = OcspRequest::from_der(request).unwrap();
    let now = Utc::now();
    let time = |t: chrono::DateTime<Utc>| {
        OcspGeneralizedTime(GeneralizedTime::from_unix_duration(Duration::from_secs(t.timestamp() as u64)).unwrap())
    };
    let data = ResponseData {
        version: Default::default(),
        responder_id: ResponderId::ByName(signer.cert.tbs_certificate.subject.clone()),
        produced_at: time(now),
        responses: vec![SingleResponse {
            cert_id: request.tbs_request.request_list[0].req_cert.clone(),
            cert_status: status,
            this_update: time(now - chrono::Duration::hours(1)),
            next_update: Some(time(now + chrono::Duration::days(next_update_days))),
            single_extensions: None,
        }],
        response_extensions: None,
    };
    let signature = signer.key.sign(&data.to_der().unwrap()).unwrap();
    let basic = BasicOcspResponse {
        tbs_response_data: data,
        signature_algorithm: ecdsa(),
        signature: BitString::from_bytes(signature.as_bytes()).unwrap(),
        certs: None,
    };
    OcspResponse {
        response_status: OcspResponseStatus::Successful,
        response_bytes: Some(ResponseBytes {
            response_type: ID_PKIX_OCSP_BASIC,
            response: OctetString::new(basic.to_der().unwrap()).unwrap(),
        }),
    }
    .to_der()
    .unwrap()
}

fn crl(signer: &Ca, issuer: &Name, revoked: &[&Certificate]) -> Vec<u8> {
    crl_with(signer, issuer, revoked, None)
}

fn crl_with(signer: &Ca, issuer: &Name, revoked: &[&Certificate], extensions: Option<Vec<x509_cert::ext::Extension>>) -> Vec<u8> {
    let now = Utc::now().timestamp() as u64;
    let utc = |s: u64| Time::UtcTime(UtcTime::from_unix_duration(Duration::from_secs(s)).unwrap());
    let tbs = TbsCertList {
        version: x509_cert::Version::V2,
        signature: ecdsa(),
        issuer: issuer.clone(),
        this_update: utc(now - 3600),
        next_update: Some(utc(now + 86400)),
        revoked_certificates: Some(
            revoked
                .iter()
                .map(|c| RevokedCert {
                    serial_number: c.tbs_certificate.serial_number.clone(),
                    revocation_date: utc(now - 60),
                    crl_entry_extensions: None,
                })
                .collect(),
        ),
        crl_extensions: extensions,
    };
    let signature = signer.key.sign(&tbs.to_der().unwrap()).unwrap();
    CertificateList { tbs_cert_list: tbs, signature_algorithm: ecdsa(), signature: BitString::from_bytes(signature.as_bytes()).unwrap() }
        .to_der()
        .unwrap()
}

#[test]
fn time_stamp_signatures_are_checked() {
    let setup = sealed_with_timestamp();
    let checks = verify_signatures(&setup.sealed).unwrap();
    assert!(checks[0].intact && checks[0].problems.is_empty(), "{checks:?}");

    let impostor = root("Impostor");
    let mut forged = issue(&setup.ca, "Test TSA", 7);
    forged.key = impostor.key;
    let identity = generate_seal_identity("Acme AB", "SE", 2).unwrap();
    let options = SealOptions { name: "Acme".into(), reason: "Signed".into(), location: String::new(), signing_time: Utc::now() };
    let sealed = seal(&sample(), &identity, &options, |digest| Ok(Some(token(&forged, &[&forged.cert], digest)))).unwrap();
    let checks = verify_signatures(&sealed).unwrap();
    assert!(checks[0].problems.iter().any(|p| p.contains("time stamp authority")), "{checks:?}");
}

#[test]
fn stored_time_stamp_tokens_are_checked() {
    let ca = root("Test CA");
    let tsa = issue(&ca, "Test TSA", 7);
    let digest: [u8; 32] = Sha256::digest(b"head").into();
    let good = token(&tsa, &[&tsa.cert], &digest);
    let time = crate::verify_timestamp_token(&good, b"head").unwrap();
    assert!((Utc::now() - time).num_hours().abs() <= 1);
    assert!(crate::verify_timestamp_token(&good, b"another head").is_err(), "the token covers something else");
    let impostor = root("Impostor");
    let mut forged = issue(&ca, "Test TSA", 7);
    forged.key = impostor.key;
    let bad = token(&forged, &[&forged.cert], &digest);
    assert!(crate::verify_timestamp_token(&bad, b"head").is_err(), "the signature does not verify");
    assert!(crate::verify_timestamp_token(b"garbage", b"head").is_err());
}

#[test]
fn plans_and_checks_revocation_data() {
    let setup = sealed_with_timestamp();
    let plan = plan_validation(&setup.sealed, &[]).unwrap();
    assert_eq!(plan.certificates.len(), 3, "seal, TSA and root");
    assert!(plan.missing_issuers.is_empty());
    let [check] = &plan.checks[..] else { panic!("only the TSA needs revocation data") };
    assert_eq!(check.ocsp_urls, ["http://ocsp.test"]);
    assert_eq!(check.crl_urls, ["http://crl.test/ca.crl"]);

    let request = check.ocsp_request().unwrap();
    let now = Utc::now();
    let good = check.accept_ocsp(&ocsp_response(&setup.ca, &request, CertStatus::Good(Null), 1), now).unwrap();
    assert_eq!(good.status, RevocationStatus::Good);
    assert!(check.accept_ocsp(&ocsp_response(&setup.ca, &request, CertStatus::Good(Null), -2), now).is_err(), "expired");
    assert!(check.accept_ocsp(&ocsp_response(&root("Test Root CA"), &request, CertStatus::Good(Null), 1), now).is_err(), "wrong key");
    let other = issue(&setup.ca, "Other", 8);
    let other_request = ocsp_request_for(&other, &setup.ca);
    assert!(check.accept_ocsp(&ocsp_response(&setup.ca, &other_request, CertStatus::Good(Null), 1), now).is_err(), "other certificate");

    let issuer = setup.ca.cert.tbs_certificate.subject.clone();
    let clean = check.accept_crl(&crl(&setup.ca, &issuer, &[&other.cert]), now).unwrap();
    assert_eq!(clean.status, RevocationStatus::Good);
    let revoked = check.accept_crl(&crl(&setup.ca, &issuer, &[&setup.tsa.cert]), now).unwrap();
    assert!(matches!(revoked.status, RevocationStatus::Revoked(_)));
    assert!(check.accept_crl(&crl(&root("Test Root CA"), &issuer, &[]), now).is_err(), "wrong key");
}

/// An OCSP request for `cert`, built the way OpenSign builds them.
fn ocsp_request_for(cert: &Ca, issuer: &Ca) -> Vec<u8> {
    let identity = generate_seal_identity("Other", "SE", 1).unwrap();
    let options = SealOptions { name: "x".into(), reason: "x".into(), location: String::new(), signing_time: Utc::now() };
    let pdf = seal(&sample(), &identity, &options, |digest| Ok(Some(token(cert, &[&cert.cert, &issuer.cert], digest)))).unwrap();
    plan_validation(&pdf, &[]).unwrap().checks[0].ocsp_request().unwrap()
}

#[test]
fn adds_validation_data_and_an_archive_time_stamp() {
    let setup = sealed_with_timestamp();
    let plan = plan_validation(&setup.sealed, &[]).unwrap();
    let check = &plan.checks[0];
    let mut data = ValidationData { certificates: plan.certificates.clone(), ..Default::default() };
    data.add(check.accept_ocsp(&ocsp_response(&setup.ca, &check.ocsp_request().unwrap(), CertStatus::Good(Null), 1), Utc::now()).unwrap());

    let with_dss = add_validation_data(&setup.sealed, &data).unwrap();
    assert!(with_dss.starts_with(&setup.sealed), "an incremental update leaves the signed bytes alone");
    let again = add_validation_data(&with_dss, &data).unwrap();
    let doc = Document::load_mem(&again).unwrap();
    let dss = doc.catalog().unwrap().get(b"DSS").unwrap().as_dict().unwrap();
    assert_eq!(dss.get(b"Certs").unwrap().as_array().unwrap().len(), 3, "nothing is stored twice");
    assert_eq!(dss.get(b"OCSPs").unwrap().as_array().unwrap().len(), 1);

    let checks = verify_signatures(&with_dss).unwrap();
    assert!(checks[0].intact && !checks[0].covers_whole_file && checks[0].only_validation_data_added, "{checks:?}");

    let prepared = prepare_document_timestamp(&with_dss).unwrap();
    let digest = prepared.digest;
    let archived = finish_document_timestamp(prepared, &token(&setup.tsa, &[&setup.tsa.cert, &setup.ca.cert], &digest)).unwrap();
    let checks = verify_signatures(&archived).unwrap();
    assert_eq!(checks.len(), 2);
    assert_eq!(checks[0].kind, "signature");
    assert!(checks[0].intact, "{checks:?}");
    assert_eq!(checks[1].kind, "documentTimestamp");
    assert!(checks[1].intact && checks[1].covers_whole_file && checks[1].problems.is_empty(), "{checks:?}");
    assert!(sanitize(&archived, Limits::default()).is_ok());

    let prepared = prepare_document_timestamp(&archived).unwrap();
    assert!(finish_document_timestamp(prepared, &token(&setup.tsa, &[&setup.tsa.cert], &[0u8; 32])).is_err(), "token for other bytes");
}

/// Appends an incremental update made by `change` to the sealed file.
fn update(pdf: &[u8], change: impl FnOnce(&mut IncrementalDocument)) -> Vec<u8> {
    let mut update = IncrementalDocument::create_from(pdf.to_vec(), Document::load_mem(pdf).unwrap());
    change(&mut update);
    let mut out = Vec::new();
    update.save_to(&mut out).unwrap();
    out
}

#[test]
fn later_changes_other_than_validation_data_break_the_seal() {
    let sealed = sealed_with_timestamp().sealed;
    let doc = Document::load_mem(&sealed).unwrap();
    let page = *doc.get_pages().values().next().unwrap();
    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();

    let new_content = update(&sealed, |u| {
        let content = u.new_document.add_object(Stream::new(dictionary! {}, b"BT /F1 24 Tf 72 600 Td (Pay 1000000) Tj ET".to_vec()));
        u.opt_clone_object_to_new_document(page).unwrap();
        u.new_document.get_dictionary_mut(page).unwrap().set("Contents", content);
    });
    let visible_annotation = update(&sealed, |u| {
        let annot = u.new_document.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "FreeText", "Rect" => vec![10.into(), 10.into(), 300.into(), 60.into()],
            "Contents" => Object::string_literal("Void"),
        });
        u.opt_clone_object_to_new_document(page).unwrap();
        let page = u.new_document.get_dictionary_mut(page).unwrap();
        let mut annots = page.get(b"Annots").unwrap().as_array().unwrap().clone();
        annots.push(annot.into());
        page.set("Annots", annots);
    });
    let open_action = update(&sealed, |u| {
        u.opt_clone_object_to_new_document(root).unwrap();
        u.new_document
            .get_dictionary_mut(root)
            .unwrap()
            .set("OpenAction", dictionary! { "S" => "URI", "URI" => Object::string_literal("https://x.test") });
    });
    let mut garbage = sealed.clone();
    garbage.extend_from_slice(b"\n% comment\n");

    for (name, pdf) in [("content", new_content), ("annotation", visible_annotation), ("open action", open_action)] {
        let check = &verify_signatures(&pdf).unwrap()[0];
        assert!(!check.intact && !check.only_validation_data_added, "{name}: {check:?}");
        assert!(check.digest_matches, "{name}");
    }
    let check = &verify_signatures(&garbage).unwrap()[0];
    assert!(check.intact, "a trailing comment changes nothing: {check:?}");
}

#[derive(Sequence)]
struct Status {
    status: u8,
}

#[derive(Sequence)]
struct TimeStampResponse {
    status: Status,
    token: ContentInfo,
}

fn response(token: Vec<u8>) -> Vec<u8> {
    TimeStampResponse { status: Status { status: 0 }, token: ContentInfo::from_der(&token).unwrap() }.to_der().unwrap()
}

#[test]
fn a_time_stamp_is_checked_when_it_arrives() {
    let ca = root("Test Root CA");
    let tsa = issue(&ca, "Test TSA", 7);
    let identity = generate_seal_identity("Acme AB", "SE", 2).unwrap();
    let options = SealOptions { name: "Acme".into(), reason: "Signed".into(), location: String::new(), signing_time: Utc::now() };
    let nonce = [0xA5u8; 16];
    let mut echoed = nonce.to_vec();
    echoed[0] &= 0x7f;
    let echoed = Int::new(&echoed).unwrap();

    let sealed = seal(&sample(), &identity, &options, |digest| {
        let body = response(token_with_nonce(&tsa, &[&tsa.cert], digest, Some(echoed.clone())));
        Ok(Some(parse_timestamp_response(&body, digest, &nonce).map_err(|e| PdfError::Timestamp(e.to_string()))?))
    })
    .unwrap();
    let check = &verify_signatures(&sealed).unwrap()[0];
    assert!(check.is_intact() && check.timestamp.is_some() && check.problems.is_empty(), "{check:?}");

    let (digest, other) = ([1u8; 32], [2u8; 32]);
    let good = |nonce_value: Option<Int>, signed_for: &[u8; 32]| response(token_with_nonce(&tsa, &[&tsa.cert], signed_for, nonce_value));
    assert!(parse_timestamp_response(&good(Some(echoed.clone()), &digest), &digest, &nonce).is_ok());
    assert!(parse_timestamp_response(&good(Some(Int::new(&[1, 2]).unwrap()), &digest), &digest, &nonce).is_err(), "wrong nonce");
    assert!(parse_timestamp_response(&good(Some(echoed.clone()), &other), &digest, &nonce).is_err(), "wrong imprint");

    let not_for_stamping = issue_other(&ca, "Web Server", 8);
    let body = response(token_with_nonce(&not_for_stamping, &[&not_for_stamping.cert], &digest, Some(echoed.clone())));
    assert!(parse_timestamp_response(&body, &digest, &nonce).is_err(), "certificate not for time stamping");

    let mut wrong_key = issue(&ca, "Test TSA", 7);
    wrong_key.key = root("Other").key;
    let body = response(token_with_nonce(&wrong_key, &[&wrong_key.cert], &digest, Some(echoed)));
    assert!(parse_timestamp_response(&body, &digest, &nonce).is_err(), "signature does not verify");
}

#[test]
fn a_time_stamp_from_the_wrong_kind_of_certificate_shows_no_time() {
    let ca = root("Test Root CA");
    let other = issue_other(&ca, "Web Server", 8);
    let identity = generate_seal_identity("Acme AB", "SE", 2).unwrap();
    let options = SealOptions { name: "Acme".into(), reason: "Signed".into(), location: String::new(), signing_time: Utc::now() };
    let sealed = seal(&sample(), &identity, &options, |digest| Ok(Some(token(&other, &[&other.cert], digest)))).unwrap();
    let check = &verify_signatures(&sealed).unwrap()[0];
    assert!(check.timestamp.is_none() && check.problems.iter().any(|p| p.contains("not for time stamping")), "{check:?}");

    let prepared = prepare_document_timestamp(&sealed).unwrap();
    let digest = prepared.digest;
    assert!(finish_document_timestamp(prepared, &token(&other, &[&other.cert], &digest)).is_err());
}

#[test]
fn a_partial_crl_says_nothing_about_a_certificate() {
    let setup = sealed_with_timestamp();
    let plan = plan_validation(&setup.sealed, &[]).unwrap();
    let check = &plan.checks[0];
    let issuer = setup.ca.cert.tbs_certificate.subject.clone();
    let partial = x509_cert::ext::Extension {
        extn_id: const_oid::db::rfc5912::ID_CE_ISSUING_DISTRIBUTION_POINT,
        critical: true,
        extn_value: OctetString::new(vec![0x30, 0x00]).unwrap(),
    };
    assert!(check.accept_crl(&crl_with(&setup.ca, &issuer, &[], Some(vec![partial])), Utc::now()).is_err());
}

/// Appends raw objects and free entries the way an ordinary PDF tool writes an update.
fn raw_update(pdf: &[u8], objects: &[(u32, &str)], free: &[u32], trailer_extra: &str) -> Vec<u8> {
    let doc = Document::load_mem(pdf).unwrap();
    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let mut out = pdf.to_vec();
    out.push(b'\n');
    let mut table = String::from("xref\n0 1\n0000000000 65535 f \n");
    for (id, body) in objects {
        table.push_str(&format!("{id} 1\n{:010} 00000 n \n", out.len()));
        out.extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
    }
    for id in free {
        table.push_str(&format!("{id} 1\n0000000000 00001 f \n"));
    }
    let start = out.len();
    let size = doc.max_id.max(objects.iter().map(|o| o.0).max().unwrap_or(0)) + 1;
    let prev = doc.xref_start;
    out.extend_from_slice(table.as_bytes());
    out.extend_from_slice(
        format!("trailer\n<</Size {size}/Root {} 0 R/Prev {prev}{trailer_extra}>>\nstartxref\n{start}\n%%EOF\n", root.0).as_bytes(),
    );
    out
}

fn assert_refused(pdf: &[u8]) {
    for check in verify_signatures(pdf).unwrap_or_default() {
        assert!(!check.intact && !check.only_validation_data_added, "{check:?}");
    }
}

const HIDDEN: &str = "5000 0 obj\n<</Length 44>>stream\nBT /F1 24 Tf 72 600 Td (Pay 1000000) Tj ET   \nendstream\nendobj\n";

#[test]
fn a_time_stamp_names_its_authority() {
    let sealed = sealed_with_timestamp().sealed;
    let checks = verify_signatures(&sealed).unwrap();
    assert!(
        checks.iter().all(|c| c.timestamp.is_none() || c.timestamp_authority_sha256.as_ref().is_some_and(|sha| sha.len() == 64)),
        "{checks:?}"
    );
    assert!(checks.iter().any(|c| c.timestamp_authority_sha256.is_some()), "{checks:?}");
    assert!(checks.iter().all(|c| !c.timestamp_trusted), "only the caller, with the list, can say");
}

#[test]
fn bytes_after_the_seal_that_no_table_lists_break_it() {
    let sealed = sealed_with_timestamp().sealed;
    assert!(verify_signatures(&raw_update(&sealed, &[], &[], "")).unwrap()[0].only_validation_data_added);

    let mut after_the_end = raw_update(&sealed, &[], &[], "");
    after_the_end.extend_from_slice(HIDDEN.as_bytes());
    assert_refused(&after_the_end);

    let mut before_a_table = sealed.clone();
    before_a_table.extend_from_slice(HIDDEN.as_bytes());
    assert_refused(&raw_update(&before_a_table, &[], &[], ""));
}

#[test]
fn an_update_the_newest_table_skips_breaks_the_seal() {
    let sealed = sealed_with_timestamp().sealed;
    let doc = Document::load_mem(&sealed).unwrap();
    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap().0;
    let mut out = raw_update(&sealed, &[(5001, "<</Unused true>>")], &[], "");
    let start = out.len();
    out.extend_from_slice(
        format!(
            "xref\n0 1\n0000000000 65535 f \ntrailer\n<</Size {}/Root {root} 0 R/Prev {}>>\nstartxref\n{start}\n%%EOF\n",
            doc.max_id + 1,
            doc.xref_start
        )
        .as_bytes(),
    );
    assert_refused(&out);
}

#[test]
fn an_update_must_end_where_its_table_says() {
    let sealed = sealed_with_timestamp().sealed;
    let doc = Document::load_mem(&sealed).unwrap();
    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap().0;
    let older = raw_update(&sealed, &[], &[], "");
    let keyword = older.windows(9).rposition(|w| w == b"startxref").unwrap();
    let own = older[..keyword].windows(5).rposition(|w| w == b"\nxref").unwrap() + 1;
    let mut out = older[..keyword].to_vec();
    out.extend_from_slice(format!("startxref\n{}\n%%EOF\n", doc.xref_start).as_bytes());
    let start = out.len();
    out.extend_from_slice(
        format!(
            "xref\n0 1\n0000000000 65535 f \ntrailer\n<</Size {}/Root {root} 0 R/Prev {own}>>\nstartxref\n{start}\n%%EOF\n",
            doc.max_id + 1
        )
        .as_bytes(),
    );
    assert_refused(&out);
}

#[test]
fn keys_a_plain_search_would_misread_in_a_trailer_are_refused() {
    let sealed = sealed_with_timestamp().sealed;
    for extra in ["/Prev 5", "/X<</Y 1>>", "%comment\n"] {
        assert_refused(&raw_update(&sealed, &[], &[], extra));
    }
    let prev = Document::load_mem(&sealed).unwrap().xref_start;
    let updated = raw_update(&sealed, &[], &[], "");
    let needle = format!("/Prev {prev}");
    let at = updated.windows(needle.len()).rposition(|w| w == needle.as_bytes()).unwrap();
    let mut out = updated[..at].to_vec();
    out.extend_from_slice(format!("/Note({needle})").as_bytes());
    out.extend_from_slice(&updated[at + needle.len()..]);
    assert_refused(&out);
}

#[test]
fn an_index_a_plain_search_would_misread_in_a_cross_reference_stream_is_refused() {
    let sealed = sealed_with_timestamp().sealed;
    let doc = Document::load_mem(&sealed).unwrap();
    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap().0;
    let own = doc.max_id + 2;
    let mut out = sealed.clone();
    let hidden = out.len();
    out.extend_from_slice(b"5000 0 obj\n<</Length 44>>stream\nBT /F1 24 Tf 72 600 Td (Pay 1000000) Tj ET   \nendstream\nendobj\n");
    let start = out.len();
    let mut entries = vec![1u8];
    entries.extend_from_slice(&(hidden as u32).to_be_bytes());
    entries.extend_from_slice(&[0, 0]);
    let dict = format!(
        "<</Type/XRef/Size {}/W[1 4 2]/Index [5000 1]/Note(/Index[0 1])/Root {root} 0 R/Prev {}/Length {}>>",
        own + 1,
        doc.xref_start,
        entries.len()
    );
    out.extend_from_slice(format!("{own} 0 obj{dict}stream\n").as_bytes());
    out.extend_from_slice(&entries);
    out.extend_from_slice(format!("\nendstream\nendobj\nstartxref\n{start}\n%%EOF\n").as_bytes());
    assert_refused(&out);
}

#[test]
fn an_update_that_frees_an_object_breaks_the_seal() {
    let sealed = sealed_with_timestamp().sealed;
    let doc = Document::load_mem(&sealed).unwrap();
    let page = *doc.get_pages().values().next().unwrap();
    let content = doc.get_page_contents(page)[0];
    let updated = raw_update(&sealed, &[], &[content.0], "");
    let check = &verify_signatures(&updated).unwrap()[0];
    assert!(!check.intact && !check.only_validation_data_added, "{check:?}");
}

#[test]
fn an_update_that_defines_an_undefined_object_breaks_the_seal() {
    let mut doc = Document::load_mem(&sample()).unwrap();
    let page = *doc.get_pages().values().next().unwrap();
    let content = doc.get_page_contents(page)[0];
    doc.get_dictionary_mut(page).unwrap().set("Contents", vec![Object::Reference(content), Object::Reference((7777, 0))]);
    let mut pdf = Vec::new();
    doc.save_to(&mut pdf).unwrap();

    let identity = generate_seal_identity("Acme AB", "SE", 1).unwrap();
    let options = SealOptions { name: "Acme".into(), reason: "Signed".into(), location: String::new(), signing_time: Utc::now() };
    let sealed = seal(&pdf, &identity, &options, |_| Ok(None)).unwrap();
    assert!(verify_signatures(&sealed).unwrap()[0].intact);

    let shadow = "<</Length 44>>stream\nBT /F1 24 Tf 72 600 Td (Pay 1000000) Tj ET   \nendstream";
    let updated = raw_update(&sealed, &[(7777, shadow)], &[], "");
    let check = &verify_signatures(&updated).unwrap()[0];
    assert!(!check.intact && !check.only_validation_data_added, "{check:?}");

    let unused = raw_update(&sealed, &[(8888, "<</Unused true>>")], &[], "");
    assert!(!verify_signatures(&unused).unwrap()[0].intact, "nothing may be added that is not validation data");
}

#[test]
fn a_time_stamp_field_must_be_exactly_what_opensign_writes() {
    let setup = sealed_with_timestamp();
    let prepared = prepare_document_timestamp(&setup.sealed).unwrap();
    let digest = prepared.digest;
    let archived = finish_document_timestamp(prepared, &token(&setup.tsa, &[&setup.tsa.cert], &digest)).unwrap();
    assert!(verify_signatures(&archived).unwrap()[0].intact, "the plain form holds");

    let doc = Document::load_mem(&archived).unwrap();
    let fields = doc.catalog().unwrap().get(b"AcroForm").unwrap().as_dict().unwrap().get(b"Fields").unwrap().as_array().unwrap().clone();
    let widget = fields.last().unwrap().as_reference().unwrap();
    let with_action = update(&archived, |u| {
        u.opt_clone_object_to_new_document(widget).unwrap();
        let script = dictionary! { "S" => "JavaScript", "JS" => Object::string_literal("app.alert(1)") };
        u.new_document.get_dictionary_mut(widget).unwrap().set("AA", dictionary! { "O" => script });
    });
    assert!(!verify_signatures(&with_action).unwrap()[0].intact);

    let check = |sealed: &[u8], extra: lopdf::Dictionary, page_id, root_id| {
        let updated = update(sealed, |u| {
            let sig = u.new_document.add_object(dictionary! {
                "Type" => "DocTimeStamp", "Filter" => "Adobe.PPKLite", "SubFilter" => "ETSI.RFC3161",
                "ByteRange" => vec![0.into(), 1.into(), 2.into(), 3.into()], "Contents" => Object::String(vec![0], StringFormat::Hexadecimal),
            });
            let mut widget = dictionary! {
                "Type" => "Annot", "Subtype" => "Widget", "FT" => "Sig", "T" => Object::string_literal("Added"), "V" => sig,
                "Rect" => vec![0.into(), 0.into(), 0.into(), 0.into()], "F" => 132, "P" => Object::Reference(page_id),
            };
            widget.extend(&extra);
            let widget = u.new_document.add_object(widget);
            u.opt_clone_object_to_new_document(page_id).unwrap();
            u.opt_clone_object_to_new_document(root_id).unwrap();
            let page = u.new_document.get_dictionary_mut(page_id).unwrap();
            let mut annots = page.get(b"Annots").unwrap().as_array().unwrap().clone();
            annots.push(widget.into());
            page.set("Annots", annots);
            let catalog = u.new_document.get_dictionary_mut(root_id).unwrap();
            let form = catalog.get_mut(b"AcroForm").unwrap().as_dict_mut().unwrap();
            let mut fields = form.get(b"Fields").unwrap().as_array().unwrap().clone();
            fields.push(widget.into());
            form.set("Fields", fields);
        });
        verify_signatures(&updated).unwrap().iter().find(|c| c.kind == "signature").unwrap().only_validation_data_added
    };
    let doc = Document::load_mem(&setup.sealed).unwrap();
    let page = *doc.get_pages().values().next().unwrap();
    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    assert!(check(&setup.sealed, dictionary! {}, page, root), "the plain form holds");
    assert!(!check(&setup.sealed, dictionary! { "AA" => dictionary! {} }, page, root), "an action");
    assert!(!check(&setup.sealed, dictionary! { "AP" => dictionary! {} }, page, root), "an appearance");
}

#[test]
fn a_later_trailer_cannot_change_how_the_file_is_read() {
    let sealed = sealed_with_timestamp().sealed;
    for extra in ["/Encrypt 1 0 R", "/#45ncrypt 1 0 R", "/XRefStm 1"] {
        let updated = raw_update(&sealed, &[], &[], extra);
        if let Ok(checks) = verify_signatures(&updated) {
            assert!(checks.iter().all(|c| !c.intact && !c.only_validation_data_added), "{extra}: {checks:?}");
        }
    }
}

#[test]
fn a_signature_can_only_be_checked_against_its_own_hole() {
    let sealed = sealed_with_timestamp().sealed;
    let at = sealed.windows(10).position(|w| w == b"/Contents<").unwrap() + 10;
    let mut changed = sealed.clone();
    let end = at + changed[at..].iter().position(|b| *b == b'>').unwrap();
    assert_eq!(changed[end - 1], b'0', "the padding");
    changed[end - 1] = b'1';
    let check = &verify_signatures(&changed).unwrap()[0];
    assert!(check.digest_matches);
    assert_ne!(check.signature_valid, None);
}

#[test]
fn text_in_a_document_cannot_be_mistaken_for_the_signature_fields() {
    let mut doc = Document::load_mem(&sample()).unwrap();
    let page = *doc.get_pages().values().next().unwrap();
    let hole = format!("/Contents<{}>", "0".repeat(48 * 1024));
    let lure = format!("BT (/ByteRange[0 9999999999 9999999999 9999999999]{hole} 9999999999.pdf) Tj ET");
    let content = doc.add_object(Stream::new(dictionary! {}, lure.into_bytes()));
    doc.get_dictionary_mut(page).unwrap().set("Contents", content);
    let mut pdf = Vec::new();
    doc.save_to(&mut pdf).unwrap();

    let identity = generate_seal_identity("Acme AB", "SE", 1).unwrap();
    let options = SealOptions { name: "9999999999".into(), reason: "9999999999".into(), location: String::new(), signing_time: Utc::now() };
    let sealed = seal(&pdf, &identity, &options, |_| Ok(None)).unwrap();
    let check = &verify_signatures(&sealed).unwrap()[0];
    assert!(check.intact && check.covers_whole_file, "{check:?}");
}

fn raw_pdf(objects: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = std::collections::BTreeMap::new();
    for (id, body) in objects {
        offsets.insert(*id, out.len());
        out.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let start = out.len();
    let max = objects.iter().map(|o| o.0).max().unwrap();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", max + 1).as_bytes());
    for id in 1..=max {
        match offsets.get(&id) {
            Some(offset) => out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes()),
            None => out.extend_from_slice(b"0000000000 00000 f \n"),
        }
    }
    out.extend_from_slice(format!("trailer\n<</Size {}/Root 1 0 R>>\nstartxref\n{start}\n%%EOF\n", max + 1).as_bytes());
    out
}

fn raw_stream(dict: &str, data: &[u8]) -> Vec<u8> {
    let mut out = format!("<<{dict}/Length {}>>\nstream\n", data.len()).into_bytes();
    out.extend_from_slice(data);
    out.extend_from_slice(b"\nendstream");
    out
}

fn raw_base() -> Vec<(u32, Vec<u8>)> {
    vec![
        (1, b"<</Type/Catalog/Pages 2 0 R>>".to_vec()),
        (2, b"<</Type/Pages/Kids[3 0 R]/Count 1>>".to_vec()),
        (3, b"<</Type/Page/Parent 2 0 R/MediaBox[0 0 600 800]/Contents 4 0 R>>".to_vec()),
        (4, raw_stream("", b"BT ET")),
    ]
}

fn contains(pdf: &[u8], marker: &str) -> bool {
    let mut doc = Document::load_mem(pdf).unwrap();
    doc.decompress();
    let mut all = Vec::new();
    doc.save_to(&mut all).unwrap();
    all.windows(marker.len()).any(|w| w == marker.as_bytes())
}

#[test]
fn actions_and_files_in_unusual_spellings_are_removed() {
    let mut objects = raw_base();
    objects[0].1 = b"<</Type/Catalog/Pages 2 0 R/Outlines 10 0 R/AF[20 0 R]/Unknown<</URI(UNKNOWNMARK)>>>>".to_vec();
    objects.push((10, b"<</Type/Outlines/First 11 0 R/Last 13 0 R/Count 3>>".to_vec()));
    objects.push((11, b"<</Title(a)/Parent 10 0 R/Next 12 0 R/A<</S 15 0 R/URI(URIMARK)>>>>".to_vec()));
    objects.push((12, b"<</Title(b)/Parent 10 0 R/Next 13 0 R/A<</S 16 0 R/F(LAUNCHMARK)>>>>".to_vec()));
    objects.push((13, b"<</Title(c)/Parent 10 0 R/A<</S/#55RI/URI(ESCAPEDMARK)>>>>".to_vec()));
    objects.push((15, b"/URI".to_vec()));
    objects.push((16, b"/Launch".to_vec()));
    objects.push((20, b"<</F(a.exe)/EF<</F 21 0 R>>>>".to_vec()));
    objects.push((21, raw_stream("", b"EMBEDDEDMARK")));
    let clean = sanitize(&raw_pdf(&objects), Limits::default()).unwrap();
    for marker in ["URIMARK", "LAUNCHMARK", "ESCAPEDMARK", "EMBEDDEDMARK", "UNKNOWNMARK"] {
        assert!(!contains(&clean.pdf, marker), "{marker} survived");
    }
    assert!(contains(&clean.pdf, "Outlines"), "the outline itself stays");
}

#[test]
fn a_resource_named_like_an_action_key_stays() {
    let mut objects = raw_base();
    objects[2].1 = b"<</Type/Page/Parent 2 0 R/MediaBox[0 0 600 800]/Contents 4 0 R/Resources<</Font<</A 5 0 R>>>>>>".to_vec();
    objects.push((5, b"<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>".to_vec()));
    let clean = sanitize(&raw_pdf(&objects), Limits::default()).unwrap();
    assert!(contains(&clean.pdf, "Helvetica"));
}

#[test]
fn a_page_tree_with_nodes_that_are_not_pages_is_refused() {
    let mut objects = raw_base();
    objects[1].1 = b"<</Type/Pages/Kids[3 0 R 30 0 R]/Count 2>>".to_vec();
    objects.push((30, b"<</Type/Foo/Parent 2 0 R/MediaBox[0 0 600 800]/Annots[<</Subtype/Link/Rect[0 0 600 800]>>]>>".to_vec()));
    assert!(matches!(sanitize(&raw_pdf(&objects), Limits::default()), Err(PdfError::Rejected(_))));
}

#[test]
fn references_to_nothing_do_not_survive_cleaning() {
    let mut objects = raw_base();
    objects[2].1 = b"<</Type/Page/Parent 2 0 R/MediaBox[0 0 600 800]/Contents[4 0 R 7777 0 R]>>".to_vec();
    let clean = sanitize(&raw_pdf(&objects), Limits::default()).unwrap();
    let doc = Document::load_mem(&clean.pdf).unwrap();
    for object in doc.objects.values() {
        if let Ok(items) = object.as_dict().and_then(|d| d.get(b"Contents")).and_then(Object::as_array) {
            assert!(items.iter().all(|item| item.as_reference().map_or(true, |id| doc.objects.contains_key(&id))), "{items:?}");
        }
    }
}

#[test]
fn overlapping_object_stream_entries_are_refused() {
    let entries = 3000;
    let mut header = String::new();
    for n in 0..entries {
        header.push_str(&format!("{} 0 ", 100 + n));
    }
    let first = header.len();
    let mut content = header.into_bytes();
    content.push(b'(');
    content.extend(std::iter::repeat_n(b'a', 1024 * 1024));
    content.push(b')');
    let mut objects = raw_base();
    objects.push((5, raw_stream(&format!("/Type/ObjStm/N {entries}/First {first}"), &content)));
    let pdf = raw_pdf(&objects);
    assert!(matches!(sanitize(&pdf, Limits::default()), Err(PdfError::Rejected(_))));
    for result in
        [page_sizes(&pdf).map(|_| ()), extract_text(&pdf).map(|_| ()), hidden_text(&pdf).map(|_| ()), verify_signatures(&pdf).map(|_| ())]
    {
        assert!(result.is_err());
    }
}

#[test]
fn page_content_over_the_limit_is_refused() {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    std::io::Write::write_all(&mut encoder, &vec![b' '; 4 * 1024 * 1024]).unwrap();
    let bomb = encoder.finish().unwrap();
    let mut objects = raw_base();
    objects[3] = (4, raw_stream("/Filter/FlateDecode", &bomb));
    let limits = Limits { max_stream_bytes: 1024 * 1024, ..Limits::default() };
    assert!(matches!(sanitize(&raw_pdf(&objects), limits), Err(PdfError::Rejected(_))));
}

#[test]
fn a_file_with_too_many_signatures_is_refused() {
    let mut objects = raw_base();
    for n in 0..300 {
        objects.push((10 + n, format!("<</Type/Sig/ByteRange[0 1 2 3]/Contents<00>/N {n}>>").into_bytes()));
    }
    assert!(matches!(verify_signatures(&raw_pdf(&objects)), Err(PdfError::Rejected(_))));
}

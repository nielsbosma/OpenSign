#![allow(clippy::unwrap_used)]

use std::str::FromStr;
use std::time::Duration;

use chrono::Utc;
use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::content_info::{CmsVersion, ContentInfo};
use cms::signed_data::{CertificateSet, EncapsulatedContentInfo, SignedData, SignerIdentifier, SignerInfo, SignerInfos};
use const_oid::ObjectIdentifier;
use const_oid::db::rfc5911::{ID_CONTENT_TYPE, ID_MESSAGE_DIGEST, ID_SIGNED_DATA};
use const_oid::db::rfc5912::{ECDSA_WITH_SHA_256, ID_AD_OCSP, ID_SHA_256};
use const_oid::db::rfc6960::ID_PKIX_OCSP_BASIC;
use der::asn1::{Any, BitString, GeneralizedTime, Ia5String, Int, Null, OctetString, SetOfVec, UtcTime};
use der::{Decode, Encode, Sequence};
use lopdf::{Document, IncrementalDocument, Object, Stream, dictionary};
use sha2::{Digest, Sha256};
use spki::{AlgorithmIdentifierOwned, SubjectPublicKeyInfoOwned};
use x509_cert::Certificate;
use x509_cert::attr::Attribute;
use x509_cert::builder::{Builder, CertificateBuilder, Profile};
use x509_cert::crl::{CertificateList, RevokedCert, TbsCertList};
use x509_cert::ext::pkix::crl::dp::DistributionPoint;
use x509_cert::ext::pkix::name::{DistributionPointName, GeneralName};
use x509_cert::ext::pkix::{AccessDescription, AuthorityInfoAccessSyntax, CrlDistributionPoints};
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
}

/// A signed RFC 3161 token over `digest`, as a real TSA returns it.
fn token(tsa: &Ca, chain: &[&Certificate], digest: &[u8; 32]) -> Vec<u8> {
    let info = TstInfo {
        version: 1,
        policy: ObjectIdentifier::new_unwrap("1.2.3.4"),
        message_imprint: Imprint {
            hash_algorithm: AlgorithmIdentifierOwned { oid: ID_SHA_256, parameters: None },
            hashed_message: OctetString::new(digest.to_vec()).unwrap(),
        },
        serial_number: Int::new(&[9]).unwrap(),
        gen_time: GeneralizedTime::from_unix_duration(Duration::from_secs(Utc::now().timestamp() as u64)).unwrap(),
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
        crl_extensions: None,
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

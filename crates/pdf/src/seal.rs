use chrono::{DateTime, Utc};
use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::content_info::{CmsVersion, ContentInfo};
use cms::signed_data::{CertificateSet, EncapsulatedContentInfo, SignedData, SignerIdentifier, SignerInfo, SignerInfos};
use const_oid::ObjectIdentifier;
use const_oid::db::rfc5911::{ID_CONTENT_TYPE, ID_DATA, ID_MESSAGE_DIGEST, ID_SIGNED_DATA};
use const_oid::db::rfc5912::{ECDSA_WITH_SHA_256, ID_SHA_256};
use der::asn1::{Any, OctetString, SetOfVec};
use der::{Encode, Sequence};
use lopdf::{Document, Object, StringFormat, dictionary};
use p256::ecdsa::DerSignature;
use sha2::{Digest, Sha256};
use signature::Signer;
use spki::AlgorithmIdentifierOwned;
use x509_cert::attr::Attribute;

use crate::cert::SealIdentity;
use crate::{PdfError, Result};

pub const ID_AA_SIGNING_CERTIFICATE_V2: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.47");
pub const ID_AA_SIGNATURE_TIME_STAMP_TOKEN: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.14");

/// Room for the CMS signature: certificate chain, time stamp token with the TSA's chain, and margin.
const CONTENTS_BYTES: usize = 24 * 1024;
const RANGE_PLACEHOLDER: &[u8] = b"9999999999";

#[derive(Debug, Clone)]
pub struct SealOptions {
    pub name: String,
    pub reason: String,
    pub location: String,
    pub signing_time: DateTime<Utc>,
}

#[derive(Sequence)]
struct EssCertIdV2 {
    cert_hash: OctetString,
}

#[derive(Sequence)]
struct SigningCertificateV2 {
    certs: Vec<EssCertIdV2>,
}

fn sig_err(e: impl std::fmt::Display) -> PdfError {
    PdfError::Signing(e.to_string())
}

fn attribute(oid: ObjectIdentifier, value: Any) -> Result<Attribute> {
    Ok(Attribute { oid, values: SetOfVec::try_from(vec![value]).map_err(sig_err)? })
}

fn pdf_date(at: &DateTime<Utc>) -> Vec<u8> {
    at.format("D:%Y%m%d%H%M%S+00'00'").to_string().into_bytes()
}

fn pdf_string(text: &str) -> Object {
    Object::String(crate::stamp::winansi(text), StringFormat::Literal)
}

/// A PDF with an empty signature field whose `/Contents` is a fixed-size hole.
pub struct Prepared {
    pdf: Vec<u8>,
    hole_start: usize,
    hole_end: usize,
    /// SHA-256 over every byte except the hole: what the signature covers.
    pub digest: [u8; 32],
}

fn find(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    haystack.windows(needle.len()).enumerate().filter(|(_, w)| *w == needle).map(|(i, _)| i).collect()
}

pub fn prepare(pdf: &[u8], options: &SealOptions) -> Result<Prepared> {
    let mut doc = Document::load_mem(pdf)?;
    let first_page = *doc.get_pages().values().next().ok_or_else(|| PdfError::Malformed("no pages".into()))?;
    let placeholder = || Object::Integer(9_999_999_999);
    let mut sig = dictionary! {
        "Type" => "Sig",
        "Filter" => "Adobe.PPKLite",
        "SubFilter" => "ETSI.CAdES.detached",
        "ByteRange" => vec![Object::Integer(0), placeholder(), placeholder(), placeholder()],
        "Contents" => Object::String(vec![0u8; CONTENTS_BYTES], StringFormat::Hexadecimal),
        "M" => Object::String(pdf_date(&options.signing_time), StringFormat::Literal),
        "Name" => pdf_string(&options.name),
        "Reason" => pdf_string(&options.reason),
    };
    if !options.location.is_empty() {
        sig.set("Location", pdf_string(&options.location));
    }
    let sig_id = doc.add_object(sig);
    let widget = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Sig",
        "T" => Object::String(b"OpenSign Seal".to_vec(), StringFormat::Literal),
        "V" => Object::Reference(sig_id),
        "Rect" => vec![0.into(), 0.into(), 0.into(), 0.into()],
        "F" => 132,
        "P" => Object::Reference(first_page),
    });
    let page = doc.get_dictionary_mut(first_page)?;
    let mut annots = page.get(b"Annots").ok().and_then(|a| a.as_array().ok()).cloned().unwrap_or_default();
    annots.push(Object::Reference(widget));
    page.set("Annots", annots);
    doc.catalog_mut()?.set("AcroForm", dictionary! { "Fields" => vec![Object::Reference(widget)], "SigFlags" => 3 });

    let mut out = Vec::with_capacity(pdf.len() + CONTENTS_BYTES * 2 + 1024);
    doc.save_to(&mut out).map_err(|e| PdfError::Malformed(e.to_string()))?;

    let mut hole = vec![b'<'];
    hole.extend(std::iter::repeat_n(b'0', CONTENTS_BYTES * 2));
    hole.push(b'>');
    let holes = find(&out, &hole);
    let [hole_start] = holes[..] else { return Err(sig_err("could not find the signature placeholder")) };
    let hole_end = hole_start + hole.len();

    let ranges = find(&out, RANGE_PLACEHOLDER);
    if ranges.len() != 3 {
        return Err(sig_err("could not find the byte range placeholder"));
    }
    let values = [hole_start, hole_end, out.len() - hole_end];
    for (at, value) in ranges.iter().zip(values) {
        let text = format!("{value:<10}");
        if text.len() != RANGE_PLACEHOLDER.len() {
            return Err(sig_err("the document is too large to seal"));
        }
        out[*at..*at + 10].copy_from_slice(text.as_bytes());
    }
    let mut hasher = Sha256::new();
    hasher.update(&out[..hole_start]);
    hasher.update(&out[hole_end..]);
    Ok(Prepared { pdf: out, hole_start, hole_end, digest: hasher.finalize().into() })
}

/// The signer's part of the CMS structure: signed attributes and the signature over them.
pub struct Signed {
    signed_attrs: SetOfVec<Attribute>,
    pub signature: Vec<u8>,
}

pub fn sign(identity: &SealIdentity, digest: &[u8; 32]) -> Result<Signed> {
    let cert_der = identity.certificate().to_der().map_err(sig_err)?;
    let signing_certificate = SigningCertificateV2 {
        certs: vec![EssCertIdV2 { cert_hash: OctetString::new(Sha256::digest(&cert_der).to_vec()).map_err(sig_err)? }],
    };
    let attrs = vec![
        attribute(ID_CONTENT_TYPE, Any::encode_from(&ID_DATA).map_err(sig_err)?)?,
        attribute(ID_MESSAGE_DIGEST, Any::encode_from(&OctetString::new(digest.to_vec()).map_err(sig_err)?).map_err(sig_err)?)?,
        attribute(ID_AA_SIGNING_CERTIFICATE_V2, Any::encode_from(&signing_certificate).map_err(sig_err)?)?,
    ];
    let signed_attrs = SetOfVec::try_from(attrs).map_err(sig_err)?;
    let to_sign = signed_attrs.to_der().map_err(sig_err)?;
    let signature: DerSignature = identity.key.try_sign(&to_sign).map_err(sig_err)?;
    Ok(Signed { signed_attrs, signature: signature.as_bytes().to_vec() })
}

/// Builds the CMS SignedData (with the time stamp token, when given, as an unsigned attribute)
/// and writes it into the hole.
pub fn finish(prepared: Prepared, identity: &SealIdentity, signed: Signed, timestamp_token: Option<Vec<u8>>) -> Result<Vec<u8>> {
    let cert = identity.certificate();
    let sha256 = AlgorithmIdentifierOwned { oid: ID_SHA_256, parameters: None };
    let unsigned_attrs = match timestamp_token {
        Some(token) => {
            let token = ContentInfo::from_der_bytes(&token)?;
            Some(
                SetOfVec::try_from(vec![attribute(ID_AA_SIGNATURE_TIME_STAMP_TOKEN, Any::encode_from(&token).map_err(sig_err)?)?])
                    .map_err(sig_err)?,
            )
        }
        None => None,
    };
    let signer_info = SignerInfo {
        version: CmsVersion::V1,
        sid: SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
            issuer: cert.tbs_certificate.issuer.clone(),
            serial_number: cert.tbs_certificate.serial_number.clone(),
        }),
        digest_alg: sha256.clone(),
        signed_attrs: Some(signed.signed_attrs),
        signature_algorithm: AlgorithmIdentifierOwned { oid: ECDSA_WITH_SHA_256, parameters: None },
        signature: OctetString::new(signed.signature).map_err(sig_err)?,
        unsigned_attrs,
    };
    let certificates = CertificateSet::try_from(identity.chain.iter().cloned().map(CertificateChoices::Certificate).collect::<Vec<_>>())
        .map_err(sig_err)?;
    let signed_data = SignedData {
        version: CmsVersion::V1,
        digest_algorithms: SetOfVec::try_from(vec![sha256]).map_err(sig_err)?,
        encap_content_info: EncapsulatedContentInfo { econtent_type: ID_DATA, econtent: None },
        certificates: Some(certificates),
        crls: None,
        signer_infos: SignerInfos::try_from(vec![signer_info]).map_err(sig_err)?,
    };
    let content_info = ContentInfo { content_type: ID_SIGNED_DATA, content: Any::encode_from(&signed_data).map_err(sig_err)? };
    let der = content_info.to_der().map_err(sig_err)?;
    if der.len() > CONTENTS_BYTES {
        return Err(sig_err("the signature is larger than the space reserved for it"));
    }
    let mut pdf = prepared.pdf;
    let hex = hex::encode_upper(&der);
    pdf[prepared.hole_start + 1..prepared.hole_start + 1 + hex.len()].copy_from_slice(hex.as_bytes());
    debug_assert_eq!(pdf[prepared.hole_end - 1], b'>');
    Ok(pdf)
}

trait FromDerBytes: Sized {
    fn from_der_bytes(bytes: &[u8]) -> Result<Self>;
}

impl FromDerBytes for ContentInfo {
    fn from_der_bytes(bytes: &[u8]) -> Result<Self> {
        der::Decode::from_der(bytes).map_err(|e| PdfError::Timestamp(e.to_string()))
    }
}

/// Seals in one go when the time stamp can be fetched synchronously. `timestamp` receives the
/// signature value and returns an RFC 3161 token, or `None` to seal without one (PAdES B-B).
pub fn seal(
    pdf: &[u8],
    identity: &SealIdentity,
    options: &SealOptions,
    timestamp: impl FnOnce(&[u8]) -> Result<Option<Vec<u8>>>,
) -> Result<Vec<u8>> {
    let prepared = prepare(pdf, options)?;
    let signed = sign(identity, &prepared.digest)?;
    let token = timestamp(&signed.signature)?;
    finish(prepared, identity, signed, token)
}

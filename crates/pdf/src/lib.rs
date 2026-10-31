//! Everything OpenSign does to PDF bytes. Uploads are untrusted: `sanitize` is the only function
//! that parses them, and it rewrites the file from scratch without active content.

mod cert;
mod detect;
mod evidence;
mod form;
mod geometry;
mod hidden;
mod ltv;
mod merge;
mod sanitize;
mod seal;
mod stamp;
mod tags;
mod text;
mod tsa;
mod verify;
mod x509;

#[cfg(test)]
mod ltv_tests;
#[cfg(test)]
mod tests;

pub use cert::{SealIdentity, generate_seal_identity};
pub use detect::{DetectedField, PageText, TextLine, detect_fields, page_text};
pub use evidence::{EvidenceAttachment, EvidenceEvent, EvidenceParty, EvidenceReport, append_evidence};
pub use form::{FormField, FormKind};
pub use geometry::{PageBox, extract_text, page_sizes};
pub use hidden::{HiddenText, hidden_text};
pub use ltv::{
    Revocation, RevocationCheck, RevocationStatus, ValidationData, ValidationPlan, add_validation_data, finish_document_timestamp,
    parse_issuer_certificates, plan as plan_validation, prepare_document_timestamp,
};
pub use merge::merge;
pub use sanitize::{Limits, Sanitized, sanitize};
pub use seal::{Prepared, SealOptions, Signed, finish, prepare, seal, sign};
pub use stamp::{Stamp, StampContent, check_signature_image, stamp};
pub use tags::{TagScan, TextTag, take_text_tags};
pub use tsa::verify_timestamp_token;
pub use tsa::{TimestampError, build_request as build_timestamp_request, parse_response as parse_timestamp_response, token_time};
pub use verify::{SignatureCheck, verify_signatures};

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("{0}")]
    Rejected(String),
    #[error("the PDF could not be read: {0}")]
    Malformed(String),
    #[error("signing failed: {0}")]
    Signing(String),
    #[error("timestamping failed: {0}")]
    Timestamp(String),
}

impl From<lopdf::Error> for PdfError {
    fn from(error: lopdf::Error) -> Self {
        PdfError::Malformed(error.to_string())
    }
}

pub type Result<T, E = PdfError> = std::result::Result<T, E>;

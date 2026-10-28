use std::str::FromStr;
use std::time::Duration;

use der::{Decode, Encode};
use p256::ecdsa::{DerSignature, SigningKey};
use p256::pkcs8::{DecodePrivateKey, EncodePrivateKey};
use spki::SubjectPublicKeyInfoOwned;
use x509_cert::Certificate;
use x509_cert::builder::{Builder, CertificateBuilder, Profile};
use x509_cert::ext::pkix::ExtendedKeyUsage;
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::time::Validity;

use crate::{PdfError, Result};

const ID_KP_DOCUMENT_SIGNING: const_oid::ObjectIdentifier = const_oid::ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.3.36");

/// The key and certificate chain that seal completed documents. The chain starts with the seal
/// certificate itself; an imported certificate brings its intermediates.
pub struct SealIdentity {
    pub key: SigningKey,
    pub chain: Vec<Certificate>,
}

impl SealIdentity {
    pub fn certificate(&self) -> &Certificate {
        &self.chain[0]
    }

    /// PKCS#8 private key and DER certificates, for encrypted storage.
    pub fn to_parts(&self) -> Result<(Vec<u8>, Vec<Vec<u8>>)> {
        let key = self.key.to_pkcs8_der().map_err(|e| PdfError::Signing(e.to_string()))?.as_bytes().to_vec();
        let chain = self.chain.iter().map(|c| c.to_der().map_err(|e| PdfError::Signing(e.to_string()))).collect::<Result<_>>()?;
        Ok((key, chain))
    }

    pub fn from_parts(key: &[u8], chain: &[Vec<u8>]) -> Result<Self> {
        let key = SigningKey::from_pkcs8_der(key).map_err(|e| PdfError::Signing(format!("seal key: {e}")))?;
        let chain: Vec<Certificate> = chain
            .iter()
            .map(|c| Certificate::from_der(c).map_err(|e| PdfError::Signing(format!("seal certificate: {e}"))))
            .collect::<Result<_>>()?;
        if chain.is_empty() {
            return Err(PdfError::Signing("the seal has no certificate".into()));
        }
        let spki = SubjectPublicKeyInfoOwned::from_key(*key.verifying_key()).map_err(|e| PdfError::Signing(e.to_string()))?;
        if chain[0].tbs_certificate.subject_public_key_info != spki {
            return Err(PdfError::Signing("the seal certificate does not belong to the seal key".into()));
        }
        Ok(Self { key, chain })
    }
}

fn escape_rdn(value: &str) -> String {
    value.chars().filter(|c| !c.is_control()).map(|c| if ",+\"\\<>;=#".contains(c) { format!("\\{c}") } else { c.to_string() }).collect()
}

/// A self-signed ECDSA P-256 seal for the organisation, valid for `years`. Readers will show it as
/// not yet trusted until it is added to their trust list; importing a certificate from a trusted
/// CA (AATL or an EU trust list) avoids that.
pub fn generate_seal_identity(organisation: &str, country: &str, years: u64) -> Result<SealIdentity> {
    let key = SigningKey::random(&mut rand::rngs::OsRng);
    let org = escape_rdn(if organisation.trim().is_empty() { "OpenSign" } else { organisation.trim() });
    let country: String = country.chars().filter(char::is_ascii_alphabetic).take(2).collect::<String>().to_uppercase();
    let mut subject = format!("CN={org} Document Seal,O={org}");
    if country.len() == 2 {
        subject.push_str(&format!(",C={country}"));
    }
    let subject = Name::from_str(&subject).map_err(|e| PdfError::Signing(e.to_string()))?;
    let mut serial = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut serial);
    serial[0] &= 0x7f;
    let serial = SerialNumber::new(&serial).map_err(|e| PdfError::Signing(e.to_string()))?;
    let validity =
        Validity::from_now(Duration::from_secs(years.clamp(1, 20) * 365 * 24 * 3600)).map_err(|e| PdfError::Signing(e.to_string()))?;
    let spki = SubjectPublicKeyInfoOwned::from_key(*key.verifying_key()).map_err(|e| PdfError::Signing(e.to_string()))?;
    let profile = Profile::Leaf { issuer: subject.clone(), enable_key_agreement: false, enable_key_encipherment: false };
    let mut builder =
        CertificateBuilder::new(profile, serial, validity, subject, spki, &key).map_err(|e| PdfError::Signing(e.to_string()))?;
    builder.add_extension(&ExtendedKeyUsage(vec![ID_KP_DOCUMENT_SIGNING])).map_err(|e| PdfError::Signing(e.to_string()))?;
    let certificate = builder.build::<DerSignature>().map_err(|e| PdfError::Signing(e.to_string()))?;
    Ok(SealIdentity { key, chain: vec![certificate] })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn generates_and_round_trips() {
        let identity = generate_seal_identity("Acme, AB", "se", 5).unwrap();
        let subject = identity.certificate().tbs_certificate.subject.to_string();
        assert!(subject.contains("Acme\\, AB"), "{subject}");
        let (key, chain) = identity.to_parts().unwrap();
        let back = SealIdentity::from_parts(&key, &chain).unwrap();
        assert_eq!(back.certificate(), identity.certificate());
        let other = generate_seal_identity("Other", "SE", 1).unwrap();
        let (other_key, _) = other.to_parts().unwrap();
        assert!(SealIdentity::from_parts(&other_key, &chain).is_err());
    }
}

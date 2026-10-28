//! Signature checks for the algorithms CAs, OCSP responders and time stamp authorities actually
//! use, and the certificate lookups that long-term validation and verification share.

use const_oid::ObjectIdentifier;
use const_oid::db::rfc5912::{
    ECDSA_WITH_SHA_256, ECDSA_WITH_SHA_384, ECDSA_WITH_SHA_512, ID_AD_CA_ISSUERS, ID_AD_OCSP, ID_EC_PUBLIC_KEY, ID_SHA_256, ID_SHA_384,
    ID_SHA_512, RSA_ENCRYPTION, SECP_256_R_1, SECP_384_R_1, SHA_256_WITH_RSA_ENCRYPTION, SHA_384_WITH_RSA_ENCRYPTION,
    SHA_512_WITH_RSA_ENCRYPTION,
};
use der::Encode;
use rsa::pkcs1::DecodeRsaPublicKey;
use rsa::traits::PublicKeyParts;
use sha2::{Digest, Sha256, Sha384, Sha512};
use signature::hazmat::PrehashVerifier;
use spki::SubjectPublicKeyInfoOwned;
use x509_cert::Certificate;
use x509_cert::ext::pkix::name::{DistributionPointName, GeneralName};
use x509_cert::ext::pkix::{AuthorityInfoAccessSyntax, CrlDistributionPoints, ExtendedKeyUsage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hash {
    Sha256,
    Sha384,
    Sha512,
}

impl Hash {
    pub fn from_oid(oid: &ObjectIdentifier) -> Option<Self> {
        match *oid {
            ID_SHA_256 => Some(Self::Sha256),
            ID_SHA_384 => Some(Self::Sha384),
            ID_SHA_512 => Some(Self::Sha512),
            _ => None,
        }
    }

    pub fn digest(self, data: &[u8]) -> Vec<u8> {
        match self {
            Self::Sha256 => Sha256::digest(data).to_vec(),
            Self::Sha384 => Sha384::digest(data).to_vec(),
            Self::Sha512 => Sha512::digest(data).to_vec(),
        }
    }
}

/// Verifies `signature` over `message`. `digest_alg` is the CMS digest algorithm, needed when
/// the signature algorithm is plain `rsaEncryption` or ECDSA without a hash. RSA keys under
/// 2048 bits, SHA-1 and unknown algorithms are refused.
pub(crate) fn verify_signature(
    key: &SubjectPublicKeyInfoOwned,
    algorithm: &ObjectIdentifier,
    digest_alg: Option<&ObjectIdentifier>,
    message: &[u8],
    signature: &[u8],
) -> bool {
    let from_digest = || digest_alg.and_then(Hash::from_oid);
    let (hash, rsa) = match *algorithm {
        SHA_256_WITH_RSA_ENCRYPTION => (Some(Hash::Sha256), true),
        SHA_384_WITH_RSA_ENCRYPTION => (Some(Hash::Sha384), true),
        SHA_512_WITH_RSA_ENCRYPTION => (Some(Hash::Sha512), true),
        RSA_ENCRYPTION => (from_digest(), true),
        ECDSA_WITH_SHA_256 => (Some(Hash::Sha256), false),
        ECDSA_WITH_SHA_384 => (Some(Hash::Sha384), false),
        ECDSA_WITH_SHA_512 => (Some(Hash::Sha512), false),
        ID_EC_PUBLIC_KEY => (from_digest(), false),
        _ => (None, false),
    };
    let Some(hash) = hash else { return false };
    let digest = hash.digest(message);
    let raw = key.subject_public_key.raw_bytes();
    if rsa {
        if key.algorithm.oid != RSA_ENCRYPTION {
            return false;
        }
        let Ok(public) = rsa::RsaPublicKey::from_pkcs1_der(raw) else { return false };
        if public.size() < 256 {
            return false;
        }
        let scheme = match hash {
            Hash::Sha256 => rsa::Pkcs1v15Sign::new::<Sha256>(),
            Hash::Sha384 => rsa::Pkcs1v15Sign::new::<Sha384>(),
            Hash::Sha512 => rsa::Pkcs1v15Sign::new::<Sha512>(),
        };
        return public.verify(scheme, &digest, signature).is_ok();
    }
    if key.algorithm.oid != ID_EC_PUBLIC_KEY {
        return false;
    }
    match key.algorithm.parameters.as_ref().and_then(|p| p.decode_as::<ObjectIdentifier>().ok()) {
        Some(SECP_256_R_1) => {
            let (Ok(public), Ok(sig)) = (p256::ecdsa::VerifyingKey::from_sec1_bytes(raw), p256::ecdsa::Signature::from_der(signature))
            else {
                return false;
            };
            public.verify_prehash(&digest, &sig).is_ok()
        }
        Some(SECP_384_R_1) => {
            let (Ok(public), Ok(sig)) = (p384::ecdsa::VerifyingKey::from_sec1_bytes(raw), p384::ecdsa::Signature::from_der(signature))
            else {
                return false;
            };
            public.verify_prehash(&digest, &sig).is_ok()
        }
        _ => false,
    }
}

/// True when `issuer` is named as the issuer of `cert` and its key verifies the certificate.
pub(crate) fn issued_by(cert: &Certificate, issuer: &Certificate) -> bool {
    cert.tbs_certificate.issuer == issuer.tbs_certificate.subject
        && cert.tbs_certificate.to_der().is_ok_and(|tbs| {
            verify_signature(
                &issuer.tbs_certificate.subject_public_key_info,
                &cert.signature_algorithm.oid,
                None,
                &tbs,
                cert.signature.raw_bytes(),
            )
        })
}

pub(crate) fn is_self_signed(cert: &Certificate) -> bool {
    issued_by(cert, cert)
}

pub(crate) fn has_extended_key_usage(cert: &Certificate, usage: ObjectIdentifier) -> bool {
    matches!(cert.tbs_certificate.get::<ExtendedKeyUsage>(), Ok(Some((_, eku))) if eku.0.contains(&usage))
}

pub(crate) fn has_extension(cert: &Certificate, oid: ObjectIdentifier) -> bool {
    cert.tbs_certificate.extensions.iter().flatten().any(|e| e.extn_id == oid)
}

fn http_url(name: &GeneralName) -> Option<String> {
    match name {
        GeneralName::UniformResourceIdentifier(uri) => {
            let uri = uri.as_str();
            (uri.starts_with("http://") || uri.starts_with("https://")).then(|| uri.to_string())
        }
        _ => None,
    }
}

fn access_urls(cert: &Certificate, method: ObjectIdentifier) -> Vec<String> {
    match cert.tbs_certificate.get::<AuthorityInfoAccessSyntax>() {
        Ok(Some((_, aia))) => aia.0.iter().filter(|a| a.access_method == method).filter_map(|a| http_url(&a.access_location)).collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn ocsp_urls(cert: &Certificate) -> Vec<String> {
    access_urls(cert, ID_AD_OCSP)
}

pub(crate) fn ca_issuer_urls(cert: &Certificate) -> Vec<String> {
    access_urls(cert, ID_AD_CA_ISSUERS)
}

pub(crate) fn crl_urls(cert: &Certificate) -> Vec<String> {
    match cert.tbs_certificate.get::<CrlDistributionPoints>() {
        Ok(Some((_, points))) => points
            .0
            .iter()
            .filter_map(|p| match &p.distribution_point {
                Some(DistributionPointName::FullName(names)) => Some(names.iter().filter_map(http_url).collect::<Vec<_>>()),
                _ => None,
            })
            .flatten()
            .collect(),
        _ => Vec::new(),
    }
}

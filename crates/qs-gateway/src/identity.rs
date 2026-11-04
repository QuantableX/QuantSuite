//! Keys and certificates (PLAN-SERVER-CLIENT §4.1 layers 2–4).
//!
//! The server and every device have an Ed25519 key, kept in their vault. The
//! TLS certificate is made from that key each time and is self-signed: what
//! is trusted is the **public key** — the client pins the server's, the server
//! knows each paired device's. A fingerprint is the SHA-256 of the public key.

use ring::signature::{Ed25519KeyPair, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

/// The DER prefix of an Ed25519 SubjectPublicKeyInfo; the key's 32 bytes
/// follow it.
const ED25519_SPKI: [u8; 12] = [0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00];

pub struct Identity {
    pkcs8: Zeroizing<Vec<u8>>,
    public: [u8; 32],
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity").field("fingerprint", &self.fingerprint()).finish()
    }
}

impl Identity {
    /// A new key from the system's randomness.
    pub fn generate() -> Result<Self, String> {
        let rng = ring::rand::SystemRandom::new();
        let doc = Ed25519KeyPair::generate_pkcs8(&rng).map_err(|_| "no system randomness".to_string())?;
        Self::from_pkcs8(doc.as_ref())
    }

    pub fn from_pkcs8(pkcs8: &[u8]) -> Result<Self, String> {
        let pair = Ed25519KeyPair::from_pkcs8_maybe_unchecked(pkcs8).map_err(|_| "not an Ed25519 key".to_string())?;
        let mut public = [0u8; 32];
        public.copy_from_slice(pair.public_key().as_ref());
        Ok(Self { pkcs8: Zeroizing::new(pkcs8.to_vec()), public })
    }

    /// The key as stored in the vault.
    pub fn pkcs8(&self) -> &[u8] {
        &self.pkcs8
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.public
    }

    pub fn fingerprint(&self) -> String {
        fingerprint(&self.public)
    }

    /// A self-signed certificate of this key for a TLS handshake.
    pub fn certificate(&self, name: &str) -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>), String> {
        let key = rcgen::KeyPair::try_from(self.pkcs8.as_slice()).map_err(|e| format!("key: {e}"))?;
        let mut params = rcgen::CertificateParams::new(vec![name.to_string()]).map_err(|e| format!("certificate: {e}"))?;
        params.distinguished_name.push(rcgen::DnType::CommonName, name);
        let cert = params.self_signed(&key).map_err(|e| format!("certificate: {e}"))?;
        Ok((cert.der().clone(), PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.pkcs8.to_vec()))))
    }
}

/// `ab12 cd34 …` — 16 groups of four hex digits, easy to read aloud.
pub fn fingerprint(public: &[u8; 32]) -> String {
    let h = hex::encode(Sha256::digest(public));
    h.as_bytes().chunks(4).map(|c| std::str::from_utf8(c).unwrap_or("")).collect::<Vec<_>>().join(" ")
}

/// The Ed25519 public key a certificate carries.
pub fn public_key_of_cert(cert: &[u8]) -> Result<[u8; 32], String> {
    let at = cert
        .windows(ED25519_SPKI.len())
        .position(|w| w == ED25519_SPKI)
        .ok_or("the certificate's key is not Ed25519")?;
    let start = at + ED25519_SPKI.len();
    cert.get(start..start + 32)
        .and_then(|k| k.try_into().ok())
        .ok_or_else(|| "the certificate is truncated".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_certificate_carries_the_identitys_key() {
        let id = Identity::generate().unwrap();
        let (cert, _) = id.certificate("quantsuite.server").unwrap();
        assert_eq!(public_key_of_cert(cert.as_ref()).unwrap(), id.public_key());
        // Stored and loaded again, the same identity.
        let again = Identity::from_pkcs8(id.pkcs8()).unwrap();
        assert_eq!(again.fingerprint(), id.fingerprint());
        assert_eq!(id.fingerprint().split(' ').count(), 16);
        assert!(!format!("{id:?}").contains(&hex::encode(id.pkcs8())), "no key in Debug");
        assert_ne!(Identity::generate().unwrap().public_key(), id.public_key());
    }
}

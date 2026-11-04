//! TLS 1.3 with both keys (PLAN-SERVER-CLIENT §4.1 layers 2–3).
//!
//! Neither side uses certificate authorities: what is trusted is a **key**.
//! - The client pins the server's public key (learned at pairing, confirmed by
//!   the four words). A different key fails the handshake.
//! - The server asks every client for its certificate and checks that the
//!   client holds the key inside (the handshake signature). Whether that key is
//!   a paired device is decided right after, against the registry — an unpaired
//!   key may only pair.
//!
//! Only Ed25519, only TLS 1.3, no session resumption and no early data: every
//! connection is a full handshake with fresh keys, so a recorded handshake
//! cannot be replayed.

use crate::identity::{public_key_of_cert, Identity};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{CertificateError, ClientConfig, DigitallySignedStruct, DistinguishedName, Error, ServerConfig, SignatureScheme};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// The name inside the server's certificate and in the client's SNI. Not
/// checked: the key is.
pub const SERVER_NAME: &str = "quantsuite.server";

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn verify_ed25519(message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, Error> {
    if dss.scheme != SignatureScheme::ED25519 {
        return Err(Error::General("only Ed25519 keys are accepted".into()));
    }
    let key = public_key_of_cert(cert.as_ref()).map_err(|_| Error::InvalidCertificate(CertificateError::BadEncoding))?;
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key)
        .verify(message, dss.signature())
        .map_err(|_| Error::InvalidCertificate(CertificateError::BadSignature))?;
    Ok(HandshakeSignatureValid::assertion())
}

/// The server's view: any client that proves an Ed25519 key.
#[derive(Debug)]
struct DeviceKeys;

impl ClientCertVerifier for DeviceKeys {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(&self, end_entity: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: UnixTime) -> Result<ClientCertVerified, Error> {
        public_key_of_cert(end_entity.as_ref()).map_err(|_| Error::InvalidCertificate(CertificateError::BadEncoding))?;
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, Error> {
        Err(Error::General("TLS 1.2 is not offered".into()))
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, Error> {
        verify_ed25519(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}

/// The client's view: the pinned server key, or — while pairing — whatever
/// key the server shows (the user compares the words before it is pinned).
#[derive(Debug, Default)]
pub struct PinnedServer {
    pinned: Option<[u8; 32]>,
    seen: Mutex<Option<[u8; 32]>>,
    mismatch: AtomicBool,
}

impl PinnedServer {
    /// The server key of the last handshake.
    pub fn seen(&self) -> Option<[u8; 32]> {
        *self.seen.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The server showed a key other than the pinned one.
    pub fn mismatch(&self) -> bool {
        self.mismatch.load(Ordering::Relaxed)
    }
}

impl ServerCertVerifier for PinnedServer {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let key = public_key_of_cert(end_entity.as_ref()).map_err(|_| Error::InvalidCertificate(CertificateError::BadEncoding))?;
        if let Some(pinned) = self.pinned {
            if pinned != key {
                self.mismatch.store(true, Ordering::Relaxed);
                return Err(Error::InvalidCertificate(CertificateError::ApplicationVerificationFailure));
            }
        }
        *self.seen.lock().unwrap_or_else(|p| p.into_inner()) = Some(key);
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, Error> {
        Err(Error::General("TLS 1.2 is not offered".into()))
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, Error> {
        verify_ed25519(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}

pub fn server_config(identity: &Identity) -> Result<Arc<ServerConfig>, String> {
    let (cert, key) = identity.certificate(SERVER_NAME)?;
    let mut cfg = ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| format!("TLS: {e}"))?
        .with_client_cert_verifier(Arc::new(DeviceKeys))
        .with_single_cert(vec![cert], key)
        .map_err(|e| format!("TLS: {e}"))?;
    cfg.session_storage = Arc::new(rustls::server::NoServerSessionStorage {});
    cfg.send_tls13_tickets = 0;
    cfg.max_early_data_size = 0;
    Ok(Arc::new(cfg))
}

pub fn client_config(identity: &Identity, pinned: Option<[u8; 32]>) -> Result<(Arc<ClientConfig>, Arc<PinnedServer>), String> {
    let (cert, key) = identity.certificate("quantsuite.device")?;
    let verifier = Arc::new(PinnedServer { pinned, ..Default::default() });
    let mut cfg = ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| format!("TLS: {e}"))?
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_client_auth_cert(vec![cert], key)
        .map_err(|e| format!("TLS: {e}"))?;
    cfg.resumption = rustls::client::Resumption::disabled();
    cfg.enable_early_data = false;
    Ok((Arc::new(cfg), verifier))
}

/// The Ed25519 key a connection's peer proved.
pub fn peer_key(conn: &rustls::CommonState) -> Option<[u8; 32]> {
    conn.peer_certificates()?.first().and_then(|c| public_key_of_cert(c.as_ref()).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::{ClientConnection, ServerConnection};

    /// Moves bytes between two connections until both are done or one fails;
    /// returns every byte the client sent.
    fn handshake(c: &mut ClientConnection, s: &mut ServerConnection) -> Result<Vec<u8>, Error> {
        let mut sent = Vec::new();
        for _ in 0..20 {
            let mut buf = Vec::new();
            while c.wants_write() {
                c.write_tls(&mut buf).unwrap();
            }
            sent.extend_from_slice(&buf);
            let mut rd = &buf[..];
            while !rd.is_empty() {
                s.read_tls(&mut rd).unwrap();
                s.process_new_packets()?;
            }
            let mut buf = Vec::new();
            while s.wants_write() {
                s.write_tls(&mut buf).unwrap();
            }
            let mut rd = &buf[..];
            while !rd.is_empty() {
                c.read_tls(&mut rd).unwrap();
                c.process_new_packets()?;
            }
            if !c.is_handshaking() && !s.is_handshaking() {
                return Ok(sent);
            }
        }
        Err(Error::General("handshake did not finish".into()))
    }

    fn name() -> ServerName<'static> {
        ServerName::try_from(SERVER_NAME).unwrap()
    }

    #[test]
    fn both_sides_learn_the_others_key() {
        let (server, device) = (Identity::generate().unwrap(), Identity::generate().unwrap());
        let (cc, seen) = client_config(&device, Some(server.public_key())).unwrap();
        let mut c = ClientConnection::new(cc, name()).unwrap();
        let mut s = ServerConnection::new(server_config(&server).unwrap()).unwrap();
        handshake(&mut c, &mut s).unwrap();
        assert_eq!(peer_key(&s), Some(device.public_key()));
        assert_eq!(seen.seen(), Some(server.public_key()));
        assert_eq!(c.protocol_version(), Some(rustls::ProtocolVersion::TLSv1_3));
    }

    #[test]
    fn a_wrong_server_key_is_refused() {
        let (server, device, other) = (Identity::generate().unwrap(), Identity::generate().unwrap(), Identity::generate().unwrap());
        let (cc, seen) = client_config(&device, Some(other.public_key())).unwrap();
        let mut c = ClientConnection::new(cc, name()).unwrap();
        let mut s = ServerConnection::new(server_config(&server).unwrap()).unwrap();
        assert!(handshake(&mut c, &mut s).is_err());
        assert!(seen.mismatch());
        assert_eq!(seen.seen(), None);
    }

    #[test]
    fn a_recorded_handshake_cannot_be_replayed() {
        let (server, device) = (Identity::generate().unwrap(), Identity::generate().unwrap());
        let scfg = server_config(&server).unwrap();
        let (cc, _) = client_config(&device, Some(server.public_key())).unwrap();
        let mut c = ClientConnection::new(cc, name()).unwrap();
        let mut s = ServerConnection::new(scfg.clone()).unwrap();
        let recorded = handshake(&mut c, &mut s).unwrap();

        // Someone who recorded the client's bytes plays them to the server
        // again: the server's fresh key share makes the client's encrypted
        // flight (certificate, signature, Finished) undecryptable.
        let mut replay = ServerConnection::new(scfg).unwrap();
        let mut rd = &recorded[..];
        let mut failed = false;
        while !rd.is_empty() {
            if replay.read_tls(&mut rd).is_err() || replay.process_new_packets().is_err() {
                failed = true;
                break;
            }
        }
        assert!(failed || replay.is_handshaking(), "the replay must not complete a handshake");
        assert!(peer_key(&replay).is_none(), "and must not authenticate the device");
    }
}

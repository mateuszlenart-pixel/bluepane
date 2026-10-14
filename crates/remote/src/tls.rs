//! Trust for FTPS servers: the certificate is pinned like an SSH host key. Nothing is
//! checked against certificate authorities (home servers use self-signed certificates);
//! instead the first contact shows the certificate's SHA-256 fingerprint and asks, and a
//! later change of the certificate is refused.

use std::sync::{Arc, Mutex};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{ring, verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, Error, SignatureScheme};
use sha2::{Digest, Sha256};

use crate::known_hosts::{HostKeyStatus, KnownHosts};

/// The "algorithm" under which a pinned FTPS certificate is stored in the known hosts.
pub(crate) const CERT_KIND: &str = "tls-certificate";

/// What the verifier saw, so a refused connection can say why.
#[derive(Debug, Clone)]
pub(crate) struct Observed {
    pub fingerprint: String,
    pub status: HostKeyStatus,
}

#[derive(Debug)]
struct Pin {
    host: String,
    port: u16,
    known: KnownHosts,
    trust: Option<String>,
    observed: Arc<Mutex<Option<Observed>>>,
    provider: Arc<CryptoProvider>,
}

pub(crate) fn fingerprint_of(certificate: &[u8]) -> String {
    let digest = Sha256::digest(certificate);
    let hex: Vec<String> = digest.iter().map(|b| format!("{b:02X}")).collect();
    format!("SHA256:{}", hex.join(":"))
}

impl ServerCertVerifier for Pin {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let fingerprint = fingerprint_of(end_entity.as_ref());
        let status = self
            .known
            .status(&self.host, self.port, CERT_KIND, &fingerprint);
        let trusted = match &status {
            HostKeyStatus::Known => true,
            HostKeyStatus::Unknown => self.trust.as_deref() == Some(fingerprint.as_str()),
            HostKeyStatus::Changed { .. } => false,
        };
        *self.observed.lock().unwrap_or_else(|e| e.into_inner()) = Some(Observed {
            fingerprint,
            status,
        });
        if trusted {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(Error::General("niezaufany certyfikat serwera".into()))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// What a TLS check leaves behind for the caller to read.
pub(crate) type SharedObserved = Arc<Mutex<Option<Observed>>>;

/// A TLS client configuration that trusts exactly the pinned (or just confirmed) certificate.
pub(crate) fn pinned_config(
    host: &str,
    port: u16,
    known: &KnownHosts,
    trust: Option<&str>,
) -> Result<(Arc<ClientConfig>, SharedObserved), Error> {
    let provider = Arc::new(ring::default_provider());
    let observed = Arc::new(Mutex::new(None));
    let pin = Pin {
        host: host.to_string(),
        port,
        known: known.clone(),
        trust: trust.map(str::to_string),
        observed: observed.clone(),
        provider: provider.clone(),
    };
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(pin))
        .with_no_client_auth();
    Ok((Arc::new(config), observed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_are_stable_and_readable() {
        let a = fingerprint_of(b"certificate bytes");
        assert!(a.starts_with("SHA256:"));
        assert_eq!(a, fingerprint_of(b"certificate bytes"));
        assert_ne!(a, fingerprint_of(b"other"));
        assert_eq!(a.matches(':').count(), 32, "SHA256: plus 32 hex pairs");
    }
}

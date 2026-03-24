use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand_core::{OsRng, RngCore};
use serde::Serialize;
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

#[derive(Clone)]
pub struct IdentityMaterial {
    pub signing_key: SigningKey,
    pub encryption_secret: StaticSecret,
}

impl IdentityMaterial {
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let encryption_secret = StaticSecret::random_from_rng(OsRng);
        Self {
            signing_key,
            encryption_secret,
        }
    }

    pub fn from_b64(signing_secret_key: &str, encryption_secret_key: &str) -> Result<Self> {
        let signing_bytes: [u8; 32] = B64
            .decode(signing_secret_key)
            .context("decode signing secret")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid signing secret length"))?;
        let enc_bytes: [u8; 32] = B64
            .decode(encryption_secret_key)
            .context("decode encryption secret")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid encryption secret length"))?;
        Ok(Self {
            signing_key: SigningKey::from_bytes(&signing_bytes),
            encryption_secret: StaticSecret::from(enc_bytes),
        })
    }

    pub fn signing_secret_b64(&self) -> String {
        B64.encode(self.signing_key.to_bytes())
    }

    pub fn signing_public_b64(&self) -> String {
        B64.encode(self.signing_key.verifying_key().to_bytes())
    }

    pub fn encryption_secret_b64(&self) -> String {
        B64.encode(self.encryption_secret.to_bytes())
    }

    pub fn encryption_public_b64(&self) -> String {
        B64.encode(X25519PublicKey::from(&self.encryption_secret).as_bytes())
    }

    pub fn peer_id(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.signing_key.verifying_key().to_bytes());
        hex::encode(hasher.finalize())
    }

    pub fn decrypt(
        &self,
        ciphertext_b64: &str,
        nonce_b64: &str,
        ephemeral_public_key_b64: &str,
    ) -> Result<Vec<u8>> {
        let ciphertext = B64.decode(ciphertext_b64).context("decode ciphertext")?;
        let nonce_bytes = B64.decode(nonce_b64).context("decode nonce")?;
        let nonce: [u8; 24] = nonce_bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid nonce length"))?;
        let eph_bytes: [u8; 32] = B64
            .decode(ephemeral_public_key_b64)
            .context("decode ephemeral public key")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid ephemeral public key length"))?;
        let eph = X25519PublicKey::from(eph_bytes);
        let shared = self.encryption_secret.diffie_hellman(&eph);
        let digest = Sha256::digest(shared.as_bytes());
        let key = Key::from_slice(&digest);
        let cipher = XChaCha20Poly1305::new(key);
        cipher
            .decrypt(XNonce::from_slice(&nonce), ciphertext.as_ref())
            .map_err(|_| anyhow::anyhow!("decrypt failed"))
    }
}

pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(value).context("serialize canonical json")
}

pub fn sign_payload<T: Serialize>(signing_key: &SigningKey, value: &T) -> Result<String> {
    let bytes = canonical_json(value)?;
    Ok(B64.encode(signing_key.sign(&bytes).to_bytes()))
}

pub fn verify_signature<T: Serialize>(
    public_key_b64: &str,
    signature_b64: &str,
    value: &T,
) -> Result<()> {
    let public_bytes: [u8; 32] = B64
        .decode(public_key_b64)
        .context("decode signing public key")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid signing public key length"))?;
    let verifying =
        VerifyingKey::from_bytes(&public_bytes).context("invalid signing public key")?;
    let sig_bytes: [u8; 64] = B64
        .decode(signature_b64)
        .context("decode signature")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid signature length"))?;
    let signature = Signature::from_bytes(&sig_bytes);
    let bytes = canonical_json(value)?;
    verifying
        .verify(&bytes, &signature)
        .map_err(|_| anyhow::anyhow!("invalid signature"))
}

pub fn encrypt_for_peer(
    body: &serde_json::Value,
    encryption_public_key_b64: &str,
) -> Result<(String, String, String, String)> {
    let plaintext = serde_json::to_vec(body)?;
    let recipient_bytes: [u8; 32] = B64
        .decode(encryption_public_key_b64)
        .context("decode peer encryption public key")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid peer encryption public key length"))?;
    let recipient = X25519PublicKey::from(recipient_bytes);
    let ephemeral = StaticSecret::random_from_rng(OsRng);
    let ephemeral_public = X25519PublicKey::from(&ephemeral);
    let shared = ephemeral.diffie_hellman(&recipient);
    let digest = Sha256::digest(shared.as_bytes());
    let key = Key::from_slice(&digest);
    let cipher = XChaCha20Poly1305::new(key);
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), plaintext.as_ref())
        .map_err(|_| anyhow::anyhow!("encrypt failed"))?;
    Ok((
        B64.encode(ciphertext),
        B64.encode(nonce),
        B64.encode(ephemeral_public.as_bytes()),
        hex::encode(Sha256::digest(&plaintext)),
    ))
}

pub fn derive_peer_id(signing_public_key_b64: &str) -> Result<String> {
    let public_bytes = B64
        .decode(signing_public_key_b64)
        .context("decode signing public key")?;
    if public_bytes.len() != 32 {
        bail!("invalid signing public key length");
    }
    Ok(hex::encode(Sha256::digest(public_bytes)))
}

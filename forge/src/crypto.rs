//! Engine-free digest core.
//!
//! The scripting-engine-independent half of `crypto.subtle.digest`: the
//! algorithm table and the hash itself. It names no scripting-engine types;
//! the marshalling layer (`flux/src/standards_plugins/crypto.rs`) decodes the
//! algorithm and the input buffer and wraps the result in a promise.

use sha2::Digest;

/// Hash `data` with `algorithm`, matched case-insensitively against the Web
/// Crypto names. SHA-256, SHA-384 and SHA-512 are supported; anything else
/// (SHA-1 included) is an error naming the supported set.
pub fn digest(algorithm: &str, data: &[u8]) -> Result<Vec<u8>, String> {
  match algorithm.to_uppercase().as_str() {
    "SHA-256" => Ok(sha2::Sha256::digest(data).to_vec()),
    "SHA-384" => Ok(sha2::Sha384::digest(data).to_vec()),
    "SHA-512" => Ok(sha2::Sha512::digest(data).to_vec()),
    other => Err(format!("unsupported algorithm \"{other}\" (SHA-256, SHA-384, SHA-512)")),
  }
}

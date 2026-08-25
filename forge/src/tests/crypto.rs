use crate::crypto::digest;

fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn digest_known_vectors() {
  assert_eq!(
    hex(&digest("SHA-256", b"abc").expect("sha-256")),
    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
  );
  assert_eq!(digest("sha-384", b"").expect("sha-384").len(), 48);
  assert_eq!(digest("SHA-512", b"abc").expect("sha-512").len(), 64);
}

#[test]
fn digest_rejects_unsupported() {
  let err = digest("SHA-1", b"").expect_err("sha-1 is not supported");
  assert_eq!(err, "unsupported algorithm \"SHA-1\" (SHA-256, SHA-384, SHA-512)");
}

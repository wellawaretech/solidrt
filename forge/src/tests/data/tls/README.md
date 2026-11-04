# TLS test certificates

The fixtures of the TLS tests: the forge tests (`forge/src/tests/tls.rs`)
and the flux module tests (`flux/tests/net-tls.test.ts`) read the same
files. All keys are ECDSA P-256 in PKCS#8, and every certificate is valid
for 100 years. They are test material only: the private keys are public.

- `ca.pem`: a private CA. `ca-key.pem` is its key, kept so a new leaf can
  be signed by the same CA.
- `server.pem`, `server-key.pem`: a server leaf for `localhost` and
  `127.0.0.1`, signed by the CA.
- `client.pem`, `client-key.pem`: a client leaf, signed by the CA.
- `self-signed.pem`, `self-signed-key.pem`: a self-signed server for
  `localhost` and `127.0.0.1`, for the pin cases.
- `self-signed.pin`: the SHA-256 of the self-signed server's
  SubjectPublicKeyInfo, as lowercase hex (the form of `pin` and `peerKey`).
- `client.pin`: the same hash for the client leaf.

To make the set again, run these commands in this folder:

```sh
DAYS=36500
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out ca-key.pem
openssl req -x509 -new -key ca-key.pem -sha256 -days $DAYS -subj "/CN=solidrt test CA" \
  -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign" \
  -out ca.pem

openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out server-key.pem
openssl req -new -key server-key.pem -subj "/CN=localhost" -out server.csr
openssl x509 -req -in server.csr -CA ca.pem -CAkey ca-key.pem -sha256 -days $DAYS \
  -extfile <(printf "basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost,IP:127.0.0.1") \
  -out server.pem

openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out client-key.pem
openssl req -new -key client-key.pem -subj "/CN=solidrt test client" -out client.csr
openssl x509 -req -in client.csr -CA ca.pem -CAkey ca-key.pem -sha256 -days $DAYS \
  -extfile <(printf "basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=clientAuth") \
  -out client.pem

openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out self-signed-key.pem
openssl req -x509 -new -key self-signed-key.pem -sha256 -days $DAYS -subj "/CN=localhost" \
  -addext "basicConstraints=critical,CA:FALSE" -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=serverAuth" -addext "subjectAltName=DNS:localhost,IP:127.0.0.1" \
  -out self-signed.pem

rm server.csr client.csr
for name in self-signed client; do
  openssl x509 -in $name.pem -pubkey -noout | openssl pkey -pubin -outform DER \
    | openssl dgst -sha256 -hex | sed 's/.*= //' | tr -d '\n' > $name.pin
done
```

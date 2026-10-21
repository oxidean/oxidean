-- OIDC RS256 keys (cloud-provider interop): GCP STS / AWS STS require RSA or
-- EC keys — Ed25519 (`alg='EdDSA'`) is not accepted by either. New keys are
-- written with `alg='RS256'`; the signing private key lives in `private_enc`
-- as Actions-secrets ciphertext (same crypto as repo secrets), never
-- plaintext. `private_der` stays for legacy Ed25519 rows, which stop signing
-- and leave the JWKS set entirely — in-flight EdDSA tokens age out.
ALTER TABLE action_oidc_keys ADD COLUMN private_enc TEXT;

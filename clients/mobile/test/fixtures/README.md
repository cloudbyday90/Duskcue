# Relay TLS fixture

The localhost certificate and unencrypted private key in this directory are public, test-only fixtures. They protect no deployment or user data. The certificate names only `localhost`; tests load it into an isolated `SecurityContext`, never into platform trust. Production relay connections retain the SDK's normal trust roots and hostname validation.

The certificate is valid from January 2026 through January 2036. The key is deliberately committed so tests can start a reproducible local TLS endpoint. Do not reuse it outside qualification.

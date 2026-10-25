# Feed TLS qualification fixtures

These certificates and private keys are public test material, never deployment credentials. `scripts/generate-iptv-feed-fixtures.ps1` creates them in memory with .NET cryptography and writes DER files without accessing certificate stores. The CA private key is not saved. Regeneration creates new keys and replaces the fixture set together.

The valid leaf has DNS SAN `duskcue-feed.invalid`, IPv4 loopback and IPv6 loopback SANs, server-auth usage and validity from January 2025 through January 2039. The expired leaf has the same identities and validity from January 2021 through January 2022. The fixture CA spans January 2020 through January 2040. Tests must fail rather than bypass time validation when the valid set eventually expires.

Only the default-off feed qualification module's explicit fixed-fixture constructor trusts this CA. Its normal constructor uses public WebPKI roots and must reject this fixture chain. Production has no arbitrary private-CA configuration or insecure verifier option in this increment.

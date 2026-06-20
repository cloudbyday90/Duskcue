# Reverse Proxy

## Overview

This document is the authoritative design for reverse proxy deployment in front of Duskcue — the recommended external software for TLS termination, request routing, header forwarding, and SSE/WebSocket proxying in exposed mode. The goal is to give operators a single recommended path with copy-paste configuration, while documenting alternatives for those with existing infrastructure.

The decision documented here: **Duskcue's built-in rustls TLS termination handles local and simple exposed deployments with zero extra software.** For multi-service routing, HTTP/3, sophisticated access control, or operator preference, **Caddy is the recommended external reverse proxy** (matching the Jellyfin pattern); Nginx and Traefik are documented as alternatives. Duskcue is NOT bundled with an embedded reverse proxy — the container stays lean, and operators with existing proxies reuse them.

## Scope

**Covers:**

- TLS termination strategy (Duskcue's built-in rustls vs external proxy)
- Recommended external reverse proxy (Caddy) and rationale
- Alternative proxies (Nginx, Traefik) — when to choose them instead
- TLS certificate automation (ACME / Let's Encrypt; HTTP-01 vs DNS-01)
- Critical header forwarding (`X-Forwarded-For`, `X-Forwarded-Proto`, `X-Real-IP`) for client IP detection and security
- SSE proxying (already partially decided in [REAL_TIME_PUSH.md](REAL_TIME_PUSH.md); unified here)
- Trusted-proxy configuration — defending against `X-Forwarded-For` spoofing
- Cloudflare TOS issue and recommended alternatives
- HTTP/3 (QUIC) support
- Embedded-in-container vs external-sidecar topology decision

**Does NOT cover:**

- Duskcue's internal TLS stack — see [SECURITY.md](../security/SECURITY.md) (rustls, ACME auto-cert, cert storage location)
- Docker single-container topology — see [DOCKER_DEPLOYMENT.md](../operations/DOCKER_DEPLOYMENT.md) and [MULTI_INSTANCE.md](MULTI_INSTANCE.md)
- Load balancing across multiple instances — ruled out by [MULTI_INSTANCE.md](MULTI_INSTANCE.md) (single-instance by design)
- Web Application Firewall (WAF) rules — operator's choice; out of scope for Duskcue

## Decision — Built-in TLS for Simple Deployments; Caddy Recommended for Multi-Service Routing

**Duskcue ships with its own rustls-based TLS termination** ([SECURITY.md](../security/SECURITY.md)). This handles two deployment patterns without any external software:

1. **Local mode** — HTTP on `localhost:48027`, no TLS needed (LAN-only)
2. **Simple exposed mode** — Duskcue binds `:443` directly with its own rustls TLS, ACME-managed cert via Let's Encrypt. No reverse proxy required.

For more complex exposed-mode deployments (multiple services on one domain, HTTP/3, WAF, sophisticated routing, operator preference for an existing proxy), **Caddy is the recommended external reverse proxy**. This matches the [Jellyfin reverse-proxy documentation](https://jellyfin.org/docs/general/post-install/networking/reverse-proxy/) recommendation and the broader self-hosted community consensus in 2026.

### Why Caddy Is Recommended

| Concern | Caddy 2.11 | Nginx 1.28 | Traefik v3.6 |
|---|---|---|---|
| Automatic HTTPS (ACME) | ✅ Built-in, zero config beyond domain | ❌ Requires Certbot sidecar | ✅ Built-in |
| Config simplicity | ✅ Caddyfile is 4 lines for basic reverse proxy | ⚠️ Many directives; steep learning curve | ⚠️ Docker labels + static config; medium learning curve |
| Memory footprint | ✅ ~14 MB idle | ✅ ~5 MB idle (Certbot adds ~25 MB) | ⚠️ ~17 MB idle |
| Docker image size | ✅ ~88 MB | ~240 MB (Certbot adds ~298 MB) | ~242 MB |
| HTTP/3 (QUIC) | ✅ Default-on | ⚠️ Third-party module | ⚠️ Experimental |
| Docker socket required | ❌ No | ❌ No | ⚠️ Yes (security concern; mitigated by socket-proxy) |
| Debugging ease | ✅ Straightforward logs | ⚠️ error.log + access.log | ❌ Notoriously difficult |
| Auto-discovery (Docker labels) | ❌ Manual Caddyfile (or via `caddy-docker-proxy` plugin) | ❌ Manual conf files | ✅ Built-in |
| Reload without downtime | ✅ `caddy reload` | ✅ `nginx -s reload` | ✅ Automatic on container events |
| Peer recommendation | ✅ Jellyfin officially recommends Caddy | Documented alternative | Documented alternative |

**Key reasons Caddy wins for Duskcue's deployment target:**

1. **Single-instance Duskcue doesn't need Traefik's auto-discovery** — Per [MULTI_INSTANCE.md](MULTI_INSTANCE.md), Duskcue is single-instance. Auto-discovery is overkill; a static Caddyfile is simpler and more debuggable.
2. **Smallest image + lowest config ceremony** — Operators on NAS hardware benefit from Caddy's small footprint and intuitive Caddyfile.
3. **No Docker socket exposure** — Traefik requires `/var/run/docker.sock` (security concern; mitigated by socket-proxy but adds complexity). Caddy and Nginx don't.
4. **Jellyfin's recommendation aligns the market** — Duskcue users migrating from Jellyfin (likely Phase 14 use case) already have Caddy configured. Reusing the existing Caddy setup is one-line change.
5. **HTTP/3 default-on** — Caddy serves HTTP/3 to clients that support it without any config. Improves mobile performance.

### Why Not Embed Caddy in the Duskcue Container

Tempting option: bundle Caddy inside Duskcue's Docker image so users get zero-config HTTPS in a single container. **Rejected** for several reasons:

1. **Duskcue already has built-in rustls TLS termination** — For simple exposed mode (single domain, no routing), Duskcue binds `:443` directly. Adding Caddy would be redundant for the simple case.
2. **Container bloat** — Caddy adds ~88 MB to the image. The current Alpine-based Duskcue image is small; bundling Caddy inflates it materially.
3. **Lifecycle coupling** — Restarting Duskcue shouldn't restart the proxy (and sever other services routed through it). Decoupled external proxy survives app restarts.
4. **Operators with existing proxies** — Most self-hosters run multiple services (Jellyfin, Home Assistant, Nextcloud, etc.) behind one reverse proxy. An embedded Caddy forces a second proxy layer or conflicts with the existing one.
5. **Configuration flexibility** — External Caddy supports any routing, header, or WAF rule the operator wants. Embedded Caddy would have to expose those as Duskcue config fields, doubling the configuration surface.
6. **Single-container model preserved** — Per [MULTI_INSTANCE.md](MULTI_INSTANCE.md), Duskcue's canonical topology is single-container. That container is the Duskcue binary + embedded PostgreSQL. The proxy is a separate concern.

## Deployment Patterns

### Pattern 1: Local Mode (No Proxy)

```
┌────────────────────────────────────────┐
│ Duskcue Docker container               │
│ ┌──────────┐  ┌──────────────────────┐ │
│ │ Duskcue  │  │ Embedded PostgreSQL  │ │
│ │ :48027   │←→│ :5432 (loopback)     │ │
│ │ HTTP     │  │                      │ │
│ └──────────┘  └──────────────────────┘ │
└────────────────────────────────────────┘
        ↑
        │ HTTP (no TLS)
        │
   Browser on LAN
```

**Use case:** Home network only; no remote access. No reverse proxy needed. Duskcue binds `:48027` plain HTTP. Default v1.0 deployment.

### Pattern 2: Direct Exposed Mode (Duskcue rustls TLS, No Proxy)

```
┌────────────────────────────────────────┐
│ Duskcue Docker container               │
│ ┌──────────────┐  ┌──────────────────┐ │
│ │ Duskcue      │  │ Embedded PG      │ │
│ │ :443 (rustls)│←→│ :5432            │ │
│ │ ACME-managed │  │                  │ │
│ │ cert         │  │                  │ │
│ └──────────────┘  └──────────────────┘ │
└────────────────────────────────────────┘
        ↑
        │ HTTPS (TLS terminated by Duskcue)
        │
   Browser / Internet
```

**Use case:** Single-domain exposed mode (e.g., `duskcue.example.com`); operator wants direct HTTPS without proxy management. Duskcue's built-in ACME handles cert lifecycle. Port forwarding `:443` from router to Duskcue container.

**Limitations:** No multi-service routing (only one domain). No HTTP/3 (rustls is HTTP/1.1 + HTTP/2; HTTP/3 would require a proxy). No WAF. No sticky-session load balancing (not needed — single instance).

### Pattern 3: External Caddy (Recommended for Multi-Service)

```
┌───────────────────────────────┐
│ Caddy container               │
│ :80, :443                     │
│ Automatic ACME                │
│ Routes by domain              │
└────────────┬──────────────────┘
             │
   ┌─────────┼─────────┐
   │         │         │
   ▼         ▼         ▼
┌──────┐ ┌──────┐ ┌────────────┐
│Dusk- │ │Jelly-│ │Home        │
│cue   │ │fin   ││Assistant   │
│:48027│ │:8096 │ │:8123       │
└──────┘ └──────┘ └────────────┘
```

**Use case:** Operator runs multiple self-hosted services; wants one entry point with automatic HTTPS for all of them. Caddy routes by domain name (`duskcue.example.com` → Duskcue, `tv.example.com` → Jellyfin, etc.).

### Pattern 4: Existing Nginx/Traefik (Operator Choice)

Operators with existing Nginx or Traefik deployments add Duskcue as another routed service. Configuration patterns documented in Appendix below.

## Recommended Caddy Configuration

The canonical Caddyfile for Duskcue:

```caddyfile
duskcue.example.com {
    reverse_proxy duskcue:48027 {
        # Critical: forward client IP for rate limiting + trust scoring
        header_up X-Real-IP {remote_host}
        header_up X-Forwarded-For {remote_host}
        header_up X-Forwarded-Proto {scheme}
    }

    # SSE: disable buffering for /api/v1/events endpoint
    @events path /api/v1/events
    handle @events {
        reverse_proxy duskcue:48027 {
            flush_interval -1
        }
    }

    # Security headers (defense in depth; Duskcue also sets these
    # when not behind a proxy)
    header {
        Strict-Transport-Security "max-age=31536000; includeSubDomains; preload"
        X-Content-Type-Options nosniff
        Referrer-Policy strict-origin-when-cross-origin
        Permissions-Policy "geolocation=(), microphone=(), camera=()"
    }

    # HTTP/3 advertisement (Caddy does this automatically, explicit here
    # for documentation)
    encode gzip zstd

    # Access logging — be careful not to log sensitive URL params
    # (Duskcue uses cookies for auth, not URL tokens, so this is safe)
    log {
        output file /data/access.log {
            roll_size 100mb
            roll_keep 10
        }
    }
}
```

**Key points:**

1. **`header_up X-Real-IP` and `X-Forwarded-For`** — critical for Duskcue's rate limiting and trust scoring (see [ANALYTICS_SECURITY.md](../security/ANALYTICS_SECURITY.md)). Without these, Duskcue sees Caddy's IP for every request.
2. **SSE-specific handler** — `flush_interval -1` disables buffering for the `/api/v1/events` endpoint so server-sent events stream live. See [REAL_TIME_PUSH.md](REAL_TIME_PUSH.md) for why this matters.
3. **Security headers** — Duskcue sets these itself when not behind a proxy; Caddy duplicates for defense in depth.
4. **HTTP/3** — Caddy serves HTTP/3 (QUIC) automatically on UDP `:443`. Operator must open UDP `:443` on firewall for HTTP/3 to work.
5. **No Docker socket mount** — Caddyfile-based config; no need for Caddy to read Docker API.

### docker-compose.yml for Pattern 3

```yaml
services:
  caddy:
    image: caddy:2.11-alpine
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
      - "443:443/udp"  # HTTP/3 (QUIC)
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile:ro
      - caddy_data:/data
      - caddy_config:/config
    networks:
      - proxy
    cap_drop:
      - ALL
    cap_add:
      - NET_BIND_SERVICE

  duskcue:
    image: ghcr.io/duskcue/duskcue:latest
    restart: unless-stopped
    volumes:
      - duskcue-data:/data
      - duskcue-cache:/cache
    tmpfs:
      - /transcode:size=4G
    environment:
      DUSKCUE_DATABASE_URL: postgresql://duskcue@localhost/duskcue
      DUSKCUE_TRUSTED_PROXIES: "172.16.0.0/12"  # Caddy's network
    networks:
      - proxy
    # NOTE: Duskcue does NOT expose :48027 publicly — only Caddy does.
    # Duskcue is reachable only within the proxy network.

networks:
  proxy:
    driver: bridge

volumes:
  caddy_data:
  caddy_config:
  duskcue-data:
  duskcue-cache:
```

## Trusted-Proxy Configuration (Critical Security Control)

### The `X-Forwarded-For` Spoofing Risk

When Duskcue sits behind a reverse proxy, every request appears to come from the proxy's IP. To recover the real client IP, the proxy sets `X-Forwarded-For` headers. **However**, if Duskcue trusts `X-Forwarded-For` from any source, a malicious client can spoof the header to:

- Bypass rate limiting (claim to be many different IPs)
- Bypass IP-range allow/deny rules (claim to be on the LAN)
- Skew trust scoring and impossible-travel detection (see [ANALYTICS_SECURITY.md](../security/ANALYTICS_SECURITY.md))
- Forge audit log entries

### Solution: `DUSKCUE_TRUSTED_PROXIES` Configuration

Duskcue accepts a `DUSKCUE_TRUSTED_PROXIES` environment variable (or `server_config.security.trusted_proxies` JSONB field) listing the IP ranges of trusted reverse proxies. The middleware (`extract_client_ip()` in `server/src/middleware.rs`) follows this logic:

1. Extract the immediate TCP peer IP from the socket (`ConnectInfo<SocketAddr>`)
2. If the peer IP is in a trusted-proxy range → trust `X-Forwarded-For` and use the leftmost-non-trusted IP from the chain
3. If the peer IP is NOT in a trusted-proxy range → ignore `X-Forwarded-For` entirely; use the peer IP directly

**Default:** `DUSKCUE_TRUSTED_PROXIES` is empty in local mode (no proxy expected) and `["127.0.0.1/32", "::1/128"]` in exposed mode (default to loopback if the proxy runs on the same host). Operators running Caddy/Nginx on a different host must add its IP range explicitly.

**Configuration example:**

```bash
# Caddy on the same Docker host (default)
DUSKCUE_TRUSTED_PROXIES="127.0.0.1/32,::1/128"

# Caddy on a different machine at 10.0.0.5
DUSKCUE_TRUSTED_PROXIES="10.0.0.5/32"

# Caddy on a Docker bridge network (typical Docker Compose setup)
DUSKCUE_TRUSTED_PROXIES="172.16.0.0/12"
```

If `DUSKCUE_TRUSTED_PROXIES` is misconfigured, Duskcue logs a startup warning: "Trusted proxies not configured; client IP detection may be incorrect behind a reverse proxy. See REVERSE_PROXY.md."

### Headers Duskcue Reads

When the peer is a trusted proxy, Duskcue reads these headers in priority order:

1. `X-Real-IP` — set by Caddy, Nginx (`proxy_set_header X-Real-IP $remote_addr;`); single IP, no chain
2. `X-Forwarded-For` — set by all proxies; comma-separated chain; Duskcue uses the leftmost IP that is not in a trusted-proxy range
3. `Forwarded` (RFC 7239) — newer standard; Duskcue parses `for=...` parameter

`X-Real-IP` is preferred when present because it's a single value set by the immediate proxy (no chain ambiguity).

## SSE Proxying (Cross-Reference)

Already partially documented in [REAL_TIME_PUSH.md](REAL_TIME_PUSH.md). The unified proxy config:

### Caddy (Default — Just Works)

Caddy streams responses by default. The `flush_interval -1` directive in the recommended config above is belt-and-suspenders; even without it, Caddy streams SSE correctly.

### Nginx (Requires Explicit Config)

```nginx
location /api/v1/events {
    proxy_pass http://duskcue:48027;
    proxy_buffering off;        # Critical: stream SSE without buffering
    proxy_cache off;
    proxy_set_header Connection '';
    proxy_http_version 1.1;
    chunked_transfer_encoding on;
    proxy_read_timeout 24h;     # SSE connections are long-lived
}
```

Without `proxy_buffering off`, nginx buffers the entire SSE response and the client sees no events until the buffer fills or the connection closes.

### Traefik (Default — Just Works)

Traefik streams responses by default. No special config needed.

### Cloudflare (Problematic)

Cloudflare buffers SSE by default on the free tier. Operators using Cloudflare must either:
- Disable buffering via Cloudflare Rules (configuration burden)
- Accept heartbeat-paced delivery (Duskcue's 15-second KeepAlive flushes the buffer periodically)
- Avoid Cloudflare for the `/api/v1/events` endpoint (route around it)

**Cloudflare TOS for video streaming is also problematic** — see [SECURITY.md](../security/SECURITY.md). The recommendation is to avoid Cloudflare entirely for Duskcue and use a direct Caddy/Nginx setup with Let's Encrypt.

## TLS Certificate Automation

### Duskcue's Built-in ACME (Pattern 2)

When Duskcue binds `:443` directly (no proxy), it uses `rustls-acme` to manage Let's Encrypt certificates. Configuration via `server_config.security`:

- `tls_acme_email` — registration email (optional but recommended for expiry warnings)
- `tls_acme_directory` — `letsencrypt` (production) or `letsencrypt_staging` (testing)
- `tls_challenge_type` — `http-01` (default; requires `:80` accessible) or `dns-01` (requires DNS provider API key; for operators who can't open `:80`)
- Cert storage: `{data_dir}/tls/` directory

See [SECURITY.md](../security/SECURITY.md) for full details on Duskcue's TLS configuration.

### Caddy's Built-in ACME (Pattern 3)

Caddy handles ACME automatically. The Caddyfile just needs the domain name; Caddy does the rest:

```caddyfile
duskcue.example.com {
    reverse_proxy duskcue:48027
}
# Caddy automatically:
# 1. Obtains a Let's Encrypt cert for duskcue.example.com
# 2. Renews it before expiry
# 3. Redirects HTTP → HTTPS
# 4. Enables HTTP/2 and HTTP/3
# 5. Manages OCSP stapling
```

**DNS-01 challenge for wildcard certs:** If the operator wants `*.example.com` (wildcard), Caddy needs the DNS provider's API key:

```caddyfile
*.example.com {
    tls {
        dns cloudflare {env.CLOUDFLARE_API_TOKEN}
    }
    reverse_proxy duskcue:48027
}
```

Caddy supports 70+ DNS providers via plugins. Build the Caddy image with the relevant plugin via `caddy build --with github.com/caddy-dns/cloudflare`.

### Nginx + Certbot (Pattern 4)

Certbot runs as a sidecar container, requesting and renewing certs via HTTP-01 challenge. See [Appendix: Nginx Configuration](#appendix-nginx-configuration) below.

## Cloudflare Considerations

Cloudflare is the most popular CDN/DDNS provider in the self-hosted community, but it has two specific problems for Duskcue:

### 1. Cloudflare TOS for Video Streaming

Cloudflare's [Self-Service Subscription Agreement §2.8](https://www.cloudflare.com/terms/) prohibits using the free tier to serve "non-HTML" content including video/audio. Duskcue serves video streams, which violates this. Operators using Cloudflare free tier for Duskcue risk account suspension.

**Mitigation:** Use Cloudflare for DNS only (not proxying); serve Duskcue directly via Caddy + Let's Encrypt. Or use Cloudflare's paid Stream distribution service (expensive for personal use). Or use a non-CDN-fronted setup with a static IP / dynamic DNS (Tailscale, DuckDNS, etc.).

See [SECURITY.md](../security/SECURITY.md) "Cloudflare Alternatives" section for the full analysis.

### 2. Cloudflare SSE Buffering

Cloudflare buffers SSE responses on the free tier, breaking real-time event push. Operators who must use Cloudflare can disable buffering via Cloudflare Rules, but this is fragile and easy to misconfigure.

### Recommended Alternatives to Cloudflare

For Duskcue exposed mode, the recommended patterns (no Cloudflare):

| Need | Recommendation |
|---|---|
| Domain name + dynamic IP | DuckDNS, No-IP, afraid.org freedns |
| TLS cert | Caddy's automatic ACME (Let's Encrypt) |
| VPN-style access | Tailscale, Headscale, WireGuard (SECURITY.md tier 2 model) |
| DDoS protection | Reasonable for personal media server to skip; operators concerned can use OVH/Hetzter DDoS-protected VPS |

## HTTP/3 (QUIC) Support

HTTP/3 improves mobile performance (fewer connection-setup round trips) and survives network changes (cellular→WiFi handoff) better than HTTP/2.

| Component | HTTP/3 Support |
|---|---|
| Caddy 2.11 | ✅ Default-on; serves HTTP/3 on UDP `:443` |
| Duskcue's rustls | ❌ HTTP/1.1 + HTTP/2 only (rustls-acme doesn't include HTTP/3) |
| Nginx 1.28 | ⚠️ Experimental; via third-party module |
| Traefik v3.6 | ⚠️ Experimental |

**Recommendation:** For HTTP/3, use Caddy as external proxy. Operators running Duskcue's built-in TLS (Pattern 2) get HTTP/2 only — acceptable for v1.0; HTTP/3 is a future enhancement if `quinn` (Rust HTTP/3 library) stabilizes sufficiently to bundle.

## Edge Cases

### Operator Runs Multiple Duskcue Instances

Ruled out by [MULTI_INSTANCE.md](MULTI_INSTANCE.md). The proxy config routes one domain → one Duskcue container. Operators attempting multiple Duskcue instances behind one proxy will hit the lockfile and in-memory state divergence described in that doc.

### WebSockets for Backward Compatibility

Duskcue's previous design (pre-[REAL_TIME_PUSH.md](REAL_TIME_PUSH.md)) spec'd WebSockets. All current proxy configurations support both SSE and WebSocket proxying transparently — operators with existing WebSocket proxy configs don't need to change anything.

### Subpath vs Subdomain Routing

Duskcue is designed for **subdomain routing** (`duskcue.example.com`), not subpath (`example.com/duskcue/`). Subpath routing requires URL-rewrite logic in the proxy and Duskcue's `base_url` config. Subpath is documented but not recommended — subdomain is simpler for HTTPS cert management (per-domain cert via ACME vs requiring a wildcard cert).

### IPv6

All recommended proxies handle IPv6 transparently. Duskcue's `extract_client_ip()` parses both IPv4 and IPv6 addresses. Trusted-proxy ranges can include IPv6 CIDR (e.g., `::1/128`, `fd00::/8`).

### Custom Error Pages

Operators wanting branded error pages (e.g., "Duskcue is restarting, back in 30 seconds") configure these in the proxy, not in Duskcue. Duskcue's `/health` endpoint can be used by the proxy for health-check-based error page serving.

### Streaming Large Media Files Through the Proxy

When Dusdcue serves media bytes via `/api/v1/stream/{file_id}`, the response is large and streamed with HTTP Range requests. Proxy config must:

- **Caddy** — Default; streams large responses correctly.
- **Nginx** — Default `proxy_pass` handles Range; ensure `proxy_buffering off` is NOT set globally (only on `/api/v1/events`). For streaming endpoints, buffering is fine and improves client throughput.
- **Traefik** — Default; handles Range correctly.

### HTTP → HTTPS Redirect

All three proxies handle this automatically:

- **Caddy** — Default behavior; redirects `:80` to `:443`.
- **Nginx** — Requires explicit `return 301 https://$host$request_uri;` block.
- **Traefik** — Configurable via entrypoint redirection (`--entrypoints.web.http.redirections.entrypoint.to=websecure`).

## Appendix: Nginx Configuration

For operators with existing Nginx infrastructure (Pattern 4), the equivalent config:

```nginx
# /etc/nginx/conf.d/duskcue.conf

upstream duskcue {
    server duskcue:48027;
    keepalive 32;
}

server {
    listen 80;
    server_name duskcue.example.com;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl http2;
    server_name duskcue.example.com;

    ssl_certificate /etc/letsencrypt/live/duskcue.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/duskcue.example.com/privkey.pem;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_prefer_server_ciphers off;
    server_tokens off;

    # Security headers
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains; preload" always;
    add_header X-Content-Type-Options nosniff always;
    add_header Referrer-Policy strict-origin-when-cross-origin always;

    # Standard proxy config
    location / {
        proxy_pass http://duskcue;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_http_version 1.1;
        proxy_set_header Connection "";

        # Streaming media — large responses
        proxy_buffering on;
        proxy_max_temp_file_size 0;
        client_max_body_size 0;  # No upload limit
    }

    # SSE endpoint — disable buffering
    location /api/v1/events {
        proxy_pass http://duskcue;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_http_version 1.1;
        proxy_set_header Connection "";

        proxy_buffering off;
        proxy_cache off;
        chunked_transfer_encoding on;
        proxy_read_timeout 24h;
    }

    # ACME challenge passthrough (if Certbot in use)
    location /.well-known/acme-challenge/ {
        root /var/www/certbot;
    }
}
```

## Appendix: Traefik Configuration

For operators with existing Traefik infrastructure (Pattern 4), the equivalent labels on the Duskcue container:

```yaml
services:
  duskcue:
    image: ghcr.io/duskcue/duskcue:latest
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.duskcue.rule=Host(`duskcue.example.com`)"
      - "traefik.http.routers.duskcue.entrypoints=websecure"
      - "traefik.http.routers.duskcue.tls.certresolver=letsencrypt"
      - "traefik.http.services.duskcue.loadbalancer.server.port=48027"
      # Forward client IP
      - "traefik.http.middlewares.duskcue-headers.headers.customrequestheaders.X-Forwarded-Proto=https"
    networks:
      - proxy
```

**Note:** Traefik automatically sets `X-Forwarded-For` with the real client IP when the `forwardAuth` middleware is configured. Operators should ensure the `passHostHeader` option is `true` (default).

## Implementation Status

| Component | Status | Notes |
|---|---|---|
| Duskcue built-in rustls TLS termination | ✅ Implemented | [SECURITY.md](../security/SECURITY.md); rustls with `ring` backend |
| Duskcue built-in ACME cert management | ✅ Implemented | `rustls-acme` crate; HTTP-01 and DNS-01 challenges |
| `extract_client_ip()` middleware | ✅ Implemented | `server/src/middleware.rs`; reads `X-Real-IP` then `X-Forwarded-For`; falls back to socket peer IP |
| `DUSKCUE_TRUSTED_PROXIES` config | ✅ Implemented | `BootstrapConfig.trusted_proxies`; parsed to `Vec<IpNet>` at startup |
| Recommended Caddyfile documentation | This document | Phase 15 deployment guide includes the canonical config |
| Nginx + Traefik alternative configs | This document | Appendix sections above |
| Health-check endpoint for proxy health | ✅ Implemented | `GET /health` returns 200 with DB status; suitable for Caddy/Nginx/Traefik health checks |

**No implementation work required for v1.0** — Duskcue already handles TLS, client-IP extraction, and trusted-proxy configuration. This document is the canonical operator-facing recommendation and the configuration templates for Phase 15 deployment guide.

## Key Decisions

1. **Built-in rustls TLS for simple exposed mode** — Duskcue binds `:443` directly with ACME-managed certs. No reverse proxy required for the single-domain case.
2. **Caddy as the recommended external reverse proxy** — Matches Jellyfin pattern; simplest path to multi-service HTTPS; small footprint; no Docker socket exposure; HTTP/3 default-on.
3. **Don't bundle Caddy in the Dusdcue container** — Container stays lean; operators with existing proxies reuse them; lifecycle decoupling (proxy survives app restarts); matches single-container model from [MULTI_INSTANCE.md](MULTI_INSTANCE.md).
4. **Nginx and Traefik documented as alternatives** — Operators with existing infrastructure use what they know. Caddy is the opinionated default; the alternatives are first-class supported but not the primary recommendation.
5. **`DUSKCUE_TRUSTED_PROXIES` is critical security config** — Without it, `X-Forwarded-For` spoofing enables rate-limit bypass, IP-allowlist bypass, and trust-scoring manipulation. Default is empty/loopback; operators must explicitly add proxy ranges.
6. **SSE proxying: Caddy default-on, Nginx requires explicit `proxy_buffering off`, Traefik default-on** — Unified recommendation across all three proxies; details in [REAL_TIME_PUSH.md](REAL_TIME_PUSH.md).
7. **Avoid Cloudflare** — TOS prohibits video streaming on free tier; SSE buffering breaks real-time events. Recommend DuckDNS + Caddy + Let's Encrypt for dynamic-IP setups; Tailscale for VPN-style access.
8. **HTTP/3 via Caddy; HTTP/2 via Duskcue built-in TLS** — Duskcue's rustls doesn't support HTTP/3 yet. Operators wanting HTTP/3 use Caddy. HTTP/3 for built-in TLS is a future enhancement if `quinn` stabilizes.
9. **Subdomain routing (`duskcue.example.com`) over subpath (`example.com/duskcue/`)** — Simpler cert management; simpler client-side URL handling; subpath requires URL rewriting and `base_url` config.
10. **Operator's choice for VPN vs HTTPS exposure** — [SECURITY.md](../security/SECURITY.md) tier model: Local (no TLS, no proxy), VPN (Tailscale/WireGuard, no proxy needed), Exposed (Caddy/Nginx + ACME). The proxy recommendation here applies to tier 3 (Exposed) only.

## Relationship to Other Domains

| Document | Relationship |
|---|---|
| [SECURITY.md](../security/SECURITY.md) | Three-tier network model (Local / VPN / Exposed); TLS via rustls; trusted-proxy configuration; Cloudflare TOS analysis |
| [REAL_TIME_PUSH.md](REAL_TIME_PUSH.md) | SSE proxying config (Caddy default-on; Nginx `proxy_buffering off`; Traefik default-on; Cloudflare problematic) |
| [HTTP_CACHING.md](HTTP_CACHING.md) | Artwork URL caching (`Cache-Control: public, max-age=86400, immutable`) — proxies respect these headers transparently |
| [MULTI_INSTANCE.md](MULTI_INSTANCE.md) | Single-instance constraint: proxy routes one domain → one Duskcue; no load-balancer-multi-instance pattern |
| [DOCKER_DEPLOYMENT.md](../operations/DOCKER_DEPLOYMENT.md) | Phase 15 single-container canonical topology; Pattern 3 docker-compose.yml above is the canonical exposed-mode deployment |
| [API_CONVENTIONS.md](API_CONVENTIONS.md) | `X-Forwarded-For` and friends are part of the API contract; clients/SDKs don't set these (proxy does) |
| [ANALYTICS_SECURITY.md](../security/ANALYTICS_SECURITY.md) | Trust scoring and impossible-travel detection depend on accurate client IP — trusted-proxy config is the prerequisite |
| [BUILD_ORDER.md](../../BUILD_ORDER.md) | Phase 15 (Docker & Deployment) — operator-facing proxy recommendation shipped with the deployment guide |

## Research Sources

- **[Jellyfin: Reverse Proxy documentation](https://jellyfin.org/docs/general/post-install/networking/reverse-proxy/)** — Official Jellyfin docs explicitly recommend Caddy: "We recommend using Caddy for its ease of use, especially with https."
- **[Jellyfin: Caddy configuration guide](https://jellyfin.org/docs/general/post-install/networking/reverse-proxy/caddy/)** — Caddyfile patterns for media servers; one-liner `caddy reverse-proxy` option
- **[Traefik vs Caddy vs Nginx: Docker Reverse Proxy Compared](https://www.virtua.cloud/learn/en/concepts/traefik-caddy-nginx-docker-reverse-proxy)** — Side-by-side Docker Compose examples with memory/image benchmarks: Caddy 14MB/88MB, Nginx 5MB/240MB+Certbot 25MB/298MB, Traefik 17MB/242MB
- **[Reddit: Caddy vs Traefik, Which Do You Use and Why?](https://www.reddit.com/r/selfhosted/comments/1jdy2gn/)** — Community consensus favoring Caddy for simplicity; Traefik for auto-discovery (overkill for single-instance Duskcue)
- **[Caddy vs Nginx 2026: 22% Speed Gap, 32.8% Market Share](https://tech-insider.org/caddy-vs-nginx-2026/)** — Performance benchmarks; Caddy 22% faster than Nginx 1.26 on throughput; Nginx still owns 32.8% market share
- **[Nginx Proxy Manager vs Traefik vs Caddy (Stackademic)](https://blog.stackademic.com/npm-traefik-or-caddy-how-to-pick-the-reverse-proxy-youll-still-like-in-6-months-1e1101815e07)** — Decision framework for self-hosted proxy choice; emphasizes "operating model" over feature comparison
- **[Caddy documentation: Automatic HTTPS](https://caddyserver.com/docs/automatic-https)** — How Caddy's zero-config ACME works
- **[Cloudflare Self-Service Subscription Agreement §2.8](https://www.cloudflare.com/terms/)** — TOS prohibiting video streaming on free tier
- **[Let's Encrypt: Challenge Types](https://letsencrypt.org/docs/challenge-types/)** — HTTP-01 vs DNS-01 challenge tradeoffs
- **[RFC 7239: Forwarded HTTP Extension](https://www.rfc-editor.org/rfc/rfc7239)** — Standardized `Forwarded` header (newer alternative to `X-Forwarded-For`)

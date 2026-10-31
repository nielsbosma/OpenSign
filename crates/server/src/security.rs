use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use opensign_core::settings::{SecuritySettings, TrustedProxies};

use crate::AppState;
use crate::error::json_error;

/// A strict policy for the app shell. pdf.js renders from a blob worker; nothing loads from
/// another origin.
const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self'; \
    connect-src 'self'; worker-src 'self' blob:; frame-src 'none'; frame-ancestors 'none'; object-src 'none'; base-uri 'none'; form-action 'self'";

pub async fn headers(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    // Only the signing page may ask for the camera, for the webcam photo, and only it may be framed.
    let signing_page = path == "/sign";
    let script = path == "/embed.js";
    let framers = if signing_page { embed_origins(&state).await } else { Vec::new() };
    let mut response = next.run(request).await;
    let h = response.headers_mut();
    if framers.is_empty() {
        h.entry(header::CONTENT_SECURITY_POLICY).or_insert(HeaderValue::from_static(CSP));
        h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    } else if let Ok(csp) = HeaderValue::from_str(&framed_by(&framers)) {
        h.insert(header::CONTENT_SECURITY_POLICY, csp);
    } else {
        h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP));
        h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    }
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    h.insert("cross-origin-opener-policy", HeaderValue::from_static("same-origin"));
    if script {
        // Customers load the embedding script from their own pages.
        h.insert("cross-origin-resource-policy", HeaderValue::from_static("cross-origin"));
        h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    }
    h.entry("cross-origin-resource-policy").or_insert(HeaderValue::from_static("same-origin"));
    h.insert(
        "permissions-policy",
        HeaderValue::from_static(if signing_page {
            "camera=(self), microphone=(), geolocation=(), payment=(), usb=(), interest-cohort=()"
        } else {
            "camera=(), microphone=(), geolocation=(), payment=(), usb=(), interest-cohort=()"
        }),
    );
    if state.https {
        h.insert(header::STRICT_TRANSPORT_SECURITY, HeaderValue::from_static("max-age=63072000; includeSubDomains"));
    }
    response
}

/// The sites an admin allowed to embed the signing page. Any failure to read them means none.
async fn embed_origins(state: &AppState) -> Vec<String> {
    let Ok(mut conn) = state.app.db.acquire().await else { return Vec::new() };
    match state.app.settings.get::<SecuritySettings>(&mut conn, &state.app.keys).await {
        Ok(security) => security.embed_origins(),
        Err(_) => Vec::new(),
    }
}

/// The app's policy with `frame-ancestors` naming the allowed sites instead of none.
fn framed_by(origins: &[String]) -> String {
    CSP.replace("frame-ancestors 'none'", &format!("frame-ancestors {}", origins.join(" ")))
}

/// The admin's choice of trusted proxies, read at most every few seconds.
pub struct ProxyPolicy(RwLock<(Option<Instant>, Arc<TrustedProxies>)>);

impl Default for ProxyPolicy {
    fn default() -> Self {
        Self(RwLock::new((None, Arc::new(TrustedProxies::Private))))
    }
}

const POLICY_TTL: Duration = Duration::from_secs(5);

async fn trusted_proxies(state: &AppState) -> Arc<TrustedProxies> {
    let (read_at, current) = state.proxies.0.read().map(|p| p.clone()).unwrap_or_else(|e| e.into_inner().clone());
    if read_at.is_some_and(|at| at.elapsed() < POLICY_TTL) {
        return current;
    }
    // Every request comes through here, so once the policy is known a busy pool must not hold them
    // up: the rate limiter has to keep answering quickly when the database cannot.
    let conn = if read_at.is_some() { state.app.db.try_acquire() } else { state.app.db.acquire().await.ok() };
    let Some(mut conn) = conn else { return current };
    let Ok(security) = state.app.settings.get::<SecuritySettings>(&mut conn, &state.app.keys).await else { return current };
    let policy = Arc::new(security.trusted_proxies());
    if let Ok(mut slot) = state.proxies.0.write() {
        *slot = (Some(Instant::now()), policy.clone());
    }
    policy
}

/// The client's address. `X-Forwarded-For` is believed only from a trusted proxy.
pub async fn client_ip(state: &AppState, headers: &HeaderMap, peer: Option<SocketAddr>) -> Option<IpAddr> {
    forwarded_client(&*trusted_proxies(state).await, headers, peer)
}

/// With a list, entries are read from the right past the admin's own proxies. With `private` only
/// the last entry counts, the one the platform's load balancer wrote.
pub fn forwarded_client(proxies: &TrustedProxies, headers: &HeaderMap, peer: Option<SocketAddr>) -> Option<IpAddr> {
    let peer = peer.map(|p| p.ip().to_canonical());
    let trusted = |ip: IpAddr| match proxies {
        TrustedProxies::None => false,
        TrustedProxies::Private => ip.is_loopback() || is_private(ip),
        TrustedProxies::List(nets) => nets.iter().any(|net| net.contains(&ip)),
    };
    if matches!(proxies, TrustedProxies::None) || !peer.is_none_or(trusted) {
        return peer;
    }
    let mut client = peer;
    let lines: Vec<_> = headers.get_all("x-forwarded-for").iter().collect();
    for entry in lines.iter().rev().flat_map(|line| line.to_str().unwrap_or("-").rsplit(',')) {
        let Ok(ip) = entry.trim().parse::<IpAddr>() else { break };
        let ip = ip.to_canonical();
        client = Some(ip);
        if matches!(proxies, TrustedProxies::Private) || !trusted(ip) {
            break;
        }
    }
    client
}

fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local() || v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64,
        IpAddr::V6(v6) => (v6.segments()[0] & 0xfe00) == 0xfc00 || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_private()),
    }
}

pub fn peer(request: &Request) -> Option<SocketAddr> {
    request.extensions().get::<ConnectInfo<SocketAddr>>().map(|c| c.0)
}

/// Cookie-authenticated writes must come from our own pages. Browsers send `Origin` on every
/// cross-site POST, and `Sec-Fetch-Site` everywhere modern; one of them has to vouch for us.
pub async fn same_origin(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let safe = matches!(*request.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    let has_session = crate::auth::session_cookie(request.headers(), &state).is_some();
    if safe {
        return next.run(request).await;
    }
    let headers = request.headers();
    // API clients do not send this header. A browser sends it, so a page on another site cannot
    // sign a signed-out visitor in or spend a public call in their name.
    if headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) == Some("cross-site") {
        return json_error(StatusCode::FORBIDDEN, "forbidden", "Cross-origin request refused");
    }
    if !has_session {
        return next.run(request).await;
    }
    let scheme = if state.https { "https" } else { "http" };
    let expected = state
        .app
        .env
        .app_url
        .as_deref()
        .and_then(origin_of)
        .or_else(|| headers.get(header::HOST).and_then(|h| h.to_str().ok()).map(|h| format!("{scheme}://{}", h.to_lowercase())));
    let origin_ok = match headers.get(header::ORIGIN).and_then(|o| o.to_str().ok()) {
        Some(origin) => origin_of(origin).is_some_and(|o| Some(o) == expected),
        None => headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) == Some("same-origin"),
    };
    if !origin_ok {
        return json_error(StatusCode::FORBIDDEN, "forbidden", "Cross-origin request refused");
    }
    next.run(request).await
}

/// `scheme://host[:port]`, lowercased: an `http://` page on the same host is another origin.
fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    (!host.is_empty()).then(|| format!("{}://{}", scheme.to_lowercase(), host.to_lowercase()))
}

#[derive(Debug, Clone, Copy)]
pub struct Limit {
    pub name: &'static str,
    pub per_minute: u32,
    pub burst: u32,
}

pub const GENERAL: Limit = Limit { name: "general", per_minute: 600, burst: 120 };
pub const AUTH: Limit = Limit { name: "auth", per_minute: 10, burst: 10 };
pub const SIGNER: Limit = Limit { name: "signer", per_minute: 60, burst: 30 };
pub const CODES: Limit = Limit { name: "codes", per_minute: 6, burst: 6 };
pub const VERIFY: Limit = Limit { name: "verify", per_minute: 10, burst: 10 };
pub const UPLOAD: Limit = Limit { name: "upload", per_minute: 30, burst: 10 };
pub const REVIEW: Limit = Limit { name: "review", per_minute: 20, burst: 10 };

struct Bucket {
    tokens: f64,
    at: Instant,
    /// From here on the bucket is full again, the same as having none.
    full_at: Instant,
}

const MAX_BUCKETS: usize = 50_000;

/// Token buckets per address, in memory. Each replica limits on its own, which still caps an
/// attacker at a small multiple; the database-backed lockouts are the real brake on guessing.
#[derive(Default)]
pub struct RateLimiter {
    buckets: Mutex<HashMap<(&'static str, IpAddr), Bucket>>,
}

/// One customer usually holds a whole IPv6 /64, so it counts as one address.
fn bucket_of(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V6(v6) => IpAddr::V6((u128::from(v6) & !0u128 << 64).into()),
        v4 => v4,
    }
}

impl RateLimiter {
    pub fn allow(&self, limit: Limit, ip: Option<IpAddr>) -> bool {
        let Some(ip) = ip else { return true };
        let now = Instant::now();
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        if buckets.len() >= MAX_BUCKETS {
            buckets.retain(|_, b| b.full_at > now);
            // Still that many live buckets: forget some rather than grow. Sweeping at most once
            // per MAX_BUCKETS / 4 new addresses keeps this from running on every request.
            if buckets.len() >= MAX_BUCKETS / 4 * 3 {
                let mut keep = false;
                buckets.retain(|_, _| {
                    keep = !keep;
                    keep
                });
            }
        }
        let bucket = buckets.entry((limit.name, bucket_of(ip))).or_insert(Bucket { tokens: limit.burst as f64, at: now, full_at: now });
        let per_second = limit.per_minute as f64 / 60.0;
        bucket.tokens = (bucket.tokens + now.duration_since(bucket.at).as_secs_f64() * per_second).min(limit.burst as f64);
        bucket.at = now;
        let allowed = bucket.tokens >= 1.0;
        if allowed {
            bucket.tokens -= 1.0;
        }
        bucket.full_at = now + Duration::from_secs_f64((limit.burst as f64 - bucket.tokens) / per_second);
        allowed
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.buckets.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

/// Requests handled at once by this replica. Past it a quick refusal costs less than queueing
/// bodies in memory until the process dies.
pub const MAX_IN_FLIGHT: usize = 512;

pub async fn in_flight(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if matches!(request.uri().path(), "/api/health" | "/api/ready") {
        return next.run(request).await;
    }
    let Ok(_permit) = state.in_flight.clone().try_acquire_owned() else {
        let mut response = json_error(StatusCode::SERVICE_UNAVAILABLE, "unavailable", "The server is busy. Try again in a moment.");
        response.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
        return response;
    };
    next.run(request).await
}

pub fn too_many() -> Response {
    let mut response = json_error(StatusCode::TOO_MANY_REQUESTS, "too_many_requests", "Too many requests. Wait a minute and try again.");
    response.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
    response
}

pub async fn general_limit(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let peer = peer(&request);
    let ip = client_ip(&state, request.headers(), peer).await;
    if !state.limiter.allow(GENERAL, ip) {
        return too_many().into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwarded_for_only_from_private_proxies() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", HeaderValue::from_static("6.6.6.6, 203.0.113.9"));
        let proxy: SocketAddr = "10.0.0.2:5000".parse().unwrap_or_else(|_| unreachable!());
        let direct: SocketAddr = "198.51.100.1:5000".parse().unwrap_or_else(|_| unreachable!());
        let private = TrustedProxies::Private;
        assert_eq!(forwarded_client(&private, &h, Some(proxy)), "203.0.113.9".parse().ok());
        assert_eq!(forwarded_client(&private, &h, Some(direct)), "198.51.100.1".parse().ok());
    }

    #[test]
    fn forwarded_for_follows_the_setting() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", HeaderValue::from_static("6.6.6.6, 203.0.113.9, 10.0.0.7"));
        let proxy: SocketAddr = "10.0.0.2:5000".parse().unwrap_or_else(|_| unreachable!());
        let other: SocketAddr = "10.9.0.2:5000".parse().unwrap_or_else(|_| unreachable!());
        let none = TrustedProxies::parse("none").expect("valid");
        assert_eq!(forwarded_client(&none, &h, Some(proxy)), "10.0.0.2".parse().ok());
        let list = TrustedProxies::parse("10.0.0.0/24").expect("valid");
        assert_eq!(forwarded_client(&list, &h, Some(proxy)), "203.0.113.9".parse().ok(), "reads past the listed proxies");
        assert_eq!(forwarded_client(&list, &h, Some(other)), "10.9.0.2".parse().ok(), "an unlisted peer states nothing");
        assert!(TrustedProxies::parse("everyone").is_err());
        assert!(TrustedProxies::parse("").is_err());
    }

    #[test]
    fn an_ipv6_block_shares_one_bucket() {
        let limiter = RateLimiter::default();
        let allowed = (0..20).filter(|i| limiter.allow(AUTH, format!("2001:db8:1:1::{i:x}").parse().ok())).count();
        assert_eq!(allowed, 10);
        assert!(limiter.allow(AUTH, "2001:db8:1:2::1".parse().ok()), "the next /64 is someone else");
        let mapped: Option<IpAddr> = "::ffff:203.0.113.1".parse().ok();
        assert_eq!(mapped.map(bucket_of), "203.0.113.1".parse().ok());
    }

    #[test]
    fn buckets_stay_bounded() {
        let limiter = RateLimiter::default();
        for i in 0..(MAX_BUCKETS as u32 + 1000) {
            limiter.allow(AUTH, Some(IpAddr::V4(i.into())));
        }
        assert!(limiter.len() <= MAX_BUCKETS, "{}", limiter.len());
    }

    #[test]
    fn buckets_refuse_bursts() {
        let limiter = RateLimiter::default();
        let ip = "203.0.113.1".parse().ok();
        let allowed = (0..20).filter(|_| limiter.allow(AUTH, ip)).count();
        assert_eq!(allowed, 10);
        assert!(limiter.allow(AUTH, "203.0.113.2".parse().ok()), "other addresses are unaffected");
    }

    #[test]
    fn framing_names_only_the_listed_sites() {
        let csp = framed_by(&["https://app.example.com".into(), "https://crm.example.com".into()]);
        assert!(csp.contains("frame-ancestors https://app.example.com https://crm.example.com;"), "{csp}");
        assert!(csp.contains("frame-src 'none'"), "the page itself still frames nothing");
    }

    #[test]
    fn hosts_compare_without_paths() {
        assert_eq!(origin_of("https://Sign.Example.com/x"), Some("https://sign.example.com".into()));
        assert_eq!(origin_of("null"), None);
    }
}

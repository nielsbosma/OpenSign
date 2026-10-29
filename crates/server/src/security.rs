use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::Instant;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::AppState;
use crate::error::json_error;

/// A strict policy for the app shell. pdf.js renders from a blob worker; nothing loads from
/// another origin.
const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self'; \
    connect-src 'self'; worker-src 'self' blob:; frame-src 'none'; frame-ancestors 'none'; object-src 'none'; base-uri 'none'; form-action 'self'";

pub async fn headers(State(state): State<AppState>, request: Request, next: Next) -> Response {
    // Only the signing page may ask for the camera, for the webcam photo.
    let camera = request.uri().path() == "/sign";
    let mut response = next.run(request).await;
    let h = response.headers_mut();
    h.entry(header::CONTENT_SECURITY_POLICY).or_insert(HeaderValue::from_static(CSP));
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    h.insert("cross-origin-opener-policy", HeaderValue::from_static("same-origin"));
    h.entry("cross-origin-resource-policy").or_insert(HeaderValue::from_static("same-origin"));
    h.insert(
        "permissions-policy",
        HeaderValue::from_static(if camera {
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

/// The client's address. `X-Forwarded-For` is believed only from a proxy on a private network
/// (the platform's load balancer), and then only its last entry, which that proxy wrote.
pub fn client_ip(headers: &HeaderMap, peer: Option<SocketAddr>) -> Option<IpAddr> {
    let peer = peer.map(|p| p.ip());
    let behind_proxy = peer.is_none_or(|ip| ip.is_loopback() || is_private(ip));
    if behind_proxy
        && let Some(forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
        && let Some(last) = forwarded.rsplit(',').next().and_then(|s| s.trim().parse::<IpAddr>().ok())
    {
        return Some(last);
    }
    peer
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
    if safe || !has_session {
        return next.run(request).await;
    }
    let headers = request.headers();
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

/// Token buckets per address, in memory. Each replica limits on its own, which still caps an
/// attacker at a small multiple; the database-backed lockouts are the real brake on guessing.
type Buckets = HashMap<(&'static str, IpAddr), (f64, Instant)>;

#[derive(Default)]
pub struct RateLimiter {
    buckets: Mutex<Buckets>,
}

impl RateLimiter {
    pub fn allow(&self, limit: Limit, ip: Option<IpAddr>) -> bool {
        let Some(ip) = ip else { return true };
        let now = Instant::now();
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        if buckets.len() > 50_000 {
            buckets.retain(|_, (_, at)| now.duration_since(*at).as_secs() < 600);
        }
        let (tokens, at) = buckets.entry((limit.name, ip)).or_insert((limit.burst as f64, now));
        let refill = now.duration_since(*at).as_secs_f64() * limit.per_minute as f64 / 60.0;
        *tokens = (*tokens + refill).min(limit.burst as f64);
        *at = now;
        if *tokens >= 1.0 {
            *tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

pub fn too_many() -> Response {
    let mut response = json_error(StatusCode::TOO_MANY_REQUESTS, "too_many_requests", "Too many requests. Wait a minute and try again.");
    response.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
    response
}

pub async fn general_limit(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let ip = client_ip(request.headers(), peer(&request));
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
        assert_eq!(client_ip(&h, Some(proxy)), "203.0.113.9".parse().ok());
        assert_eq!(client_ip(&h, Some(direct)), "198.51.100.1".parse().ok());
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
    fn hosts_compare_without_paths() {
        assert_eq!(origin_of("https://Sign.Example.com/x"), Some("https://sign.example.com".into()));
        assert_eq!(origin_of("null"), None);
    }
}

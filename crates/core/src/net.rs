use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

/// Addresses that outbound calls to user-supplied URLs (webhooks, AI base URLs, providers) must
/// never reach: loopback, private networks, link-local (cloud metadata), and the like.
pub fn is_forbidden(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_multicast()
                || o[0] == 0
                || (o[0] == 100 && (64..=127).contains(&o[1]))
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)
                || (o[0] == 198 && (18..=19).contains(&o[1]))
                || (o[0] == 192 && o[1] == 88 && o[2] == 99)
                || o[0] >= 240
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_forbidden(IpAddr::V4(v4));
            }
            let s = v6.segments();
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00
                || (s[0] & 0xffc0) == 0xfe80
                || (s[0] == 0x64 && s[1] == 0xff9b)
                || (s[0] == 0x2001 && (s[1] == 0x0db8 || s[1] == 0))
                || s[0] == 0x2002
                || s[..6].iter().all(|x| *x == 0)
        }
    }
}

struct FilteringResolver;

impl Resolve for FilteringResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let host = name.as_str().to_string();
            // The same answer for a name that does not exist as for one on a private network, since
            // delivery errors are shown to people who should not learn the internal DNS from them.
            let refused = || -> Box<dyn std::error::Error + Send + Sync> { format!("{host} has no public address").into() };
            let found = tokio::net::lookup_host((host.as_str(), 0)).await.map_err(|_| refused())?;
            let addrs: Vec<SocketAddr> = found.filter(|a| !is_forbidden(a.ip())).collect();
            if addrs.is_empty() {
                return Err(refused());
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

/// Link-local and cloud metadata addresses. A self-hosted AI proxy may be on the private network,
/// but nothing legitimate is here, and these answer with the host's credentials.
fn is_metadata(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_link_local() || matches!(v4.octets(), [100, 100, 100, 200] | [192, 0, 0, 192] | [168, 63, 129, 16]),
        IpAddr::V6(v6) => {
            v6.to_ipv4_mapped().is_some_and(|v4| is_metadata(IpAddr::V4(v4)))
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                || v6.segments()[..3] == [0xfd00, 0x0ec2, 0]
        }
    }
}

struct ProxyResolver;

impl Resolve for ProxyResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let host = name.as_str().to_string();
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0)).await?.filter(|a| !is_metadata(a.ip())).collect();
            if addrs.is_empty() {
                return Err(format!("{host} has no usable address").into());
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

/// The address of a self-hosted AI proxy. Literal IPs bypass DNS, so they are checked here too.
pub fn check_ai_proxy_url(url: &str) -> Result<reqwest::Url, String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "Enter the proxy's http or https address".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") || !parsed.username().is_empty() {
        return Err("Enter the proxy's http or https address".into());
    }
    let literal = match parsed.host() {
        Some(url::Host::Ipv4(ip)) => Some(IpAddr::V4(ip)),
        Some(url::Host::Ipv6(ip)) => Some(IpAddr::V6(ip)),
        _ => None,
    };
    if literal.is_some_and(is_metadata) {
        return Err("That address is not allowed for an AI proxy".into());
    }
    Ok(parsed)
}

/// HTTP clients for the rest of the app. `public` filters destinations after DNS resolution, so a
/// hostname that points at 169.254.169.254 or 10.0.0.1 is refused; it follows no redirects.
/// `internal` is for addresses the operator configured (the converter, development mail).
/// `ai_proxy` is for a self-hosted AI proxy: the private network, but not metadata addresses.
/// None honors proxy variables in the environment: a proxy would resolve names itself, past the filter.
#[derive(Clone)]
pub struct SafeHttp {
    pub public: reqwest::Client,
    pub internal: reqwest::Client,
    pub ai_proxy: reqwest::Client,
}

impl SafeHttp {
    pub fn new() -> Self {
        let ua = format!("OpenSign/{}", crate::env::app_version());
        let public = reqwest::Client::builder()
            .user_agent(&ua)
            .dns_resolver(Arc::new(FilteringResolver))
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .https_only(false)
            .no_proxy()
            .build()
            .expect("HTTP client");
        let internal = reqwest::Client::builder()
            .user_agent(&ua)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(120))
            .no_proxy()
            .build()
            .expect("HTTP client");
        let ai_proxy = reqwest::Client::builder()
            .user_agent(&ua)
            .dns_resolver(Arc::new(ProxyResolver))
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(180))
            .no_proxy()
            .build()
            .expect("HTTP client");
        Self { public, internal, ai_proxy }
    }
}

impl Default for SafeHttp {
    fn default() -> Self {
        Self::new()
    }
}

/// A client for one provider that authenticates with a client certificate (BankID, Freja). With
/// `roots`, only those CAs are trusted, so a public CA cannot impersonate the provider.
pub fn client_with_certificate(identity_pem: &str, roots: Option<&str>) -> Result<reqwest::Client, String> {
    let identity = reqwest::Identity::from_pem(identity_pem.as_bytes())
        .map_err(|_| "The client certificate must be PEM with the certificate and its unencrypted private key".to_string())?;
    let mut builder = reqwest::Client::builder()
        .user_agent(format!("OpenSign/{}", crate::env::app_version()))
        .dns_resolver(Arc::new(FilteringResolver))
        .redirect(reqwest::redirect::Policy::none())
        .https_only(true)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .no_proxy()
        .identity(identity);
    if let Some(roots) = roots.filter(|r| !r.trim().is_empty()) {
        let certs = reqwest::Certificate::from_pem_bundle(roots.as_bytes()).map_err(|_| "The CA certificate must be PEM".to_string())?;
        if certs.is_empty() {
            return Err("The CA certificate must be PEM".into());
        }
        builder = builder.tls_built_in_root_certs(false);
        for cert in certs {
            builder = builder.add_root_certificate(cert);
        }
    }
    builder.build().map_err(|e| format!("Could not set up the connection: {e}"))
}

/// Literal IPs in a URL bypass DNS, so they are checked here as well.
pub fn check_public_url(url: &str) -> Result<reqwest::Url, String> {
    check_outbound_url(url, false)
}

/// As `check_public_url`, also allowing plain http for services that sign what they return (time
/// stamp authorities).
pub fn check_outbound_url(url: &str, allow_http: bool) -> Result<reqwest::Url, String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "Enter a valid URL".to_string())?;
    if parsed.scheme() != "https" && !(allow_http && parsed.scheme() == "http") {
        return Err(if allow_http { "Use an http or https URL" } else { "Use an https URL" }.into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("Leave credentials out of the URL".into());
    }
    match parsed.host() {
        Some(url::Host::Ipv4(ip)) if is_forbidden(IpAddr::V4(ip)) => Err("That address is private or reserved".into()),
        Some(url::Host::Ipv6(ip)) if is_forbidden(IpAddr::V6(ip)) => Err("That address is private or reserved".into()),
        Some(url::Host::Domain(d)) if d == "localhost" || d.ends_with(".localhost") || d.ends_with(".internal") => {
            Err("That address is private or reserved".into())
        }
        None => Err("Enter a valid URL".into()),
        _ => Ok(parsed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_private_and_metadata_addresses() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:10.0.0.1",
            "::10.0.0.1",
            "2002:a00:1::1",
            "2001::1",
        ] {
            assert!(is_forbidden(ip.parse().expect("ip")), "{ip}");
        }
        for ip in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(!is_forbidden(ip.parse().expect("ip")), "{ip}");
        }
    }

    #[test]
    fn checks_urls() {
        assert!(check_public_url("https://hooks.example.com/x").is_ok());
        assert!(check_public_url("http://hooks.example.com/x").is_err());
        assert!(check_public_url("https://169.254.169.254/latest").is_err());
        assert!(check_public_url("https://[::1]/").is_err());
        assert!(check_public_url("https://user:pw@example.com/").is_err());
        assert!(check_public_url("https://localhost/").is_err());
        assert!(check_outbound_url("http://timestamp.digicert.com", true).is_ok());
        assert!(check_outbound_url("http://169.254.169.254/latest", true).is_err());
        assert!(check_outbound_url("ftp://example.com/", true).is_err());
    }

    #[tokio::test]
    async fn refuses_hostnames_that_resolve_privately() {
        let http = SafeHttp::new();
        let err = http.public.get("http://localhost:9/").send().await.expect_err("blocked");
        assert!(format!("{err:?}").contains("no public address"));
    }

    #[tokio::test]
    async fn a_private_name_and_a_missing_one_look_the_same() {
        let http = SafeHttp::new();
        for host in ["localhost", "no-such-name.invalid"] {
            let err = http.public.get(format!("http://{host}:9/")).send().await.expect_err("blocked");
            assert!(format!("{err:?}").contains(&format!("{host} has no public address")), "{err:?}");
        }
    }

    #[test]
    fn an_ai_proxy_may_be_private_but_not_a_metadata_address() {
        for good in ["http://10.0.0.5:4000", "http://192.168.1.2/v1", "http://127.0.0.1:4000", "https://litellm.internal"] {
            assert!(check_ai_proxy_url(good).is_ok(), "{good}");
        }
        for bad in [
            "http://169.254.169.254/latest",
            "http://[fe80::1]/",
            "http://[::ffff:169.254.169.254]/",
            "http://[fd00:ec2::254]/",
            "http://100.100.100.200/",
            "http://168.63.129.16/",
            "ftp://10.0.0.5/",
            "http://user@10.0.0.5/",
        ] {
            assert!(check_ai_proxy_url(bad).is_err(), "{bad}");
        }
    }
}

//! Resolve .local hosts directly to verified LAN addresses at connection time.
//! Never validate DNS and then let a second lookup choose the actual destination.
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

pub fn private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_private(),
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local(),
    }
}
pub fn local_hostname(host: &str) -> bool {
    host.len() <= 253
        && host.ends_with(".local")
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

struct SystemResolver;
impl Resolve for SystemResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let addresses = tokio::net::lookup_host((name.as_str(), 0)).await?;
            Ok(Box::new(addresses.collect::<Vec<_>>().into_iter()) as Addrs)
        })
    }
}
struct LanResolver(Arc<dyn Resolve>);
impl Resolve for LanResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let local = local_hostname(name.as_str());
        let lookup = self.0.resolve(name);
        Box::pin(async move {
            let addresses: Vec<SocketAddr> = tokio::time::timeout(Duration::from_secs(5), lookup)
                .await??
                .collect();
            // Reject mixed public/private answers instead of silently accepting one.
            if addresses.is_empty()
                || (local && addresses.iter().any(|address| !private_ip(address.ip())))
            {
                return Err(std::io::Error::other(
                    "Local server DNS must resolve only to loopback or private LAN addresses.",
                )
                .into());
            }
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}
pub fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .dns_resolver(Arc::new(LanResolver(Arc::new(SystemResolver))))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(Vec<SocketAddr>);
    impl Resolve for Fixture {
        fn resolve(&self, _: Name) -> Resolving {
            let addresses = self.0.clone();
            Box::pin(async move { Ok(Box::new(addresses.into_iter()) as Addrs) })
        }
    }
    #[tokio::test]
    async fn local_resolution_returns_only_the_checked_addresses_and_rejects_mixed_answers() {
        for (host, addresses, allowed) in [
            (
                "fixture.local",
                vec!["192.168.1.10:0", "[fd00::10]:0"],
                true,
            ),
            ("fixture.local", vec!["192.168.1.10:0", "8.8.8.8:0"], false),
            ("fixture.local", vec!["169.254.169.254:0"], false),
            ("fixture.local", vec!["[::ffff:8.8.8.8]:0"], false),
            ("fixture.local", vec![], false),
            ("cloud.example", vec!["8.8.8.8:0"], true),
        ] {
            let addresses = addresses
                .into_iter()
                .map(|value| value.parse().unwrap())
                .collect::<Vec<_>>();
            let resolver = LanResolver(Arc::new(Fixture(addresses.clone())));
            let result = resolver.resolve(host.parse().unwrap()).await;
            assert_eq!(result.is_ok(), allowed);
            if allowed {
                assert_eq!(result.unwrap().collect::<Vec<_>>(), addresses);
            }
        }
    }
}

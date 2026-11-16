//! Whether this machine has a route to the internet at all (#105). Sync only
//! notices a lost connection when its long poll times out (about 30 s with
//! Wi-Fi off, since requests hang rather than fail); this check sees it within
//! seconds. It asks the OS about routes only: no traffic, no hostnames.
//!
//! A default route is the test, not interface addresses: VM bridges and VPN
//! tunnels keep addresses when every real link is down.
use std::time::Duration;

const INTERVAL: Duration = Duration::from_secs(2);

/// macOS: the system's reachability for "any address", which is reachable
/// exactly when a default route exists without needing a connection first.
#[cfg(target_os = "macos")]
pub fn available() -> bool {
    use system_configuration::network_reachability::{ReachabilityFlags, SCNetworkReachability};
    let any = std::net::SocketAddr::from(([0, 0, 0, 0], 0));
    match SCNetworkReachability::from(any).reachability() {
        Ok(flags) => {
            flags.contains(ReachabilityFlags::REACHABLE)
                && !flags.contains(ReachabilityFlags::CONNECTION_REQUIRED)
        }
        // Unknown: let sync decide rather than claim there is no network.
        Err(_) => true,
    }
}

/// Linux: an IPv4 or IPv6 default route through anything but loopback.
#[cfg(target_os = "linux")]
pub fn available() -> bool {
    let v4 = std::fs::read_to_string("/proc/net/route");
    let v6 = std::fs::read_to_string("/proc/net/ipv6_route");
    if v4.is_err() && v6.is_err() {
        return true;
    }
    v4.as_deref().is_ok_and(has_default_v4) || v6.as_deref().is_ok_and(has_default_v6)
}

/// No route check on this platform yet; sync's own timeout applies.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn available() -> bool {
    true
}

/// `/proc/net/route`: Iface, Destination (hex), Gateway, Flags (hex), …
/// A default route has destination 00000000 and the RTF_UP flag.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn has_default_v4(table: &str) -> bool {
    table.lines().skip(1).any(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        fields.len() > 3
            && fields[0] != "lo"
            && fields[1] == "00000000"
            && u32::from_str_radix(fields[3], 16).is_ok_and(|flags| flags & 0x1 != 0)
    })
}

/// `/proc/net/ipv6_route`: destination (32 hex), prefix length, …, flags, iface.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn has_default_v6(table: &str) -> bool {
    table.lines().any(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        fields.len() == 10
            && fields[0].chars().all(|c| c == '0')
            && fields[1] == "00"
            && fields[9] != "lo"
            && u32::from_str_radix(fields[8], 16).is_ok_and(|flags| flags & 0x1 != 0)
    })
}

/// Calls `changed` with the current state, then whenever it changes.
pub async fn run(mut changed: impl FnMut(bool)) {
    let mut last = None;
    let mut tick = tokio::time::interval(INTERVAL);
    loop {
        tick.tick().await;
        let now = tokio::task::spawn_blocking(available).await.unwrap_or(true);
        if last != Some(now) {
            last = Some(now);
            changed(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{has_default_v4, has_default_v6};

    const HEADER: &str =
        "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n";

    #[test]
    fn a_default_route_through_a_real_link_counts() {
        let with = format!(
            "{HEADER}wlan0\t00000000\t0101A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0\n\
             wlan0\t0001A8C0\t00000000\t0001\t0\t0\t600\t00FFFFFF\t0\t0\t0\n"
        );
        assert!(has_default_v4(&with));
        // Only a VM bridge's own subnet: no way out.
        let bridge =
            format!("{HEADER}virbr0\t007AA8C0\t00000000\t0001\t0\t0\t0\t00FFFFFF\t0\t0\t0\n");
        assert!(!has_default_v4(&bridge));
        // A default route that is down does not count.
        let down =
            format!("{HEADER}eth0\t00000000\t0101A8C0\t0002\t0\t0\t100\t00000000\t0\t0\t0\n");
        assert!(!has_default_v4(&down));
    }

    #[test]
    fn ipv6_default_routes_skip_loopback() {
        let zeros = "00000000000000000000000000000000";
        let up = format!(
            "{zeros} 00 {zeros} 00 fe800000000000000000000000000001 00000400 00000001 00000000 00000003 wlan0\n"
        );
        assert!(has_default_v6(&up));
        let lo = format!("{zeros} 00 {zeros} 00 {zeros} ffffffff 00000001 00000000 00200200 lo\n");
        assert!(!has_default_v6(&lo));
    }

    #[test]
    fn this_machine_answers_without_panicking() {
        let _ = super::available();
    }
}

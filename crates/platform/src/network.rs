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

/// Windows: a default route (IPv4 or IPv6) on an interface that is up, has
/// its medium connected and is not loopback. Unknown counts as available.
#[cfg(target_os = "windows")]
pub fn available() -> bool {
    win::default_route_up().unwrap_or(true)
}

#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "GetIpForwardTable2 hands back an OS-allocated table that is read in place \
              and freed once with FreeMibTable; GetIfEntry2 fills a caller-owned row"
)]
mod win {
    use windows::Win32::Foundation::NO_ERROR;
    use windows::Win32::NetworkManagement::IpHelper::{
        FreeMibTable, GetIfEntry2, GetIpForwardTable2, IF_TYPE_SOFTWARE_LOOPBACK, MIB_IF_ROW2,
        MIB_IPFORWARD_ROW2, MIB_IPFORWARD_TABLE2,
    };
    use windows::Win32::NetworkManagement::Ndis::{IfOperStatusUp, MediaConnectStateConnected};
    use windows::Win32::Networking::WinSock::AF_UNSPEC;

    pub(super) fn default_route_up() -> Option<bool> {
        let mut table: *mut MIB_IPFORWARD_TABLE2 = std::ptr::null_mut();
        // SAFETY: on success `table` points to an OS-owned table of
        // `NumEntries` rows, valid until FreeMibTable below.
        if unsafe { GetIpForwardTable2(AF_UNSPEC, &mut table) } != NO_ERROR || table.is_null() {
            return None;
        }
        let up = {
            let rows = unsafe {
                std::slice::from_raw_parts(
                    std::ptr::addr_of!((*table).Table).cast::<MIB_IPFORWARD_ROW2>(),
                    (*table).NumEntries as usize,
                )
            };
            rows.iter().any(|route| {
                route.DestinationPrefix.PrefixLength == 0 && !route.Loopback && interface_up(route)
            })
        };
        unsafe { FreeMibTable(table as *const std::ffi::c_void) };
        Some(up)
    }

    /// A Wi-Fi adapter that lost its network, or a cable that was pulled,
    /// reports its medium as disconnected even while a static route remains.
    fn interface_up(route: &MIB_IPFORWARD_ROW2) -> bool {
        let mut row = MIB_IF_ROW2 {
            InterfaceLuid: route.InterfaceLuid,
            InterfaceIndex: route.InterfaceIndex,
            ..Default::default()
        };
        let found = unsafe { GetIfEntry2(&mut row) } == NO_ERROR;
        found
            && row.Type != IF_TYPE_SOFTWARE_LOOPBACK
            && row.OperStatus == IfOperStatusUp
            && row.MediaConnectState == MediaConnectStateConnected
    }
}

/// No route check on this platform yet; sync's own timeout applies.
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
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

/// Whether the current connection is metered or in a low-data mode, so
/// media should not download on its own (#174). `None` when the system does
/// not say; callers treat that as unmetered.
#[cfg(target_os = "macos")]
pub async fn metered() -> Option<bool> {
    tokio::task::spawn_blocking(mac::metered)
        .await
        .ok()
        .flatten()
}

/// Linux: NetworkManager's overall `Metered` property (yes or guessed yes).
#[cfg(target_os = "linux")]
pub async fn metered() -> Option<bool> {
    let connection = zbus::Connection::system().await.ok()?;
    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.NetworkManager",
    )
    .await
    .ok()?;
    let value: u32 = proxy.get_property("Metered").await.ok()?;
    nm_metered(value)
}

/// Windows: the internet profile's connection cost (fixed or variable plans,
/// roaming, or near or over the data limit).
#[cfg(target_os = "windows")]
pub async fn metered() -> Option<bool> {
    tokio::task::spawn_blocking(|| {
        use windows::Networking::Connectivity::{NetworkCostType, NetworkInformation};
        let cost = NetworkInformation::GetInternetConnectionProfile()
            .ok()?
            .GetConnectionCost()
            .ok()?;
        let kind = cost.NetworkCostType().ok()?;
        Some(
            matches!(kind, NetworkCostType::Fixed | NetworkCostType::Variable)
                || cost.Roaming().unwrap_or(false)
                || cost.OverDataLimit().unwrap_or(false)
                || cost.ApproachingDataLimit().unwrap_or(false),
        )
    })
    .await
    .ok()
    .flatten()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub async fn metered() -> Option<bool> {
    None
}

/// NetworkManager's NMMetered: 1 yes, 2 no, 3 guessed yes, 4 guessed no.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn nm_metered(value: u32) -> Option<bool> {
    match value {
        1 | 3 => Some(true),
        2 | 4 => Some(false),
        _ => None,
    }
}

/// Calls `changed` with the metered state, then whenever it changes.
pub async fn run_metered(mut changed: impl FnMut(Option<bool>)) {
    let mut last = None;
    let mut tick = tokio::time::interval(Duration::from_secs(15));
    loop {
        tick.tick().await;
        let now = metered().await;
        if last != Some(now) {
            last = Some(now);
            changed(now);
        }
    }
}

#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "Network.framework path monitor C API: the monitor is created, \
              started, cancelled and released here; the path is only read \
              inside its update handler"
)]
mod mac {
    use std::ffi::c_void;
    use std::time::Duration;

    #[link(name = "Network", kind = "framework")]
    unsafe extern "C" {
        fn nw_path_monitor_create() -> *mut c_void;
        fn nw_path_monitor_set_queue(monitor: *mut c_void, queue: *mut c_void);
        fn nw_path_monitor_set_update_handler(
            monitor: *mut c_void,
            handler: &block2::Block<dyn Fn(*mut c_void)>,
        );
        fn nw_path_monitor_start(monitor: *mut c_void);
        fn nw_path_monitor_cancel(monitor: *mut c_void);
        fn nw_path_is_expensive(path: *mut c_void) -> bool;
        fn nw_path_is_constrained(path: *mut c_void) -> bool;
        fn nw_release(object: *mut c_void);
    }
    unsafe extern "C" {
        fn dispatch_get_global_queue(identifier: isize, flags: usize) -> *mut c_void;
    }

    /// One reading of the current path: expensive (cellular, personal
    /// hotspot) or constrained (Low Data Mode).
    pub(super) fn metered() -> Option<bool> {
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        let handler = block2::RcBlock::new(move |path: *mut c_void| {
            // SAFETY: the path is valid for the duration of the handler.
            let metered = unsafe { nw_path_is_expensive(path) || nw_path_is_constrained(path) };
            let _ = send.try_send(metered);
        });
        // SAFETY: the monitor is a fresh object owned here; the handler block
        // is copied by Network.framework; cancel stops further callbacks and
        // the release balances create.
        unsafe {
            let monitor = nw_path_monitor_create();
            if monitor.is_null() {
                return None;
            }
            nw_path_monitor_set_queue(monitor, dispatch_get_global_queue(0, 0));
            nw_path_monitor_set_update_handler(monitor, &handler);
            nw_path_monitor_start(monitor);
            let reading = receive.recv_timeout(Duration::from_secs(2)).ok();
            nw_path_monitor_cancel(monitor);
            nw_release(monitor);
            reading
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

    #[test]
    fn network_manager_metered_values() {
        assert_eq!(super::nm_metered(1), Some(true));
        assert_eq!(super::nm_metered(3), Some(true));
        assert_eq!(super::nm_metered(2), Some(false));
        assert_eq!(super::nm_metered(4), Some(false));
        assert_eq!(super::nm_metered(0), None);
    }

    #[tokio::test]
    async fn this_machine_reports_metering_without_panicking() {
        eprintln!("metered: {:?}", super::metered().await);
    }
}

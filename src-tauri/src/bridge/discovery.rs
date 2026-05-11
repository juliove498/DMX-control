//! mDNS / Bonjour advertisement of the bridge service. iOS publishes
//! and resolves Bonjour out of the box; on Mac the system mDNSResponder
//! handles it; on Windows mdns-sd does it in-process so we don't need
//! the user to install Bonjour Print Services. The Windows Firewall
//! prompt on first run is the only real friction point, and the UI
//! always shows the literal IP+port as a fallback the user can type
//! into the phone.

use std::sync::Arc;

use mdns_sd::{ServiceDaemon, ServiceInfo};
use parking_lot::Mutex;

const SERVICE_TYPE: &str = "_dmxctrl._tcp.local.";

#[derive(Default)]
pub struct Discovery {
    daemon: Mutex<Option<ServiceDaemon>>,
    instance: Mutex<Option<String>>,
}

pub type SharedDiscovery = Arc<Discovery>;

pub fn shared_discovery() -> SharedDiscovery {
    Arc::new(Discovery::default())
}

impl Discovery {
    pub fn start(&self, port: u16, host_ip: &str, instance_name: &str) -> Result<(), String> {
        // Stop any prior advertisement so a restart on a different
        // port doesn't leave a stale record on the network.
        self.stop();
        let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
        let host_name = format!("{}.local.", instance_name);
        let info = ServiceInfo::new(SERVICE_TYPE, instance_name, &host_name, host_ip, port, None)
            .map_err(|e| e.to_string())?
            .enable_addr_auto();
        daemon.register(info).map_err(|e| e.to_string())?;
        *self.daemon.lock() = Some(daemon);
        *self.instance.lock() = Some(instance_name.to_string());
        tracing::info!(%port, %host_ip, %instance_name, "mDNS advertising _dmxctrl._tcp.local.");
        Ok(())
    }

    pub fn stop(&self) {
        let daemon_opt = self.daemon.lock().take();
        let instance_opt = self.instance.lock().take();
        if let (Some(daemon), Some(instance)) = (daemon_opt, instance_opt) {
            // `unregister` is best-effort; if the daemon already shut
            // down we don't care about the error.
            let full = format!("{}.{}", instance, SERVICE_TYPE);
            let _ = daemon.unregister(&full);
            let _ = daemon.shutdown();
            tracing::info!(%instance, "mDNS unregistered");
        }
    }
}

impl Drop for Discovery {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Pick the best non-loopback IPv4 address to advertise. Prefers
/// private RFC1918 ranges (likely the venue WiFi) over anything
/// public. Falls back to `0.0.0.0` so mdns-sd still publishes a
/// record — better something than nothing if all interfaces look weird.
pub fn pick_advertise_ip() -> String {
    use local_ip_address::list_afinet_netifas;
    let Ok(ifs) = list_afinet_netifas() else {
        return "0.0.0.0".to_string();
    };
    let mut candidates: Vec<std::net::Ipv4Addr> = ifs
        .into_iter()
        .filter_map(|(_, addr)| match addr {
            std::net::IpAddr::V4(v4) if !v4.is_loopback() && !v4.is_unspecified() => Some(v4),
            _ => None,
        })
        .collect();
    // Sort: private first (most likely to be the LAN we want), then
    // everything else. Within each group, deterministic by octets.
    candidates.sort_by_key(|ip| {
        let private = ip.is_private();
        (!private, ip.octets())
    });
    candidates
        .first()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "0.0.0.0".to_string())
}

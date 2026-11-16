//! Announcing the server on the local network, so tablets and control
//! surfaces find it without anyone typing an address.
//!
//! Two services are published over mDNS: `_mazzalab._tcp` with the HTTP
//! port (the operator screen and the client API) and `_osc._udp` with the
//! OSC port. Nothing is announced under any other product's name.

use std::net::IpAddr;

use mdns_sd::{ServiceDaemon, ServiceInfo};

/// The service type of the web UI and client API.
pub const MAZZALAB_SERVICE: &str = "_mazzalab._tcp.local.";
/// The service type of OSC remote control.
pub const OSC_SERVICE: &str = "_osc._udp.local.";

/// The names a machine is announced under: the instance name people see in a
/// browser of services, and the host name its address is published for.
pub fn names(machine: &str) -> (String, String) {
    let machine = machine.trim();
    let machine = if machine.is_empty() {
        "show-machine"
    } else {
        machine
    };
    // A host label cannot carry dots, spaces or most punctuation.
    let label: String = machine
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let label = label.trim_matches('-');
    let label = if label.is_empty() {
        "show-machine"
    } else {
        label
    };
    (format!("MazzaLab on {machine}"), format!("{label}.local."))
}

/// This machine's name, from the environment.
fn machine_name() -> String {
    ["COMPUTERNAME", "HOSTNAME", "HOST"]
        .iter()
        .find_map(|name| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_default()
}

/// Ends the announcement when dropped.
pub struct Announcement {
    daemon: ServiceDaemon,
}

impl std::fmt::Debug for Announcement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Announcement").finish_non_exhaustive()
    }
}

impl Drop for Announcement {
    fn drop(&mut self) {
        // Says goodbye on the network; the daemon's thread then ends.
        let _ = self.daemon.shutdown();
    }
}

/// Publishes the web and OSC services. `bind` is the address the server
/// listens on: a specific one is announced as it is, the unspecified one
/// (all) is announced on every interface of the machine.
pub fn announce(
    bind: IpAddr,
    http_port: u16,
    osc_port: u16,
) -> Result<Announcement, mdns_sd::Error> {
    let daemon = ServiceDaemon::new()?;
    let (instance, host) = names(&machine_name());
    let version = env!("CARGO_PKG_VERSION");
    let address = if bind.is_unspecified() {
        String::new()
    } else {
        bind.to_string()
    };
    let publish =
        |service: &str, port: u16, properties: &[(&str, &str)]| -> Result<(), mdns_sd::Error> {
            let info = ServiceInfo::new(
                service,
                &instance,
                &host,
                address.as_str(),
                port,
                properties,
            )?;
            daemon.register(if bind.is_unspecified() {
                info.enable_addr_auto()
            } else {
                info
            })
        };
    publish(
        MAZZALAB_SERVICE,
        http_port,
        &[("version", version), ("path", "/")],
    )?;
    publish(OSC_SERVICE, osc_port, &[("version", version)])?;
    Ok(Announcement { daemon })
}

#[cfg(test)]
mod tests {
    use super::names;

    #[test]
    fn instance_and_host_come_from_the_machine_name() {
        assert_eq!(
            names("FOH-Laptop"),
            ("MazzaLab on FOH-Laptop".into(), "FOH-Laptop.local.".into())
        );
    }

    #[test]
    fn host_labels_lose_what_dns_refuses() {
        let (instance, host) = names("Régie son 2.0");
        assert_eq!(instance, "MazzaLab on Régie son 2.0");
        assert_eq!(host, "R-gie-son-2-0.local.");
    }

    #[test]
    fn a_machine_without_a_name_still_gets_one() {
        assert_eq!(
            names("  "),
            (
                "MazzaLab on show-machine".into(),
                "show-machine.local.".into()
            )
        );
        assert_eq!(names("...").1, "show-machine.local.");
    }
}

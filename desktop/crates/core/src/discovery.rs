//! Co 2 s rozgłasza `announce` na UDP 47811, żeby telefon sam znalazł komputer.

use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::proto::DISCOVERY_PORT;

/// Adresy IPv4 aktywnych kart (bez loopbacka i APIPA) z adresem rozgłoszeniowym.
pub fn local_ipv4() -> Vec<(Ipv4Addr, Ipv4Addr)> {
    let mut out = Vec::new();
    for iface in if_addrs::get_if_addrs().unwrap_or_default() {
        if iface.is_loopback() {
            continue;
        }
        if let if_addrs::IfAddr::V4(v4) = iface.addr {
            let o = v4.ip.octets();
            if o[0] == 169 && o[1] == 254 {
                continue;
            }
            let b = v4.broadcast.unwrap_or_else(|| {
                let (a, m) = (u32::from(v4.ip), u32::from(v4.netmask));
                Ipv4Addr::from(a | !m)
            });
            out.push((v4.ip, b));
        }
    }
    out
}

pub fn local_addresses() -> Vec<String> {
    let mut seen = HashSet::new();
    local_ipv4().into_iter().map(|(a, _)| a.to_string()).filter(|a| seen.insert(a.clone())).collect()
}

pub struct Discovery {
    stop: Arc<AtomicBool>,
}

impl Discovery {
    /// `message` – treść `announce` (budowana od nowa co 2 s: źródło i adresy mogą się zmienić);
    /// `None` = nic nie rozgłaszaj (np. serwer nie nasłuchuje, bo port był zajęty).
    pub fn start(message: impl Fn() -> Option<String> + Send + 'static) -> Discovery {
        let stop = Arc::new(AtomicBool::new(false));
        let s = stop.clone();
        std::thread::Builder::new()
            .name("discovery".into())
            .spawn(move || {
                let Ok(sock) = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)) else {
                    crate::log::warn("Rozgłaszanie: nie można otworzyć gniazda UDP");
                    return;
                };
                let _ = sock.set_broadcast(true);
                let mut warned = false;
                while !s.load(Ordering::SeqCst) {
                    if let Some(payload) = message() {
                        let mut targets: HashSet<Ipv4Addr> = HashSet::from([Ipv4Addr::BROADCAST]);
                        targets.extend(local_ipv4().into_iter().map(|(_, b)| b));
                        for t in targets {
                            if let Err(e) = sock.send_to(payload.as_bytes(), SocketAddrV4::new(t, DISCOVERY_PORT)) {
                                if !warned {
                                    warned = true;
                                    crate::log::warn(&crate::trf!("Rozgłaszanie w sieci nie działa: {e}"));
                                }
                            }
                        }
                    }
                    for _ in 0..20 {
                        if s.load(Ordering::SeqCst) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            })
            .ok();
        Discovery { stop }
    }
}

impl Drop for Discovery {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

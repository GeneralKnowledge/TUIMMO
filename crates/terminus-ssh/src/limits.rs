//! Connection and authentication-failure limits for the SSH gateway.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct Limits {
    pub max_connections: usize,
    pub max_per_ip: usize,
    pub failures_per_minute: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_connections: 64, max_per_ip: 4, failures_per_minute: 8 }
    }
}

#[derive(Debug)]
pub struct GateState {
    limits: Limits,
    total: usize,
    per_ip: HashMap<IpAddr, usize>,
    failures: HashMap<IpAddr, Vec<Instant>>,
}

impl GateState {
    pub fn new(limits: Limits) -> Self {
        Self { limits, total: 0, per_ip: HashMap::new(), failures: HashMap::new() }
    }

    pub fn try_acquire(&mut self, addr: Option<SocketAddr>) -> bool {
        if self.total >= self.limits.max_connections {
            return false;
        }
        if let Some(ip) = addr.map(|addr| addr.ip()) {
            let count = self.per_ip.get(&ip).copied().unwrap_or(0);
            if count >= self.limits.max_per_ip {
                return false;
            }
            self.per_ip.insert(ip, count + 1);
        }
        self.total += 1;
        true
    }

    pub fn release(&mut self, addr: Option<SocketAddr>) {
        self.total = self.total.saturating_sub(1);
        if let Some(ip) = addr.map(|addr| addr.ip()) {
            if let Some(count) = self.per_ip.get_mut(&ip) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.per_ip.remove(&ip);
                }
            }
        }
    }

    pub fn record_failure(&mut self, addr: Option<SocketAddr>) {
        let Some(ip) = addr.map(|addr| addr.ip()) else {
            return;
        };
        let now = Instant::now();
        let window = self.failures.entry(ip).or_default();
        window.push(now);
        prune(window, now);
    }

    pub fn blocked(&mut self, addr: Option<SocketAddr>) -> bool {
        let Some(ip) = addr.map(|addr| addr.ip()) else {
            return false;
        };
        let now = Instant::now();
        let window = self.failures.entry(ip).or_default();
        prune(window, now);
        window.len() >= self.limits.failures_per_minute
    }
}

fn prune(window: &mut Vec<Instant>, now: Instant) {
    window.retain(|instant| now.saturating_duration_since(*instant) < Duration::from_secs(60));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1)
    }

    #[test]
    fn caps_connections_and_failures() {
        let mut gate =
            GateState::new(Limits { max_connections: 2, max_per_ip: 1, failures_per_minute: 2 });
        assert!(gate.try_acquire(Some(addr())));
        assert!(!gate.try_acquire(Some(addr())));
        gate.release(Some(addr()));
        assert!(gate.try_acquire(Some(addr())));
        gate.record_failure(Some(addr()));
        gate.record_failure(Some(addr()));
        assert!(gate.blocked(Some(addr())));
    }
}

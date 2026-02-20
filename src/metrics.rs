use std::sync::atomic::{AtomicU64, Ordering};

pub struct RoutingSnapshot {
    pub keyed_total: u64,
    pub keyed_local: u64,
    pub keyed_remote: u64,
    pub direct: u64,
}

impl RoutingSnapshot {
    pub fn local_pct(&self) -> f64 {
        if self.keyed_total == 0 {
            return 0.0;
        }
        (self.keyed_local as f64 * 100.0) / self.keyed_total as f64
    }

    pub fn remote_pct(&self) -> f64 {
        if self.keyed_total == 0 {
            return 0.0;
        }
        (self.keyed_remote as f64 * 100.0) / self.keyed_total as f64
    }
}

pub struct RoutingMetrics {
    keyed_total: AtomicU64,
    keyed_local: AtomicU64,
    keyed_remote: AtomicU64,
    direct: AtomicU64,
}

impl RoutingMetrics {
    const REPORT_EVERY_KEYED: u64 = 1_000_000;

    pub const fn new() -> Self {
        Self {
            keyed_total: AtomicU64::new(0),
            keyed_local: AtomicU64::new(0),
            keyed_remote: AtomicU64::new(0),
            direct: AtomicU64::new(0),
        }
    }

    pub fn record_keyed_local(&self) {
        let keyed_total = self.keyed_total.fetch_add(1, Ordering::Relaxed) + 1;
        self.keyed_local.fetch_add(1, Ordering::Relaxed);
        self.maybe_report(keyed_total);
    }

    pub fn record_keyed_remote(&self) {
        let keyed_total = self.keyed_total.fetch_add(1, Ordering::Relaxed) + 1;
        self.keyed_remote.fetch_add(1, Ordering::Relaxed);
        self.maybe_report(keyed_total);
    }

    pub fn record_direct(&self) {
        self.direct.fetch_add(1, Ordering::Relaxed);
    }

    fn maybe_report(&self, keyed_total: u64) {
        if !keyed_total.is_multiple_of(Self::REPORT_EVERY_KEYED) {
            return;
        }

        let snapshot = self.snapshot();
        eprintln!(
            "[routing] keyed_total={} local={} ({:.2}%) remote={} ({:.2}%) direct={}",
            snapshot.keyed_total,
            snapshot.keyed_local,
            snapshot.local_pct(),
            snapshot.keyed_remote,
            snapshot.remote_pct(),
            snapshot.direct,
        );
    }

    pub fn snapshot(&self) -> RoutingSnapshot {
        RoutingSnapshot {
            keyed_total: self.keyed_total.load(Ordering::Relaxed),
            keyed_local: self.keyed_local.load(Ordering::Relaxed),
            keyed_remote: self.keyed_remote.load(Ordering::Relaxed),
            direct: self.direct.load(Ordering::Relaxed),
        }
    }
}

impl Default for RoutingMetrics {
    fn default() -> Self {
        Self::new()
    }
}

pub static ROUTING_METRICS: RoutingMetrics = RoutingMetrics::new();

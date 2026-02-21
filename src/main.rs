use rustis::{metrics::ROUTING_METRICS, threads::spawn_threads};
#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn main() {
    spawn_threads();
    let routing = ROUTING_METRICS.snapshot();
    eprintln!(
        "[routing-final] keyed_total={} local={} ({:.2}%) remote={} ({:.2}%) direct={}",
        routing.keyed_total,
        routing.keyed_local,
        routing.local_pct(),
        routing.keyed_remote,
        routing.remote_pct(),
        routing.direct,
    );
}

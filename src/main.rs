use std::sync::Arc;

use rustis::{connection::spawn_io, threads::spawn_threads};
#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;
use tokio::runtime::Builder;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn main() {
    // spawn threads
    let vec_router = spawn_threads();

    let router = Arc::new(vec_router);

    let runtime = Builder::new_current_thread().enable_all().build().unwrap();

    runtime.block_on(spawn_io(router)).unwrap();
}

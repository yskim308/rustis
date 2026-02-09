use std::sync::Arc;

use core_affinity;
use rtrb::RingBuffer;
use thread_priority::{set_current_thread_priority, ThreadPriority};
use tokio::task::LocalSet;

use crate::{connection::spawn_io, message::WorkerMessage, worker::worker_main};

pub fn spawn_threads() {
    let core_ids = core_affinity::get_core_ids().unwrap();
    let num_cores = core_ids.len();

    let mut txs = Vec::with_capacity(num_cores);
    let mut rxs = Vec::with_capacity(num_cores);

    for _ in 0..num_cores {
        let mut tx_vec = Vec::with_capacity(num_cores);
        let mut rx_vec = Vec::with_capacity(num_cores);
        for _ in 0..num_cores {
            let (tx, rx) = RingBuffer::<WorkerMessage>::new(4096);
            tx_vec.push(tx);
            rx_vec.push(rx);
        }
        txs.push(tx_vec);
        rxs.push(rx_vec);
    }

    for core_id in core_ids.into_iter() {
        let mailbox = rxs.remove(0);
        let outbox = txs.remove(0);

        std::thread::spawn(move || {
            let router = Arc::new(outbox);
            if let Err(err) = set_current_thread_priority(ThreadPriority::Max) {
                eprintln!("Warning: failed to set priority to thread {:?}", err);
            }

            #[cfg(target_os = "linux")]
            if !core_affinity::set_for_current(core_id) {
                eprintln!("failed to pin thread to core: {:?}", core_id);
            }
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            let local = LocalSet::new();

            // spawn worker / poller
            local.spawn_local(worker_main(core_id.id, mailbox));
            local.spawn_local(spawn_io(router));

            rt.block_on(local);
        });
    }
}

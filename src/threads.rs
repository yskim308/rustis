use core_affinity;
use crossbeam::channel::{self, Sender};
use thread_priority::{set_current_thread_priority, ThreadPriority};

use crate::{message::WorkerMessage, worker::worker_main};

pub fn spawn_threads() -> Vec<Vec<Sender<WorkerMessage>>> {
    let core_ids = core_affinity::get_core_ids().unwrap();
    let num_cores = core_ids.len();

    let mut txs = Vec::with_capacity(num_cores);
    let mut rxs = Vec::with_capacity(num_cores);

    for _ in 0..num_cores {
        let mut tx_vec = Vec::with_capacity(num_cores);
        let mut rx_vec = Vec::with_capacity(num_cores);
        for _ in 0..num_cores {
            let (tx, rx) = channel::unbounded::<WorkerMessage>();
            tx_vec.push(tx);
            rx_vec.push(rx);
        }
        txs.push(tx_vec);
        rxs.push(rx_vec);
    }

    for core_id in core_ids.into_iter() {
        let mailxbox = rxs.remove(0);

        std::thread::spawn(move || {
            if let Err(err) = set_current_thread_priority(ThreadPriority::Max) {
                eprintln!("Warning: failed to set priority to thread {:?}", err);
            }

            #[cfg(target_os = "linux")]
            if !core_affinity::set_for_current(core_id) {
                eprintln!("failed to pin thread to core: {:?}", core_id);
            }

            // worker_main(core_id.id, mailxbox);
        });
    }

    // return the router
    txs
}

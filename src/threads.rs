use core_affinity;
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::{message::WorkerMessage, worker::worker_main};

pub fn spawn_threads() -> Vec<UnboundedSender<WorkerMessage>> {
    let core_ids = core_affinity::get_core_ids().unwrap();
    let num_cores = core_ids.len();

    let mut txs = Vec::with_capacity(num_cores);
    let mut rxs = Vec::with_capacity(num_cores);

    for _ in 0..num_cores {
        let (tx, rx) = mpsc::unbounded_channel::<WorkerMessage>();
        txs.push(tx);
        rxs.push(rx);
    }

    for core_id in core_ids.into_iter() {
        let mailxbox = rxs.remove(0);

        std::thread::spawn(move || {
            if !core_affinity::set_for_current(core_id) {
                eprintln!("failed to pin thread to core: {:?}", core_id);
            }
            worker_main(core_id.id, mailxbox);
        });
    }

    // return the router
    txs
}

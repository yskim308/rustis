use std::{cell::RefCell, rc::Rc};

use core_affinity;
use rtrb::{Consumer, Producer, RingBuffer};
use thread_priority::{set_current_thread_priority, ThreadPriority};
use tokio::task::LocalSet;

use crate::{
    connection::spawn_io,
    message::{ResponseMessage, WorkerMessage},
    worker::worker_main,
};

pub type ProducerMesh<T> = Vec<Vec<Producer<T>>>;
pub type ConsumerMesh<T> = Vec<Vec<Consumer<T>>>;

pub fn spawn_threads() {
    let core_ids = core_affinity::get_core_ids().unwrap();
    let num_cores = core_ids.len();

    let (mut req_txs, mut req_rxs) = create_mesh::<WorkerMessage>(num_cores);
    let (mut resp_txs, mut resp_rxs) = create_mesh::<ResponseMessage>(num_cores);

    let mut handles = Vec::with_capacity(num_cores);

    for core_id in core_ids.into_iter() {
        let req_outbox = req_txs.remove(0);
        let req_inbox = req_rxs.remove(0);

        let resp_outbox = resp_txs.remove(0);
        let resp_inbox = resp_rxs.remove(0);

        let handle = std::thread::spawn(move || {
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

            // this router will be shared by all connections, wrap in Rc<RefCell<>>
            let sharded_router = Rc::new(RefCell::new(req_outbox));

            // // spawn worker / poller
            local.spawn_local(worker_main(core_id.id, req_inbox, resp_outbox));
            local.spawn_local(spawn_io(core_id.id, sharded_router, resp_inbox));
            //
            rt.block_on(local);
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }
}

fn create_mesh<T>(num_cores: usize) -> (ProducerMesh<T>, ConsumerMesh<T>) {
    let mut txs = Vec::with_capacity(num_cores);
    let mut rxs = Vec::with_capacity(num_cores);

    for _ in 0..num_cores {
        let mut tx_vec = Vec::with_capacity(num_cores);
        let mut rx_vec = Vec::with_capacity(num_cores);

        for _ in 0..num_cores {
            let (tx, rx) = RingBuffer::<T>::new(4096);
            tx_vec.push(tx);
            rx_vec.push(rx);
        }
        txs.push(tx_vec);
        rxs.push(rx_vec);
    }

    (txs, rxs)
}

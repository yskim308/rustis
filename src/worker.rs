use crossbeam::utils;

use rtrb::Consumer;

use crate::{
    handler::process_command,
    kv::KvStore,
    message::{ResponseMessage, WorkerMessage},
};

pub fn worker_main(_worker_id: usize, mut inboxes: Vec<Consumer<WorkerMessage>>) {
    let kv = KvStore::new();
    let backoff = utils::Backoff::new();

    loop {
        let mut processed = false;

        for inbox in inboxes.iter_mut() {
            if let Ok(mut msg) = inbox.pop() {
                let response = process_command(&kv, msg.response_value);
                let _ = msg.tx.push(ResponseMessage {
                    seq: msg.seq,
                    response_value: response,
                });
                processed = true;
                backoff.reset();
            }
        }

        // Adaptive Backoff
        if !processed {
            backoff.snooze();
        }
    }
}

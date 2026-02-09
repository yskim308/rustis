use rtrb::{Consumer, Producer};

use crate::{
    handler::process_command,
    kv::KvStore,
    message::{ResponseMessage, ResponseValue, WorkerMessage},
};

pub async fn worker_main(
    _worker_id: usize,
    mut inboxes: Vec<Consumer<WorkerMessage>>,
    mut resp_outboxes: Vec<Producer<ResponseMessage>>,
) {
    let kv = KvStore::new();

    loop {
        let mut processed = false;

        for inbox in inboxes.iter_mut() {
            if let Ok(msg) = inbox.pop() {
                let response = match msg.response_value {
                    ResponseValue::Array(_) => process_command(&kv, msg.response_value),
                    _ => msg.response_value,
                };
                // note: later, we should be using .get() and handling errors properly
                resp_outboxes[msg.src_core].push(ResponseMessage {
                    seq: msg.seq,
                    conn_token: msg.conn_token,
                    response_value: response,
                });
                processed = true;
            }
        }

        if !processed {
            tokio::task::yield_now().await;
        }
    }
}

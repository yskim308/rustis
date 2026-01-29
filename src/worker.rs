use tokio::{
    runtime::Builder,
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
};

use crate::{
    handler::process_command,
    kv::KvStore,
    message::{ResponseMessage, WorkerMessage},
};

pub fn worker_main(
    _worker_id: usize,
    mut rx: UnboundedReceiver<WorkerMessage>,
    tx: UnboundedSender<ResponseMessage>,
) {
    let kv = KvStore::new();

    let runtime = Builder::new_current_thread().enable_all().build().unwrap();

    runtime.block_on(async move {
        while let Some(msg) = rx.recv().await {
            let response = process_command(&kv, msg.response_value);
            let _ = tx.send(ResponseMessage {
                connection_id: msg.connection_id,
                seq: msg.seq,
                response_value: response,
            });
        }
    })
}

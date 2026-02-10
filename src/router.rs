use std::hash::{DefaultHasher, Hash, Hasher};

use crate::{
    connection::WorkerQueues,
    message::{ResponseValue, WorkerMessage},
};

pub struct MessageRouter {
    worker_queues: WorkerQueues,
    conn_token: usize,
    src_core: usize,
}

impl MessageRouter {
    pub fn new(worker_queues: WorkerQueues, conn_token: usize, src_core: usize) -> Self {
        MessageRouter {
            worker_queues,
            conn_token,
            src_core,
        }
    }

    pub fn route_message(&self, frame: ResponseValue, seq: u64) {
        let items = match &frame {
            ResponseValue::Array(Some(items)) => items,
            _ => {
                self.send_value_directly(
                    seq,
                    ResponseValue::Error("sent value must be of type Array".into()),
                );
                return;
            }
        };

        if items.is_empty() {
            self.send_value_directly(seq, ResponseValue::Error("request is empty".into()));
        }

        let key = self.extract_key(seq, items);

        let mut worker_queues = self.worker_queues.borrow_mut();
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let to_worker = hasher.finish() as usize % worker_queues.len();

        let destination_worker_queue = match worker_queues.get_mut(to_worker) {
            Some(queue) => queue,
            None => {
                self.send_value_directly(
                    seq,
                    ResponseValue::Error("internal server error, invalid worker index".into()),
                );
                return;
            }
        };

        destination_worker_queue.push(WorkerMessage {
            seq,
            conn_token: self.conn_token,
            src_core: self.src_core,
            response_value: frame,
        });
    }

    fn extract_key(&self, seq: u64, items: &[ResponseValue]) -> Option<bytes::Bytes> {
        let (cmd, args) = match items.split_first() {
            Some((ResponseValue::BulkString(Some(bytes)), rest)) => (bytes, rest),
            _ => {
                self.send_value_directly(
                    seq,
                    ResponseValue::Error("command must be bulk string".into()),
                );
                return None;
            }
        };

        if cmd.eq_ignore_ascii_case(b"PING") {
            self.send_value_directly(seq, ResponseValue::SimpleString("PONG".into()));
            return None;
        } else if cmd.eq_ignore_ascii_case(b"CONFIG") {
            self.send_value_directly(seq, ResponseValue::SimpleString("".into()));
            return None;
        }

        let key = match args.first() {
            Some(ResponseValue::BulkString(Some(bytes))) => bytes,
            _ => {
                self.send_value_directly(
                    seq,
                    ResponseValue::Error("error while parsing key".into()),
                );
                return None;
            }
        };

        Some(key.clone())
    }

    fn send_value_directly(&self, seq: u64, value: ResponseValue) {
        let mut worker_queues = self.worker_queues.borrow_mut();
        let worker_queue = match worker_queues.get_mut(self.src_core) {
            Some(queue) => queue,
            None => panic!(),
        };

        worker_queue.push(WorkerMessage {
            seq,
            conn_token: self.conn_token,
            src_core: self.src_core,
            response_value: value,
        });
    }
}

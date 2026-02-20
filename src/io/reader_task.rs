use std::{
    cell::RefCell,
    hash::{DefaultHasher, Hash, Hasher},
    rc::Rc,
};

use bytes::BytesMut;
use tokio::{io::AsyncReadExt, net::tcp::OwnedReadHalf};

use crate::{
    core::{reply_dispatcher::ReplyDispatcher, shard_executor::ShardExecutor},
    io::spawn_io::WorkerQueues,
    metrics::ROUTING_METRICS,
    message::{RespFrame, WorkerMessage},
    parser::{parse, BufParseError},
};

pub struct ReaderTask {
    read_half: OwnedReadHalf,
    read_buffer: BytesMut,
    core: CoreHandles,
    conn_token: usize,
    seq: u64,
}

struct CoreHandles {
    worker_queues: WorkerQueues,
    core_id: usize,
    shard_executor: Rc<RefCell<ShardExecutor>>,
    reply_dispatcher: Rc<RefCell<ReplyDispatcher>>,
}

impl ReaderTask {
    pub fn new(
        read_half: OwnedReadHalf,
        worker_queues: WorkerQueues,
        conn_token: usize,
        core_id: usize,
        shard_executor: Rc<RefCell<ShardExecutor>>,
        reply_dispatcher: Rc<RefCell<ReplyDispatcher>>,
    ) -> Self {
        Self {
            read_half,
            read_buffer: BytesMut::with_capacity(64 * 1024),
            core: CoreHandles {
                worker_queues,
                core_id,
                shard_executor,
                reply_dispatcher,
            },
            conn_token,
            seq: 0,
        }
    }

    pub async fn run(&mut self) -> tokio::io::Result<()> {
        loop {
            self.read_buffer.reserve(1024);
            if self.read_half.read_buf(&mut self.read_buffer).await? == 0 {
                break;
            }

            if !self.parse_and_route_frame() {
                return Ok(());
            }
        }

        Ok(())
    }

    pub fn parse_and_route_frame(&mut self) -> bool {
        loop {
            match parse(&mut self.read_buffer) {
                Ok(value) => {
                    #[cfg(debug_assertions)]
                    println!("parsed: {:?}", value);

                    self.route_message(value, self.seq);
                    self.seq += 1;
                }
                Err(BufParseError::Incomplete) => {
                    break;
                }
                Err(BufParseError::InvalidFirstByte(b)) => {
                    match b {
                        Some(byte) => {
                            let s = format!("ERR invalid first byte: {}", byte);
                            self.route_message(RespFrame::Error(s.into()), self.seq);
                        }
                        None => {
                            self.route_message(
                                RespFrame::Error("ERR first byte not found".into()),
                                self.seq,
                            );
                        }
                    };
                    return false;
                }
                _ => {
                    self.route_message(
                        RespFrame::Error("ERR internal server error".into()),
                        self.seq,
                    );
                    return false;
                }
            }
        }

        true
    }

    fn route_message(&self, frame: RespFrame, seq: u64) {
        let items = match &frame {
            RespFrame::Array(Some(items)) => items,
            other => {
                self.send_value_directly(seq, other.to_owned());
                return;
            }
        };

        if items.is_empty() {
            self.send_value_directly(seq, RespFrame::Error("request is empty".into()));
            return;
        }

        let key = match self.extract_key(seq, items) {
            Some(key) => key,
            None => return,
        };

        let mut worker_queues = self.core.worker_queues.borrow_mut();
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let to_worker = hasher.finish() as usize % worker_queues.len();

        if to_worker == self.core.core_id {
            ROUTING_METRICS.record_keyed_local();
            let response = self.core.shard_executor.borrow_mut().process_message(frame);
            self.core.reply_dispatcher.borrow_mut().send_to_io(
                self.core.core_id,
                self.conn_token,
                seq,
                response,
            );
            return;
        }

        let destination_worker_queue = match worker_queues.get_mut(to_worker) {
            Some(queue) => queue,
            None => {
                self.send_value_directly(
                    seq,
                    RespFrame::Error("internal server error, invalid worker index".into()),
                );
                return;
            }
        };

        #[cfg(debug_assertions)]
        println!(
            "sending message to worker {} with frame: {:?}",
            to_worker, frame
        );

        ROUTING_METRICS.record_keyed_remote();
        destination_worker_queue
            .push_with_notify(WorkerMessage {
                seq,
                conn_token: self.conn_token,
                src_core: self.core.core_id,
                response_value: frame,
            })
            .unwrap_or_else(|err| {
                eprintln!(
                    "router push failed (conn {}, seq {}): {:?}",
                    self.conn_token, seq, err
                );
            });
    }

    fn extract_key(&self, seq: u64, items: &[RespFrame]) -> Option<bytes::Bytes> {
        let (cmd, args) = match items.split_first() {
            Some((RespFrame::BulkString(Some(bytes)), rest)) => (bytes, rest),
            _ => {
                self.send_value_directly(
                    seq,
                    RespFrame::Error("command must be bulk string".into()),
                );
                return None;
            }
        };

        if cmd.eq_ignore_ascii_case(b"PING") {
            self.send_value_directly(seq, RespFrame::SimpleString("PONG".into()));
            return None;
        }

        if cmd.eq_ignore_ascii_case(b"CONFIG") {
            self.send_value_directly(seq, RespFrame::SimpleString("".into()));
            return None;
        }

        let key = match args.first() {
            Some(RespFrame::BulkString(Some(bytes))) => bytes,
            _ => {
                self.send_value_directly(seq, RespFrame::Error("error while parsing key".into()));
                return None;
            }
        };

        Some(key.clone())
    }

    fn send_value_directly(&self, seq: u64, value: RespFrame) {
        ROUTING_METRICS.record_direct();
        let mut worker_queues = self.core.worker_queues.borrow_mut();
        let worker_queue = match worker_queues.get_mut(self.core.core_id) {
            Some(queue) => queue,
            None => panic!(),
        };

        worker_queue
            .push_with_notify(WorkerMessage {
                seq,
                conn_token: self.conn_token,
                src_core: self.core.core_id,
                response_value: value,
            })
            .unwrap_or_else(|err| {
                eprintln!(
                    "router direct push failed (conn {}, seq {}): {:?}",
                    self.conn_token, seq, err
                );
            });
    }
}

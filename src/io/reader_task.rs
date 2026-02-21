use std::{
    cell::RefCell,
    hash::{DefaultHasher, Hash, Hasher},
    rc::Rc,
};

use bytes::BytesMut;
use tokio::{io::AsyncReadExt, net::tcp::OwnedReadHalf};

use crate::{
    config::{BATCH_SIZE, MAX_BATCH_FLUSHES_PER_TICK},
    core::{reply_dispatcher::ReplyDispatcher, shard_executor::ShardExecutor},
    io::spawn_io::WorkerQueues,
    message::{RespFrame, WorkerMessage},
    metrics::ROUTING_METRICS,
    parser::{parse, BufParseError},
};

pub struct ReaderTask {
    read_half: OwnedReadHalf,
    read_buffer: BytesMut,
    core: CoreHandles,
    conn_token: usize,
    seq: u64,
    pending_by_dst: Vec<Vec<WorkerMessage>>,
    next_flush_dst: usize,
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
        let worker_count = worker_queues.borrow().len();
        let mut pending_by_dst = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            pending_by_dst.push(Vec::new());
        }

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
            pending_by_dst,
            next_flush_dst: 0,
        }
    }

    pub async fn run(&mut self) -> tokio::io::Result<()> {
        loop {
            self.read_buffer.reserve(1024);
            if self.read_half.read_buf(&mut self.read_buffer).await? == 0 {
                self.flush_pending_requests(true);
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

        self.flush_pending_requests(true);
        true
    }

    fn route_message(&mut self, frame: RespFrame, seq: u64) {
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

        let worker_count = self.core.worker_queues.borrow().len();
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let to_worker = hasher.finish() as usize % worker_count;

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

        #[cfg(debug_assertions)]
        println!(
            "sending message to worker {} with frame: {:?}",
            to_worker, frame
        );

        ROUTING_METRICS.record_keyed_remote();
        let msg = WorkerMessage {
            seq,
            conn_token: self.conn_token,
            src_core: self.core.core_id,
            response_value: frame,
        };
        self.enqueue_remote_message(to_worker, msg);
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

    fn enqueue_remote_message(&mut self, dst_core: usize, msg: WorkerMessage) {
        if let Some(bucket) = self.pending_by_dst.get_mut(dst_core) {
            bucket.push(msg);
        } else {
            eprintln!(
                "router enqueue failed (conn {}, seq {}): invalid worker index {}",
                self.conn_token, msg.seq, dst_core
            );
            return;
        }

        if self.pending_by_dst[dst_core].len() >= BATCH_SIZE {
            self.flush_pending_requests(false);
        }
    }

    fn flush_pending_requests(&mut self, force_all: bool) {
        let total_dsts = self.pending_by_dst.len();
        if total_dsts == 0 {
            return;
        }

        let mut worker_queues = self.core.worker_queues.borrow_mut();
        let max_flushes = if force_all {
            total_dsts
        } else {
            MAX_BATCH_FLUSHES_PER_TICK.min(total_dsts)
        };

        let mut flushed_dsts = 0usize;
        let mut visited = 0usize;
        while flushed_dsts < max_flushes && visited < total_dsts {
            let idx = (self.next_flush_dst + visited) % total_dsts;
            visited += 1;

            if self.pending_by_dst[idx].is_empty() {
                continue;
            }

            if !force_all && self.pending_by_dst[idx].len() < BATCH_SIZE {
                continue;
            }

            let queue = match worker_queues.get_mut(idx) {
                Some(queue) => queue,
                None => continue,
            };

            let to_send = std::mem::take(&mut self.pending_by_dst[idx]);
            let unsent = queue.push_batch_with_notify(to_send);

            if !unsent.is_empty() {
                self.pending_by_dst[idx] = unsent;
            }

            flushed_dsts += 1;
        }

        self.next_flush_dst = (self.next_flush_dst + visited) % total_dsts;
    }
}

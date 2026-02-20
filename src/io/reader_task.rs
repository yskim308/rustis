use std::{cell::RefCell, rc::Rc};

use bytes::BytesMut;
use tokio::{io::AsyncReadExt, net::tcp::OwnedReadHalf};

use crate::{
    core::{reply_dispatcher::ReplyDispatcher, shard_executor::ShardExecutor},
    io::spawn_io::WorkerQueues,
    message::RespFrame,
    parser::{parse, BufParseError},
    router::MessageRouter,
};

pub struct ReaderTask {
    read_half: OwnedReadHalf,
    read_buffer: BytesMut,
    router: MessageRouter,
    seq: u64,
}

impl ReaderTask {
    pub fn new(
        read_half: OwnedReadHalf,
        worker_queues: WorkerQueues,
        conn_token: usize,
        core_id: usize,
        shard_executor: Rc<RefCell<ShardExecutor>>,
        _reply_dispatcher: Rc<RefCell<ReplyDispatcher>>,
    ) -> Self {
        let router = MessageRouter::new(worker_queues, conn_token, core_id, shard_executor);
        Self {
            read_half,
            read_buffer: BytesMut::with_capacity(64 * 1024),
            router,
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

                    self.router.route_message(value, self.seq);
                    self.seq += 1;
                }
                Err(BufParseError::Incomplete) => {
                    break;
                }
                Err(BufParseError::InvalidFirstByte(b)) => {
                    match b {
                        Some(byte) => {
                            let s = format!("ERR invalid first byte: {}", byte);
                            self.router
                                .route_message(RespFrame::Error(s.into()), self.seq);
                        }
                        None => self.router.route_message(
                            RespFrame::Error("ERR first byte not found".into()),
                            self.seq,
                        ),
                    };
                    return false;
                }
                _ => {
                    self.router.route_message(
                        RespFrame::Error("ERR internal server error".into()),
                        self.seq,
                    );
                    return false;
                }
            }
        }

        true
    }
}

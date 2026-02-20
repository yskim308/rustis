use crate::{
    message::{RespFrame, ResponseMessage},
    polling::notified_ring_buffer::NotifiedProducer,
};

pub struct ReplyDispatcher {
    to_writer: Vec<NotifiedProducer<ResponseMessage>>,
}

impl ReplyDispatcher {
    pub fn new(to_writer: Vec<NotifiedProducer<ResponseMessage>>) -> Self {
        ReplyDispatcher { to_writer }
    }

    pub fn send_to_io(
        &mut self,
        src_core: usize,
        conn_token: usize,
        seq: u64,
        response_value: RespFrame,
    ) {
        self.to_writer[src_core]
            .push_with_notify(ResponseMessage {
                seq,
                conn_token,
                response_value,
            })
            .unwrap_or_else(|err| {
                eprintln!(
                    "worker response push failed (conn {}, seq {}): {:?}",
                    conn_token, seq, err
                );
            });
    }
}

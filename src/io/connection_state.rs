use std::{cell::RefCell, rc::Rc};

use bytes::BytesMut;
use slab::Slab;
use tokio::net::tcp::OwnedWriteHalf;

use crate::message::RespFrame;

pub struct ConnectionState {
    pub writer: OwnedWriteHalf,
    pub pending_responses: Vec<Option<RespFrame>>,
    pub next_seq: u64,
    pub write_buffer: BytesMut,
}

const WINDOW_SIZE: usize = 1024;

impl ConnectionState {
    pub fn new(writer: OwnedWriteHalf) -> Self {
        let mut buffer = Vec::with_capacity(WINDOW_SIZE);
        for _ in 0..WINDOW_SIZE {
            buffer.push(None);
        }

        Self {
            writer,
            pending_responses: buffer,
            next_seq: 0,
            write_buffer: BytesMut::with_capacity(32 * 1024),
        }
    }

    pub fn enqueue_response(&mut self, seq: u64, value: RespFrame) {
        let index = seq as usize & (WINDOW_SIZE - 1);

        // try to insert
        if self.pending_responses[index].is_some() {
            panic!("Pipeline depth exceeded in connection buffer!");
        }
        self.pending_responses[index] = Some(value);

        // loop through and write to write buffer
        loop {
            let to_drain = self.next_seq as usize & (WINDOW_SIZE - 1);

            match self.pending_responses[to_drain].take() {
                Some(value) => {
                    value.serialize(&mut self.write_buffer);
                    self.next_seq += 1;
                }
                None => break,
            }
        }
    }
}

pub type ConnectionStore = Rc<RefCell<Slab<ConnectionState>>>;

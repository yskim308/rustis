use std::{
    sync::{atomic::Ordering, Arc},
    task::Poll,
};

use bytes::Buf;
use rtrb::Consumer;

use crate::{
    io::connection_state::{ConnectionState, ConnectionStore},
    message::ResponseMessage,
    polling::task_notifier::TaskNotifier,
};

pub struct IOInboxPoller {
    connections: ConnectionStore,
    inboxes: Vec<Consumer<ResponseMessage>>,
    doorbell: Arc<TaskNotifier>,
}

impl Future for IOInboxPoller {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let mut did_work = false;

        for i in 0..self.inboxes.len() {
            let mut quota = 32;

            while quota > 0 {
                match self.inboxes[i].pop() {
                    Ok(msg) => {
                        did_work = true;
                        Self::handle_message(msg, &self.connections);
                    }
                    Err(_) => break,
                }
                quota -= 1;
            }
        }

        if did_work {
            // Keep polling to flush any buffered writes.
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }

        // No new messages, but we may still have buffered writes to flush.
        let mut needs_flush = false;
        {
            let mut connections = self.connections.borrow_mut();
            for (_, conn_state) in connections.iter_mut() {
                if !conn_state.write_buffer.is_empty() {
                    needs_flush = true;
                    Self::write_to_buffer(conn_state);
                }
            }
        }
        if needs_flush {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }

        self.doorbell.waker.register(cx.waker());
        self.doorbell.is_sleeping.store(true, Ordering::Release);

        for consumer in &self.inboxes {
            if !consumer.is_empty() {
                self.doorbell.is_sleeping.store(false, Ordering::Release);
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        }

        Poll::Pending
    }
}

impl IOInboxPoller {
    pub fn new(
        connections: ConnectionStore,
        inboxes: Vec<Consumer<ResponseMessage>>,
        doorbell: Arc<TaskNotifier>,
    ) -> Self {
        IOInboxPoller {
            connections,
            inboxes,
            doorbell,
        }
    }

    fn handle_message(msg: ResponseMessage, connections: &ConnectionStore) {
        let mut connections = connections.borrow_mut();
        // 1. Lookup the connection by Token
        #[cfg(debug_assertions)]
        println!("handling message from IO Poller: {:?}", msg);

        if let Some(conn_state) = connections.get_mut(msg.conn_token) {
            conn_state.enqueue_response(msg.seq, msg.response_value);
            if !conn_state.write_buffer.is_empty() {
                Self::write_to_buffer(conn_state);
            }
        }
    }

    fn write_to_buffer(conn_state: &mut ConnectionState) {
        while !conn_state.write_buffer.is_empty() {
            match conn_state.writer.try_write(&conn_state.write_buffer) {
                Ok(0) => break,
                Ok(n) => {
                    #[cfg(debug_assertions)]
                    println!("write success");

                    conn_state.write_buffer.advance(n);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    eprintln!("Connection died: {}", e);
                    break;
                }
            }
        }
    }
}

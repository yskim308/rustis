use std::{
    sync::{atomic::Ordering, Arc},
    task::Poll,
};

use rtrb::Consumer;

use crate::{
    handler::process_command,
    kv::KvStore,
    message::{RespFrame, ResponseMessage, WorkerMessage},
    polling::{notified_ring_buffer::NotifiedProducer, task_notifier::TaskNotifier},
};

pub struct WorkerTask {
    _worker_id: usize,
    inboxes: Vec<Consumer<WorkerMessage>>,
    to_writer: Vec<NotifiedProducer<ResponseMessage>>,
    doorbell: Arc<TaskNotifier>,
    kv: KvStore,
}

impl Future for WorkerTask {
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
                        self.process_message(msg);
                        did_work = true;
                    }
                    Err(_) => break,
                }
                quota -= 1;
            }
        }

        // if work is processed, we're hot, yield but do NOT sleep (yield to runtime)
        if did_work {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }

        // go to sleep, only wake by manual wakeup
        self.doorbell.is_sleeping.store(true, Ordering::Release);
        self.doorbell.waker.register(cx.waker());

        // final check, if anything in inbox, cancel sleep and yield to runtime
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

impl WorkerTask {
    pub fn new(
        worker_id: usize,
        inboxes: Vec<Consumer<WorkerMessage>>,
        to_writer: Vec<NotifiedProducer<ResponseMessage>>,
        doorbell: Arc<TaskNotifier>,
    ) -> Self {
        let kv = KvStore::new();
        WorkerTask {
            _worker_id: worker_id,
            inboxes,
            to_writer,
            doorbell,
            kv,
        }
    }

    fn process_message(&mut self, msg: WorkerMessage) {
        let response = match msg.response_value {
            RespFrame::Array(_) => process_command(&self.kv, msg.response_value),
            _ => msg.response_value,
        };
        // note: later, we should be using .get() and handling errors properly
        self.to_writer[msg.src_core]
            .push_with_notify(ResponseMessage {
                seq: msg.seq,
                conn_token: msg.conn_token,
                response_value: response,
            })
            .unwrap();
    }
}

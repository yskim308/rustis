use std::{
    cell::RefCell,
    rc::Rc,
    sync::{atomic::Ordering, Arc},
    task::Poll,
};

use rtrb::Consumer;

use crate::{
    core::{reply_dispatcher::ReplyDispatcher, shard_executor::ShardExecutor},
    message::ResponseMessage,
    message::WorkerMessage,
    polling::task_notifier::TaskNotifier,
};

const RESPONSE_BATCH_SIZE: usize = 64;
const MAX_RESPONSE_BATCH_FLUSHES_PER_TICK: usize = 16;

pub struct WorkerTask {
    _worker_id: usize,
    inboxes: Vec<Consumer<WorkerMessage>>,
    doorbell: Arc<TaskNotifier>,
    shard_executor: Rc<RefCell<ShardExecutor>>,
    reply_dispatcher: Rc<RefCell<ReplyDispatcher>>,
    pending_responses_by_io: Vec<Vec<ResponseMessage>>,
    next_flush_io_dst: usize,
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

        self.flush_pending_responses(false);

        // if work is processed, we're hot, yield but do NOT sleep (yield to runtime)
        if did_work {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }

        self.flush_pending_responses(true);

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
        doorbell: Arc<TaskNotifier>,
        shard_executor: Rc<RefCell<ShardExecutor>>,
        reply_dispatcher: Rc<RefCell<ReplyDispatcher>>,
    ) -> Self {
        let destination_count = reply_dispatcher.borrow().destination_count();
        let mut pending_responses_by_io = Vec::with_capacity(destination_count);
        for _ in 0..destination_count {
            pending_responses_by_io.push(Vec::new());
        }

        WorkerTask {
            _worker_id: worker_id,
            inboxes,
            doorbell,
            shard_executor,
            reply_dispatcher,
            pending_responses_by_io,
            next_flush_io_dst: 0,
        }
    }

    fn process_message(&mut self, msg: WorkerMessage) {
        #[cfg(debug_assertions)]
        println!("msg: {:?} processed", msg);
        let response = self
            .shard_executor
            .borrow_mut()
            .process_message(msg.response_value);
        #[cfg(debug_assertions)]
        println!("sending response: {:?} to core {}", response, msg.src_core);
        self.enqueue_response(
            msg.src_core,
            ResponseMessage {
                seq: msg.seq,
                conn_token: msg.conn_token,
                response_value: response,
            },
        );
    }

    fn enqueue_response(&mut self, dst_io_core: usize, msg: ResponseMessage) {
        if let Some(bucket) = self.pending_responses_by_io.get_mut(dst_io_core) {
            bucket.push(msg);
        } else {
            eprintln!(
                "worker response enqueue failed: invalid io core {}",
                dst_io_core
            );
            return;
        }

        if self.pending_responses_by_io[dst_io_core].len() >= RESPONSE_BATCH_SIZE {
            self.flush_pending_responses(false);
        }
    }

    fn flush_pending_responses(&mut self, force_all: bool) {
        let total_dsts = self.pending_responses_by_io.len();
        if total_dsts == 0 {
            return;
        }

        let max_flushes = if force_all {
            total_dsts
        } else {
            MAX_RESPONSE_BATCH_FLUSHES_PER_TICK.min(total_dsts)
        };

        let mut flushed_dsts = 0usize;
        let mut visited = 0usize;
        while flushed_dsts < max_flushes && visited < total_dsts {
            let idx = (self.next_flush_io_dst + visited) % total_dsts;
            visited += 1;

            if self.pending_responses_by_io[idx].is_empty() {
                continue;
            }

            if !force_all && self.pending_responses_by_io[idx].len() < RESPONSE_BATCH_SIZE {
                continue;
            }

            let to_send = std::mem::take(&mut self.pending_responses_by_io[idx]);
            let unsent = self
                .reply_dispatcher
                .borrow_mut()
                .send_batch_to_io(idx, to_send);
            if !unsent.is_empty() {
                self.pending_responses_by_io[idx] = unsent;
            }

            flushed_dsts += 1;
        }

        self.next_flush_io_dst = (self.next_flush_io_dst + visited) % total_dsts;
    }
}

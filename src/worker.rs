use std::{
    cell::RefCell,
    rc::Rc,
    sync::{atomic::Ordering, Arc},
    task::Poll,
};

use rtrb::Consumer;

use crate::{
    core::{reply_dispatcher::ReplyDispatcher, shard_executor::ShardExecutor},
    message::WorkerMessage,
    polling::task_notifier::TaskNotifier,
};

pub struct WorkerTask {
    _worker_id: usize,
    inboxes: Vec<Consumer<WorkerMessage>>,
    doorbell: Arc<TaskNotifier>,
    shard_executor: Rc<RefCell<ShardExecutor>>,
    reply_dispatcher: Rc<RefCell<ReplyDispatcher>>,
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
        doorbell: Arc<TaskNotifier>,
        shard_executor: Rc<RefCell<ShardExecutor>>,
        reply_dispatcher: Rc<RefCell<ReplyDispatcher>>,
    ) -> Self {
        WorkerTask {
            _worker_id: worker_id,
            inboxes,
            doorbell,
            shard_executor,
            reply_dispatcher,
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
        // note: later, we should be using .get() and handling errors properly

        self.reply_dispatcher.borrow_mut().send_to_io(
            msg.src_core,
            msg.conn_token,
            msg.seq,
            response,
        );
    }
}

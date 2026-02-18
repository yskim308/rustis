use std::sync::Arc;

use rtrb::{Producer, PushError};

use crate::polling::task_notifier::TaskNotifier;

#[derive(Debug)]
pub struct NotifiedProducer<T> {
    producer: Producer<T>,
    consumer_notifier: Arc<TaskNotifier>,
}

impl<T> NotifiedProducer<T> {
    pub fn new(producer: Producer<T>, consumer_notifier: Arc<TaskNotifier>) -> Self {
        NotifiedProducer {
            producer,
            consumer_notifier,
        }
    }

    pub fn push_with_notify(&mut self, item: T) -> Result<(), PushError<T>> {
        self.producer.push(item)?;

        self.consumer_notifier.wake_if_sleeping();

        Ok(())
    }
}

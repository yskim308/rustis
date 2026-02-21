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

    pub fn push_batch_with_notify(&mut self, items: Vec<T>) -> Vec<T> {
        let mut pushed_any = false;
        let mut iter = items.into_iter();

        while let Some(item) = iter.next() {
            match self.producer.push(item) {
                Ok(()) => {
                    pushed_any = true;
                }
                Err(PushError::Full(item)) => {
                    let mut unsent = Vec::new();
                    unsent.push(item);
                    unsent.extend(iter);
                    if pushed_any {
                        self.consumer_notifier.wake_if_sleeping();
                    }
                    return unsent;
                }
            }
        }

        if pushed_any {
            self.consumer_notifier.wake_if_sleeping();
        }

        Vec::new()
    }
}

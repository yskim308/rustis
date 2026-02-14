use std::sync::Arc;

use rtrb::{Consumer, Producer, PushError};

use crate::polling::{config::CooldownConfig, sleep_state::SleepState};

#[derive(Debug)]
pub struct NotifiedConsumer<T> {
    consumer: Consumer<T>,
    sleep_state: Arc<SleepState>,
    config: CooldownConfig,
    idle_cycles: u32,
}

#[derive(Debug)]
pub struct NotifiedProducer<T> {
    producer: Producer<T>,
    sleep_state: Arc<SleepState>,
}

impl<T> NotifiedProducer<T> {
    pub fn push_with_notify(&mut self, item: T) -> Result<(), PushError<T>> {
        // Check if we should wake up the consumer
        self.sleep_state.notify_if_asleep();

        self.producer.push(item)?;

        Ok(())
    }
}

impl<T> NotifiedConsumer<T> {
    pub async fn poll_loop(&mut self) {
        loop {
            let mut has_work = false;

            // Drain available items
            while let Ok(item) = self.consumer.pop() {
                has_work = true;
                self.sleep_state.notify_if_asleep();

                // Process item here (delegate to caller)
                todo!("figure out how to delegate to caller");
            }

            if has_work {
                // Reset idle counter when we have work
                self.idle_cycles = 0;
            } else {
                // Increment idle counter
                self.idle_cycles += 1;

                // Check if should enter sleep
                if self.idle_cycles >= self.config.max_idle_cycles {
                    self.sleep_state.enter_sleep().await;
                    self.idle_cycles = 0;
                } else {
                    // Brief yield before next poll
                    tokio::task::yield_now().await;
                }
            }
        }
    }
}

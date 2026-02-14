use std::sync::Arc;

use rtrb::{
    chunks::{ChunkError, ReadChunk},
    Consumer, Producer, PushError,
};

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
    pub fn read_chunk(&mut self, max_batch: usize) -> Result<ReadChunk<'_, T>, ChunkError> {
        match self.consumer.read_chunk(max_batch) {
            Ok(chunk) => Ok(chunk),
            Err(e) => Err(e),
        }
    }

    pub fn notify_producer(&mut self) {
        self.sleep_state.notify_if_asleep();
    }
}

use crate::{handler::process_command, kv::KvStore, message::RespFrame};

pub struct ShardExecutor {
    kv: KvStore,
}

impl ShardExecutor {
    pub fn new() -> Self {
        ShardExecutor { kv: KvStore::new() }
    }
    pub fn process_message(&mut self, frame: RespFrame) -> RespFrame {
        match frame {
            RespFrame::Array(_) => process_command(&self.kv, frame),
            _ => frame,
        }
    }
}

impl Default for ShardExecutor {
    fn default() -> Self {
        Self::new()
    }
}

use crate::{handler::process_command, kv::KvStore, message::RespFrame};

#[derive(Default)]
pub struct ShardExecutor {
    kv: KvStore,
}

impl ShardExecutor {
    pub fn process_message(&mut self, frame: RespFrame) -> RespFrame {
        match frame {
            RespFrame::Array(_) => process_command(&self.kv, frame),
            _ => frame,
        }
    }
}

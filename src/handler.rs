use bytes::Bytes;

use crate::kv::KvStore;
use crate::parser::ReponseValue;
use std::sync::Arc;

pub struct CommandHandler {
    kv: Arc<KvStore>,
}

impl CommandHandler {
    pub fn new(kv: Arc<KvStore>) -> Self {
        CommandHandler { kv }
    }

    pub fn process_command(&self, value: ReponseValue) -> ReponseValue {
        let items = match value {
            ReponseValue::Array(Some(items)) => items,
            _ => return ReponseValue::Error("request must be array".to_string()),
        };

        if items.is_empty() {
            return ReponseValue::Error("empty request".to_string());
        }

        let mut it = items.into_iter();
        let command_part = it.next();

        let command = match command_part {
            Some(ReponseValue::BulkString(Some(bytes))) => {
                String::from_utf8_lossy(&bytes).to_uppercase()
            }
            _ => return ReponseValue::Error("command must be bulk string".to_string()),
        };

        match command.as_str() {
            "PING" => ReponseValue::SimpleString("PONG".to_string()),
            "GET" => {
                let key_part = it.next();
                match key_part {
                    Some(ReponseValue::BulkString(Some(bytes))) => {
                        self.handle_get(&String::from_utf8_lossy(&bytes))
                    }
                    _ => ReponseValue::Error("error while processing key".to_string()),
                }
            }
            "SET" => {
                let key_part = it.next();
                let key = match key_part {
                    Some(ReponseValue::BulkString(Some(bytes))) => {
                        String::from_utf8_lossy(&bytes).into_owned()
                    }
                    _ => return ReponseValue::Error("error while processing key".to_string()),
                };

                let value_part = it.next();
                match value_part {
                    Some(ReponseValue::BulkString(Some(bytes))) => {
                        self.handle_set(key, Bytes::from(bytes))
                    }
                    _ => ReponseValue::Error("error while processing key".to_string()),
                }
            }
            _ => ReponseValue::Error("invalid command".to_string()),
        }
    }

    fn handle_get(&self, key: &str) -> ReponseValue {
        match self.kv.get(key) {
            Ok(Some(bytes)) => ReponseValue::BulkString(Some(bytes.to_vec())),
            Ok(None) => ReponseValue::BulkString(None),
            Err(_) => ReponseValue::Error("internal server error (poisoned lock)".to_string()),
        }
    }

    fn handle_set(&self, key: String, value: Bytes) -> ReponseValue {
        match self.kv.set(key, value) {
            Ok(()) => ReponseValue::BulkString(Some(b"OK".to_vec())),
            Err(_) => ReponseValue::Error("internal server error (poisoned lock)".to_string()),
        }
    }
}

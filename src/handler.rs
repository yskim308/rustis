use bytes::Bytes;
use std::sync::Arc;
use tokio::task::coop::RestoreOnPending;

use crate::kv::{KvStore, RedisValue};
use crate::parser::ReponseValue;

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

        let (cmd, args) = match items.split_first() {
            Some((ReponseValue::BulkString(Some(bytes)), rest)) => {
                (String::from_utf8_lossy(bytes).to_uppercase(), rest)
            }
            _ => return ReponseValue::Error("command must be bulk string".to_string()),
        };

        match cmd.as_str() {
            "PING" => ReponseValue::SimpleString("PONG".to_string()),
            "GET" => self.handle_get(args),
            "SET" => self.handle_set(args),
            "LPUSH" => self.handle_lpush(args),
            "RPUSH" => self.handle_rpush(args),
            _ => ReponseValue::Error("invalid command".to_string()),
        }
    }

    fn handle_get(&self, args: &[ReponseValue]) -> ReponseValue {
        if args.len() != 1 {
            return ReponseValue::Error(
                "ERR wrong number of arguments for 'get' command".to_string(),
            );
        }

        let key = match &args[0] {
            ReponseValue::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
            _ => return ReponseValue::Error("ERR key must be bulk string".to_string()),
        };

        match self.kv.get(&key) {
            Ok(Some(RedisValue::String(b))) => ReponseValue::BulkString(Some(b.to_vec())),
            Ok(Some(_)) => ReponseValue::Error(
                "WRONGTYPE Operation against a key holding the wrong kind of value".to_string(),
            ),
            Ok(None) => ReponseValue::BulkString(None),
            Err(_) => ReponseValue::Error("internal server error".to_string()),
        }
    }

    fn handle_set(&self, args: &[ReponseValue]) -> ReponseValue {
        if args.len() != 2 {
            return ReponseValue::Error(
                "ERR wrong number of arguments for 'set' command".to_string(),
            );
        }

        let key = match &args[0] {
            ReponseValue::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
            _ => return ReponseValue::Error("ERR key must be bulk string".to_string()),
        };

        let value = match &args[1] {
            ReponseValue::BulkString(Some(bytes)) => Bytes::from(bytes.clone()),
            _ => return ReponseValue::Error("ERR value must be bulk string".to_string()),
        };

        match self.kv.set(key, value) {
            Ok(()) => ReponseValue::SimpleString("OK".to_string()),
            Err(_) => ReponseValue::Error("internal server error (poisoned lock)".to_string()),
        }
    }

    fn handle_lpush(&self, args: &[ReponseValue]) -> ReponseValue {
        let key = match &args[0] {
            ReponseValue::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
            _ => return ReponseValue::Error("key must be bulk string".to_string()),
        };

        let mut values = Vec::with_capacity(args.len() - 1);
        for arg in &args[1..] {
            if let ReponseValue::BulkString(Some(bytes)) = arg {
                let to_push = Bytes::from(bytes.clone());
                values.push(to_push);
            } else {
                return ReponseValue::Error("pushed values must be bulk strings".to_string());
            }
        }

        match self.kv.lpush(key.to_string(), values) {
            Ok(size) => ReponseValue::Integer(size),
            Err(err) => ReponseValue::Error(format!("internal db error: {:?}", err)),
        }
    }

    fn handle_rpush(&self, args: &[ReponseValue]) -> ReponseValue {
        let key = match &args[0] {
            ReponseValue::BulkString(Some(bytes)) => String::from_utf8_lossy(bytes),
            _ => return ReponseValue::Error("key must be bulk string".to_string()),
        };

        let mut values = Vec::with_capacity(args.len() - 1);
        for arg in &args[1..] {
            if let ReponseValue::BulkString(Some(bytes)) = arg {
                let to_push = Bytes::from(bytes.clone());
                values.push(to_push);
            } else {
                return ReponseValue::Error("pushed values must be bulk strings".to_string());
            }
        }

        match self.kv.rpush(key.to_string(), values) {
            Ok(size) => ReponseValue::Integer(size),
            Err(err) => ReponseValue::Error(format!("internal db error: {:?}", err)),
        }
    }
}

use bytes::Bytes;
use std::sync::Arc;

use crate::kv::{KvStore, RedisValue};
use crate::parser::ResponseValue;

pub struct CommandHandler {
    kv: Arc<KvStore>,
}

fn parse_int(value: &ResponseValue) -> Result<i64, String> {
    match value {
        ResponseValue::BulkString(Some(bytes)) => {
            let s = std::str::from_utf8(bytes)
                .map_err(|_| "ERR value is not valid utf8".to_string())?;
            s.parse::<i64>()
                .map_err(|_| "ERR value is not an integer or out of range".to_string())
        }
        _ => Err("ERR protocol error: expected bulk string".to_string()),
    }
}

impl CommandHandler {
    pub fn new(kv: Arc<KvStore>) -> Self {
        CommandHandler { kv }
    }

    pub fn process_command(&self, value: ResponseValue) -> ResponseValue {
        let items = match value {
            ResponseValue::Array(Some(items)) => items,
            _ => return ResponseValue::Error("request must be array".to_string()),
        };

        if items.is_empty() {
            return ResponseValue::Error("empty request".to_string());
        }

        let (cmd, args) = match items.split_first() {
            Some((ResponseValue::BulkString(Some(bytes)), rest)) => {
                (String::from_utf8_lossy(bytes).to_uppercase(), rest)
            }
            _ => return ResponseValue::Error("command must be bulk string".to_string()),
        };

        match cmd.as_str() {
            "PING" => ResponseValue::SimpleString("PONG".to_string()),
            "GET" => self.handle_get(args),
            "SET" => self.handle_set(args),
            "LPUSH" => self.handle_lpush(args),
            "LPOP" => self.handle_lpop(args),
            "RPUSH" => self.handle_rpush(args),
            "RPOP" => self.handle_rpop(args),
            "LRANGE" => self.handle_lrange(args),
            _ => ResponseValue::Error("invalid command".to_string()),
        }
    }

    fn handle_get(&self, args: &[ResponseValue]) -> ResponseValue {
        if args.len() != 1 {
            return ResponseValue::Error(
                "ERR wrong number of arguments for 'get' command".to_string(),
            );
        }

        let key = match args.first() {
            Some(ResponseValue::BulkString(Some(bytes))) => String::from_utf8_lossy(bytes),
            Some(_) => return ResponseValue::Error("ERR key must be bulk string".to_string()),
            None => return ResponseValue::Error("ERR invalid number of arguments".to_string()),
        };

        match self.kv.get(&key) {
            Ok(Some(RedisValue::String(b))) => ResponseValue::BulkString(Some(b.to_vec())),
            Ok(Some(_)) => ResponseValue::Error(
                "WRONGTYPE Operation against a key holding the wrong kind of value".to_string(),
            ),
            Ok(None) => ResponseValue::BulkString(None),
            Err(_) => ResponseValue::Error("internal server error".to_string()),
        }
    }

    fn handle_set(&self, args: &[ResponseValue]) -> ResponseValue {
        if args.len() != 2 {
            return ResponseValue::Error(
                "ERR wrong number of arguments for 'set' command".to_string(),
            );
        }

        let key = match args.first() {
            Some(ResponseValue::BulkString(Some(bytes))) => {
                String::from_utf8_lossy(bytes).into_owned()
            }
            Some(_) => return ResponseValue::Error("ERR key must be bulk string".to_string()),
            None => return ResponseValue::Error("ERR invalid number of arguments".to_string()),
        };

        let value = match args.get(1) {
            Some(ResponseValue::BulkString(Some(bytes))) => Bytes::from(bytes.clone()),
            Some(_) => return ResponseValue::Error("ERR value must be bulk string".to_string()),
            None => return ResponseValue::Error("ERR invalid number of arguments".to_string()),
        };

        match self.kv.set(key, value) {
            Ok(()) => ResponseValue::SimpleString("OK".to_string()),
            Err(_) => ResponseValue::Error("internal server error (poisoned lock)".to_string()),
        }
    }

    fn handle_lpush(&self, args: &[ResponseValue]) -> ResponseValue {
        let key = match args.first() {
            Some(ResponseValue::BulkString(Some(bytes))) => String::from_utf8_lossy(bytes),
            Some(_) => return ResponseValue::Error("ERR key must be bulk string".to_string()),
            None => return ResponseValue::Error("ERR invalid number of arguments".to_string()),
        };

        let mut values = Vec::with_capacity(args.len().saturating_sub(1));
        for arg in &args[1..] {
            if let ResponseValue::BulkString(Some(bytes)) = arg {
                let to_push = Bytes::from(bytes.clone());
                values.push(to_push);
            } else {
                return ResponseValue::Error("ERR pushed values must be bulk strings".to_string());
            }
        }

        match self.kv.lpush(key.to_string(), values) {
            Ok(size) => ResponseValue::Integer(size),
            Err(err) => ResponseValue::Error(format!("ERR internal db error: {:?}", err)),
        }
    }

    fn handle_lpop(&self, args: &[ResponseValue]) -> ResponseValue {
        let key = match args.first() {
            Some(ResponseValue::BulkString(Some(bytes))) => String::from_utf8_lossy(bytes),
            Some(_) => return ResponseValue::Error("ERR key must be bulk string".to_string()),
            None => return ResponseValue::Error("ERR invalid number of arguments".to_string()),
        };

        let count = match args.get(1) {
            Some(ResponseValue::BulkString(Some(bytes))) => {
                match String::from_utf8_lossy(bytes).parse::<i64>() {
                    Ok(num) => num,
                    Err(err) => return ResponseValue::Error(format!("ERR {:?}", err)),
                }
            }
            Some(_) => return ResponseValue::Error("ERR count must be bulk string".to_string()),
            None => 1, // Default count is 1 if not provided
        };

        match self.kv.lpop(&key, count) {
            Ok(bytes_vec) => {
                if bytes_vec.len() == 1 {
                    ResponseValue::BulkString(Some(bytes_vec[0].to_vec()))
                } else {
                    let response_elements: Vec<ResponseValue> = bytes_vec
                        .into_iter()
                        .map(|b| ResponseValue::BulkString(Some(b.to_vec())))
                        .collect();
                    ResponseValue::Array(Some(response_elements))
                }
            }
            Err(err) => ResponseValue::Error(format!("ERR {:?}", err)),
        }
    }

    fn handle_rpush(&self, args: &[ResponseValue]) -> ResponseValue {
        let key = match args.first() {
            Some(ResponseValue::BulkString(Some(bytes))) => String::from_utf8_lossy(bytes),
            Some(_) => return ResponseValue::Error("ERR key must be bulk string".to_string()),
            None => return ResponseValue::Error("ERR invalid number of arguments".to_string()),
        };

        let mut values = Vec::with_capacity(args.len().saturating_sub(1));
        for arg in &args[1..] {
            if let ResponseValue::BulkString(Some(bytes)) = arg {
                let to_push = Bytes::from(bytes.clone());
                values.push(to_push);
            } else {
                return ResponseValue::Error("ERR pushed values must be bulk strings".to_string());
            }
        }

        match self.kv.rpush(key.to_string(), values) {
            Ok(size) => ResponseValue::Integer(size),
            Err(err) => ResponseValue::Error(format!("ERR internal db error: {:?}", err)),
        }
    }

    fn handle_rpop(&self, args: &[ResponseValue]) -> ResponseValue {
        let key = match args.first() {
            Some(ResponseValue::BulkString(Some(bytes))) => String::from_utf8_lossy(bytes),
            Some(_) => return ResponseValue::Error("ERR key must be bulk string".to_string()),
            None => return ResponseValue::Error("ERR invalid number of arguments".to_string()),
        };

        let count = match args.get(1) {
            Some(ResponseValue::BulkString(Some(bytes))) => {
                match String::from_utf8_lossy(bytes).parse::<i64>() {
                    Ok(num) => num,
                    Err(err) => return ResponseValue::Error(format!("ERR {:?}", err)),
                }
            }
            Some(_) => return ResponseValue::Error("ERR count must be bulk string".to_string()),
            None => 1, // Default count is 1 if not provided
        };

        match self.kv.rpop(&key, count) {
            Ok(bytes_vec) => {
                if bytes_vec.len() == 1 {
                    ResponseValue::BulkString(Some(bytes_vec[0].to_vec()))
                } else {
                    let response_elements: Vec<ResponseValue> = bytes_vec
                        .into_iter()
                        .map(|b| ResponseValue::BulkString(Some(b.to_vec())))
                        .collect();
                    ResponseValue::Array(Some(response_elements))
                }
            }
            Err(err) => ResponseValue::Error(format!("ERR {:?}", err)),
        }
    }

    fn handle_lrange(&self, args: &[ResponseValue]) -> ResponseValue {
        let key = match args.first() {
            Some(ResponseValue::BulkString(Some(bytes))) => String::from_utf8_lossy(bytes),
            Some(_) => return ResponseValue::Error("ERR key must be bulk string".to_string()),
            None => return ResponseValue::Error("ERR invalid number of arguments".to_string()),
        };

        let start = match args.get(1) {
            Some(value) => match parse_int(value) {
                Ok(integer) => integer,
                Err(err) => return ResponseValue::Error(err),
            },
            None => return ResponseValue::Error("ERR invalid number of arguments".to_string()),
        };

        let stop = match args.get(2) {
            Some(value) => match parse_int(value) {
                Ok(integer) => integer,
                Err(err) => return ResponseValue::Error(err),
            },
            None => return ResponseValue::Error("ERR invalid number of arguments".to_string()),
        };

        match self.kv.lrange(&key, start, stop) {
            Ok(bytes_vec) => {
                let response_elements: Vec<ResponseValue> = bytes_vec
                    .into_iter()
                    .map(|b| ResponseValue::BulkString(Some(b.to_vec())))
                    .collect();

                ResponseValue::Array(Some(response_elements))
            }
            Err(err) => ResponseValue::Error(format!("ERR {:?}", err)),
        }
    }
}

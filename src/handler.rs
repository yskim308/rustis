use bytes::Bytes;

use crate::kv::{KvStore, RedisValue};
use crate::parser::ResponseValue;

pub struct CommandHandler {
    kv: KvStore,
}

fn parse_int(value: &ResponseValue) -> Result<i64, Bytes> {
    match value {
        ResponseValue::BulkString(Some(bytes)) => {
            let s = std::str::from_utf8(bytes)
                .map_err(|_| "ERR value is not valid utf8".to_string())?;
            s.parse::<i64>()
                .map_err(|_| "ERR value is not an integer or out of range".into())
        }
        _ => Err("ERR protocol error: expected bulk string".into()),
    }
}

const COMPACTION_THRESHOLD: usize = 64;
fn compact(b: &Bytes) -> Bytes {
    b.clone()
    // if b.len() <= COMPACTION_THRESHOLD {
    //     Bytes::copy_from_slice(b)
    // } else {
    //     b.clone()
    // }
}

impl CommandHandler {
    pub fn new(kv: KvStore) -> Self {
        CommandHandler { kv }
    }

    pub fn process_command(&self, value: ResponseValue) -> ResponseValue {
        let items = match value {
            ResponseValue::Array(Some(items)) => items,
            _ => return ResponseValue::Error("request must be array".into()),
        };

        if items.is_empty() {
            return ResponseValue::Error("empty request".into());
        }

        let (cmd, args) = match items.split_first() {
            Some((ResponseValue::BulkString(Some(bytes)), rest)) => (bytes, rest),
            _ => return ResponseValue::Error("command must be bulk string".into()),
        };

        if cmd.eq_ignore_ascii_case(b"PING") {
            ResponseValue::SimpleString("PONG".into())
        } else if cmd.eq_ignore_ascii_case(b"CONFIG") {
            ResponseValue::Array(None)
        } else if cmd.eq_ignore_ascii_case(b"GET") {
            self.handle_get(args)
        } else if cmd.eq_ignore_ascii_case(b"SET") {
            self.handle_set(args)
        } else if cmd.eq_ignore_ascii_case(b"LPUSH") {
            self.handle_lpush(args)
        } else if cmd.eq_ignore_ascii_case(b"LPOP") {
            self.handle_lpop(args)
        } else if cmd.eq_ignore_ascii_case(b"RPUSH") {
            self.handle_rpush(args)
        } else if cmd.eq_ignore_ascii_case(b"RPOP") {
            self.handle_rpop(args)
        } else if cmd.eq_ignore_ascii_case(b"LRANGE") {
            self.handle_lrange(args)
        } else if cmd.eq_ignore_ascii_case(b"SADD") {
            self.handle_sadd(args)
        } else if cmd.eq_ignore_ascii_case(b"SPOP") {
            self.handle_spop(args)
        } else if cmd.eq_ignore_ascii_case(b"SMEMBERS") {
            self.handle_smembers(args)
        } else {
            ResponseValue::Error("invalid command".into())
        }
    }

    fn handle_get(&self, args: &[ResponseValue]) -> ResponseValue {
        if args.len() != 1 {
            return ResponseValue::Error("ERR wrong number of arguments for 'get' command".into());
        }

        let key = match args.first() {
            Some(ResponseValue::BulkString(Some(bytes))) => bytes,
            Some(_) => return ResponseValue::Error("ERR key must be bulk string".into()),
            None => return ResponseValue::Error("ERR invalid number of arguments".into()),
        };

        match self.kv.get(key) {
            Ok(Some(RedisValue::String(b))) => ResponseValue::BulkString(Some(b)),
            Ok(Some(_)) => ResponseValue::Error(
                "WRONGTYPE Operation against a key holding the wrong kind of value".into(),
            ),
            Ok(None) => ResponseValue::BulkString(None),
            Err(_) => ResponseValue::Error("internal server error".into()),
        }
    }

    fn handle_set(&self, args: &[ResponseValue]) -> ResponseValue {
        if args.len() != 2 {
            return ResponseValue::Error("ERR wrong number of arguments for 'set' command".into());
        }

        let key = match args.first() {
            Some(ResponseValue::BulkString(Some(bytes))) => compact(bytes),
            Some(_) => return ResponseValue::Error("ERR key must be bulk string".into()),
            None => return ResponseValue::Error("ERR invalid number of arguments".into()),
        };

        let value = match args.get(1) {
            Some(ResponseValue::BulkString(Some(bytes))) => compact(bytes),
            Some(_) => return ResponseValue::Error("ERR value must be bulk string".into()),
            None => return ResponseValue::Error("ERR invalid number of arguments".into()),
        };

        match self.kv.set(key, value) {
            Ok(()) => ResponseValue::SimpleString("OK".into()),
            Err(_) => ResponseValue::Error("internal server error (poisoned lock)".into()),
        }
    }

    fn handle_lpush(&self, args: &[ResponseValue]) -> ResponseValue {
        let key = match args.first() {
            Some(ResponseValue::BulkString(Some(bytes))) => compact(bytes),
            Some(_) => return ResponseValue::Error("ERR key must be bulk string".into()),
            None => return ResponseValue::Error("ERR invalid number of arguments".into()),
        };

        let mut values = Vec::with_capacity(args.len().saturating_sub(1));
        for arg in &args[1..] {
            if let ResponseValue::BulkString(Some(bytes)) = arg {
                values.push(compact(bytes));
            } else {
                return ResponseValue::Error("ERR pushed values must be bulk strings".into());
            }
        }

        match self.kv.lpush(key, values) {
            Ok(size) => ResponseValue::Integer(size),
            Err(err) => ResponseValue::Error(format!("ERR internal db error: {:?}", err).into()),
        }
    }

    fn handle_lpop(&self, args: &[ResponseValue]) -> ResponseValue {
        let key = match args.first() {
            Some(ResponseValue::BulkString(Some(bytes))) => bytes,
            Some(_) => return ResponseValue::Error("ERR key must be bulk string".into()),
            None => return ResponseValue::Error("ERR invalid number of arguments".into()),
        };

        let count = match args.get(1) {
            Some(ResponseValue::BulkString(Some(bytes))) => {
                match String::from_utf8_lossy(bytes).parse::<i64>() {
                    Ok(num) => num,
                    Err(err) => return ResponseValue::Error(format!("ERR {:?}", err).into()),
                }
            }
            Some(_) => return ResponseValue::Error("ERR count must be bulk string".into()),
            None => 1, // Default count is 1 if not provided
        };

        match self.kv.lpop(key, count) {
            Ok(bytes_vec) => {
                if bytes_vec.len() == 1 {
                    ResponseValue::BulkString(Some(bytes_vec[0].clone()))
                } else {
                    let response_elements: Vec<ResponseValue> = bytes_vec
                        .into_iter()
                        .map(|b| ResponseValue::BulkString(Some(b)))
                        .collect();
                    ResponseValue::Array(Some(response_elements))
                }
            }
            Err(err) => ResponseValue::Error(format!("ERR {:?}", err).into()),
        }
    }

    fn handle_rpush(&self, args: &[ResponseValue]) -> ResponseValue {
        let key = match args.first() {
            Some(ResponseValue::BulkString(Some(bytes))) => compact(bytes),
            Some(_) => return ResponseValue::Error("ERR key must be bulk string".into()),
            None => return ResponseValue::Error("ERR invalid number of arguments".into()),
        };

        let mut values = Vec::with_capacity(args.len().saturating_sub(1));
        for arg in &args[1..] {
            if let ResponseValue::BulkString(Some(bytes)) = arg {
                values.push(compact(bytes));
            } else {
                return ResponseValue::Error("ERR pushed values must be bulk strings".into());
            }
        }

        match self.kv.rpush(key, values) {
            Ok(size) => ResponseValue::Integer(size),
            Err(err) => ResponseValue::Error(format!("ERR internal db error: {:?}", err).into()),
        }
    }

    fn handle_rpop(&self, args: &[ResponseValue]) -> ResponseValue {
        let key = match args.first() {
            Some(ResponseValue::BulkString(Some(bytes))) => bytes,
            Some(_) => return ResponseValue::Error("ERR key must be bulk string".into()),
            None => return ResponseValue::Error("ERR invalid number of arguments".into()),
        };

        let count = match args.get(1) {
            Some(ResponseValue::BulkString(Some(bytes))) => {
                match String::from_utf8_lossy(bytes).parse::<i64>() {
                    Ok(num) => num,
                    Err(err) => return ResponseValue::Error(format!("ERR {:?}", err).into()),
                }
            }
            Some(_) => return ResponseValue::Error("ERR count must be bulk string".into()),
            None => 1, // Default count is 1 if not provided
        };

        match self.kv.rpop(key, count) {
            Ok(bytes_vec) => {
                if bytes_vec.len() == 1 {
                    ResponseValue::BulkString(Some(bytes_vec[0].clone()))
                } else {
                    let response_elements: Vec<ResponseValue> = bytes_vec
                        .into_iter()
                        .map(|b| ResponseValue::BulkString(Some(b)))
                        .collect();
                    ResponseValue::Array(Some(response_elements))
                }
            }
            Err(err) => ResponseValue::Error(format!("ERR {:?}", err).into()),
        }
    }

    fn handle_lrange(&self, args: &[ResponseValue]) -> ResponseValue {
        let key = match args.first() {
            Some(ResponseValue::BulkString(Some(bytes))) => bytes,
            Some(_) => return ResponseValue::Error("ERR key must be bulk string".into()),
            None => return ResponseValue::Error("ERR invalid number of arguments".into()),
        };

        let start = match args.get(1) {
            Some(value) => match parse_int(value) {
                Ok(integer) => integer,
                Err(err) => return ResponseValue::Error(err),
            },
            None => return ResponseValue::Error("ERR invalid number of arguments".into()),
        };

        let stop = match args.get(2) {
            Some(value) => match parse_int(value) {
                Ok(integer) => integer,
                Err(err) => return ResponseValue::Error(err),
            },
            None => return ResponseValue::Error("ERR invalid number of arguments".into()),
        };

        match self.kv.lrange(key, start, stop) {
            Ok(bytes_vec) => {
                let response_elements: Vec<ResponseValue> = bytes_vec
                    .into_iter()
                    .map(|b| ResponseValue::BulkString(Some(b)))
                    .collect();

                ResponseValue::Array(Some(response_elements))
            }
            Err(err) => ResponseValue::Error(format!("ERR {:?}", err).into()),
        }
    }

    fn handle_sadd(&self, args: &[ResponseValue]) -> ResponseValue {
        let key = match args.first() {
            Some(ResponseValue::BulkString(Some(bytes))) => compact(bytes),
            Some(_) => return ResponseValue::Error("ERR key must be bulk string".into()),
            None => return ResponseValue::Error("ERR invalid number of arguments".into()),
        };

        let mut values = Vec::with_capacity(args.len().saturating_sub(1));
        for arg in &args[1..] {
            if let ResponseValue::BulkString(Some(bytes)) = arg {
                let to_push = compact(bytes);
                values.push(to_push);
            } else {
                return ResponseValue::Error("ERR pushed values must be bulk strings".into());
            }
        }

        match self.kv.sadd(key, values) {
            Ok(size) => ResponseValue::Integer(size),
            Err(err) => ResponseValue::Error(format!("ERR internal db error: {:?}", err).into()),
        }
    }

    fn handle_spop(&self, args: &[ResponseValue]) -> ResponseValue {
        let key = match args.first() {
            Some(ResponseValue::BulkString(Some(bytes))) => bytes,
            Some(_) => return ResponseValue::Error("ERR key must be bulk string".into()),
            None => return ResponseValue::Error("ERR invalid number of arguments".into()),
        };

        let count = match args.get(1) {
            Some(value) => match parse_int(value) {
                Ok(n) => n,
                Err(e) => return ResponseValue::Error(e),
            },
            None => 1,
        };

        match self.kv.spop(key, count) {
            Ok(bytes_vec) => {
                let response_vector: Vec<ResponseValue> = bytes_vec
                    .into_iter()
                    .map(|b| ResponseValue::BulkString(Some(b)))
                    .collect();
                ResponseValue::Array(Some(response_vector))
            }
            Err(e) => ResponseValue::Error(format!("ERR: {:?}", e).into()),
        }
    }

    fn handle_smembers(&self, args: &[ResponseValue]) -> ResponseValue {
        let key = match args.first() {
            Some(ResponseValue::BulkString(Some(bytes))) => bytes,
            Some(_) => return ResponseValue::Error("ERR key must be bulk string".into()),
            None => return ResponseValue::Error("ERR invalid number of arguments".into()),
        };

        match self.kv.smembers(key) {
            Ok(bytes_vec) => {
                let response_elements: Vec<ResponseValue> = bytes_vec
                    .into_iter()
                    .map(|b| ResponseValue::BulkString(Some(b)))
                    .collect();
                ResponseValue::Array(Some(response_elements))
            }
            Err(e) => ResponseValue::Error(format!("ERR {:?}", e).into()),
        }
    }
}

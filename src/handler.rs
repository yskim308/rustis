use bytes::Bytes;

use crate::kv::{KvStore, RedisValue};
use crate::message::RespFrame;

fn parse_int(value: &RespFrame) -> Result<i64, Bytes> {
    match value {
        RespFrame::BulkString(Some(bytes)) => {
            let s = std::str::from_utf8(bytes)
                .map_err(|_| "ERR value is not valid utf8".to_string())?;
            s.parse::<i64>()
                .map_err(|_| "ERR value is not an integer or out of range".into())
        }
        _ => Err("ERR protocol error: expected bulk string".into()),
    }
}

pub fn process_command(kv: &KvStore, value: RespFrame) -> RespFrame {
    let items = match value {
        RespFrame::Array(Some(items)) => items,
        _ => return RespFrame::Error("request must be array".into()),
    };

    if items.is_empty() {
        return RespFrame::Error("empty request".into());
    }

    let (cmd, args) = match items.split_first() {
        Some((RespFrame::BulkString(Some(bytes)), rest)) => (bytes, rest),
        _ => return RespFrame::Error("command must be bulk string".into()),
    };

    if cmd.eq_ignore_ascii_case(b"PING") {
        RespFrame::SimpleString("PONG".into())
    } else if cmd.eq_ignore_ascii_case(b"CONFIG") {
        RespFrame::Array(None)
    } else if cmd.eq_ignore_ascii_case(b"GET") {
        handle_get(kv, args)
    } else if cmd.eq_ignore_ascii_case(b"SET") {
        handle_set(kv, args)
    } else if cmd.eq_ignore_ascii_case(b"LPUSH") {
        handle_lpush(kv, args)
    } else if cmd.eq_ignore_ascii_case(b"LPOP") {
        handle_lpop(kv, args)
    } else if cmd.eq_ignore_ascii_case(b"RPUSH") {
        handle_rpush(kv, args)
    } else if cmd.eq_ignore_ascii_case(b"RPOP") {
        handle_rpop(kv, args)
    } else if cmd.eq_ignore_ascii_case(b"LRANGE") {
        handle_lrange(kv, args)
    } else if cmd.eq_ignore_ascii_case(b"SADD") {
        handle_sadd(kv, args)
    } else if cmd.eq_ignore_ascii_case(b"SPOP") {
        handle_spop(kv, args)
    } else if cmd.eq_ignore_ascii_case(b"SMEMBERS") {
        handle_smembers(kv, args)
    } else {
        RespFrame::Error("invalid command".into())
    }
}

fn handle_get(kv: &KvStore, args: &[RespFrame]) -> RespFrame {
    if args.len() != 1 {
        return RespFrame::Error("ERR wrong number of arguments for 'get' command".into());
    }

    let key = match args.first() {
        Some(RespFrame::BulkString(Some(bytes))) => bytes,
        Some(_) => return RespFrame::Error("ERR key must be bulk string".into()),
        None => return RespFrame::Error("ERR invalid number of arguments".into()),
    };

    match kv.get(key) {
        Ok(Some(RedisValue::String(b))) => RespFrame::BulkString(Some(b)),
        Ok(Some(_)) => RespFrame::Error(
            "WRONGTYPE Operation against a key holding the wrong kind of value".into(),
        ),
        Ok(None) => RespFrame::BulkString(None),
        Err(_) => RespFrame::Error("internal server error".into()),
    }
}

fn handle_set(kv: &KvStore, args: &[RespFrame]) -> RespFrame {
    if args.len() != 2 {
        return RespFrame::Error("ERR wrong number of arguments for 'set' command".into());
    }

    let key = match args.first() {
        Some(RespFrame::BulkString(Some(bytes))) => Bytes::copy_from_slice(bytes),
        Some(_) => return RespFrame::Error("ERR key must be bulk string".into()),
        None => return RespFrame::Error("ERR invalid number of arguments".into()),
    };

    let value = match args.get(1) {
        Some(RespFrame::BulkString(Some(bytes))) => Bytes::copy_from_slice(bytes),
        Some(_) => return RespFrame::Error("ERR value must be bulk string".into()),
        None => return RespFrame::Error("ERR invalid number of arguments".into()),
    };

    match kv.set(key, value) {
        Ok(()) => RespFrame::SimpleString("OK".into()),
        Err(_) => RespFrame::Error("internal server error (poisoned lock)".into()),
    }
}

fn handle_lpush(kv: &KvStore, args: &[RespFrame]) -> RespFrame {
    let key = match args.first() {
        Some(RespFrame::BulkString(Some(bytes))) => Bytes::copy_from_slice(bytes),
        Some(_) => return RespFrame::Error("ERR key must be bulk string".into()),
        None => return RespFrame::Error("ERR invalid number of arguments".into()),
    };

    let mut values = Vec::with_capacity(args.len().saturating_sub(1));
    for arg in &args[1..] {
        if let RespFrame::BulkString(Some(bytes)) = arg {
            values.push(Bytes::copy_from_slice(bytes));
        } else {
            return RespFrame::Error("ERR pushed values must be bulk strings".into());
        }
    }

    match kv.lpush(key, values) {
        Ok(size) => RespFrame::Integer(size),
        Err(err) => RespFrame::Error(format!("ERR internal db error: {:?}", err).into()),
    }
}

fn handle_lpop(kv: &KvStore, args: &[RespFrame]) -> RespFrame {
    let key = match args.first() {
        Some(RespFrame::BulkString(Some(bytes))) => bytes,
        Some(_) => return RespFrame::Error("ERR key must be bulk string".into()),
        None => return RespFrame::Error("ERR invalid number of arguments".into()),
    };

    let count = match args.get(1) {
        Some(RespFrame::BulkString(Some(bytes))) => {
            match String::from_utf8_lossy(bytes).parse::<i64>() {
                Ok(num) => num,
                Err(err) => return RespFrame::Error(format!("ERR {:?}", err).into()),
            }
        }
        Some(_) => return RespFrame::Error("ERR count must be bulk string".into()),
        None => 1, // Default count is 1 if not provided
    };

    match kv.lpop(key, count) {
        Ok(bytes_vec) => {
            if bytes_vec.len() == 1 {
                RespFrame::BulkString(Some(bytes_vec[0].clone()))
            } else {
                let response_elements: Vec<RespFrame> = bytes_vec
                    .into_iter()
                    .map(|b| RespFrame::BulkString(Some(b)))
                    .collect();
                RespFrame::Array(Some(response_elements))
            }
        }
        Err(err) => RespFrame::Error(format!("ERR {:?}", err).into()),
    }
}

fn handle_rpush(kv: &KvStore, args: &[RespFrame]) -> RespFrame {
    let key = match args.first() {
        Some(RespFrame::BulkString(Some(bytes))) => Bytes::copy_from_slice(bytes),
        Some(_) => return RespFrame::Error("ERR key must be bulk string".into()),
        None => return RespFrame::Error("ERR invalid number of arguments".into()),
    };

    let mut values = Vec::with_capacity(args.len().saturating_sub(1));
    for arg in &args[1..] {
        if let RespFrame::BulkString(Some(bytes)) = arg {
            values.push(Bytes::copy_from_slice(bytes));
        } else {
            return RespFrame::Error("ERR pushed values must be bulk strings".into());
        }
    }

    match kv.rpush(key, values) {
        Ok(size) => RespFrame::Integer(size),
        Err(err) => RespFrame::Error(format!("ERR internal db error: {:?}", err).into()),
    }
}

fn handle_rpop(kv: &KvStore, args: &[RespFrame]) -> RespFrame {
    let key = match args.first() {
        Some(RespFrame::BulkString(Some(bytes))) => bytes,
        Some(_) => return RespFrame::Error("ERR key must be bulk string".into()),
        None => return RespFrame::Error("ERR invalid number of arguments".into()),
    };

    let count = match args.get(1) {
        Some(RespFrame::BulkString(Some(bytes))) => {
            match String::from_utf8_lossy(bytes).parse::<i64>() {
                Ok(num) => num,
                Err(err) => return RespFrame::Error(format!("ERR {:?}", err).into()),
            }
        }
        Some(_) => return RespFrame::Error("ERR count must be bulk string".into()),
        None => 1, // Default count is 1 if not provided
    };

    match kv.rpop(key, count) {
        Ok(bytes_vec) => {
            if bytes_vec.len() == 1 {
                RespFrame::BulkString(Some(bytes_vec[0].clone()))
            } else {
                let response_elements: Vec<RespFrame> = bytes_vec
                    .into_iter()
                    .map(|b| RespFrame::BulkString(Some(b)))
                    .collect();
                RespFrame::Array(Some(response_elements))
            }
        }
        Err(err) => RespFrame::Error(format!("ERR {:?}", err).into()),
    }
}

fn handle_lrange(kv: &KvStore, args: &[RespFrame]) -> RespFrame {
    let key = match args.first() {
        Some(RespFrame::BulkString(Some(bytes))) => bytes,
        Some(_) => return RespFrame::Error("ERR key must be bulk string".into()),
        None => return RespFrame::Error("ERR invalid number of arguments".into()),
    };

    let start = match args.get(1) {
        Some(value) => match parse_int(value) {
            Ok(integer) => integer,
            Err(err) => return RespFrame::Error(err),
        },
        None => return RespFrame::Error("ERR invalid number of arguments".into()),
    };

    let stop = match args.get(2) {
        Some(value) => match parse_int(value) {
            Ok(integer) => integer,
            Err(err) => return RespFrame::Error(err),
        },
        None => return RespFrame::Error("ERR invalid number of arguments".into()),
    };

    match kv.lrange(key, start, stop) {
        Ok(bytes_vec) => {
            let response_elements: Vec<RespFrame> = bytes_vec
                .into_iter()
                .map(|b| RespFrame::BulkString(Some(b)))
                .collect();

            RespFrame::Array(Some(response_elements))
        }
        Err(err) => RespFrame::Error(format!("ERR {:?}", err).into()),
    }
}

fn handle_sadd(kv: &KvStore, args: &[RespFrame]) -> RespFrame {
    let key = match args.first() {
        Some(RespFrame::BulkString(Some(bytes))) => Bytes::copy_from_slice(bytes),
        Some(_) => return RespFrame::Error("ERR key must be bulk string".into()),
        None => return RespFrame::Error("ERR invalid number of arguments".into()),
    };

    let mut values = Vec::with_capacity(args.len().saturating_sub(1));
    for arg in &args[1..] {
        if let RespFrame::BulkString(Some(bytes)) = arg {
            let to_push = Bytes::copy_from_slice(bytes);
            values.push(to_push);
        } else {
            return RespFrame::Error("ERR pushed values must be bulk strings".into());
        }
    }

    match kv.sadd(key, values) {
        Ok(size) => RespFrame::Integer(size),
        Err(err) => RespFrame::Error(format!("ERR internal db error: {:?}", err).into()),
    }
}

fn handle_spop(kv: &KvStore, args: &[RespFrame]) -> RespFrame {
    let key = match args.first() {
        Some(RespFrame::BulkString(Some(bytes))) => bytes,
        Some(_) => return RespFrame::Error("ERR key must be bulk string".into()),
        None => return RespFrame::Error("ERR invalid number of arguments".into()),
    };

    let count = match args.get(1) {
        Some(value) => match parse_int(value) {
            Ok(n) => n,
            Err(e) => return RespFrame::Error(e),
        },
        None => 1,
    };

    if count < 0 {
        return RespFrame::Error("ERR value is out of range, must be non-negative".into());
    }

    match kv.spop(key, count) {
        Ok(bytes_vec) => {
            let response_vector: Vec<RespFrame> = bytes_vec
                .into_iter()
                .map(|b| RespFrame::BulkString(Some(b)))
                .collect();
            RespFrame::Array(Some(response_vector))
        }
        Err(e) => RespFrame::Error(format!("ERR: {:?}", e).into()),
    }
}

fn handle_smembers(kv: &KvStore, args: &[RespFrame]) -> RespFrame {
    let key = match args.first() {
        Some(RespFrame::BulkString(Some(bytes))) => bytes,
        Some(_) => return RespFrame::Error("ERR key must be bulk string".into()),
        None => return RespFrame::Error("ERR invalid number of arguments".into()),
    };

    match kv.smembers(key) {
        Ok(bytes_vec) => {
            let response_elements: Vec<RespFrame> = bytes_vec
                .into_iter()
                .map(|b| RespFrame::BulkString(Some(b)))
                .collect();
            RespFrame::Array(Some(response_elements))
        }
        Err(e) => RespFrame::Error(format!("ERR {:?}", e).into()),
    }
}

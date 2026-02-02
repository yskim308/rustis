use std::hash::{DefaultHasher, Hash, Hasher};

use bytes::{Bytes, BytesMut};
use tokio::sync::mpsc::UnboundedSender;

use crate::message::{ResponseValue, WorkerMessage};

pub fn route_message(
    router: &[UnboundedSender<WorkerMessage>],
    frame: ResponseValue,
    seq: u64,
    writer_tx: UnboundedSender<Bytes>,
) {
    // make sure parsed frame is an array
    let items = match &frame {
        ResponseValue::Array(Some(items)) => items,
        _ => {
            send_error(&writer_tx, "Value must be array");
            return;
        }
    };

    // make sure array is not empty
    if items.is_empty() {
        send_error(&writer_tx, "empty request");
        return;
    }

    // extract key
    let key = match extract_key(&writer_tx, items) {
        Some(key) => key,
        None => {
            return;
        }
    };

    // hash and send
    let router_len = router.len() as u64;
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    let worker_mailbox = hasher.finish() % router_len;

    // send frame to correct worker
    let tx = match router.get(worker_mailbox as usize) {
        Some(tx) => tx,
        None => {
            send_error(&writer_tx, "internal server error, invalid worker index");
            return;
        }
    };

    let _ = tx.send(WorkerMessage {
        seq,
        response_value: frame,
    });
}

fn send_error(writer_tx: &UnboundedSender<Bytes>, error_msg: &'static str) {
    let mut write_buffer = BytesMut::with_capacity(1024);
    ResponseValue::Error(error_msg.into()).serialize(&mut write_buffer);
    writer_tx.send(write_buffer.freeze());
    return;
}

fn send_string(writer_tx: &UnboundedSender<Bytes>, msg: &'static str) {
    let mut write_buffer = BytesMut::with_capacity(1024);
    ResponseValue::SimpleString(msg.into()).serialize(&mut write_buffer);
    writer_tx.send(write_buffer.freeze());
    return;
}

fn extract_key(writer_tx: &UnboundedSender<Bytes>, items: &Vec<ResponseValue>) -> Option<Bytes> {
    let (cmd, args) = match items.split_first() {
        Some((ResponseValue::BulkString(Some(bytes)), rest)) => (bytes, rest),
        _ => {
            send_error(writer_tx, "command must be bulk string");
            return None;
        }
    };

    if cmd.eq_ignore_ascii_case(b"PING") {
        send_string(writer_tx, "PONG");
        return None;
    } else if cmd.eq_ignore_ascii_case(b"CONFIG") {
        send_string(writer_tx, "");
        return None;
    }

    let key = match args.first() {
        Some(ResponseValue::BulkString(Some(bytes))) => bytes,
        _ => {
            send_error(writer_tx, "error while parsing key");
            return None;
        }
    };

    Some(key.clone())
}

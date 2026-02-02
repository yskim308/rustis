use std::hash::{DefaultHasher, Hash, Hasher};

use bytes::{Bytes, BytesMut};
use tokio::sync::mpsc::UnboundedSender;

use crate::message::{ResponseMessage, ResponseValue, WorkerMessage};

pub fn route_message(
    router: &[UnboundedSender<WorkerMessage>],
    frame: ResponseValue,
    seq: u64,
    writer_tx: UnboundedSender<ResponseMessage>,
) {
    // make sure parsed frame is an array
    let items = match &frame {
        ResponseValue::Array(Some(items)) => items,
        _ => {
            send_error(&writer_tx, seq, "Value must be array");
            return;
        }
    };

    // make sure array is not empty
    if items.is_empty() {
        send_error(&writer_tx, seq, "empty request");
        return;
    }

    // extract key
    let key = match extract_key(&writer_tx, seq, items) {
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
            send_error(
                &writer_tx,
                seq,
                "internal server error, invalid worker index",
            );
            return;
        }
    };

    tx.send(WorkerMessage {
        seq,
        response_value: frame,
        tx: writer_tx,
    })
    .unwrap()
}

fn send_error(writer_tx: &UnboundedSender<ResponseMessage>, seq: u64, error_msg: &'static str) {
    writer_tx
        .send(ResponseMessage {
            seq,
            response_value: ResponseValue::Error(error_msg.into()),
        })
        .unwrap();
}

fn send_string(writer_tx: &UnboundedSender<ResponseMessage>, seq: u64, msg: &'static str) {
    writer_tx
        .send(ResponseMessage {
            seq,
            response_value: ResponseValue::Error(msg.into()),
        })
        .unwrap();
}

fn extract_key(
    writer_tx: &UnboundedSender<ResponseMessage>,
    seq: u64,
    items: &[ResponseValue],
) -> Option<Bytes> {
    let (cmd, args) = match items.split_first() {
        Some((ResponseValue::BulkString(Some(bytes)), rest)) => (bytes, rest),
        _ => {
            send_error(writer_tx, seq, "command must be bulk string");
            return None;
        }
    };

    if cmd.eq_ignore_ascii_case(b"PING") {
        send_string(writer_tx, seq, "PONG");
        return None;
    } else if cmd.eq_ignore_ascii_case(b"CONFIG") {
        send_string(writer_tx, seq, "");
        return None;
    }

    let key = match args.first() {
        Some(ResponseValue::BulkString(Some(bytes))) => bytes,
        _ => {
            send_error(writer_tx, seq, "error while parsing key");
            return None;
        }
    };

    Some(key.clone())
}

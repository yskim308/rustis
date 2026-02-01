use bytes::{BufMut, Bytes, BytesMut};
use tokio::sync::oneshot;

pub enum ShardRequest {
    Commmand {
        args: Vec<Bytes>,
        response_tx: oneshot::Sender<ResponseValue>,
    },
}

#[derive(Debug, PartialEq, Clone)]
pub enum ResponseValue {
    SimpleString(Bytes),
    Error(Bytes),
    Integer(i64),
    BulkString(Option<Bytes>),
    Array(Option<Vec<ResponseValue>>),
}

impl ResponseValue {
    pub fn serialize(&self, dst: &mut BytesMut) {
        match self {
            ResponseValue::SimpleString(s) => {
                dst.put_u8(b'+');
                dst.put_slice(s);
                dst.put_slice(b"\r\n");
            }
            ResponseValue::Error(msg) => {
                dst.put_u8(b'-');
                dst.put_slice(msg);
                dst.put_slice(b"\r\n");
            }
            ResponseValue::Integer(i) => {
                dst.put_u8(b':');
                let val_str = i.to_string();
                dst.put_slice(val_str.as_bytes());
                dst.put_slice(b"\r\n");
            }
            ResponseValue::BulkString(None) => {
                dst.put_slice(b"$-1\r\n");
            }
            ResponseValue::BulkString(Some(data)) => {
                dst.put_u8(b'$');
                dst.put_slice(data.len().to_string().as_bytes());
                dst.put_slice(b"\r\n");
                dst.put_slice(data);
                dst.put_slice(b"\r\n");
            }
            ResponseValue::Array(None) => {
                dst.put_slice(b"*-1\r\n");
            }
            ResponseValue::Array(Some(items)) => {
                dst.put_u8(b'*');
                dst.put_slice(items.len().to_string().as_bytes());
                dst.put_slice(b"\r\n");
                for item in items {
                    item.serialize(dst);
                }
            }
        }
    }
}

pub struct WorkerMessage {
    pub seq: u64,
    pub response_value: ResponseValue,
}

pub struct ResponseMessage {
    pub seq: u64,
    pub response_value: ResponseValue,
}

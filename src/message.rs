use bytes::{BufMut, Bytes, BytesMut};

#[derive(Debug, PartialEq, Clone)]
pub enum RespFrame {
    SimpleString(Bytes),
    Error(Bytes),
    Integer(i64),
    BulkString(Option<Bytes>),
    Array(Option<Vec<RespFrame>>),
}

impl RespFrame {
    pub fn serialize(&self, dst: &mut BytesMut) {
        match self {
            RespFrame::SimpleString(s) => {
                dst.put_u8(b'+');
                dst.put_slice(s);
                dst.put_slice(b"\r\n");
            }
            RespFrame::Error(msg) => {
                dst.put_u8(b'-');
                dst.put_slice(msg);
                dst.put_slice(b"\r\n");
            }
            RespFrame::Integer(i) => {
                dst.put_u8(b':');
                let val_str = i.to_string();
                dst.put_slice(val_str.as_bytes());
                dst.put_slice(b"\r\n");
            }
            RespFrame::BulkString(None) => {
                dst.put_slice(b"$-1\r\n");
            }
            RespFrame::BulkString(Some(data)) => {
                dst.put_u8(b'$');
                dst.put_slice(data.len().to_string().as_bytes());
                dst.put_slice(b"\r\n");
                dst.put_slice(data);
                dst.put_slice(b"\r\n");
            }
            RespFrame::Array(None) => {
                dst.put_slice(b"*-1\r\n");
            }
            RespFrame::Array(Some(items)) => {
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

#[derive(Debug)]
pub struct WorkerMessage {
    pub seq: u64,
    pub conn_token: usize,
    pub src_core: usize,
    pub response_value: RespFrame,
}

#[derive(Debug)]
pub struct ResponseMessage {
    pub seq: u64,
    pub conn_token: usize,
    pub response_value: RespFrame,
}

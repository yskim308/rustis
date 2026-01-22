use std::num::ParseIntError;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use memchr::memmem;
#[derive(Debug, PartialEq, Clone)]
pub enum ResponseValue {
    SimpleString(String),
    Error(String),
    Integer(i64),
    BulkString(Option<Bytes>),
    Array(Option<Vec<ResponseValue>>),
}

impl ResponseValue {
    // Goal 3: When serializing, write directly to the buffer (Zero allocation)
    pub fn serialize(&self, dst: &mut BytesMut) {
        match self {
            ResponseValue::SimpleString(s) => {
                dst.put_u8(b'+');
                dst.put_slice(s.as_bytes());
                dst.put_slice(b"\r\n");
            }
            ResponseValue::Error(msg) => {
                dst.put_u8(b'-');
                dst.put_slice(msg.as_bytes());
                dst.put_slice(b"\r\n");
            }
            ResponseValue::Integer(i) => {
                dst.put_u8(b':');
                // Use a stack buffer to avoid allocation for itoa
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
                dst.put_slice(data); // O(N) copy, but unavoidable for network write
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

#[derive(Debug, PartialEq)]
pub enum BufParseError {
    Incomplete,
    UnexpectedEOF { expected: &'static str },
    InvalidFirstByte(Option<u8>),
    UnexpectedByte { expected: u8, found: Option<u8> },
    StringConversionError(ParseIntError),
    ByteConversionError(std::str::Utf8Error),
}

impl From<std::str::Utf8Error> for BufParseError {
    fn from(value: std::str::Utf8Error) -> Self {
        BufParseError::ByteConversionError(value)
    }
}

impl From<std::num::ParseIntError> for BufParseError {
    fn from(value: std::num::ParseIntError) -> Self {
        BufParseError::StringConversionError(value)
    }
}

fn read_line(buffer: &BytesMut) -> Option<usize> {
    memmem::find(buffer, b"\r\n")
}

pub fn parse(buffer: &mut BytesMut) -> Result<ResponseValue, BufParseError> {
    match buffer.first() {
        Some(b'+') => parse_simple_string(buffer),
        Some(b'-') => parse_simple_error(buffer),
        Some(b':') => parse_integer(buffer),
        Some(b'$') => parse_bulk_string(buffer),
        Some(b'*') => parse_array(buffer),
        Some(byte) if byte.is_ascii_alphabetic() => parse_inline(buffer),
        Some(byte) => Err(BufParseError::InvalidFirstByte(Some(*byte))),
        None => Err(BufParseError::Incomplete),
    }
}

fn parse_inline(buffer: &mut BytesMut) -> Result<ResponseValue, BufParseError> {
    let header_end = match read_line(buffer) {
        Some(i) => i,
        None => return Err(BufParseError::Incomplete),
    };

    let val_slice = &buffer[..header_end];

    let parts: Vec<&[u8]> = val_slice
        .split(|b| b.is_ascii_whitespace())
        .filter(|chunk| chunk.is_empty())
        .collect();

    let items = parts
        .into_iter()
        .map(|bytes| ResponseValue::BulkString(Some(Bytes::copy_from_slice(bytes))))
        .collect();

    buffer.advance(header_end + 2);

    Ok(ResponseValue::Array(Some(items)))
}

fn parse_simple_string(buffer: &mut BytesMut) -> Result<ResponseValue, BufParseError> {
    let header_end = match read_line(buffer) {
        Some(i) => i,
        None => return Err(BufParseError::Incomplete),
    };

    let val_slice = &buffer[1..header_end];

    let return_string = String::from_utf8_lossy(val_slice).into_owned();
    buffer.advance(header_end + 2);

    Ok(ResponseValue::SimpleString(return_string))
}

fn parse_simple_error(buffer: &mut BytesMut) -> Result<ResponseValue, BufParseError> {
    let header_end = match read_line(buffer) {
        Some(i) => i,
        None => return Err(BufParseError::Incomplete),
    };

    let val_slice = &buffer[1..header_end];

    let return_string = String::from_utf8_lossy(val_slice).into_owned();
    buffer.advance(header_end + 2);

    Ok(ResponseValue::Error(return_string))
}

fn parse_integer(buffer: &mut BytesMut) -> Result<ResponseValue, BufParseError> {
    let header_end = match read_line(buffer) {
        Some(i) => i,
        None => return Err(BufParseError::Incomplete),
    };

    let val_slice = &buffer[1..header_end];

    let integer_val: i64 = std::str::from_utf8(val_slice)?.parse()?;

    buffer.advance(header_end + 2);

    Ok(ResponseValue::Integer(integer_val))
}

fn parse_bulk_string(buffer: &mut BytesMut) -> Result<ResponseValue, BufParseError> {
    let header_end = match read_line(buffer) {
        Some(i) => i,
        None => return Err(BufParseError::Incomplete),
    };

    let len_slice = &buffer[1..header_end];
    let integer_len: i64 = std::str::from_utf8(len_slice)?.parse()?;

    if integer_len < 0 {
        buffer.advance(header_end + 2);
        return Ok(ResponseValue::BulkString(None));
    }

    let len = integer_len as usize;
    let total_length = header_end + 2 + len + 2;
    if buffer.len() < total_length {
        return Err(BufParseError::Incomplete);
    }

    buffer.advance(header_end + 2);

    let data = buffer.split_to(len).freeze();

    if buffer[0] != b'\r' || buffer[1] != b'\n' {
        return Err(BufParseError::UnexpectedByte {
            expected: b'\r',
            found: Some(buffer[0]),
        });
    }

    buffer.advance(2);

    Ok(ResponseValue::BulkString(Some(data)))
}

fn parse_array(buffer: &mut BytesMut) -> Result<ResponseValue, BufParseError> {
    let header_end = match read_line(buffer) {
        Some(i) => i,
        None => return Err(BufParseError::Incomplete),
    };

    let val_slice = &buffer[1..header_end];

    let length: i64 = std::str::from_utf8(val_slice)?.parse()?;

    if length < 0 {
        return Ok(ResponseValue::Array(None));
    }

    buffer.advance(header_end + 2);

    let mut items = Vec::new();

    for _ in 0..length {
        let value = parse(buffer)?;
        items.push(value);
    }

    Ok(ResponseValue::Array(Some(items)))
}

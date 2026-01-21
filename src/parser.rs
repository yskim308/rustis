use std::{num::ParseIntError, string::FromUtf8Error};

use bytes::{Buf, Bytes, BytesMut};
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
    pub fn serialize(&self) -> Vec<u8> {
        match self {
            ResponseValue::SimpleString(s) => format!("+{}\r\n", s).into_bytes(),
            ResponseValue::Error(msg) => format!("-{}\r\n", msg).into_bytes(),
            ResponseValue::Integer(i) => format!(":{}\r\n", i).into_bytes(),
            ResponseValue::BulkString(None) => b"$-1\r\n".to_vec(),
            ResponseValue::BulkString(Some(data)) => {
                let mut bytes = Vec::new();
                bytes.extend_from_slice(format!("${}\r\n", data.len()).as_bytes());
                bytes.extend_from_slice(data);
                bytes.extend_from_slice(b"\r\n");
                bytes
            }
            ResponseValue::Array(Some(items)) => {
                let mut bytes = Vec::new();
                bytes.extend_from_slice(format!("*{}\r\n", items.len()).as_bytes());
                for item in items {
                    bytes.extend(item.serialize());
                }
                bytes
            }
            ResponseValue::Array(None) => b"*-1\r\n".to_vec(),
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

fn expect_byte(found: u8, expected: u8) -> Result<(), BufParseError> {
    if found != expected {
        return Err(BufParseError::UnexpectedByte {
            expected,
            found: Some(found),
        });
    }

    Ok(())
}

fn read_line(buffer: &BytesMut) -> Option<usize> {
    memmem::find(&buffer, b"\r\n")
}

pub fn parse(buffer: &mut BytesMut) -> Result<ResponseValue, BufParseError> {
    match buffer.get(0) {
        Some(b'+') => parse_simple_string(buffer),
        Some(b'-') => parse_simple_error(buffer),
        Some(b':') => parse_integer(buffer),
        Some(b'$') => parse_bulk_string(buffer),
        Some(b'*') => parse_array(buffer),
        Some(byte) if byte.is_ascii_alphabetic() => parse_inline(buffer),
        Some(byte) => Err(BufParseError::InvalidFirstByte(Some(byte.clone()))),
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
        return Ok(ResponseValue::BulkString(None));
    }

    buffer.advance(header_end + 2);

    let data = buffer.split_to(len).freeze();

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

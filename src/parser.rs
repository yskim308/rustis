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

fn find_crlf(data: &[u8]) -> Option<usize> {
    memmem::find(data, b"\r\n")
}

/// Main public parsing function - atomically parses and advances buffer
pub fn parse(buffer: &mut BytesMut) -> Result<ResponseValue, BufParseError> {
    // Parse from immutable view first
    let (value, bytes_consumed) = parse_value(&buffer[..])?;

    // Only advance buffer if parsing succeeded
    buffer.advance(bytes_consumed);

    Ok(value)
}

/// Internal parser that works on immutable slices
fn parse_value(data: &[u8]) -> Result<(ResponseValue, usize), BufParseError> {
    match data.first() {
        Some(b'+') => parse_simple_string(data),
        Some(b'-') => parse_simple_error(data),
        Some(b':') => parse_integer(data),
        Some(b'$') => parse_bulk_string(data),
        Some(b'*') => parse_array(data),
        Some(byte) if byte.is_ascii_alphabetic() => parse_inline(data),
        Some(byte) => Err(BufParseError::InvalidFirstByte(Some(*byte))),
        None => Err(BufParseError::Incomplete),
    }
}

fn parse_inline(data: &[u8]) -> Result<(ResponseValue, usize), BufParseError> {
    let header_end = match find_crlf(data) {
        Some(i) => i,
        None => return Err(BufParseError::Incomplete),
    };

    let val_slice = &data[..header_end];

    let parts: Vec<&[u8]> = val_slice
        .split(|b| b.is_ascii_whitespace())
        .filter(|chunk| !chunk.is_empty())
        .collect();

    let items = parts
        .into_iter()
        .map(|bytes| ResponseValue::BulkString(Some(Bytes::copy_from_slice(bytes))))
        .collect();

    let bytes_consumed = header_end + 2;

    Ok((ResponseValue::Array(Some(items)), bytes_consumed))
}

fn parse_simple_string(data: &[u8]) -> Result<(ResponseValue, usize), BufParseError> {
    let header_end = match find_crlf(data) {
        Some(i) => i,
        None => return Err(BufParseError::Incomplete),
    };

    let val_slice = &data[1..header_end];
    let return_string = String::from_utf8_lossy(val_slice).into_owned();
    let bytes_consumed = header_end + 2;

    Ok((ResponseValue::SimpleString(return_string), bytes_consumed))
}

fn parse_simple_error(data: &[u8]) -> Result<(ResponseValue, usize), BufParseError> {
    let header_end = match find_crlf(data) {
        Some(i) => i,
        None => return Err(BufParseError::Incomplete),
    };

    let val_slice = &data[1..header_end];
    let return_string = String::from_utf8_lossy(val_slice).into_owned();
    let bytes_consumed = header_end + 2;

    Ok((ResponseValue::Error(return_string), bytes_consumed))
}

fn parse_integer(data: &[u8]) -> Result<(ResponseValue, usize), BufParseError> {
    let header_end = match find_crlf(data) {
        Some(i) => i,
        None => return Err(BufParseError::Incomplete),
    };

    let val_slice = &data[1..header_end];
    let integer_val: i64 = std::str::from_utf8(val_slice)?.parse()?;
    let bytes_consumed = header_end + 2;

    Ok((ResponseValue::Integer(integer_val), bytes_consumed))
}

fn parse_bulk_string(data: &[u8]) -> Result<(ResponseValue, usize), BufParseError> {
    let header_end = match find_crlf(data) {
        Some(i) => i,
        None => return Err(BufParseError::Incomplete),
    };

    let len_slice = &data[1..header_end];
    let integer_len: i64 = std::str::from_utf8(len_slice)?.parse()?;

    // Handle null bulk string
    if integer_len < 0 {
        let bytes_consumed = header_end + 2;
        return Ok((ResponseValue::BulkString(None), bytes_consumed));
    }

    let len = integer_len as usize;
    let total_length = header_end + 2 + len + 2;

    // Check if we have all the data
    if data.len() < total_length {
        return Err(BufParseError::Incomplete);
    }

    // Verify trailing CRLF
    let data_start = header_end + 2;
    let data_end = data_start + len;

    if data[data_end] != b'\r' || data[data_end + 1] != b'\n' {
        return Err(BufParseError::UnexpectedByte {
            expected: b'\r',
            found: Some(data[data_end]),
        });
    }

    // Extract the actual string data
    let string_data = Bytes::copy_from_slice(&data[data_start..data_end]);

    Ok((ResponseValue::BulkString(Some(string_data)), total_length))
}

fn parse_array(data: &[u8]) -> Result<(ResponseValue, usize), BufParseError> {
    let header_end = match find_crlf(data) {
        Some(i) => i,
        None => return Err(BufParseError::Incomplete),
    };

    let val_slice = &data[1..header_end];
    let length: i64 = std::str::from_utf8(val_slice)?.parse()?;

    // Handle null array
    if length < 0 {
        let bytes_consumed = header_end + 2;
        return Ok((ResponseValue::Array(None), bytes_consumed));
    }

    let mut offset = header_end + 2;
    let mut items = Vec::with_capacity(length as usize);

    // Parse each element in the array
    for _ in 0..length {
        // Check if we have data left
        if offset >= data.len() {
            return Err(BufParseError::Incomplete);
        }

        // Parse the next value from the remaining data
        let (value, consumed) = parse_value(&data[offset..])?;
        items.push(value);
        offset += consumed;
    }

    Ok((ResponseValue::Array(Some(items)), offset))
}

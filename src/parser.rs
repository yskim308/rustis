use crate::message::ResponseValue;
use bytes::{Bytes, BytesMut};
use memchr::memmem;
use std::num::ParseIntError;

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

pub fn parse(buffer: &mut BytesMut) -> Result<ResponseValue, BufParseError> {
    let bytes_needed = peek_bytes_needed(&buffer[..])?;

    if buffer.len() < bytes_needed {
        return Err(BufParseError::Incomplete);
    }

    let frame = buffer.split_to(bytes_needed).freeze();

    parse_frame(&frame)
}

fn peek_bytes_needed(data: &[u8]) -> Result<usize, BufParseError> {
    match data.first() {
        Some(b'+') | Some(b'-') | Some(b':') => {
            let header_end = find_crlf(data).ok_or(BufParseError::Incomplete)?;
            Ok(header_end + 2)
        }
        Some(b'$') => peek_bulk_string_size(data),
        Some(b'*') => peek_array_size(data),
        Some(byte) if byte.is_ascii_alphabetic() => {
            let header_end = find_crlf(data).ok_or(BufParseError::Incomplete)?;
            Ok(header_end + 2)
        }
        Some(byte) => Err(BufParseError::InvalidFirstByte(Some(*byte))),
        None => Err(BufParseError::Incomplete),
    }
}

fn peek_bulk_string_size(data: &[u8]) -> Result<usize, BufParseError> {
    let header_end = find_crlf(data).ok_or(BufParseError::Incomplete)?;
    let len_slice = &data[1..header_end];
    let integer_len: i64 = std::str::from_utf8(len_slice)?.parse()?;

    if integer_len < 0 {
        return Ok(header_end + 2);
    }

    let len = integer_len as usize;
    let total_length = header_end + 2 + len + 2;

    Ok(total_length)
}

fn peek_array_size(data: &[u8]) -> Result<usize, BufParseError> {
    let header_end = find_crlf(data).ok_or(BufParseError::Incomplete)?;
    let val_slice = &data[1..header_end];
    let length: i64 = std::str::from_utf8(val_slice)?.parse()?;

    if length < 0 {
        return Ok(header_end + 2);
    }

    let mut offset = header_end + 2;

    // Recursively peek at each array element
    for _ in 0..length {
        if offset >= data.len() {
            return Err(BufParseError::Incomplete);
        }
        let element_size = peek_bytes_needed(&data[offset..])?;
        offset += element_size;
    }

    Ok(offset)
}

/// Parse a complete frame into a ResponseValue using zero-copy slices
fn parse_frame(frame: &Bytes) -> Result<ResponseValue, BufParseError> {
    let (value, consumed) = parse_value_from_frame(frame, 0)?;

    debug_assert_eq!(consumed, frame.len());

    Ok(value)
}

/// Parse a value from a frame starting at offset, returning (value, bytes_consumed)
fn parse_value_from_frame(
    frame: &Bytes,
    offset: usize,
) -> Result<(ResponseValue, usize), BufParseError> {
    let data = &frame[offset..];

    match data.first() {
        Some(b'+') => parse_simple_string_frame(frame, offset),
        Some(b'-') => parse_simple_error_frame(frame, offset),
        Some(b':') => parse_integer_frame(frame, offset),
        Some(b'$') => parse_bulk_string_frame(frame, offset),
        Some(b'*') => parse_array_frame(frame, offset),
        Some(byte) if byte.is_ascii_alphabetic() => parse_inline_frame(frame, offset),
        Some(byte) => Err(BufParseError::InvalidFirstByte(Some(*byte))),
        None => Err(BufParseError::Incomplete),
    }
}

fn parse_simple_string_frame(
    frame: &Bytes,
    offset: usize,
) -> Result<(ResponseValue, usize), BufParseError> {
    let data = &frame[offset..];
    let header_end = find_crlf(data).ok_or(BufParseError::Incomplete)?;

    // Zero-copy slice! Just adjusts pointers and ref count
    let string_bytes = frame.slice((offset + 1)..(offset + header_end));
    let bytes_consumed = header_end + 2;

    Ok((ResponseValue::SimpleString(string_bytes), bytes_consumed))
}

fn parse_simple_error_frame(
    frame: &Bytes,
    offset: usize,
) -> Result<(ResponseValue, usize), BufParseError> {
    let data = &frame[offset..];
    let header_end = find_crlf(data).ok_or(BufParseError::Incomplete)?;

    // Zero-copy slice!
    let error_bytes = frame.slice((offset + 1)..(offset + header_end));
    let bytes_consumed = header_end + 2;

    Ok((ResponseValue::Error(error_bytes), bytes_consumed))
}

fn parse_integer_frame(
    frame: &Bytes,
    offset: usize,
) -> Result<(ResponseValue, usize), BufParseError> {
    let data = &frame[offset..];
    let header_end = find_crlf(data).ok_or(BufParseError::Incomplete)?;

    let val_slice = &data[1..header_end];
    let integer_val: i64 = std::str::from_utf8(val_slice)?.parse()?;
    let bytes_consumed = header_end + 2;

    Ok((ResponseValue::Integer(integer_val), bytes_consumed))
}

fn parse_bulk_string_frame(
    frame: &Bytes,
    offset: usize,
) -> Result<(ResponseValue, usize), BufParseError> {
    let data = &frame[offset..];
    let header_end = find_crlf(data).ok_or(BufParseError::Incomplete)?;

    let len_slice = &data[1..header_end];
    let integer_len: i64 = std::str::from_utf8(len_slice)?.parse()?;

    if integer_len < 0 {
        let bytes_consumed = header_end + 2;
        return Ok((ResponseValue::BulkString(None), bytes_consumed));
    }

    let len = integer_len as usize;
    let data_start = offset + header_end + 2;
    let data_end = data_start + len;
    let total_length = header_end + 2 + len + 2;

    // Verify trailing CRLF
    if frame[data_end] != b'\r' || frame[data_end + 1] != b'\n' {
        return Err(BufParseError::UnexpectedByte {
            expected: b'\r',
            found: Some(frame[data_end]),
        });
    }

    // Zero-copy slice! This is the magic - no memcpy
    let string_data = frame.slice(data_start..data_end);

    Ok((ResponseValue::BulkString(Some(string_data)), total_length))
}

fn parse_inline_frame(
    frame: &Bytes,
    offset: usize,
) -> Result<(ResponseValue, usize), BufParseError> {
    let data = &frame[offset..];
    let header_end = find_crlf(data).ok_or(BufParseError::Incomplete)?;

    let val_slice = &data[..header_end];

    // Find word boundaries
    let mut items = Vec::new();
    let mut word_start = None;

    for (i, &byte) in val_slice.iter().enumerate() {
        if byte.is_ascii_whitespace() {
            if let Some(start) = word_start {
                // Zero-copy slice for each word
                let word = frame.slice((offset + start)..(offset + i));
                items.push(ResponseValue::BulkString(Some(word)));
                word_start = None;
            }
        } else if word_start.is_none() {
            word_start = Some(i);
        }
    }

    // Handle final word
    if let Some(start) = word_start {
        let word = frame.slice((offset + start)..(offset + header_end));
        items.push(ResponseValue::BulkString(Some(word)));
    }

    let bytes_consumed = header_end + 2;

    Ok((ResponseValue::Array(Some(items)), bytes_consumed))
}

fn parse_array_frame(
    frame: &Bytes,
    offset: usize,
) -> Result<(ResponseValue, usize), BufParseError> {
    let data = &frame[offset..];
    let header_end = find_crlf(data).ok_or(BufParseError::Incomplete)?;

    let val_slice = &data[1..header_end];
    let length: i64 = std::str::from_utf8(val_slice)?.parse()?;

    if length < 0 {
        let bytes_consumed = header_end + 2;
        return Ok((ResponseValue::Array(None), bytes_consumed));
    }

    let mut local_offset = header_end + 2;
    let mut items = Vec::with_capacity(length as usize);

    for _ in 0..length {
        let (value, consumed) = parse_value_from_frame(frame, offset + local_offset)?;
        items.push(value);
        local_offset += consumed;
    }

    Ok((ResponseValue::Array(Some(items)), local_offset))
}

// Helper to convert Bytes to &str when needed (e.g., for command handling)
impl ResponseValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            ResponseValue::SimpleString(b) | ResponseValue::Error(b) => std::str::from_utf8(b).ok(),
            ResponseValue::BulkString(Some(b)) => std::str::from_utf8(b).ok(),
            _ => None,
        }
    }
}

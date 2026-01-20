use std::{num::ParseIntError, string::FromUtf8Error};

use bytes::{Buf, BytesMut};

#[derive(Debug, PartialEq, Clone)]
pub enum ResponseValue {
    SimpleString(String),
    Error(String),
    Integer(i64),
    BulkString(Option<Vec<u8>>),
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
    InvalidLength,
    UnexpectedEOF { expected: &'static str },
    InvalidFirstByte(Option<u8>),
    UnexpectedByte { expected: u8, found: Option<u8> },
    StringConversionError(ParseIntError),
    ByteConversionError(FromUtf8Error),
}

impl From<FromUtf8Error> for BufParseError {
    fn from(value: FromUtf8Error) -> Self {
        BufParseError::ByteConversionError(value)
    }
}

impl From<ParseIntError> for BufParseError {
    fn from(value: ParseIntError) -> Self {
        BufParseError::StringConversionError(value)
    }
}

pub struct Parser {
    pub buffer: BytesMut,
    cursor: usize,
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

impl Parser {
    pub fn new(capacity: usize) -> Self {
        let buffer = BytesMut::with_capacity(capacity);
        Self { buffer, cursor: 0 }
    }

    fn peek(&self) -> Option<u8> {
        if self.cursor >= self.buffer.len() {
            None
        } else {
            Some(self.buffer[self.cursor])
        }
    }

    pub fn compact(&mut self) {
        if self.cursor > 0 {
            self.buffer.advance(self.cursor);
            self.cursor = 0;
        }
    }

    fn read_line(&mut self) -> Result<String, BufParseError> {
        let start = self.cursor;

        while let Some(byte) = self.peek() {
            if byte == b'\r' {
                break;
            }
            self.cursor += 1;
        }

        if self.cursor >= self.buffer.len() {
            return Err(BufParseError::Incomplete);
        }

        let bytes = &self.buffer[start..self.cursor];
        let output = String::from_utf8(bytes.to_vec())?;

        self.cursor += 1;
        match self.peek() {
            Some(b'\n') => Ok(()),
            Some(other) => Err(BufParseError::UnexpectedByte {
                expected: b'\n',
                found: Some(other),
            }),
            None => Err(BufParseError::Incomplete),
        }?;

        self.cursor += 1;

        Ok(output)
    }

    pub fn parse(&mut self) -> Result<ResponseValue, BufParseError> {
        match self.peek() {
            Some(b'+') => self.parse_simple_string(),
            Some(b'-') => self.parse_simple_error(),
            Some(b':') => self.parse_integer(),
            Some(b'$') => self.parse_bulk_string(),
            Some(b'*') => self.parse_array(),
            Some(byte) => Err(BufParseError::InvalidFirstByte(Some(byte))),
            None => Err(BufParseError::InvalidFirstByte(None)),
        }
    }

    fn parse_simple_string(&mut self) -> Result<ResponseValue, BufParseError> {
        let first_byte = self.peek().ok_or(BufParseError::InvalidFirstByte(None))?;

        expect_byte(first_byte, b'+')?;

        self.cursor += 1;

        let line = self.read_line()?;

        Ok(ResponseValue::SimpleString(line))
    }

    fn parse_simple_error(&mut self) -> Result<ResponseValue, BufParseError> {
        let first_byte = self.peek().ok_or(BufParseError::InvalidFirstByte(None))?;

        expect_byte(first_byte, b'-')?;

        self.cursor += 1;

        let line = self.read_line()?;

        Ok(ResponseValue::Error(line))
    }

    fn parse_integer(&mut self) -> Result<ResponseValue, BufParseError> {
        let first_byte = self.peek().ok_or(BufParseError::InvalidFirstByte(None))?;

        expect_byte(first_byte, b':')?;

        self.cursor += 1;

        let value = self.read_line()?.parse::<i64>()?;
        Ok(ResponseValue::Integer(value))
    }

    fn parse_bulk_string(&mut self) -> Result<ResponseValue, BufParseError> {
        let first_byte = self.peek().ok_or(BufParseError::InvalidFirstByte(None))?;

        expect_byte(first_byte, b'$')?;

        self.cursor += 1;
        let length = self.read_line()?.parse::<i64>()?;
        if length < 0 {
            return Ok(ResponseValue::BulkString(None));
        }

        let length = length as usize;
        if self.cursor + length > self.buffer.len() {
            return Err(BufParseError::InvalidLength);
        }
        let bytes = &self.buffer[self.cursor..self.cursor + length];

        self.cursor += length;
        if self.cursor >= self.buffer.len() {
            return Err(BufParseError::UnexpectedEOF { expected: "CR \\r" });
        }

        match self.peek() {
            Some(b'\r') => Ok(()),
            Some(other) => Err(BufParseError::UnexpectedByte {
                expected: b'\r',
                found: Some(other),
            }),
            None => Err(BufParseError::Incomplete),
        }?;

        self.cursor += 1;
        match self.peek() {
            Some(b'\n') => Ok(()),
            Some(other) => Err(BufParseError::UnexpectedByte {
                expected: b'\n',
                found: Some(other),
            }),
            None => Err(BufParseError::Incomplete),
        }?;

        self.cursor += 1;

        Ok(ResponseValue::BulkString(Some(bytes.to_vec())))
    }

    fn parse_array(&mut self) -> Result<ResponseValue, BufParseError> {
        let first_byte = self.peek().ok_or(BufParseError::InvalidFirstByte(None))?;

        expect_byte(first_byte, b'*')?;
        self.cursor += 1;

        let length = self.read_line()?.parse::<i64>()?;

        if length < 0 {
            return Ok(ResponseValue::Array(None));
        }

        let mut items = Vec::new();

        for _ in 0..length {
            let value = self.parse()?;
            items.push(value);
        }

        Ok(ResponseValue::Array(Some(items)))
    }
}

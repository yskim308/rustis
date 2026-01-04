use std::{num::ParseIntError, string::FromUtf8Error};

pub enum ReponseValue {
    SimpleString(String),
    Error(String),
    Integer(i64),
    BulkString(Option<Vec<u8>>),
    Array(Option<Vec<ReponseValue>>),
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
    buffer: Vec<u8>,
    cursor: usize,
}

fn expect_byte(found: u8, expected: u8) -> Result<(), BufParseError> {
    if (found != expected) {
        return Err(BufParseError::UnexpectedByte {
            expected,
            found: Some(found),
        });
    }

    Ok(())
}

impl Parser {
    pub fn new(buffer: Vec<u8>) -> Self {
        Self { buffer, cursor: 0 }
    }

    fn peek(&self) -> Option<u8> {
        if self.cursor >= self.buffer.len() {
            None
        } else {
            Some(self.buffer[self.cursor])
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

    pub fn parse(&mut self) -> Result<ReponseValue, BufParseError> {
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

    fn parse_simple_string(&mut self) -> Result<ReponseValue, BufParseError> {
        let first_byte = self.peek().ok_or(BufParseError::InvalidFirstByte(None))?;

        expect_byte(first_byte, b'+')?;

        self.cursor += 1;

        let line = self.read_line()?;

        Ok(ReponseValue::SimpleString(line))
    }

    fn parse_simple_error(&mut self) -> Result<ReponseValue, BufParseError> {
        let first_byte = self.peek().ok_or(BufParseError::InvalidFirstByte(None))?;

        expect_byte(first_byte, b'-')?;

        self.cursor += 1;

        let line = self.read_line()?;

        Ok(ReponseValue::Error(line))
    }

    fn parse_integer(&mut self) -> Result<ReponseValue, BufParseError> {
        let first_byte = self.peek().ok_or(BufParseError::InvalidFirstByte(None))?;

        expect_byte(first_byte, b':')?;

        self.cursor += 1;

        let value = self.read_line()?.parse::<i64>()?;
        Ok(ReponseValue::Integer(value))
    }

    fn parse_bulk_string(&mut self) -> Result<ReponseValue, BufParseError> {
        let first_byte = self.peek().ok_or(BufParseError::InvalidFirstByte(None))?;

        expect_byte(first_byte, b'$')?;

        self.cursor += 1;
        let length = self.read_line()?.parse::<i64>()?;
        if length < 0 {
            return Ok(ReponseValue::BulkString(None));
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

        Ok(ReponseValue::BulkString(Some(bytes.to_vec())))
    }

    fn parse_array(&mut self) -> Result<ReponseValue, BufParseError> {
        let first_byte = self.peek().ok_or(BufParseError::InvalidFirstByte(None))?;

        expect_byte(first_byte, b'*')?;
        self.cursor += 1;

        let length = self.read_line()?.parse::<i64>()?;

        if length < 0 {
            return Ok(ReponseValue::Array(None));
        }

        let mut items = Vec::new();

        for _ in 0..length {
            let value = self.parse()?;
            items.push(value);
        }

        Ok(ReponseValue::Array(Some(items)))
    }
}

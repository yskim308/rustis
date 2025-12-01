use std::{num::ParseIntError, string::FromUtf8Error};

pub enum ReponseValue {
    SimpleString(String),
    Error(String),
    Integer(i64),
    BulkString(Option<Vec<u8>>),
    Array(Option<Vec<ReponseValue>>),
}

pub enum BufParseError {
    EofError(&'static str),
    ByteConversionError(FromUtf8Error),
    FirstByteError(u8),
    InvalidFirstByte(),
    StringConversionError(ParseIntError),
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

impl Parser {
    fn peek(&self) -> u8 {
        self.buffer[self.cursor]
    }

    fn read_line(&mut self) -> Result<String, BufParseError> {
        while self.cursor < self.buffer.len() && self.peek() != b'\r' {
            self.cursor += 1;
        }

        if self.cursor == self.buffer.len() {
            return Err(BufParseError::EofError("Expected \\r at EOF"));
        }

        let bytes = &self.buffer[..self.cursor];
        let output = String::from_utf8(bytes.to_vec())?;

        self.cursor += 1;
        if self.cursor == self.buffer.len() || self.peek() != b'\n' {
            return Err(BufParseError::EofError("expected \\n after \\r"));
        }

        self.cursor += 1;

        Ok(output)
    }

    // todo: robust handling of TCP fragmentation (if we dont find /r/n)
    fn parse(&mut self) -> Result<ReponseValue, BufParseError> {
        match self.peek() {
            b'+' => self.parse_simple_string(),
            b'-' => self.parse_simple_error(),
            // b':' => self.parse_integer(),
            // b'$' => self.parse_bulk_string(),
            // b'*' => self.parse_array(),
            _ => Err(BufParseError::InvalidFirstByte()),
        }
    }

    fn parse_simple_string(&mut self) -> Result<ReponseValue, BufParseError> {
        let first_byte = self.peek();

        if first_byte != b'+' {
            return Err(BufParseError::FirstByteError(first_byte));
        }

        let line = self.read_line()?;

        Ok(ReponseValue::SimpleString(line))
    }

    fn parse_simple_error(&mut self) -> Result<ReponseValue, BufParseError> {
        let first_byte = self.peek();

        if first_byte != b'-' {
            return Err(BufParseError::FirstByteError(first_byte));
        }

        let line = self.read_line()?;

        Ok(ReponseValue::Error(line))
    }

    fn parse_integer(&mut self) -> Result<ReponseValue, BufParseError> {
        let first_byte = self.peek();

        if first_byte != b':' {
            return Err(BufParseError::FirstByteError(first_byte));
        }

        let value = self.read_line()?.parse::<i64>()?;
        Ok(ReponseValue::Integer(value))
    }

    fn parse_bulk_string(&mut self) {}

    fn parse_array(&mut self) {}
}

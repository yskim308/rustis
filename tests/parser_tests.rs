use rustis::parser::{BufParseError, Parser, ReponseValue};

// Helper to reduce boilerplate
fn parse_buffer(input: &[u8]) -> Result<ReponseValue, BufParseError> {
    let input_to_vec = input.to_vec();
    let mut parser = Parser::new(&input_to_vec);
    parser.parse()
}

// =========================================================================
// 1. SIMPLE STRING (+)
// =========================================================================

#[test]
fn test_simple_string_happy_path() {
    let input = b"+OK\r\n";
    let result = parse_buffer(input).unwrap();

    match result {
        ReponseValue::SimpleString(s) => assert_eq!(s, "OK"),
        _ => panic!("Expected SimpleString"),
    }
}

#[test]
fn test_simple_string_eof_error() {
    // Missing \n
    let input = b"+OK\r";
    let result = parse_buffer(input);

    assert!(matches!(result, Err(BufParseError::Incomplete)));
}

#[test]
fn test_simple_string_empty() {
    // Valid empty string
    let input = b"+\r\n";
    let result = parse_buffer(input).unwrap();

    match result {
        ReponseValue::SimpleString(s) => assert_eq!(s, ""),
        _ => panic!("Expected Empty SimpleString"),
    }
}

// =========================================================================
// 2. ERROR (-)
// =========================================================================

#[test]
fn test_error_happy_path() {
    let input = b"-ERR unknown command\r\n";
    let result = parse_buffer(input).unwrap();

    match result {
        ReponseValue::Error(s) => assert_eq!(s, "ERR unknown command"),
        _ => panic!("Expected Error"),
    }
}

#[test]
fn test_error_missing_crlf() {
    let input = b"-ERR";
    let result = parse_buffer(input);

    assert!(matches!(result, Err(BufParseError::Incomplete)));
}

// =========================================================================
// 3. INTEGER (:)
// =========================================================================

#[test]
fn test_integer_happy_path() {
    let input = b":1000\r\n";
    let result = parse_buffer(input).unwrap();

    match result {
        ReponseValue::Integer(i) => assert_eq!(i, 1000),
        _ => panic!("Expected Integer"),
    }
}

#[test]
fn test_integer_negative() {
    let input = b":-42\r\n";
    let result = parse_buffer(input).unwrap();

    match result {
        ReponseValue::Integer(i) => assert_eq!(i, -42),
        _ => panic!("Expected Negative Integer"),
    }
}

#[test]
fn test_integer_parse_error() {
    // Non-numeric characters
    let input = b":abc\r\n";
    let result = parse_buffer(input);

    assert!(matches!(
        result,
        Err(BufParseError::StringConversionError(_))
    ));
}

// =========================================================================
// 4. BULK STRING ($)
// =========================================================================

#[test]
fn test_bulk_string_happy_path() {
    // $5\r\nhello\r\n
    let input = b"$5\r\nhello\r\n";
    let result = parse_buffer(input).unwrap();

    match result {
        ReponseValue::BulkString(Some(bytes)) => assert_eq!(bytes, b"hello"),
        _ => panic!("Expected BulkString"),
    }
}

#[test]
fn test_bulk_string_null() {
    // $-1\r\n
    let input = b"$-1\r\n";
    let result = parse_buffer(input).unwrap();

    match result {
        ReponseValue::BulkString(None) => {} // Pass
        _ => panic!("Expected Null BulkString"),
    }
}

#[test]
fn test_bulk_string_eof_in_payload() {
    // Claim 5 bytes, only provide 3
    let input = b"$5\r\nhel";
    let result = parse_buffer(input);

    // Note: Your specific implementation might return UnexpectedEOF or index out of bounds depending on how you handled the slice
    assert!(result.is_err());
}

#[test]
fn test_bulk_string_missing_terminator() {
    // Missing the final \r\n
    // Note: Your current implementation has a bug here (it skips the \r check).
    // This test expects standard RESP behavior ($5\r\nhello\r\n).
    let input = b"$5\r\nhello";
    let result = parse_buffer(input);

    assert!(result.is_err());
}

// =========================================================================
// 5. ARRAY (*)
// =========================================================================

#[test]
fn test_array_happy_path() {
    // *2\r\n$3\r\nfoo\r\n$3\r\nbar\r\n
    let input = b"*2\r\n$3\r\nfoo\r\n$3\r\nbar\r\n";
    let result = parse_buffer(input).unwrap();

    match result {
        ReponseValue::Array(Some(items)) => {
            assert_eq!(items.len(), 2);
            match &items[0] {
                ReponseValue::BulkString(Some(b)) => assert_eq!(b, b"foo"),
                _ => panic!("Item 0 should be BulkString"),
            }
            match &items[1] {
                ReponseValue::BulkString(Some(b)) => assert_eq!(b, b"bar"),
                _ => panic!("Item 1 should be BulkString"),
            }
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_array_nested() {
    // Array containing an integer and a simple string
    // *2\r\n:1\r\n+OK\r\n
    let input = b"*2\r\n:1\r\n+OK\r\n";
    let result = parse_buffer(input).unwrap();

    if let ReponseValue::Array(Some(items)) = result {
        assert!(matches!(items[0], ReponseValue::Integer(1)));
        assert!(matches!(items[1], ReponseValue::SimpleString(_)));
    } else {
        panic!("Expected Mixed Array");
    }
}

#[test]
fn test_array_empty() {
    // *0\r\n
    let input = b"*0\r\n";
    let result = parse_buffer(input).unwrap();

    match result {
        ReponseValue::Array(Some(items)) => assert!(items.is_empty()),
        _ => panic!("Expected Empty Array"),
    }
}

#[test]
fn test_array_incomplete() {
    // Claims 2 items, only has 1
    let input = b"*2\r\n:1\r\n";
    let result = parse_buffer(input);

    assert!(matches!(result, Err(BufParseError::InvalidFirstByte(None))));
    // Or UnexpectedEOF, depending on where your loop hits the end
}

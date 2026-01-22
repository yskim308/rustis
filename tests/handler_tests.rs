#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use rustis::handler::CommandHandler;
    use rustis::kv::KvStore;
    use rustis::parser::ResponseValue;

    // Helper to construct a command request (Array of BulkStrings)
    fn make_cmd(args: Vec<&str>) -> ResponseValue {
        let items = args
            .into_iter()
            .map(|s| ResponseValue::BulkString(Some(Bytes::copy_from_slice(s.as_bytes()))))
            .collect();
        ResponseValue::Array(Some(items))
    }

    // Helper to extract string from ResponseValue for assertions
    fn extract_str(val: ResponseValue) -> String {
        match val {
            ResponseValue::SimpleString(s) => s,
            ResponseValue::BulkString(Some(b)) => String::from_utf8(b.to_vec()).unwrap(),
            ResponseValue::Error(s) => s,
            _ => panic!("Unexpected type for extraction: {:?}", val),
        }
    }

    fn setup() -> CommandHandler {
        let kv = KvStore::new();
        CommandHandler::new(kv)
    }

    #[test]
    fn test_ping() {
        let handler = setup();
        let res = handler.process_command(make_cmd(vec!["PING"]));
        assert_eq!(res, ResponseValue::SimpleString("PONG".to_string()));
    }

    #[test]
    fn test_set_get() {
        let handler = setup();

        // SET key value
        let res = handler.process_command(make_cmd(vec!["SET", "mykey", "hello"]));
        assert_eq!(res, ResponseValue::SimpleString("OK".to_string()));

        // GET key
        let res = handler.process_command(make_cmd(vec!["GET", "mykey"]));
        assert_eq!(extract_str(res), "hello");

        // GET missing
        let res = handler.process_command(make_cmd(vec!["GET", "missing"]));
        assert_eq!(res, ResponseValue::BulkString(None));
    }

    #[test]
    fn test_list_integration() {
        let handler = setup();

        // LPUSH list a
        let res = handler.process_command(make_cmd(vec!["LPUSH", "mylist", "a"]));
        assert_eq!(res, ResponseValue::Integer(1));

        // RPUSH list b
        let res = handler.process_command(make_cmd(vec!["RPUSH", "mylist", "b"]));
        assert_eq!(res, ResponseValue::Integer(2));

        // LRANGE list 0 -1 (expect ["a", "b"])
        let res = handler.process_command(make_cmd(vec!["LRANGE", "mylist", "0", "-1"]));
        if let ResponseValue::Array(Some(items)) = res {
            assert_eq!(items.len(), 2);
            assert_eq!(extract_str(items[0].clone()), "a");
            assert_eq!(extract_str(items[1].clone()), "b");
        } else {
            panic!("Expected Array response for LRANGE");
        }

        // LPOP list (default count 1, returns BulkString("a"))
        let res = handler.process_command(make_cmd(vec!["LPOP", "mylist"]));
        assert_eq!(extract_str(res), "a");
    }

    #[test]
    fn test_set_integration() {
        let handler = setup();

        // SADD set val
        let res = handler.process_command(make_cmd(vec!["SADD", "myset", "val"]));
        assert_eq!(res, ResponseValue::Integer(1));

        // SMEMBERS set
        let res = handler.process_command(make_cmd(vec!["SMEMBERS", "myset"]));
        if let ResponseValue::Array(Some(items)) = res {
            assert_eq!(items.len(), 1);
            assert_eq!(extract_str(items[0].clone()), "val");
        } else {
            panic!("Expected Array response for SMEMBERS");
        }

        // SPOP set (returns Array because logic might vary, but handle_spop returns Array for consistency if >1,
        // though your specific implementation wraps it in Array regardless for single item?)
        // Checking your implementation: handle_spop maps everything to Array regardless of count.
        let res = handler.process_command(make_cmd(vec!["SPOP", "myset"]));
        if let ResponseValue::Array(Some(items)) = res {
            assert_eq!(items.len(), 1);
            assert_eq!(extract_str(items[0].clone()), "val");
        } else {
            panic!("Expected Array response for SPOP");
        }
    }

    #[test]
    fn test_invalid_command() {
        let handler = setup();
        let res = handler.process_command(make_cmd(vec!["FOOBAR"]));
        assert!(matches!(res, ResponseValue::Error(_)));
    }

    #[test]
    fn test_argument_validation() {
        let handler = setup();
        // SET without value
        let res = handler.process_command(make_cmd(vec!["SET", "key"]));
        assert!(extract_str(res).contains("wrong number of arguments"));
    }
}

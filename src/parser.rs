pub struct Parser {
    buffer: Vec<u8>,
    cursor: usize,
}

impl Parser {
    fn peek(&self) -> u8 {
        self.buffer[self.cursor]
    }

    // tood: make anenum for value, return that enum in parse
    // todo: proper error handling, return result on parse
    fn parse(&mut self) {
        match self.peek() {
            b'+' => self.parse_simple_string(),
            b'-' => self.parse_simple_error(),
            b':' => self.parse_integer(),
            b'$' => self.parse_bulk_string(),
            _ => panic!("failed parsing"),
        }
    }

    fn parse_simple_string(&mut self) {}

    fn parse_simple_error(&mut self) {}

    fn parse_integer(&mut self) {}

    fn parse_bulk_string(&mut self) {}
}

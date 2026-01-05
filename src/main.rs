use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
};

use rustis::parser::{BufParseError, Parser};

fn main() {
    let port = 4000;
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(addr).unwrap();

    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                eprintln!("failed to establish connection, {}", e);
                continue;
            }
        };
        handle_connection(stream);
    }
    println!("binding to port {port}")
}

fn handle_connection(mut stream: TcpStream) {
    let mut buffer = [0; 512];
    let mut accumulated: Vec<u8> = Vec::new();
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => {
                break;
            }
            Ok(bytes_read) => {
                // todo: not sure if we should be creating a new parser instance per read
                accumulated.extend_from_slice(&buffer[..bytes_read]);
                let mut parser = Parser::new(&accumulated);

                match parser.parse() {
                    Ok(value) => {
                        println!("{value:?}");
                        accumulated.clear();
                    }
                    Err(BufParseError::Incomplete) => {
                        continue;
                    }
                    Err(e) => {
                        eprint!("parse error: {:?}", e);
                        break;
                    }
                }
            }
            Err(e) => {
                eprintln!("failed to read buffer, {}", e);
                break;
            }
        }
    }
}

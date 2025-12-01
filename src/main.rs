use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
};

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
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => {
                break;
            }
            Ok(bytes) => {
                if let Err(e) = stream.write_all(&buffer[..bytes]) {
                    eprintln!("failed to write to stream: {}", e);
                    break;
                }
            }
            Err(e) => {
                eprintln!("failed to read buffer, {}", e);
                break;
            }
        }
    }
}

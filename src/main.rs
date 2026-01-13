use std::env;
use std::sync::Arc;

use rustis::handler::CommandHandler;
use rustis::kv::KvStore;
use rustis::parser::{BufParseError, Parser};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main(flavor = "current_thread")]
async fn main() -> tokio::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let port = args
        .get(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(6379);
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    println!("Listening on port {port}");

    loop {
        let (stream, _) = listener.accept().await?;

        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream).await {
                eprintln!("Error handling connection: {:?}", e);
            }
        });
    }
}

async fn handle_connection(mut stream: TcpStream) -> tokio::io::Result<()> {
    let mut buffer = [0; 512];
    let mut accumulated: Vec<u8> = Vec::new();

    loop {
        let bytes_read = stream.read(&mut buffer).await?;

        if bytes_read == 0 {
            return Ok(()); // Connection closed
        }

        accumulated.extend_from_slice(&buffer[..bytes_read]);

        // Note: Creating a new parser per loop is fine for now, but we'd want a state-ful parser
        // later
        let mut parser = Parser::new(&accumulated);
        let kv = Arc::new(KvStore::new());
        let handler = CommandHandler::new(kv);

        match parser.parse() {
            Ok(value) => {
                println!("Parsed: {:?}", value);
                let response = handler.process_command(value);
                // Echo back the data
                stream.write_all(&response.serialize()).await?;
                accumulated.clear();
            }
            Err(BufParseError::Incomplete) => {
                // Not enough data yet, continue reading from the socket
                continue;
            }
            Err(e) => {
                eprintln!("Parse error: {:?}", e);
                return Ok(());
            }
        }
    }
}

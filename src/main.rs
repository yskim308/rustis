use rustis::parser::{BufParseError, Parser};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let port = 4000;
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

        match parser.parse() {
            Ok(value) => {
                println!("Parsed: {:?}", value);
                // Echo back the data
                stream.write_all(&accumulated).await?;
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

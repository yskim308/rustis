use socket2::{Domain, Socket, Type};
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::runtime::Builder;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task;

use bytes::{Bytes, BytesMut};
use rustis::handler::CommandHandler;
use rustis::kv::KvStore;
use rustis::message::ShardRequest;
use rustis::parser::{parse, BufParseError};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn main() {
    // get port from args
    let args: Vec<String> = env::args().collect();
    let port = args
        .get(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(6379);
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();

    // create channel matrix (bus) for N cores
    let core_ids = core_affinity::get_core_ids().unwrap();
    let num_cores = core_ids.len();

    let mut txs: Vec<UnboundedSender<ShardRequest>> = Vec::with_capacity(num_cores);
    let mut rxs: Vec<UnboundedReceiver<ShardRequest>> = Vec::with_capacity(num_cores);

    for i in 0..num_cores {
        let (tx, rx) = mpsc::unbounded_channel::<ShardRequest>();
        txs[i] = tx;
        rxs[i] = rx;
    }

    let router = Arc::new(txs); // every thread needs access to each transmitte

    // spawn cores
    let mut handles = Vec::new();

    for (i, core_id) in core_ids.into_iter().enumerate() {
        let core_router = router.clone();

        // n^2 but should be fine, law of small numbers
        let mut mailbox = rxs.remove(0);

        let handle = std::thread::spawn(move || {
            // pin the thread to the core
            if !core_affinity::set_for_current(core_id) {
                eprint!("Failed to pin thread to core: {:?}", core_id);
                return;
            }

            // set up socket for reuse
            let socket =
                Socket::new(Domain::IPV4, Type::STREAM, None).expect("Error while creating socket");
            socket
                .set_reuse_port(true)
                .expect("socket reuse not supported");
            socket
                .set_nonblocking(true)
                .expect("failed to set nonblocking");

            socket
                .bind(&addr.into())
                .expect("failed to bind to address");
            socket.listen(1024).expect("failed to listen");

            // convert to standard listener
            let std_listener: std::net::TcpListener = socket.into();

            // create single thread runtime for this specific core
            let runtime = Builder::new_current_thread().enable_all().build().unwrap();

            runtime.block_on(per_thread());
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }
}

async fn per_thread() -> tokio::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let port = args
        .get(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(6379);
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    println!("Listening on port {port}");

    let kv = KvStore::new();

    let local = task::LocalSet::new();

    local
        .run_until(async move {
            loop {
                // todo: handle unwrap
                let (stream, _) = listener.accept().await.unwrap();

                let kv_clone = kv.clone();
                tokio::task::spawn_local(async move {
                    if let Err(e) = handle_connection(stream, kv_clone).await {
                        match e.kind() {
                            std::io::ErrorKind::ConnectionReset => {}
                            _ => eprintln!("Error handling connection: {:?}", e),
                        }
                    }
                });
            }
        })
        .await;
    Ok(())
}

async fn handle_connection(stream: TcpStream, kv: KvStore) -> tokio::io::Result<()> {
    stream.set_nodelay(true)?;

    let (read_half, write_half) = stream.into_split();

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

    tokio::task::spawn_local(async move { writer_task(write_half, rx).await });

    reader_task(read_half, kv, tx).await?;

    Ok(())
}

async fn writer_task(
    mut write_half: OwnedWriteHalf,
    mut rx: UnboundedReceiver<Bytes>,
) -> tokio::io::Result<()> {
    while let Some(data) = rx.recv().await {
        write_half.write_all(&data).await?;
    }
    Ok(())
}

async fn reader_task(
    mut read_half: OwnedReadHalf,
    kv: KvStore,
    tx: UnboundedSender<Bytes>,
) -> tokio::io::Result<()> {
    let mut read_buffer = BytesMut::with_capacity(64 * 1024);
    let mut write_buffer = BytesMut::with_capacity(64 * 1024);
    let handler = CommandHandler::new(kv);

    // repeat until nothing to read
    loop {
        read_buffer.reserve(1024);
        if read_half.read_buf(&mut read_buffer).await? == 0 {
            break; //
        }

        loop {
            match parse(&mut read_buffer) {
                Ok(value) => {
                    handler.process_command(value).serialize(&mut write_buffer);
                }
                Err(BufParseError::Incomplete) => {
                    break;
                }
                Err(BufParseError::InvalidFirstByte(b)) => {
                    match b {
                        Some(byte) => {
                            let s = format!("-ERR invalid first byte: {}\r\n", byte);
                            write_buffer.extend_from_slice(s.as_bytes());
                        }
                        None => write_buffer.extend_from_slice(b"-ERR first byte not found \r\n"),
                    };
                    let _ = tx.send(write_buffer.freeze());
                    return Ok(()); // Close connection on protocol error
                }
                _ => {
                    write_buffer.extend_from_slice(b"-ERR internal server error\r\n");
                    let _ = tx.send(write_buffer.freeze());
                    return Ok(()); // Close connection on error
                }
            }
        }

        // flush after all possible frames are handled
        if tx.send(write_buffer.split().freeze()).is_err() {
            eprint!("error in reader task send");
            return Ok(()); // kill task if error
        };
    }

    Ok(())
}

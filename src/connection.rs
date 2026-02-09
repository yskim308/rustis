use std::{
    cell::RefCell,
    env,
    net::{self},
    rc::Rc,
    sync::Arc,
};

use bytes::BytesMut;
use rtrb::{Consumer, Producer};
use slab::Slab;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task,
};

use crate::{
    message::{ResponseMessage, ResponseValue, WorkerMessage},
    parser::{parse, BufParseError},
    router::route_message,
};

struct ConnectionState {
    writer: OwnedWriteHalf,
    pending_responses: Vec<Option<ResponseValue>>,
    next_seq: u64,
    write_buffer: BytesMut,
}

const WINDOW_SIZE: usize = 1024;

impl ConnectionState {
    fn new(writer: OwnedWriteHalf) -> Self {
        let mut buffer = Vec::with_capacity(WINDOW_SIZE);
        for _ in 0..WINDOW_SIZE {
            buffer.push(None);
        }

        Self {
            writer,
            pending_responses: buffer,
            next_seq: 0,
            write_buffer: BytesMut::with_capacity(32 * 1024),
        }
    }

    async fn handle_response(&mut self, seq: u64, value: ResponseValue) {
        let index = seq as usize & (WINDOW_SIZE - 1);

        // try to insert
        if self.pending_responses[index].is_some() {
            panic!("Pipeline depth exceeded in connection buffer!");
        }
        self.pending_responses[index] = Some(value);

        // loop through and write to write buffer
        loop {
            let to_drain = self.next_seq as usize & (WINDOW_SIZE - 1);

            match self.pending_responses[to_drain].take() {
                Some(value) => {
                    value.serialize(&mut self.write_buffer);
                    self.next_seq += 1;
                }
                None => break,
            }
        }

        if !self.write_buffer.is_empty() {
            if let Err(e) = self.writer.write_all(&self.write_buffer).await {
                eprintln!("Write error: {}", e);
            }

            self.write_buffer.clear();
        }
    }
}

type ConnectionStore = Rc<RefCell<Slab<ConnectionState>>>;
type ShardedRouter = Rc<RefCell<Vec<Producer<WorkerMessage>>>>;

pub async fn spawn_io(
    req_outbox: ShardedRouter,
    resp_inbox: Vec<Consumer<ResponseMessage>>,
) -> tokio::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let port = args
        .get(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(6379);
    let addr = format!("127.0.0.1:{}", port);
    let std_addr: net::SocketAddr = addr.parse().unwrap();
    let socket2_addr: socket2::SockAddr = std_addr.into();

    // set up socket (note: reuse port only works on unix machines)
    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
        .expect("failed to create socket2 socket");
    socket
        .set_reuse_address(true)
        .expect("failed to set reuse socket2 address");
    socket
        .set_reuse_port(true)
        .expect("failed to set reuse port");
    socket
        .bind(&socket2_addr)
        .expect("failed to bind to socket2 address");
    socket.listen(1024);

    let std_listener: net::TcpListener = socket.into();
    std_listener.set_nonblocking(true).unwrap();
    let listener = tokio::net::TcpListener::from_std(std_listener)
        .expect("failed to create async listener from std listener");

    println!("Listening on port {port}");

    // create the registry of connections for this thread
    let connections = Rc::new(RefCell::new(Slab::<ConnectionState>::with_capacity(1024)));

    let local = task::LocalSet::new();
    Ok(())
}

async fn handle_connection(stream: TcpStream, router: &ShardedRouter) -> tokio::io::Result<()> {
    stream.set_nodelay(true)?;

    let (read_half, write_half) = stream.into_split();

    let (writer_tx, writer_rx) = tokio::sync::mpsc::unbounded_channel();

    tokio::task::spawn_local(async move { writer_task(write_half, writer_rx).await });

    reader_task(read_half, tx, router).await?;

    Ok(())
}

async fn poll_inboxes(
    connections: ConnectionStore,
    mut inboxes: Vec<Consumer<ResponseMessage>>,
) -> tokio::io::Result<()> {
    loop {
        let mut progress = false;
        for inbox in inboxes.iter_mut() {
            while let Ok(msg) = inbox.pop() {
                progress = true;
                let mut store = connections.borrow_mut();
                // 1. Lookup the connection by Token
                if let Some(conn) = store.get_mut(msg.conn_token) {
                    // 2. Buffer the response (to handle out-of-order return)
                    conn.pending_responses.insert(msg.seq, msg.response_value);
                    // 3. Write strictly in order
                    while let Some(val) = conn.pending_responses.remove(&conn.next_seq) {
                        // Serialize (Zero-copy optimization: do this before loop)
                        let bytes = val.serialize();

                        // Write to socket
                        // Note: try_write is better here to avoid await in loop
                        // but await is safe because we are the only one writing.
                        if let Err(_) = conn.writer.write_all(&bytes).await {
                            // Connection died, remove from slab?
                            // (Handled by the reader usually, or lazy cleanup)
                        }

                        conn.next_seq += 1;
                    }
                }
            }
        }

        if !progress {
            // Yield to let the Reader Task run
            tokio::task::yield_now().await;
        }
    }
}

async fn reader_task(
    mut read_half: OwnedReadHalf,
    tx: UnboundedSender<ResponseMessage>,
    router: &[UnboundedSender<WorkerMessage>],
) -> tokio::io::Result<()> {
    let mut read_buffer = BytesMut::with_capacity(64 * 1024);

    let mut seq: u64 = 0;
    loop {
        read_buffer.reserve(1024);
        if read_half.read_buf(&mut read_buffer).await? == 0 {
            break; //
        }

        loop {
            match parse(&mut read_buffer) {
                Ok(value) => {
                    seq += 1;
                    let tx_clone = tx.clone();
                    route_message(router, value, seq, tx_clone);
                }
                Err(BufParseError::Incomplete) => {
                    break;
                }
                Err(BufParseError::InvalidFirstByte(b)) => {
                    match b {
                        Some(byte) => {
                            let s = format!("-ERR invalid first byte: {}", byte);
                            let _ = tx.send(ResponseMessage {
                                seq,
                                response_value: ResponseValue::Error(s.into()),
                            });
                        }
                        None => {
                            let _ = tx.send(ResponseMessage {
                                seq,
                                response_value: ResponseValue::Error(
                                    "ERR first byte not found".into(),
                                ),
                            });
                        }
                    };
                    return Ok(()); // Close connection on protocol error
                }
                _ => {
                    let _ = tx.send(ResponseMessage {
                        seq,
                        response_value: ResponseValue::Error("ERR internal server error".into()),
                    });
                    return Ok(()); // Close connection on error
                }
            }
        }
    }

    Ok(())
}

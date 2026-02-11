use std::{
    cell::RefCell,
    env,
    net::{self},
    rc::Rc,
};

use bytes::{Buf, BytesMut};
use rtrb::{Consumer, Producer};
use slab::Slab;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::{
    io::AsyncReadExt,
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
};

use crate::{
    message::{ResponseMessage, ResponseValue, WorkerMessage},
    parser::{parse, BufParseError},
    router::MessageRouter,
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

    fn enqueue_response(&mut self, seq: u64, value: ResponseValue) {
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
    }
}

type ConnectionStore = Rc<RefCell<Slab<ConnectionState>>>;
pub type WorkerQueues = Rc<RefCell<Vec<Producer<WorkerMessage>>>>;

pub async fn spawn_io(
    core_id: usize,
    worker_queues: WorkerQueues,
    io_queues: Vec<Consumer<ResponseMessage>>,
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
    socket
        .listen(1024)
        .expect("failed to listen and set 1024 backlog");

    let std_listener: net::TcpListener = socket.into();
    std_listener.set_nonblocking(true).unwrap();
    let listener = tokio::net::TcpListener::from_std(std_listener)
        .expect("failed to create async listener from std listener");

    println!("Listening on port {port}");

    // create the registry of connections for this thread
    let connections = Rc::new(RefCell::new(Slab::<ConnectionState>::with_capacity(1024)));

    // spawn the inbox polling task
    let poller_connections = connections.clone();
    tokio::task::spawn_local(async move {
        poll_inboxes(poller_connections, io_queues).await.unwrap();
    });

    // connection accepting loop
    loop {
        let (stream, _) = listener.accept().await?;

        let (read_half, write_half) = stream.into_split();

        let mut reader_connections = connections.borrow_mut();

        let token = reader_connections.insert(ConnectionState::new(write_half));
        let cloned_worker_queues = worker_queues.clone();
        // pass in token to reader task
        tokio::task::spawn_local(async move {
            reader_task(read_half, cloned_worker_queues, token, core_id)
                .await
                .unwrap();
        });
    }
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
                handle_message(msg, &connections);
            }
        }

        if !progress {
            // Yield to let the Reader Task run
            tokio::task::yield_now().await;
        }
    }
}

fn handle_message(msg: ResponseMessage, connections: &ConnectionStore) {
    let mut connections = connections.borrow_mut();
    // 1. Lookup the connection by Token
    if let Some(conn_state) = connections.get_mut(msg.conn_token) {
        conn_state.enqueue_response(msg.seq, msg.response_value);
        if !conn_state.write_buffer.is_empty() {
            write_to_buffer(conn_state);
        }
    }
}

fn write_to_buffer(conn_state: &mut ConnectionState) {
    match conn_state.writer.try_write(&conn_state.write_buffer) {
        Ok(n) => {
            conn_state.write_buffer.advance(n);
        }
        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
        Err(e) => {
            eprintln!("Connection died: {}", e);
        }
    }
}

async fn reader_task(
    mut read_half: OwnedReadHalf,
    worker_queues: WorkerQueues,
    conn_token: usize,
    core_id: usize,
) -> tokio::io::Result<()> {
    let mut read_buffer = BytesMut::with_capacity(64 * 1024);

    // each connection gets their own stateful router
    let router = MessageRouter::new(worker_queues, conn_token, core_id);

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
                    router.route_message(value, seq);
                }
                Err(BufParseError::Incomplete) => {
                    break;
                }
                Err(BufParseError::InvalidFirstByte(b)) => {
                    match b {
                        Some(byte) => {
                            let s = format!("ERR invalid first byte: {}", byte);
                            router.route_message(ResponseValue::Error(s.into()), seq);
                        }
                        None => router.route_message(
                            ResponseValue::Error("ERR first byte not found".into()),
                            seq,
                        ),
                    };
                    return Ok(()); // Close connection on protocol error
                }
                _ => {
                    router.route_message(
                        ResponseValue::Error("ERR internal server error".into()),
                        seq,
                    );
                    return Ok(()); // Close connection on error
                }
            }
        }
    }

    Ok(())
}

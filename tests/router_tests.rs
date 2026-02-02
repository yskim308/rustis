use bytes::Bytes;
use tokio::sync::mpsc;
// Replace 'your_crate_name' with the actual name of your crate
use rustis::message::{ResponseValue, WorkerMessage};
use rustis::route_message;

/// Helper to setup a mock environment
fn setup(
    worker_count: usize,
) -> (
    Vec<mpsc::UnboundedReceiver<WorkerMessage>>,
    mpsc::UnboundedSender<mpsc::UnboundedReceiver<Bytes>>, // Not used, but for structure
    mpsc::UnboundedSender<Bytes>,
    mpsc::UnboundedReceiver<Bytes>,
) {
    let mut router_tx = Vec::new();
    let mut router_rx = Vec::new();
    for _ in 0..worker_count {
        let (tx, rx) = mpsc::unbounded_channel();
        router_tx.push(tx);
        router_rx.push(rx);
    }
    let (writer_tx, writer_rx) = mpsc::unbounded_channel();

    // We return the rx ends to inspect them, and router_tx is passed into the function
    (router_rx, mpsc::unbounded_channel().0, writer_tx, writer_rx)
}

#[tokio::test]
async fn test_happy_path_routing() {
    let worker_count = 4;
    let (mut workers, _, writer_tx, mut writer_rx) = setup(worker_count);

    // We need to collect the senders for the function call
    // In a real scenario, you'd have these held in a struct
    let (tx_list, _) = (0..worker_count)
        .map(|_| mpsc::unbounded_channel::<WorkerMessage>())
        .unzip::<_, _, Vec<_>, Vec<_>>();

    // Re-creating the router slice for the test
    let router_senders: Vec<mpsc::UnboundedSender<WorkerMessage>> = (0..worker_count)
        .map(|_| mpsc::unbounded_channel().0)
        .collect();
    // Wait, let's use the actual receivers we created in setup
    let mut worker_rxs = Vec::new();
    let mut worker_txs = Vec::new();
    for _ in 0..worker_count {
        let (tx, rx) = mpsc::unbounded_channel();
        worker_txs.push(tx);
        worker_rxs.push(rx);
    }

    let frame = ResponseValue::Array(Some(vec![
        ResponseValue::BulkString(Some(Bytes::from("GET"))),
        ResponseValue::BulkString(Some(Bytes::from("user_123"))),
    ]));

    // Execute
    route_message(&worker_txs, frame.clone(), 42, &writer_tx);

    // 1. Ensure NO error was sent to the writer
    assert!(writer_rx.try_recv().is_err());

    // 2. Ensure exactly ONE worker received the message
    let mut found = false;
    for mut rx in worker_rxs {
        if let Ok(msg) = rx.try_recv() {
            assert_eq!(msg.seq, 42);
            assert_eq!(msg.response_value, frame);
            found = true;
            break;
        }
    }
    assert!(found, "Message was not routed to any worker");
}

#[tokio::test]
async fn test_ping_pong_intercept() {
    let (worker_txs, _) = (mpsc::unbounded_channel::<WorkerMessage>().0, ());
    let (writer_tx, mut writer_rx) = mpsc::unbounded_channel();

    let frame = ResponseValue::Array(Some(vec![ResponseValue::BulkString(Some(Bytes::from(
        "PING",
    )))]));

    route_message(&vec![], frame, 1, &writer_tx);

    let response = writer_rx.try_recv().expect("Should receive PONG response");
    // Depending on your serializer, check for "+PONG\r\n" or similar
    assert!(response.windows(4).any(|w| w == b"PONG"));
}

#[tokio::test]
async fn test_invalid_frame_type() {
    let (writer_tx, mut writer_rx) = mpsc::unbounded_channel();

    // Sending a SimpleString where an Array is expected
    let frame = ResponseValue::SimpleString("I am not an array".into());

    route_message(&vec![], frame, 1, &writer_tx);

    let response = writer_rx.try_recv().expect("Should receive error response");
    // Check if it's an error message (usually starts with '-' in RESP)
    assert!(response.starts_with(b"-") || response.contains(&b"Value must be array"[..]));
}

#[tokio::test]
async fn test_missing_key_error() {
    let (writer_tx, mut writer_rx) = mpsc::unbounded_channel();

    // Command with no key: ["GET"]
    let frame = ResponseValue::Array(Some(vec![ResponseValue::BulkString(Some(Bytes::from(
        "GET",
    )))]));

    route_message(&vec![], frame, 1, &writer_tx);

    let response = writer_rx.try_recv().expect("Should receive parsing error");
    assert!(response.contains(&b"error while parsing key"[..]));
}

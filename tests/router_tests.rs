use bytes::Bytes;
use rustis::message::{ResponseMessage, ResponseValue, WorkerMessage};
use rustis::route_message;
use tokio::sync::mpsc;

/// Helper to setup a mock environment
fn setup(
    worker_count: usize,
) -> (
    Vec<mpsc::UnboundedSender<WorkerMessage>>,
    Vec<mpsc::UnboundedReceiver<WorkerMessage>>,
    mpsc::UnboundedSender<ResponseMessage>,
    mpsc::UnboundedReceiver<ResponseMessage>,
) {
    let mut worker_txs = Vec::new();
    let mut worker_rxs = Vec::new();

    for _ in 0..worker_count {
        let (tx, rx) = mpsc::unbounded_channel();
        worker_txs.push(tx);
        worker_rxs.push(rx);
    }

    let (writer_tx, writer_rx) = mpsc::unbounded_channel();

    (worker_txs, worker_rxs, writer_tx, writer_rx)
}

#[tokio::test]
async fn test_happy_path_routing() {
    let worker_count = 4;
    let (worker_txs, mut worker_rxs, writer_tx, mut writer_rx) = setup(worker_count);

    let frame = ResponseValue::Array(Some(vec![
        ResponseValue::BulkString(Some(Bytes::from("GET"))),
        ResponseValue::BulkString(Some(Bytes::from("user_123"))),
    ]));

    // Execute
    route_message(&worker_txs, frame.clone(), 42, writer_tx);

    // 1. Ensure NO error was sent to the writer
    assert!(writer_rx.try_recv().is_err());

    // 2. Ensure exactly ONE worker received the message
    let mut found = false;
    for rx in &mut worker_rxs {
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
    let worker_count = 2;
    let (worker_txs, _, writer_tx, mut writer_rx) = setup(worker_count);

    let frame = ResponseValue::Array(Some(vec![ResponseValue::BulkString(Some(Bytes::from(
        "PING",
    )))]));

    route_message(&worker_txs, frame, 1, writer_tx);

    let response = writer_rx.try_recv().expect("Should receive PONG response");
    // Check the ResponseMessage structure
    match response.response_value {
        ResponseValue::Error(msg) => {
            assert_eq!(msg, "PONG");
        }
        _ => panic!("Expected Error variant with PONG"),
    }
}

#[tokio::test]
async fn test_invalid_frame_type() {
    let worker_count = 2;
    let (worker_txs, _, writer_tx, mut writer_rx) = setup(worker_count);

    // Sending a SimpleString where an Array is expected
    let frame = ResponseValue::SimpleString("I am not an array".into());

    route_message(&worker_txs, frame, 1, writer_tx);

    let response = writer_rx.try_recv().expect("Should receive error response");
    match response.response_value {
        ResponseValue::Error(msg) => {
            assert!(msg.contains("Value must be array"));
        }
        _ => panic!("Expected Error variant"),
    }
}

#[tokio::test]
async fn test_missing_key_error() {
    let worker_count = 2;
    let (worker_txs, _, writer_tx, mut writer_rx) = setup(worker_count);

    // Command with no key: ["GET"]
    let frame = ResponseValue::Array(Some(vec![ResponseValue::BulkString(Some(Bytes::from(
        "GET",
    )))]));

    route_message(&worker_txs, frame, 1, writer_tx);

    let response = writer_rx.try_recv().expect("Should receive parsing error");
    match response.response_value {
        ResponseValue::Error(msg) => {
            assert!(msg.contains("error while parsing key"));
        }
        _ => panic!("Expected Error variant"),
    }
}

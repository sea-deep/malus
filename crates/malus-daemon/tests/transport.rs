//! ProviderProcess transport multiplexing tests.

use malus_daemon::ProviderProcess;
use malus_protocol::provider::{ProviderEvent, ProviderRequest, ProviderResponse};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;

#[tokio::test]
async fn test_transport_multiplexing_interleaved_events_and_out_of_order() {
    let py_script = r#"
import sys, json

def send(obj):
    body = json.dumps(obj, separators=(',', ':')).encode('utf-8')
    header = f"Content-Length: {len(body)}\r\n\r\n".encode('utf-8')
    sys.stdout.buffer.write(header + body)
    sys.stdout.buffer.flush()

def read_frame():
    headers = b""
    while not headers.endswith(b"\r\n\r\n"):
        c = sys.stdin.buffer.read(1)
        if not c:
            return None
        headers += c
    length = 0
    for line in headers.decode('latin1').split('\r\n'):
        if line.lower().startswith('content-length:'):
            length = int(line.split(':')[1].strip())
            break
    body = sys.stdin.buffer.read(length)
    return json.loads(body.decode('utf-8'))

# 1. Read Hello request
req1 = read_frame()
send({"kind":"response","id":req1["id"],"type":"Hello","data":{"id":"mux","name":"Multiplex Provider","version":[0,1],"capabilities":[],"status":"ready"}})

# 2. Wait for two concurrent requests
req2 = read_frame()
req3 = read_frame()

# Emit unsolicited event 1
send({"kind":"event","event":"StatusChanged","data":{"state":"Playing","current_track":None,"position_ms":100,"duration_ms":200,"volume":100,"muted":False,"shuffle":False,"repeat":"Off"}})

# Emit response for req3 FIRST (id=3, out of order)
send({"kind":"response","id":req3["id"],"type":"Ok"})

# Emit unsolicited event 2
send({"kind":"event","event":"StatusChanged","data":{"state":"Playing","current_track":None,"position_ms":200,"duration_ms":200,"volume":100,"muted":False,"shuffle":False,"repeat":"Off"}})

# Emit response for req2 SECOND (id=2)
send({"kind":"response","id":req2["id"],"type":"Pong"})

# Keep alive
try:
    while sys.stdin.buffer.read(1024):
        pass
except Exception:
    pass
"#;

    let proc = Arc::new(
        ProviderProcess::spawn(
            "mux",
            "Multiplex Provider",
            "python3",
            &["-u", "-c", py_script],
        )
        .await
        .expect("Spawn failed"),
    );

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    proc.set_event_callback(Arc::new(move |evt| {
        let _ = event_tx.send(evt);
    }))
    .await;

    // Concurrently send request A (Ping) and request B (Play)
    let proc_a = proc.clone();
    let fut_a = tokio::spawn(async move { proc_a.send_request(ProviderRequest::Ping).await });

    let proc_b = proc.clone();
    let fut_b = tokio::spawn(async move { proc_b.send_request(ProviderRequest::Play).await });

    let (res_a, res_b) = tokio::join!(fut_a, fut_b);
    let resp_a = res_a.unwrap().expect("Req A should succeed");
    let resp_b = res_b.unwrap().expect("Req B should succeed");

    assert_eq!(resp_a, ProviderResponse::Pong);
    assert_eq!(resp_b, ProviderResponse::Ok);

    // Verify 2 unsolicited events arrived
    let evt1 = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
        .await
        .expect("Timeout on event 1")
        .expect("Channel closed");

    let evt2 = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
        .await
        .expect("Timeout on event 2")
        .expect("Channel closed");

    match evt1 {
        ProviderEvent::StatusChanged(s) => assert_eq!(s.position_ms, 100),
        other => panic!("Unexpected event 1: {other:?}"),
    }

    match evt2 {
        ProviderEvent::StatusChanged(s) => assert_eq!(s.position_ms, 200),
        other => panic!("Unexpected event 2: {other:?}"),
    }

    proc.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_transport_child_exit_fails_pending_requests() {
    let py_script = r#"
import sys, json

def send(obj):
    body = json.dumps(obj, separators=(',', ':')).encode('utf-8')
    header = f"Content-Length: {len(body)}\r\n\r\n".encode('utf-8')
    sys.stdout.buffer.write(header + body)
    sys.stdout.buffer.flush()

send({"kind":"response","id":1,"type":"Hello","data":{"id":"mux","name":"Multiplex Provider","version":[0,1],"capabilities":[],"status":"ready"}})

sys.stdin.buffer.readline()
sys.exit(1)
"#;

    let proc = Arc::new(
        ProviderProcess::spawn(
            "mux",
            "Multiplex Provider",
            "python3",
            &["-u", "-c", py_script],
        )
        .await
        .expect("Spawn failed"),
    );

    let res = proc.send_request(ProviderRequest::Ping).await;
    assert!(res.is_err(), "Request must fail when child exits");
}

#[tokio::test]
async fn test_transport_request_timeout_pruning() {
    let py_script = r#"
import sys, json

def send(obj):
    body = json.dumps(obj, separators=(',', ':')).encode('utf-8')
    header = f"Content-Length: {len(body)}\r\n\r\n".encode('utf-8')
    sys.stdout.buffer.write(header + body)
    sys.stdout.buffer.flush()

send({"kind":"response","id":1,"type":"Hello","data":{"id":"mux","name":"Multiplex Provider","version":[0,1],"capabilities":[],"status":"ready"}})

# Read request but NEVER respond
try:
    while sys.stdin.buffer.read(1024):
        pass
except Exception:
    pass
"#;

    let proc = Arc::new(
        ProviderProcess::spawn(
            "mux",
            "Multiplex Provider",
            "python3",
            &["-u", "-c", py_script],
        )
        .await
        .expect("Spawn failed"),
    );

    // Send a request with a short timeout
    let fut = proc.send_request(ProviderRequest::Ping);
    let res = tokio::time::timeout(Duration::from_millis(50), fut).await;
    assert!(res.is_err(), "Expected timeout");

    proc.shutdown().await.unwrap();
}

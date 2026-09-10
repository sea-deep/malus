//! Transport regressions use a local fake CDP server; no account or browser needed.
use futures_util::{SinkExt, StreamExt};
use malus::engine::CdpClient;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{WebSocketStream, accept_async, tungstenite::Message};
async fn server() -> (String, tokio::task::JoinHandle<WebSocketStream<TcpStream>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        accept_async(stream).await.unwrap()
    });
    (url, task)
}
#[tokio::test]
async fn responses_can_arrive_out_of_order() {
    let (url, server) = server().await;
    let cdp = Arc::new(CdpClient::connect_ws(&url).await.unwrap());
    let mut ws = server.await.unwrap();
    let reply = tokio::spawn(async move {
        let mut requests = vec![];
        for _ in 0..2 {
            let message = ws.next().await.unwrap().unwrap();
            requests.push(serde_json::from_str::<Value>(message.to_text().unwrap()).unwrap());
        }
        for r in requests.into_iter().rev() {
            ws.send(Message::Text(
                json!({"id":r["id"],"result":r["params"]})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        }
    });
    let (a, b) = tokio::join!(
        cdp.send_command("A", json!({"value":1})),
        cdp.send_command("B", json!({"value":2}))
    );
    assert_eq!(a.unwrap()["value"], 1);
    assert_eq!(b.unwrap()["value"], 2);
    reply.await.unwrap();
}
#[tokio::test]
async fn disconnect_fails_pending_requests_immediately() {
    let (url, server) = server().await;
    let cdp = CdpClient::connect_ws(&url).await.unwrap();
    let mut ws = server.await.unwrap();
    tokio::spawn(async move {
        let _ = ws.next().await;
        ws.close(None).await.unwrap();
    });
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        cdp.send_command("Never.reply", json!({})),
    )
    .await
    .expect("hung after disconnect");
    assert!(result.is_err());
}
#[tokio::test]
async fn timeout_does_not_poison_the_next_request() {
    let (url, server) = server().await;
    let cdp = CdpClient::connect_ws(&url).await.unwrap();
    let mut ws = server.await.unwrap();
    tokio::spawn(async move {
        let _ = ws.next().await;
        let r: Value =
            serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        ws.send(Message::Text(
            json!({"id":r["id"],"result":{"ok":true}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    });
    assert_eq!(
        cdp.send_command_timeout("Hang", json!({}), Duration::from_millis(30))
            .await
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::TimedOut
    );
    assert_eq!(
        cdp.send_command("Works", json!({})).await.unwrap()["ok"],
        true
    );
}
#[tokio::test]
async fn cancelled_request_does_not_consume_another_response() {
    let (url, server) = server().await;
    let cdp = Arc::new(CdpClient::connect_ws(&url).await.unwrap());
    let mut ws = server.await.unwrap();
    let clone = cdp.clone();
    let pending = tokio::spawn(async move { clone.send_command("Cancel", json!({})).await });
    let first: Value =
        serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    pending.abort();
    let _ = pending.await;
    tokio::spawn(async move {
        ws.send(Message::Text(
            json!({"id":first["id"],"result":{"wrong":true}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        let r: Value =
            serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        ws.send(Message::Text(
            json!({"id":r["id"],"result":{"ok":true}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    });
    assert_eq!(
        cdp.send_command("After.cancel", json!({})).await.unwrap()["ok"],
        true
    );
}

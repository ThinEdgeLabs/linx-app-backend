use futures_util::SinkExt;
use std::fmt;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{handshake::client::Response, protocol::Message, Error},
    MaybeTlsStream, WebSocketStream,
};

// Example usage:
// #[tokio::main]
// async fn main() {
//     let (mut conn, _) = WsClient::connect_async(URL)
//         .await
//         .expect("Failed to connect");
//     conn.subscribe_blocks().await;
//     let timer = tokio::time::Instant::now();
//     let duration = Duration::new(10, 0);
//     while let Some(message) = conn.as_mut().next().await {
//         if timer.elapsed() >= duration {
//             break;
//         }
//         match message {
//             Ok(message) => {
//                 let data = message.into_data();
//                 let string_data = String::from_utf8(data).expect("Found invalid UTF-8 chars");
//                 tracing::info!("Received: {}", string_data);
//             }
//             Err(_) => break,
//         }
//     }
//     conn.close().await.expect("Failed to disconnect");
// }

pub struct WsClient;

impl WsClient {
    pub async fn connect_async(url: &str) -> Result<(ConnectionState<MaybeTlsStream<TcpStream>>, Response), Error> {
        let (socket, response) = connect_async(url).await?;
        Ok((ConnectionState::new(socket), response))
    }
}

pub struct ConnectionState<T> {
    socket: WebSocketStream<T>,
    id: u64,
}

impl<T: AsyncRead + AsyncWrite + Unpin> ConnectionState<T> {
    pub fn new(socket: WebSocketStream<T>) -> Self {
        Self { socket, id: 0 }
    }

    async fn send(&mut self, method: &str, params: impl IntoIterator<Item = &str>) -> u64 {
        let mut params_str: String =
            params.into_iter().map(|param| format!("\"{}\"", param)).collect::<Vec<String>>().join(",");

        if !params_str.is_empty() {
            params_str = format!("\"params\": [{params}],", params = params_str)
        };

        let id: u64 = self.id;
        self.id += 1;

        let s: String =
            format!("{{\"method\":\"{method}\",{params}\"id\":{id}}}", method = method, params = params_str, id = id);
        let message = Message::Text(s.into());

        self.socket.send(message).await.unwrap();

        id
    }

    pub async fn subscribe_blocks(&mut self) -> u64 {
        self.send("block_notify", vec![]).await
    }

    pub async fn close(mut self) -> Result<(), Error> {
        self.socket.close(None).await
    }
}

impl<T> From<ConnectionState<T>> for WebSocketStream<T> {
    fn from(conn: ConnectionState<T>) -> WebSocketStream<T> {
        conn.socket
    }
}

impl<T> AsMut<WebSocketStream<T>> for ConnectionState<T> {
    fn as_mut(&mut self) -> &mut WebSocketStream<T> {
        &mut self.socket
    }
}

pub struct Stream {
    name: String,
}

impl Stream {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_owned() }
    }

    pub fn as_str(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for Stream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

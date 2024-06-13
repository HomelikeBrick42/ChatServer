use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientToServerMessage {
    Leave,
    Ping,
    Send(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerToClientMessage {
    Ping,
    Joined(SocketAddr),
    Left(SocketAddr),
    Sent(String, SocketAddr),
}

pub async fn send_message<T>(
    stream: impl AsyncWrite,
    message: T,
) -> Result<(), ciborium::ser::Error<std::io::Error>>
where
    T: Serialize,
{
    tokio::pin!(stream);

    let mut bytes = vec![];
    ciborium::into_writer(&message, &mut bytes)?;
    let len: u64 = bytes
        .len()
        .try_into()
        .expect("message size should not exceed u64");

    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(bytes.as_slice()).await?;

    Ok(())
}

pub async fn read_message<T>(
    stream: impl AsyncRead,
) -> Result<T, ciborium::de::Error<std::io::Error>>
where
    T: DeserializeOwned,
{
    tokio::pin!(stream);

    let mut len = [0; std::mem::size_of::<u64>()];
    stream.read_exact(&mut len).await?;
    let len: usize = u64::from_be_bytes(len)
        .try_into()
        .expect("message length should not exceed usize");

    let mut bytes = vec![0; len];
    stream.read_exact(&mut bytes).await?;

    let message = ciborium::from_reader(bytes.as_slice())?;
    Ok(message)
}

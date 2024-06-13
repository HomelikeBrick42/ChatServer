use chat_system::{read_message, send_message, ClientToServerMessage, ServerToClientMessage};
use std::net::SocketAddr;
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
};

async fn handle_connection(
    mut stream: TcpStream,
    addr: SocketAddr,
    to_server_messages: tokio::sync::mpsc::Sender<(ClientToServerMessage, SocketAddr)>,
    mut from_server_messages: tokio::sync::broadcast::Receiver<ServerToClientMessage>,
) -> anyhow::Result<()> {
    let (mut reader, mut writer) = stream.split();
    'outer_loop: loop {
        tokio::pin! {
            let reading_future = read_message(&mut reader);
        }

        loop {
            use tokio::sync::broadcast::error::RecvError;
            tokio::select! {
                result = &mut reading_future => {
                    let message = result?;
                    if let ClientToServerMessage::Leave = message {
                        break 'outer_loop;
                    } else {
                        match to_server_messages.send((message, addr)).await {
                            Ok(()) => continue 'outer_loop,
                            Err(_) => break 'outer_loop,
                        }
                    }
                }
                message = from_server_messages.recv() => match message {
                    Ok(message) => send_message(&mut writer, message).await?,
                    Err(RecvError::Closed) => break 'outer_loop,
                    Err(RecvError::Lagged(count)) => {
                        eprintln!("{addr}: Lagged {count} messages behind");
                    }
                },
            }
        }
    }

    _ = send_message(&mut stream, ServerToClientMessage::Left(addr)).await;
    _ = stream.shutdown().await;
    Ok(())
}

enum Command {
    Quit,
    Ping,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (commands_sender, commands) = tokio::sync::mpsc::channel(1);
    std::thread::spawn(move || {
        for line in std::io::stdin().lines().map_while(Result::ok) {
            let mut tokens = line.split_whitespace();
            if commands_sender
                .blocking_send(match tokens.next() {
                    None => continue,
                    Some("quit") => Command::Quit,
                    Some("ping") => Command::Ping,
                    Some(_) => {
                        eprintln!("Unknown command: '{line}'");
                        continue;
                    }
                })
                .is_err()
            {
                break;
            }
        }
    });

    tokio::spawn(server(commands)).await??;

    Ok(())
}

async fn server(mut commands: tokio::sync::mpsc::Receiver<Command>) -> anyhow::Result<()> {
    let (to_clients, _) = tokio::sync::broadcast::channel(16);
    let (from_client_messages_sender, mut from_client_messages) = tokio::sync::mpsc::channel(1);
    let listener = TcpListener::bind("127.0.0.1:1234").await?;
    loop {
        tokio::select! {
            Ok((stream, addr)) = listener.accept() => {
                let from_server_messages = to_clients.subscribe();
                let from_client_messages = from_client_messages_sender.clone();
                let to_clients = to_clients.clone();
                tokio::spawn(async move {
                    println!("{addr} joined");
                    _ = to_clients.send(ServerToClientMessage::Joined(addr));
                    match handle_connection(stream, addr, from_client_messages, from_server_messages).await {
                        Ok(()) => {}
                        Err(error) => eprintln!("{addr}: {error}"),
                    }
                    println!("{addr} left");
                    _ = to_clients.send(ServerToClientMessage::Left(addr));
                });
            }

            Some(command) = commands.recv() => match command {
                Command::Quit => break,
                Command::Ping => _ = to_clients.send(ServerToClientMessage::Ping),
            },

            Some((message, addr)) = from_client_messages.recv() => match message {
                ClientToServerMessage::Ping => println!("ping from {addr}"),
                ClientToServerMessage::Send(message) => {
                    println!("{addr}: {message}");
                    _ = to_clients.send(ServerToClientMessage::Sent(message, addr));
                }
                ClientToServerMessage::Leave => _ = to_clients.send(ServerToClientMessage::Left(addr)),
            },

            else => {}
        }
    }
    Ok(())
}

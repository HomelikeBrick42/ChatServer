use chat_system::{read_message, send_message, ClientToServerMessage, ServerToClientMessage};
use tokio::{io::AsyncWriteExt, net::TcpStream};

enum Command {
    Quit,
    Ping,
    Send(String),
}

async fn client(mut commands: tokio::sync::mpsc::Receiver<Command>) -> anyhow::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:1234").await?;
    let (mut reader, mut writer) = stream.split();
    'outer_loop: loop {
        tokio::pin! {
            let reading_future = read_message(&mut reader);
        }

        loop {
            tokio::select! {
                message = &mut reading_future => {
                    match message? {
                        ServerToClientMessage::Ping => {
                            send_message(&mut writer, ClientToServerMessage::Ping).await?;
                            println!("pinged");
                        }
                        ServerToClientMessage::Joined(addr) => {
                            println!("{addr} joined");
                        }
                        ServerToClientMessage::Left(addr) => {
                            println!("{addr} left");
                        }
                        ServerToClientMessage::Sent(message, addr) => {
                            println!("{addr}: {message}");
                        }
                    }
                    continue 'outer_loop;
                }

                Some(command) = commands.recv() => match command {
                    Command::Quit => break 'outer_loop,
                    Command::Ping => send_message(&mut writer, ClientToServerMessage::Ping).await?,
                    Command::Send(message) => send_message(&mut writer, ClientToServerMessage::Send(message)).await?,
                },
            }
        }
    }

    send_message(&mut writer, ClientToServerMessage::Leave).await?;
    stream.shutdown().await?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (commands_sender, commands) = tokio::sync::mpsc::channel(1);
    std::thread::spawn({
        let commands_sender = commands_sender.clone();
        move || {
            for line in std::io::stdin().lines().map_while(Result::ok) {
                let mut tokens = line.split_whitespace();
                if commands_sender
                    .blocking_send(match tokens.next() {
                        None => continue,
                        Some("quit") => Command::Quit,
                        Some("ping") => Command::Ping,
                        Some("send") => {
                            Command::Send(line.strip_prefix("send").unwrap().trim().to_string())
                        }
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
        }
    });

    tokio::spawn(client(commands)).await??;

    Ok(())
}

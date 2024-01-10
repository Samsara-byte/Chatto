//Kullanmak için önce kodu açıp serveri başlatıyoruz
//sonra telnet ile localden bağlanıyoruz 127.0.0.1 6969
// ve mesajlaşmaya başlayabiliriz
//ddos için ise cat /dev/urandom | nc 127.0.0.1 6969

//ban sistemi ddos saldırısı için chat için değil

//TODO: kod optimizeye ihtiyacı var. bazım kısımlarda yorum eklemedim daha yorum ihtiyacı var.

use getrandom::getrandom;
use std::collections::HashMap;
use std::fmt::Write as OtherWrite;
use std::io::{Read, Write};
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::result;
use std::str;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime};

type Result<T> = result::Result<T, ()>; //much easier to use

const BAN_LIMIT: Duration = Duration::from_secs(10 * 60); //10 min

const STRIKE_LIMIT: i32 = 10;

const MESSAGE_RATE: Duration = Duration::from_secs(1); //1 second

enum Message {
    ClientConnected {
        author: Arc<TcpStream>,
        author_addr: SocketAddr,
    },
    ClientDisConnected {
        author_addr: SocketAddr,
    },
    NewMessage {
        author_addr: SocketAddr,
        bytes: Vec<u8>,
    },
}


struct Client {
    conn: Arc<TcpStream>,
    last_message: SystemTime,
    strike_count: i32,
    authed: bool,
}

fn server(messages: Receiver<Message>,token:String) -> Result<()> {
    let mut clients = HashMap::<SocketAddr, Client>::new();
    let mut banned_mfs = HashMap::<IpAddr, SystemTime>::new();
    loop {
        let msg = messages.recv().expect("The server receiver not hung up");
        match msg {
            Message::ClientConnected { author,author_addr } => {
                let now = SystemTime::now();
                let banned_at_and_diff = banned_mfs.remove(&author_addr.ip()).and_then(|banned_at| {
                    let diff= now.duration_since(banned_at).unwrap_or_else(|err|{
                        eprintln!("ERROR: Ban time check on client connection: the clock might have gone backwards: {err}");
                        Duration::from_secs(0)
                    });
                
                    if diff >= BAN_LIMIT {
                        None
                    } else {
                        Some(banned_at)
                    }
                });
                

                if let Some(banned_at) = banned_at_and_diff {
                    let diff = banned_at.elapsed().unwrap_or_default(); // Assuming banned_at is a SystemTime
                    banned_mfs.insert(author_addr.ip().clone(), banned_at);
                    let mut author = author.as_ref();
                    let secs = (BAN_LIMIT - diff).as_secs_f32();
                    println!("INFO: Client {} tried to connect, but that MF is banned for {} secs", author_addr, secs);
                    let _ = writeln!(author, "You are now banned MF: {} left", secs).map_err(|err| {
                        eprintln!("ERROR: could not send banned message to {}: {}", author_addr, err);
                    });
                    let _ = author.shutdown(Shutdown::Both).map_err(|err| {
                        eprintln!("ERROR: could not shutdown socket for {}: {}", author_addr, err);
                    });
                }
                
                    else {
                    println!("INFO: Client {author_addr} connected");
                    clients.insert(
                        author_addr.clone(),
                        Client {
                            conn: author.clone(),
                            last_message: now-2*MESSAGE_RATE,
                            strike_count: 0,
                            authed: false,
                        });

                        let _ = write!(author.as_ref(), "Token: ")
        .map_err(|err| eprintln!("ERROR: could not send token promt to {author_addr}:{err}"));
    
                }
            },
            Message::ClientDisConnected { author_addr } => {
                println!("INFO: Client {author_addr} disconnected");
                clients.remove(&author_addr);
            }
            Message::NewMessage { author_addr, bytes } => {
                if let Some(author) = clients.get_mut(&author_addr) {
                    let now = SystemTime::now();
                    let diff = now
                        .duration_since(author.last_message)
                        .unwrap_or_else(|err|{
                            eprintln!("ERROR: message rate check on new message: the clock might have gone backwards: {err}");
                            Duration::from_secs(0)
                        });
                    if diff >= MESSAGE_RATE {
                        if let Ok(text) = str::from_utf8(&bytes) {
                            author.last_message= now;
                            author.strike_count = 0;
                            println!("INFO: client {author_addr} sent message:{bytes:?}");
                            if author.authed {
                            for (addr, client) in clients.iter() {
                                if *addr != author_addr && client.authed {
                                    let _ = writeln!(client.conn.as_ref(),"{text}").map_err(|err| {
                                        eprintln!("ERROR: could not broadcast message to  all clients from {author_addr}: {err}");
                                    });
                                }
                            }
                        } else {
                            if text == token {
                                author.authed = true;
                                println!("INFO: {author_addr} authorizated");

                                let _ = writeln!(author.conn.as_ref(), "Welcom to hell my besto friendo").map_err(|err| {
                                    eprintln!("ERROR: could not send welcome message to {author_addr}: {err}");
                                });
                            }else {
                                println!("INFO:{author_addr} failed authorization");
                                let _ = writeln!(author.conn.as_ref(), "Invalid Token ").map_err(|err| {
                                    eprintln!("Could not notify client {author_addr} about invalid token: {err} ");
                                });
                                let _ = author.conn.shutdown(Shutdown::Both).map_err(|err| {
                                    eprintln!("ERROR: Could not shutdown {author_addr}:{err}");
                                });
                                clients.remove(&author_addr);
                            }
                        }
                        } else {
                            author.strike_count += 1;
                            if author.strike_count >= STRIKE_LIMIT {
                                println!("INFO: Client {author_addr} got banned");
                                banned_mfs.insert(author_addr.ip(), now);
                                let _ = writeln!(author.conn.as_ref(), "You are now banned").map_err(|err| {
                                    eprintln!("ERROR: could not send banned message to {author_addr}: {err}");
                                });
                                let _ = author.conn.shutdown(Shutdown::Both).map_err(|err| {
                                    eprintln!(
                                        "ERROR: could not shutdown socket for {author_addr}: {err}"
                                    );
                                });
                            }
                        }
                    } else {
                        author.strike_count += 1;
                        if author.strike_count >= STRIKE_LIMIT {
                            println!("INFO: Client {author_addr} got banned");
                            banned_mfs.insert(author_addr.ip(), now);
                            let _ = writeln!(author.conn.as_ref(), "You are now banned").map_err(|err| {
                                eprintln!("ERROR: could not send banned message to {author_addr}: {err}");
                            });
                            let _ = author.conn.shutdown(Shutdown::Both).map_err(|err| {
                                eprintln!(
                                    "ERROR: could not shutdown socket for {author_addr}: {err}"
                                );
                            });
                        }
                    }
                }
            }
        }
    }
}


fn client(stream: Arc<TcpStream>, messages: Sender<Message>) -> Result<()> {
    let author_addr = stream.peer_addr().map_err(|err| {
        eprintln!("ERROR: could not get peer address: {err} ");
    })?;


    messages
        .send(Message::ClientConnected {
            author: stream.clone(),
            author_addr,
        })
        .map_err(|err| {
            eprintln!("ERROR: could not send message to the server thread: {err}");
        })?;

    let mut buffer = Vec::new();
    buffer.resize(64, 0);

    loop {
        let n = stream.as_ref().read(&mut buffer).map_err(|err| {
            eprintln!("ERROR: could not send message from client: {err}");
            let _ = messages
                .send(Message::ClientDisConnected { author_addr })
                .map_err(|err| {
                    eprintln!("ERROR: could not send message to the server thread: {err}");
                });
        })?;
        if n > 0 {
            let mut bytes = Vec::new();
            for x in &buffer[0..n] {
                if *x >= 32 {
                    bytes.push(*x)
                }
            }
            messages
                .send(Message::NewMessage { author_addr, bytes })
                .map_err(|err| {
                    eprintln!("ERROR: could not send message to the server thread: {err}");
                })?;
        } else {
            let _ = messages
                .send(Message::ClientDisConnected { author_addr })
                .map_err(|err| {
                    eprintln!("ERROR: could not send message to the server thread: {err}");
                });
            break;
        }
    }
    Ok(())
}

//using () does not bother what error is
// ? is to propagate the errors which means errorları fonksiyounlara yaymak için kullanılır.
// when ? use function must turn into Result and ok
fn main() -> Result<()> {
    //get tokenize//
    let mut buffer = [0; 16];
    let _ = getrandom(&mut buffer).map_err(|err| {
        eprintln!("ERROR: could generate random token: {err}");
    });

    let mut token = String::new();
    for x in buffer.iter() {
        let _ = write!(&mut token, "{x:02X}");
    }

    println!("Token: {token}");
    //finish//

    // address belirlendi
    let address = "0.0.0.0:6969"; //listen all possible addresses
                                  //listener oluşturuldu hangi ip ve porttan dinleyeceği belirlendi
    let listener = TcpListener::bind(address)
        .map_err(|err| eprintln!("ERROR: could not bind {address}: {err}",))?; //this is just error checking

    println!("INFO: listening on {}", address);

    let (message_sender, message_receiver) = channel();

    thread::spawn(|| server(message_receiver,token));

    //gelecek listenerlar için for döngüsü oluşturuldu bu sayede sürekli dinleme modunda olabilecek.
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let stream = Arc::new(stream);
                let message_sender = message_sender.clone();

                thread::spawn(|| client(stream, message_sender));
            }
            Err(err) => {
                eprintln!("ERROR: could not accept connection: {err} ");
            }
        }
    }
    Ok(())
}

use std::{
    thread,
    io::{prelude::*, BufReader, BufWriter, self, stdin},
    net::{TcpListener, TcpStream, ToSocketAddrs, SocketAddr}, error::Error, 
};
use regex::Regex;
use time::OffsetDateTime;
use serde::{Serialize, Deserialize};
use serde_json;

#[derive(Serialize, Deserialize, Debug)]
enum MessageContent {
    Text(String),
}

#[derive(Serialize, Deserialize, Debug)]
struct MessageMetadata {
    sender: SocketAddr,
    time_sent: OffsetDateTime,
}
impl MessageMetadata {
    fn new(sender: SocketAddr) -> Self {
        Self { sender, time_sent: OffsetDateTime::now_utc() }
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Message {
    metadata: MessageMetadata,
    content: MessageContent,
}
impl Message {
    fn new_text(content: &str, sender: SocketAddr) -> Self {
        Message {
            metadata: MessageMetadata::new(sender),
            content: MessageContent::Text(content.to_owned()),
        }
    }
}

fn get_listener<A: ToSocketAddrs>(addr: A) -> Result<TcpListener, Box<dyn Error>> {
    let addr = addr.to_socket_addrs()?.next().unwrap();
    let ip = addr.ip();
    let port = addr.port();

    let listener = match TcpListener::bind(&format!("{}:{}", ip, port)) {
        Ok(listener) => { listener },
        Err(e) => {
            if e.kind() == io::ErrorKind::AddrInUse {
                TcpListener::bind(&format!("{}:0", ip))?
            } else {
                return Err(Box::new(e));
            }
        }
    };

    println!("Bound on {}", listener.local_addr()?);
    return Ok(listener);
}

fn listen(listener: TcpListener) {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("New connection: {}", stream.peer_addr().unwrap());
                thread::spawn(move|| {
                    handle_connection(stream)
                });
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
    }

    fn handle_connection(conn: TcpStream) {
        let peer_addr = conn.peer_addr().unwrap();
        let mut writer = BufWriter::new(conn.try_clone().unwrap());
        let mut reader = BufReader::new(conn.try_clone().unwrap());
    
        loop {
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            if request.is_empty() {
                println!("{} has closed the connection.", peer_addr);
                break;
            }

            println!("Request: {:#?}", serde_json::from_str::<Message>(&request).unwrap());
    
            writer.write(b"Response\n").unwrap();
            writer.flush().unwrap();
        } 
    }
}

fn main() {
    let listener = get_listener("0.0.0.0:7878").unwrap();
    thread::spawn(move|| {
        listen(listener);
    });

    let mut input = String::new();
    loop {
        println!("Enter peer ip adress and port");
        stdin().read_line(&mut input).unwrap();
        input = input.split_whitespace().collect::<Vec<_>>().join(" ");

        let ip_pattern = Regex::new(r"^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}:\d{1,6}$").unwrap();
        if !ip_pattern.is_match(&input) {
            println!("Invalid ip or port for {:?}.", &input);
            input = String::from("");
        } else {
            break;
        }
    }
    

    match TcpStream::connect(&input) {
        Ok(conn) => {
            let peer_addr = conn.peer_addr().unwrap();
            let local_addr = conn.local_addr().unwrap();
            println!("Connected on {}.", peer_addr);
            let mut writer = BufWriter::new(conn.try_clone().unwrap());
            let mut reader = BufReader::new(conn);
            loop {
                println!("Type your message: ");
                let mut input = String::new();
                let mut response = String::new();
                stdin().read_line(&mut input).unwrap();
                let message = Message::new_text(&input, local_addr);
                let request = serde_json::to_string(&message).unwrap() + "\n";
                println!("{:?}", request);
                writer.write(request.as_bytes()).unwrap();
                writer.flush().unwrap();
                reader.read_line(&mut response).unwrap();
                if response.is_empty() {
                    println!("{} has closed the connection.", peer_addr);
                    break;
                }
                println!("Response: {:?}", response.trim());
            }
        },
        Err(e) => {
            println!("Couldn't connect to peer on {:?}, Error: {}.", input, e)
        }
    }
}
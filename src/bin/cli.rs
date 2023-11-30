use std::{io::{stdin, BufWriter, prelude::*, BufReader, stdout}, net::{TcpStream, SocketAddr}, env};
use ap2pmsg::*;
use crossterm::{QueueableCommand, cursor::{SavePosition, RestorePosition}, ExecutableCommand};
use time::OffsetDateTime;

enum InputMode {
    Message,
    AddConnection,
    SelectConnection,
}

fn main() {
    let args: Vec<_> = env::args().collect();

    if args.len() < 2 {
        panic!("ERROR: Insufficient arguments, provide backend address")
    }
    let backend_addr = &args[1];
    let serv_conn = TcpStream::connect(backend_addr).unwrap_or_else(|e| { panic!("ERROR: Could not connect to backend, {}", e) });

    let mut serv_writer = BufWriter::new(serv_conn.try_clone().unwrap());
    let mut serv_reader = BufReader::new(serv_conn);

    BackendToFrontendRequest::LinkingRequest.write(&mut serv_writer).unwrap();

    let mut response = String::new();
    serv_reader.read_line(&mut response).unwrap();
    if let Ok(BackendToFrontendResponse::LinkingResult(response)) = serde_json::from_str::<BackendToFrontendResponse>(&response) {
        match response {
            Ok(()) => { println!("Connection with backend succesfully established") },
            Err(e) => { panic!("ERROR: {}", e) }
        }
    } else {
        panic!("ERROR: Couldn't establish connection with backend; Invalid backend response")
    }

    let mut peer_conn = None;

    let mut input;
    let mut input_mode = InputMode::SelectConnection;
    loop {
        print!("\x1b[2J\x1b[1;1H");
        match input_mode {
            InputMode::SelectConnection => {
                BackendToFrontendRequest::ListPeerConnections.write(&mut serv_writer).unwrap();

                let mut response = String::new();
                serv_reader.read_line(&mut response).unwrap();
                if let Ok(BackendToFrontendResponse::PeerConnectionsListed(mut connections)) = serde_json::from_str::<BackendToFrontendResponse>(&response) {
                    let mut index = 0;
                    if connections.is_empty() {
                        println!("No connections");
                    }

                    connections.dedup_by(|a, b| a.peer_addr == b.peer_addr && a.peer_name == b.peer_name );
                    for conn in &connections {
                        println!("{}) {}: {}", index, conn.peer_name, conn.peer_addr);
                        index += 1;
                    }
                    println!("Type '+' to add a connection");

                    loop {
                        print!("\nSelect device: ");
                        stdout().flush().unwrap();
                        input = String::new();
                        stdin().read_line(&mut input).unwrap();
                        
                        if let Ok(i) = input.trim().parse::<usize>() {
                            if i + 1 > connections.len() {
                                println!("Invalid index; index out of range.");
                                continue;
                            }
                            peer_conn = Some(connections[i].clone());
                            input_mode = InputMode::Message;
                            break;
                        } else {
                            println!("{}", input);
                            if input == "+\n" {
                                input_mode = InputMode::AddConnection;
                                break;
                            }
                            println!("Invalid index; index not a number.");
                            continue;
                        }          
                    }
                } else {
                    panic!("ERROR: Couldn't list connections; Invalid backend response: {}", response)
                }
            },
            InputMode::AddConnection => {
                println!("Add connection");
                let peer_addr: SocketAddr;
                loop {
                    print!("Peer address: ");
                    stdout().flush().unwrap();
                    input = String::new();
                    stdin().read_line(&mut input).unwrap();
                    if let Ok(addr) = input.trim().parse::<SocketAddr>() {
                        peer_addr = addr;
                        break;
                    } else {
                        println!("Invalid address");
                        continue;
                    }
                }                

                BackendToFrontendRequest::EstablishPeerConnection(peer_addr).write(&mut serv_writer).unwrap();
                input_mode = InputMode::SelectConnection;
            },
            InputMode::Message => {
                if let Some(ref peer_conn) = peer_conn {
                    BackendToFrontendRequest::ListMessages(peer_conn.peer_id, OffsetDateTime::UNIX_EPOCH, get_now()).write(&mut serv_writer).unwrap();

                    let mut response = String::new();
                    serv_reader.read_line(&mut response).unwrap();
                    if let Ok(BackendToFrontendResponse::MessagesListed(messages)) = serde_json::from_str::<BackendToFrontendResponse>(&response) {
                        for message in messages {
                            match message.content {
                                MessageContent::Text(text) => {
                                    print!("{}: {}", message.peer_id, text);
                                    stdout().flush().unwrap();
                                }
                            }
                        }
                        println!("");
                    } else {
                        panic!("ERROR: Couldn't list messages; Invalid backend response: {}", response)
                    }
                    

                    print!("Message {}: ", peer_conn.peer_name);
                    stdout().flush().unwrap();
                    stdout().execute(SavePosition).unwrap();
                    println!("\nType Escape to exit.");
                    stdout().execute(RestorePosition).unwrap();
                    input = String::new();
                    stdin().read_line(&mut input).unwrap();

                    if input == "\u{1b}\n" {
                        input_mode = InputMode::SelectConnection;
                        continue;
                    }

                    BackendToFrontendRequest::MessagePeer((
                        peer_conn.peer_id, 
                        MessageContent::Text(input.to_string())
                    )).write(&mut serv_writer).unwrap();
                } else {
                    panic!("ERROR: attempted to write message without a selected connection");
                }
            }      
        } 
    }
}
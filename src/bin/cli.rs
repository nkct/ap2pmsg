use std::{io::{stdin, BufWriter, prelude::*, BufReader, stdout}, net::TcpStream, env};
use ap2pmsg::*;

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

    let mut response = String::new();
    serv_reader.read_line(&mut response).unwrap();
    if let Ok(BackendResponse::ConnectionEstablished(response)) = serde_json::from_str::<BackendResponse>(&response) {
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
        match input_mode {
            InputMode::SelectConnection => {
                let request = serde_json::to_string(
                    &BackendRequest::ListConnections).unwrap() + "\n";
                serv_writer.write(request.as_bytes()).unwrap();
                serv_writer.flush().unwrap();

                let mut response = String::new();
                serv_reader.read_line(&mut response).unwrap();
                if let Ok(BackendResponse::ConnectionsListed(connections)) = serde_json::from_str::<BackendResponse>(&response) {
                    let mut index = 0;
                    if connections.is_empty() {
                        println!("No connections");
                    }
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
                    panic!("ERROR: Couldn't list connections; Invalid backend response")
                }
            },
            InputMode::AddConnection => {
                println!("Not yet implemented");
            },
            InputMode::Message => {
                if let Some(ref peer_conn) = peer_conn {
                    println!("Type your message to {}: ", peer_conn.peer_name);
                    input = String::new();
                    stdin().read_line(&mut input).unwrap();

                    let request = serde_json::to_string(
                        &BackendRequest::Send((
                            peer_conn.peer_id, 
                            MessageContent::Text(input.to_string())
                        ))).unwrap() + "\n";
                    serv_writer.write(request.as_bytes()).unwrap();
                    serv_writer.flush().unwrap();
                } else {
                    panic!("ERROR: attempted to write message without a selected connection");
                }
            }      
        } 
    }
}
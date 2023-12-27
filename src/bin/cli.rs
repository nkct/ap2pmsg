use std::{
    io::{
        stdin, 
        stdout,
        BufWriter, 
        prelude::*, 
        BufReader
    }, 
    net::{
        TcpStream, 
        SocketAddr
    }, 
    env, 
    thread, 
    sync::{
        Arc, 
        Mutex
    }, 
    str::from_utf8
};
use ap2pmsg::*;
use crossterm::{cursor::{SavePosition, RestorePosition}, ExecutableCommand};
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
    let mut serv_reader = BufReader::new(serv_conn.try_clone().unwrap());

    InitialRequest::Frontend(BackendToFrontendRequest::LinkingRequest).write_into(&mut serv_writer).unwrap();

    if let Ok(BackendToFrontendResponse::LinkingResult(response)) = BackendToFrontendResponse::read_from(&mut serv_reader) {
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
    let mut conn_refr_handle = None;
    loop {
        print!("\x1b[2J\x1b[1;1H");
        match input_mode {
            InputMode::SelectConnection => {
                let conns: Arc<Mutex<Vec<Connection>>> = Arc::new(Mutex::new(Vec::new()));
                let serv_conn_clone = serv_conn.try_clone().unwrap();
                let conns_clone = conns.clone();
                conn_refr_handle = Some(thread::spawn(move || {
                    let mut serv_writer = BufWriter::new(serv_conn_clone.try_clone().unwrap());
                    let mut serv_reader = BufReader::new(serv_conn_clone);

                    fn print_connections(serv_writer: &mut BufWriter<TcpStream>, serv_reader: &mut BufReader<TcpStream>, conns: &Arc<Mutex<Vec<Connection>>>) {
                        let conns_clone = conns.clone();
                        
                        BackendToFrontendRequest::ListPeerConnections.write_into(serv_writer).unwrap();

                        if let Ok(BackendToFrontendResponse::PeerConnectionsListed(mut connections)) = BackendToFrontendResponse::read_from(serv_reader) {
                            let mut index = 0;
                            if connections.is_empty() {
                                println!("No connections");
                            }
            
                            connections.dedup_by(|a, b| a.peer_addr == b.peer_addr && a.peer_name == b.peer_name );
                            for conn in &connections {
                                println!("{}) {}: {}", index, conn.peer_name, conn.peer_addr);
                                index += 1;
                            }
            
                            *conns_clone.lock().unwrap() = connections;
                        } else {
                            panic!("ERROR: Couldn't list connections; Invalid backend response")
                        }
                    }
                    print_connections(&mut serv_writer, &mut serv_reader, &conns_clone);
 
                    loop {

                        let mut len_buf = [0; 4];
                        serv_reader.read_exact(&mut len_buf).unwrap();
                        let response_len = u32::from_be_bytes(len_buf) as usize;
                        let mut response_buf = vec![0; response_len];
                        serv_reader.read_exact(&mut response_buf).unwrap();
                        let response = from_utf8(&response_buf).unwrap();
                        if let Ok(refresh_request) = serde_json::from_str::<RefreshRequest>(response) {
                            match refresh_request {
                                RefreshRequest::Connection => {
                                    print_connections(&mut serv_writer, &mut serv_reader, &conns_clone);
                                },
                                RefreshRequest::Message => {
                                    continue;
                                },
                                RefreshRequest::Kill => {
                                    break;
                                }
                            }
                        } else {
                            panic!("Refresher encountred unexpected message: {response:?}");
                        }
                    }
                }));

                loop {
                    // todo: change to crossterm key event
                    println!("Type '+' to add a connection");
                    println!("\nSelect device: ");
                    input = String::new();
                    stdin().read_line(&mut input).unwrap();
                    let conns = conns.lock().unwrap();
                    if let Ok(i) = input.trim().parse::<usize>() {
                        if i + 1 > conns.len() {
                            println!("Invalid index; index out of range.");
                            continue;
                        }
                        peer_conn = Some(conns[i].clone());
                        input_mode = InputMode::Message;
                        break;
                    } else {
                        if input.trim() == "+" {
                            input_mode = InputMode::AddConnection;
                            break;
                        }
                        println!("Invalid index; index not a number.");
                        continue;
                    }          
                }

                if let Some(conn_refresher) = conn_refr_handle {
                    BackendToFrontendRequest::KillRefresher.write_into(&mut serv_writer).unwrap();
                    conn_refresher.join().unwrap();
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

                BackendToFrontendRequest::EstablishPeerConnection(peer_addr).write_into(&mut serv_writer).unwrap();
                input_mode = InputMode::SelectConnection;
            },
            InputMode::Message => {
                if let Some(ref peer_conn) = peer_conn {
                    BackendToFrontendRequest::ListMessages(peer_conn.peer_id, OffsetDateTime::UNIX_EPOCH, get_now()).write_into(&mut serv_writer).unwrap();

                    if let Ok(BackendToFrontendResponse::MessagesListed(messages)) = BackendToFrontendResponse::read_from(&mut serv_reader) {
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
                        panic!("ERROR: Couldn't list messages; Invalid backend response")
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
                    )).write_into(&mut serv_writer).unwrap();
                } else {
                    panic!("ERROR: attempted to write message without a selected connection");
                }
            }      
        } 
    }
}
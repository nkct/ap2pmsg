use std::{
    env, 
    fs, 
    io::{stdin, stdout,BufWriter, prelude::*, BufReader}, 
    iter::repeat, 
    net::{TcpStream, SocketAddr}, 
    path::{Path, PathBuf}, 
    process::exit, 
    str::from_utf8, 
    sync::{Arc, Mutex}, 
    thread
};
use ap2pmsg::*;
use crossterm::{
    cursor::{MoveTo, RestorePosition, SavePosition}, 
    ExecutableCommand, 
    event::{self, Event, KeyCode, KeyModifiers, KeyEventKind}, 
    terminal
};
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
    let mut conn_refr_handle;
    let mut msg_refr_handle;
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
                    let peer_id = peer_conn.peer_id;
                    let serv_conn_clone = serv_conn.try_clone().unwrap();
                    msg_refr_handle = Some(thread::spawn(move || {
                        let mut serv_writer = BufWriter::new(serv_conn_clone.try_clone().unwrap());
                        let mut serv_reader = BufReader::new(serv_conn_clone);

                        fn print_messages(serv_writer: &mut BufWriter<TcpStream>, serv_reader: &mut BufReader<TcpStream>, peer_id: u32) {
                            BackendToFrontendRequest::RetryUnrecieved(peer_id).write_into(serv_writer).unwrap();
                            BackendToFrontendRequest::ListMessages(peer_id, OffsetDateTime::UNIX_EPOCH, get_now()).write_into(serv_writer).unwrap();

                            if let Ok(BackendToFrontendResponse::MessagesListed(mut messages)) = BackendToFrontendResponse::read_from(serv_reader) {
                                let window_size = terminal::size().unwrap();
                                if messages.len() > window_size.1 as usize - 4 {
                                    messages = messages.into_iter().rev().take(window_size.1 as usize - 4).rev().collect();
                                }
                                stdout().execute(SavePosition).unwrap();
                                let mut i = 0;
                                for message in messages {
                                    let mut unsent = "";
                                    if message.time_recieved.is_none() {
                                        unsent = "*";
                                    }
                                    // clear line
                                    stdout().execute(MoveTo(0, i)).unwrap();
                                    write!(stdout(), "{}", repeat(" ").take(window_size.0 as usize).collect::<String>()).unwrap();
                                    // todo: display the correct sender 
                                    match message.content {
                                        MessageContent::Text(text) => {
                                            stdout().execute(MoveTo(0, i)).unwrap();
                                            write!(stdout(), "{}:{}{}", message.peer_id, unsent, text).unwrap();
                                        }
                                        MessageContent::File((name, blob)) => {
                                            if !Path::new("./files").exists() {
                                                fs::create_dir("./files").unwrap();
                                            }
                                            fs::write(["./files", &name].iter().collect::<PathBuf>(), blob).unwrap();
                                            stdout().execute(MoveTo(0, i)).unwrap();
                                            write!(stdout(), "{}:{}FILE: {}", message.peer_id, unsent, name).unwrap();
                                        },
                                    }
                                    i += 1;
                                }
                                stdout().execute(RestorePosition).unwrap();
                            } else {
                                panic!("ERROR: Couldn't list messages; Invalid backend response")
                            }
                        }
                        BackendToFrontendRequest::PingPeer(peer_id).write_into(&mut serv_writer).unwrap();
                        print_messages(&mut serv_writer, &mut serv_reader, peer_id);
    
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
                                        continue;
                                    },
                                    RefreshRequest::Message => {
                                        print_messages(&mut serv_writer, &mut serv_reader, peer_id);
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
                    
                    'message_loop: loop {
                        let window_size = terminal::size().unwrap();
                        stdout().execute(MoveTo(0, window_size.1 - 3)).unwrap();
                        
                        write!(stdout(), "Message {}: ", peer_conn.peer_name).unwrap();
                        stdout().execute(MoveTo(0, window_size.1 - 1)).unwrap();
                        write!(stdout(), "Press Escape to exit.").unwrap();
                        
                        let mut input: Vec<char> = Vec::new();
                        let mut cursor = input.len();
                        let prompt_len = format!("Message {}: ", peer_conn.peer_name).len() as u16;
                        let input_pos = (prompt_len, window_size.1 - 3);
                        terminal::enable_raw_mode().unwrap();
                        'read_loop: loop {
                            stdout().execute(MoveTo(input_pos.0, input_pos.1)).unwrap();
                            write!(stdout(), "{}", repeat(" ").take((window_size.0 - prompt_len) as usize).collect::<String>()).unwrap();
                            stdout().execute(MoveTo(input_pos.0, input_pos.1)).unwrap();
                            write!(stdout(), "{}", input.clone().into_iter().collect::<String>()).unwrap();
                            stdout().execute(MoveTo(input_pos.0 + cursor as u16, input_pos.1)).unwrap();
                            
                            if let Ok(Event::Key(key_event)) = event::read() {
                                if key_event.code == KeyCode::Char('c') && key_event.modifiers == KeyModifiers::CONTROL {
                                    exit(1)
                                }
                                if key_event.kind == KeyEventKind::Release {
                                    continue;
                                }
                                match key_event.code {
                                    KeyCode::Esc => {
                                        terminal::disable_raw_mode().unwrap();
                                        input_mode = InputMode::SelectConnection;
                                        break 'message_loop;
                                    }
                                    KeyCode::Char(c) => {
                                        input.insert(cursor, c);
                                        cursor += 1
                                    }
                                    KeyCode::Left => {
                                        if cursor > 0 {
                                            cursor -= 1
                                        }
                                    }
                                    KeyCode::Right => {
                                        if cursor < input.len() {
                                            cursor += 1
                                        }
                                    }
                                    KeyCode::Backspace => {
                                        if cursor != 0 {
                                            input.remove(cursor-1);
                                            cursor -= 1;
                                        }
                                    }
                                    KeyCode::Delete => {
                                        if cursor != input.len() {
                                            input.remove(cursor);
                                        }
                                    }
                                    KeyCode::Enter => {
                                        break 'read_loop;
                                    }
                                    _ => {}
                                }
                            }
                        }

                        let input: String = input.into_iter().collect();
                        let error_line = window_size.1 - 2;
                        let content;
                        if input.starts_with("/") {
                            let parts: Vec<&str> = input.split_ascii_whitespace().collect();
                            if parts.len() != 2 {
                                stdout().execute(MoveTo(0, error_line)).unwrap();
                                write!(stdout(), "{}", repeat(" ").take(window_size.0 as usize).collect::<String>()).unwrap();
                                stdout().execute(MoveTo(0, error_line)).unwrap();
                                write!(stdout(), "Error: Incorrect command format; Should be: /[COMMAND] [PATH]").unwrap();
                                continue;
                            }
                            match parts[0] {
                                "/file" => {
                                    let path = Path::new(parts[1]);
                                    let file = fs::read_to_string(path);
                                    if let Err(e) = file {
                                        stdout().execute(MoveTo(0, error_line)).unwrap();
                                        write!(stdout(), "{}", repeat(" ").take(window_size.0 as usize).collect::<String>()).unwrap();
                                        stdout().execute(MoveTo(0, error_line)).unwrap();
                                        write!(stdout(), "Error: Could not open file; {}", e).unwrap();
                                        continue;
                                    }
                                    content = MessageContent::File((
                                        path.file_name().unwrap().to_str().unwrap().to_owned(), 
                                        file.unwrap().into()
                                    ));
                                },
                                _ => { 
                                    stdout().execute(MoveTo(0, error_line)).unwrap();
                                    write!(stdout(), "{}", repeat(" ").take(window_size.0 as usize).collect::<String>()).unwrap();
                                    stdout().execute(MoveTo(0, error_line)).unwrap();
                                    write!(stdout(), "Error: Unrecognized command").unwrap();
                                    continue; 
                                },
                            }
                        } else {
                            content = MessageContent::Text(input);
                        }

                        // clear the error line
                        stdout().execute(MoveTo(0, error_line)).unwrap();
                        write!(stdout(), "{}", repeat(" ").take(window_size.0 as usize).collect::<String>()).unwrap();
                        
                        terminal::disable_raw_mode().unwrap();
    
                        BackendToFrontendRequest::MessagePeer((
                            peer_conn.peer_id, 
                            content
                        )).write_into(&mut serv_writer).unwrap();
                    }

                    if let Some(msg_refresher) = msg_refr_handle {
                        BackendToFrontendRequest::KillRefresher.write_into(&mut serv_writer).unwrap();
                        msg_refresher.join().unwrap();
                    }
                } else {
                    panic!("ERROR: attempted to write message without a selected connection");
                }
            }      
        } 
    }
}
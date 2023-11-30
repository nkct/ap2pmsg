use std::{
    thread::{self, JoinHandle},
    io::{prelude::*, BufReader, BufWriter, self, stdin, ErrorKind},
    net::{TcpListener, ToSocketAddrs, SocketAddr, TcpStream}, 
    error::Error, 
    env, 
    process::Command, 
    time::Duration, 
};
use serde_json;

use ap2pmsg::*;

fn main() {
    Server::run();
}

#[derive(Debug, Clone, Copy)]
struct Setttings {
    frontend_type: FrontendType,
    serv_in_background: bool,
    serv_addr: SocketAddr,
    terminal_emulator: &'static str,
    db_path: &'static str,
    self_name: &'static str,
    peer_timeout: Duration
}
impl Default for Setttings {
    fn default() -> Self {
        Setttings { 
            frontend_type: FrontendType::CLI,
            serv_in_background: false,
            serv_addr: "0.0.0.0:7878".parse().unwrap(),
            terminal_emulator: "xfce4-terminal",
            db_path: "./local_storage.db",
            self_name: "Default Name",
            peer_timeout: Duration::from_secs(1),
        }
    }
}

struct Server {
}
impl Server {
    pub fn run() {
        let args: Vec<_> = env::args().collect();
        let mut settings = Setttings::default();

        // initialize db
        let db_conn = DbConn::new(rusqlite::Connection::open(&settings.db_path).unwrap());
        if !db_conn.table_exists("Connections").unwrap() {
            println!("Table Connections doesn't exist, creating");
            db_conn.create_connections_table().unwrap();
        }
        if !db_conn.table_exists("Messages").unwrap() {
            println!("Table Messages doesn't exist, creating");
            db_conn.create_messages_table().unwrap();
        }
        drop(db_conn);

        // arg parsing
        if args.len() >= 2 {
            for (i, arg) in args.iter().enumerate() {
                if arg == "-f" || arg == "--frontend" {
                    if args.len() < i + 1 {
                        panic!("ERROR: Did not supply value for frontend_type argument")
                    }
                    match args[i + 1].to_uppercase().as_str() {
                        "CLI" | "C" | "CMD" | "TERMINAL" => { settings.frontend_type = FrontendType::CLI },
                        "WEB" | "W" | "REACT" => { settings.frontend_type = FrontendType::WEB },
                        _ => { panic!("ERROR: Invalid value for flag frontend_type ('-f', '--frontend'): {}", args[i + 1]) }
                    }
                }
                if arg == "-b" || arg == "--background" {
                    if settings.serv_in_background {
                        settings.serv_in_background = false
                    } else {
                        settings.serv_in_background = true
                    }
                }
            }
        }

        // start listening
        let listener = Self::get_listener(settings.serv_addr).unwrap();
        let listener_thread = thread::spawn(move|| {
            Self::listen(listener, settings);
        });

        // set up frontend
        match settings.frontend_type {
            FrontendType::CLI => {
                if cfg!(debug_assertions) {
                    Command::new("cargo")
                        .args(["build", "--bin", "cli"])
                        .output()
                        .expect("failed to build cli frontend");
                    Command::new(settings.terminal_emulator)
                        .args(["-e", &format!("target/debug/cli {}", settings.serv_addr)])
                        .status()
                        .expect("failed to start cli frontend");
                } else {
                    // get child procces returned status code and handle errors
                    Command::new(settings.terminal_emulator)
                        .args(["-e", &format!("./frontends/cli {}", settings.serv_addr)])
                        .status()
                        .expect("failed to start cli frontend");
                }
                
            },
            FrontendType::WEB => { panic!("TODO: Web frontend is not yet implemented") },
        }

        if settings.serv_in_background {
            panic!("TODO: Running backend in the background is not yet implemented")
        }

        listener_thread.join().unwrap();
        println!("\nPress enter to exit");
        stdin().read_line(&mut String::new()).unwrap();
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

    fn listen(listener: TcpListener, setttings: Setttings) {
        let mut frontend_thread: Option<JoinHandle<_>> = None;
        for incoming in listener.incoming() {
            let conn: TcpStream;
            if let Err(e) = incoming {
                if e.kind() == ErrorKind::WouldBlock {
                    continue;
                } else {
                    println!("EEROR: Cannot accept frontend connection; {}", e);
                    continue;
                }
            } else {
                conn = incoming.unwrap()
            }

            let mut reader = BufReader::new(conn.try_clone().unwrap());
            let mut writer = BufWriter::new(conn.try_clone().unwrap());

            let mut initial_request = String::new();
            reader.read_line(&mut initial_request).unwrap();
            let peer_request_result = serde_json::from_str::<PeerToPeerRequest>(&initial_request);
            if peer_request_result.is_ok() {
                thread::spawn(move || {
                    Server::handle_peer(conn, peer_request_result.unwrap(), setttings);
                });
            } else {
                if let Some(ref handle) = frontend_thread {
                    if !handle.is_finished() {
                        let frontend_addr = conn.peer_addr().unwrap();
                        BackendToFrontendResponse::LinkingResult(Err(format!(
                            "Linking to backend refused; this backend is already serving a frontend at: {}", frontend_addr
                        ))).write(&mut writer).unwrap();
                        println!("Refused fronend linking request from: {}", frontend_addr);
                        break;
                    }
                }

                if let Ok(BackendToFrontendRequest::LinkingRequest) = serde_json::from_str::<BackendToFrontendRequest>(&initial_request) {
                    frontend_thread = Some(thread::spawn(move || {
                        Server::handle_frontend(conn, setttings);
                    }));
                } else {
                    println!("Incorrect request: {:?}", initial_request);
                }
            }
        }
    }

    fn handle_peer(conn: TcpStream, peer_request: PeerToPeerRequest, setttings: Setttings) {
        let mut peer_writer = BufWriter::new(conn.try_clone().unwrap());
        let db_conn = DbConn::new(rusqlite::Connection::open(setttings.db_path).unwrap());

        println!("PeerToPeerRequest::{:#?}", peer_request);
        match peer_request {
            PeerToPeerRequest::ProposeConnection(self_id, peer_name, peer_addr) => {
                let peer_id = db_conn.generate_peer_id().unwrap();

                db_conn.insert_connection(Connection::new(peer_id, self_id, peer_name, peer_addr)).unwrap();
                PeerToPeerResponse::AcceptConnection(peer_id, setttings.self_name.to_owned(), conn.local_addr().unwrap()).write(&mut peer_writer).unwrap();
            },
            PeerToPeerRequest::Message(msg) => {
                PeerToPeerResponse::Recieved(msg.message_id).write(&mut peer_writer).unwrap();
                // handle edge case where a host is sending a message to itself
                if (db_conn.get_peer_addr(msg.peer_id).unwrap() == conn.local_addr().unwrap()) && (db_conn.get_message(msg.message_id).unwrap().is_some()) {
                    db_conn.mark_as_recieved(msg.message_id).unwrap();
                } else {
                    db_conn.insert_message(msg).unwrap();
                }

                // pass msg to frontend
            }
        }
    }

    fn handle_frontend(conn: TcpStream, setttings: Setttings) {
        let db_conn = DbConn::new(rusqlite::Connection::open(setttings.db_path).unwrap());

        let mut frontend_writer = BufWriter::new(conn.try_clone().unwrap());
        let mut frontend_reader = BufReader::new(conn.try_clone().unwrap());

        let frontend_addr = conn.peer_addr().unwrap();

        BackendToFrontendResponse::LinkingResult(Ok(())).write(&mut frontend_writer).unwrap();

        println!("New connection: {}", frontend_addr);            
        loop {
            let mut request = String::new();
            let result = frontend_reader.read_line(&mut request);
            if result.is_err() || request.is_empty() {
                let e = result.err();
                if request.is_empty() || e.as_ref().unwrap().kind() == ErrorKind::ConnectionReset {
                    println!("{} has closed the connection.", frontend_addr);
                } else {
                    println!("Error: {:?}", e.unwrap());
                }
                break;
            }

            match serde_json::from_str::<BackendToFrontendRequest>(&request) {
                Ok(request) => {
                    println!("BackendToFrontendRequest::{:#?}", request);
                    match request {
                        BackendToFrontendRequest::MessagePeer((peer_id, content)) => {
                            let msg = db_conn.new_message(peer_id, content).unwrap();
                            let peer_addr = &db_conn.get_peer_addr(msg.peer_id).unwrap();
                            let peer_conn_result = TcpStream::connect_timeout(peer_addr, setttings.peer_timeout);
                            if let Err(e) = peer_conn_result {
                                println!("ERROR: Could not connect to peer, {}", e);
                                continue;
                            }
                            let peer_conn = peer_conn_result.unwrap();
                            let mut peer_writer = BufWriter::new(peer_conn.try_clone().unwrap());
                            let mut peer_reader = BufReader::new(peer_conn);

                            PeerToPeerRequest::Message(msg).write(&mut peer_writer).unwrap();

                            let mut response = String::new();
                            peer_reader.read_line(&mut response).unwrap();
                            if let Ok(PeerToPeerResponse::Recieved(msg_id)) = serde_json::from_str::<PeerToPeerResponse>(&response) {
                                db_conn.mark_as_recieved(msg_id).unwrap();
                            } else {
                                panic!("ERROR: Invalid peer response from {}: {}", peer_addr, &response)
                            }
                        }
                        BackendToFrontendRequest::ListPeerConnections => {
                            BackendToFrontendResponse::PeerConnectionsListed(db_conn.get_connections().unwrap()).write(&mut frontend_writer).unwrap();
                        }
                        BackendToFrontendRequest::EstablishPeerConnection(peer_addr) => {
                            let peer_conn_result = TcpStream::connect_timeout(&peer_addr, setttings.peer_timeout);
                            if let Err(e) = peer_conn_result {
                                println!("ERROR: Could not connect to peer, {}", e);
                                continue;
                            }
                            let peer_conn = peer_conn_result.unwrap();

                            let mut peer_writer = BufWriter::new(peer_conn.try_clone().unwrap());
                            let mut peer_reader = BufReader::new(peer_conn.try_clone().unwrap());
                            
                            let peer_id = db_conn.generate_peer_id().unwrap();

                            PeerToPeerRequest::ProposeConnection(peer_id, setttings.self_name.to_owned(), conn.local_addr().unwrap()).write(&mut peer_writer).unwrap();

                            let mut response = String::new();
                            peer_reader.read_line(&mut response).unwrap();
                            if let Ok(PeerToPeerResponse::AcceptConnection(self_id, peer_name, peer_addr)) = serde_json::from_str::<PeerToPeerResponse>(&response) {
                                db_conn.insert_connection(Connection::new(peer_id, self_id, peer_name, peer_addr)).unwrap();
                            } else {
                                panic!("ERROR: Couldn't establish connection with peer {}; Invalid peer response", peer_addr)
                            }
                        },
                        BackendToFrontendRequest::LinkingRequest => {
                            panic!("ERROR: Attempted to link an already linked frontend")
                        }
                    }
                },
                Err(e) => {
                    println!("ERROR: Invalid request \n{} \n{:#?}", e, request);
                    frontend_writer.write(b"ERROR: Invalid request\n").unwrap();
                    frontend_writer.flush().unwrap();
                    continue;
                }
            }
        };
    }
}
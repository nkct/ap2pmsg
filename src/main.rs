use std::{
    thread::{self, JoinHandle},
    io::{prelude::*, BufReader, BufWriter, self, stdin, ErrorKind},
    net::{TcpListener, ToSocketAddrs, SocketAddr, TcpStream}, 
    error::Error, 
    env, 
    process::{Command, ExitCode}, 
    time::Duration
};
use serde_json;
use env_logger;
use log::{error, warn, info, debug};
use local_ip_address::local_ip;

use ap2pmsg::*;

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
            serv_addr: SocketAddr::new(local_ip().unwrap(), 7878),
            terminal_emulator: "xfce4-terminal",
            db_path: "./local_storage.db",
            self_name: "Default Name",
            peer_timeout: Duration::from_secs(1),
        }
    }
}

fn main() -> ExitCode {
    env_logger::init();

    let args: Vec<_> = env::args().collect();
    let mut settings = Setttings::default();

    // initialize db
    let db_conn = DbConn::new(rusqlite::Connection::open(&settings.db_path).unwrap());
    if !db_conn.table_exists("Connections").unwrap() {
        info!("Table Connections doesn't exist; creating");
        db_conn.create_connections_table().unwrap();
    }
    if !db_conn.table_exists("Messages").unwrap() {
        info!("Table Messages doesn't exist; creating");
        db_conn.create_messages_table().unwrap();
    }
    drop(db_conn);

    // arg parsing
    if args.len() >= 2 {
        for (i, arg) in args.iter().enumerate() {
            match arg.as_str() {
                "-f" | "--frontend" => {
                    if args.len() < i + 2 {
                        error!("Did not supply value for frontend_type argument");
                        return ExitCode::from(1);
                    }
                    match args[i + 1].to_uppercase().as_str() {
                        "CLI" | "C" | "CMD" | "TERMINAL" => { settings.frontend_type = FrontendType::CLI },
                        "WEB" | "W" | "REACT" => { settings.frontend_type = FrontendType::WEB },
                        "NONE" | "N" => { settings.frontend_type = FrontendType::NONE }
                        _ => { 
                            error!("Invalid value for flag frontend_type ('-f', '--frontend'): {}", args[i + 1]);
                            return ExitCode::from(1);
                        }
                    }
                },
                "-b" | "--background" => {
                    settings.serv_in_background = !settings.serv_in_background;
                }
                _ => {

                }
            }
        }
    }

    // start listening
    let listener = get_listener(settings.serv_addr).unwrap();
    let listener_thread = thread::spawn(move|| {
        listen(listener, settings);
    });

    // set up frontend
    match settings.frontend_type {
        FrontendType::CLI => {
            let mut frontend_path = "./frontends/cli";
            if cfg!(debug_assertions) {
                Command::new("cargo")
                    .args(["build", "--bin", "cli"])
                    .output()
                    .expect("failed to build cli frontend");
                frontend_path = "target/debug/cli";
            }
            // get child procces returned status code and handle errors
            info!("Spawning frontend: {} -e {} {}", settings.terminal_emulator, frontend_path, settings.serv_addr);
            Command::new(settings.terminal_emulator)
                .args(["-e", &format!("{} {}", frontend_path, settings.serv_addr)])
                .status()
                .expect("failed to start cli frontend");
        },
        FrontendType::WEB => { todo!("Web frontend") },
        FrontendType::NONE => {},
    }

    if settings.serv_in_background {
        todo!("Running backend in the background")
    }

    listener_thread.join().unwrap();
    println!("\nPress enter to exit");
    stdin().read_line(&mut String::new()).unwrap();
    info!("Exiting succesfully");
    return ExitCode::SUCCESS;
}

fn get_listener<A: ToSocketAddrs>(addr: A) -> Result<TcpListener, Box<dyn Error>> {
    let addr = addr.to_socket_addrs()?.next().unwrap();
    debug!("Getting listener at {:?}", addr);
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

    info!("Bound listener on {}", listener.local_addr()?);
    return Ok(listener);
}

fn listen(listener: TcpListener, setttings: Setttings) {
    let mut frontend_conn: Option<TcpStream> = None;
    let mut frontend_thread: Option<JoinHandle<_>> = None;
    for incoming in listener.incoming() {
        let conn: TcpStream;
        if let Err(e) = incoming {
            // listener is blocking; this is pointless
            if e.kind() == ErrorKind::WouldBlock {
                continue;
            } else {
                warn!("Cannot accept connection; {}", e);
                continue;
            }
        } else {
            conn = incoming.unwrap()
        }

        let mut reader = BufReader::new(conn.try_clone().unwrap());
        let mut writer = BufWriter::new(conn.try_clone().unwrap());

        let addr = conn.peer_addr().unwrap();


        // rework this into a general request handling pattern
        let mut initial_request = String::new();
        reader.read_line(&mut initial_request).unwrap();
        let peer_request_result = serde_json::from_str::<PeerToPeerRequest>(&initial_request);
        if peer_request_result.is_ok() {
            info!("New peer connection: {}", addr);       
            let frontend_conn_clone;
            if let Some(ref conn) = frontend_conn {
                frontend_conn_clone = Some(conn.try_clone().unwrap());
            } else {
                frontend_conn_clone = None;
            }
            thread::spawn(move || {
                handle_peer(conn, peer_request_result.unwrap(), setttings, frontend_conn_clone);
            });
        } else {
            if let Some(ref handle) = frontend_thread {
                if !handle.is_finished() {
                    BackendToFrontendResponse::LinkingResult(Err(format!(
                        "Linking to backend refused; this backend is already serving a frontend at {}", addr
                    ))).write(&mut writer).unwrap();
                    info!("Refused linking request from frontend at {}", addr);
                    break;
                }
            }

            if let Ok(BackendToFrontendRequest::LinkingRequest) = serde_json::from_str::<BackendToFrontendRequest>(&initial_request) {
                info!("New frontend connection: {}", addr); 
                frontend_conn = Some(conn.try_clone().unwrap());           
                frontend_thread = Some(thread::spawn(move || {
                    handle_frontend(conn, setttings);
                }));
            } else {
                warn!("Incorrect request: {:?}", initial_request);
                continue;
            }
        }
    }
}

fn handle_peer(conn: TcpStream, peer_request: PeerToPeerRequest, setttings: Setttings, fontend_conn: Option<TcpStream>) {
    let peer_addr = conn.peer_addr().unwrap();
    debug!("Handling peer at {}", peer_addr);
    let mut peer_writer = BufWriter::new(conn.try_clone().unwrap());
    let db_conn = DbConn::new(rusqlite::Connection::open(setttings.db_path).unwrap());

    let local_addr = conn.local_addr().unwrap();

    debug!("Recieved PeerToPeerRequest::{:#?}", peer_request);
    match peer_request {
        PeerToPeerRequest::ProposeConnection(self_id, peer_name, peer_addr) => {
            let peer_id = db_conn.generate_peer_id().unwrap();

            db_conn.insert_connection(Connection::new(peer_id, self_id, peer_name, peer_addr)).unwrap();
            PeerToPeerResponse::AcceptConnection(peer_id, setttings.self_name.to_owned(), local_addr).write(&mut peer_writer).unwrap();
            info!("Accepted peer connection from {}", peer_addr);
            if let Some(conn) = fontend_conn {
                let mut frontend_writer = BufWriter::new(conn);
                RefreshRequest::Connection(peer_addr == local_addr).write(&mut frontend_writer).unwrap();
            }
        },
        PeerToPeerRequest::Message(msg) => {
            PeerToPeerResponse::Recieved(msg.message_id).write(&mut peer_writer).unwrap();
            info!("Confirmed recieving message {} from {}", msg.message_id, peer_addr);
            // handle edge case where a host is sending a message to itself
            if (db_conn.get_peer_addr(msg.self_id).unwrap() == local_addr) && (db_conn.get_message(msg.message_id).unwrap().is_some()) {
                db_conn.mark_as_recieved(msg.message_id).unwrap();
            } else {
                db_conn.insert_message(msg).unwrap();
            }
        }
    }
}

fn handle_frontend(conn: TcpStream, setttings: Setttings) {
    let frontend_addr = conn.peer_addr().unwrap();
    debug!("Handling frontend at {}", frontend_addr);
    let db_conn = DbConn::new(rusqlite::Connection::open(setttings.db_path).unwrap());

    let mut frontend_writer = BufWriter::new(conn.try_clone().unwrap());
    let mut frontend_reader = BufReader::new(conn.try_clone().unwrap());


    BackendToFrontendResponse::LinkingResult(Ok(())).write(&mut frontend_writer).unwrap();
    info!("Confirmed linking for frontend at {}", frontend_addr);

    loop {
        let mut request = String::new();
        let result = frontend_reader.read_line(&mut request);
        if result.is_err() || request.is_empty() {
            let e = result.err();
            if request.is_empty() || e.as_ref().unwrap().kind() == ErrorKind::ConnectionReset {
                info!("Frontend at {} has closed the connection", frontend_addr);
            } else {
                warn!("{:?}", e.unwrap());
            }
            break;
        }

        match serde_json::from_str::<BackendToFrontendRequest>(&request) {
            Ok(request) => {
                debug!("Recieved BackendToFrontendRequest::{:#?}", request);
                match request {
                    BackendToFrontendRequest::MessagePeer((peer_id, content)) => {
                        let peer_addr = &db_conn.get_peer_addr(peer_id).unwrap();
                        let peer_conn_result = TcpStream::connect_timeout(peer_addr, setttings.peer_timeout);
                        if let Err(e) = peer_conn_result {
                            warn!("Could not connect to peer, {}", e);
                            continue;
                        }

                        let peer_conn = peer_conn_result.unwrap();
                        let mut peer_writer = BufWriter::new(peer_conn.try_clone().unwrap());
                        let mut peer_reader = BufReader::new(peer_conn);

                        let msg = db_conn.new_message(peer_id, content).unwrap();
                        let msg_id = msg.message_id;
                        PeerToPeerRequest::Message(msg).write(&mut peer_writer).unwrap();
                        info!("Sent message {} to peer at {}", msg_id, peer_addr);

                        let mut response = String::new();
                        peer_reader.read_line(&mut response).unwrap();
                        // refresh frontend after success
                        if let Ok(PeerToPeerResponse::Recieved(msg_id)) = serde_json::from_str::<PeerToPeerResponse>(&response) {
                            db_conn.mark_as_recieved(msg_id).unwrap();
                        } else {
                            error!("Invalid peer response from {}: {}", peer_addr, &response);
                        }
                    }
                    BackendToFrontendRequest::ListPeerConnections => {
                        BackendToFrontendResponse::PeerConnectionsListed(
                            db_conn.get_connections().unwrap()
                        ).write(&mut frontend_writer).unwrap();
                        info!("Listed peer connections");
                    }
                    BackendToFrontendRequest::ListMessages(peer_id, since, untill) => {
                        BackendToFrontendResponse::MessagesListed(
                            db_conn.get_messages(peer_id, since, untill).unwrap()
                        ).write(&mut frontend_writer).unwrap();
                        info!("Listed messages");
                    }
                    BackendToFrontendRequest::EstablishPeerConnection(peer_addr) => {
                        let peer_conn_result = TcpStream::connect_timeout(&peer_addr, setttings.peer_timeout);
                        if let Err(e) = peer_conn_result {
                            warn!("Could not connect to peer, {}", e);
                            // todo: inform frontend about failure
                            continue;
                        }
                        let peer_conn = peer_conn_result.unwrap();

                        let mut peer_writer = BufWriter::new(peer_conn.try_clone().unwrap());
                        let mut peer_reader = BufReader::new(peer_conn.try_clone().unwrap());
                        
                        let peer_id = db_conn.generate_peer_id().unwrap();

                        PeerToPeerRequest::ProposeConnection(peer_id, setttings.self_name.to_owned(), conn.local_addr().unwrap()).write(&mut peer_writer).unwrap();
                        info!("Proposed connection to {}", peer_addr);

                        let mut response = String::new();
                        peer_reader.read_line(&mut response).unwrap();
                        if let Ok(PeerToPeerResponse::AcceptConnection(self_id, peer_name, peer_addr)) = serde_json::from_str::<PeerToPeerResponse>(&response) {
                            db_conn.insert_connection(Connection::new(peer_id, self_id, peer_name, peer_addr)).unwrap();
                        } else {
                            error!("Couldn't establish connection with peer {}; Invalid peer response", peer_addr)
                        }
                    },
                    BackendToFrontendRequest::LinkingRequest => {
                        error!("Attempted to link an already linked frontend")
                    }
                }
            },
            Err(e) => {
                error!("Invalid request \n{} \n{:#?}", e, request);
                BackendToFrontendResponse::InvalidRequest.write(&mut frontend_writer).unwrap();
                continue;
            }
        }
    };
}

use std::{
    thread::{self, JoinHandle},
    io::{BufReader, BufWriter, self, stdin, ErrorKind},
    net::{TcpListener, ToSocketAddrs, SocketAddr, TcpStream}, 
    error::Error, 
    env, 
    process::{Command, ExitCode}, 
    time::Duration
};
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
                info!("Building frontend");
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

        if let Ok(initial_request) = InitialRequest::read_from(&mut reader) {
            match initial_request {
                InitialRequest::Peer(peer_request) => {
                    info!("New peer connection: {}", addr);       
                    let frontend_conn_clone;
                    if let Some(ref conn) = frontend_conn {
                        frontend_conn_clone = Some(conn.try_clone().unwrap());
                    } else {
                        frontend_conn_clone = None;
                    }
                    thread::spawn(move || {
                        handle_peer(conn, peer_request, setttings, frontend_conn_clone);
                    });
                },
                InitialRequest::Frontend(frontend_request) => {
                    if let Some(ref handle) = frontend_thread {
                        if !handle.is_finished() {
                            BackendToFrontendResponse::LinkingResult(Err(format!(
                                "Linking to backend refused; this backend is already serving a frontend at {}", addr
                            ))).write_into(&mut writer).unwrap();
                            info!("Refused linking request from frontend at {}", addr);
                            break;
                        }
                    }
        
                    if let BackendToFrontendRequest::LinkingRequest = frontend_request {
                        info!("New frontend connection: {}", addr); 
                        frontend_conn = Some(conn.try_clone().unwrap());           
                        frontend_thread = Some(thread::spawn(move || {
                            handle_frontend(conn, setttings);
                        }));
                    }
                }
            }
        } else {
            // todo: log the incorrect request
            warn!("Incorrect request");
            continue;
        }
    }
}

fn handle_peer(conn: TcpStream, peer_request: PeerToPeerRequest, setttings: Setttings, frontend_conn: Option<TcpStream>) {
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
            PeerToPeerResponse::AcceptConnection(peer_id, setttings.self_name.to_owned(), local_addr).write_into(&mut peer_writer).unwrap();
            info!("Accepted peer connection from {}", peer_addr);
            if let Some(frontend_conn) = frontend_conn {
                let mut frontend_writer = BufWriter::new(frontend_conn);
                if peer_addr != local_addr {
                    RefreshRequest::Connection.write_into(&mut frontend_writer).unwrap();
                }
            }
        },
        PeerToPeerRequest::Message(msg) => {
            PeerToPeerResponse::Recieved(msg.message_id).write_into(&mut peer_writer).unwrap();
            info!("Confirmed recieving message {} from {}", msg.message_id, peer_addr);
            recieve_message(msg, &db_conn, local_addr, frontend_conn.as_ref())

        }
        PeerToPeerRequest::BulkMessage(msgs) => {
            let msg_ids: Vec<u32> = msgs.iter().map(|msg| msg.message_id).collect();
            PeerToPeerResponse::BulkRecieved(msg_ids.clone()).write_into(&mut peer_writer).unwrap();
            if msgs.is_empty() {
                warn!("Recieved BulkMessage request containing zero messages");
                return;
            }
            let mut msg_ids = msg_ids.into_iter();
            let first = msg_ids.next().unwrap();
            info!("Confirmed recieving messages {} from {}", msg_ids.fold(first.to_string(), |acc, id| format!("{acc}, {id}")), peer_addr);
            for msg in msgs {
                recieve_message(msg, &db_conn, local_addr, frontend_conn.as_ref())
            }
        }
    }
}

fn recieve_message(msg: Message, db_conn: &DbConn, local_addr: SocketAddr, frontend_conn: Option<&TcpStream>) {
    // todo: handle messages from unregistered peer
    // handle edge case where a host is sending a message to itself
    if (db_conn.get_peer_addr(msg.self_id).unwrap() == local_addr) && (db_conn.get_message(msg.message_id).unwrap().is_some()) {
        db_conn.mark_as_recieved(msg.message_id).unwrap();
    } else {
        db_conn.insert_message(msg).unwrap();
        if let Some(frontend_conn) = frontend_conn {
            let mut frontend_writer = BufWriter::new(frontend_conn.try_clone().unwrap());
            RefreshRequest::Message.write_into(&mut frontend_writer).unwrap();
        }
    }
}

fn retry_unrecieved(peer_id: u32, db_conn: &DbConn, setttings: Setttings) {
    if !db_conn.peer_online(peer_id).unwrap() {
        return;
    }
    let unrecieved = db_conn.get_unrecieved_for(peer_id).unwrap();
    if unrecieved.is_empty() {
        return;
    }
    let peer_addr = &db_conn.get_peer_addr(peer_id).unwrap();
    let peer_conn_result = TcpStream::connect_timeout(peer_addr, setttings.peer_timeout);
    if let Err(ref e) = peer_conn_result {
        warn!("Could not connect to peer, {}", e);
        db_conn.set_peer_online(peer_id, false).unwrap();
        return;
    }

    let peer_conn = peer_conn_result.unwrap();
    let mut peer_writer = BufWriter::new(peer_conn.try_clone().unwrap());
    let mut peer_reader = BufReader::new(peer_conn);
                        
    let mut msg_ids = unrecieved.iter().map(|msg| msg.message_id);
    let first_id = msg_ids.next().unwrap();
    let msg_ids = msg_ids.fold(first_id.to_string(), |acc, id| format!("{acc}, {id}"));
    InitialRequest::Peer(PeerToPeerRequest::BulkMessage(unrecieved)).write_into(&mut peer_writer).unwrap();
    info!("Sent messages {} to peer at {}", msg_ids, peer_addr);
    
    if let Ok(PeerToPeerResponse::BulkRecieved(msg_ids)) = PeerToPeerResponse::read_from(&mut peer_reader) {
        db_conn.bulk_mark_as_recieved(msg_ids).unwrap();
    } else {
        error!("Invalid peer response from {}", peer_addr);
    }
}

fn handle_frontend(conn: TcpStream, setttings: Setttings) {
    let frontend_addr = conn.peer_addr().unwrap();
    debug!("Handling frontend at {}", frontend_addr);
    let db_conn = DbConn::new(rusqlite::Connection::open(setttings.db_path).unwrap());

    let mut frontend_writer = BufWriter::new(conn.try_clone().unwrap());
    let mut frontend_reader = BufReader::new(conn.try_clone().unwrap());


    BackendToFrontendResponse::LinkingResult(Ok(())).write_into(&mut frontend_writer).unwrap();
    info!("Confirmed linking for frontend at {}", frontend_addr);

    loop {
        match BackendToFrontendRequest::read_from(&mut frontend_reader) {
            Ok(request) => {
                debug!("Recieved BackendToFrontendRequest::{:#?}", request);
                match request {
                    BackendToFrontendRequest::KillRefresher => {
                        RefreshRequest::Kill.write_into(&mut frontend_writer).unwrap();
                        debug!("Killed refresher");
                    },
                    BackendToFrontendRequest::MessagePeer((peer_id, content)) => {
                        let msg = db_conn.new_message(peer_id, content).unwrap();
                        RefreshRequest::Message.write_into(&mut frontend_writer).unwrap();
                        
                        let peer_addr = &db_conn.get_peer_addr(peer_id).unwrap();
                        let peer_conn_result = TcpStream::connect_timeout(peer_addr, setttings.peer_timeout);
                        if let Err(ref e) = peer_conn_result {
                            warn!("Could not connect to peer, {}", e);
                            db_conn.set_peer_online(peer_id, false).unwrap();
                            continue;
                        }
                        db_conn.set_peer_online(peer_id, true).unwrap();

                        let peer_conn = peer_conn_result.unwrap();
                        let mut peer_writer = BufWriter::new(peer_conn.try_clone().unwrap());
                        let mut peer_reader = BufReader::new(peer_conn);

                        let msg_id = msg.message_id;
                        InitialRequest::Peer(PeerToPeerRequest::Message(msg)).write_into(&mut peer_writer).unwrap();
                        info!("Sent message {} to peer at {}", msg_id, peer_addr);

                        if let Ok(PeerToPeerResponse::Recieved(msg_id)) = PeerToPeerResponse::read_from(&mut peer_reader) {
                            db_conn.mark_as_recieved(msg_id).unwrap();
                        } else {
                            error!("Invalid peer response from {}", peer_addr);
                        }
                    }
                    BackendToFrontendRequest::ListPeerConnections => {
                        BackendToFrontendResponse::PeerConnectionsListed(
                            db_conn.get_connections().unwrap()
                        ).write_into(&mut frontend_writer).unwrap();
                        info!("Listed peer connections");
                    }
                    BackendToFrontendRequest::ListMessages(peer_id, since, untill) => {
                        BackendToFrontendResponse::MessagesListed(
                            db_conn.get_messages(peer_id, since, untill).unwrap()
                        ).write_into(&mut frontend_writer).unwrap();
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

                        InitialRequest::Peer(PeerToPeerRequest::ProposeConnection(peer_id, setttings.self_name.to_owned(), conn.local_addr().unwrap())).write_into(&mut peer_writer).unwrap();
                        info!("Proposed connection to {}", peer_addr);

                        if let Ok(PeerToPeerResponse::AcceptConnection(self_id, peer_name, peer_addr)) = PeerToPeerResponse::read_from(&mut peer_reader) {
                            db_conn.insert_connection(Connection::new(peer_id, self_id, peer_name, peer_addr)).unwrap();
                        } else {
                            error!("Couldn't establish connection with peer {}; Invalid peer response", peer_addr)
                        }
                    },
                    BackendToFrontendRequest::LinkingRequest => {
                        error!("Attempted to link an already linked frontend")
                    },
                    BackendToFrontendRequest::RetryUnrecieved(peer_id) => {
                        retry_unrecieved(peer_id, &db_conn, setttings);
                    },
                    BackendToFrontendRequest::PingPeer(peer_id) => {
                        let peer_addr = &db_conn.get_peer_addr(peer_id).unwrap();
                        let peer_conn_result = TcpStream::connect_timeout(peer_addr, setttings.peer_timeout);
                        if let Err(ref e) = peer_conn_result {
                            warn!("Could not connect to peer, {}", e);
                            db_conn.set_peer_online(peer_id, false).unwrap();
                            continue;
                        }
                        db_conn.set_peer_online(peer_id, true).unwrap();
                    }
                }
            },
            Err(e) => {
                error!("Invalid request: {}", e);
                BackendToFrontendResponse::InvalidRequest.write_into(&mut frontend_writer).unwrap();
                continue;
            }
        }
    };
}

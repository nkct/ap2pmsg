use std::{
    thread::{self, JoinHandle},
    io::{prelude::*, BufReader, BufWriter, self, stdin, ErrorKind},
    net::{TcpListener, ToSocketAddrs, SocketAddr, TcpStream}, 
    error::Error, 
    env, 
    process::Command, path::PathBuf, 
};
use serde_json;

use ap2pmsg::*;

fn main() {
    Server::run();
}

struct Setttings {
    frontend_type: FrontendType,
    serv_in_background: bool,
    serv_addr: SocketAddr,
    terminal_emulator: PathBuf,
    db_path: PathBuf,
}
impl Default for Setttings {
    fn default() -> Self {
        Setttings { 
            frontend_type: FrontendType::CLI,
            serv_in_background: false,
            serv_addr: "0.0.0.0:7878".parse().unwrap(),
            terminal_emulator: "xfce4-terminal".parse().unwrap(),
            db_path: "./local_storage.db".parse().unwrap(),
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
            Self::listen(listener, settings.db_path);
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
                        .spawn()
                        .expect("failed to start cli frontend");
                } else {
                    // get child procces returned status code and handle errors
                    Command::new(settings.terminal_emulator)
                        .args(["-e", &format!("./frontends/cli {}", settings.serv_addr)])
                        .spawn()
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

    fn listen(listener: TcpListener, db_path: PathBuf) {
        // sketchy stuff to simply share an str among threads
        let db_path: &'static str = Box::leak(db_path.into_os_string().into_string().unwrap().into_boxed_str());

        let mut frontend_addr: Option<SocketAddr> = None;
        let mut listener_thread: Option<JoinHandle<()>> = None;
        listener.set_nonblocking(true).expect("Cannot set non-blocking");
        for incoming in listener.incoming() {
            if let Some(ref handle) = listener_thread {
                if handle.is_finished() {
                    break;
                }
            }

            let frontend_conn: TcpStream;
            if let Err(e) = incoming {
                if e.kind() == ErrorKind::WouldBlock {
                    continue;
                } else {
                    println!("EEROR: Cannot accept frontend connection; {}", e);
                    continue;
                }
            } else {
                frontend_conn = incoming.unwrap()
            }
            let mut writer = BufWriter::new(frontend_conn.try_clone().unwrap());
            let mut reader = BufReader::new(frontend_conn.try_clone().unwrap());

            if frontend_addr.is_some() {
                BackendResponse::ConnectionEstablished(Err(format!(
                    "Connection to backend refused; this backend is already serving a frontend at: {}", frontend_addr.unwrap()
                ))).write(&mut writer).unwrap();
                drop(frontend_conn);
                continue;
            }
            frontend_addr = frontend_conn.peer_addr().ok();

            listener_thread = Some(thread::spawn(move|| {
                BackendResponse::ConnectionEstablished(Ok(())).write(&mut writer).unwrap();
                    
                println!("New connection: {}", frontend_addr.unwrap());            
                loop {
                    let mut request = String::new();
                    let result = reader.read_line(&mut request);
                    if result.is_err() || request.is_empty() {
                        let e = result.err();
                        if request.is_empty() || e.as_ref().unwrap().kind() == ErrorKind::ConnectionReset {
                            println!("{} has closed the connection.", frontend_addr.unwrap());
                        } else {
                            println!("Error: {:?}", e.unwrap());
                        }
                        break;
                    }

                    match serde_json::from_str::<BackendRequest>(&request) {
                        Ok(request) => {
                            let db_conn = DbConn::new(rusqlite::Connection::open(db_path).unwrap());
                            println!("Backend Request: {:#?}", request);
                            match request {
                                BackendRequest::Send((peer_id, content)) => {
                                    // TO DO: construct a Message, save to db, send to peer
                                    db_conn.insert_message(peer_id, content).unwrap();
                                }
                                BackendRequest::ListConnections => {
                                    BackendResponse::ConnectionsListed(db_conn.get_connections().unwrap()).write(&mut writer).unwrap();
                                }
                                _ => {
                                    println!("Handling this backend request is not yet implemented")
                                } 
                            }
                        },
                        Err(e) => {
                            println!("ERROR: Invalid request \n{} \n{:#?}", e, request);
                            writer.write(b"ERROR: Invalid request\n").unwrap();
                            writer.flush().unwrap();
                            continue;
                        }
                    }
        
                    writer.write(b"Response\n").unwrap();
                    writer.flush().unwrap();
                } 
            }));
        }
    }
}
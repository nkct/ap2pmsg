use std::{
    thread,
    io::{prelude::*, BufReader, BufWriter, self, stdin},
    net::{TcpListener, TcpStream, ToSocketAddrs}, error::Error, 
};
use serde_json;

use ap2pmsg::*;

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

            match serde_json::from_str::<ServerRequest>(&request) {
                Ok(request) => {
                    match request {
                        ServerRequest::Send((addr, content)) => {
                            println!("Server Request: Send({}, {:?})", addr, content)
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
    }
}

fn main() {
    let listener = get_listener("0.0.0.0:7878").unwrap();
    thread::spawn(move|| {
        listen(listener);
    });
    let mut exit = String::new();
    stdin().read_line(&mut exit).unwrap();
}
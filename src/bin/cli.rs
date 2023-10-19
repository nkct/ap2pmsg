use std::{io::{stdin, BufWriter, prelude::*}, net::TcpStream};
use ap2pmsg::*;

fn establish_server_conn() -> TcpStream {
    let mut server_addr = String::new();
    loop {
        println!("Enter backend adress and port (by default 0.0.0.0:7878)");
        stdin().read_line(&mut server_addr).unwrap();
        server_addr = server_addr.split_whitespace().collect::<Vec<_>>().join(" ");
        if server_addr == "" {
            server_addr = "0.0.0.0:7878".to_owned()
        }

        match TcpStream::connect(&server_addr) {
            Ok(serv_conn) => {
                return serv_conn;
            },
            Err(e) => {
                println!("Couldn't connect to server on {:?}, Error: {}.", server_addr, e);
                continue;
            }
        }
    }
}

fn main() {
    let serv_conn = establish_server_conn();
    let serv_addr = serv_conn.peer_addr().unwrap();

    let mut input = String::new();
    loop {
        println!("Enter request to server: ");
        stdin().read_line(&mut input).unwrap();

        match &input[0..1] {
            "M" => {
                let mut serv_writer = BufWriter::new(serv_conn.try_clone().unwrap());
                let request = serde_json::to_string(&ServerRequest::Send((serv_addr, MessageContent::Text(input[2..].to_string())))).unwrap() + "\n";
                serv_writer.write(request.as_bytes()).unwrap();
                serv_writer.flush().unwrap();
            },
            _ => { 
                input = String::new();
                println!("Invalid request prefix") 
            }
        }
    }
}
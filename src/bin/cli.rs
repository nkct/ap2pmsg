use std::{io::{stdin, BufWriter, prelude::*, BufReader}, net::TcpStream, env};
use ap2pmsg::*;

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

    let mut input = String::new();
    loop {
        println!("Enter request to backend: ");
        stdin().read_line(&mut input).unwrap();

        let request = serde_json::to_string(
            &BackendRequest::Send((
                backend_addr.parse().unwrap(), 
                MessageContent::Text(input.to_string())
            ))).unwrap() + "\n";
        serv_writer.write(request.as_bytes()).unwrap();
        serv_writer.flush().unwrap();
        input = String::new();
    }
}
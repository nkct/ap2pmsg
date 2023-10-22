use std::{io::{stdin, BufWriter, prelude::*}, net::TcpStream, env};
use ap2pmsg::*;

fn main() {
    let args: Vec<_> = env::args().collect();

    if args.len() < 2 {
        panic!("ERROR: Insufficient arguments, provide server address")
    }
    let server_addr = &args[1];
    let serv_conn = TcpStream::connect(server_addr).unwrap_or_else(|e| { panic!("ERROR: Could not connect to server, {}", e) });

    let mut input = String::new();
    loop {
        println!("Enter request to server: ");
        stdin().read_line(&mut input).unwrap();

        match &input[0..1] {
            "M" => {
                let mut serv_writer = BufWriter::new(serv_conn.try_clone().unwrap());
                let request = serde_json::to_string(
                    &ServerRequest::Send((
                        server_addr.parse().unwrap(), 
                        MessageContent::Text(input[2..].to_string())
                    ))).unwrap() + "\n";
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
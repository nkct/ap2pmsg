use std::{io::{stdin, BufWriter, prelude::*, BufReader}, net::TcpStream, env};
use ap2pmsg::*;

fn main() {
    let args: Vec<_> = env::args().collect();

    if args.len() < 2 {
        panic!("ERROR: Insufficient arguments, provide server address")
    }
    let server_addr = &args[1];
    let serv_conn = TcpStream::connect(server_addr).unwrap_or_else(|e| { panic!("ERROR: Could not connect to server, {}", e) });

    let mut serv_writer = BufWriter::new(serv_conn.try_clone().unwrap());
    let mut serv_reader = BufReader::new(serv_conn);

    let mut response = String::new();
    serv_reader.read_line(&mut response).unwrap();
    match &response[..4] {
        "OK: " => { println!("{}", &response[4..]) },
        "ER: " => { panic!("ERROR: {}", &response[4..]) },
        _ => { panic!("ERROR: Invalid backend response") }
    }


    let mut input = String::new();
    loop {
        println!("Enter request to server: ");
        stdin().read_line(&mut input).unwrap();

        let request = serde_json::to_string(
            &ServerRequest::Send((
                server_addr.parse().unwrap(), 
                MessageContent::Text(input.to_string())
            ))).unwrap() + "\n";
        serv_writer.write(request.as_bytes()).unwrap();
        serv_writer.flush().unwrap();
        input = String::new();
    }
}
use std::{
    thread,
    time::Duration,
    io::{prelude::*, BufReader, stdin},
    net::{TcpListener, TcpStream, SocketAddr}, 
    panic,
};
use regex::Regex;

fn main() {
    println!("Enter peer ip adress and port");
    let mut input = String::new();
    stdin().read_line(&mut input).unwrap();
    input.pop();

    let ip_pattern = Regex::new(r"^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}:\d{1,4}$").unwrap();
    if !ip_pattern.is_match(&input) {
        panic!("Invalid ip or port for {:?}.", &input);
    }

    thread::spawn(move|| {
        loop {
            if let Ok(mut connection) = TcpStream::connect_timeout(&input.as_str().parse::<SocketAddr>().unwrap(), Duration::from_millis(500)) {
                println!("Connected on {}.", connection.peer_addr().unwrap());

                connection.write(b"Request").unwrap();
                println!("Pinged {}.", &input);
            } else {
                println!("Couldn't connect to peer.")
            }
            

            thread::sleep(Duration::from_millis(3000))
        }
    });

    let server_addr = "0.0.0.0:7878";
    let listener = TcpListener::bind(server_addr).unwrap();
    println!("Bound on {}.", server_addr);

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
}

fn handle_connection(mut stream: TcpStream) {
    let buf_reader = BufReader::new(&mut stream);
    let request: Vec<_> = buf_reader
        .lines()
        .map(|result| result.unwrap())
        .take_while(|line| !line.is_empty())
        .collect();

    println!("Request: {:#?}", request);

    stream.write(b"Response").unwrap();
}
use std::{
    thread,
    time::Duration,
    io::{prelude::*, BufReader},
    net::{TcpListener, TcpStream},
};

fn main() {
    thread::spawn(move|| {
        let mut connection = TcpStream::connect("127.0.0.1:7878").unwrap();
        loop {
            connection.write(b"Request").unwrap();

            thread::sleep(Duration::from_millis(3000))
        }
    });

    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();

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
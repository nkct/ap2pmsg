use std::{net::{SocketAddr, TcpStream}, io::{BufWriter, self, Write}};
use serde::{Serialize, Deserialize};
use time::OffsetDateTime;

pub trait Writable {
    fn write(&self, writer: &mut BufWriter<TcpStream>) -> Result<(), io::Error> where Self: Serialize {
        let send = serde_json::to_string(
            self
        )? + "\n";
        writer.write(send.as_bytes())?;
        writer.flush()?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendRequest {
    Send((SocketAddr, MessageContent)),
}
impl Writable for BackendRequest {}

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendResponse {
    ConnectionEstablished(Result<(), String>)
}
impl Writable for BackendResponse {}


#[derive(Serialize, Deserialize, Debug)]
pub enum MessageContent {
    Text(String),
}

// sender_id is unique to each connection
#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    id: u64,
    sender_id: u64,
    recepient_id: u64,
    recieved: bool,
    time_sent: OffsetDateTime,
    time_recieved: Option<OffsetDateTime>,
    content: MessageContent,
}
impl Message {
    fn get_new_message_id() -> u64 {
        1
    }
    fn get_sender(recepient_id: u64) -> u64 {
        1
    }
    fn get_recepient_id(recepient_name: &str) -> u64 {
        1
    }
    pub fn new_text(content: &str, recepient_name: &str) -> Self {
        let recepient_id = Message::get_recepient_id(recepient_name);
        Message {
            id: Message::get_new_message_id(),
            sender_id: Message::get_sender(recepient_id),
            recepient_id,
            recieved: false,
            time_sent: get_now(),
            time_recieved: None,
            content: MessageContent::Text(content.to_owned()),
        }
    }
}

#[derive(Debug)]
pub enum FrontendType {
    CLI,
    WEB,
}
use std::{net::{SocketAddr, TcpStream}, io::{BufWriter, self, Write}};
use serde::{Serialize, Deserialize};
use time::OffsetDateTime;

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendRequest {
    Send((SocketAddr, MessageContent)),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendResponse {
    ConnectionEstablished(Result<(), String>)
}
impl BackendResponse {
    pub fn write(self, writer: &mut BufWriter<TcpStream>) -> Result<(), io::Error> {
        let response = serde_json::to_string(
            &self
        )? + "\n";
        writer.write(response.as_bytes())?;
        writer.flush()?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum MessageContent {
    Text(String),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MessageMetadata {
    sender: SocketAddr,
    time_sent: OffsetDateTime,
}
impl MessageMetadata {
    pub fn new(sender: SocketAddr) -> Self {
        Self { sender, time_sent: OffsetDateTime::now_utc() }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    metadata: MessageMetadata,
    content: MessageContent,
}
impl Message {
    pub fn new_text(content: &str, sender: SocketAddr) -> Self {
        Message {
            metadata: MessageMetadata::new(sender),
            content: MessageContent::Text(content.to_owned()),
        }
    }
}

#[derive(Debug)]
pub enum FrontendType {
    CLI,
    WEB,
}
use std::{net::{SocketAddr, TcpStream, IpAddr, Ipv4Addr}, io::{BufWriter, self, Write}, any::type_name};
use serde::{Serialize, Deserialize};
use serde_json::Value;
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
    pub fn empty() -> Self {
        Message {
            id: 0,
            sender_id: 0,
            recepient_id: 0,
            recieved: false,
            time_sent: get_now(),
            time_recieved: None,
            content: MessageContent::Text(String::new()),
        }
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

#[derive(Serialize, Deserialize, Debug)]
pub struct Connection {
    peer_id: u64,
    peer_name: String,
    peer_addr: SocketAddr,
    online: bool,
    time_established: OffsetDateTime,
    self_id: u64,
}
impl Connection {
    fn get_peer_id(peer_name: &str) -> u64 {
        1
    }
    fn get_peer_addr(peer_id: u64) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080)
    }
    fn get_self_id(peer_id: u64) -> u64 {
        1
    }
    pub fn empty() -> Self {
        Connection {
            peer_id: 0,
            peer_name: String::new(),
            peer_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            online: false,
            time_established: get_now(),
            self_id: 0,
        }
    }
    pub fn new(peer_name: &str) -> Self {
        let peer_id = Connection::get_peer_id(peer_name);
        Connection {
            peer_id,
            peer_name: peer_name.to_owned(),
            peer_addr: Connection::get_peer_addr(peer_id),
            online: false,
            time_established: get_now(),
            self_id: Connection::get_self_id(peer_id),
        }
    }
}

#[derive(Debug)]
pub enum FrontendType {
    CLI,
    WEB,
}

pub fn get_now() -> OffsetDateTime {
    if let Ok(time) = OffsetDateTime::now_local() {
        return time;
    } else {
        OffsetDateTime::now_utc()
    }
}

pub struct DbConn(rusqlite::Connection);
impl DbConn {
    pub fn new(conn: rusqlite::Connection) -> Self {
        DbConn(conn)
    }
    pub fn table_exists(&self, table_name: &str) -> rusqlite::Result<bool, > {
        Ok(self.0
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = :table_name;")?
            .query([table_name])?.next().unwrap().is_some()
        )
    }
    pub fn table_from_struct<T: Serialize>(&self, t: T) -> Result<(), Box<dyn std::error::Error>> {
        fn destructure(object: &serde_json::Map<String, Value>) -> String {
            let mut columns = String::new();
            for (field, value) in object {
                let datatype = match value {
                    Value::Bool(_)   => { "INTEGER".to_owned() }
                    Value::String(_) => { "TEXT".to_owned()    },
                    Value::Number(_) => { "INTEGER".to_owned() },
                    Value::Null      => { "NULL".to_owned()    },
                    Value::Object(object) => {
                        let (field, _) = object.into_iter().next().unwrap();
                        let datatype = match field.as_str() {
                            "Text" => { "TEXT".to_owned() },
                            _ => { panic!("ERROR: encountered unsupported message content type") }
                        };
                        columns.push_str("content_type TEXT, ");
                        columns.push_str(&format!("content {}, ", &datatype));
                        continue;
                    }
                    _ => {
                        panic!("ERROR: encountered unsupported datatype");
                    }
                };
                columns.push_str(&format!("{} {}, ", field, &datatype));
            }
            let last_comma = columns.rfind(",").unwrap();
            return String::from(columns[..last_comma].to_owned() + &columns[last_comma + 1..]);
        }

        let mut columns = String::new();
        if let Some(object) = serde_json::json!(t).as_object() {
            columns.push_str(&destructure(object));
        } else {
            return Err("supplied struct is not an object".into());
        }
        
        let mut typename = type_name::<T>();
        if typename.starts_with("ap2pmsg::") {
            typename = &typename[9..];
        }

        let query = &format!("CREATE TABLE {}s ({})", typename, columns);
        self.0.execute(query, ())?;

        Ok(())
    }
}
use std::{net::{SocketAddr, TcpStream}, io::{BufWriter, self, Write}, error::Error, fmt::Display, string};
use serde::{Serialize, Deserialize};
use time::{OffsetDateTime};

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
    message_id: u64,
    self_id: u64,
    peer_id: u64,
    recieved: bool,
    time_sent: OffsetDateTime,
    time_recieved: Option<OffsetDateTime>,
    content: MessageContent,
}
impl Message {
    pub fn empty() -> Self {
        Message {
            message_id: 0,
            self_id: 0,
            peer_id: 0,
            recieved: false,
            time_sent: get_now(),
            time_recieved: None,
            content: MessageContent::Text(String::new()),
        }
    }
    pub fn new_text(conn: DbConn, content: &str, peer_id: u64) -> Self {
        Message {
            message_id: conn.get_new_message_id().unwrap(),
            self_id: conn.get_self_id(peer_id).unwrap(),
            peer_id,
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
    self_id: u64,
    peer_name: String,
    peer_addr: SocketAddr,
    online: bool,
    time_established: OffsetDateTime,
}
impl Connection {
    pub fn new(peer_id: u64, self_id: u64, peer_name: String, peer_addr: SocketAddr) -> Self {
        Connection {
            peer_id,
            self_id,
            peer_name,
            peer_addr,
            online: true,
            time_established: get_now(),
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

#[derive(Debug)]
pub enum DbErr {
    SqlError(rusqlite::Error),
    TimeError(time::Error),
    UtfError(string::FromUtf8Error),
    InvalidMessageContentType,
}
impl Display for DbErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#}", self)
    }
}
impl Error for DbErr {}
impl From<rusqlite::Error> for DbErr {
    fn from(value: rusqlite::Error) -> Self {
        DbErr::SqlError(value)
    }
}
impl From<time::Error> for DbErr {
    fn from(value: time::Error) -> Self {
        DbErr::TimeError(value)
    }
}
impl From<string::FromUtf8Error> for DbErr {
    fn from(value: string::FromUtf8Error) -> Self {
        DbErr::UtfError(value)
    }
}

// look into replacing sqlite with nosql
pub struct DbConn(rusqlite::Connection);
impl DbConn {
    pub fn new(conn: rusqlite::Connection) -> Self {
        DbConn(conn)
    }
    pub fn table_exists(&self, table_name: &str) -> rusqlite::Result<bool> {
        Ok(self.0
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = :table_name;")?
            .query([table_name])?.next().unwrap().is_some()
        )
    }    

    pub fn create_messages_table(&self) -> Result<usize, Box<dyn std::error::Error>> {
        Ok(self.0.execute("
        CREATE TABLE Messages (
            message_id INTEGER, 
            connection_id INTEGER,
            time_sent TEXT DEFAULT CURRENT_TIMESTAMP, 
            time_recieved TEXT, 
            content_type TEXT NOT NULL, 
            content BLOB, 
            PRIMARY KEY (message_id),
            FOREIGN KEY (connection_id) REFERENCES Connections(connection_id)
        );", ())?)
    }

    pub fn create_connections_table(&self) -> Result<usize, Box<dyn std::error::Error>> {
        Ok(self.0.execute("
        CREATE TABLE Connections (
            connection_id INTEGER,
            peer_id INTEGER NOT NULL UNIQUE, 
            self_id INTEGER NOT NULL, 
            peer_name TEXT NOT NULL, 
            peer_addr TEXT NOT NULL, 
            online INTEGER DEFAULT 1, 
            time_established TEXT DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (connection_id)
        );", ())?)
    }

    fn insert_connection(&self, connection: Connection) -> Result<usize, Box<dyn std::error::Error>> {
        // for the sake of brevity
        let c = connection;
        Ok(self.0.execute("
            INSERT INTO Connections (peer_id, self_id, peer_name, peer_addr, online, time_established) VALUES 
            (?1, ?2, ?3, ?4, ?5, ?6);", 
            (c.peer_id, c.self_id, c.peer_name, c.peer_addr.to_string(), c.online, c.time_established.to_string())
        )?)
    }


        }
        
}
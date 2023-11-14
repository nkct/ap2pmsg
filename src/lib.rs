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
    time_sent: OffsetDateTime,
    time_recieved: Option<OffsetDateTime>,
    content: MessageContent,
}
impl Message {
    pub fn new(conn: &DbConn, peer_id: u64, content: MessageContent) -> Self {
        let message_id = conn.insert_message(peer_id, content).unwrap();
        conn.get_message(message_id).unwrap()
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


    pub fn insert_message(&self, peer_id: u64, content: MessageContent) -> Result<u64, Box<dyn std::error::Error>> {
        let content_type: &str;
        let blob: Vec<u8>;
        match content {
            MessageContent::Text(content) => {
                content_type = "TEXT";
                blob = content.into();
            }
        }
        self.0.execute("
        INSERT INTO Messages (connection_id, time_recieved, content_type, content) VALUES 
        ((SELECT connection_id FROM Connections WHERE peer_id == ?1), NULL, ?2, ?3)", (peer_id, content_type, blob))?;
        
        let mut stmt = self.0.prepare("SELECT message_id FROM Messages ORDER BY message_id LIMIT 1;")?;
        let mut results = stmt.query_map((), |row| {row.get::<usize, u64>(0)})?;
        Ok(results.next().unwrap()?)
    }

    pub fn get_message(&self, message_id: u64) -> Result<Message, DbErr> {
        let mut stmt = self.0.prepare("
            SELECT message_id, peer_id, self_id, time_sent, time_recieved, content_type, content FROM Messages 
            NATURAL JOIN Connections
            WHERE message_id == 1?
        ;")?;
        let values = stmt.query_map([message_id], |row| {
            Ok((
                row.get::<usize, u64>(0)?,
                row.get::<usize, u64>(1)?,
                row.get::<usize, u64>(2)?,
                row.get::<usize, String>(3)?,
                row.get::<usize, Option<String>>(4)?,
                row.get::<usize, String>(5)?,
                row.get::<usize, Vec<u8>>(6)?,
            ))
        })?.next().unwrap()?;

        let datetime_format =  &time::format_description::parse(
            "[year]-[month]-[day] [hour]:[minute]:[second]"
        ).unwrap();

        let content = match values.5.as_str() {
            "TEXT" => {
                MessageContent::Text(String::from_utf8(values.6)?)
            }
            _ => {
                return Err(DbErr::InvalidMessageContentType)?;
            }
        };

        Ok(Message {
            message_id: values.0,
            peer_id: values.1,
            self_id: values.2,
            time_sent: OffsetDateTime::parse(&values.3, datetime_format).unwrap(),
            time_recieved: values.4.map(|time_recieved| {
                OffsetDateTime::parse(&time_recieved, datetime_format).unwrap()
            }),
            content,
        })
    }    
}
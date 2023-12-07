use std::{
    net::{SocketAddr, TcpStream}, 
    io::{BufWriter, self, Write, BufReader}, 
    error::Error, 
    fmt::Display, 
    string, 
    fs::File, 
    path::Path, 
};
use serde::{Serialize, Deserialize};
use time::{OffsetDateTime, format_description};
use rand::rngs::OsRng;
use rand_unique::RandomSequenceBuilder;

pub const DATETIME_FORMAT: &str = "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3][offset_hour]:[offset_minute]";

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
pub enum BackendToFrontendRequest {
    // close server
    LinkingRequest,
    EstablishPeerConnection(SocketAddr),
    ListMessages(u32, OffsetDateTime, OffsetDateTime),
    ListPeerConnections,
    MessagePeer((u32, MessageContent)),
}
impl Writable for BackendToFrontendRequest {}

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendToFrontendResponse {
    LinkingResult(Result<(), String>),
    PeerConnectionsListed(Vec<Connection>),
    MessagesListed(Vec<Message>),
    InvalidRequest,
}
impl Writable for BackendToFrontendResponse {}

#[derive(Serialize, Deserialize, Debug)]
pub enum PeerToPeerRequest {
    ProposeConnection(u32, String, SocketAddr),
    Message(Message),
}
impl Writable for PeerToPeerRequest {}

#[derive(Serialize, Deserialize, Debug)]
pub enum PeerToPeerResponse {
    AcceptConnection(u32, String, SocketAddr),
    Recieved(u32),
}
impl Writable for PeerToPeerResponse {}

#[derive(Serialize, Deserialize, Debug)]
pub enum MessageContent {
    Text(String),
}

// sender_id is unique to each connection
#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    pub message_id: u32,
    pub self_id: u32,
    pub peer_id: u32,
    pub time_sent: OffsetDateTime,
    pub time_recieved: Option<OffsetDateTime>,
    pub content: MessageContent,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Connection {
    pub peer_id: u32,
    pub self_id: u32,
    pub peer_name: String,
    pub peer_addr: SocketAddr,
    pub online: bool,
    pub time_established: OffsetDateTime,
}
impl Connection {
    pub fn new(peer_id: u32, self_id: u32, peer_name: String, peer_addr: SocketAddr) -> Self {
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

#[derive(Debug, Clone, Copy)]
pub enum FrontendType {
    CLI,
    WEB,
    NONE,
}

pub fn get_now() -> OffsetDateTime {
    if let Ok(time) = OffsetDateTime::now_local() {
        return time;
    } else {
        OffsetDateTime::now_utc()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct IdSequence{
    config: RandomSequenceBuilder<u32>, 
    n: u32,
}
impl IdSequence {
    fn new() -> Self {
        Self {
            config: RandomSequenceBuilder::<u32>::rand(&mut OsRng),
            n: 0,
        }
    }
}
impl Iterator for IdSequence {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.n > u32::MAX {
            return None;
        }

        let res = self.config.into_iter().n(self.n);
        self.n += 1;
        return Some(res);
    }
}

#[derive(Debug)]
pub enum DbErr {
    SqlError(rusqlite::Error),
    TimeError(time::Error),
    UtfError(string::FromUtf8Error),
    InvalidMessageContentType,
    NoAvailableId,
    UnableToSaveSequenceConfig(io::Error)
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
impl From<io::Error> for DbErr {
    fn from(value: io::Error) -> Self {
        DbErr::UnableToSaveSequenceConfig(value)
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

    pub fn insert_connection(&self, connection: Connection) -> Result<usize, Box<dyn std::error::Error>> {
        let datetime_format =  &format_description::parse(DATETIME_FORMAT).unwrap();
        // for the sake of brevity
        let c = connection;
        let mut stmt = self.0.prepare("INSERT INTO Connections 
        (peer_id, self_id, peer_name, peer_addr, online, time_established) VALUES 
        (?1, ?2, ?3, ?4, ?5, ?6);")?;
        let res = stmt.execute((
            c.peer_id, 
            c.self_id, 
            c.peer_name, 
            c.peer_addr.to_string(), 
            c.online as u8, 
            c.time_established.format(datetime_format).unwrap()))?;
        Ok(res)
    }

    pub fn get_peer_addr(&self, peer_id: u32) -> Result<SocketAddr, DbErr>{
        let mut stmt = self.0.prepare("
            SELECT peer_addr FROM Connections 
            WHERE peer_id == ?1
        ;")?;
        let addr = stmt.query_row([peer_id], |row| {
            Ok(row.get::<usize, String>(0)?)
        })?;
        return Ok(addr.parse::<SocketAddr>().unwrap());
    }

    pub fn get_peer_name(&self, peer_id: u32) -> Result<String, DbErr>{
        let mut stmt = self.0.prepare("
            SELECT peer_name FROM Connections 
            WHERE peer_id == ?1
        ;")?;
        let name = stmt.query_row([peer_id], |row| {
            Ok(row.get::<usize, String>(0)?)
        })?;
        return Ok(name);
    }

    pub fn mark_as_recieved(&self, msg_id: u32) -> Result<usize, DbErr> {
        let datetime_format =  &format_description::parse(DATETIME_FORMAT).unwrap();
        let mut stmt = self.0.prepare("
            UPDATE Messages SET time_recieved = ?1
            WHERE message_id == ?2
        ;")?;
        Ok(stmt.execute((get_now().format(datetime_format).unwrap(), msg_id))?)
    }

    pub fn get_connection(&self, connection_id: u32) -> Result<Connection, DbErr> {
        let mut stmt = self.0.prepare("
            SELECT peer_id, self_id, peer_name, peer_addr, online, time_established FROM Connections 
            WHERE connection_id == ?1
        ;")?;
        let values = stmt.query_row([connection_id], |row| {
            Ok((
                row.get::<usize, u32>(0)?,
                row.get::<usize, u32>(1)?,
                row.get::<usize, String>(2)?,
                row.get::<usize, String>(3)?,
                row.get::<usize, bool>(4)?,
                row.get::<usize, String>(5)?,
            ))
        })?;

        Ok(Connection {
            peer_id: values.0,
            self_id: values.1,
            peer_name: values.2,
            peer_addr: values.3.parse::<SocketAddr>().unwrap(),
            online: values.4,
            time_established: OffsetDateTime::parse(&values.5, &format_description::parse(DATETIME_FORMAT).unwrap()).unwrap(),
        })
    }

    pub fn get_connections(&self) -> Result<Vec<Connection>, DbErr> {
        let mut stmt = self.0.prepare("
            SELECT peer_id, self_id, peer_name, peer_addr, online, time_established FROM Connections
        ;")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<usize, u32>(0)?,
                row.get::<usize, u32>(1)?,
                row.get::<usize, String>(2)?,
                row.get::<usize, String>(3)?,
                row.get::<usize, bool>(4)?,
                row.get::<usize, String>(5)?,
            ))
        })?;

        let mut conns = Vec::new();
        for values in rows {
            let values = values?;
            conns.push(Connection {
                peer_id: values.0,
                self_id: values.1,
                peer_name: values.2,
                peer_addr: values.3.parse::<SocketAddr>().unwrap(),
                online: values.4,
                time_established: OffsetDateTime::parse(&values.5, &format_description::parse(DATETIME_FORMAT).unwrap()).unwrap(),
            })
        }
        

        return Ok(conns);
    }

    pub fn new_message(&self, peer_id: u32, content: MessageContent) -> Result<Message, Box<dyn std::error::Error>> {
        let content_type: &str;
        let blob: Vec<u8>;
        match content {
            MessageContent::Text(content) => {
                content_type = "TEXT";
                blob = content.into();
            }
        }
        
        let datetime_format =  &format_description::parse(DATETIME_FORMAT).unwrap();
        self.0.execute("
        INSERT INTO Messages (connection_id, time_sent, time_recieved, content_type, content) VALUES 
        ((SELECT connection_id FROM Connections WHERE peer_id == ?1), ?2, NULL, ?3, ?4)", 
        (peer_id, get_now().format(datetime_format).unwrap(), content_type, blob))?;
        
        let msg = self.get_message(self.0.last_insert_rowid() as u32).unwrap().unwrap();
        return Ok(msg);
    }

    pub fn insert_message(&self, msg: Message) -> Result<u32, Box<dyn std::error::Error>> {
        let content_type: &str;
        let blob: Vec<u8>;
        match msg.content {
            MessageContent::Text(content) => {
                content_type = "TEXT";
                blob = content.into();
            }
        }
        let datetime_format =  &format_description::parse(DATETIME_FORMAT).unwrap();
        self.0.execute("
        INSERT INTO Messages (message_id, connection_id, time_sent, time_recieved, content_type, content) VALUES 
        (?1, (SELECT connection_id FROM Connections WHERE peer_id == ?2), ?3, ?4, ?5, ?6)", 
        (msg.message_id, msg.peer_id, msg.time_sent.format(datetime_format).unwrap(), get_now().format(datetime_format).unwrap(), content_type, blob))?;
        
        Ok(self.0.last_insert_rowid() as u32)
    }

    pub fn update_message_content(&self, msg_id: u32, content: MessageContent) -> Result<u32, Box<dyn std::error::Error>> {
        let content_type: &str;
        let blob: Vec<u8>;
        match content {
            MessageContent::Text(content) => {
                content_type = "TEXT";
                blob = content.into();
            }
        }
        self.0.execute("
        UPDATE Messages  SET content_type = ?1, content = ?2;
        WHERE message_id == ?3",
        (content_type, blob, msg_id))?;
        
        Ok(self.0.last_insert_rowid() as u32)
    }

    pub fn get_message(&self, message_id: u32) -> Result<Option<Message>, DbErr> {
        let mut stmt = self.0.prepare("
            SELECT message_id, peer_id, self_id, time_sent, time_recieved, content_type, content FROM Messages 
            NATURAL JOIN Connections
            WHERE message_id == ?1
        ;")?;
        let values_result = stmt.query_row([message_id], |row| {
            Ok((
                row.get::<usize, u32>(0)?,
                row.get::<usize, u32>(1)?,
                row.get::<usize, u32>(2)?,
                row.get::<usize, String>(3)?,
                row.get::<usize, Option<String>>(4)?,
                row.get::<usize, String>(5)?,
                row.get::<usize, Vec<u8>>(6)?,
            ))
        });

        if let Err(rusqlite::Error::QueryReturnedNoRows) = values_result {
            return Ok(None);
        }
        let values = values_result.unwrap();

        let datetime_format =  &format_description::parse(DATETIME_FORMAT).unwrap();

        let content = match values.5.as_str() {
            "TEXT" => {
                MessageContent::Text(String::from_utf8(values.6)?)
            }
            _ => {
                return Err(DbErr::InvalidMessageContentType)?;
            }
        };

        Ok(Some(Message {
            message_id: values.0,
            peer_id: values.1,
            self_id: values.2,
            time_sent: OffsetDateTime::parse(&values.3, datetime_format).unwrap(),
            time_recieved: values.4.map(|time_recieved| {
                OffsetDateTime::parse(&time_recieved, datetime_format).unwrap()
            }),
            content,
        }))
    }

    pub fn get_messages(&self, peer_id: u32, since: OffsetDateTime, untill: OffsetDateTime) -> Result<Vec<Message>, DbErr> {
        let datetime_format =  &format_description::parse(DATETIME_FORMAT).unwrap();
        let mut stmt = self.0.prepare("
            SELECT message_id, peer_id, self_id, time_sent, time_recieved, content_type, content FROM Messages
            NATURAL JOIN Connections
            WHERE peer_id = ?1 AND time_sent BETWEEN ?2 AND ?3
        ;")?;
        let rows = stmt.query_map((
            peer_id,
            since.format(datetime_format).unwrap(), 
            untill.format(datetime_format).unwrap()
        ), |row| {
            Ok((
                row.get::<usize, u32>(0)?,
                row.get::<usize, u32>(1)?,
                row.get::<usize, u32>(2)?,
                row.get::<usize, String>(3)?,
                row.get::<usize, Option<String>>(4)?,
                row.get::<usize, String>(5)?,
                row.get::<usize, Vec<u8>>(6)?,
            ))
        })?;

        let mut msgs = Vec::new();
        for values in rows {
            let values = values?;

            let content = match values.5.as_str() {
                "TEXT" => {
                    MessageContent::Text(String::from_utf8(values.6)?)
                }
                _ => {
                    return Err(DbErr::InvalidMessageContentType)?;
                }
            };

            msgs.push(Message {
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

        return Ok(msgs);
    }

    pub fn generate_peer_id(&self) -> Result<u32, DbErr> {      
        let id_sequence_path = Path::new("id_sequence.json");
        let mut id_sequence: IdSequence;
        if id_sequence_path.exists() {
            let seq_reader = BufReader::new(File::open(id_sequence_path)?);
            id_sequence = serde_json::from_reader::<BufReader<_>, IdSequence>(seq_reader).unwrap();
        } else {
            id_sequence = IdSequence::new();
        }

        let res: u32;
        if let Some(id) = id_sequence.next() {
            res = id;
        } else {
            return Err(DbErr::NoAvailableId);
        }

        let mut seq_writer = BufWriter::new(File::create(id_sequence_path)?);
        serde_json::to_writer_pretty(&mut seq_writer, &id_sequence).unwrap();
        seq_writer.flush().unwrap();

        return Ok(res);
    }
}
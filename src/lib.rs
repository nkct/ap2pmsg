use std::{
    net::{SocketAddr, TcpStream}, 
    io::{BufWriter, self, Write, BufReader, Read}, 
    error::Error, 
    fmt::{Debug, Display}, 
    string, 
    fs::{self, File}, 
    path::{Path, PathBuf}, str::from_utf8, 
};
use log::trace;
use serde::{Serialize, Deserialize};
use time::{OffsetDateTime, format_description};
use rand::rngs::OsRng;
use rand_unique::RandomSequenceBuilder;

pub const DATETIME_FORMAT: &str = "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3][offset_hour]:[offset_minute]";

pub trait Writable {
    fn write_into(&self, writer: &mut BufWriter<TcpStream>) -> Result<(), io::Error> where Self: Serialize {
        trace!("Writing {}", std::any::type_name::<Self>());
        let message = serde_json::to_string(self)?;
        let len = message.len() as u32;
        let mut send = Vec::new();
        send.extend_from_slice(&len.to_be_bytes());
        send.extend_from_slice(&message.as_bytes());
        writer.write(&send)?;
        writer.flush()?;
        trace!("Wrote: {:.*} with length {}", u8::MAX as usize, message, len);
        Ok(())
    }
}

pub trait Readable {
    fn read_from(reader: &mut BufReader<TcpStream>) -> Result<Self, Box<dyn Error>> 
    where Self: Sized + for<'a> Deserialize<'a> {
        trace!("Reading {}", std::any::type_name::<Self>());
        let mut len = [0;4];
        reader.read_exact(&mut len)?;
        let mut buf = vec![0; u32::from_be_bytes(len) as usize];
        reader.read_exact(&mut buf)?;
        trace!("Read {:.*} with length {}", u8::MAX as usize, from_utf8(&buf)?, u32::from_be_bytes(len));
        Ok(serde_json::from_str::<Self>(from_utf8(&buf)?)?) 
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum RefreshRequest {
    Connection,
    Message,
    Kill
}
impl Writable for RefreshRequest {}
impl Readable for RefreshRequest {}

#[derive(Serialize, Deserialize, Debug)]
pub enum InitialRequest {
    Frontend(BackendToFrontendRequest),
    Peer(PeerToPeerRequest)
}
impl Writable for InitialRequest {}
impl Readable for InitialRequest {}

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendToFrontendRequest {
    LinkingRequest,
    EstablishPeerConnection(SocketAddr),
    ListMessages(u32, OffsetDateTime, OffsetDateTime),
    ListPeerConnections,
    MessagePeer((u32, MessageContent)),
    RetryUnrecieved(u32),
    PingPeer(u32),
    KillRefresher
}
impl Writable for BackendToFrontendRequest {}
impl Readable for BackendToFrontendRequest {}

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendToFrontendResponse {
    LinkingResult(Result<(), String>),
    PeerConnectionsListed(Vec<Connection>),
    MessagesListed(Vec<Message>),
    InvalidRequest,
}
impl Writable for BackendToFrontendResponse {}
impl Readable for BackendToFrontendResponse {}

#[derive(Serialize, Deserialize, Debug)]
pub enum PeerToPeerRequest {
    ProposeConnection(u32, String, SocketAddr),
    Message(Message),
    BulkMessage(Vec<Message>),
}
impl Writable for PeerToPeerRequest {}
impl Readable for PeerToPeerRequest {}

#[derive(Serialize, Deserialize, Debug)]
pub enum PeerToPeerResponse {
    AcceptConnection(u32, String, SocketAddr),
    Recieved(u32),
    BulkRecieved(Vec<u32>),
}
impl Writable for PeerToPeerResponse {}
impl Readable for PeerToPeerResponse {}

#[derive(Serialize, Deserialize)]
pub enum MessageContent {
    Text(String),
    File((String, Vec<u8>)),
}
impl Debug for MessageContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageContent::Text(text) => {
                write!(f, "TEXT: {:?}", text)
            }
            MessageContent::File((filename, blob)) => {
                write!(f, "FILE: {:?} with length: {}", filename, blob.len())
            },
        }
    }
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
    UnableToSaveSequenceConfig(io::Error),
    FilenameTooLong,
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

pub struct DbConn {
    db_conn: rusqlite::Connection,
    file_storage_path: PathBuf,
}
impl DbConn {
    pub fn new(db_conn: rusqlite::Connection) -> Self {
        DbConn { db_conn, file_storage_path: "./files".into() }
    }

    pub fn table_exists(&self, table_name: &str) -> rusqlite::Result<bool> {
        Ok(self.db_conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = :table_name;")?
            .query([table_name])?.next().unwrap().is_some()
        )
    }    

    pub fn create_messages_table(&self) -> Result<usize, Box<dyn std::error::Error>> {
        Ok(self.db_conn.execute("
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
        Ok(self.db_conn.execute("
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
        let mut stmt = self.db_conn.prepare("INSERT INTO Connections 
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
        let mut stmt = self.db_conn.prepare("
            SELECT peer_addr FROM Connections 
            WHERE peer_id == ?1
        ;")?;
        let addr = stmt.query_row([peer_id], |row| {
            Ok(row.get::<usize, String>(0)?)
        })?;
        return Ok(addr.parse::<SocketAddr>().unwrap());
    }

    pub fn get_peer_name(&self, peer_id: u32) -> Result<String, DbErr>{
        let mut stmt = self.db_conn.prepare("
            SELECT peer_name FROM Connections 
            WHERE peer_id == ?1
        ;")?;
        let name = stmt.query_row([peer_id], |row| {
            Ok(row.get::<usize, String>(0)?)
        })?;
        return Ok(name);
    }

    pub fn peer_online(&self, peer_id: u32) -> Result<bool, DbErr>{
        let mut stmt = self.db_conn.prepare("
            SELECT online FROM Connections 
            WHERE peer_id == ?1
        ;")?;
        let status = stmt.query_row([peer_id], |row| {
            Ok(row.get::<usize, usize>(0)?)
        })?;
        return Ok(status == 1);
    }

    pub fn set_peer_online(&self, peer_id: u32, online: bool) -> Result<usize, DbErr> {
        let mut stmt = self.db_conn.prepare("
            UPDATE Connections SET online = ?1
            WHERE peer_id == ?2
        ;")?;
        Ok(stmt.execute((online, peer_id))?)
    }

    pub fn mark_as_recieved(&self, msg_id: u32) -> Result<usize, DbErr> {
        let datetime_format =  &format_description::parse(DATETIME_FORMAT).unwrap();
        let mut stmt = self.db_conn.prepare("
            UPDATE Messages SET time_recieved = ?1
            WHERE message_id == ?2
        ;")?;
        Ok(stmt.execute((get_now().format(datetime_format).unwrap(), msg_id))?)
    }

    pub fn bulk_mark_as_recieved(&self, msg_ids: Vec<u32>) -> Result<usize, DbErr> {
        let datetime_format =  &format_description::parse(DATETIME_FORMAT).unwrap();
        let placeholders: String = std::iter::repeat("?").take(msg_ids.len()).collect::<Vec<_>>().join(", ");
        let mut stmt = self.db_conn.prepare(&format!("
            UPDATE Messages SET time_recieved = ?1
            WHERE message_id IN ({})
        ;", placeholders))?;
        if msg_ids.is_empty() {
            return Ok(0);
        }
        let parameters = Some(get_now()
            .format(datetime_format)
            .unwrap())
            .into_iter()
            .chain(msg_ids
                .into_iter()
                .map(|id| id.to_string())
            );
        Ok(stmt.execute(rusqlite::params_from_iter(parameters))?)
    }

    pub fn get_connection(&self, connection_id: u32) -> Result<Connection, DbErr> {
        let mut stmt = self.db_conn.prepare("
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
        let mut stmt = self.db_conn.prepare("
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

    fn serialize_message_content(&self, content: &MessageContent) -> Result<(String, Vec<u8>), DbErr> {
        match content {
            MessageContent::Text(content) => {
                return Ok(("TEXT".to_owned(), content.to_owned().into_bytes()));
            }
            MessageContent::File((filename, blob)) => {
                if !&self.file_storage_path.exists() {
                    fs::create_dir(&self.file_storage_path).unwrap();
                }
                let filepath = [self.file_storage_path.clone(), filename.into()].iter().collect::<PathBuf>();
                fs::write(&filepath, blob).unwrap();

                return Ok(("FILE".to_owned(), filename.to_owned().into_bytes()));
            },
        }
    }

    pub fn new_message(&self, peer_id: u32, content: &MessageContent) -> Result<Message, Box<dyn std::error::Error>> {
        let (content_type, blob) = self.serialize_message_content(content)?;
        
        let datetime_format =  &format_description::parse(DATETIME_FORMAT).unwrap();
        self.db_conn.execute("
        INSERT INTO Messages (connection_id, time_sent, time_recieved, content_type, content) VALUES 
        ((SELECT connection_id FROM Connections WHERE peer_id == ?1), ?2, NULL, ?3, ?4)", 
        (peer_id, get_now().format(datetime_format).unwrap(), content_type, blob))?;
        
        let msg = self.get_message(self.db_conn.last_insert_rowid() as u32).unwrap().unwrap();
        return Ok(msg);
    }

    pub fn insert_message(&self, msg: &Message) -> Result<u32, Box<dyn std::error::Error>> {
        let (content_type, blob) = self.serialize_message_content(&msg.content)?;

        let datetime_format =  &format_description::parse(DATETIME_FORMAT).unwrap();
        self.db_conn.execute("
        INSERT INTO Messages (message_id, connection_id, time_sent, time_recieved, content_type, content) VALUES 
        (?1, (SELECT connection_id FROM Connections WHERE peer_id == ?2), ?3, ?4, ?5, ?6)", 
        (msg.message_id, msg.self_id, msg.time_sent.format(datetime_format).unwrap(), get_now().format(datetime_format).unwrap(), content_type, blob))?;
        
        Ok(self.db_conn.last_insert_rowid() as u32)
    }

    pub fn update_message_content(&self, msg_id: u32, content: MessageContent) -> Result<u32, Box<dyn std::error::Error>> {
        let (content_type, blob) = self.serialize_message_content(&content)?;

        self.db_conn.execute("
        UPDATE Messages  SET content_type = ?1, content = ?2;
        WHERE message_id == ?3",
        (content_type, blob, msg_id))?;
        
        Ok(self.db_conn.last_insert_rowid() as u32)
    }

    fn deserialize_message_content(&self, content_type: &str, blob: Vec<u8>) -> Result<MessageContent, DbErr> {
        match content_type {
            "TEXT" => {
                return Ok(MessageContent::Text(String::from_utf8(blob)?));
            }
            "FILE" => {
                let filename = String::from_utf8(blob)?;
                let file_content = fs::read([self.file_storage_path.clone(), filename.clone().into()].iter().collect::<PathBuf>())?;
                return Ok(MessageContent::File((filename, file_content)));
            }
            _ => {
                return Err(DbErr::InvalidMessageContentType)?;
            }
        };
    }

    pub fn get_message(&self, message_id: u32) -> Result<Option<Message>, DbErr> {
        let mut stmt = self.db_conn.prepare("
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

        let content = self.deserialize_message_content(values.5.as_str(), values.6)?;

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
        let mut stmt = self.db_conn.prepare("
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

            let content = self.deserialize_message_content(values.5.as_str(), values.6)?;
            
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

    pub fn get_unrecieved_for(&self, peer_id: u32) -> Result<Vec<Message>, DbErr> {
        let mut stmt = self.db_conn.prepare("
            SELECT message_id, peer_id, self_id, time_sent, time_recieved, content_type, content FROM Messages
            NATURAL JOIN Connections
            WHERE peer_id = ?1 AND time_recieved IS NULL
        ;")?;
        let rows = stmt.query_map((
            peer_id,
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

            let content = self.deserialize_message_content(values.5.as_str(), values.6)?;

            let datetime_format =  &format_description::parse(DATETIME_FORMAT).unwrap();
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
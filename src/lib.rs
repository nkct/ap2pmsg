use std::{net::{SocketAddr, TcpStream, IpAddr, Ipv4Addr}, io::{BufWriter, self, Write}, any::type_name, error::Error, fmt::Display};
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
    peer_name: String,
    peer_addr: SocketAddr,
    online: bool,
    time_established: OffsetDateTime,
    self_id: u64,
}
impl Connection {
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
    pub fn new(conn: DbConn, peer_id: u64) -> Self {
        Connection {
            peer_id,
            peer_name: conn.get_peer_name(peer_id).unwrap(),
            peer_addr: conn.get_peer_addr(peer_id).unwrap(),
            online: false,
            time_established: get_now(),
            self_id: conn.get_self_id(peer_id).unwrap(),
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
enum DbErr {
    IdNotUniqe,
    NoResults
}
impl Display for DbErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#}", self)
    }
}
impl Error for DbErr {}

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

    fn get_new_message_id(&self) -> rusqlite::Result<u64> {
        Ok(if let Some(row) = self.0
            .prepare("SELECT id FROM Messages ORDER BY id DESC LIMIT 1;")?
            .query([])?.next()? {
                // for some reason i've had trouble simply incrementing this
                let mut r = row.get(0)?;
                r += 1;
                r
        } else {
            0
        })
    }
    fn get_self_id(&self, peer_id: u64) -> Result<u64, Box<dyn Error>> {
        let mut stmt = self.0.prepare("SELECT self_id FROM Connections WHERE peer_id = :peer_id;")?;
        let mut results = stmt.query_map([peer_id], |row| {row.get::<usize, u64>(0)})?;

        if let Some(id) = results.next() {
            // the query returned multiple rows
            if results.next().is_some() {
                return Err(Box::new(DbErr::IdNotUniqe));
            }

            return Ok(id?);
        } else {
            return Err(Box::new(DbErr::NoResults));
        }
    }
    fn get_peer_name(&self, peer_id: u64) -> Result<String, Box<dyn Error>> {
        let mut stmt = self.0.prepare("SELECT peer_name FROM Connections WHERE peer_id = :peer_id;")?;
        let mut results = stmt.query_map([peer_id], |row| {row.get::<usize, String>(0)})?;

        if let Some(peer_name) = results.next() {
            // the query returned multiple rows
            if results.next().is_some() {
                return Err(Box::new(DbErr::IdNotUniqe));
            }

            return Ok(peer_name?);
        } else {
            return Err(Box::new(DbErr::NoResults));
        }
    }
    fn get_peer_addr(&self, peer_id: u64) -> Result<SocketAddr, Box<dyn Error>> {
        let mut stmt = self.0.prepare("SELECT peer_addr FROM Connections WHERE peer_id = :peer_id;")?;
        let mut results = stmt.query_map([peer_id], |row| {row.get::<usize, String>(0)})?;

        if let Some(peer_addr) = results.next() {
            // the query returned multiple rows
            if results.next().is_some() {
                return Err(Box::new(DbErr::IdNotUniqe));
            }

            return Ok(peer_addr?.parse()?);
        } else {
            return Err(Box::new(DbErr::NoResults));
        }
    }

    pub fn create_messages_table(&self) -> Result<usize, Box<dyn std::error::Error>> {
        Ok(self.0.execute("
        CREATE TABLE Messages (
            message_id INTEGER, 
            connection_id INTEGER,
            recieved INTEGER DEFAULT 0, 
            time_recieved TEXT, 
            time_sent TEXT DEFAULT CURRENT_TIMESTAMP, 
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
            peer_name TEXT, 
            peer_addr TEXT, 
            online INTEGER DEFAULT 1, 
            time_established TEXT DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (connection_id)
        );", ())?)
    }

    pub fn _table_from_struct<T: Serialize>(&self, t: T) -> Result<(), Box<dyn std::error::Error>> {
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
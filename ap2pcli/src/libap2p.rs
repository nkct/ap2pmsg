#![allow(dead_code)]

use std::{slice, str};

#[link(name = "ap2p")]
#[link(name = "sqlite3")]
extern "C" {
    fn ap2p_strlen(s: *const u8) -> usize;
    fn ap2p_list_connections(buf: *const Connection, buf_len: &i32) -> i32;
    fn ap2p_list_messages(buf: *const Message, buf_len: &i32) -> i32;
    fn ap2p_request_connection(addr: *const u8, port: i32) -> i32;
    fn ap2p_select_connection(conn_id: u64) -> i32;
    fn ap2p_decide_on_connection(conn_id: u64, decison: i32) -> i32;
    fn ap2p_listen() -> i32;
}

#[repr(i8)]
#[derive(Debug, PartialEq)]
pub enum ConnStatus {
    Rejected = -1,
    Accepted = 0,
    Pending = 1,
}
#[repr(C)]
#[derive(Debug)]
pub struct Connection {
    conn_id: i64,
    peer_id: i64,
    self_id: i64,
    // TODO: check if these raw ptrs need manual freeing
    peer_name: *const u8,
    peer_addr: *const u8,
    peer_port: i32,
    online: bool,
    requested_at: i64,
    resolved_at: i64,
    status: ConnStatus,
}
impl Connection {
    pub fn get_peer_name(&self) -> Option<&str> {
        if self.status == ConnStatus::Accepted {
            let peer_name = unsafe { str::from_utf8_unchecked(slice::from_raw_parts(self.peer_name, ap2p_strlen(self.peer_name))) };
            return Some(peer_name);
        } else {
            return None;
        }
    }
    pub fn get_peer_addr(&self) -> &str {
        unsafe { str::from_utf8_unchecked(slice::from_raw_parts(self.peer_addr, ap2p_strlen(self.peer_addr))) }
    }
}

pub fn list_connections(max: i32) -> Result<Vec<Connection>, ()> {
    let buf_len: i32 = max;
    let mut buf = Vec::with_capacity(buf_len as usize);
    
    unsafe { 
        if ap2p_list_connections(buf.as_ptr(), &buf_len)!=0 {
            return Err(());
        }
        buf.set_len(buf_len as usize);
    }
    
    return Ok(buf);
}

#[repr(C)]
#[derive(Debug)]
pub struct Message {
    msg_id: i64,
    conn_id: i64,
    time_sent: i64,
    time_recieved: i64,
    content_type: u8,
    content_len: i32,
    content: *const u8,
}
impl Message {
    pub fn get_content(&self) -> Vec<u8> {
        let content;
        unsafe {
            content = slice::from_raw_parts(self.content, self.content_len as usize).to_vec();
        }
        return content;
    }
}

pub fn list_messages(max: i32) -> Result<Vec<Message>, ()> {
    let buf_len: i32 = max;
    let mut buf = Vec::with_capacity(buf_len as usize);
    
    unsafe { 
        if ap2p_list_messages(buf.as_ptr(), &buf_len)!=0 {
            return Err(());
        }
        buf.set_len(buf_len as usize);
    }
    
    return Ok(buf);
}

pub fn request_connection(addr: &str, port: i32) -> i32 {
    return unsafe { ap2p_request_connection(addr.as_ptr(), port) }
}

pub fn select_connection(conn_id: u64) -> i32 {
    return unsafe { ap2p_select_connection(conn_id) }
}

pub fn decide_on_connection(conn_id: u64, decision: i32) -> i32 {
    return unsafe { ap2p_decide_on_connection(conn_id, decision) }
}

pub fn listen() -> i32 {
    return unsafe { ap2p_listen() }
}
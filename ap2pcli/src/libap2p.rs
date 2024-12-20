#![allow(dead_code)]

use std::{
    ffi::{c_char, c_void, CStr, CString}, fmt, ptr, slice, str 
};
use chrono::prelude::*;

extern "C" {
    fn ap2p_strlen(s: *const i8) -> usize;
    fn ap2p_free(p: *const c_void);
    fn ap2p_list_connections(buf: *const Connection, buf_len: &i32) -> i32;
    fn ap2p_list_messages(buf: *const Message, buf_len: &i32) -> i32;
    fn ap2p_request_connection(addr: *const c_char, port: i32) -> i32;
    fn ap2p_decide_on_connection(conn_id: u64, decison: i32) -> i32;
    fn ap2p_listen() -> i32;
    fn ap2p_state_get(db: *const c_void, key: *const c_char) -> *const c_char;
    fn ap2p_state_set(db: *const c_void, key: *const c_char, value: *const c_char) -> i32;
    fn ap2p_send_message(content_type: u8, content_len: i32, content: *const u8) -> i32;
}

#[repr(i8)]
#[derive(Debug, PartialEq)]
pub enum ConnStatus {
    Rejected = -1,
    Accepted = 0,
    Pending = 1,
    SelfReview = 2,
    PeerReview = 3,
}
#[repr(C)]
#[derive(Debug)]
pub struct Connection {
    conn_id: i64,
    peer_id: i64,
    self_id: i64,
    // TODO: check if these raw ptrs need manual freeing
    peer_name: *const i8,
    peer_addr: *const i8,
    peer_port: i32,
    online: bool,
    requested_at: i64,
    updated_at: i64,
    status: ConnStatus,
}
impl fmt::Display for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let requested_at = DateTime::from_timestamp(self.requested_at, 0).expect("INVALID TIMESTAMP");
        let updated_at = if self.status != ConnStatus::Pending {
            Some(DateTime::from_timestamp(self.updated_at, 0).expect("INVALID TIMESTAMP"))
        } else {
            None
        };
        
        let self_id = if self.status == ConnStatus::Accepted || self.status == ConnStatus::SelfReview {
            Some(self.self_id)
        } else {
            None
        };
        
        let peer_id = if self.status == ConnStatus::SelfReview || self.status == ConnStatus::Rejected {
            None
        } else {
            Some(self.peer_id)
        };
        
        write!(f, "\
            Connection with '{:?}' at {}:{} {{\n\
            \x20\x20\x20\x20Connection ID: {}, \n\
            \x20\x20\x20\x20Peer ID: {:?}, \n\
            \x20\x20\x20\x20Self ID: {:?}, \n\
            \x20\x20\x20\x20Online:  {}, \n\
            \x20\x20\x20\x20Status:  {:?}, \n\
            \x20\x20\x20\x20Requested at: {}, \n\
            \x20\x20\x20\x20Updated   at: {:?}, \n\
            }}\
            ", 
            self.get_peer_name(), 
            self.get_peer_addr(), 
            self.peer_port, 
            self.conn_id, 
            peer_id, 
            self_id, 
            self.online, 
            self.status, 
            requested_at.to_string(), 
            updated_at
        )
    }
}
impl Connection {
    pub fn get_peer_name(&self) -> Option<&str> {
        if self.status == ConnStatus::Accepted || self.status == ConnStatus::SelfReview {
            let peer_name = unsafe { CStr::from_ptr(self.peer_name).to_str().expect("self.peer_name not valid &str") };
            return Some(peer_name);
        } else {
            return None;
        }
    }
    pub fn get_peer_addr(&self) -> &str {
        unsafe { CStr::from_ptr(self.peer_addr).to_str().expect("self.peer_addr not valid &str") }
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
    shared_msg_id: i64,
    time_sent: i64,
    time_recieved: i64,
    content_type: u8,
    content_len: i32,
    content: *const u8,
}
impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let time_sent = DateTime::from_timestamp(self.time_sent, 0).expect("INVALID TIMESTAMP");
        let time_recieved = if self.time_recieved > 0 {
            Some(DateTime::from_timestamp(self.time_recieved, 0).expect("INVALID TIMESTAMP"))
        } else {
            None
        };
        write!(f, "\
            Message {} on connection {} of type {}, \n\
            Sent at {} and recieved at {:?}, \n\
            Content: {:?} \
            ",
            self.shared_msg_id,
            self.conn_id,
            self.content_type,
            time_sent, 
            time_recieved,
            self.get_content()
        )
    }
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
    unsafe { 
        let addr_c = CString::new(addr).expect("addr not valid CString");
        return ap2p_request_connection(addr_c.as_ptr(), port) 
    }
}

pub fn select_connection(conn_id: u64) -> i32 {
    return state_set("selected_conn", &conn_id.to_string());
}

pub fn decide_on_connection(conn_id: u64, decision: i32) -> i32 {
    return unsafe { ap2p_decide_on_connection(conn_id, decision) }
}

pub fn send_text_message(text: &str) -> i32 {
    return unsafe { ap2p_send_message(0, text.len() as i32, text.as_ptr()) }
}

pub fn listen() -> i32 {
    return unsafe { ap2p_listen() }
}

pub fn state_get(key: &str) -> Option<String> {
    unsafe { 
        let key_c = CString::new(key).expect("key not valid CString");
        let value_ptr = ap2p_state_get(ptr::null(), key_c.as_ptr());
        if value_ptr.is_null() {
            return None;
        }
        
        let value = CStr::from_ptr(value_ptr).to_str().expect("value not valid &str").to_owned();
        ap2p_free(value_ptr as *const c_void);
        return Some(value);
    }
}

pub fn state_set(key: &str, value: &str) -> i32 {
    unsafe { 
        let key_c = CString::new(key).expect("key not valid CString");
        let value_c = CString::new(value).expect("value not valid CString");
        
        return ap2p_state_set(ptr::null(), key_c.as_ptr(), value_c.as_ptr());
    }
}
use std::{slice, str};

#[link(name = "ap2p")]
#[link(name = "sqlite3")]
extern "C" {
    fn ap2p_strlen(s: *const u8) -> usize;
    fn ap2p_list_connections(buf: *const Connection, buf_len: &i32) -> i32;
}

#[repr(C)]
#[derive(Debug)]
pub struct Connection {
    conn_id: u64,
    peer_id: u64,
    self_id: u64,
    // check if these raw ptrs need manual freeing
    peer_name: *const u8,
    peer_addr: *const u8,
    online: bool,
    time_established: u64,
}
impl Connection {
    pub fn get_peer_name(&self) -> &str {
        unsafe { str::from_utf8_unchecked(slice::from_raw_parts(self.peer_name, ap2p_strlen(self.peer_name))) }
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
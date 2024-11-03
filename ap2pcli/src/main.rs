use std::env;

mod libap2p;

fn log(msg: &str, file: &str, line: u32) {
    const RED:    &str = "\x1b[31m";
    const GREEN:  &str = "\x1b[32m";
    const YELLOW: &str = "\x1b[33m";
    const BLUE:   &str = "\x1b[34m";
    const PURPLE: &str = "\x1b[35m";
    const CYAN:   &str = "\x1b[36m";
    const ENDC:   &str = "\x1b[0m";
    
    let color: Option<&str>;
    match msg {
        _ if msg.starts_with("DEBUG:") => { color=Some(CYAN  ) }
        _ if msg.starts_with("INFO:" ) => { color=Some(BLUE  ) }
        _ if msg.starts_with("OK:"   ) => { color=Some(GREEN ) }
        _ if msg.starts_with("WARN:" ) => { color=Some(YELLOW) }
        _ if msg.starts_with("ERROR:") => { color=Some(RED   ) }
        _ if msg.starts_with("TEST:" ) => { color=Some(PURPLE) }
        _ => { color=None }
    }
    
    print!("{}:{} ", file, line);
    if let Some(code) = color { 
        let mut msg_parts = msg.split(":");
        print!("{code}");
        print!("{}", msg_parts.next().unwrap());
        print!("{ENDC}");
        print!(":{}", msg_parts.next().unwrap());
        print!("\n");
    } else {
        println!("{}", msg);
    }
}
macro_rules! log {
    ($e:expr) => {
        { log(&format!($e), file!(), line!()); }
    };
}


fn main() -> Result<(), isize> {
    let mut args = env::args();
    let prog_path = args.next().expect("ARGS CANNOT BE EMPTY");
    let mut next_arg = || -> Result<String, isize> {
        if let Some(arg) = args.next() {
            return Ok(arg);
        } else {
            log!("ERROR: not enough arguments, see `{prog_path} help` for usage info");
            return Err(-1);
        }
    };
    
    let command = next_arg()?;
    match command.as_str() {
        "conn" | "conns" | "connection" | "connections" => { 
            match next_arg()?.as_str() {
                "l" | "-l" | "list"    | "--list"    => { 
                    let conns = libap2p::list_connections(5).expect("could not list conns");
                    
                    println!("conn_count: {}", conns.len());
                    for conn in conns {
                        println!("{:#?}", conn);
                        println!("peer_name: {:?}", conn.get_peer_name());
                        println!("peer_addr: {:?}", conn.get_peer_addr());
                    }
                }
                "s" | "-s" | "select"  | "--select"  => { 
                    if let Ok(id) = next_arg()?.parse::<u64>() {
                        println!("ID: {id}");
                        let res = libap2p::select_connection(id);
                        println!("select_connection result: {res}"); 
                    } else {
                        log!("ERROR: <ID> must be a valid integer");
                        return Err(-1);
                    }
                }
                "r" | "-r" | "request" | "--request" => { 
                    let addr_port = next_arg()?;
                    if let Some((addr, port_str)) = addr_port.split_once(":") {
                        let port;
                        if let Ok(p) = port_str.parse::<i32>() {
                            port = p;
                        } else {
                            log!("ERROR: <PORT> must be a valid integer");
                            return Err(-1);
                        }
                        
                        println!("ADDR: {addr}"); 
                        println!("PORT: {port}"); 
                        let res = libap2p::request_connection(addr, port);
                        println!("request_connection result: {res}"); 
                    } else {
                        log!("ERROR: could not split the provided addres into <ADDR> and <PORT>");
                        return Err(-1);
                    }
                }
                "d" | "-d" | "decide" | "--decide" => {
                    if let Ok(id) = next_arg()?.parse::<u64>() {
                        println!("ID: {id}");
                        let res;
                        match next_arg()?.as_str() {
                            "y" | "yes" | "a" | "acc" | "accept" => { res = libap2p::decide_on_connection(id,  0); }
                            "n" | "no"  | "r" | "rej" | "reject" => { res = libap2p::decide_on_connection(id, -1);}
                            _ => {
                                log!("ERROR: <DECISION> must be either [\"y\", \"yes\", \"a\", \"acc\", \"accept\"] or [\"n\", \"no\", \"r\", \"rej\", \"reject\"]");
                                return Err(-1);
                            }
                        }
                        println!("decide_on_connection result: {res}"); 
                    }
                }
                subcommand => {
                    log!("ERROR: '{subcommand}' is not a recognized subcommand for {command}, see `{prog_path} help` for usage info");
                    return Err(-1);
                }
            }
        }
        "msg"  | "msgs"  | "message"    | "messages"    => { 
            match next_arg()?.as_str() {
                "l" | "-l" | "list" | "--list" => { 
                    let msgs = libap2p::list_messages(5).expect("could not list msgs");
                    
                    println!("msgs_count: {}", msgs.len());
                    for msg in msgs {
                        println!("{:#?}", msg);
                        println!("content: {:?}", msg.get_content());
                    }
                }
                "s" | "-s" | "send" | "--send" => { 
                    let msg = next_arg()?;
                    println!("MSG: {msg}"); 
                    let res = libap2p::send_text_message(&msg);
                    println!("send_text_message result: {res}"); 
                }
                "b" | "-b" | "bulk" | "--bulk" => { 
                    let msgs = next_arg()?;
                    println!("MSGS: {msgs}"); todo!("sending bulk messages")
                }
                subcommand => {
                    log!("ERROR: '{subcommand}' is not a recognized subcommand for {command}, see `{prog_path} help` for usage info");
                    return Err(-1);
                }
            }
        }
        "listen" | "l" => {
            let res = libap2p::listen();
            println!("Finished listening with {}", res);
        }
        "state" => {
            match next_arg()?.as_str() {
                "g" | "get" => {
                    let key = next_arg()?;
                    if let Some(value) = libap2p::state_get(&key) {
                        println!("[STATE] {}: {}", key, value);
                    } else {
                        log!("ERROR: failed to get '{key}' from State");
                        return Err(-1);
                    }
                }
                "s" | "set" => {
                    let key = next_arg()?;
                    let value = next_arg()?;
                    let res = libap2p::state_set(&key, &value);
                    println!("state_set result: {res}"); 
                }
                subcommand => {
                    log!("ERROR: '{subcommand}' is not a recognized subcommand for {command}, see `{prog_path} help` for usage info");
                    return Err(-1);
                }
            }
        }
        "h" | "help" | "-h"    | "--help"                     => { 
            println!("Usage: {prog_path} [conn | conns | connection | connections] [l | -l | list    | --list   ]");
            print!("{}", " ".repeat(50 + prog_path.len()));              println!("[s | -s | select  | --select ] <ID>");
            print!("{}", " ".repeat(50 + prog_path.len()));              println!("[r | -r | request | --request] <ADDR>");
            print!("{}", " ".repeat(50 + prog_path.len()));              println!("[d | -d | decide  | --decide ] <ID> <DECISION>");
            println!();
            println!("       {prog_path} [msg  | msgs  | message    | messages   ] [l | -l | list  | --list ]");
            print!("{}", " ".repeat(50 + prog_path.len()));              println!("[s | -s | send  | --send ] <MSG>");
            print!("{}", " ".repeat(50 + prog_path.len()));              println!("[b | -b | bulk  | --bulk ] <MSGS>");
            println!();
            println!("       {prog_path} [help | -h    | --help   ]");
            println!("       └─> Print this message and exit.");
        }
        _ => {
            log!("ERROR: '{command}' is not a recognized command, see `{prog_path} help` for usage info");
            return Err(-1);
        }
    }
    
    return Ok(());
}

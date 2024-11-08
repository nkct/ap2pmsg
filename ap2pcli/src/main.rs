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
                    let conns_len = conns.len();
                    
                    log!("OK: retrieved {conns_len} connections");
                    for conn in conns {
                        println!("{}", conn);
                    }
                }
                "s" | "-s" | "select"  | "--select"  => { 
                    if let Ok(id) = next_arg()?.parse::<u64>() {
                        log!("DEBUG: <ID>={id}");
                        if libap2p::select_connection(id) == 0 {
                            log!("OK: succesfully selected connection");
                        } else {
                            log!("ERROR: failed to select connection");
                            return Err(-1);
                        }
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
                        
                        log!("DEBUG: <ADDR>='{addr}'"); 
                        log!("DEBUG: <PORT>='{port}'"); 
                        if libap2p::request_connection(addr, port) == 0 {
                            log!("OK: succesfully requested connection");
                        } else {
                            log!("ERROR: failed to request connection");
                            return Err(-1);
                        }
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
                        if res == 0 {
                            log!("OK: succesfully decided on connection");
                        } else {
                            log!("ERROR: failed to decide on connection");
                            return Err(-1);
                        }
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
                    let msgs_len = msgs.len();
                    
                    log!("OK: retrieved {msgs_len} messages");
                    for msg in msgs {
                        println!("{}", msg);
                    }
                }
                "s" | "-s" | "send" | "--send" => { 
                    let msg = next_arg()?;
                    log!("DEBUG: <MSG>='{msg}'"); 
                    if libap2p::send_text_message(&msg) == 0 {
                        log!("OK: succesfully sent message");
                    } else {
                        log!("ERROR: failed to send message");
                        return Err(-1);
                    }
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
        "l" | "listen" | "await" => {
            if libap2p::listen() == 0 {
                log!("OK: finished listening");
            } else {
                log!("ERROR: error while listening");
                return Err(-1);
            }
        }
        "s" | "state" => {
            let param = next_arg()?;
            match param.split_once("=") {
                None => {
                    if let Some(value) = libap2p::state_get(&param) {
                        println!("{}: {:?}", param, value);
                    } else {
                        log!("ERROR: failed to get from state");
                        return Err(-1);
                    }
                }
                Some((key, value)) => {
                    if libap2p::state_set(&key, &value) == 0 {
                        log!("OK: succesfully set to state");
                    } else {
                        log!("ERROR: failed to set to state");
                        return Err(-1);
                    }
                }
            }
        }
        "h" | "help" | "-h"    | "--help"                     => { 
            println!("Usage: ap2pcli [conn | conns | connection | connections] [l | -l | list    | --list   ]");
            print!("{}", " ".repeat(57));              println!("[s | -s | select  | --select ] <ID>");
            print!("{}", " ".repeat(57));              println!("[r | -r | request | --request] <ADDR>");
            print!("{}", " ".repeat(57));              println!("[d | -d | decide  | --decide ] <ID> <DECISION>");
            println!();
            println!("       ap2pcli [msg  | msgs  | message    | messages   ] [l | -l | list  | --list ]");
            print!("{}", " ".repeat(57));              println!("[s | -s | send  | --send ] <MSG>");
            print!("{}", " ".repeat(57));              println!("[b | -b | bulk  | --bulk ] <MSGS>");
            println!();
            println!("       ap2pcli [l | listen | await       ]");
            println!("       ┗━▶ Listen for incoming connections and messages. Can be closed with Enter.");
            println!("       ap2pcli [s | state                ]");
            println!("       ┗━▶ Provide a key to get it's value from State; Use key=value syntax to set a value for a given key.");
            println!("       ap2pcli [h | help | -h    | --help]");
            println!("       ┗━▶ Print this message and exit.");
        }
        _ => {
            log!("ERROR: '{command}' is not a recognized command, see `{prog_path} help` for usage info");
            return Err(-1);
        }
    }
    
    return Ok(());
}

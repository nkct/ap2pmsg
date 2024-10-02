use std::env;

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
    println!("args: {:?}", args);
    
    let path = args.next().expect("ARGS CANNOT BE EMPTY");
    
    let verb = args.next();
    if verb.is_none() {
        log!("ERROR: not enough arguments, see `{path} help` for usage info");
        return Err(-1);
    }
    match verb.unwrap().as_str() {
        "conn" | "conns" | "connection" | "connections" => { todo!() }
        "msg"  | "msgs"  | "message"    | "messages"    => { todo!() }
        "help" | "-h"    | "--help"                     => { todo!() }
        _ => {
            log!("ERROR: unrecognized verb, see `{path} help` for usage info");
            return Err(-1);
        }
    }
    
    return Ok(());
}

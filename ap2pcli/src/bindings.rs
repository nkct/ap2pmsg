#[link(name = "ap2p")]
extern "C" {
    fn ap2p_print_hello();
}

pub fn print_hello() {
    return unsafe { ap2p_print_hello(); }
}
#include <stdio.h>
#include <string.h>
#include <stdbool.h> 

unsigned long ap2p_strlen(const char* s) {
    return strlen(s);
}
 
typedef struct Connection {
    unsigned long peer_id;
    unsigned long self_id;
    const char* peer_name;
    const char* peer_addr;
    bool online;
    unsigned long time_established;
} Connection;

void ap2p_list_connections(Connection* buf, int* buf_len) {
    printf("buf_len: %d\n", *buf_len);
    
    Connection conn = {
        .peer_id = 0,
        .self_id = 0,
        .peer_name = "test_peer",
        .peer_addr = "test_addr",
        .online = 1,
        .time_established = 0
    };
    for (int i = 0; i < *buf_len; i++) {
        buf[i] = conn;
    }
}
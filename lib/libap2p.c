#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <stdbool.h> 
#include <sqlite3.h>

#define ERROR "\x1b[31mERROR\x1b[0m"

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

int ap2p_list_connections(Connection* buf, int* buf_len) {
    // Connection conn = {
    //     .peer_id = 0,
    //     .self_id = 0,
    //     .peer_name = "test_peer",
    //     .peer_addr = "test_addr",
    //     .online = 1,
    //     .time_established = 0
    // };
    // for (int i = 0; i < *buf_len; i++) {
    //     buf[i] = conn;
    // }
    
    sqlite3 *db;
    char* errmsg = 0;
    
    if ( sqlite3_open("ap2p_storage.db", &db) ) {
        printf(ERROR": could not open database\n");
        return -1;
    }
    
    const char* create_conns_sql = ""
    "CREATE TABLE Connections ("
        "connection_id INTEGER, "
        "peer_id INTEGER NOT NULL UNIQUE, "
        "self_id INTEGER NOT NULL, "
        "peer_name TEXT NOT NULL, "
        "peer_addr TEXT NOT NULL, "
        "online INTEGER DEFAULT 1, "
        "time_established INTEGER DEFAULT unixepoch NOT NULL,"
        "PRIMARY KEY (connection_id)"
    ");";
    if ( sqlite3_exec(db, create_conns_sql, NULL, NULL, &errmsg) != SQLITE_OK) {
        printf(ERROR": could not create the Connections table; %s\n", errmsg);
        sqlite3_free(errmsg);
        return -1;
    }
    
    if ( sqlite3_close(db) ) {
        printf(ERROR": could not close database\n");
        return -1;
    }
    
    return 0;
}
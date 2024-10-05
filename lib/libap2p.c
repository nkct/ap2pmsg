#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <stdbool.h> 
#include <sqlite3.h>

#define ERROR "\x1b[31mERROR\x1b[0m"
#define INFO  "\x1b[34mINFO\x1b[0m"

unsigned long ap2p_strlen(const char* s) {
    return strlen(s);
}
 
typedef struct Connection {
    unsigned long connection_id;
    unsigned long peer_id;
    unsigned long self_id;
    const char* peer_name;
    const char* peer_addr;
    bool online;
    unsigned long time_established;
} Connection;

int create_conn_table(sqlite3* db) {
    printf(INFO": creating Connections table\n");
    
    char* errmsg = 0;
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
    if ( sqlite3_exec(db, create_conns_sql, NULL, NULL, &errmsg) != SQLITE_OK ) {
        printf(ERROR": could not create the Connections table; %s\n", errmsg);
        sqlite3_free(errmsg);
        return -1;
    }
    return 0;
}

int ap2p_list_connections(Connection* buf, int* buf_len) {
    sqlite3 *db;
    
    if ( sqlite3_open("ap2p_storage.db", &db) ) {
        printf(ERROR": could not open database\n");
        return -1;
    }
    
    sqlite3_stmt *conn_stmt;
    while ( sqlite3_prepare_v2(db, "SELECT * FROM Connections", -1, &conn_stmt, NULL) != SQLITE_OK ) {
        if ( strncmp(sqlite3_errmsg(db), "no such table", 14) != 0 ) {
            if ( create_conn_table(db) != SQLITE_OK ) {
                sqlite3_close(db);
                return -1;
            } else {
                continue;
            }
        }
        printf(ERROR": could not SELECT * FROM Connections; %s (%d)\n", sqlite3_errmsg(db), sqlite3_errcode(db));
        sqlite3_close(db);
        return -1;
    }
    
    int res;
    int row_count = 0;
    while ( (res = sqlite3_step(conn_stmt)) == SQLITE_ROW ) {
        char* peer_name = sqlite3_malloc(sqlite3_column_bytes(conn_stmt, 3));
        sprintf(peer_name, "%s", sqlite3_column_text(conn_stmt, 3));
        
        char* peer_addr = sqlite3_malloc(sqlite3_column_bytes(conn_stmt, 4));
        sprintf(peer_addr, "%s", sqlite3_column_text(conn_stmt, 4));
        
        Connection conn = {
            .connection_id    = sqlite3_column_int64(conn_stmt, 0),
            .peer_id          = sqlite3_column_int64(conn_stmt, 1),
            .self_id          = sqlite3_column_int64(conn_stmt, 2),
            .peer_name        =  peer_name,
            .peer_addr        =  peer_addr,
            .online           =   sqlite3_column_int(conn_stmt, 5),
            .time_established = sqlite3_column_int64(conn_stmt, 6)
        };
        buf[row_count] = conn;
        row_count += 1;
    }
    if ( res != SQLITE_DONE ) {
        printf(ERROR": failed while iterating conn result; with code %d\n", res);
        sqlite3_close(db);
        return -1;
    }
    sqlite3_finalize(conn_stmt);
    *buf_len = row_count;
    
    sqlite3_close(db);
    return 0;
}
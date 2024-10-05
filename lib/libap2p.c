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
        "time_established INTEGER DEFAULT unixepoch NOT NULL, "
        "PRIMARY KEY (connection_id)"
    ");";
    if ( sqlite3_exec(db, create_conns_sql, NULL, NULL, &errmsg) != SQLITE_OK ) {
        printf(ERROR": could not create the Connections table; %s\n", errmsg);
        sqlite3_free(errmsg);
        return -1;
    }
    return 0;
}

int create_msg_table(sqlite3* db) {
    printf(INFO": creating Messages table\n");
    
    char* errmsg = 0;
    const char* create_msgs_sql = ""
    "CREATE TABLE Messages ("
        "message_id INTEGER, "
        "connection_id INTEGER, "
        "time_sent INTEGER DEFAULT unixepoch, "
        "time_recieved INTEGER, "
        "content_type INTEGER NOT NULL, "
        "content BLOB, "
        "PRIMARY KEY (message_id), "
        "FOREIGN KEY (connection_id) REFERENCES Connections(connection_id)"
    ");";
    if ( sqlite3_exec(db, create_msgs_sql, NULL, NULL, &errmsg) != SQLITE_OK ) {
        printf(ERROR": could not create the Messages table; %s\n", errmsg);
        sqlite3_free(errmsg);
        return -1;
    }
    return 0;
}

typedef struct Connection {
    unsigned long conn_id;
    unsigned long peer_id;
    unsigned long self_id;
    const char* peer_name;
    const char* peer_addr;
    bool online;
    unsigned long time_established;
} Connection;

typedef struct Message {
    unsigned long msg_id;
    unsigned long conn_id;
    unsigned long time_sent;
    unsigned long time_recieved;
    unsigned char content_type;
    unsigned long content_len;
    const unsigned char* content;
} Message;

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
            .conn_id    = sqlite3_column_int64(conn_stmt, 0),
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

int ap2p_list_messages(Message* buf, int* buf_len) {
    sqlite3 *db;
    
    if ( sqlite3_open("ap2p_storage.db", &db) ) {
        printf(ERROR": could not open database\n");
        return -1;
    }
    
    sqlite3_stmt *msg_stmt;
    while ( sqlite3_prepare_v2(db, "SELECT * FROM Messages", -1, &msg_stmt, NULL) != SQLITE_OK ) {
        if ( strncmp(sqlite3_errmsg(db), "no such table", 14) != 0 ) {
            if ( create_msg_table(db) != SQLITE_OK ) {
                sqlite3_close(db);
                return -1;
            } else {
                continue;
            }
        }
        printf(ERROR": could not SELECT * FROM Messages; %s (%d)\n", sqlite3_errmsg(db), sqlite3_errcode(db));
        sqlite3_close(db);
        return -1;
    }
    
    int res;
    int row_count = 0;
    while ( (res = sqlite3_step(msg_stmt)) == SQLITE_ROW ) {
        unsigned long content_len = sqlite3_column_bytes(msg_stmt, 6);
        unsigned char* content = sqlite3_malloc(content_len);
        memcpy(content, sqlite3_column_blob(msg_stmt, 6), content_len);
        
        Message msg = {
            .msg_id        = sqlite3_column_int64(msg_stmt, 0),
            .conn_id       = sqlite3_column_int64(msg_stmt, 1),
            .time_sent     = sqlite3_column_int64(msg_stmt, 2),
            .time_recieved = sqlite3_column_int64(msg_stmt, 3),
            .content_type  =   sqlite3_column_int(msg_stmt, 4),
            .content_len   = content_len,
            .content       =  sqlite3_column_blob(msg_stmt, 5),
        };
        buf[row_count] = msg;
        row_count += 1;
    }
    if ( res != SQLITE_DONE ) {
        printf(ERROR": failed while iterating conn result; with code %d\n", res);
        sqlite3_close(db);
        return -1;
    }
    sqlite3_finalize(msg_stmt);
    *buf_len = row_count;
    
    sqlite3_close(db);
    return 0;
}
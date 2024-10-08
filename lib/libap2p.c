#include <time.h>
#include <errno.h>
#include <stdio.h>
#include <unistd.h>
#include <string.h>
#include <stdlib.h>
#include <stdbool.h> 
#include <sqlite3.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>

#define ERROR "\x1b[31mERROR\x1b[0m"
#define WARN "\x1b[33mWARN\x1b[0m"
#define INFO  "\x1b[34mINFO\x1b[0m"
#define DEBUG  "\x1b[36mDEBUG\x1b[0m"

#define DB_FILE "ap2p_storage.db"

#define CONN_STATUS_REJECTED -1
#define CONN_STATUS_ACCEPTED 0
#define CONN_STATUS_PENDING 1

#define DEFAULT_PORT 7676

#define MAX_HOST_NAME 64 // in bytes

#define PARCEL_CONN_EST_KIND 1 // establish conn
#define PARCEL_CONN_EST_LEN 73 // 1 + 8 + 64

#define PARCEL_CONN_REJ_KIND 2 // reject conn establish attempt
#define PARCEL_CONN_REJ_LEN  1 // just kind

/* Reverse the byte order of an unsigned short. */
#define revbo_u16(d) ( ((d&0xff)<<8)|(d>>8) )

#define startswith(str, pat) strncmp((str), (pat), strlen((pat)))

unsigned long ap2p_strlen(const char* s) {
    return strlen(s);
}
 
int create_conn_table(sqlite3* db) {
    printf(INFO": creating Connections table\n");
    
    char* errmsg = 0;
    const char* create_conns_sql = ""
    "CREATE TABLE Connections ("
        "conn_id INTEGER PRIMARY KEY, "
        "peer_id INTEGER NOT NULL UNIQUE, "
        "self_id INTEGER, "
        "peer_name TEXT, "
        "peer_addr TEXT NOT NULL, "
        "online INTEGER DEFAULT 0, "
        "requested_at INTEGER DEFAULT unixepoch NOT NULL, "
        "resolved_at INTEGER, "
        "status INTEGER DEFAULT 1 NOT NULL"
    ");";
    if ( sqlite3_exec(db, create_conns_sql, NULL, NULL, &errmsg) != SQLITE_OK ) {
        printf(ERROR": could not create the Connections table; %s\n", errmsg);
        sqlite3_free(errmsg);
        return -1;
    }
    return 0;
}

// self_id, peer_name and resolved_at of a pending conn are undefined
// self_id and peer_name of a rejected conn are undefined
typedef struct Connection {
    long conn_id;
    long peer_id;
    long self_id;
    const char* peer_name;
    const char* peer_addr;
    bool online;
    long requested_at;
    long resolved_at;
    char status; // negative on rejected, zero on accepted, positive on pending
} Connection;

int create_msg_table(sqlite3* db) {
    printf(INFO": creating Messages table\n");
    
    char* errmsg = 0;
    const char* create_msgs_sql = ""
    "CREATE TABLE Messages ("
        "msg_id INTEGER PRIMARY KEY, "
        "conn_id INTEGER, "
        "time_sent INTEGER DEFAULT unixepoch, "
        "time_recieved INTEGER, "
        "content_type INTEGER NOT NULL, "
        "content BLOB, "
        "FOREIGN KEY (conn_id) REFERENCES Connections(conn_id)"
    ");";
    if ( sqlite3_exec(db, create_msgs_sql, NULL, NULL, &errmsg) != SQLITE_OK ) {
        printf(ERROR": could not create the Messages table; %s\n", errmsg);
        sqlite3_free(errmsg);
        return -1;
    }
    return 0;
}

typedef struct Message {
    long msg_id;
    long conn_id;
    long time_sent;
    long time_recieved;
    unsigned char content_type;
    int content_len;
    const unsigned char* content;
} Message;

// non-zero on error
int ap2p_list_connections(Connection* buf, int* buf_len) {
    sqlite3 *db;
    
    if ( sqlite3_open(DB_FILE, &db) ) {
        printf(ERROR": could not open database\n");
        return -1;
    }
    
    sqlite3_stmt *conn_stmt;
    while ( sqlite3_prepare_v2(db, "SELECT * FROM Connections;", -1, &conn_stmt, NULL) != SQLITE_OK ) {
        if ( startswith(sqlite3_errmsg(db), "no such table") == 0 ) {
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
        int status = sqlite3_column_int(conn_stmt, 8);
        
        char* peer_name;
        if ( status==CONN_STATUS_ACCEPTED ) {
            peer_name = sqlite3_malloc(sqlite3_column_bytes(conn_stmt, 3));
            sprintf(peer_name, "%s", sqlite3_column_text(conn_stmt, 3));
        } else {
            peer_name = NULL;
        }
        
        char* peer_addr = sqlite3_malloc(sqlite3_column_bytes(conn_stmt, 4));
        sprintf(peer_addr, "%s", sqlite3_column_text(conn_stmt, 4));
        
        Connection conn = {
            .conn_id      = sqlite3_column_int64(conn_stmt, 0),
            .peer_id      = sqlite3_column_int64(conn_stmt, 1),
            .self_id      = sqlite3_column_int64(conn_stmt, 2),
            .peer_name    = peer_name,
            .peer_addr    = peer_addr,
            .online       =   sqlite3_column_int(conn_stmt, 5),
            .requested_at = sqlite3_column_int64(conn_stmt, 6),
            .resolved_at  = sqlite3_column_int64(conn_stmt, 7),
            .status       = status,
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

// non-zero on error
int ap2p_list_messages(Message* buf, int* buf_len) {
    sqlite3 *db;
    
    if ( sqlite3_open(DB_FILE, &db) ) {
        printf(ERROR": could not open database\n");
        return -1;
    }
    
    sqlite3_stmt *msg_stmt;
    while ( sqlite3_prepare_v2(db, "SELECT * FROM Messages;", -1, &msg_stmt, NULL) != SQLITE_OK ) {
        if ( startswith(sqlite3_errmsg(db), "no such table") == 0 ) {
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
        unsigned long content_len = sqlite3_column_bytes(msg_stmt, 5);
        unsigned char* content = sqlite3_malloc(content_len);
        memcpy(content, sqlite3_column_blob(msg_stmt, 5), content_len);
        
        Message msg = {
            .msg_id        = sqlite3_column_int64(msg_stmt, 0),
            .conn_id       = sqlite3_column_int64(msg_stmt, 1),
            .time_sent     = sqlite3_column_int64(msg_stmt, 2),
            .time_recieved = sqlite3_column_int64(msg_stmt, 3),
            .content_type  =   sqlite3_column_int(msg_stmt, 4),
            .content_len   = content_len,
            .content       = content,
        };
        buf[row_count] = msg;
        row_count += 1;
    }
    if ( res != SQLITE_DONE ) {
        printf(ERROR": failed while executing select; with code %d\n", res);
        sqlite3_close(db);
        return -1;
    }
    sqlite3_finalize(msg_stmt);
    *buf_len = row_count;
    
    sqlite3_close(db);
    return 0;
}

// negative on error
// zero on succes
// positive on pending
int ap2p_request_connection(char* peer_addr) {
    srandom(time(NULL));
    long peer_id = random();
    
    Connection conn = {
        .peer_id   = peer_id,
        .peer_addr = peer_addr,
    };
    
    
    { // insert the conn into the db
        sqlite3 *db;
        if ( sqlite3_open(DB_FILE, &db) ) {
            printf(ERROR": could not open database\n");
            return -1;
        }
        
        sqlite3_stmt *insert_stmt;
        const char* insert_sql = "INSERT INTO Connections (peer_id, peer_addr) VALUES (?, ?);";
        while ( sqlite3_prepare_v2(db, insert_sql, -1, &insert_stmt, NULL) != SQLITE_OK ) {
            if ( startswith(sqlite3_errmsg(db), "no such table") == 0 ) {
                if ( create_conn_table(db) != SQLITE_OK ) {
                    sqlite3_close(db);
                    return -1;
                } else {
                    continue;
                }
            }
            printf(ERROR": could not INSERT INTO Connections; %s (%d)\n", sqlite3_errmsg(db), sqlite3_errcode(db));
            sqlite3_close(db);
            return -1;
        }
        
        int res;
        if ( (res = sqlite3_bind_int64(insert_stmt, 1, conn.peer_id)) != SQLITE_OK ) {
            printf(ERROR": failed to bind peer_id with code: (%d)\n", res);
            sqlite3_close(db);
            return -1;
        }
        if ( (res = sqlite3_bind_text(insert_stmt, 2, conn.peer_addr, strlen(conn.peer_addr), SQLITE_STATIC)) != SQLITE_OK ) {
            printf(ERROR": failed to bind peer_addr with code: (%d)\n", res);
            sqlite3_close(db);
            return -1;
        }
        
        if ( (res = sqlite3_step(insert_stmt)) != SQLITE_DONE ) {
            printf(ERROR": failed while executing insert; with code %d\n", res);
            sqlite3_close(db);
            return -1;
        }
        sqlite3_finalize(insert_stmt);
        sqlite3_close(db);
    } // end inserting the conn into the db
    
    int conn_status = 1;
    long self_id;
    char* peer_name;
    { // attempt to communicate the conn to the peer
        int peer_sock = socket(AF_INET, SOCK_STREAM, 0);
        if (peer_sock < 0) {
            printf(ERROR": peer socket creation failed\n");
            return -1;
        }
    
        struct sockaddr_in peer_addr = {
            .sin_family = AF_INET,
            .sin_addr.s_addr = inet_addr(conn.peer_addr),
            .sin_port = revbo_u16(DEFAULT_PORT),
        };
        if ( connect(peer_sock, (struct sockaddr*)&peer_addr, sizeof(peer_addr)) != 0 ) {
            printf(WARN": could not connect to peer at %s; conn is pending\n", conn.peer_addr);
            return 1;
        }
        printf(INFO": connected to peer at %s\n", conn.peer_addr);
        
        
        const char* self_name = "the_pear_of_adam";
        char parcel[PARCEL_CONN_EST_LEN] = {0};
        {
            parcel[0] = PARCEL_CONN_EST_KIND;
            
            for (int i=1;i<=4;i++) {
                parcel[i] = (peer_id >> (8*(4-i))) & 0xFF;
            }

            strncpy(parcel+5, self_name, MAX_HOST_NAME);
        }
        if ( send(peer_sock, parcel, PARCEL_CONN_EST_LEN, 0) < 0) {
            printf(WARN": could not send parcel to peer at %s; %s; conn is pending\n", conn.peer_addr, strerror(errno));
            close(peer_sock);
            return 1;
        }
        printf(DEBUG": sent establish conn parcel to peer at %s with [peer_id: %ld, self_name: '%s']\n", conn.peer_addr, peer_id, self_name);
        
        printf(INFO": awaiting response from peer at %s\n", conn.peer_addr);
        char resp_kind;
        if ( recv(peer_sock, &resp_kind, 1, 0) < 1 ) {
            printf(WARN": could not recieve response from peer at %s; %s; conn is pending\n", conn.peer_addr, strerror(errno));
            close(peer_sock);
            return 1;
        }
        
        if ( resp_kind == 1 ) { // acepted, kind=PARCEL_CONN_EST_KIND
            char resp[PARCEL_CONN_EST_LEN-1];
            if ( recv(peer_sock, &resp, PARCEL_CONN_EST_LEN-1, 0) < PARCEL_CONN_EST_LEN-1 ) {
                printf(ERROR": could not read PARCEL_CONN_EST from peer at %s; %s \n", conn.peer_addr, strerror(errno));
                close(peer_sock);
                return -1;
            }
            conn_status = 0;
            self_id = *(long*)(&resp);
            peer_name = (char*)(&resp+4);
            printf(INFO": peer at %s accepted conn request with [self_id: %ld, peer_name: %s]\n", conn.peer_addr, self_id, peer_name);
            
        } else if ( resp_kind == 2 ) { // rejected, kind=PARCEL_CONN_REJ_KIND
            printf(INFO": peer at %s rejected conn request\n", conn.peer_addr);
            conn_status = -1;
        } else { // invalid
            printf(WARN": invalid response kind from peer at %s; %s; conn is pending\n", conn.peer_addr, strerror(errno));
            close(peer_sock);
            return 1;
        }
         
        close(peer_sock);
        return 0;
    } // end attempt to communicate the conn to the peer
    
    if (conn_status == 0 || conn_status == -1) { // update conn in db
        
    } // end update conn in db
    
    return 1;
}
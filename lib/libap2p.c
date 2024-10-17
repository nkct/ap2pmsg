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

#define SQL_ERR_FMT "%s (%d)"
#define SQL_ERR(db) sqlite3_errmsg((db)), sqlite3_errcode((db))
#define NET_ERR_FMT "%s; %s"
#define NET_ERR(addr) (addr), strerror(errno)

#define DB_FILE "ap2p_storage.db"

#define LISTEN_ADDR "0.0.0.0"
#define DEFAULT_PORT 7676

#define MAX_HOST_NAME 64 // in bytes

#define PARCEL_CONN_REQ_KIND 1 // request conn
#define PARCEL_CONN_REQ_LEN 73 // kind[1] + peer_id[8] + self_name[64]

#define PARCEL_CONN_ACK_KIND 2 // acknowledge conn request
#define PARCEL_CONN_ACK_LEN  9 // kind[1] + self_id[8]

#define PARCEL_CONN_REJ_KIND 3 // reject conn request
#define PARCEL_CONN_REJ_LEN  9 // kind[1] + self_id[8]

#define PARCEL_CONN_ACC_KIND 4 // accept conn request
#define PARCEL_CONN_ACC_LEN 81 // kind[1] + self_id[8] + peer_id[8] + self_name[64]

/* Reverse the byte order of an unsigned short. */
#define revbo_u16(d) ( ((d&0xff)<<8)|(d>>8) )

#define startswith(str, pat) (strncmp((str), (pat), strlen((pat))) == 0)

long generate_id() {
    // TODO: more sophisticated peer_id generation
    // which would ensure non-repeatability
    srandom(time(NULL));
    return random();
}
void cpy_self_name(char* dst) {
    // TODO: get self_name from state table
    const char* self_name = "the_pear_of_adam";
    strcpy(dst, self_name);
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
        "requested_at INTEGER DEFAULT (strftime('%s', 'now')) NOT NULL, "
        "updated_at INTEGER, "
        "status INTEGER DEFAULT 1 NOT NULL" // see ConnStatus
    ");";
    if ( sqlite3_exec(db, create_conns_sql, NULL, NULL, &errmsg) != SQLITE_OK ) {
        printf(ERROR": could not create the Connections table; %s\n", errmsg);
        sqlite3_free(errmsg);
        return -1;
    }
    return 0;
}

unsigned long ap2p_strlen(const char* s) {
    return strlen(s);
}

typedef enum ConnStatus {
    rejected    = -1, // the peer has reviewed this connection request and rejected it
    accepted    =  0, // this connection has been accepted and can be used to send and recieve messages
    pending     =  1, // the peer has not yet recieved this connection request
    self_review =  2, // this connection has been requested of you, you can resolve (reject or accept), it
    peer_review =  3, // the peer has recieved this connections request, but not yet resolved it
} ConnStatus;

// self_id, peer_name and updated_at of an unaccepted conn are undefined
// peer_id, self_id, peer_name of a rejected conn are undefined (peer_id can be reused)
typedef struct Connection {
    long conn_id;
    long peer_id;
    long self_id;
    const char* peer_name;
    const char* peer_addr;
    bool online;
    long requested_at;
    long updated_at;
    char status; // see ConnStatus
} Connection;

int create_msg_table(sqlite3* db) {
    printf(INFO": creating Messages table\n");
    
    char* errmsg = 0;
    const char* create_msgs_sql = ""
    "CREATE TABLE Messages ("
        "msg_id INTEGER PRIMARY KEY, "
        "conn_id INTEGER, "
        "time_sent INTEGER DEFAULT (strftime('%s', 'now')), "
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

int create_state_table(sqlite3* db) {
    printf(INFO": creating State table\n");
    
    char* errmsg = 0;
    const char* create_state_sql = ""
    "CREATE TABLE State ("
        "pair_id INTEGER PRIMARY KEY, "
        "key TEXT, "
        "value TEXT"
    ");";
    if ( sqlite3_exec(db, create_state_sql, NULL, NULL, &errmsg) != SQLITE_OK ) {
        printf(ERROR": could not create the State table; %s\n", errmsg);
        sqlite3_free(errmsg);
        return -1;
    }
    char* default_state_sql = ""
    "INSERT INTO State (key, value) VALUES"
        "('selected_conn', -1)"
    ";";
    if ( sqlite3_exec(db, default_state_sql, NULL, NULL, &errmsg) != SQLITE_OK ) {
        printf(ERROR": could not insert dafaults into the State table; %s\n", errmsg);
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
        printf(ERROR": could not open database at '%s'\n", DB_FILE);
        goto exit_err;
    }
    
    int res;
    sqlite3_stmt *conn_stmt;
    const char* select_sql = "SELECT * FROM Connections;";
    res = sqlite3_prepare_v2(db, select_sql, -1, &conn_stmt, NULL);
    if ( res != SQLITE_OK && startswith(sqlite3_errmsg(db), "no such table") ) {
        if ( create_conn_table(db) == SQLITE_OK ) {
            res = sqlite3_prepare_v2(db, select_sql, -1, &conn_stmt, NULL);
        } else {
            goto exit_err_db;
        }
    }
    if ( res != SQLITE_OK ) {
        printf(ERROR": could not SELECT * FROM Connections; "SQL_ERR_FMT"\n", SQL_ERR(db));
        goto exit_err_db;
    }
    
    int row_count = 0;
    while ( (res = sqlite3_step(conn_stmt)) == SQLITE_ROW ) {
        int status = sqlite3_column_int(conn_stmt, 8);
        
        char* peer_name;
        if ( status==accepted ) {
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
            .updated_at  = sqlite3_column_int64(conn_stmt, 7),
            .status       = status,
        };
        buf[row_count] = conn;
        row_count += 1;
    }
    if ( res != SQLITE_DONE ) {
        printf(ERROR": failed while iterating conn result; with code %d\n", res);
        goto exit_err_db;
    }
    sqlite3_finalize(conn_stmt);
    *buf_len = row_count;
    
    sqlite3_close(db);
    return 0;
    
    exit_err_db:
        sqlite3_close(db);
    exit_err:
        return -1;
}

// non-zero on error
int ap2p_list_messages(Message* buf, int* buf_len) {
    sqlite3 *db;
    
    if ( sqlite3_open(DB_FILE, &db) ) {
        printf(ERROR": could not open database at '%s'\n", DB_FILE);
        goto exit_err;
    }
    
    int res;
    sqlite3_stmt *msg_stmt;
    const char* select_sql = "SELECT * FROM Messages;";
    res = sqlite3_prepare_v2(db, select_sql, -1, &msg_stmt, NULL);
    if ( res != SQLITE_OK && startswith(sqlite3_errmsg(db), "no such table") ) {
        if ( create_msg_table(db) == SQLITE_OK ) {
            res = sqlite3_prepare_v2(db, select_sql, -1, &msg_stmt, NULL);
        } else {
            goto exit_err_db;
        }
    }
    if ( res != SQLITE_OK ) {
        printf(ERROR": could not SELECT * FROM Messages; "SQL_ERR_FMT"\n", SQL_ERR(db));
        goto exit_err_db;
    }
    
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
        goto exit_err_db;
    }
    sqlite3_finalize(msg_stmt);
    *buf_len = row_count;
    
    sqlite3_close(db);
    return 0;
    
    exit_err_db:
        sqlite3_close(db);
    exit_err:
        return -1;
}

// negative on error
// positive on pending
// zero on acked
int ap2p_request_connection(char* peer_addr) {
    long peer_id = generate_id();
    
    int res;
    sqlite3 *db;
    if ( sqlite3_open(DB_FILE, &db) ) {
        printf(ERROR": could not open database at '%s'\n", DB_FILE);
        goto exit_err;
    }
    
    { // insert the conn into the db        
        sqlite3_stmt *insert_stmt;
        const char* insert_sql = "INSERT INTO Connections (peer_id, peer_addr) VALUES (?, ?);";
        res = sqlite3_prepare_v2(db, insert_sql, -1, &insert_stmt, NULL);
        if ( res != SQLITE_OK && startswith(sqlite3_errmsg(db), "no such table") ) {
            if ( create_conn_table(db) == SQLITE_OK ) {
                res = sqlite3_prepare_v2(db, insert_sql, -1, &insert_stmt, NULL);
            } else {
                goto exit_err_db;
            }
        }
        if ( res != SQLITE_OK ) {
            printf(ERROR": could not INSERT INTO Connections; "SQL_ERR_FMT"\n", SQL_ERR(db));
            goto exit_err_db;
        }
        
        if ( (res = sqlite3_bind_int64(insert_stmt, 1, peer_id)) != SQLITE_OK ) {
            printf(ERROR": failed to bind peer_id with code: (%d)\n", res);
            goto exit_err_db;
        }
        if ( (res = sqlite3_bind_text(insert_stmt, 2, peer_addr, strlen(peer_addr), SQLITE_STATIC)) != SQLITE_OK ) {
            printf(ERROR": failed to bind peer_addr with code: (%d)\n", res);
            goto exit_err_db;
        }
        
        if ( (res = sqlite3_step(insert_stmt)) != SQLITE_DONE ) {
            printf(ERROR": failed while executing insert; with code %d\n", res);
            goto exit_err_db;
        }
        sqlite3_finalize(insert_stmt);
    } // end inserting the conn into the db
    
    int peer_sock = socket(AF_INET, SOCK_STREAM, 0);
    if (peer_sock < 0) {
        printf(ERROR": peer socket creation failed\n");
        goto exit_err_net;
    }
    
    char resp_kind;
    { // attempt to communicate the conn req to the peer
        struct sockaddr_in peer_sockaddr = {
            .sin_family = AF_INET,
            .sin_addr.s_addr = inet_addr(peer_addr),
            .sin_port = revbo_u16(DEFAULT_PORT),
        };
        if ( connect(peer_sock, (struct sockaddr*)&peer_sockaddr, sizeof(peer_sockaddr)) != 0 ) {
            printf(WARN": could not connect to peer at "NET_ERR_FMT"; conn is pending\n", NET_ERR(peer_addr));
            goto exit_pending;
        }
        printf(INFO": connected to peer at %s\n", peer_addr);
        
        char self_name[MAX_HOST_NAME];
        cpy_self_name(self_name);
        char parcel[PARCEL_CONN_REQ_LEN] = {0};
        {
            parcel[0] = PARCEL_CONN_REQ_KIND;
            
            for (int i=1;i<=8;i++) {
                parcel[i] = (peer_id >> (8*(8-i))) & 0xFF;
            }

            strncpy(parcel+9, self_name, MAX_HOST_NAME);
        }
        if ( send(peer_sock, parcel, PARCEL_CONN_REQ_LEN, 0) < 0) {
            printf(WARN": could not send parcel to peer at "NET_ERR_FMT"; conn is pending\n", NET_ERR(peer_addr));
            goto exit_pending;
        }
        printf(DEBUG": sent conn req parcel to peer at %s with [peer_id: %ld, self_name: '%s']\n", peer_addr, peer_id, self_name);
        
        printf(INFO": awaiting response from peer at %s\n", peer_addr);
        
        // TODO: implement a timeout on recv (see setsockopt() or set non-blocking and poll)
        char resp[PARCEL_CONN_ACK_LEN];
        if ( recv(peer_sock, &resp, PARCEL_CONN_ACK_LEN, 0) < PARCEL_CONN_ACK_LEN ) {
            printf(WARN": could not recieve response from peer at "NET_ERR_FMT"; conn is pending\n", NET_ERR(peer_addr));
            goto exit_pending;
        }
        printf(DEBUG": recieved response [%s] from peer at %s\n", resp, peer_addr);
        
        resp_kind = resp[0];
        long resp_peer_id = 0;
        for (int i=0; i<8; i++) {
            resp_peer_id = (resp_peer_id << 8) && resp[i+1];
        }
        
        if (resp_peer_id != peer_id) {
            printf(WARN": peer at %s attempted to ack conn with different peer_id (%ld != %ld)\n", peer_addr, resp_peer_id, peer_id);
            goto exit_pending;
        }
    } // end attempt to communicate the conn to the peer
    
    if ( resp_kind != PARCEL_CONN_ACK_KIND ) {
        printf(WARN": invalid response kind from peer at "NET_ERR_FMT"; conn is pending\n", NET_ERR(peer_addr));
        goto exit_pending;
    } else { // acked, update conn in db
        printf(INFO": peer at %s acknowdelged the connection request\n", peer_addr);
        printf(DEBUG": updating conn to ststus=2 where peer_id=%ld\n", peer_id);
        
        sqlite3_stmt *update_stmt;
        const char* update_sql = ""
        "UPDATE Connections "
        "SET online=1, updated_at=(strftime('%s', 'now')), status=2 "
        "WHERE peer_id=(?);";
        // conn table must exist since we create it above if it doesn't
        if ( sqlite3_prepare_v2(db, update_sql, -1, &update_stmt, NULL) != SQLITE_OK ) {
            printf(ERROR": could not UPDATE Connections; "SQL_ERR_FMT"\n", SQL_ERR(db));
            goto exit_err_db;
        }
        
        if ( (res = sqlite3_bind_int64(update_stmt, 1, peer_id)) != SQLITE_OK ) {
            printf(ERROR": failed to bind peer_id with code: (%d)\n", res);
            goto exit_err_db;
        }
        
        if ( (res = sqlite3_step(update_stmt)) != SQLITE_DONE ) {
            printf(ERROR": failed while executing UPDATE; with code %d\n", res);
            goto exit_err_db;
        }
        sqlite3_finalize(update_stmt);
    }
    
    exit_pending:
        close(peer_sock);
        sqlite3_close(db);
        return 1;
        
    exit_err_net:
        close(peer_sock);
    exit_err_db:
        sqlite3_close(db);
    exit_err:
        return -1;
}

int ap2p_accept_connection(long conn_id) {
    char* peer_addr;
    long self_id;
    int conn_status;
    
    long peer_id = generate_id();
    char self_name[MAX_HOST_NAME];
    cpy_self_name(self_name);
    
    int res;
    sqlite3 *db;
    if ( sqlite3_open(DB_FILE, &db) ) {
        printf(ERROR": could not open database at '%s'\n", DB_FILE);
        goto exit_err;
    }
    { // retrieve conn info from db
        sqlite3_stmt *select_stmt;
        const char* select_sql = "SELECT peer_addr, self_id FROM Connections WHERE conn_id=(?);";
        res = sqlite3_prepare_v2(db, select_sql, -1, &select_stmt, NULL);
        if ( res != SQLITE_OK && startswith(sqlite3_errmsg(db), "no such table") ) {
            if ( create_conn_table(db) == SQLITE_OK ) {
                res = sqlite3_prepare_v2(db, select_sql, -1, &select_stmt, NULL);
            } else {
                goto exit_err_db;
            }
        }
        if ( res != SQLITE_OK ) {
            printf(ERROR": could not SELECT status, peer_addr, self_id FROM Connections WHERE conn_id=%ld; "SQL_ERR_FMT"\n", conn_id, SQL_ERR(db));
            goto exit_err_db;
        }
        
        if ( (res = sqlite3_bind_int64(select_stmt, 1, conn_id)) != SQLITE_OK ) {
            printf(ERROR": failed to bind conn_id with code: (%d)\n", res);
            goto exit_err_db;
        }
        
        
        if ( (res = sqlite3_step(select_stmt)) == SQLITE_ROW ) {
            conn_status = sqlite3_column_int(select_stmt, 0);
            
            peer_addr = sqlite3_malloc(sqlite3_column_bytes(select_stmt, 1));
            sprintf(peer_addr, "%s", sqlite3_column_text(select_stmt, 1));
            
            self_id = sqlite3_column_int64(select_stmt, 2);
        }
        if ( res != SQLITE_DONE ) {
            printf(ERROR": failed while iterating conn result; with code %d\n", res);
            goto exit_err_db;
        }
        sqlite3_finalize(select_stmt);
    } // end retrieve conn info from db
    
    if (conn_status != self_review) {
        printf(ERROR": attempted to accept a connection which wasn't awaiting review, conn status: (%c)\n", conn_status);
        goto exit_err_db;
    }
    
    { // update conn in db
        sqlite3_stmt *update_stmt;
        const char* update_sql = ""
        "UPDATE Connections "
        "SET updated_at=(strftime('%s', 'now')), peer_id=(?), self_name=(?), status=0 "
        "WHERE conn_id=(?);";
        // conn table must exist since we create it above if it doesn't
        if ( sqlite3_prepare_v2(db, update_sql, -1, &update_stmt, NULL) != SQLITE_OK ) {
            printf(ERROR": could not UPDATE Connections; "SQL_ERR_FMT"\n", SQL_ERR(db));
            goto exit_err_db;
        }
        
        if ( (res = sqlite3_bind_int64(update_stmt, 1, peer_id)) != SQLITE_OK ) {
            printf(ERROR": failed to bind peer_id with code: (%d)\n", res);
            goto exit_err_db;
        }
        if ( (res = sqlite3_bind_text(update_stmt, 2, self_name, strlen(self_name), SQLITE_STATIC)) != SQLITE_OK ) {
            printf(ERROR": failed to bind self_name with code: (%d)\n", res);
            goto exit_err_db;
        }
        if ( (res = sqlite3_bind_int64(update_stmt, 3, conn_id)) != SQLITE_OK ) {
            printf(ERROR": failed to bind conn_id with code: (%d)\n", res);
            goto exit_err_db;
        }
        
        if ( (res = sqlite3_step(update_stmt)) != SQLITE_DONE ) {
            printf(ERROR": failed while executing UPDATE; with code %d\n", res);
            goto exit_err_db;
        }
        sqlite3_finalize(update_stmt);
    } // end update conn in db
    
    int peer_sock = socket(AF_INET, SOCK_STREAM, 0);
    if (peer_sock < 0) {
        printf(ERROR": peer socket creation failed\n");
        goto exit_err_net;
    }
    { // communicate the acceptance to the peer
        struct sockaddr_in peer_sockaddr = {
            .sin_family = AF_INET,
            .sin_addr.s_addr = inet_addr(peer_addr),
            .sin_port = revbo_u16(DEFAULT_PORT),
        };
        if ( connect(peer_sock, (struct sockaddr*)&peer_sockaddr, sizeof(peer_sockaddr)) != 0 ) {
            printf(WARN": could not connect to peer at "NET_ERR_FMT"\n", NET_ERR(peer_addr));
            goto exit_err_net;
        }
        printf(INFO": connected to peer at %s\n", peer_addr);
        
        char parcel[PARCEL_CONN_ACC_LEN] = {0};
        {
            parcel[0] = PARCEL_CONN_ACC_KIND;
            
            for (int i=1;i<=8;i++) {
                parcel[i] = (self_id >> (8*(8-i))) & 0xFF;
            }
            
            for (int i=9;i<=16;i++) {
                parcel[i] = (peer_id >> (8*(8-i))) & 0xFF;
            }

            strncpy(parcel+17, self_name, MAX_HOST_NAME);
        }
        if ( send(peer_sock, parcel, PARCEL_CONN_ACC_LEN, 0) < 0) {
            printf(WARN": could not send parcel to peer at "NET_ERR_FMT"\n", NET_ERR(peer_addr));
            goto exit_err_net;
        }
        printf(DEBUG": sent acc parcel to peer at %s with [self_id: %ld, peer_id: %ld, self_name: '%s']\n", peer_addr, self_id, peer_id, self_name);
    } // end communicate the acceptance to the peer
    
    sqlite3_close(db);
    return 0;
    
    exit_err_net:
        close(peer_sock);
    exit_err_db:
        sqlite3_close(db);
    exit_err:
        return -1;
}

int ap2p_select_connection(long conn_id) {
    sqlite3 *db;
    if ( sqlite3_open(DB_FILE, &db) ) {
        printf(ERROR": could not open database at '%s'\n", DB_FILE);
        goto exit_err;
    }
    
    int res;
    sqlite3_stmt *update_stmt;
    const char* update_sql = "UPDATE State SET value=(?) WHERE key='selected_conn';";
    res = sqlite3_prepare_v2(db, update_sql, -1, &update_stmt, NULL);
    if ( res != SQLITE_OK && startswith(sqlite3_errmsg(db), "no such table") ) {
        if ( create_state_table(db) == SQLITE_OK ) {
            res = sqlite3_prepare_v2(db, update_sql, -1, &update_stmt, NULL);
        } else {
            goto exit_err_db;
        }
    }
    if ( res != SQLITE_OK ) {
        printf(ERROR": could not SELECT status, peer_addr, self_id FROM Connections WHERE conn_id=%ld; "SQL_ERR_FMT"\n", conn_id, SQL_ERR(db));
        goto exit_err_db;
    }
    
    if ( (res = sqlite3_bind_int64(update_stmt, 1, conn_id)) != SQLITE_OK ) {
        printf(ERROR": failed to bind conn_id with code: (%d)\n", res);
        goto exit_err_db;
    }
        
    if ( (res = sqlite3_step(update_stmt)) != SQLITE_DONE ) {
        printf(ERROR": failed while executing update; with code %d\n", res);
        goto exit_err_db;
    }
    sqlite3_finalize(update_stmt);
    
    sqlite3_close(db);
    return 0;
    
    exit_err_db:
        sqlite3_close(db);
    exit_err:
        return -1;
}

int ap2p_listen() {
    sqlite3 *db;
    if ( sqlite3_open(DB_FILE, &db) ) {
        printf(ERROR": could not open database at '%s'\n", DB_FILE);
        goto exit_err;
    }
    
    int listening_sock = socket(AF_INET, SOCK_STREAM, 0);
    if (listening_sock < 0) {
      printf(ERROR": peer socket creation failed\n");
      goto exit_err_net;
    }
  
    struct sockaddr_in listening_addr = {
        .sin_family = AF_INET,
        .sin_addr.s_addr = inet_addr(LISTEN_ADDR),
        .sin_port = revbo_u16(DEFAULT_PORT),
    };
    if (bind(listening_sock, (struct sockaddr *)&listening_addr, sizeof(listening_addr)) < 0) {
      printf(ERROR": failed to bind server socket");
      goto exit_err_net;
    }
  
    if (listen(listening_sock, 1) < 0) {
      printf(ERROR": failed to listen on peer socket");
      goto exit_err_net;
    }
    printf(INFO": Listening for parcels at %s:%d...\n", LISTEN_ADDR, DEFAULT_PORT);
    
    struct sockaddr_in incoming_addr;
    int incoming_addr_len = sizeof(incoming_addr);
    while (1) {
        int incoming_sock = accept(listening_sock, (struct sockaddr*)&incoming_addr, (socklen_t*)&incoming_addr_len);
        char incoming_addr_str[15];
        inet_ntop(AF_INET, &incoming_addr.sin_addr, incoming_addr_str, 15);
        
        char resp_kind;
        recv(incoming_sock, &resp_kind, 1, 0);
        printf(DEBUG": conn from %s:%d with kind: %d\n", incoming_addr_str, incoming_addr.sin_port, resp_kind);
        
        switch (resp_kind) {
            break; case PARCEL_CONN_REQ_KIND:
                printf(INFO": recieved a CONN_REQ parcel\n");
            break; case PARCEL_CONN_ACK_KIND:
                printf(INFO": recieved a CONN_ACK parcel\n");
            break; case PARCEL_CONN_REJ_KIND:
                printf(INFO": recieved a CONN_REJ parcel\n");
            break; case PARCEL_CONN_ACC_KIND:
                printf(INFO": recieved a CONN_ACC parcel\n");
            break; default:
                printf(WARN": invalid resp_kind: %d\n", resp_kind);
        }
    }
    
    close(listening_sock);
    sqlite3_close(db);
    return 0;
    
    exit_err_net:
        close(listening_sock);
    exit_err_db:
        sqlite3_close(db);
    exit_err:
        return -1;
}
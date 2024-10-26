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
#define NET_ERR_FMT "at %s; %s"
#define NET_ERR(addr) (addr), strerror(errno)

#define FAILED_DB_OPEN_ERR_MSG ERROR": could not open database at '%s'\n", DB_FILE
#define FAILED_PREPARE_STMT_ERR_MSG(stmt) ERROR": failed to prepare statement [%s]; "SQL_ERR_FMT"\n", sqlite3_sql((*stmt)), SQL_ERR(db)
#define FAILED_STMT_STEP_ERR_MSG ERROR": failed while evaluating the statement; "SQL_ERR_FMT"\n", SQL_ERR(db)
#define FAILED_PARAM_BIND_ERR_MSG ERROR": failed to bind parameters; "SQL_ERR_FMT"\n", SQL_ERR(db)

#define DB_FILE "ap2p_storage.db"

#define LISTEN_ADDR "0.0.0.0"
#define DEFAULT_PORT 7676

#define MAX_HOST_NAME 64 // in bytes

// IDs and names are from the perspective of the sender
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

#define ap2p_log(...) fprintf(stderr, __VA_ARGS__)

#define pack_long(buf, d)                 \
for (int i=0;i<8;i++) {                   \
    (buf)[i] = ((d) >> (8*(7-i))) & 0xFF; \
}

#define unpack_long(d, buf)      \
for (int i=0;i<8;i++) {          \
    (d) = ((d) << 8) + (buf)[i]; \
}

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
    ap2p_log(INFO": creating Connections table\n");
    
    char* errmsg = 0;
    const char* create_conns_sql = ""
    "CREATE TABLE Connections ("
        "conn_id INTEGER PRIMARY KEY, "
        "peer_id INTEGER UNIQUE, "
        "self_id INTEGER, "
        "peer_name TEXT, "
        "peer_addr TEXT NOT NULL, "
        "online INTEGER DEFAULT 0, "
        "requested_at INTEGER DEFAULT (strftime('%s', 'now')) NOT NULL, "
        "updated_at INTEGER, "
        "status INTEGER DEFAULT 1 NOT NULL" // see ConnStatus
    ");";
    if ( sqlite3_exec(db, create_conns_sql, NULL, NULL, &errmsg) != SQLITE_OK ) {
        ap2p_log(ERROR": could not create the Connections table; %s\n", errmsg);
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
    self_review =  2, // this connection has been requested of you, you can resolve (reject or accept) it
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

int send_parcel(unsigned char* parcel, unsigned long parcel_len, char* addr) {
    if (parcel_len == 0) { return 0; }
    
    ap2p_log(DEBUG": parcel: [");
    for (int i = 0; i<parcel_len; i++) {
        ap2p_log("%d, ", parcel[i]);
    }
    ap2p_log("]\n");
    
    int peer_sock = socket(AF_INET, SOCK_STREAM, 0);
    if (peer_sock < 0) {
        ap2p_log(ERROR": failed to create peer socket; %s\n", strerror(errno));
        close(peer_sock);
        return -1;
    }
    
    struct sockaddr_in peer_sockaddr = {
        .sin_family = AF_INET,
        .sin_addr.s_addr = inet_addr(addr),
        .sin_port = revbo_u16(DEFAULT_PORT),
    };
    if ( connect(peer_sock, (struct sockaddr*)&peer_sockaddr, sizeof(peer_sockaddr)) != 0 ) {
        ap2p_log(WARN": could not connect "NET_ERR_FMT"\n", NET_ERR(addr));
        close(peer_sock);
        return -1;
    }
    
    if ( send(peer_sock, parcel, parcel_len, 0) == parcel_len) {
        ap2p_log(DEBUG": sent parcel of kind %d to %s\n", parcel[0], addr);
    } else {
        ap2p_log(WARN": could not send parcel "NET_ERR_FMT"\n", NET_ERR(addr));
        close(peer_sock);
        return -1;
    }
    
    return 0;
}
int recv_parcel(int sock, unsigned char* parcel, unsigned long parcel_len) {
    if (recv(sock, parcel, parcel_len, 0) < parcel_len) {
        ap2p_log(WARN": could not read parcel contents; %s\n", strerror(errno));
        return -1;
    }
    ap2p_log(DEBUG": parcel: [");
    for (int i = 0; i<parcel_len; i++) {
        ap2p_log("%d, ", parcel[i]);
    }
    ap2p_log("]\n");
    
    return 0;
}

int create_msg_table(sqlite3* db) {
    ap2p_log(INFO": creating Messages table\n");
    
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
        ap2p_log(ERROR": could not create the Messages table; %s\n", errmsg);
        sqlite3_free(errmsg);
        return -1;
    }
    return 0;
}

int create_state_table(sqlite3* db) {
    ap2p_log(INFO": creating State table\n");
    
    char* errmsg = 0;
    const char* create_state_sql = ""
    "CREATE TABLE State ("
        "pair_id INTEGER PRIMARY KEY, "
        "key TEXT, "
        "value TEXT"
    ");";
    if ( sqlite3_exec(db, create_state_sql, NULL, NULL, &errmsg) != SQLITE_OK ) {
        ap2p_log(ERROR": could not create the State table; %s\n", errmsg);
        sqlite3_free(errmsg);
        return -1;
    }
    char* default_state_sql = ""
    "INSERT INTO State (key, value) VALUES"
        "('selected_conn', -1)"
    ";";
    if ( sqlite3_exec(db, default_state_sql, NULL, NULL, &errmsg) != SQLITE_OK ) {
        ap2p_log(ERROR": could not insert dafaults into the State table; %s\n", errmsg);
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

int trace_callback(unsigned int T, void* C, void* P, void* X) {
    ap2p_log(DEBUG": executing query: '%s'\n", sqlite3_expanded_sql(P));
    return 0;
}
sqlite3* open_db() {
    sqlite3* db;
    if ( sqlite3_open(DB_FILE, &db) ) {
        ap2p_log(FAILED_DB_OPEN_ERR_MSG);
        return NULL;
    }
    sqlite3_trace_v2(db, SQLITE_TRACE_STMT, trace_callback, NULL);
    
    return db;
}
int prepare_sql_statement(sqlite3* db, sqlite3_stmt** stmt, const char* sql, int create_table(sqlite3*)) {
    int res;
    res = sqlite3_prepare_v2(db, sql, -1, stmt, NULL);
    if ( res != SQLITE_OK && startswith(sqlite3_errmsg(db), "no such table") ) {
        if ( create_table(db) == SQLITE_OK ) {
            res = sqlite3_prepare_v2(db, sql, -1, stmt, NULL);
        } else {
            return -1;
        }
    }
    if ( res != SQLITE_OK ) {
        ap2p_log(FAILED_PREPARE_STMT_ERR_MSG(stmt));
        return -1;
    }
    
    return 0;
}

// non-zero on error
int ap2p_list_connections(Connection* buf, int* buf_len) {
    sqlite3 *db = open_db();
    if ( db == NULL ) { goto exit_err; }
    
    int res;
    sqlite3_stmt* conn_stmt;
    const char* select_sql = "SELECT * FROM Connections;";
    if ( prepare_sql_statement(db, &conn_stmt, select_sql, &create_conn_table) ) {
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
        ap2p_log(FAILED_STMT_STEP_ERR_MSG);
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
    sqlite3 *db = open_db();
    if ( db == NULL ) { goto exit_err; }
    
    int res;
    sqlite3_stmt *msg_stmt;
    const char* select_sql = "SELECT * FROM Messages;";
    if ( prepare_sql_statement(db, &msg_stmt, select_sql, &create_msg_table) ) {
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
        ap2p_log(FAILED_STMT_STEP_ERR_MSG);
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

// non-zero on error
int ap2p_request_connection(char* peer_addr) {
    long peer_id = generate_id();
    
    sqlite3 *db = open_db();
    if ( db == NULL ) { goto exit_err; }
    
    { // insert the conn into the db     
        sqlite3_stmt *insert_stmt;
        const char* insert_sql = "INSERT INTO Connections (peer_id, peer_addr) VALUES (?, ?);";
        if ( prepare_sql_statement(db, &insert_stmt, insert_sql, &create_conn_table) ) {
            goto exit_err_db;
        }
        
        int bind_fail = 0;
        bind_fail |= (sqlite3_bind_int64(insert_stmt, 1, peer_id) != SQLITE_OK);
        bind_fail |= (sqlite3_bind_text(insert_stmt, 2, peer_addr, strlen(peer_addr), SQLITE_STATIC) != SQLITE_OK);
        if ( bind_fail ) {
            ap2p_log(FAILED_PARAM_BIND_ERR_MSG);
            goto exit_err_db;
        }
        
        if ( sqlite3_step(insert_stmt) != SQLITE_DONE ) {
            ap2p_log(FAILED_STMT_STEP_ERR_MSG);
            goto exit_err_db;
        }
        sqlite3_finalize(insert_stmt);
        sqlite3_close(db);
    } // end inserting the conn into the db
    
    char self_name[MAX_HOST_NAME];
    cpy_self_name(self_name);
    
    unsigned char parcel[PARCEL_CONN_REQ_LEN] = {0};
    parcel[0] = PARCEL_CONN_REQ_KIND;
    pack_long(parcel+1, peer_id);
    strncpy((char*)parcel+9, self_name, MAX_HOST_NAME);
    
    if ( send_parcel(parcel, PARCEL_CONN_REQ_LEN, peer_addr) == 0 ) {
        ap2p_log(INFO": sent connection request to peer at %s; connection is awaiting acknowledgement\n", peer_addr);
    } else {
        ap2p_log(INFO": could not send connection request to peer at %s; \x1b[33mconnection is pending\x1b[0m\n", peer_addr);
    }
    return 0;
    
    exit_err_db:
        sqlite3_close(db);
    exit_err:
        return -1;
}

// decision: 0 for acc, non-zero for rej
int ap2p_decide_on_connection(long conn_id, int decision) {
    char* peer_addr;
    long self_id;
    int conn_status;
    
    int res;
    sqlite3 *db = open_db();
    if ( db == NULL ) { goto exit_err; }
    { // retrieve conn info from db
        sqlite3_stmt *select_stmt;
        const char* select_sql = "SELECT peer_addr, self_id FROM Connections WHERE conn_id=(?);";
        if ( prepare_sql_statement(db, &select_stmt, select_sql, &create_conn_table) ) {
            goto exit_err_db;
        }
        
        if ( (res = sqlite3_bind_int64(select_stmt, 1, conn_id)) != SQLITE_OK ) {
            ap2p_log(FAILED_PARAM_BIND_ERR_MSG);
            goto exit_err_db;
        }
        
        if ( (res = sqlite3_step(select_stmt)) == SQLITE_ROW ) {
            conn_status = sqlite3_column_int(select_stmt, 0);
            
            peer_addr = sqlite3_malloc(sqlite3_column_bytes(select_stmt, 1));
            sprintf(peer_addr, "%s", sqlite3_column_text(select_stmt, 1));
            
            self_id = sqlite3_column_int64(select_stmt, 2);
        }
        if ( res != SQLITE_DONE ) {
            ap2p_log(FAILED_STMT_STEP_ERR_MSG);
            goto exit_err_db;
        }
        sqlite3_finalize(select_stmt);
    } // end retrieve conn info from db
    
    if (conn_status != self_review) {
        ap2p_log(ERROR": attempted to decide on a connection which wasn't awaiting review, conn status: (%c)\n", conn_status);
        goto exit_err_db;
    }
    
    if ( decision != 0 ) { // rejected
        { // update conn in db
            sqlite3_stmt *update_stmt;
            const char* update_sql = ""
            "UPDATE Connections "
            "SET updated_at=(strftime('%s', 'now')), status=-1 "
            "WHERE conn_id=(?);";
            if ( prepare_sql_statement(db, &update_stmt, update_sql, &create_conn_table) ) {
                goto exit_err_db;
            }
            
            if ( sqlite3_bind_int64(update_stmt, 1, conn_id) != SQLITE_OK ) {
                ap2p_log(FAILED_PARAM_BIND_ERR_MSG);
                goto exit_err_db;
            }
            
            if ( sqlite3_step(update_stmt) != SQLITE_DONE ) {
                ap2p_log(FAILED_STMT_STEP_ERR_MSG);
                goto exit_err_db;
            }
            sqlite3_finalize(update_stmt);
        } // end update conn in db
        
        unsigned char parcel[PARCEL_CONN_REJ_LEN] = {0};
        parcel[0] = PARCEL_CONN_REJ_KIND;
        pack_long(parcel+1, self_id);
            
        if ( send_parcel(parcel, PARCEL_CONN_ACC_LEN, peer_addr) == 0 ) {
            ap2p_log(INFO": rejected connection request from peer at %s", peer_addr);
        } else {
            ap2p_log(INFO": marked connection request from peer at %s as rejected, \x1b[33mbut\x1b[0m could not communicate it to the peer", peer_addr);
        }
    } else { // accepted
        long peer_id = generate_id();
        char self_name[MAX_HOST_NAME];
        cpy_self_name(self_name);
        
        { // update conn in db
            sqlite3_stmt *update_stmt;
            const char* update_sql = ""
            "UPDATE Connections "
            "SET updated_at=(strftime('%s', 'now')), peer_id=(?), self_name=(?), status=0 "
            "WHERE conn_id=(?);";
            if ( prepare_sql_statement(db, &update_stmt, update_sql, &create_conn_table) ) {
                goto exit_err_db;
            }
            
            int bind_fail = 0;
            bind_fail |= (sqlite3_bind_int64(update_stmt, 1, peer_id) != SQLITE_OK);
            bind_fail |= (sqlite3_bind_text(update_stmt, 2, self_name, strlen(self_name), SQLITE_STATIC) != SQLITE_OK);
            bind_fail |= (sqlite3_bind_int64(update_stmt, 3, conn_id) != SQLITE_OK);
            if ( bind_fail ) {
                ap2p_log(FAILED_PARAM_BIND_ERR_MSG);
                goto exit_err_db;
            }
            
            if ( sqlite3_step(update_stmt) != SQLITE_DONE ) {
                ap2p_log(FAILED_STMT_STEP_ERR_MSG);
                goto exit_err_db;
            }
            sqlite3_finalize(update_stmt);
        } // end update conn in db
        
        unsigned char parcel[PARCEL_CONN_ACC_LEN] = {0};
        parcel[0] = PARCEL_CONN_ACC_KIND;
        pack_long(parcel+1, self_id);
        pack_long(parcel+9, peer_id);
        strncpy((char*)parcel+17, self_name, MAX_HOST_NAME);
            
        if ( send_parcel(parcel, PARCEL_CONN_ACC_LEN, peer_addr) == 0 ) {
            ap2p_log(INFO": accepted connection request from peer at %s", peer_addr);
        } else {
            ap2p_log(INFO": marked connection request from peer at %s as accepted, \x1b[33mbut\x1b[0m could not communicate it to the peer", peer_addr);
        }
    }
    
    sqlite3_close(db);
    return 0;
    
    exit_err_db:
        sqlite3_close(db);
    exit_err:
        return -1;
}

int ap2p_select_connection(long conn_id) {
    sqlite3 *db = open_db();
    if ( db == NULL ) { goto exit_err; }
    
    int res;
    sqlite3_stmt *update_stmt;
    const char* update_sql = "UPDATE State SET value=(?) WHERE key='selected_conn';";
    if ( prepare_sql_statement(db, &update_stmt, update_sql, &create_state_table) ) {
        goto exit_err_db;
    }
    
    if ( (res = sqlite3_bind_int64(update_stmt, 1, conn_id)) != SQLITE_OK ) {
        ap2p_log(FAILED_PARAM_BIND_ERR_MSG);
        goto exit_err_db;
    }
        
    if ( sqlite3_step(update_stmt) != SQLITE_DONE ) {
        ap2p_log(FAILED_STMT_STEP_ERR_MSG);
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
    int res;
    sqlite3 *db = open_db();
    if ( db == NULL ) { goto exit_err; }
    
    int listening_sock = socket(AF_INET, SOCK_STREAM, 0);
    if (listening_sock < 0) {
      ap2p_log(ERROR": peer socket creation failed\n");
      goto exit_err_net;
    }
  
    struct sockaddr_in listening_addr = {
        .sin_family = AF_INET,
        .sin_addr.s_addr = inet_addr(LISTEN_ADDR),
        .sin_port = revbo_u16(DEFAULT_PORT),
    };
    if (bind(listening_sock, (struct sockaddr *)&listening_addr, sizeof(listening_addr)) < 0) {
      ap2p_log(ERROR": failed to bind server socket; %s\n", strerror(errno));
      goto exit_err_net;
    }
  
    if (listen(listening_sock, 1) < 0) {
      ap2p_log(ERROR": failed to listen on peer socket; %s\n", strerror(errno));
      goto exit_err_net;
    }
    ap2p_log(INFO": Listening for parcels at %s:%d...\n", LISTEN_ADDR, DEFAULT_PORT);
    
    struct sockaddr_in incoming_addr;
    int incoming_addr_len = sizeof(incoming_addr);
    while (1) {
        int incoming_sock = accept(listening_sock, (struct sockaddr*)&incoming_addr, (socklen_t*)&incoming_addr_len);
        char incoming_addr_str[15];
        inet_ntop(AF_INET, &incoming_addr.sin_addr, incoming_addr_str, 15);
        
        char parcel_kind;
        // note that we only peek at parcel_kind without consuming the first byte
        // this makes parcel reading simpler as there's no need to offset PARCEL_LEN by one
        if ( recv(incoming_sock, &parcel_kind, 1, MSG_PEEK) < 1) {
            ap2p_log(WARN": could not read parcel kind");
            continue;
        }
        ap2p_log(DEBUG": conn from %s:%d with kind: %d\n", incoming_addr_str, incoming_addr.sin_port, parcel_kind);
        
        switch (parcel_kind) {
            break; case PARCEL_CONN_REQ_KIND: {
                ap2p_log(INFO": recieved a CONN_REQ parcel\n");
                unsigned char parcel[PARCEL_CONN_REQ_LEN];
                if ( recv_parcel(incoming_sock, parcel, PARCEL_CONN_REQ_LEN) ) { continue; }

                long self_id = 0;
                unpack_long(self_id, parcel+1);
                
                char peer_name[MAX_HOST_NAME] = {0};
                strncpy(peer_name, (char*)parcel+9, MAX_HOST_NAME);

                ap2p_log(DEBUG": peer '%s' requested conn with self_id: %ld, \n", peer_name, self_id);
                
                sqlite3_stmt *insert_stmt;
                const char* insert_sql = "INSERT INTO Connections (self_id, peer_name, peer_addr, status) VALUES (?, ?, ?, 2);";
                if ( prepare_sql_statement(db, &insert_stmt, insert_sql, &create_conn_table) ) {
                    continue;
                }
                
                int bind_fail = 0;
                bind_fail |= (sqlite3_bind_int64(insert_stmt, 1, self_id) != SQLITE_OK);
                bind_fail |= (sqlite3_bind_text(insert_stmt, 2, peer_name, strlen(peer_name), SQLITE_STATIC) != SQLITE_OK);
                bind_fail |= (sqlite3_bind_text(insert_stmt, 3, incoming_addr_str, strlen(incoming_addr_str), SQLITE_STATIC) != SQLITE_OK);
                if ( bind_fail ) {
                    ap2p_log(FAILED_PARAM_BIND_ERR_MSG);
                    continue;
                }
                
                if ( sqlite3_step(insert_stmt) != SQLITE_DONE ) {
                    ap2p_log(FAILED_STMT_STEP_ERR_MSG);
                    continue;
                }
                sqlite3_finalize(insert_stmt);
                ap2p_log(DEBUG": inserted requested conn into the db, with self_id: %ld, peer_name: %s, peer_addr: %s\n", self_id, peer_name, incoming_addr_str);
                
                unsigned char resp_parcel[PARCEL_CONN_ACK_LEN] = {0};
                resp_parcel[0] = PARCEL_CONN_ACK_KIND;
                pack_long(parcel+1, self_id);

                if ( send_parcel(resp_parcel, PARCEL_CONN_ACK_LEN, incoming_addr_str) == 0 ) {
                    ap2p_log(INFO": acknowledged connection request from peer at %s\n", incoming_addr_str);
                } else {
                    ap2p_log(WARN": failed to acknowledge connection request from peer at %s\n", incoming_addr_str);
                }
                
            } break; case PARCEL_CONN_ACK_KIND: {
                ap2p_log(INFO": recieved a CONN_ACK parcel\n");
                unsigned char parcel[PARCEL_CONN_ACK_LEN];
                if ( recv_parcel(incoming_sock, parcel, PARCEL_CONN_ACK_LEN) ) { continue; }

                long peer_id = 0;
                unpack_long(peer_id, parcel+1);

                ap2p_log(DEBUG": peer with ID %ld acked conn req\n", peer_id);
                
                sqlite3_stmt *update_stmt;
                const char* update_sql = "UPDATE Connections SET status=3 WHERE peer_id=(?);";
                if ( prepare_sql_statement(db, &update_stmt, update_sql, &create_conn_table) ) {
                    continue;
                }
                
                if ( sqlite3_bind_int64(update_stmt, 1, peer_id) != SQLITE_OK ) {
                    ap2p_log(FAILED_PARAM_BIND_ERR_MSG);
                    continue;
                }
                
                if ( sqlite3_step(update_stmt) != SQLITE_DONE ) {
                    ap2p_log(FAILED_STMT_STEP_ERR_MSG);
                    continue;
                }
                sqlite3_finalize(update_stmt);
                ap2p_log(DEBUG": updated conn to 'awaiting peer review' where peer_id=%ld\n", peer_id);
                
            } break; case PARCEL_CONN_REJ_KIND: {
                ap2p_log(INFO": recieved a CONN_REJ parcel\n");
            } break; case PARCEL_CONN_ACC_KIND: {
                ap2p_log(INFO": recieved a CONN_ACC parcel\n");
            } break; default:
                ap2p_log(WARN": invalid parcel kind: %d\n", parcel_kind);
        }
    }
    
    ap2p_log(DEBUG": finished handling the parcel\n");
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
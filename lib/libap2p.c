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
#include <ifaddrs.h>
#include <netdb.h>

#include "utilap2p.h"


#define DEFAULT_LISTEN_ADDR "0.0.0.0"
#define DEFAULT_PORT "7676"
#define DEFAULT_NAME "the_pear_of_adam"

// IDs and names are from the perspective of the sender
#define PARCEL_CONN_REQ_KIND 1 // request conn
#define PARCEL_CONN_REQ_LEN 93 // kind[1] + peer_id[8] + self_name[64] + self_addr[16] + self_port[4]

#define PARCEL_CONN_ACK_KIND 2 // acknowledge conn request
#define PARCEL_CONN_ACK_LEN  9 // kind[1] + self_id[8]

#define PARCEL_CONN_REJ_KIND 3 // reject conn request
#define PARCEL_CONN_REJ_LEN  9 // kind[1] + self_id[8]

#define PARCEL_CONN_ACC_KIND 4 // accept conn request
#define PARCEL_CONN_ACC_LEN 81 // kind[1] + self_id[8] + peer_id[8] + self_name[64]

#define PARCEL_MSG_SEND_KIND 10 // send msg
// add time_sent
#define PARCEL_MSG_SEND_HDR_LEN 14 // kind[1] + self_id[8] + content_type[1] + content_len[4] // add content_len separately

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
    int peer_port;
    bool online;
    long requested_at;
    long updated_at;
    char status; // see ConnStatus
} Connection;

typedef enum MsgContentType {
    text = 0,
} MsgContentType;

typedef struct Message {
    long msg_id;
    long conn_id;
    long time_sent;
    long time_recieved; // 0 on pending
    unsigned char content_type; // see MsgContentType
    int content_len;
    const unsigned char* content;
} Message;

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
        "peer_port INTEGER NOT NULL, " // in host byte order
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

int create_msg_table(sqlite3* db) {
    ap2p_log(INFO": creating Messages table\n");
    
    char* errmsg = 0;
    const char* create_msgs_sql = ""
    "CREATE TABLE Messages ("
        "msg_id INTEGER PRIMARY KEY, "
        "conn_id INTEGER, "
        "time_sent INTEGER DEFAULT (strftime('%s', 'now')), "
        "time_recieved INTEGER, " // time_recieved determines msg status, if null, msg is pending
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
        "key TEXT UNIQUE, " // consder indexing key
        "value TEXT"
    ");";
    if ( sqlite3_exec(db, create_state_sql, NULL, NULL, &errmsg) != SQLITE_OK ) {
        ap2p_log(ERROR": could not create the State table; %s\n", errmsg);
        sqlite3_free(errmsg);
        return -1;
    }
    
    char self_addr[MAX_IP_ADDR_LEN] = {0};
    { // get self addr
        struct ifaddrs *ifaddr;
        if (getifaddrs(&ifaddr) == -1) {
            ap2p_log(ERROR": could not obtain the local IP addr structure; %s\n", strerror(errno));
            return -1;
        }
        
        int found_addr = 0;
        for (struct ifaddrs *ifa = ifaddr; ifa != NULL; ifa = ifa->ifa_next) {
            if ( ifa->ifa_addr == NULL || ifa->ifa_addr->sa_family != AF_INET ) { continue; }

            struct sockaddr_in* inet_addr = (struct sockaddr_in*)ifa->ifa_addr;
            
            if ( ntohl(inet_addr->sin_addr.s_addr) != INADDR_LOOPBACK ) {
                inet_ntop(AF_INET, &inet_addr->sin_addr, self_addr, MAX_IP_ADDR_LEN);
                found_addr = 1;
                break;
            }
        }
        
        freeifaddrs(ifaddr);

        if (!found_addr) {
            ap2p_log(ERROR": failed to find self addr\n");
            return -1;
        }
    } // end get self addr
    
    sqlite3_stmt* insert_default_stmt;
    char* default_state_sql = ""
    "INSERT INTO State (key, value) VALUES"
        "('selected_conn', -1),"
        "('listen_addr', '"DEFAULT_LISTEN_ADDR"'),"
        "('self_addr', ?),"
        "('self_port', '"DEFAULT_PORT"'), "
        "('self_name', '"DEFAULT_NAME"')"
    ";";
    if ( prepare_sql_statement(db, &insert_default_stmt, default_state_sql, &create_state_table) ) { return -1; }
    
    if ( sqlite3_bind_text(insert_default_stmt, 1, self_addr, strlen(self_addr), SQLITE_STATIC) != SQLITE_OK ) {
        ap2p_log(FAILED_PARAM_BIND_ERR_MSG);
        return -1;
    }
        
    if ( sqlite3_step(insert_default_stmt) != SQLITE_DONE ) {
        ap2p_log(FAILED_STMT_STEP_ERR_MSG);
        return -1;
    }
    sqlite3_finalize(insert_default_stmt);
    
    return 0;
}

// remember to free value, this functions allocs
// TODO: consider an interface akin to 'state_get_many()' for multiple keys at once
char* ap2p_state_get(sqlite3* db, const char* key) {  
    bool db_null = 0;
    if ( db == NULL ) {
        db_null = 1;
        
        db = open_db();
        if ( db == NULL ) { 
            ap2p_log(ERROR": did not pass in a db connection to state_get, and opening a new one failed\n");
            return NULL;
        }
    }
    
    sqlite3_stmt* get_stmt;
    const char* get_sql = "SELECT value FROM State WHERE key=?;";
    if ( prepare_sql_statement(db, &get_stmt, get_sql, &create_state_table) != SQLITE_OK ) { return NULL; }
    
    if ( sqlite3_bind_text(get_stmt, 1, key, strlen(key), SQLITE_STATIC) != SQLITE_OK ) {
        ap2p_log(FAILED_PARAM_BIND_ERR_MSG);
        return NULL;
    }
    
    char* value = NULL;
    if ( sqlite3_step(get_stmt) == SQLITE_ROW ) {
        value = (char*)malloc(sqlite3_column_bytes(get_stmt, 0)+1);
        strcpy(value, (char*)sqlite3_column_text(get_stmt, 0));
    } else {
        ap2p_log(FAILED_STMT_STEP_ERR_MSG);
        return NULL;
    }
    sqlite3_finalize(get_stmt);
    
    if ( db_null ) {
        sqlite3_close(db);
    }
    
    return value;
}

int ap2p_state_set(sqlite3* db, const char* key, const char* value) {  
    bool db_null = 0;
    if ( db == NULL ) {
        db_null = 1;
        
        db = open_db();
        if ( db == NULL ) { 
            ap2p_log(ERROR": did not pass in a db connection to state_get, and opening a new one failed\n");
            return -1;
        }
    }
    
    sqlite3_stmt* set_stmt;
    const char* set_sql = ""
    "INSERT INTO State (key, value) "
    "VALUES(?, ?) "
    "ON CONFLICT "
    "DO UPDATE SET value=?;";
    if ( prepare_sql_statement(db, &set_stmt, set_sql, &create_state_table) != SQLITE_OK ) { return NULL; }
    
    int bind_fail = 0;
    bind_fail |= (sqlite3_bind_text(set_stmt, 1, key,   strlen(key),   SQLITE_STATIC) != SQLITE_OK);
    bind_fail |= (sqlite3_bind_text(set_stmt, 2, value, strlen(value), SQLITE_STATIC) != SQLITE_OK);
    bind_fail |= (sqlite3_bind_text(set_stmt, 3, value, strlen(value), SQLITE_STATIC) != SQLITE_OK);
    if ( bind_fail ) {
        ap2p_log(FAILED_PARAM_BIND_ERR_MSG);
        return -1;
    }
    
    if ( sqlite3_step(set_stmt) != SQLITE_DONE ) {
        ap2p_log(FAILED_STMT_STEP_ERR_MSG);
        return -1;
    }
    sqlite3_finalize(set_stmt);
    
    if ( db_null ) {
        sqlite3_close(db);
    }
    
    return 0;
}

// non-zero on error
int ap2p_list_connections(Connection* buf, int* buf_len) {
    sqlite3 *db = open_db();
    if ( db == NULL ) { goto exit_err; }
    
    int res;
    sqlite3_stmt* conn_stmt;
    // prefer SELECT * over specific fields 
    // because this way will fail on any change to the Connections table
    // and remind you to update this function
    const char* select_sql = "SELECT * FROM Connections;";
    if ( prepare_sql_statement(db, &conn_stmt, select_sql, &create_conn_table) ) {
        goto exit_err_db;
    }
    
    int row_count = 0;
    while ( (res = sqlite3_step(conn_stmt)) == SQLITE_ROW ) {
        int status = sqlite3_column_int(conn_stmt, 9);
        
        char* peer_name;
        if ( status==accepted || status==self_review ) {
            peer_name = sqlite3_malloc(sqlite3_column_bytes(conn_stmt, 3)+1);
            strcpy(peer_name, (char*)sqlite3_column_text(conn_stmt, 3));
        } else {
            peer_name = NULL;
        }
        
        char* peer_addr = sqlite3_malloc(sqlite3_column_bytes(conn_stmt, 4)+1);
        strcpy(peer_addr, (char*)sqlite3_column_text(conn_stmt, 4));
        
        Connection conn = {
            .conn_id      = sqlite3_column_int64(conn_stmt, 0),
            .peer_id      = sqlite3_column_int64(conn_stmt, 1),
            .self_id      = sqlite3_column_int64(conn_stmt, 2),
            .peer_name    = peer_name,
            .peer_addr    = peer_addr,
            .peer_port    =   sqlite3_column_int(conn_stmt, 5),
            .online       =   sqlite3_column_int(conn_stmt, 6),
            .requested_at = sqlite3_column_int64(conn_stmt, 7),
            .updated_at   = sqlite3_column_int64(conn_stmt, 8),
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
        unsigned long content_len = sqlite3_column_bytes(msg_stmt, 5)+1;
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
int ap2p_request_connection(char* peer_addr, int peer_port) {
    long peer_id = generate_id();
    
    sqlite3 *db = open_db();
    if ( db == NULL ) { goto exit_err; }
    
    { // insert the conn into the db     
        sqlite3_stmt *insert_stmt;
        const char* insert_sql = "INSERT INTO Connections (peer_id, peer_addr, peer_port) VALUES (?, ?, ?);";
        if ( prepare_sql_statement(db, &insert_stmt, insert_sql, &create_conn_table) ) {
            goto exit_err_db;
        }
        
        int bind_fail = 0;
        bind_fail |= (sqlite3_bind_int64(insert_stmt, 1, peer_id) != SQLITE_OK);
        bind_fail |= (sqlite3_bind_text(insert_stmt, 2, peer_addr, strlen(peer_addr), SQLITE_STATIC) != SQLITE_OK);
        bind_fail |= (sqlite3_bind_int(insert_stmt, 3, peer_port) != SQLITE_OK);
        if ( bind_fail ) {
            ap2p_log(FAILED_PARAM_BIND_ERR_MSG);
            goto exit_err_db;
        }
        
        if ( sqlite3_step(insert_stmt) != SQLITE_DONE ) {
            ap2p_log(FAILED_STMT_STEP_ERR_MSG);
            goto exit_err_db;
        }
        sqlite3_finalize(insert_stmt);
    } // end inserting the conn into the db
    
    char* self_name = ap2p_state_get(db, "self_name");
    
    char* self_addr = ap2p_state_get(db, "self_addr");
    if ( self_addr == NULL ) {
        printf(ERROR": failed to retrieve self_addr from the State table\n");
        goto exit_err_db;
    }
    
    char* self_port_str = ap2p_state_get(db, "self_port");
    if ( self_port_str == NULL ) {
        printf(ERROR": failed to retrieve self_port from the State table\n");
        goto exit_err_db;
    }
    
    errno = 0;
    long self_port = strtol(self_port_str, NULL, 10);
    free(self_port_str);
    if ( errno != 0 ) {
        printf(ERROR": failed to convert self_port to long\n");
        goto exit_err_db;
    }
    
    unsigned char parcel[PARCEL_CONN_REQ_LEN] = {0};
    parcel[0] = PARCEL_CONN_REQ_KIND;
    pack_long(parcel+1, peer_id);
    strncpy((char*)parcel+9, self_name, MAX_HOST_NAME);
    strncpy((char*)parcel+73, self_addr, MAX_IP_ADDR_LEN);
    pack_int(parcel+89, self_port);
    
    free(self_name);
    free(self_addr);
    
    if ( send_parcel(parcel, PARCEL_CONN_REQ_LEN, peer_addr, peer_port) == 0 ) {
        ap2p_log(INFO": sent connection request to peer at %s:%d; connection is awaiting acknowledgement\n", peer_addr, peer_port);
    } else {
        ap2p_log(INFO": could not send connection request to peer at %s:%d; \x1b[33mconnection is pending\x1b[0m\n", peer_addr, peer_port);
    }
    
    sqlite3_close(db);
    return 0;
    
    exit_err_db:
        sqlite3_close(db);
    exit_err:
        return -1;
}

// decision: 0 for acc, non-zero for rej
int ap2p_decide_on_connection(long conn_id, int decision) {
    char* peer_addr;
    int peer_port;
    long self_id;
    int conn_status;
    
    sqlite3 *db = open_db();
    if ( db == NULL ) { goto exit_err; }
    { // retrieve conn info from db
        sqlite3_stmt *select_stmt;
        const char* select_sql = "SELECT self_id, peer_addr, peer_port, status FROM Connections WHERE conn_id=(?);";
        if ( prepare_sql_statement(db, &select_stmt, select_sql, &create_conn_table) ) {
            goto exit_err_db;
        }
        
        if ( sqlite3_bind_int64(select_stmt, 1, conn_id) != SQLITE_OK ) {
            ap2p_log(FAILED_PARAM_BIND_ERR_MSG);
            goto exit_err_db;
        }
        
        if ( sqlite3_step(select_stmt) == SQLITE_ROW ) {
            self_id = sqlite3_column_int64(select_stmt, 0);
            
            peer_addr = sqlite3_malloc(sqlite3_column_bytes(select_stmt, 1)+1);
            strcpy(peer_addr, (char*)sqlite3_column_text(select_stmt, 1));
            
            peer_port = sqlite3_column_int(select_stmt, 2);
            conn_status = sqlite3_column_int(select_stmt, 3);
        }
        if ( sqlite3_step(select_stmt) != SQLITE_DONE ) {
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
        
        if ( send_parcel(parcel, PARCEL_CONN_REJ_LEN, peer_addr, peer_port) == 0 ) {
            ap2p_log(INFO": rejected connection request from peer at %s\n", peer_addr);
        } else {
            ap2p_log(INFO": marked connection request from peer at %s as rejected, \x1b[33mbut\x1b[0m could not communicate it to the peer\n", peer_addr);
        }
    } else { // accepted
        long peer_id = generate_id();
        char* self_name = ap2p_state_get(db, "self_name");
        
        { // update conn in db
            sqlite3_stmt *update_stmt;
            const char* update_sql = ""
            "UPDATE Connections "
            "SET updated_at=(strftime('%s', 'now')), peer_id=(?), status=0 "
            "WHERE conn_id=(?);";
            if ( prepare_sql_statement(db, &update_stmt, update_sql, &create_conn_table) ) {
                goto exit_err_db;
            }
            
            int bind_fail = 0;
            bind_fail |= (sqlite3_bind_int64(update_stmt, 1, peer_id) != SQLITE_OK);
            bind_fail |= (sqlite3_bind_int64(update_stmt, 2, conn_id) != SQLITE_OK);
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
        
        free(self_name);
        
        if ( send_parcel(parcel, PARCEL_CONN_ACC_LEN, peer_addr, peer_port) == 0 ) {
            ap2p_log(INFO": accepted connection request from peer at %s\n", peer_addr);
        } else {
            ap2p_log(INFO": marked connection request from peer at %s as accepted, \x1b[33mbut\x1b[0m could not communicate it to the peer\n", peer_addr);
        }
    }
    
    sqlite3_close(db);
    return 0;
    
    exit_err_db:
        sqlite3_close(db);
    exit_err:
        return -1;
}

int ap2p_send_message(unsigned char content_type, int content_len, unsigned char* content) {
    sqlite3 *db = open_db();
    if ( db == NULL ) { goto exit_err; }
    
    sqlite3_stmt* insert_stmt;
    char* insert_sql = ""
    "INSERT INTO Messages "
    "(conn_id, content_type, content) VALUES "
    "("
        "(SELECT value FROM State WHERE key='selected_conn'), "
        "?, "
        "?"
    ");";
    if ( prepare_sql_statement(db, &insert_stmt, insert_sql, &create_msg_table) ) { goto exit_err_db; }
    
    if ( 
        sqlite3_bind_int(insert_stmt, 1, content_type) != SQLITE_OK ||
        sqlite3_bind_blob(insert_stmt, 2, content, content_len, SQLITE_STATIC) != SQLITE_OK
    ) {
        ap2p_log(FAILED_PARAM_BIND_ERR_MSG);
        return -1;
    }
        
    if ( sqlite3_step(insert_stmt) != SQLITE_DONE ) {
        ap2p_log(FAILED_STMT_STEP_ERR_MSG);
        return -1;
    }
    sqlite3_finalize(insert_stmt);
    
    long self_id;
    char peer_addr[MAX_IP_ADDR_LEN] = {0};
    int peer_port;
    char peer_name[MAX_HOST_NAME] = {0};
    
    sqlite3_stmt* select_stmt;
    char* select_sql = ""
    "SELECT status, self_id, peer_addr, peer_port, peer_name FROM Connections "
    "WHERE conn_id = (SELECT value FROM State WHERE key='selected_conn');";
    if ( prepare_sql_statement(db, &select_stmt, select_sql, &create_msg_table) ) { goto exit_err_db; }
    
    if ( sqlite3_step(select_stmt) == SQLITE_ROW ) {
        int status = sqlite3_column_int(select_stmt, 0);
        if ( status != accepted ) {
            ap2p_log(ERROR": attempted to send on connection which wasn't in the accepted state\n");
            goto exit_err_db;
        }
        
        self_id = sqlite3_column_int64(select_stmt, 1);
        
        strcpy(peer_addr, (char*)sqlite3_column_text(select_stmt, 2));
        
        peer_port = sqlite3_column_int(select_stmt, 3);
        
        strcpy(peer_name, (char*)sqlite3_column_text(select_stmt, 4));
    } else {
        ap2p_log(FAILED_STMT_STEP_ERR_MSG);
        return NULL;
    }
    sqlite3_finalize(select_stmt);
    
    {
        unsigned char parcel[PARCEL_MSG_SEND_HDR_LEN + content_len];
        parcel[0] = PARCEL_MSG_SEND_KIND;
        pack_long(parcel+1, self_id);
        parcel[9] = content_type;
        pack_int(parcel+10, content_len);
        memcpy(parcel+14, content, content_len);
        
        if ( send_parcel(parcel, PARCEL_MSG_SEND_HDR_LEN + content_len, peer_addr, peer_port) == 0 ) {
            ap2p_log(INFO": sent message of type %d to peer '%s'\n", content_type, peer_name);
        } else {
            ap2p_log(INFO": could not send message of type %d to peer '%s'; \x1b[33mmessage is pending\x1b[0m\n", content_type, peer_name);
        }
    }
    
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
    
    char* self_port_str = ap2p_state_get(db, "self_port");
    if ( self_port_str == NULL ) {
        printf(ERROR": failed to retrieve self_port from the State table\n");
        goto exit_err_db;
    }
    
    errno = 0;
    long self_port = strtol(self_port_str, NULL, 10);
    free(self_port_str);
    if ( errno != 0 ) {
        printf(ERROR": failed to convert self_port to long\n");
        goto exit_err_db;
    }
    
    char* listen_addr = ap2p_state_get(db, "listen_addr");
    if ( listen_addr == NULL ) {
        printf(ERROR": failed to retrieve listen_addr from the State table\n");
        goto exit_err_db;
    }
    
    struct sockaddr_in listening_addr = {
        .sin_family = AF_INET,
        .sin_addr.s_addr = inet_addr(listen_addr),
        .sin_port = htons(self_port),
    };
    if (bind(listening_sock, (struct sockaddr *)&listening_addr, sizeof(listening_addr)) < 0) {
      ap2p_log(ERROR": failed to bind server socket; %s\n", strerror(errno));
      goto exit_err_net;
    }
  
    if (listen(listening_sock, 1) < 0) {
      ap2p_log(ERROR": failed to listen on peer socket; %s\n", strerror(errno));
      goto exit_err_net;
    }
    ap2p_log(INFO": Listening for parcels at %s:%ld...\n", listen_addr, self_port);
    free(listen_addr);
    
    struct sockaddr_in incoming_addr;
    int incoming_addr_len = sizeof(incoming_addr);
    while (1) { // TODO: make interuptible with non-blocking accept() and getchar()
        int incoming_sock = accept(listening_sock, (struct sockaddr*)&incoming_addr, (socklen_t*)&incoming_addr_len);
        char incoming_addr_str[MAX_IP_ADDR_LEN];
        inet_ntop(AF_INET, &incoming_addr.sin_addr, incoming_addr_str, MAX_IP_ADDR_LEN);
        
        char parcel_kind;
        // note that we only peek at parcel_kind without consuming the first byte
        // this makes parcel reading simpler as there's no need to offset PARCEL_LEN by one
        if ( recv(incoming_sock, &parcel_kind, 1, MSG_PEEK) < 1) {
            ap2p_log(WARN": could not read parcel kind\n");
            continue;
        }
        ap2p_log(DEBUG": conn from %s:%d with kind: %d\n", incoming_addr_str, incoming_addr.sin_port, parcel_kind);
        
        switch (parcel_kind) {
            break; case PARCEL_CONN_REQ_KIND: {
                ap2p_log(INFO": recieved a CONN_REQ parcel\n");
                unsigned char req_parcel[PARCEL_CONN_REQ_LEN];
                if ( recv_parcel(incoming_sock, req_parcel, PARCEL_CONN_REQ_LEN) ) { continue; }

                long self_id = 0;
                unpack_long(self_id, req_parcel+1);
                
                char peer_name[MAX_HOST_NAME] = {0};
                strncpy(peer_name, (char*)req_parcel+9, MAX_HOST_NAME);
                
                char peer_addr[MAX_IP_ADDR_LEN] = {0};
                strncpy(peer_addr, (char*)req_parcel+73, MAX_IP_ADDR_LEN);
                
                int peer_port = 0;
                unpack_int(peer_port, req_parcel+89);

                ap2p_log(DEBUG": conn request [self_id: %ld, peer_name: %s, peer_addr: %s, peer_port: %d] \n", self_id, peer_name, peer_addr, peer_port);
                
                sqlite3_stmt *insert_stmt;
                const char* insert_sql = "INSERT INTO Connections (self_id, peer_name, peer_addr, peer_port, status) VALUES (?, ?, ?, ?, 2);";
                if ( prepare_sql_statement(db, &insert_stmt, insert_sql, &create_conn_table) ) {
                    continue;
                }
                
                int bind_fail = 0;
                bind_fail |= (sqlite3_bind_int64(insert_stmt, 1, self_id) != SQLITE_OK);
                bind_fail |= (sqlite3_bind_text(insert_stmt, 2, peer_name, strlen(peer_name), SQLITE_STATIC) != SQLITE_OK);
                bind_fail |= (sqlite3_bind_text(insert_stmt, 3, peer_addr, strlen(peer_addr), SQLITE_STATIC) != SQLITE_OK);
                bind_fail |= (sqlite3_bind_int(insert_stmt, 4, peer_port) != SQLITE_OK);
                if ( bind_fail ) {
                    ap2p_log(FAILED_PARAM_BIND_ERR_MSG);
                    continue;
                }
                
                if ( sqlite3_step(insert_stmt) != SQLITE_DONE ) {
                    ap2p_log(FAILED_STMT_STEP_ERR_MSG);
                    continue;
                }
                sqlite3_finalize(insert_stmt);
                ap2p_log(DEBUG": inserted requested conn into the db, with self_id: %ld, peer_name: %s, peer_addr: %s, peer_port: %d\n", self_id, peer_name, peer_addr, peer_port);
                
                unsigned char ack_parcel[PARCEL_CONN_ACK_LEN] = {0};
                ack_parcel[0] = PARCEL_CONN_ACK_KIND;
                pack_long(ack_parcel+1, self_id);
                
                if ( send_parcel(ack_parcel, PARCEL_CONN_ACK_LEN, peer_addr, peer_port) == 0 ) {
                    ap2p_log(INFO": acknowledged connection request from peer at %s:%d\n", peer_addr, peer_port);
                } else {
                    ap2p_log(WARN": failed to acknowledge connection request from peer at %s:%d\n", peer_addr, peer_port);
                }
                
            } break; case PARCEL_CONN_ACK_KIND: {
                ap2p_log(INFO": recieved a CONN_ACK parcel\n");
                unsigned char ack_parcel[PARCEL_CONN_ACK_LEN];
                if ( recv_parcel(incoming_sock, ack_parcel, PARCEL_CONN_ACK_LEN) ) { continue; }

                long peer_id = 0;
                unpack_long(peer_id, ack_parcel+1);

                ap2p_log(DEBUG": peer with ID %ld acked conn req\n", peer_id);
                
                sqlite3_stmt *update_stmt;
                const char* update_sql = "UPDATE Connections SET updated_at=(strftime('%s', 'now')), status=3 WHERE peer_id=(?);";
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
                unsigned char rej_parcel[PARCEL_CONN_REJ_LEN];
                if ( recv_parcel(incoming_sock, rej_parcel, PARCEL_CONN_REJ_LEN) ) { continue; }

                long peer_id = 0;
                unpack_long(peer_id, rej_parcel+1);

                ap2p_log(DEBUG": peer with ID %ld rejected conn req\n", peer_id);
                
                sqlite3_stmt *update_stmt;
                const char* update_sql = "UPDATE Connections SET updated_at=(strftime('%s', 'now')), status=-1 WHERE peer_id=(?);";
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
                ap2p_log(DEBUG": updated conn to 'rejected' where peer_id=%ld\n", peer_id);
                
            } break; case PARCEL_CONN_ACC_KIND: {
                ap2p_log(INFO": recieved a CONN_ACC parcel\n");
                unsigned char acc_parcel[PARCEL_CONN_ACC_LEN];
                if ( recv_parcel(incoming_sock, acc_parcel, PARCEL_CONN_ACC_LEN) ) { continue; }

                long peer_id = 0;
                unpack_long(peer_id, acc_parcel+1);
                
                long self_id = 0;
                unpack_long(self_id, acc_parcel+9);
                
                char peer_name[MAX_HOST_NAME] = {0};
                strncpy(peer_name, (char*)acc_parcel+17, MAX_HOST_NAME);
                
                ap2p_log(DEBUG": peer with ID %ld accepted conn req with self_id: %ld and peer_name: %s\n", peer_id, self_id, peer_name);
                
                sqlite3_stmt *update_stmt;
                const char* update_sql = "UPDATE Connections SET self_id=(?), peer_name=(?), updated_at=(strftime('%s', 'now')), status=0 WHERE peer_id=(?);";
                if ( prepare_sql_statement(db, &update_stmt, update_sql, &create_conn_table) ) {
                    continue;
                }
                
                if ( sqlite3_bind_int64(update_stmt, 1, peer_id) != SQLITE_OK ) {
                    ap2p_log(FAILED_PARAM_BIND_ERR_MSG);
                    continue;
                }
                int bind_fail = 0;
                bind_fail |= (sqlite3_bind_int64(update_stmt, 1, self_id) != SQLITE_OK);
                bind_fail |= (sqlite3_bind_text(update_stmt, 2, peer_name, strlen(peer_name), SQLITE_STATIC) != SQLITE_OK);
                bind_fail |= (sqlite3_bind_int64(update_stmt, 3, peer_id) != SQLITE_OK);
                if ( bind_fail ) {
                    ap2p_log(FAILED_PARAM_BIND_ERR_MSG);
                    continue;
                }
                
                if ( sqlite3_step(update_stmt) != SQLITE_DONE ) {
                    ap2p_log(FAILED_STMT_STEP_ERR_MSG);
                    continue;
                }
                sqlite3_finalize(update_stmt);
                ap2p_log(DEBUG": updated conn to 'accepted' where peer_id=%ld\n", peer_id);
            } break; case PARCEL_MSG_SEND_KIND: {
                ap2p_log(INFO": recieved a MSG_SEND parcel\n");
                
                unsigned char send_parcel_hdr[PARCEL_MSG_SEND_HDR_LEN];
                if ( recv_parcel(incoming_sock, send_parcel_hdr, PARCEL_MSG_SEND_HDR_LEN) ) { continue; }
                
                long peer_id = 0;
                unsigned char content_type;
                int content_len = 0;
                {
                    unpack_long(peer_id, send_parcel_hdr+1);
                    content_type = send_parcel_hdr[9];
                    unpack_int(content_len, send_parcel_hdr+10)
                }
                ap2p_log(DEBUG": msg_send header, peer_id: %ld, content_type: %d, content_len: %d\n", peer_id, content_type, content_len);
                
                char peer_addr[MAX_IP_ADDR_LEN] = {0};
                int peer_port;
                char peer_name[MAX_HOST_NAME] = {0};
                {
                    sqlite3_stmt* select_stmt;
                    char* select_sql = ""
                    "SELECT status, peer_addr, peer_port, peer_name FROM Connections "
                    "WHERE peer_id = ?;";
                    if ( prepare_sql_statement(db, &select_stmt, select_sql, &create_msg_table) ) { goto exit_err_db; }
                    
                    if ( sqlite3_bind_int64(select_stmt, 1, peer_id) != SQLITE_OK ) {
                        ap2p_log(FAILED_PARAM_BIND_ERR_MSG);
                        return -1;
                    }
                    
                    if ( sqlite3_step(select_stmt) == SQLITE_ROW ) {
                        int status = sqlite3_column_int(select_stmt, 0);
                        if ( status != accepted ) {
                            ap2p_log(ERROR": attempted to recieve message on connection which wasn't in the accepted state\n");
                            goto exit_err_db;
                        }
                        
                        strcpy(peer_addr, (char*)sqlite3_column_text(select_stmt, 1));
                        
                        peer_port = sqlite3_column_int(select_stmt, 2);
                        
                        strcpy(peer_name, (char*)sqlite3_column_text(select_stmt, 3));
                    } else {
                        ap2p_log(FAILED_STMT_STEP_ERR_MSG);
                        return NULL;
                    }
                    sqlite3_finalize(select_stmt);
                }
                
                ap2p_log(INFO": recieved message of type %d from peer '%s'\n", content_type, peer_name);
                
                unsigned char content[content_len];
                if ( recv(incoming_sock, content, content_len, 0) < content_len ) {
                    ap2p_log(ERROR": failed to read message contents\n");
                    continue;
                }
                
                {
                    sqlite3_stmt* insert_stmt;
                    char* insert_sql = ""
                    "INSERT INTO Messages "
                    "(conn_id, time_recieved, content_type, content) VALUES "
                    "("
                        "(SELECT conn_id FROM Connections WHERE peer_id=?), "
                        "(strftime('%s', 'now')), "
                        "?, "
                        "?"
                    ");";
                    if ( prepare_sql_statement(db, &insert_stmt, insert_sql, &create_msg_table) ) { goto exit_err_db; }
                    
                    if ( 
                        sqlite3_bind_int64(insert_stmt, 1, peer_id) != SQLITE_OK ||
                        sqlite3_bind_int(insert_stmt, 2, content_type) != SQLITE_OK ||
                        sqlite3_bind_blob(insert_stmt, 3, content, content_len, SQLITE_STATIC) != SQLITE_OK
                    ) {
                        ap2p_log(FAILED_PARAM_BIND_ERR_MSG);
                        return -1;
                    }
                        
                    if ( sqlite3_step(insert_stmt) != SQLITE_DONE ) {
                        ap2p_log(FAILED_STMT_STEP_ERR_MSG);
                        return -1;
                    }
                    sqlite3_finalize(insert_stmt);
                }
                
                // TODO: ack the msg
                
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

// expose strlen and free to anybody binding libap2p, in a self-contained way
unsigned long ap2p_strlen(const char* s) {
    return strlen(s);
}
void ap2p_free(void* p) {
    free(p);
}
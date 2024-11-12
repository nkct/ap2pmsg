#include <stdio.h>
#include <time.h>
#include <errno.h>
#include <arpa/inet.h>
#include <string.h>
#include <unistd.h>
#include <stdlib.h>

#include "sqlite3/sqlite3.h"

#define startswith(str, pat) (strncmp((str), (pat), strlen((pat))) == 0)

// ============ Generic Constants ===================
#define DB_FILE "ap2p_storage.db"

#define MAX_HOST_NAME 64
#define MAX_IP_ADDR_LEN 16
// ==================================================

// =========== Error Handling and Logging ===========
#define LOG_ERROR "\x1b[31mERROR\x1b[0m"
#define LOG_WARN "\x1b[33mWARN\x1b[0m"
#define LOG_INFO  "\x1b[34mINFO\x1b[0m"
#define LOG_DEBUG  "\x1b[36mDEBUG\x1b[0m"

#define SQL_ERR_FMT "%s (%d)"
#define SQL_ERR(db) sqlite3_errmsg((db)), sqlite3_errcode((db))
#define NET_ERR_FMT "at %s:%d; %s"
#define NET_ERR(addr, port) (addr), (port), strerror(errno)

#define FAILED_DB_OPEN_ERR_MSG LOG_ERROR": could not open database at '%s'\n", DB_FILE
#define FAILED_PREPARE_STMT_ERR_MSG(sql) LOG_ERROR": failed to prepare statement from '%s', " SQL_ERR_FMT "\n", (sql), SQL_ERR(db)
#define FAILED_STMT_STEP_ERR_MSG LOG_ERROR": failed while evaluating the statement; " SQL_ERR_FMT "\n", SQL_ERR(db)
#define FAILED_PARAM_BIND_ERR_MSG LOG_ERROR": failed to bind parameters; " SQL_ERR_FMT "\n", SQL_ERR(db)

#define LOG_OUT "./ap2p_log.txt"
#define ap2p_log(...) fprintf(fopen(LOG_OUT, "w"), __VA_ARGS__);
// ==================================================

// ================ Parcel Handling =================
#define pack_long(buf, d)                 \
for (int i=0;i<8;i++) {                   \
    (buf)[i] = ((d) >> (8*(7-i))) & 0xFF; \
}

#define pack_int(buf, d)                  \
for (int i=0;i<4;i++) {                   \
    (buf)[i] = ((d) >> (8*(3-i))) & 0xFF; \
}

#define unpack_long(d, buf)      \
for (int i=0;i<8;i++) {          \
    (d) = ((d) << 8) + (buf)[i]; \
}

#define unpack_int(d, buf)      \
for (int i=0;i<4;i++) {          \
    (d) = ((d) << 8) + (buf)[i]; \
}

extern inline int send_parcel(unsigned char* parcel, unsigned long parcel_len, char* addr, int port) {
    if (parcel_len == 0) { return 0; }
    
    ap2p_log(LOG_DEBUG": sending parcel: [");
    for (unsigned long i = 0; i < parcel_len; i++) {
        ap2p_log("%d, ", parcel[i]);
    }
    ap2p_log("]\n");
    
    int peer_sock = socket(AF_INET, SOCK_STREAM, 0);
    if (peer_sock < 0) {
        ap2p_log(LOG_ERROR": failed to create peer socket; %s\n", strerror(errno));
        close(peer_sock);
        return -1;
    }
    
    struct sockaddr_in peer_sockaddr;
    peer_sockaddr.sin_family = AF_INET;
    peer_sockaddr.sin_port = htons(port);
    peer_sockaddr.sin_addr.s_addr = inet_addr(addr);
    
    if ( connect(peer_sock, (struct sockaddr*)&peer_sockaddr, sizeof(peer_sockaddr)) != 0 ) {
        ap2p_log(LOG_WARN": could not connect " NET_ERR_FMT "\n", NET_ERR(addr, port));
        close(peer_sock);
        return -1;
    }
    
    if ( (unsigned)send(peer_sock, (void*)parcel, parcel_len, 0) == parcel_len) {
        ap2p_log(LOG_DEBUG": sent parcel of kind %d to %s:%d\n", parcel[0], addr, port);
    } else {
        ap2p_log(LOG_WARN": could not send parcel " NET_ERR_FMT "\n", NET_ERR(addr, port));
        close(peer_sock);
        return -1;
    }
    
    return 0;
}
extern inline int recv_parcel(int sock, unsigned char* parcel, unsigned long parcel_len) {
    if ( (unsigned)recv(sock, (void*)parcel, parcel_len, 0) < parcel_len ) {
        ap2p_log(LOG_WARN": could not read parcel contents; %s\n", strerror(errno));
        return -1;
    }
    ap2p_log(LOG_DEBUG": parcel: [");
    for (unsigned long i = 0; i<parcel_len; i++) {
        ap2p_log("%d, ", parcel[i]);
    }
    ap2p_log("]\n");
    
    return 0;
}
// ==================================================

// ============= Database Handling ==================
// logs every executed sql query, set in open_db()
extern inline int trace_callback(
    __attribute__((unused)) unsigned int T, 
    __attribute__((unused)) void* C, 
    void* P, 
    __attribute__((unused)) void* X
) {
    ap2p_log(LOG_DEBUG": executing query: '%s'\n", sqlite3_expanded_sql((sqlite3_stmt*)P));
    return 0;
}

extern inline sqlite3* open_db() {
    sqlite3* db;
    if ( sqlite3_open(DB_FILE, &db) ) {
        ap2p_log(FAILED_DB_OPEN_ERR_MSG);
        return NULL;
    }
    sqlite3_trace_v2(db, SQLITE_TRACE_STMT, trace_callback, NULL);
    
    return db;
}
extern inline int prepare_sql_statement(sqlite3* db, sqlite3_stmt** stmt, const char* sql, int create_table(sqlite3*)) {
    if (sql[strlen(sql)-1] != ';') {
        printf(LOG_WARN": no semicolon at the end of the sql\n");
    }

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
        ap2p_log(FAILED_PREPARE_STMT_ERR_MSG(sql));
        return -1;
    }
    
    return 0;
}
// ==================================================

extern inline long generate_id() {
    // TODO: more sophisticated peer_id generation
    // which would ensure non-repeatability
    srandom(time(NULL));
    return random();
}
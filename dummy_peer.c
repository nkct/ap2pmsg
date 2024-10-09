#include <arpa/inet.h>
#include <string.h>
#include <netinet/in.h>
#include <stdio.h>
#include <sys/socket.h>
#include <time.h>
#include <stdlib.h>

/* Reverse the byte order of an unsigned short. */
#define revbo_u16(d) (((d & 0xff) << 8) | (d >> 8))

int main() {
  int peer_sock = socket(AF_INET, SOCK_STREAM, 0);
  if (peer_sock < 0) {
    printf("peer socket creation failed\n");
    return -1;
  }

  struct sockaddr_in peer_addr = {
      .sin_family = AF_INET,
      .sin_addr.s_addr = inet_addr("127.0.0.1"),
      .sin_port = revbo_u16(7676),
  };

  if (bind(peer_sock, (struct sockaddr *)&peer_addr, sizeof(peer_addr)) < 0) {
    printf("Failed to bind server socket");
    exit(-1);
  }

  if (listen(peer_sock, 1) < 0) {
    printf("Failed to listen on peer socket");
    exit(-1);
  }
  printf("Listening...\n");
  
  struct sockaddr_in client_addr;
  int client_addr_len = sizeof(client_addr);
  while (1) {
      int client_sock = accept(peer_sock, (struct sockaddr*)&client_addr, (socklen_t*)&client_addr_len);
      
      char buf[2048];
      int buf_len;
      buf_len = recv(client_sock, buf, 2048, 0);
      buf[buf_len] = '\0';
      printf("recieved: %s\n", buf);
      printf("bytes: [");
      for (int i=0; i<buf_len; i++) {
          printf("%d,", buf[i]);
      }
      printf("]\n");
      
      
      #define MAX_SELF_NAME 64 // in bytes
      #define PARCEL_CONN_EST_KIND 1 // establish conn
      #define PARCEL_CONN_EST_LEN 73 // 1 + 8 + 64
      
      srandom(time(NULL)+1);
      long peer_id = random();
      printf("peer_id: %ld\n", peer_id);
      const char* self_name = "the_apple_of_eve";
      char resp[PARCEL_CONN_EST_LEN] = {0};
      {
          resp[0] = PARCEL_CONN_EST_KIND;
          
          for (int i=1;i<=4;i++) {
              resp[i] = (peer_id >> (8*(4-i))) & 0xFF;
          }

          strncpy(resp+5, self_name, MAX_SELF_NAME);
      }
      send(client_sock, resp, PARCEL_CONN_EST_LEN, 0);
      printf("sent %s back\n", resp);
      printf("bytes: [");
      for (int i=0; i<PARCEL_CONN_EST_LEN; i++) {
          printf("%d,", resp[i]);
      }
      printf("]\n");
  }
}
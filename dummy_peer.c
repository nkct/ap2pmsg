#include <arpa/inet.h>
#include <string.h>
#include <netinet/in.h>
#include <stdio.h>
#include <sys/socket.h>
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
      
      char* resp = "Dummy peer says hello :)";
      send(client_sock, resp, strlen(resp), 0);
      printf("sent %s back\n", resp);
  }
}
// SPDX-License-Identifier: GPL-3.0-or-later
// Standalone validation only: never linked into Runtime or used for admission.
#define _GNU_SOURCE
#include <sys/mman.h>
#include <sys/resource.h>
#include <signal.h>
#include <setjmp.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
static sigjmp_buf resume;
static volatile sig_atomic_t trapped;
static void illegal(int sig) { (void)sig; trapped++; siglongjmp(resume,1); }
int main(int argc,char **argv) {
  if(argc!=2) return 2;
  struct rlimit limit={0,0}; if(setrlimit(RLIMIT_CORE,&limit)) return 3;
  struct sigaction action={0};action.sa_handler=illegal;sigemptyset(&action.sa_mask);if(sigaction(SIGILL,&action,0)) return 4;
  long page=sysconf(_SC_PAGESIZE);uint32_t *code=mmap(0,page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(code==MAP_FAILED)return 5;
  FILE *input=fopen(argv[1],"r");if(!input)return 6;
  unsigned word;unsigned count=0,unexpected=0;
  while(fscanf(input,"%x",&word)==1) {
    if(mprotect(code,page,PROT_READ|PROT_WRITE))return 7;
    code[0]=0xd503245f;code[1]=word;code[2]=0xd65f03c0;
    __builtin___clear_cache((char*)code,(char*)(code+3));
    if(mprotect(code,page,PROT_READ|PROT_EXEC))return 8;
    if(!sigsetjmp(resume,1)) { ((void(*)(void))code)();unexpected++; }
    count++;
  }
  fclose(input);munmap(code,page);
  printf("{\"encodings\":%u,\"sigill\":%d,\"unexpected_success\":%u}\n",count,trapped,unexpected);
  return unexpected || count!=1776 || trapped!=1776;
}

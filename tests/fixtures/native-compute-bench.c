// SPDX-License-Identifier: GPL-3.0-or-later
/* The same guest executable and bytes with computation thunks off/on. */
#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
extern unsigned long crc32(unsigned long,const unsigned char*,unsigned);
extern int compress2(unsigned char*,unsigned long*,const unsigned char*,unsigned long,int);
extern int uncompress(unsigned char*,unsigned long*,const unsigned char*,unsigned long);
extern size_t ZSTD_compress(void*,size_t,const void*,size_t,int);
extern size_t ZSTD_decompress(void*,size_t,const void*,size_t);
extern unsigned ZSTD_isError(size_t);
extern unsigned lzma_easy_buffer_encode(uint32_t,unsigned,const void*,const uint8_t*,size_t,uint8_t*,size_t*,size_t);
extern unsigned lzma_stream_buffer_decode(uint64_t*,uint32_t,const void*,const uint8_t*,size_t*,size_t,uint8_t*,size_t*,size_t);
typedef struct {uint32_t h[8],Nl,Nh,data[16],num,md_len;} SHA256_CTX;
extern int SHA256_Init(SHA256_CTX*);
extern int SHA256_Update(SHA256_CTX*,const void*,size_t);
extern int SHA256_Final(unsigned char*,SHA256_CTX*);
typedef struct {uint32_t words[60];int rounds;} AES_KEY;
extern int AES_set_encrypt_key(const unsigned char*,int,AES_KEY*);
extern int AES_set_decrypt_key(const unsigned char*,int,AES_KEY*);
extern void AES_cbc_encrypt(const unsigned char*,unsigned char*,size_t,const AES_KEY*,unsigned char*,int);
#define N 65536
static unsigned char input[N],encoded[N*2],decoded[N],hash[32];
static uint64_t stamp(void){struct timespec t;if(clock_gettime(CLOCK_MONOTONIC,&t))return 0;return (uint64_t)t.tv_sec*1000000000+t.tv_nsec;}
static void report(const char *name,uint64_t before,size_t out){uint64_t elapsed=stamp()-before;printf("{\"operation\":\"%s\",\"bytes\":%u,\"output_bytes\":%zu,\"nanoseconds\":%llu}\n",name,N,out,(unsigned long long)elapsed);}
int main(void){
    uint32_t random=0x6d2b79f5;
    for(unsigned i=0;i<N;i++){random^=random<<13;random^=random>>17;random^=random<<5;input[i]=i%8192<2048?(unsigned char)random:(unsigned char)(i%251);}
    uint64_t before=stamp();unsigned long size=sizeof encoded;
    if(compress2(encoded,&size,input,N,6))return 1;report("zlib-compress",before,size);
    unsigned long out=N;before=stamp();int zr=uncompress(decoded,&out,encoded,size);report("zlib-decompress",before,out);if(zr||out!=N||memcmp(decoded,input,N))return 2;
    before=stamp();size=ZSTD_compress(encoded,sizeof encoded,input,N,3);if(ZSTD_isError(size))return 3;report("zstd-compress",before,size);
    before=stamp();size_t zd=ZSTD_decompress(decoded,N,encoded,size);report("zstd-decompress",before,N);if(zd!=N||memcmp(decoded,input,N))return 4;
    size_t written=0,read=0;before=stamp();if(lzma_easy_buffer_encode(3,4,0,input,N,encoded,&written,sizeof encoded))return 5;report("xz-compress",before,written);
    size_t produced=0;uint64_t limit=UINT64_MAX;before=stamp();unsigned lr=lzma_stream_buffer_decode(&limit,0,0,encoded,&read,written,decoded,&produced,N);report("xz-decompress",before,produced);if(lr||read!=written||produced!=N||memcmp(decoded,input,N))return 6;
    SHA256_CTX state;before=stamp();if(!SHA256_Init(&state)||!SHA256_Update(&state,input,N)||!SHA256_Final(hash,&state))return 7;report("sha256",before,sizeof hash);
    AES_KEY key;unsigned char iv[16]={0},secret[16]={0};if(AES_set_encrypt_key(secret,128,&key))return 8;
    before=stamp();AES_cbc_encrypt(input,encoded,N,&key,iv,1);report("aes128-cbc-encrypt",before,N);
    if(AES_set_decrypt_key(secret,128,&key))return 9;memset(iv,0,sizeof iv);AES_cbc_encrypt(encoded,decoded,N,&key,iv,0);if(memcmp(decoded,input,N))return 10;
    before=stamp();volatile unsigned long check=crc32(0,input,N);(void)check;report("crc32",before,4);
    puts("compute-benchmark-ok");return 0;
}

#include <zlib.h>
#include <lzma.h>
#include <zstd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#ifdef FORK_TEST
#include <sys/wait.h>
#include <unistd.h>
static int child_process,fork_done;
static void checkpoint(size_t pos){if(pos!=163840||fork_done)return;fork_done=1;pid_t p=fork();if(p<0)exit(90);if(!p){child_process=1;return;}int status=0;if(waitpid(p,&status,0)!=p||!WIFEXITED(status)||WEXITSTATUS(status))exit(91);}
#define STEP(pos) checkpoint(pos)
#define BEGIN_CODEC() (fork_done=0)
#define CHILD_DONE() do{if(child_process)_exit(0);}while(0)
#else
#define STEP(pos) ((void)0)
#define BEGIN_CODEC() ((void)0)
#define CHILD_DONE() ((void)0)
#endif
#ifndef N
#define N (1024*1024)
#endif
#ifndef INPUT_CHUNK
#define INPUT_CHUNK 16384
#endif
static unsigned long long stamp(void){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return (unsigned long long)t.tv_sec*1000000000+t.tv_nsec;}
static void report(const char*op,unsigned long long start){printf("{\"operation\":\"%s\",\"bytes\":%u,\"nanoseconds\":%llu}\n",op,N,stamp()-start);}
static unsigned char data[N], packed[N*2], result[N], chunk[65536];
#ifdef TRACE_TEST
static unsigned trace_calls;
unsigned long long ZSTD_trace_compress_begin(const ZSTD_CCtx*ctx){(void)ctx;trace_calls++;return 0;}
unsigned long long ZSTD_trace_decompress_begin(const ZSTD_DCtx*ctx){(void)ctx;trace_calls++;return 0;}
#endif
#define CHECK(x) do {if(!(x)){fprintf(stderr,"FAIL line %d\n",__LINE__);exit(1);}}while(0)
static void ztest(void) {
 BEGIN_CODEC();unsigned long long started=stamp();z_stream s={0};CHECK(deflateInit(&s,6)==Z_OK);size_t pos=0,written=0;int r;
 do {
  if(!s.avail_in&&pos<N){size_t n=N-pos;if(n>16384)n=16384;s.next_in=data+pos;s.avail_in=n;pos+=n;STEP(pos);}
  s.next_out=chunk;s.avail_out=sizeof(chunk);r=deflate(&s,pos==N?Z_FINISH:Z_NO_FLUSH);CHECK(r==Z_OK||r==Z_STREAM_END);
  size_t produced=sizeof(chunk)-s.avail_out;CHECK(written+produced<=sizeof(packed));memcpy(packed+written,chunk,produced);written+=produced;
 }while(r!=Z_STREAM_END);
 report("zlib-stream-compress",started);CHECK(s.total_out==written);size_t len=written;CHECK(deflateEnd(&s)==Z_OK);memset(&s,0,sizeof(s));CHECK(inflateInit(&s)==Z_OK);pos=0;started=stamp();
 do {size_t n=len-pos;if(n>INPUT_CHUNK)n=INPUT_CHUNK;s.next_in=packed+pos;s.avail_in=n;s.next_out=result+s.total_out;s.avail_out=sizeof(result)-s.total_out;pos+=n;r=inflate(&s,Z_NO_FLUSH);CHECK(r==Z_OK||r==Z_STREAM_END);}while(r!=Z_STREAM_END);
 report("zlib-stream-decompress",started);CHECK(s.total_out==N&&!memcmp(data,result,N));CHECK(inflateEnd(&s)==Z_OK);puts("zlib stream pass");CHILD_DONE();
}
static void xtest(void) {
 BEGIN_CODEC();unsigned long long started=stamp();lzma_stream s=LZMA_STREAM_INIT;CHECK(lzma_easy_encoder(&s,3,LZMA_CHECK_CRC64)==LZMA_OK);size_t pos=0,written=0;lzma_ret r;
 do {
  if(!s.avail_in&&pos<N){size_t n=N-pos;if(n>16384)n=16384;s.next_in=data+pos;s.avail_in=n;pos+=n;STEP(pos);}
  s.next_out=chunk;s.avail_out=sizeof(chunk);r=lzma_code(&s,pos==N?LZMA_FINISH:LZMA_RUN);CHECK(r==LZMA_OK||r==LZMA_STREAM_END);
  size_t produced=sizeof(chunk)-s.avail_out;CHECK(written+produced<=sizeof(packed));memcpy(packed+written,chunk,produced);written+=produced;
 }while(r!=LZMA_STREAM_END);
 report("xz-stream-compress",started);CHECK(s.total_out==written);size_t len=written;lzma_end(&s);s=(lzma_stream)LZMA_STREAM_INIT;CHECK(lzma_stream_decoder(&s,64*1024*1024,0)==LZMA_OK);pos=0;started=stamp();
 do {size_t n=len-pos;if(n>INPUT_CHUNK)n=INPUT_CHUNK;s.next_in=packed+pos;s.avail_in=n;s.next_out=result+s.total_out;s.avail_out=sizeof(result)-s.total_out;pos+=n;r=lzma_code(&s,LZMA_RUN);CHECK(r==LZMA_OK||r==LZMA_STREAM_END);}while(r!=LZMA_STREAM_END);
 report("xz-stream-decompress",started);CHECK(s.total_out==N&&!memcmp(data,result,N));lzma_end(&s);puts("xz stream pass");CHILD_DONE();
}
static void stest(void) {
 BEGIN_CODEC();unsigned long long started=stamp();ZSTD_CStream*c=ZSTD_createCStream();CHECK(c);CHECK(!ZSTD_isError(ZSTD_initCStream(c,0)));size_t pos=0,written=0,r;
 do {
  size_t n=N-pos;if(n>16384)n=16384;ZSTD_inBuffer in={data+pos,n,0};ZSTD_outBuffer out={chunk,sizeof(chunk),0};
  size_t before=pos;r=ZSTD_compressStream2(c,&out,&in,pos+n==N?ZSTD_e_end:ZSTD_e_continue);CHECK(!ZSTD_isError(r));pos+=in.pos;
  if(before<163840&&pos>=163840)STEP(163840);
  CHECK(written+out.pos<=sizeof(packed));memcpy(packed+written,chunk,out.pos);written+=out.pos;
 }while(r||pos<N);
 report("zstd-stream-compress",started);CHECK(!ZSTD_isError(ZSTD_freeCStream(c)));ZSTD_DStream*d=ZSTD_createDStream();CHECK(d);size_t len=written;pos=0;ZSTD_outBuffer out={result,sizeof(result),0};started=stamp();
 do {size_t n=len-pos;if(n>INPUT_CHUNK)n=INPUT_CHUNK;ZSTD_inBuffer in={packed+pos,n,0};pos+=n;r=ZSTD_decompressStream(d,&out,&in);CHECK(!ZSTD_isError(r)&&in.pos==in.size);}while(r);
 report("zstd-stream-decompress",started);CHECK(out.pos==N&&!memcmp(data,result,N));CHECK(!ZSTD_isError(ZSTD_freeDStream(d)));puts("zstd stream pass");CHILD_DONE();
}
static unsigned allocations,frees;
static voidpf own_alloc(voidpf unused,uInt count,uInt size){(void)unused;allocations++;return calloc(count,size);}
static void own_free(voidpf unused,voidpf ptr){(void)unused;frees++;free(ptr);}
static void controls(void){
 z_stream s={0};CHECK(deflateInit(&s,6)==Z_OK);
 s.next_in=data;s.avail_in=16384;s.next_out=packed;s.avail_out=sizeof(packed);
 CHECK(deflate(&s,Z_SYNC_FLUSH)==Z_OK);
 /* deflateParams is an original guest operation between native steps. */
 CHECK(deflateParams(&s,1,Z_DEFAULT_STRATEGY)==Z_OK);
 s.next_in=data+16384;s.avail_in=N-16384;int r;
 do{r=deflate(&s,Z_FINISH);CHECK(r==Z_OK||r==Z_STREAM_END);}while(r!=Z_STREAM_END);
 size_t len=s.total_out;CHECK(deflateEnd(&s)==Z_OK);
 s=(z_stream){0};s.zalloc=own_alloc;s.zfree=own_free;CHECK(inflateInit(&s)==Z_OK);
 s.next_in=packed;s.avail_in=len;s.next_out=result;s.avail_out=N;
 CHECK(inflate(&s,Z_FINISH)==Z_STREAM_END);CHECK(s.total_out==N&&!memcmp(data,result,N));CHECK(inflateEnd(&s)==Z_OK);
 CHECK(allocations&&allocations==frees);
 /* Reset and dictionary setup must retain ordinary guest ownership. */
 s=(z_stream){0};CHECK(deflateInit(&s,6)==Z_OK);CHECK(deflateSetDictionary(&s,data,512)==Z_OK);CHECK(deflateReset(&s)==Z_OK);
 s.next_in=data;s.avail_in=N;s.next_out=packed;s.avail_out=sizeof(packed);CHECK(deflate(&s,Z_FINISH)==Z_STREAM_END);len=s.total_out;CHECK(deflateEnd(&s)==Z_OK);
 s=(z_stream){0};CHECK(inflateInit(&s)==Z_OK);CHECK(inflateReset2(&s,15)==Z_OK);s.next_in=packed;s.avail_in=len;s.next_out=result;s.avail_out=N;CHECK(inflate(&s,Z_FINISH)==Z_STREAM_END);CHECK(!memcmp(data,result,N));CHECK(inflateEnd(&s)==Z_OK);
 for(int kind=0;kind<2;kind++){
  int bits=kind?31:-15;s=(z_stream){0};CHECK(deflateInit2(&s,6,Z_DEFLATED,bits,8,Z_DEFAULT_STRATEGY)==Z_OK);
  s.next_in=data;s.avail_in=N;size_t written=0;
  do{s.next_out=packed+written;s.avail_out=written<16?1:(sizeof(packed)-written);uInt available=s.avail_out;r=deflate(&s,Z_FINISH);written+=available-s.avail_out;CHECK(r==Z_OK||r==Z_STREAM_END);}while(r!=Z_STREAM_END);
  CHECK(deflateEnd(&s)==Z_OK);if(kind)CHECK(packed[9]==3);
  s=(z_stream){0};CHECK(inflateInit2(&s,kind?47:-15)==Z_OK);s.next_in=packed;s.avail_in=written;s.next_out=result;s.avail_out=N;CHECK(inflate(&s,Z_FINISH)==Z_STREAM_END);CHECK(!memcmp(data,result,N));CHECK(inflateEnd(&s)==Z_OK);
 }
 puts("controls and allocator callbacks pass");
}
int main(){unsigned random=0x6d2b79f5;for(size_t i=0;i<N;i++){random^=random<<13;random^=random>>17;random^=random<<5;data[i]=i%8192<2048?(unsigned char)random:(unsigned char)(i%251);}ztest();xtest();stest();
#ifdef CONTROL_TEST
controls();
#endif
#ifdef TRACE_TEST
CHECK(trace_calls>0);puts("zero-return trace callbacks pass");
#endif
return 0;}

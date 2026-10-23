#include "quietweft.h"
#include <stdio.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif
int main(int argc,char **argv) {
#ifdef _WIN32
 _setmode(_fileno(stdin), _O_BINARY);
 _setmode(_fileno(stdout), _O_BINARY);
#endif
 if(argc!=2){fprintf(stderr,"Usage: embed MODEL_DIRECTORY < input.f32 > output.f32\n");return 2;}
 Quietweft *qw=qw_create(argv[1]);
 if(!qw){fprintf(stderr,"%s\n",qw_last_create_error());return 1;}
 float in[128],out[128];size_t count;
 while((count=fread(in,sizeof(float),128,stdin))==128){
  if(qw_process_hop(qw,in,out)){fprintf(stderr,"%s\n",qw_last_error(qw));qw_destroy(qw);return 1;}
  if(fwrite(out,sizeof(float),128,stdout)!=128){qw_destroy(qw);return 1;}
 }
 qw_destroy(qw);return (count!=0||ferror(stdin))?1:0;
}

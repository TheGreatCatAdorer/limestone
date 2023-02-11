#define _GNU_SOURCE
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <sys/mman.h>
#include <unistd.h>
#include <spawn.h>
extern char**environ;
char*run(const char*restrict file,char*const argv[restrict]){
int status,len,output;pid_t pid;posix_spawn_file_actions_t acts;char*result;
if((output=memfd_create("run_output",0))==-1)exit(1);
posix_spawn_file_actions_init(&acts);
posix_spawn_file_actions_adddup2(&acts,output,STDOUT_FILENO);
if((status=posix_spawnp(&pid,file,&acts,NULL,argv,environ)))exit(status);
while(waitpid(pid,&status,0)==-1){}
posix_spawn_file_actions_destroy(&acts);
if((status=WEXITSTATUS(status)))exit(status);
result=malloc(len=lseek(output,0,SEEK_END));
lseek(output,0,SEEK_SET);
if(result==NULL&&len!=0)exit(1);
fread(result,1,len,fdopen(output, "r"));
close(output);close(output);
return result;}
int main(void){
{int lengths[1]={0};char*substrings[1]={0};int length=0;for(int i=0;i<0;)length+=lengths[i++];char*local_0=malloc(length);if(local_0==NULL&&length!=0)exit(1);char*ref=local_0;for(int i=0;i<0;)(memcpy(ref,substrings[i],lengths[i]),ref+=lengths[i++]);char*func="ls";char*args[2]={local_0,(char*)NULL};puts(run(func, args));}}

#include "grazer.h"
#include <assert.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
int main(int argc,char **argv) {
    assert(argc==3);assert(grazer_checkpoint_api_version()==GRAZER_CHECKPOINT_API_VERSION);
    FILE *file=fopen(argv[1],"rb");assert(file);assert(fseek(file,0,SEEK_END)==0);long size=ftell(file);assert(size>0&&size<256*1024*1024);rewind(file);
    uint8_t *bytes=malloc((size_t)size);assert(bytes);assert(fread(bytes,1,(size_t)size,file)==(size_t)size);fclose(file);
    GrazerReplayPlayer *player=NULL;assert(grazer_replay_load(NULL,0,&player)==GRAZER_INVALID_ARGUMENT&&player==NULL);
    assert(grazer_replay_load(bytes,(uint32_t)size,&player)==GRAZER_OK&&player);free(bytes);
    file=fopen(argv[2],"r");assert(file);uint32_t advanced;uint64_t hash;unsigned count=0;
    while(1) {assert(grazer_replay_step(player,&advanced)==GRAZER_OK);if(!advanced)break;uint64_t expected;assert(fscanf(file,"%" SCNx64,&expected)==1);assert(grazer_replay_state_hash(player,&hash)==GRAZER_OK&&hash==expected);count++;}
    assert(count==100000);fclose(file);GrazerReplayStatus status;assert(grazer_replay_status(player,&status)==GRAZER_OK&&status.frame==100000&&status.frames==100000&&!status.stopped);
    for(unsigned i=0;i<4;i++) {uint64_t frame=(uint64_t[]){0,28891,32491,100000}[i];assert(grazer_replay_seek(player,frame)==GRAZER_OK);assert(grazer_replay_status(player,&status)==GRAZER_OK&&status.frame==frame);}
    GrazerGame *game=NULL,*restored=NULL;assert(grazer_replay_clone_game(player,&game)==GRAZER_OK);uint32_t required;assert(grazer_game_checkpoint(game,NULL,0,&required)==GRAZER_BUFFER_TOO_SMALL);bytes=malloc(required);assert(bytes);uint32_t length=required;assert(grazer_game_checkpoint(game,bytes,length,&required)==GRAZER_OK);
    assert(grazer_game_restore_checkpoint(bytes,length,&restored)==GRAZER_OK);uint64_t original;assert(grazer_game_state_hash(game,&original)==GRAZER_OK);assert(grazer_game_state_hash(restored,&hash)==GRAZER_OK&&hash==original);
    bytes[20]^=1;GrazerGame *bad=NULL;assert(grazer_game_restore_checkpoint(bytes,length,&bad)==GRAZER_REPLAY_DATA_ERROR&&bad==NULL);free(bytes);
    assert(grazer_replay_seek(player,100001)==GRAZER_REPLAY_DATA_ERROR);assert(grazer_replay_state_hash(player,&hash)==GRAZER_OK&&hash==original);
    grazer_game_destroy(game);grazer_game_destroy(restored);grazer_replay_destroy(player);grazer_replay_destroy(NULL);
    printf("PASS M5 C: %u replay hashes, checkpoint/seek/clone, final=%016" PRIx64 "\n",count,hash);return 0;
}

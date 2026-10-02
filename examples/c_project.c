#include "grazer.h"
#include <assert.h>
#include <inttypes.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
_Static_assert(sizeof(GrazerGameSprite)==40,"sprite ABI");
_Static_assert(sizeof(GrazerHud)==64,"HUD ABI");
_Static_assert(offsetof(GrazerHud,health)==24,"HUD health offset");
_Static_assert(sizeof(GrazerReplayStatus)==48,"replay ABI");
int main(int argc,char **argv) {
    assert(argc==2);FILE *f=fopen(argv[1],"rb");assert(f);assert(fseek(f,0,SEEK_END)==0);long n=ftell(f);assert(n>0&&n<96*1024*1024);rewind(f);uint8_t *p=malloc((size_t)n);assert(p);assert(fread(p,1,(size_t)n,f)==(size_t)n);fclose(f);
    assert(grazer_project_api_version()==GRAZER_PROJECT_API_VERSION);GrazerGame *game=NULL;assert(grazer_game_create_project(p,(uint32_t)n,&game)==GRAZER_OK&&game);
    for(unsigned i=0;i<600;i++)assert(grazer_game_step(game,(GrazerGameInput){0,0,GRAZER_FIRE})==GRAZER_OK);
    GrazerHud hud;assert(grazer_game_hud(game,&hud)==GRAZER_OK);uint64_t hash;assert(grazer_game_state_hash(game,&hash)==GRAZER_OK);
    uint32_t required;assert(grazer_game_checkpoint(game,NULL,0,&required)==GRAZER_BUFFER_TOO_SMALL);uint8_t *c=malloc(required);assert(c);assert(grazer_game_checkpoint(game,c,required,&required)==GRAZER_OK);GrazerGame *restored=NULL;assert(grazer_game_restore_project(p,(uint32_t)n,c,required,&restored)==GRAZER_OK);uint64_t other;assert(grazer_game_state_hash(restored,&other)==GRAZER_OK&&other==hash);
    p[20]^=1;GrazerGame *bad=NULL;assert(grazer_game_create_project(p,(uint32_t)n,&bad)==GRAZER_REPLAY_DATA_ERROR&&bad==NULL);grazer_game_destroy(game);grazer_game_destroy(restored);free(c);free(p);printf("PASS C project: tick=%" PRIu64 " hash=%016" PRIx64 "\n",hud.tick,hash);return 0;
}

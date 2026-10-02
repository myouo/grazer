#include "grazer.h"
#include <assert.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(int argc, char **argv) {
    int advanced = argc > 2 && (strcmp(argv[2],"advanced") == 0 || strcmp(argv[2],"advanced-bytecode") == 0);
    int scripted = advanced || (argc > 2 && (strcmp(argv[2],"script") == 0 || strcmp(argv[2],"bytecode") == 0));
    _Static_assert(sizeof(GrazerAdvancedHud) == 48, "M4 HUD layout");
    _Static_assert(sizeof(GrazerLaserSegment) == 40, "M4 laser layout");
    assert(grazer_game_abi_version() == GRAZER_GAME_ABI_VERSION);
    GrazerGameConfig config = {GRAZER_GAME_ABI_VERSION, sizeof(config), 42, 512, 10000};
    if (advanced) config.projectile_capacity = 1024;
    GrazerGame *game = NULL;
    assert(grazer_game_create(NULL, &game) == GRAZER_INVALID_ARGUMENT && game == NULL);
    config.abi_version = 99;
    assert(grazer_game_create(&config, &game) == GRAZER_VERSION_MISMATCH && game == NULL);
    config.abi_version = GRAZER_GAME_ABI_VERSION;
    if (scripted) {
        assert(grazer_script_api_version() == GRAZER_SCRIPT_API_VERSION);
        GrazerScriptDiagnostic diagnostic;
        const char bad[] = "task main() { let n: int = true; }";
        assert(grazer_game_create_script(&config,(const uint8_t*)bad,sizeof(bad)-1,0,&game,&diagnostic) == GRAZER_SCRIPT_ERROR && game == NULL);
        assert(diagnostic.kind == 2 && diagnostic.line == 1 && diagnostic.column > 0);
        const char loop[] = "task main() { while true {} }";
        assert(grazer_game_create_script(&config,(const uint8_t*)loop,sizeof(loop)-1,0,&game,&diagnostic) == GRAZER_OK);
        assert(grazer_game_step(game,(GrazerGameInput){0,0,0}) == GRAZER_RUNTIME_ERROR);
        assert(grazer_game_diagnostic(game,&diagnostic) == GRAZER_OK && diagnostic.kind == 9 && diagnostic.line == 1);
        GrazerHud stopped;
        assert(grazer_game_hud(game,&stopped) == GRAZER_OK && stopped.phase == GRAZER_FAULTED && stopped.tick == 0);
        uint8_t message[1024]; uint32_t length;
        assert(grazer_game_diagnostic_text(game,message,sizeof(message),&length) == GRAZER_OK);
        assert(strstr((const char*)message,"c-stage.graze:1:") != NULL);
        grazer_game_destroy(game);game=NULL;
        if (advanced) {
            assert(grazer_advanced_api_version() == GRAZER_ADVANCED_API_VERSION);
            assert(grazer_game_create_advanced(&config,NULL,0,0,3,&game,&diagnostic) == GRAZER_INVALID_ARGUMENT && game == NULL);
        }
        if (argc > 3 && (strcmp(argv[2],"bytecode") == 0 || strcmp(argv[2],"advanced-bytecode") == 0)) {
            FILE *file = fopen(argv[3],"rb"); assert(file != NULL);
            assert(fseek(file,0,SEEK_END) == 0); long size=ftell(file);assert(size > 0 && size <= 16*1024*1024);
            assert(fseek(file,0,SEEK_SET) == 0);uint8_t *bytes=malloc((size_t)size);assert(bytes != NULL);
            assert(fread(bytes,1,(size_t)size,file) == (size_t)size);fclose(file);
            if (advanced) assert(grazer_game_create_advanced(&config,bytes,(uint32_t)size,1,GRAZER_NORMAL,&game,&diagnostic) == GRAZER_OK && diagnostic.kind == 0);
            else assert(grazer_game_create_script(&config,bytes,(uint32_t)size,1,&game,&diagnostic) == GRAZER_OK && diagnostic.kind == 0);
            free(bytes);
        } else if (advanced) assert(grazer_game_create_advanced(&config,NULL,0,0,GRAZER_NORMAL,&game,&diagnostic) == GRAZER_OK && diagnostic.kind == 0);
        else assert(grazer_game_create_script(&config,NULL,0,0,&game,&diagnostic) == GRAZER_OK && diagnostic.kind == 0);
    } else assert(grazer_game_create(&config, &game) == GRAZER_OK && game != NULL);
    uint64_t initial, hash;
    assert(grazer_game_state_hash(game, &initial) == GRAZER_OK);
    assert(grazer_game_step(game, (GrazerGameInput){2,0,0}) == GRAZER_INVALID_ARGUMENT);
    assert(grazer_game_step(game, (GrazerGameInput){0,0,16}) == GRAZER_INVALID_ARGUMENT);
    assert(grazer_game_state_hash(game, &hash) == GRAZER_OK && hash == initial);
    GrazerResourceInfo info;
    assert(grazer_game_resource_info(game, &info) == GRAZER_OK);
    assert(info.version == 1 && info.width == 128 && info.height == 128 && info.atlas_bytes == 65536);
    uint32_t required = 0;
    assert(grazer_game_atlas(game,NULL,0,&required) == GRAZER_BUFFER_TOO_SMALL && required == info.atlas_bytes);
    uint8_t atlas[65536];
    assert(grazer_game_atlas(game,atlas,sizeof(atlas),&required) == GRAZER_OK);
    unsigned visible = 0;
    for (unsigned i = 3; i < sizeof(atlas); i += 4) visible += atlas[i] != 0;
    assert(visible > 1000);
    GrazerSpriteAsset sprite_assets[128]; GrazerSoundAsset sound_assets[16];
    assert(grazer_game_sprite_assets(game,sprite_assets,128,&required) == GRAZER_OK && required == info.sprites);
    assert(grazer_game_sound_assets(game,sound_assets,16,&required) == GRAZER_OK && required == info.sounds);
    for (unsigned i=1;i<info.sprites;i++) assert(sprite_assets[i-1].id < sprite_assets[i].id);
    GrazerGameSprite sprites[2048], sentinel;
    memset(&sentinel,0x5a,sizeof(sentinel));
    assert(grazer_game_snapshot(game,NULL,0,&required) == GRAZER_BUFFER_TOO_SMALL && required == 1);
    assert(grazer_game_snapshot(game,sprites,1024,&required) == GRAZER_OK);
    assert(sprites[0].resource_id == 1 && sprites[0].x == 240.0f && sprites[0].y == 560.0f);
    FILE *trace = argc > 1 ? fopen(argv[1],"r") : NULL;
    if (argc > 1) assert(trace != NULL);
    unsigned clears = 0, audio_count = 0, phases_seen = 0, saw_straight = 0, saw_curve = 0; uint32_t previous_phase = GRAZER_PLAYING;
    for (uint32_t frame=0;frame<100000;frame++) {
        uint32_t flags = GRAZER_FIRE | (frame%200<100 ? GRAZER_FOCUS:0) |
            (frame%2400==100 ? GRAZER_BOMB:0) | (frame%(advanced ? 42000 : 12000)==(advanced ? 41999 : 11999) ? GRAZER_RESTART:0);
        assert(grazer_game_step(game,(GrazerGameInput){0,0,flags}) == GRAZER_OK);
        GrazerHud hud;
        assert(grazer_game_hud(game,&hud) == GRAZER_OK);
        assert(hud.phase != GRAZER_FAULTED && hud.phase != GRAZER_GAME_OVER);
        GrazerAdvancedHud extra = {0};
        if (advanced) {
            assert(grazer_game_advanced_hud(game,&extra) == GRAZER_OK && extra.difficulty == GRAZER_NORMAL && extra.power <= 4);
            if (extra.boss_phase > 0) phases_seen |= 1u << (extra.boss_phase-1);
            if (hud.phase == GRAZER_CLEARED) assert(extra.phases_started == 3 && extra.collected > 0 && extra.cancelled > 0);
        }
        if (hud.phase == GRAZER_CLEARED && previous_phase == GRAZER_PLAYING) clears++;
        previous_phase = hud.phase;
        GrazerAudioEvent events[128];
        assert(grazer_game_audio(game,events,128,&required) == GRAZER_OK);
        for (unsigned i=0;i<required;i++) assert(events[i].tick == hud.tick && events[i].sequence == i && events[i].resource_id>=1 && events[i].resource_id<=8);
        audio_count += required;
        if (frame%100==0) {
            uint32_t laser_count = 0;
            GrazerLaserSegment lasers[960];
            assert(grazer_game_lasers(game,lasers,960,&required) == GRAZER_OK);
            for (uint32_t i=0;i<required;i++) {
                assert(lasers[i].phase <= 2 && lasers[i].width > 0);
                if (lasers[i].segment == 0) laser_count++;
                if (extra.boss_phase == 2) saw_straight = 1;
                if (lasers[i].segment == 14) saw_curve = 1;
            }
            if (required > 0) {
                GrazerLaserSegment small, unchanged; memset(&small,0x5a,sizeof(small)); unchanged = small;
                assert(grazer_game_lasers(game,&small,0,&required) == GRAZER_BUFFER_TOO_SMALL);
                assert(memcmp(&small,&unchanged,sizeof(small)) == 0);
            }
            assert(grazer_game_snapshot(game,sprites,2048,&required) == GRAZER_OK);
            assert(required == hud.projectiles + hud.enemies + 1 + extra.drops - laser_count);
            if (required > 1) {
                GrazerGameSprite small = sentinel;
                assert(grazer_game_snapshot(game,&small,1,&required) == GRAZER_BUFFER_TOO_SMALL);
                assert(memcmp(&small,&sentinel,sizeof(small)) == 0);
            }
        }
        assert(grazer_game_state_hash(game,&hash) == GRAZER_OK);
        if (trace) {uint64_t expected;assert(fscanf(trace,"%" SCNx64,&expected)==1);assert(hash==expected);}
    }
    if (trace) {char extra[2];assert(fscanf(trace,"%1s",extra)==EOF);fclose(trace);}
    assert(clears >= (advanced ? 2u : 8u) && audio_count > 1000);
    if (advanced) assert(phases_seen == 7 && saw_straight && saw_curve);
    assert(grazer_game_restart(game) == GRAZER_OK);
    assert(grazer_game_state_hash(game,&hash) == GRAZER_OK && hash == initial);
    grazer_game_destroy(game); grazer_game_destroy(NULL);
    printf("PASS %s C host: 100000 hashes, %u clears, %u audio events, resource=%016" PRIx64 "\n",advanced ? "M4 advanced" : (scripted ? "M3 script":"M2 native"),clears,audio_count,info.content_hash);
    return 0;
}

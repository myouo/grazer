#include "grazer.h"
#include <assert.h>
#include <inttypes.h>
#include <stdio.h>
#include <string.h>

int main(int argc, char **argv) {
    assert(grazer_game_abi_version() == GRAZER_GAME_ABI_VERSION);
    GrazerGameConfig config = {GRAZER_GAME_ABI_VERSION, sizeof(config), 42, 512, 10000};
    GrazerGame *game = NULL;
    assert(grazer_game_create(NULL, &game) == GRAZER_INVALID_ARGUMENT && game == NULL);
    config.abi_version = 99;
    assert(grazer_game_create(&config, &game) == GRAZER_VERSION_MISMATCH && game == NULL);
    config.abi_version = GRAZER_GAME_ABI_VERSION;
    assert(grazer_game_create(&config, &game) == GRAZER_OK && game != NULL);
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
    GrazerGameSprite sprites[1024], sentinel;
    memset(&sentinel,0x5a,sizeof(sentinel));
    assert(grazer_game_snapshot(game,NULL,0,&required) == GRAZER_BUFFER_TOO_SMALL && required == 1);
    assert(grazer_game_snapshot(game,sprites,1024,&required) == GRAZER_OK);
    assert(sprites[0].resource_id == 1 && sprites[0].x == 240.0f && sprites[0].y == 560.0f);
    FILE *trace = argc > 1 ? fopen(argv[1],"r") : NULL;
    if (argc > 1) assert(trace != NULL);
    unsigned clears = 0, audio_count = 0; uint32_t previous_phase = GRAZER_PLAYING;
    for (uint32_t frame=0;frame<100000;frame++) {
        uint32_t flags = GRAZER_FIRE | (frame%200<100 ? GRAZER_FOCUS:0) |
            (frame%2400==100 ? GRAZER_BOMB:0) | (frame%12000==11999 ? GRAZER_RESTART:0);
        assert(grazer_game_step(game,(GrazerGameInput){0,0,flags}) == GRAZER_OK);
        GrazerHud hud;
        assert(grazer_game_hud(game,&hud) == GRAZER_OK);
        assert(hud.phase != GRAZER_FAULTED && hud.phase != GRAZER_GAME_OVER);
        if (hud.phase == GRAZER_CLEARED && previous_phase == GRAZER_PLAYING) clears++;
        previous_phase = hud.phase;
        GrazerAudioEvent events[128];
        assert(grazer_game_audio(game,events,128,&required) == GRAZER_OK);
        for (unsigned i=0;i<required;i++) assert(events[i].tick == hud.tick && events[i].sequence == i && events[i].resource_id>=1 && events[i].resource_id<=8);
        audio_count += required;
        if (frame%100==0) {
            assert(grazer_game_snapshot(game,sprites,1024,&required) == GRAZER_OK);
            assert(required == hud.projectiles + hud.enemies + 1);
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
    assert(clears >= 8 && audio_count > 1000);
    assert(grazer_game_restart(game) == GRAZER_OK);
    assert(grazer_game_state_hash(game,&hash) == GRAZER_OK && hash == initial);
    grazer_game_destroy(game); grazer_game_destroy(NULL);
    printf("PASS M2 C host: 100000 hashes, %u clears, %u audio events, resource=%016" PRIx64 "\n",clears,audio_count,info.content_hash);
    return 0;
}

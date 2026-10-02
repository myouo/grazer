#include "grazer.h"
#include <assert.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>

int main(void) {
    assert(grazer_abi_version() == GRAZER_ABI_VERSION);
    GrazerConfig config = {GRAZER_ABI_VERSION, sizeof(GrazerConfig), 640 * 65536, 480 * 65536, 2, 42};
    GrazerRuntime *runtime = NULL;
    assert(grazer_create(NULL, &runtime) == GRAZER_INVALID_ARGUMENT);
    assert(runtime == NULL);
    config.abi_version++;
    assert(grazer_create(&config, &runtime) == GRAZER_VERSION_MISMATCH);
    config.abi_version--;
    assert(grazer_create(&config, &runtime) == GRAZER_OK);
    GrazerBullet bullets[2] = {{0,0,65536,0,65536,0xffffffff}, {65536,0,-65536,0,65536,0xff0000ff}};
    assert(grazer_spawn(runtime, bullets, 2) == GRAZER_OK);
    uint64_t before, after;
    assert(grazer_state_hash(runtime, &before) == GRAZER_OK);
    assert(grazer_spawn(runtime, bullets, 1) == GRAZER_RUNTIME_ERROR);
    assert(grazer_set_input(runtime, (GrazerInput){2,0}) == GRAZER_INVALID_ARGUMENT);
    assert(grazer_state_hash(runtime, &after) == GRAZER_OK && before == after);
    assert(grazer_set_input(runtime, (GrazerInput){1,0}) == GRAZER_OK);
    for (int i=0; i<60; i++) assert(grazer_step(runtime) == GRAZER_OK);
    uint32_t required = 0;
    assert(grazer_snapshot(runtime, NULL, 0, &required) == GRAZER_BUFFER_TOO_SMALL && required == 3);
    GrazerSprite sentinel = {123,0,0,0,0};
    assert(grazer_snapshot(runtime, &sentinel, 1, &required) == GRAZER_BUFFER_TOO_SMALL && sentinel.id == 123);
    GrazerSprite *sprites = calloc(required, sizeof(*sprites));
    assert(sprites);
    assert(grazer_snapshot(runtime, sprites, required, &required) == GRAZER_OK);
    assert(sprites[0].id == 0 && sprites[0].x == 440.0f);
    assert(sprites[1].id == 1 && sprites[1].x == 60.0f);
    assert(sprites[2].id == 2 && sprites[2].x == 581.0f);
    assert(grazer_state_hash(runtime, &after) == GRAZER_OK);
    printf("PASS C ABI: %u sprites, hash=%016" PRIx64 "\n", required, after);
    free(sprites);
    grazer_destroy(runtime);
    grazer_destroy(NULL);
    assert(grazer_step(NULL) == GRAZER_INVALID_ARGUMENT);
    return 0;
}

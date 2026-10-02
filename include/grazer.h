#ifndef GRAZER_H
#define GRAZER_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif

#define GRAZER_ABI_VERSION 1u
typedef struct GrazerRuntime GrazerRuntime;
typedef struct {
    uint32_t abi_version, struct_size;
    int32_t width_q16, height_q16;
    uint32_t capacity;
    uint64_t seed;
} GrazerConfig;
typedef struct { int32_t x, y; } GrazerInput;
typedef struct {
    int32_t x_q16, y_q16, vx_q16, vy_q16, radius_q16;
    uint32_t rgba;
} GrazerBullet;
typedef struct { uint32_t id; float x, y, radius; uint32_t rgba; } GrazerSprite;
enum {
    GRAZER_OK = 0, GRAZER_INVALID_ARGUMENT = 1, GRAZER_VERSION_MISMATCH = 2,
    GRAZER_BUFFER_TOO_SMALL = 3, GRAZER_RUNTIME_ERROR = 4, GRAZER_PANIC = 5
};

/* ABI v1 uses the platform C ABI, default alignment, IEEE-754 32-bit float.
 * No packed structs. All calls on one runtime are serial. Non-null pointers
 * must be valid, aligned and sized for the operation; buffers must not alias
 * the runtime or each other. Arbitrary/dangling pointers cannot be validated.
 * Config is copied; spawn borrows input only for the call. The host owns all
 * output storage. No calls invoke host callbacks. After GRAZER_PANIC, destroy
 * the handle. Allocation failure may abort the process.
 * Positions and velocities are signed Q16.16; velocities are units/tick.
 * rgba is numeric 0xRRGGBBAA. IDs are stable; zero is the player.
 */
uint32_t grazer_abi_version(void);
/* Set abi_version and struct_size=sizeof(GrazerConfig). On failure *out=NULL. */
int32_t grazer_create(const GrazerConfig *config, GrazerRuntime **out);
/* NULL is accepted. Destroy a successful handle exactly once. */
void grazer_destroy(GrazerRuntime *runtime);
/* Persistent axes in {-1,0,1}; applies at the next tick. */
int32_t grazer_set_input(GrazerRuntime *runtime, GrazerInput input);
/* Batch is all-or-nothing. NULL bullets allowed only for count=0. */
int32_t grazer_spawn(GrazerRuntime *runtime, const GrazerBullet *bullets, uint32_t count);
int32_t grazer_step(GrazerRuntime *runtime);
/* Query with out=NULL/capacity=0: writes required, returns BUFFER_TOO_SMALL.
 * Insufficient capacity never partially writes sprites. */
int32_t grazer_snapshot(const GrazerRuntime *runtime, GrazerSprite *out, uint32_t capacity, uint32_t *required);
int32_t grazer_state_hash(const GrazerRuntime *runtime, uint64_t *out);

/* Additive M2 game ABI. Legacy functions above remain ABI v1.
 * Same ownership/alignment/serial-call rules. Game protocol is independent
 * from ABI and resource versions. Snapshots are presentation floats only.
 * Native Stage errors stop the game; restart to reset. Audio snapshots contain
 * events from the last successful step; hosts process them once after stepping.
 */
#define GRAZER_GAME_ABI_VERSION 2u
typedef struct GrazerGame GrazerGame;
typedef struct {
    uint32_t abi_version, struct_size;
    uint64_t seed;
    uint32_t projectile_capacity, player_health; /* zero selects defaults */
} GrazerGameConfig;
typedef struct { int32_t x, y; uint32_t flags; } GrazerGameInput;
enum { GRAZER_FIRE=1u, GRAZER_BOMB=2u, GRAZER_FOCUS=4u, GRAZER_RESTART=8u };
enum { GRAZER_PLAYING=0u, GRAZER_GAME_OVER=1u, GRAZER_CLEARED=2u, GRAZER_FAULTED=3u };
typedef struct {
    uint32_t kind, slot, generation, resource_id;
    float x, y, width, height;
    uint32_t rgba, layer;
} GrazerGameSprite;
typedef struct { uint32_t resource_id, sequence; uint64_t tick; } GrazerAudioEvent;
typedef struct {
    uint64_t tick, score, grazes;
    uint32_t health, bombs, phase, wave, boss_health, boss_max_health;
    uint32_t projectiles, enemies, bomb_flash;
} GrazerHud;
typedef struct {
    uint32_t version, width, height, atlas_bytes, sprites, sounds;
    uint64_t content_hash;
} GrazerResourceInfo;
typedef struct { uint32_t id, x, y, width, height; } GrazerSpriteAsset;
typedef struct { uint32_t id, waveform, frequency, duration_ms, gain_q8; } GrazerSoundAsset;
uint32_t grazer_game_abi_version(void);
int32_t grazer_game_create(const GrazerGameConfig *config, GrazerGame **out);
void grazer_game_destroy(GrazerGame *game);
/* Unknown flags/axes are rejected without changes. Restart uses a rising edge. */
int32_t grazer_game_step(GrazerGame *game, GrazerGameInput input);
int32_t grazer_game_restart(GrazerGame *game);
int32_t grazer_game_state_hash(const GrazerGame *game, uint64_t *out);
int32_t grazer_game_hud(const GrazerGame *game, GrazerHud *out);
/* Bulk queries: required is always written for a valid handle/buffer contract;
 * capacity < required returns BUFFER_TOO_SMALL without partial output writes.
 * Empty output queries return OK with required=0. Kind: player=0/enemy=1/shot=2.
 * Generations belong to the current run; restarting invalidates previous run
 * handles conceptually. Display layer order is ascending, player layer 30.
 */
int32_t grazer_game_snapshot(const GrazerGame *game, GrazerGameSprite *out, uint32_t capacity, uint32_t *required);
int32_t grazer_game_audio(const GrazerGame *game, GrazerAudioEvent *out, uint32_t capacity, uint32_t *required);
int32_t grazer_game_resource_info(const GrazerGame *game, GrazerResourceInfo *out);
int32_t grazer_game_atlas(const GrazerGame *game, uint8_t *out, uint32_t capacity, uint32_t *required);
int32_t grazer_game_sprite_assets(const GrazerGame *game, GrazerSpriteAsset *out, uint32_t capacity, uint32_t *required);
int32_t grazer_game_sound_assets(const GrazerGame *game, GrazerSoundAsset *out, uint32_t capacity, uint32_t *required);

/* Script API version 1 is additive to game ABI v2. Same handle/output structs.
 * Format 0 = UTF-8 source, null+zero selects shipped stage; 1 = checked bytecode.
 * Compiler/runtime faults retain source byte offsets and one-based line/column.
 * After a runtime script error Game is FAULTED and world ticks remain stopped.
 * Native game creation still uses the historical M2 Rust stage.
 */
#define GRAZER_SCRIPT_API_VERSION 1u
#define GRAZER_SCRIPT_ERROR 6
typedef struct {
    uint32_t kind, line, column, start, end, task_slot, task_generation, reserved;
} GrazerScriptDiagnostic;
uint32_t grazer_script_api_version(void);
int32_t grazer_game_create_script(const GrazerGameConfig *config, const uint8_t *source, uint32_t length, uint32_t format, GrazerGame **out, GrazerScriptDiagnostic *diagnostic);
int32_t grazer_game_diagnostic(const GrazerGame *game, GrazerScriptDiagnostic *out);
int32_t grazer_game_diagnostic_text(const GrazerGame *game, uint8_t *out, uint32_t capacity, uint32_t *required);
#ifdef __cplusplus
}
#endif
#endif

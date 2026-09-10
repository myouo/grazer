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
#ifdef __cplusplus
}
#endif
#endif

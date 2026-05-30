/*
 * RoboC++ generated-C target HAL validation example.
 *
 * Replace the generated include with the C emitted by:
 *
 *   rbcpp build-c examples/counter.st -o build/counter.c
 *
 * This wrapper shows the target-side validation points expected before a
 * deployment claims readiness on robot or embedded hardware.
 */

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

typedef struct {
    bool operator_enabled;
    bool estop_active;
    bool watchdog_expired;
    int64_t monotonic_ms;
} validation_hal_state;

static bool validation_io_read(void *ctx, const char *location, void *value, size_t size) {
    (void)ctx;
    if (strcmp(location, "%IX0.0") == 0 && size == sizeof(bool)) {
        bool input = true;
        memcpy(value, &input, sizeof(input));
        return true;
    }
    return false;
}

static bool validation_io_write(void *ctx, const char *location, const void *value, size_t size) {
    validation_hal_state *state = (validation_hal_state *)ctx;
    if (!state->operator_enabled || state->estop_active || state->watchdog_expired) {
        return false;
    }
    printf("validated output write %s size=%zu value=%d\n", location, size, *(const bool *)value);
    return true;
}

static int64_t validation_time_ms(void *ctx) {
    validation_hal_state *state = (validation_hal_state *)ctx;
    return state->monotonic_ms;
}

/*
 * Example integration flow:
 *
 * 1. Initialize generated program state.
 * 2. Install target hooks for I/O, retained state, timing, and watchdog events.
 * 3. Validate startup state before enabling outputs.
 * 4. Run scan cycles with operator enable false and verify outputs are blocked.
 * 5. Enable operator output and verify only mapped, expected outputs can change.
 * 6. Force E-stop, protective stop, stale I/O, and watchdog expiry cases.
 * 7. Record timing, retained-state, shutdown, and restart evidence.
 */

// The fleet log's C interface (crates/sheprd-fleet-ffi/src/abi.rs).
// SheprdFleetLog wraps it; nothing else calls it.
#ifndef SHEPRD_FLEET_H
#define SHEPRD_FLEET_H

#include <stddef.h>
#include <stdint.h>

// Answers one JSON request; writes the JSON answer's bytes and length.
// Returns 0 once written, -1 for a null pointer. Free the answer with
// sheprd_fleet_free.
int32_t sheprd_fleet_call(const uint8_t *request, size_t request_len,
                          uint8_t **response, size_t *response_len);

// Frees an answer sheprd_fleet_call wrote; null does nothing.
void sheprd_fleet_free(uint8_t *response, size_t len);

#endif

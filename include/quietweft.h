#ifndef QUIETWEFT_H
#define QUIETWEFT_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
typedef struct Quietweft Quietweft;
typedef struct {
 uint64_t processed_hops, processed_samples;
 double last_us, mean_us, peak_us, realtime_factor, algorithmic_latency_ms;
 int32_t last_error_code;
} QuietweftStats;
uint32_t qw_abi_version(void);
const char *qw_last_create_error(void);
Quietweft *qw_create(const char *utf8_model_directory);
void qw_destroy(Quietweft *);
int32_t qw_set_enabled(Quietweft *, int32_t enabled);
const char *qw_model_version(const Quietweft *);
const char *qw_last_error(const Quietweft *);
/* 128 finite mono float samples at 16000 Hz. One handle per stream.
   Input/output may alias. Caller owns all buffers. Serialize handle access.
   Returns 0 on success, negative on error. Output is unspecified on error.
   Returned strings are borrowed; copy before next call/destroy.
   Warm-up/algorithmic delay: 384 samples, including when bypassed. */
int32_t qw_process_hop(Quietweft *, const float input[128], float output[128]);
int32_t qw_get_stats(const Quietweft *, QuietweftStats *);
#ifdef __cplusplus
}
#endif
#endif

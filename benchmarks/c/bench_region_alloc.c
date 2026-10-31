/* Phase 13.3 - C baseline: malloc/free vs bump allocation throughput.
 * Mirrors benchmarks/bench_region_alloc.mw so `forge bench` can compare the
 * Morrow region allocator against an equivalent C program.
 *
 * Deliberately header-free: this repository ships only MinGW CRT objects in
 * lib/crt (no libc headers), so libc entry points are declared explicitly.
 *
 * Built by `forge bench` as:
 *   clang --target=x86_64-pc-windows-gnu -nostartfiles -O2 -o <exe> <src> \
 *         lib/crt/crt2.o -Llib/crt -lmingw32 -lmingwex -lmsvcrt -lkernel32 -lgcc
 */
typedef unsigned long long u64;

extern int printf(const char *, ...);
extern int fflush(void *);
extern void *malloc(u64);
extern void free(void *);
extern unsigned long clock(void);

#define ITERS 1000000
#define BLOCK_BYTES 64
#define CLOCKS_PER_SEC 1000

static long now_ms(void) {
    return (long)(((double)clock() * 1000.0) / (double)CLOCKS_PER_SEC);
}

/* Volatile sink so -O2 cannot elide the allocation / store loops. */
static volatile unsigned char sink;

static void report(const char *name, long iterations, long elapsed_ms) {
    long ops = 0;
    if (elapsed_ms > 0) {
        ops = (iterations * 1000) / elapsed_ms;
    }
    printf("BENCH %s iterations=%ld elapsed_ms=%ld ops_per_sec=%ld\n",
           name, iterations, elapsed_ms, ops);
    fflush((void *)0);
}

int main(void) {
    long i;

    printf("=== C Baseline: malloc/free vs bump allocator ===\n");

    /* Warm-up, discarded. */
    for (i = 0; i < ITERS / 10; i++) {
        void *volatile p = malloc(BLOCK_BYTES);
        ((unsigned char *)p)[0] = 1;
        sink = ((unsigned char *)p)[0];
        free((void *)p);
    }

    long t0 = now_ms();
    for (i = 0; i < ITERS; i++) {
        void *volatile p = malloc(BLOCK_BYTES);
        ((unsigned char *)p)[0] = 1;
        sink = ((unsigned char *)p)[0];
        free((void *)p);
    }
    long malloc_ms = now_ms() - t0;

    /* Bump allocation over one arena, released in O(1) at the end. */
    unsigned char *arena = (unsigned char *)malloc((u64)ITERS * BLOCK_BYTES);
    if (arena == (void *)0) {
        printf("[FAIL] C bump arena allocation failed\n");
        return 1;
    }

    long t1 = now_ms();
    for (i = 0; i < ITERS; i++) {
        arena[i * BLOCK_BYTES] = (unsigned char)1;
    }
    sink = arena[0];
    long bump_ms = now_ms() - t1;
    free(arena);

    report("c_malloc_free", ITERS, malloc_ms);
    report("c_bump_alloc", ITERS, bump_ms);
    return 0;
}

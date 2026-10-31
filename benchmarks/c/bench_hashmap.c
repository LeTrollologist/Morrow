/* Phase 13.3 - C baseline: open-addressing hash table insert/lookup density.
 * Mirrors benchmarks/bench_hashmap.mw: FNV-1a string keys, linear probing with
 * Robin-Hood displacement, 70% load-factor growth, 50k insert then 50k lookup.
 * Header-free by design (see bench_region_alloc.c for the build recipe).
 */
typedef unsigned long long u64;

extern int printf(const char *, ...);
extern int fflush(void *);
extern void *calloc(u64, u64);
extern void free(void *);
extern unsigned long clock(void);
extern int strcmp(const char *, const char *);

#define ITERS 200000
#define KEY_BYTES 16
#define CLOCKS_PER_SEC 1000

typedef struct {
    char *keys;
    long *vals;
    long *hashes;
    long *psls;
    unsigned char *occupied;
    long len;
    long cap;
} Table;

static long now_ms(void) {
    return (long)(((double)clock() * 1000.0) / (double)CLOCKS_PER_SEC);
}

static void copy_key(char *dst, const char *src) {
    long i;
    for (i = 0; i < KEY_BYTES; i++) {
        dst[i] = src[i];
        if (src[i] == '\0') {
            return;
        }
    }
    dst[KEY_BYTES - 1] = '\0';
}

/* Minimal unsigned decimal formatter (no snprintf without headers). */
static void format_key(char *buf, long value) {
    char tmp[20];
    long n = 0;
    long i;

    if (value == 0) {
        buf[0] = '0';
        buf[1] = '\0';
        return;
    }
    while (value > 0 && n < 19) {
        tmp[n] = (char)('0' + (value % 10));
        value = value / 10;
        n++;
    }
    for (i = 0; i < n; i++) {
        buf[i] = tmp[n - 1 - i];
    }
    buf[n] = '\0';
}

static long hash_str(const char *s) {
    u64 h = 1469598103934665603ULL;
    while (*s != '\0') {
        h = (h * 31ULL + (u64)(unsigned char)*s) * 1099511628211ULL;
        s++;
    }
    long out = (long)h;
    if (out < 0) {
        out = -out;
    }
    return out;
}

static int table_init(Table *t, long cap) {
    t->keys = (char *)calloc((u64)cap, KEY_BYTES);
    t->vals = (long *)calloc((u64)cap, sizeof(long));
    t->hashes = (long *)calloc((u64)cap, sizeof(long));
    t->psls = (long *)calloc((u64)cap, sizeof(long));
    t->occupied = (unsigned char *)calloc((u64)cap, 1);
    if (t->keys == (void *)0 || t->vals == (void *)0 ||
        t->hashes == (void *)0 || t->psls == (void *)0 ||
        t->occupied == (void *)0) {
        return 0;
    }
    t->len = 0;
    t->cap = cap;
    return 1;
}

static void table_free(Table *t) {
    free(t->keys);
    free(t->vals);
    free(t->hashes);
    free(t->psls);
    free(t->occupied);
}

static void table_put(Table *t, const char *key, long val);

static int table_grow(Table *t) {
    Table bigger;
    long i;

    if (!table_init(&bigger, t->cap * 2)) {
        return 0;
    }
    /* Re-insertion runs the full Robin-Hood algorithm so probe distances
     * (psls) stay consistent with the lookup early-exit invariant. */
    for (i = 0; i < t->cap; i++) {
        if (!t->occupied[i]) {
            continue;
        }
        table_put(&bigger, t->keys + i * KEY_BYTES, t->vals[i]);
    }
    table_free(t);
    *t = bigger;
    return 1;
}

static void table_put(Table *t, const char *key, long val) {
    long h;
    long idx;
    long curr_psl = 0;
    char scratch[KEY_BYTES];
    char key_copy[KEY_BYTES];

    if ((t->len + 1) * 10 >= t->cap * 7) {
        if (!table_grow(t)) {
            return;
        }
    }

    h = hash_str(key);
    idx = h % t->cap;

    for (;;) {
        if (!t->occupied[idx]) {
            copy_key(t->keys + idx * KEY_BYTES, key);
            t->vals[idx] = val;
            t->hashes[idx] = h;
            t->psls[idx] = curr_psl;
            t->occupied[idx] = 1;
            t->len++;
            return;
        }

        if (t->hashes[idx] == h &&
            strcmp(t->keys + idx * KEY_BYTES, key) == 0) {
            t->vals[idx] = val;
            return;
        }

        if (t->psls[idx] < curr_psl) {
            long disp_val = t->vals[idx];
            long disp_hash = t->hashes[idx];
            long disp_psl = t->psls[idx];

            copy_key(scratch, t->keys + idx * KEY_BYTES);
            copy_key(t->keys + idx * KEY_BYTES, key);
            t->vals[idx] = val;
            t->hashes[idx] = h;
            t->psls[idx] = curr_psl;

            copy_key(key_copy, scratch);
            key = key_copy;
            val = disp_val;
            h = disp_hash;
            curr_psl = disp_psl;
        }

        idx = (idx + 1) % t->cap;
        curr_psl++;
    }
}

static long table_get(const Table *t, const char *key) {
    long h = hash_str(key);
    long idx = h % t->cap;
    long curr_psl = 0;

    for (;;) {
        if (!t->occupied[idx] || t->psls[idx] < curr_psl) {
            return -1;
        }
        if (t->hashes[idx] == h &&
            strcmp(t->keys + idx * KEY_BYTES, key) == 0) {
            return t->vals[idx];
        }
        idx = (idx + 1) % t->cap;
        curr_psl++;
    }
}

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
    char *pool;
    long i;
    long matched = 0;
    Table table;

    printf("=== C Baseline: open-addressing hash table ===\n");

    pool = (char *)calloc((u64)ITERS, KEY_BYTES);
    if (pool == (void *)0) {
        printf("[FAIL] C key pool allocation failed\n");
        return 1;
    }
    /* Keys are pre-formatted: this baseline measures steady-state table cost. */
    for (i = 0; i < ITERS; i++) {
        format_key(pool + i * KEY_BYTES, i);
    }

    if (!table_init(&table, 256)) {
        printf("[FAIL] C hash table allocation failed\n");
        free(pool);
        return 1;
    }

    /* Warm-up, discarded, then rebuild a fresh table for the measurement. */
    for (i = 0; i < ITERS / 10; i++) {
        table_put(&table, pool + i * KEY_BYTES, i);
        (void)table_get(&table, pool + i * KEY_BYTES);
    }
    table_free(&table);
    if (!table_init(&table, 256)) {
        printf("[FAIL] C hash table re-init failed\n");
        free(pool);
        return 1;
    }

    long t0 = now_ms();
    for (i = 0; i < ITERS; i++) {
        table_put(&table, pool + i * KEY_BYTES, i);
    }
    long insert_ms = now_ms() - t0;

    long t1 = now_ms();
    for (i = 0; i < ITERS; i++) {
        if (table_get(&table, pool + i * KEY_BYTES) == i) {
            matched++;
        }
    }
    long lookup_ms = now_ms() - t1;

    report("c_hashmap_insert", ITERS, insert_ms);
    report("c_hashmap_lookup", ITERS, lookup_ms);

    long final_len = table.len;
    table_free(&table);
    free(pool);

    if (final_len != ITERS || matched != ITERS) {
        printf("[FAIL] C hash table verification failed (len=%ld matched=%ld)\n",
               final_len, matched);
        return 1;
    }
    return 0;
}

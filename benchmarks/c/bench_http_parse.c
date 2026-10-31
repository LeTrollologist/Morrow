/* Phase 13.3 - C baseline: HTTP/1.1 request parsing throughput.
 * Mirrors benchmarks/bench_http_parse.mw: locate the method, path and body of
 * the same well-formed request and copy each component, so the C baseline does
 * comparable work to Morrow's arena-backed zero-copy parser.
 *
 * Header-free by design (see bench_region_alloc.c for the build recipe).
 */
typedef unsigned long long u64;

extern int printf(const char *, ...);
extern int fflush(void *);
extern u64 strlen(const char *);
extern char *strstr(const char *, const char *);
extern int strcmp(const char *, const char *);
extern unsigned long clock(void);

#define ITERS 200000
#define CLOCKS_PER_SEC 1000

static const char *REQUEST =
    "GET /api/v1/health HTTP/1.1\r\n"
    "Host: api.morrow.dev\r\n"
    "Accept: application/json\r\n"
    "X-Request-Id: bench-42\r\n"
    "User-Agent: MorrowBench/1.0\r\n"
    "\r\n";

static char method[16];
static char path[256];
static char body[256];

static long now_ms(void) {
    return (long)(((double)clock() * 1000.0) / (double)CLOCKS_PER_SEC);
}

static void copy_range(char *dst, const char *src, long len) {
    long i;
    for (i = 0; i < len; i++) {
        dst[i] = src[i];
    }
    dst[len] = '\0';
}

/* Returns 1 when the request parsed successfully. */
static int parse_request(const char *raw) {
    const char *sp1 = (const char *)0;
    const char *sp2 = (const char *)0;
    const char *sep;
    long i;

    for (i = 0; raw[i] != '\0'; i++) {
        if (raw[i] == ' ') {
            if (sp1 == (const char *)0) {
                sp1 = raw + i;
            } else if (sp2 == (const char *)0) {
                sp2 = raw + i;
            }
        }
    }
    if (sp1 == (const char *)0 || sp2 == (const char *)0) {
        return 0;
    }

    sep = strstr(raw, "\r\n\r\n");
    if (sep == (const char *)0) {
        return 0;
    }

    long m_len = (long)(sp1 - raw);
    long p_len = (long)(sp2 - sp1 - 1);
    const char *body_start = sep + 4;
    long b_len = (long)strlen(body_start);

    if (m_len >= (long)sizeof(method) || p_len >= (long)sizeof(path) ||
        b_len >= (long)sizeof(body)) {
        return 0;
    }

    copy_range(method, raw, m_len);
    copy_range(path, sp1 + 1, p_len);
    copy_range(body, body_start, b_len);
    return 1;
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
    long parsed = 0;
    long i;

    printf("=== C Baseline: HTTP/1.1 request parser ===\n");

    for (i = 0; i < ITERS / 10; i++) {
        (void)parse_request(REQUEST);
    }

    long t0 = now_ms();
    for (i = 0; i < ITERS; i++) {
        if (parse_request(REQUEST)) {
            parsed++;
        }
    }
    long elapsed = now_ms() - t0;

    if (parsed != ITERS) {
        printf("[FAIL] C baseline parser rejected a well-formed request\n");
        return 1;
    }

    report("c_http_parse", ITERS, elapsed);
    return 0;
}

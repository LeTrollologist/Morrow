// ==========================================================
// simple.h — Example C Header for Tungsten FFI & forge bindgen
// ==========================================================

#define SQLITE_OK 0
#define SQLITE_ERROR 1
#define SQLITE_VERSION "3.45.1"

struct SQLiteHandle {
    int fd;
    int state;
};

// C Library declarations
size_t strlen(const char *s);
int puts(const char *s);
void *malloc(size_t size);
void free(void *ptr);
int sqlite3_open(const char *filename, struct SQLiteHandle *db);
int sqlite3_close(struct SQLiteHandle *db);

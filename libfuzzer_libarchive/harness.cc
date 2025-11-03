// libarchive_fuzzer.cc
// Fuzzing harness for libarchive

#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <stdlib.h>

#include <archive.h>
#include <archive_entry.h>

// Memory buffer structure - similar to libpng's BufState
struct MemoryBuffer {
    const uint8_t *data;
    size_t size;
    size_t offset;
};

// Callback functions for libarchive - similar to libpng's user_read_data
la_ssize_t memory_read_callback(struct archive *a, void *client_data, const void **buffer) {
    struct MemoryBuffer *mem_buf = (struct MemoryBuffer *)client_data;

    if (mem_buf->offset >= mem_buf->size) {
        return 0; // EOF
    }

    *buffer = mem_buf->data + mem_buf->offset;
    la_ssize_t bytes_to_read = mem_buf->size - mem_buf->offset;

    // Limit read size to avoid excessive memory usage during fuzzing
    if (bytes_to_read > 65536) {
        bytes_to_read = 65536;
    }

    mem_buf->offset += bytes_to_read;
    return bytes_to_read;
}

int memory_close_callback(struct archive *a, void *client_data) {
    return ARCHIVE_OK;
}

// Entry point for LibFuzzer
extern "C" int LLVMFuzzerTestOneInput(const uint8_t *data, size_t size) {
    if (size < 4) {
        return 0; // Too small to be meaningful
    }

    // Limit input size to prevent excessive resource consumption
    if (size > 1024 * 1024) {  // 1MB limit
        return 0;
    }

    struct archive *a;
    struct archive_entry *entry;
    int r;

    // Create a new archive reader
    a = archive_read_new();
    if (!a) {
        return 0;
    }

    // Enable all supported formats and filters
    archive_read_support_filter_all(a);
    archive_read_support_format_all(a);

    // Set up memory buffer for reading
    struct MemoryBuffer mem_buf;
    mem_buf.data = data;
    mem_buf.size = size;
    mem_buf.offset = 0;

    // Set up callback for reading from memory
    r = archive_read_set_callback_data(a, &mem_buf);
    if (r != ARCHIVE_OK) {
        archive_read_free(a);
        return 0;
    }

    r = archive_read_set_read_callback(a, memory_read_callback);
    if (r != ARCHIVE_OK) {
        archive_read_free(a);
        return 0;
    }

    r = archive_read_set_close_callback(a, memory_close_callback);
    if (r != ARCHIVE_OK) {
        archive_read_free(a);
        return 0;
    }

    // Open the archive
    r = archive_read_open1(a);
    if (r != ARCHIVE_OK) {
        archive_read_free(a);
        return 0;
    }

    // Process archive entries
    int entries_processed = 0;
    const int max_entries = 100; // Limit number of entries to process

    while (entries_processed < max_entries) {
        r = archive_read_next_header(a, &entry);
        if (r == ARCHIVE_EOF) {
            break; // End of archive
        }
        if (r != ARCHIVE_OK) {
            break; // Error reading header
        }

        entries_processed++;

        // Get entry information (this exercises more code paths)
        const char *pathname = archive_entry_pathname(entry);
        if (pathname) {
            // Just accessing the pathname exercises string handling
            size_t path_len = strlen(pathname);
            (void)path_len; // Suppress unused variable warning
        }

        la_int64_t entry_size = archive_entry_size(entry);
        mode_t mode = archive_entry_mode(entry);
        time_t mtime = archive_entry_mtime(entry);

        // Suppress unused variable warnings
        (void)entry_size;
        (void)mode;
        (void)mtime;

        // Try to read some data from the entry (limited to avoid excessive processing)
        if (archive_entry_size_is_set(entry) && archive_entry_size(entry) > 0) {
            const size_t max_read_size = 8192; // Limit data read per entry
            size_t total_read = 0;

            while (total_read < max_read_size) {
                const void *buff;
                size_t buff_size;
                la_int64_t offset;

                r = archive_read_data_block(a, &buff, &buff_size, &offset);
                if (r == ARCHIVE_EOF) {
                    break;
                }
                if (r != ARCHIVE_OK) {
                    break;
                }

                total_read += buff_size;
                if (total_read >= max_read_size) {
                    break;
                }
            }
        }

        // Skip remaining data in this entry
        archive_read_data_skip(a);
    }

    // Clean up
    archive_read_close(a);
    archive_read_free(a);

    return 0;
}
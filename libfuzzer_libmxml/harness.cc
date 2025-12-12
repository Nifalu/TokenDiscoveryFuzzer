#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include "mxml.h"

void error_callback(void *cbdata, const char *message) {
    (void)cbdata;
    (void)message;
}

extern "C" int LLVMFuzzerTestOneInput(const uint8_t *data, size_t size) {
    if (size < 4 || size > 1024 * 1024) {
        return 0;
    }

    char *xml_str = (char *)malloc(size + 1);
    if (!xml_str) return 0;
    memcpy(xml_str, data, size);
    xml_str[size] = '\0';

    mxml_options_t *options = mxmlOptionsNew();
    if (!options) {
        free(xml_str);
        return 0;
    }
    mxmlOptionsSetErrorCallback(options, error_callback, NULL);

    mxml_node_t *tree = mxmlLoadString(NULL, options, xml_str);

    if (tree) {
        mxml_node_t *node = mxmlGetFirstChild(tree);
        while (node) {
            mxml_type_t type = mxmlGetType(node);

            switch (type) {
                case MXML_TYPE_ELEMENT: {
                    const char *name = mxmlGetElement(node);
                    (void)name;
                    int count = mxmlElementGetAttrCount(node);
                    for (int i = 0; i < count && i < 100; i++) {
                        const char *attr_name;
                        const char *attr_value = mxmlElementGetAttrByIndex(node, i, &attr_name);
                        (void)attr_value;
                    }
                    break;
                }
                case MXML_TYPE_TEXT: {
                    bool whitespace;
                    const char *text = mxmlGetText(node, &whitespace);
                    (void)text;
                    break;
                }
                case MXML_TYPE_INTEGER: {
                    long val = mxmlGetInteger(node);
                    (void)val;
                    break;
                }
                case MXML_TYPE_REAL: {
                    double val = mxmlGetReal(node);
                    (void)val;
                    break;
                }
                case MXML_TYPE_OPAQUE: {
                    const char *opaque = mxmlGetOpaque(node);
                    (void)opaque;
                    break;
                }
                default:
                    break;
            }

            node = mxmlWalkNext(node, tree, MXML_DESCEND_ALL);
        }

        mxmlFindElement(tree, tree, NULL, NULL, NULL, MXML_DESCEND_ALL);
        mxmlFindPath(tree, "*");

        mxmlDelete(tree);
    }

    mxmlOptionsDelete(options);
    free(xml_str);
    return 0;
}

#ifdef STANDALONE_BUILD
#include <stdio.h>
#include <stdlib.h>

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <input_file>\n", argv[0]);
        return 1;
    }

    FILE *f = fopen(argv[1], "rb");
    if (!f) {
        perror("Failed to open input file");
        return 1;
    }

    fseek(f, 0, SEEK_END);
    size_t size = ftell(f);
    fseek(f, 0, SEEK_SET);

    uint8_t *data = (uint8_t *)malloc(size);
    if (!data) {
        fclose(f);
        return 1;
    }

    fread(data, 1, size, f);
    fclose(f);

    printf("Testing with input of size %zu bytes\n", size);
    int result = LLVMFuzzerTestOneInput(data, size);
    printf("Test completed with result: %d\n", result);

    free(data);
    return 0;
}
#endif
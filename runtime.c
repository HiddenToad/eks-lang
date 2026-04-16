#include <stdio.h>

void __eks_print_int(long long val) {
    printf("%lld\n", val);
}

void __eks_print_float(double val) {
    printf("%g\n", val);
}

void __eks_print_string(const char* val) {
    if (val != 0) {
        printf("%s\n", val);
    }
}
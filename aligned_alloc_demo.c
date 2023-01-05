#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

int main() {
  static const size_t FOUR_GIB_IN_BYTES = UINT64_C(4) << 30;
  static const size_t HUGE_2MIB_ALIGNMENT = UINT64_C(2) << 20;
  static const size_t HUGE_2MIB_MASK = HUGE_2MIB_ALIGNMENT - 1;
  static const size_t HUGE_1GIB_ALIGNMENT = UINT64_C(1) << 30;
  static const size_t HUGE_1GIB_MASK = HUGE_1GIB_ALIGNMENT - 1;

  void *plain_malloc = malloc(FOUR_GIB_IN_BYTES);
  printf("malloc 4GiB = %p; 2MiB aligned? %d; 1GiB aligned? %d\n", plain_malloc,
         ((size_t)plain_malloc & HUGE_2MIB_MASK) == 0,
         ((size_t)plain_malloc & HUGE_1GIB_MASK) == 0);
  free(plain_malloc);

  void *aligned = aligned_alloc(HUGE_1GIB_ALIGNMENT, FOUR_GIB_IN_BYTES);
  printf("aligned_alloc 4GiB = %p; 2MiB aligned? %d; 1GiB aligned? %d\n", aligned,
         ((size_t)aligned & HUGE_2MIB_MASK) == 0, ((size_t)aligned & HUGE_1GIB_MASK) == 0);
  free(aligned);

  return 0;
}

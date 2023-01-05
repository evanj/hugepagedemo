# Huge Page Demo

This is a demonstration of using huge pages on Linux to get better performance.

This will compile and run on non-Linux platforms, but won't use huge pages.

For more details, see [Reliably allocating huge pages in Linux](https://mazzo.li/posts/check-huge-page.html).


# Malloc/Mmap behaviour

On Ubuntu 20.04.5 with kernel 5.15.0-1023-aws and glibc 2.31-0ubuntu9.9, malloc 4 GiB calls mmap to allocate 4 GiB + 4 KiB, then returns a pointer that is +0x10 (+16) from the pointer actually returned by mmap. Using aligned_alloc calls mmap to allocate 5 GiB + 4 KiB (size + alignment + 1 page?), then returns an aligned pointer. Calling mmap to allocate 4 GiB returns a pointer that is not aligned. E.g. On my system, I get one that is 32 kiB aligned.

On Mac OS X 13.1 on an M1 ARM CPU, using mmap to request 4 GiB of memory returns a block that is aligned to a 1 GiB boundary. The same appears to be true for using malloc. I didn't fight to get dtruss to work to see what malloc is actually doing.
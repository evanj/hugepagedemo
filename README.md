# Huge Page Demo

This is a demonstration of using huge pages on Linux to get better performance. It allocates a 4 GiB chunk both using a Vec (which will use "regular" malloc), then using mmap to get a 1 GiB-aligned region. It then uses madvise() to mark it for huge pages, then will touch the entire region to fault it in to memory. Finally, it does a random-access benchmark. This is probably the "best case" scenario for huge pages.

On an "Intel(R) Xeon(R) Platinum 8259CL CPU @ 2.50GHz" (AWS m5d.4xlarge), the huge page version in about twice as fast.

This will compile and run on non-Linux platforms, but won't use huge pages.

For more details, see [Reliably allocating huge pages in Linux](https://mazzo.li/posts/check-huge-page.html).

Unfortunately, there appears to be no way to get the *size* of a huge page that was allocated. It is possible to check that a specific address is a huge page, which this program will do if run as root. It is also possible to get the count of the amount of memory allocated for a specific range as huge pages, by examining the `AnonHugePages` line in `/proc/self/smaps`.


## Malloc/Mmap behaviour

On Ubuntu 20.04.5 with kernel 5.15.0-1023-aws and glibc 2.31-0ubuntu9.9, malloc 4 GiB calls mmap to allocate 4 GiB + 4 KiB, then returns a pointer that is +0x10 (+16) from the pointer actually returned by mmap. Using aligned_alloc calls mmap to allocate 5 GiB + 4 KiB (size + alignment + 1 page?), then returns an aligned pointer. Calling mmap to allocate 4 GiB returns a pointer that is not aligned. E.g. On my system, I get one that is 32 kiB aligned.

On Mac OS X 13.1 on an M1 ARM CPU, using mmap to request 4 GiB of memory returns a block that is aligned to a 1 GiB boundary. The same appears to be true for using malloc. I didn't fight to get dtruss to work to see what malloc is actually doing.


## Random huge page facts

X86-64 supports 2MiB and 1GiB huge pages.

Newer Arm CPUs support a huge range of huge pages: https://github.com/lgeek/arm_tlb_huge_pages

Google's TCMalloc/Temeraire is a huge page aware allocator. They found it improved request per second performance of user code by about 7% fleet-wide. https://www.usenix.org/conference/osdi21/presentation/hunter

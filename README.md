# Huge Page Demo

This is a demonstration of using huge pages on Linux to get better performance. It allocates a 4 GiB chunk using a Vec (which calls libc's malloc), then using mmap to get a 2 MiB-aligned region. It then uses `madvise(..., MADV_HUGEPAGE)` to mark the region for huge pages, then will touch the entire region to fault it in to memory. Finally, it does a random-access benchmark. This is probably the "best case" scenario for huge pages. I also test allocating an explicit huge page region with `mmap(..., MAP_HUGETLB | MAP_HUGE_1GB)`.

On a "11th Gen Intel(R) Core(TM) i5-1135G7 @ 2.40GHz", the transparent 2MiB huge page version is about 2.9X faster, and the 1GiB huge page version is 3.1X faster (8% more than 2MiB pages). On an older "Intel(R) Xeon(R) Platinum 8259CL CPU @ 2.50GHz" (AWS m5d.4xlarge), the transparent 2MiB huge page version is about 2X faster, and I did not test the GiB huge pages. This seems to suggest that programs that make random accesses to large amounts of memory will benefit from huge pages. The benefit from the gigabyte huge pages is minimal, so probably not worth the pain of having to manually configure them.

As of 2022-01-10, the Linux kernel only supports a single size of transparent huge pages. The size will be reported as `Hugepagesize` in `/proc/meminfo`. On x86_64, this will be 2 MiB. For Arm (aarch64), most recent Linux distributions also defalut to 4 kiB/2 MiB pages. Redhat used to use 64 kiB pages, but [RHEL 9 changed it to 4 kiB around 2021-07](https://bugzilla.redhat.com/show_bug.cgi?id=1978730).

When running as root, it is possible to check if a specific address is a huge page. It is also possible to get the amount of memory allocated for a specific range as huge pages, by examining the `AnonHugePages` line in `/proc/self/smaps`. The `thp_` statistics in `/proc/vmstat` also can tell you if this worked by checking `thp_fault_alloc` and `thp_fault_fallback` before and after the allocation. Sometimes the kernel will not be able to find huge pages. This program only tests the first page, so it won't be able to tell if the huge page allocation fails. See [the Monitoring usage section in the kernel's transhuge.txt for details](https://www.kernel.org/doc/Documentation/vm/transhuge.txt).


### Testing GiB huge pages

To allocate 1 GiB pages, you must run:

```
echo 4 | sudo tee /sys/kernel/mm/hugepages/hugepages-1048576kB/nr_hugepages
```

On my machine after running for a while, this will "succeed", but checking with cat shows the number does not change, and calling mmap will fail with `ENOMEM`. I needed to test this shortly after boot to get it to work.

This demo compiles and runs on Mac OS X, but won't use huge pages.


## Results

From a system where `/proc/cpuinfo` reports "11th Gen Intel(R) Core(TM) i5-1135G7 @ 2.40GHz", using `perf stat -e dTLB-load-misses,iTLB-load-misses,page-faults,dtlb_load_misses.walk_completed,dtlb_load_misses.stlb_hit`:

### Vec

```
200000000 accessses in 6.421793881s; 31143945.7 accesses/sec

       199,687,753      dTLB-load-misses
             4,432      iTLB-load-misses
         1,048,699      page-faults
       199,687,753      dtlb_load_misses.walk_completed
         5,801,701      dtlb_load_misses.stlb_hit
```

### Transparent 2MiB Huge Page mmap

```
200000000 in 2.193096392s; 91195262.0 accesses/sec

       112,933,198      dTLB-load-misses
             2,431      iTLB-load-misses
             2,197      page-faults
       112,933,198      dtlb_load_misses.walk_completed
        84,037,596      dtlb_load_misses.stlb_hit
```

### 1GiB Huge Page mmap HUGE_TLB

```
200000000 accesses in 2.01655466s; 99179062.2 accesses/sec

               908      dTLB-load-misses
               647      iTLB-load-misses
               127      page-faults
               908      dtlb_load_misses.walk_completed
             9,781      dtlb_load_misses.stlb_hit
```


## Malloc/Mmap behaviour notes

On Ubuntu 20.04.5 with kernel 5.15.0-1023-aws and glibc 2.31-0ubuntu9.9, `malloc(4 GiB)` calls `mmap` to allocate 4 GiB + 4 KiB, then returns a pointer that is +0x10 (+16) from the pointer actually returned by `mmap`. Using `aligned_alloc` to allocate 4 GiB with a 1 GiB alignment calls `mmap` to allocate 5 GiB + 4 KiB (size + alignment + 1 page?), then returns an aligned pointer. Calling mmap to allocate 4 GiB returns a pointer that is usually not aligned. E.g. On my system, I get one that is 32 kiB aligned. Calling mmap repeatedly seems to allocate addresses downward. [This tweet](https://twitter.com/pkhuong/status/1462988088070791173) also suggests that `mmap(MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE | MAP_HUGETLB)` will return an aligned address, although the mmap man page does not make it clear if that behavior is guaranteed or not.

On Mac OS X 13.1 on an M1 ARM CPU, using mmap to request 4 GiB of memory returns a block that is aligned to a 1 GiB boundary. The same appears to be true for using malloc. I didn't fight to get dtruss to work to see what malloc is actually doing.


## Random huge page facts

X86-64 supports 2MiB and 1GiB huge pages.

Newer Arm CPUs support a huge range of huge pages: https://github.com/lgeek/arm_tlb_huge_pages

Google's TCMalloc/Temeraire is a huge page aware allocator. They found it improved request per second performance of user code by about 7% fleet-wide. https://www.usenix.org/conference/osdi21/presentation/hunter

For a C version, see [Reliably allocating huge pages in Linux](https://mazzo.li/posts/check-huge-page.html), which I used to develop this version.

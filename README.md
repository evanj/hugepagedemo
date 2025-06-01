# Huge Page Demo

This is a demonstration of using huge pages on Linux to get better performance. It allocates a 4 GiB chunk both using a Rust [`Vec`](https://doc.rust-lang.org/std/vec/struct.Vec.html) (which allocates memory with `malloc`), then using `mmap` to get a 2 MiB-aligned region. It then uses [`madvise(..., MADV_HUGEPAGE)`](https://man7.org/linux/man-pages/man2/madvise.2.html) to mark it for huge pages, then will touch the entire region to fault it in to memory. Finally, it does a random-access benchmark. This is probably the "best case" scenario for huge pages. It also tests 1 GiB huge pages using `mmap(..., MAP_HUGETLB | MAP_HUGE_1GB)`, but that will require explicit configuration. See [my blog post for more details](https://www.evanjones.ca/hugepages-are-a-good-idea.html).

On a "11th Gen Intel(R) Core(TM) i5-1135G7 @ 2.40GHz" (TigerLake from 2020), the transparent 2 MiB huge page version is about 2.9× faster, and the 1 GiB huge page version is 3.1× faster (8% faster than 2MiB pages). On an older "Intel(R) Xeon(R) Platinum 8259CL CPU @ 2.50GHz" (AWS m5d.4xlarge), the transparent 2 MiB huge page version is about 2× faster, and I did not test the GiB huge pages. This seems to suggest that programs that make random accesses to large amounts of memory will benefit from huge pages. The benefit from the gigabyte huge pages is minimal, so probably not worth the pain of having to manually configure them.

As of 2022-01-10, the Linux kernel only supports a single size of transparent huge pages. The size will be reported as `Hugepagesize` in `/proc/meminfo`. On x86_64, this will be 2 MiB. For Arm (aarch64), most recent Linux distributions also defalut to 4 kiB/2 MiB pages. Redhat used to use 64 kiB pages, but [RHEL 9 changed it to 4 kiB around 2021-07](https://bugzilla.redhat.com/show_bug.cgi?id=1978730).

When running as root, it is possible to check if a specific address is a huge page. It is also possible to get the amount of memory allocated for a specific range as huge pages, by examining the `AnonHugePages` line in `/proc/self/smaps`. The `thp_` statistics in `/proc/vmstat` also can tell you if this worked by checking `thp_fault_alloc` and `thp_fault_fallback` before and after the allocation. Sometimes the kernel will not be able to find huge pages. This program only tests the first page, so it won't be able to tell if the huge page allocation fails. See [the Monitoring usage section in the kernel's transhuge.txt for details](https://www.kernel.org/doc/Documentation/vm/transhuge.txt).

TODO: It would be nice to check for page allocation latency. It seems likely that [fragmenting huge pages then allocating huge pages should have higher latencies](https://nitingupta.dev/post/linux-kernel-hugepage-allocation-latencies/). The `faultlatency` program in this repository is intended to test this, but I didn't (yet) implement the part that fragments memory. On my test machine, it prints the following times to allocate then touch 4 kiB and 2 MiB pages. This suggests it takes a bit longer to make two syscalls for mmap+madvise, then about 28× longer to fault the page initally. This is less bad than I was expecting, since the page is 512× larger.

```
4kiB: mmap:16.665µs fault:15.193µs second_write:124ns;   2MiB: mmap:20.884µs fault:428.13µs second_write:122ns
```

### CPU support

* x86-64 supports 2 MiB and 1 GiB huge pages, according to `ls /sys/kernel/mm/hugepages`. Transparent pages are configured as 2 MiB according to `cat /sys/kernel/mm/transparent_hugepage/hpage_pmd_size`.
* ARM Neoverse V2 (e.g. AWS Graviton4, GCP Axion) supports 64 kiB, 2 MiB, 32 MiB, and 1 GiB huge pages according to `ls /sys/kernel/mm/hugepages`. Transparent pages are configured as 2 MiB.


### Mac OS X Super Pages

This demo compiles and runs on Mac OS X, but won't use huge pages. It would be nice to add support for Mac OS X's `VM_FLAGS_SUPERPAGE_SIZE_2MB` and test it, but there is no official documentation of this flag. It used to exist in `man mmap` but not longer does. The [old text seemed to be](https://www.unix.com/man-page/osx/2/mmap):

> `VM_FLAGS_SUPERPAGE_SIZE_*` to use superpages for the allocation.  See `<mach/vm_statistics.h>` for supported architectures
and sizes (or use `VM_FLAGS_SUPERPAGE_SIZE_ANY` to have the kernel choose a size).  The specified size must be divisible by
the superpage size (except for `VM_FLAGS_SUPERPAGE_SIZE_ANY`), and if you use `MAP_FIXED`, the specified address must be prop-
erly aligned. If the system cannot satisfy the request with superpages, the call will fail. Note that currently, superpages
are always wired and not inherited by children of the process.



### Testing GiB huge pages on Linux

To allocate 4×1 GiB huge pages, you must run:

```
echo 4 | sudo tee /sys/kernel/mm/hugepages/hugepages-1048576kB/nr_hugepages
```

On my machine after running for a while, this will "succeed", but checking the current value with `cat` shows the number does not change, and calling mmap will fail with `ENOMEM`. I believe this means  I needed to test this shortly after boot to get it to work.


# Results

From a system where `/proc/cpuinfo` reports "11th Gen Intel(R) Core(TM) i5-1135G7 @ 2.40GHz", using `perf stat -e dTLB-load-misses,iTLB-load-misses,page-faults,dtlb_load_misses.walk_completed,dtlb_load_misses.stlb_hit`:

## Vec

```
200000000 accessses in 6.421793881s; 31143945.7 accesses/sec

       199,687,753      dTLB-load-misses
             4,432      iTLB-load-misses
         1,048,699      page-faults
       199,687,753      dtlb_load_misses.walk_completed
         5,801,701      dtlb_load_misses.stlb_hit
```

## Transparent 2MiB Huge Page mmap

```
200000000 in 2.193096392s; 91195262.0 accesses/sec

       112,933,198      dTLB-load-misses
             2,431      iTLB-load-misses
             2,197      page-faults
       112,933,198      dtlb_load_misses.walk_completed
        84,037,596      dtlb_load_misses.stlb_hit
```

## 1GiB Huge Page mmap HUGE_TLB

```
200000000 accesses in 2.01655466s; 99179062.2 accesses/sec

               908      dTLB-load-misses
               647      iTLB-load-misses
               127      page-faults
               908      dtlb_load_misses.walk_completed
             9,781      dtlb_load_misses.stlb_hit
```


# Malloc/Mmap behaviour notes

On Ubuntu 20.04.5 with kernel 5.15.0-1023-aws and glibc 2.31-0ubuntu9.9, `malloc(4 GiB)` calls `mmap` to allocate 4 GiB + 4 KiB, then returns a pointer that is +0x10 (+16) from the pointer actually returned by `mmap`. Using `aligned_alloc` to allocate 4 GiB with a 1 GiB alignment calls `mmap` to allocate 5 GiB + 4 KiB (size + alignment + 1 page?), then returns an aligned pointer. Calling mmap to allocate 4 GiB returns a pointer that is usually not aligned. E.g. On my system, I get one that is 32 kiB aligned. Calling mmap repeatedly seems to allocate addresses downward. [This tweet](https://twitter.com/pkhuong/status/1462988088070791173) also suggests that `mmap(MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE | MAP_HUGETLB)` will return an aligned address, although the mmap man page does not make it clear if that behavior is guaranteed or not.

On Mac OS X 13.1 on an M1 ARM CPU, using mmap to request 4 GiB of memory returns a block that is aligned to a 1 GiB boundary. The same appears to be true for using malloc. I didn't fight to get dtruss to work to see what malloc is actually doing.


# Random huge page facts

* Newer Arm CPUs support a huge range of huge pages: https://github.com/lgeek/arm_tlb_huge_pages
* Google's TCMalloc/Temeraire is a huge page aware allocator. They found it improved request per second performance of user code by about 7% fleet-wide. https://www.usenix.org/conference/osdi21/presentation/hunter
* For a C version, see [Reliably allocating huge pages in Linux](https://mazzo.li/posts/check-huge-page.html), which I used to develop this version.
* Intel created [an example of using LD_PRELOAD to map instructions as huge pages](https://github.com/intel/iodlr/tree/master/large_page-c).
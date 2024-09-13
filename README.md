# Running an application with an MFS

## Step 1: Building the FBMM enabled kernel
If you want to manually build and install the kernel yourself, follow these steps
1. Clone the [FBMM kernel repo](https://github.com/multifacet/fbmm) onto your target machine and install the required kernel build dependencies.
2. Build and install the kernel with the following config options below. The Makefiles for the MFS modules expect the compile output to be placed in `<kernel dir>/kbuild/`
    * CONFIG\_TRANSPARENT\_HUGEPAGE=y
    * CONFIG\_PAGE\_TABLE\_ISOLATION=n
    * CONFIG\_RETPOLINE=n
    * CONFIG\_GDB\_SCRIPTS=y
    * CONFIG\_FRAME\_POINTERS=y
    * CONFIG\_IKHEADERS=y
    * CONFIG\_SLAB\_FREELIST\_RANDOM=y
    * CONFIG\_SHUFFLE\_PAGE\_ALLOCATOR=y
    * CONFIG\_FS\_DAX=y
    * CONFIG\_DAX=y
    * CONFIG\_BLK\_DEV\_RAM=y
    * CONFIG\_FILE\_BASED\_MM=y
    * CONFIG\_BLK\_DEV\_PMEM=y
    * CONFIG\_ND\_BLK=y
    * CONFIG\_BTT=y
    * CONFIG\_NVDIMM\_PFN=y
    * CONFIG\_NVDIMM\_DAX=y
    * CONFIG\_X86\_PMEM\_LEGACY=y
    * CONFIG\_INIT\_ON\_ALLOC\_DEFAULT\_ON=n
3. Build the MFS kernel modules by going into their directory and running `make`.
If you don't know how to build and install a kernel, follow these [helpful instructions from Mark Mansi](https://pages.cs.wisc.edu/~markm/kernel-build-cloudlab.html).

If you instead want to use a script to build the kernel, you can use the setup instructions from [the USENIX ATC artifact README](https://github.com/multifacet/fbmm-artifact).
This requires running some software on your workstation, but also goes through the effort of installing dependencies on the test machine.

## Mounting an MFS
### BasicMMFS
1. `sudo insmod <kernel dir>/BasicMMFS/basicmmfs.ko`
2. `sudo mount -t BasicMMFS BasicMMFS -o numpages=<size> <mntdir>`

Where `<size>` is the number of pages to reserve for BasicMMFS

### BandwidthMMFS
1. `sudo insmod <kernel dir>/BandwidthMMFS/bandwidth.ko`
2. `sudo mount -t BandwidthMMFS BandwidthMMFS <mntdir>`
3. To set the interleave weight for a node: `echo <weight> | sudo tee /sys/fs/bwmmfs*/node<nid>/weight`

### ContigMMFS
1. `sudo insmod <kernel dir>/ContigMMFS/contigmmfs.ko`
2. `sudo mount -t ContigMMFS ContigMMFS <mntdir>`

### TieredMMFS
1. Follow the [following instructions](https://docs.pmem.io/persistent-memory/getting-started-guide/creating-development-enviroents/linux-enviroentsents/linux-memmap) to reserve both local and remote memory using the `memmap` boot option.
2. `sudo insmod <kernel dir>/TieredMMFS/tieredmmfs.ko`
3. `sudo mount -t TieredMMFS -o slowmem=/dev/pmem1 -o basepage=<use basepages> /dev/pmem0 <mntdir>`

Where `<use basepages>` is `true` if the MFS should only allocate base pages, and `false` if the MFS should allocate 2MB pages.

## Enabling FBMM
1. Make sure the MFS mount directory is accessable to user programs: `sudo chown -R $USER <mntdir>`
2. Enable FBMM: `echo 1 | sudo tee /sys/kernel/mm/fbmm/state`

## Running an application with an MFS
A user sets the MFS to use for an application by writing the mount directory of the MFS to `/proc/<pid>/fbmm_mnt_dir`, where `<pid>` is the PID of the process.
To have an application allocate its memory from an MFS on startup, use the [FBMM wrapper program](https://github.com/multifacet/fbmm-workspace/blob/main/bmks/fbmm_wrapper.c) provided in this repo at `./bmks/fbmm_wrapper.c`.
Its usage is

```
./fbmm_wrapper <mntdir> <app> <args>
```

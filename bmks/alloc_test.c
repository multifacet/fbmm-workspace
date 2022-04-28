#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>

#define ADDRESS (0x7f5707200000ul)
#define HPAGE_MASK (~(0x200000 - 1))
static __inline__ unsigned long long rdtsc(void)
{
	unsigned hi, lo;
	__asm__ __volatile__ ("rdtsc" : "=a"(lo), "=d"(hi));
	return ((unsigned long long)lo) | (((unsigned long long)hi) << 32);
}

int main(int argc, char *argv[]) {
	unsigned long long start, end;
	unsigned long long map_time = 0, unmap_time = 0;
	unsigned long size;
	void **addr;
	unsigned long hint;
	unsigned long num_allocations = 1;
	int flags = MAP_ANONYMOUS | MAP_PRIVATE | MAP_POPULATE;

	if (argc < 2) {
		printf("Missing size in GB.");
		return -1;
	}
	if (argc >= 3) {
		num_allocations = strtoul(argv[2], NULL, 10);
	}
	if (argc >= 4) {
		flags |= MAP_HUGETLB;
	}

	addr = malloc(num_allocations * sizeof(void*));
	hint = ADDRESS;

	size = strtoul(argv[1], NULL, 10);
	size = (size << 30) / num_allocations;

	// Do the allocing
	for (int i = 0; i < num_allocations; i++) {
		start = rdtsc();
		addr[i] = mmap((void*)hint, size, PROT_WRITE | PROT_READ,
			flags, -1, 0);
		end = rdtsc();

		map_time += end - start;
		hint = (hint - size) & HPAGE_MASK;

		printf("%p\n", addr[i]);
	}
	printf("Allocation done in %llu cycles\n", map_time);

	for (int i = 0; i < num_allocations; i++) {
		start = rdtsc();
		munmap(addr[i], size);
		end = rdtsc();

		unmap_time += end - start;
	}
	printf("Unmap done in %llu cycles\n", unmap_time);

	free(addr);
	return 0;
}

#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>

#define ADDRESS ((void*)0x7f5707200000ul)
static __inline__ unsigned long long rdtsc(void)
{
	unsigned hi, lo;
	__asm__ __volatile__ ("rdtsc" : "=a"(lo), "=d"(hi));
	return ((unsigned long long)lo) | (((unsigned long long)hi) << 32);
}

int main(int argc, char *argv[]) {
	unsigned long long start, end;
	unsigned long size;
	void *addr;

	if (argc < 2) {
		printf("Missing size in GB.");
		return -1;
	}

	size = strtoul(argv[1], NULL, 10);

	start = rdtsc();

	addr = mmap(ADDRESS, size << 30, PROT_WRITE | PROT_READ,
		MAP_ANONYMOUS | MAP_PRIVATE | MAP_POPULATE, -1, 0);

	end = rdtsc();
	printf("Done in %llu cycles\n", end - start);
	printf("%p\n", addr);
}

#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <pthread.h>

#define ADDRESS (0x7f5707200000ul)
#define PAGE_SHIFT (12)

unsigned long size;
unsigned long num_allocations = 1;
unsigned long num_threads = 1;
volatile int begin = 0;
int flags = MAP_ANONYMOUS | MAP_PRIVATE;

static __inline__ unsigned long long rdtsc(void)
{
	unsigned hi, lo;
	__asm__ __volatile__ ("rdtsc" : "=a"(lo), "=d"(hi));
	return ((unsigned long long)lo) | (((unsigned long long)hi) << 32);
}

void *map_thread(void *ptr) {
	unsigned long long start, end;
	unsigned long long map_time;
	void **addr = (void**) ptr;

	while (!begin) {}

	for (int i = 0; i < num_allocations; i++) {
		start = rdtsc();
		addr[i] = mmap(NULL, size, PROT_WRITE | PROT_READ,
			flags, -1, 0);
		end = rdtsc();

		map_time += end - start;
	}
	printf("Allocation done in %llu cycles\n", map_time);

	return (void *)map_time;
}

void *unmap_thread(void *ptr) {
	unsigned long long start, end;
	unsigned long long unmap_time;
	void **addr = (void**) ptr;

	while (!begin) {}

	for (int i = 0; i < num_allocations; i++) {
		start = rdtsc();
		munmap(addr[i], size);
		end = rdtsc();

		unmap_time += end - start;
	}
	printf("Unmap done in %llu cycles\n", unmap_time);

	return (void *)unmap_time;
}

int main(int argc, char *argv[]) {
	void ***addr;
	pthread_t *threads;
	unsigned long long total_map_time = 0, total_unmap_time = 0;

	if (argc < 2) {
		printf("Missing size in number of pages");
		return -1;
	}
	if (argc >= 3) {
		num_allocations = strtoul(argv[2], NULL, 10);
	}
	if (argc >= 4) {
		num_threads = strtoul(argv[3], NULL, 10);
	}
	if (argc >= 5) {
		flags |= MAP_HUGETLB;
	}

	size = strtoul(argv[1], NULL, 10);
	size = size << PAGE_SHIFT;

	addr = malloc(num_threads * sizeof(void**));
	threads = malloc(num_threads * sizeof(pthread_t));
	for (int i = 0; i < num_threads; i++) {
		addr[i] = malloc(num_allocations * sizeof(void*));
	}

	// map threads
	for (int i = 0; i < num_threads; i++) {
		pthread_create(&threads[i], NULL, map_thread, addr[i]);
	}

	printf("Started map threads\n");
	begin = 1;

	for (int i = 0; i < num_threads; i++) {
		unsigned long long map_time;
		pthread_join(threads[i], (void**)&map_time);

		total_map_time += map_time;
	}
	printf("Total map time: %llu cycles\n", total_map_time);

	// unmap threads
	begin = 0;
	for (int i = 0; i < num_threads; i++) {
		pthread_create(&threads[i], NULL, unmap_thread, addr[i]);
	}

	printf("Started unmap threads\n");
	begin = 1;

	for (int i = 0; i < num_threads; i++) {
		unsigned long long unmap_time;
		pthread_join(threads[i], (void**)&unmap_time);

		total_unmap_time += unmap_time;
	}
	printf("Total unmap time: %llu cycles\n", total_unmap_time);


	for (int i = 0; i < num_threads; i++)
		free(addr[i]);
	free(addr);
	return 0;
}

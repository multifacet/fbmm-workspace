all: alloc_test fbmm_wrapper badger-trap stream gups/gups
	@echo "done"

alloc_test: alloc_test.c
	gcc alloc_test.c -o alloc_test

fbmm_wrapper: fbmm_wrapper.c
	gcc fbmm_wrapper.c -o fbmm_wrapper

badger-trap: badger-trap.c
	gcc badger-trap.c -o badger-trap

stream: stream.c
	gcc -O stream.c -fopenmp -D_OPENMP -DSTREAM_ARRAY_SIZE=100000000 -DNTIMES=50 -o stream

gups/gups:
	$(MAKE) -C gups/

clean:
	rm -rf fbmm_wrapper alloc_test badger-trap stream
	$(MAKE) -C gups/ clean

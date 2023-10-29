all: alloc_test fbmm_wrapper badger-trap gups/gups
	@echo "done"

alloc_test: alloc_test.c
	gcc alloc_test.c -o alloc_test

fbmm_wrapper: fbmm_wrapper.c
	gcc fbmm_wrapper.c -o fbmm_wrapper

badger-trap: badger-trap.c
	gcc badger-trap.c -o badger-trap

gups/gups:
	$(MAKE) -C gups/

clean:
	rm -rf fbmm_wrapper alloc_test badger-trap
	$(MAKE) -C gups/ clean

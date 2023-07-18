all: alloc_test fbmm_wrapper gups/gups
	@echo "done"

alloc_test: alloc_test.c
	gcc alloc_test.c -o alloc_test

fbmm_wrapper: fbmm_wrapper.c
	gcc fbmm_wrapper.c -o fbmm_wrapper

gups/gups:
	$(MAKE) -C gups/

clean:
	rm -rf fbmm_wrapper alloc_test
	$(MAKE) -C gups/ clean

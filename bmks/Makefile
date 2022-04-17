all: alloc_test fom_wrapper gups/gups
	@echo "done"

alloc_test: alloc_test.c
	gcc alloc_test.c -o alloc_test

fom_wrapper: fom_wrapper.c
	gcc fom_wrapper.c -o fom_wrapper

gups/gups:
	$(MAKE) -C gups/

clean:
	rm -rf fom_wrapper alloc_test
	$(MAKE) -C gups/ clean

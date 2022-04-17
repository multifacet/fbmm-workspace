CC = gcc
CFLAGS = -g -Wall
LIBS = -lm -lpthread

gups: gups.o
	$(CC) $(CFLAGS) -o $@ $^ $(LIBS)

gups.o: gups.c zipf.c gups.h
	$(CC) $(CFLAGS) -c gups.c zipf.c

clean:
	rm -rf gups gups.o zipf.o

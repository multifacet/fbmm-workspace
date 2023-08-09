CC = gcc
CFLAGS = -g -Wall
LIBS = -lm -lpthread

all: gups gups-hotset-move
	@echo "Done"

gups: gups.o
	$(CC) $(CFLAGS) -o $@ $^ $(LIBS)

gups-hotset-move: gups-hotset-move.o
	$(CC) $(CFLAGS) -o $@ $^ $(LIBS)

gups.o: gups.c zipf.c gups.h
	$(CC) $(CFLAGS) -c gups.c zipf.c

gups-hotset-move.o: gups-hotset-move.c zipf.c gups.h
	$(CC) $(CFLAGS) -c gups-hotset-move.c zipf.c

clean:
	rm -rf gups gups-hotset-move gups.o gups-hotset-move.o zipf.o

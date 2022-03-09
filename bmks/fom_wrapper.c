#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <errno.h>
#include <string.h>

int main(int argc, char *argv[])
{
	char *program_name;
	char *fom_pid_filename = "/sys/kernel/mm/fom/pid";
	FILE *file;
	pid_t pid;

	if (argc < 2) {
		fprintf(stderr, "Usage: fom_wrapper <program> [args..]\n");
		return -1;
	}

	program_name = argv[1];

	pid = getpid();

	file = fopen(fom_pid_filename, "w");
	if (!file) {
		fprintf(stderr, "Could not open %s\n", fom_pid_filename);
		return -1;
	}
	fprintf(file, "%d", pid);

	fclose(file);

	// execute the intended program
	execv(program_name, &argv[1]);

	// We only get here if execv fails
	fprintf(stderr, "Failed to execute %s: %s\n", program_name, strerror(errno));

	return -1;
}

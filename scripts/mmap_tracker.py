#!/usr/bin/python3
from bcc import BPF
import argparse
import sys
import os

parser = argparse.ArgumentParser(description="Print the length of mmap calls")
parser.add_argument("-c", "--comm", help="The name of the process to track")
parser.add_argument("--ebpf", action="store_true", help="Print the eBPF script")
args = parser.parse_args()

bpf_text = """
#include <linux/sched.h>
#include <linux/fs.h>
#include <uapi/linux/ptrace.h>
#include <bcc/proto.h>

struct mmap_info_t {
	u64 len;
	u32 pid;
	u32 tgid;
	char comm[TASK_COMM_LEN];
}

BPF_PERF_OUTPUT(mmap_events);

static bool strequals(char *s1, char *s2, u32 len) {
    for (u32 i = 0; i < len; i++) {
        if (s1[i] != s2[i]) {
            return false;
        }

        if (s1[i] == '\\0') {
            return true;
        }
    }

    for (u32 i = 0; i < 2; i++){}

    return true;
}

int mmap_call(struct pt_regs *ctx, struct file *f, u64 addr, u64 len) {
    u64 pid_tgid = bpf_get_current_pid_tgid();
    u32 pid = pid_tgid >> 32;
    u32 tgid = pid_tgid & 0xFFFFFFFF;
    char comm[TASK_COMM_LEN];
    char target[TASK_COMM_LEN] = "TARGET_COMM";
    struct mmap_info_t info;

    bpf_get_current_comm(&comm, sizeof(comm));
    if (FILTER_PROC)
        return 0;

	if (f != NULL)
		return 0;

	info.len = len;
	info.pid = pid;
	info.tgid = tgid;
	bpf_get_current_comm(&info.comm, sizeof(info.comm));

	mmap_events.perf_submit(ctx, &info, sizeof(info));

	return 0;
}
"""

# Do code substitution for the process filtering
if args.comm:
    if len(args.comm) > 16:
        args.comm = args.comm[0:15]

    bpf_text = bpf_text.replace("TARGET_COMM", args.comm)
    bpf_text = bpf_text.replace("FILTER_PROC", "!strequals(comm, target, TASK_COMM_LEN)")
else:
    bpf_text = bpf_text.replace("FILTER_PROC", "0")

b = BPF(text=bpf_text)
b.attach_kprobe(event="do_mmap", fn_name="mmap_call")

header_string = "%-10.10s,%-6s,%-6s,%-14s"
format_string = "%-10.10s,%-6d,%-6d,%-14d"
print(header_string % ("COMM", "PID", "TGID", "MMAP_LEN"))

def handle_mmap_event(cpu, data, size):
	event: b["mmap_events"].event(data)

	print(format_string % (event.comm, event.pid, event.tgid, event.len))
	sys.stdout.flush()

b["mmap_events"].open_perf_buffer(handle_mmap_event)

while not os.path.isfile("/tmp/stop_mmap_tracker"):
    try:
        b.perf_buffer_poll()
    except KeyboardInterrupt:
        print()
        break
print("Exiting mmap_tracker.py")
sys.stdout.flush()
exit()

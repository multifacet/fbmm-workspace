#!/usr/bin/python3
from bcc import BPF
import argparse
import sys

parser = argparse.ArgumentParser(description="Measure how long page faults are on average")
parser.add_argument("-c", "--comm", help="The name of the process to track")
parser.add_argument("--ebpf", action="store_true", help="Print the eBPF script")
args = parser.parse_args()

bpf_text = """
#include <linux/sched.h>
#include <uapi/linux/ptrace.h>
#include <bcc/proto.h>

struct fault_info_t {
    u64 time_in_fault;
    u64 number_faults;
    u32 pid;
    u32 tgid;
    char comm[TASK_COMM_LEN];
};

BPF_HASH(fault_start, u64, u64);
BPF_HASH(fault_stats, u64, struct fault_info_t);
BPF_PERF_OUTPUT(fault_events);

static bool strequals(char *s1, char *s2, u32 len) {
    for (u32 i = 0; i < len; i++) {
        if (s1[i] != s2[i]) {
            return false;
        }

        if (s1[i] == '\\0') {
            return true;
        }
    }

//    for (u32 i = 0; i < 2; i++){}

    return true;
}

int pf_start(struct pt_regs *ctx) {
    u64 pid_tgid = bpf_get_current_pid_tgid();
    u64 start = bpf_ktime_get_ns();
    char comm[TASK_COMM_LEN];
    char target[TASK_COMM_LEN] = "TARGET_COMM";

    bpf_get_current_comm(&comm, sizeof(comm));
    if (FILTER_PROC)
        return 0;

    fault_start.update(&pid_tgid, &start);

    return 0;
}

int pf_end(struct pt_regs *ctx) {
    u64 pid_tgid = bpf_get_current_pid_tgid();
    u32 pid = pid_tgid >> 32;
    u32 tgid = pid_tgid & 0xFFFFFFFF;
    u64 end = bpf_ktime_get_ns();
    char comm[TASK_COMM_LEN];
    char target[TASK_COMM_LEN] = "TARGET_COMM";

    bpf_get_current_comm(&comm, sizeof(comm));
    if (FILTER_PROC)
        return 0;

    u64 *start;
    struct fault_info_t *info;

    start = fault_start.lookup(&pid_tgid);
    if (start == 0)
        return 0;

    info = fault_stats.lookup(&pid_tgid);
    if (info == 0) {
        struct fault_info_t new;
        new.time_in_fault = end - *start;
        new.number_faults = 1;
        new.pid = pid;
        new.tgid = tgid;
        bpf_get_current_comm(&new.comm, sizeof(new.comm));

        fault_stats.update(&pid_tgid, &new);

        return 0;
    }

    info->time_in_fault += end - *start;
    info->number_faults += 1;

    fault_start.delete(&pid_tgid);
    fault_stats.update(&pid_tgid, info);

    return 0;
}

TRACEPOINT_PROBE(sched, sched_process_exit) {
    u64 pid_tgid = bpf_get_current_pid_tgid();
    u32 pid = pid_tgid >> 32;
    u32 tgid = pid_tgid & 0xFFFFFFFF;

    struct fault_info_t *info = fault_stats.lookup(&pid_tgid);
    if (info == 0)
        return 0;

    bpf_get_current_comm(info->comm, sizeof(info->comm));
    fault_events.perf_submit(args, info, sizeof(*info));

    fault_stats.delete(&pid_tgid);

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

if args.ebpf:
    print(bpf_text)
    exit()

b = BPF(text=bpf_text)
b.attach_kprobe(event="__handle_mm_fault", fn_name="pf_start")
b.attach_kretprobe(event="__handle_mm_fault", fn_name="pf_end")

header_string = "%-10.10s %-6s %-6s %-14s %-14s %-8s"
format_string = "%-10.10s %-6d %-6d %-14d %-14d %-8d"
print(header_string % ("COMM", "PID", "TID", "FAULT_TIME", "FAULT_COUNT", "AVG"))

def handle_fault_event(cpu, data, size):
    event = b["fault_events"].event(data)

    print(format_string % (event.comm, event.pid, event.tgid, event.time_in_fault,
        event.number_faults, event.time_in_fault / event.number_faults))
    sys.stdout.flush()

b["fault_events"].open_perf_buffer(handle_fault_event)
#b.trace_print()

while 1:
    try:
        b.perf_buffer_poll()
    except KeyboardInterrupt:
        print()
        exit()

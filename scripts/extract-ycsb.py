#!/usr/bin/env python3

import sys
import os
import json
import re

def eprint(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs)

json_data = None
for line in sys.stdin:
    json_data = json.loads(line)

filename = json_data['results_path']
cmd = json_data['cmd']
machine_class = json_data['class']

kernel_type = "TPP" if machine_class == "tpp" else "FBMM"
# False if we are using actual TPP or FBMM, True otherwise
using_base_kernel = not (("--tpp" in cmd) or ("--fbmm" in cmd))
did_reserve_mem = "--dram_size" in cmd

experiment_type = kernel_type
if using_base_kernel:
    experiment_type += " Base "
    experiment_type += " Split" if did_reserve_mem else " Local"

# Parse the YCSB file for the results
runtime = None
throughput = None
latency = None

# The index of the split line array of the ycsb output that has the name
# of the value the line has
for line in open(filename, "r"):
    split = line.split()
    op_type = split[0]
    value_name = split[1]
    value = split[2]

    if "RunTime" in value_name:
        runtime = value
    elif "Throughput" in value_name:
        throughput = value
    elif op_type == "[READ]," and "AverageLatency" in value_name:
        latency = value

outdata = {
    "Type": experiment_type,
    "Runtime": runtime,
    "Throughput": throughput,
    "Latency": latency,
    "Command": cmd,
    "File": filename,
}

print(json.dumps(outdata))

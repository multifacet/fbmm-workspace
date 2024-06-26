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

if machine_class == "tpp":
    kernel_type = "TPP"
elif machine_class == "hmsdk":
    kernel_type = "HMSDK"
else:
    kernel_type = "FBMM"
# False if we are using actual TPP or FBMM, True otherwise
using_base_kernel = not (("--tpp" in cmd) or ("--fbmm" in cmd) or ("--hmsdk" in cmd))
did_reserve_mem = "--dram_size" in cmd

experiment_type = kernel_type
if using_base_kernel:
    experiment_type += " Base "
    experiment_type += " Split" if did_reserve_mem else " Local"

# Sort is used to group things in google sheets.
# The values are arbitrary based on how I wanted things ordered.
# The code for this is a little hacky, but whatever
sort = 5 if kernel_type == "TPP" else 2
if using_base_kernel:
    sort = sort - 1
    if not did_reserve_mem:
        sort = sort - 1

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
    elif (op_type == "[READ]," or op_type == "[UPDATE],") and "AverageLatency" in value_name:
        latency = value

outdata = {
    "Sort": str(sort),
    "Type": experiment_type,
    "Runtime": runtime,
    "Throughput": throughput,
    "Latency": latency,
    "Command": cmd,
    "File": filename,
}

print(json.dumps(outdata))

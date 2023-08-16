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
gups = None

# The index of the split line array of the ycsb output that has the name
# of the value the line has
for line in open(filename, "r"):
    split = line.split()
    value_name = split[0]

    if "Elapsed" in value_name:
        runtime = split[-2]
    elif "GUPS" in value_name:
        gups = split[-1]

if runtime is None:
    eprint("runtime")
if gups is None:
    eprint("gups")
    eprint(json_data['jid'])

outdata = {
    "Sort": str(sort),
    "Type": experiment_type,
    "Runtime": runtime,
    "GUPS": gups,
    "Command": cmd,
    "File": filename,
}

print(json.dumps(outdata))

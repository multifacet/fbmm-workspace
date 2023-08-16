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

local_dram = None
remote_dram = None

for line in open(filename, "r"):
    split = line.split()

    if len(split) < 2:
        continue

    value = split[0]
    label = split[1]

    if "local_dram" in label:
        local_dram = value.replace(',', '')
    elif "remote_dram" in label:
        remote_dram = value.replace(',', '')

combined = str(int(local_dram) + int(remote_dram))
percent_remote = "{:.3f}".format(float(remote_dram) * 100 / float(combined))

outdata = {
    "Sort": str(sort),
    "Type": experiment_type,
    "local_dram": local_dram,
    "remote_dram": remote_dram,
    "Combined": combined,
    "Percent Remote": percent_remote,
    "Command": cmd,
    "File": filename,
}

print(json.dumps(outdata))

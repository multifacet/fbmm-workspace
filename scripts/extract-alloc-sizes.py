#!/usr/bin/env python3

import sys
import os
import json
import re
import glob

def eprint(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs)

def find_pattern(cmd, pattern):
    REGEX = "(" + pattern + ")"
    match = re.search(REGEX, cmd)
    return match is not None

json_data = None
for line in sys.stdin:
    json_data = json.loads(line)

filename = json_data['results_path']
cmd = json_data['cmd']
num_allocs = cmd.split()[-1]
page_size = "Base" if find_pattern(cmd, "disable_thp") else "Huge"
kernel = "FOM" if find_pattern(cmd, "--fom") else "Linux"

# Read in the file, though we only care about the last 2 line
lines = []
for line in open(filename, "r"):
    lines.append(line)

# Alloc cycles is in the second to last word in the second to last line
alloc_cycles = lines[-2].split()[-2]
# Freeing cycles is in the second to last word in the last line
free_cycles = lines[-1].split()[-2]

outdata = {
    "Command": cmd,
    "File": filename,
    "Kernel": kernel,
    "Page Size": page_size,
    "Num Allocs": num_allocs,
    "Alloc Cycles": alloc_cycles,
    "Free Cycles": free_cycles,
}

eprint(json.dumps(outdata, indent=2))
print(json.dumps(outdata))

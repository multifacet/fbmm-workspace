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
jid = json_data['jid']

alloctest_results = filename + "alloctest"
fbmm_stats = filename + "fbmm_stats"

base_kernel = not ("--fbmm" in cmd)
populate = "--populate" in cmd

# Find the parameters for the command
m = re.search("alloctest ([0-9]+) ([0-9]+)", cmd)
alloc_size = m.group(1)
num_allocs = m.group(2)

m = re.search("--threads ([0-9]+)", cmd)
if m is None:
	threads = "1"
else:
	threads = m.group(1)

if base_kernel:
	kernel = "Linux"
else:
	kernel = "FBMM"

# Read in the map and unmap times
map_time = "-1"
unmap_time = "-1"
for line in open(alloctest_results, "r"):
	split = line.split()

	if "Total map time:" in line:
		map_time = split[3]
	elif "Total unmap time:" in line:
		unmap_time = split[3]

# Read in the FBMM stats if applicable
file_create_time = "-1"
file_register_time = "-1"
munmap_time = "-1"
if not base_kernel:
	for line in open(fbmm_stats):
		split = line.split()

		if "file create times:" in line:
			file_create_time = split[3]
		elif "file register times:" in line:
			file_register_time = split[3]
		elif "munmap_timeap times:" in line:
			munmap_time = split[2]

outdata = {
	"Kernel": kernel,
	"Alloc Size": alloc_size,
	"Num Allocs": num_allocs,
	"Threads": threads,
    "Populate": str(populate),
	"Map Time": map_time,
	"Unmap Time": unmap_time,
	"File Create Time": file_create_time,
	"File Register Time": file_register_time,
	"Munmap Times": munmap_time,
	"Command": cmd,
	"File": filename,
	"JID": jid,
}

print(json.dumps(outdata))

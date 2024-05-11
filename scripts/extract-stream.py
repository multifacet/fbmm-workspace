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

base_kernel = not ("--fbmm" in cmd)
bwmmfs = "--bwmmfs" in cmd

experiment_type = ""
if bwmmfs:
	# Search for the node split
	weights = re.findall(":[0-9]+", cmd)
	experiment_type = weights[0][1:] + ":" + weights[1][1:]
else:
	experiment_type = "Linux"

copy_bw = None
scale_bw = None
add_bw = None
triad_bw = None

for line in open(filename, "r"):
	split = line.split();
	value_name = split[0]
	if len(split) > 1:
		bw = split[1]
	else:
		continue

	if "Copy" in value_name:
		copy_bw = bw
	elif "Scale" in value_name:
		scale_bw = bw
	elif "Add" in value_name:
		add_bw = bw
	elif "Triad" in value_name:
		triad_bw = bw

outdata = {
	"Type": experiment_type,
	"Copy": copy_bw,
	"Scale": scale_bw,
	"Add": add_bw,
	"Triad": triad_bw,
	"Command": cmd,
	"File": filename,
	"JID": jid,
}

if copy_bw is None or scale_bw is None or add_bw is None or triad_bw is None:
    eprint("Invalid output for job ", jid, " Consider restarting that job.")

print(json.dumps(outdata))

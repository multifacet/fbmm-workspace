#!/usr/bin/env python3

import matplotlib.pyplot as plt
import matplotlib.gridspec as gridspec
import matplotlib.transforms as transforms
import numpy as np
import csv
import sys

infile = sys.argv[1]
title = sys.argv[2]
outname = None
if len(sys.argv) >= 4:
    outname = sys.argv[3]

KERNEL_ORDER = ["Linux", "FOM", "HugeTLBFS"]
data = {}

barwidth = 0.2
cur_x = 0.2

xticks = []
tick_labels = []
kernels = set()

with open(infile, 'r') as f:
    reader = csv.DictReader(f)

    for row in reader:
        kernel = row['Kernel']
        tput = float(row['Throughput'])

        kernels.add(kernel)
        data[kernel] = tput

kernels = sorted(list(kernels), key = lambda w: KERNEL_ORDER.index(w))

plt.figure(figsize=(5, 7))

highest = 0
second_highest = 0
for k in kernels:
    xticks.append(cur_x)
    tick_labels.append(k)

    tput = data[k]

    if tput > highest:
        second_highest = highest
        highest = tput
        highest_pos = cur_x
        highest_kernel = k

    plt.bar(cur_x, tput, width=barwidth, color="tab:blue")

    cur_x += 2* barwidth

# Determine if we need to truncate the graph
if highest/second_highest > 5:
    plt.ylim((0, second_highest * 1.1))
    plt.text(highest_pos, second_highest * 1.09, str(int(data[highest_kernel])), color='white', fontsize=12, ha='center', va='top')

plt.xticks(xticks, tick_labels, fontsize=16)
plt.ylabel("Throughput (GB/s)", fontsize=16)
plt.title(title, fontsize=16)

if outname:
    plt.savefig(outname, bbox_inches="tight")
plt.show()

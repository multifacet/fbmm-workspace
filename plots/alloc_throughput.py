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
if len(sys.argv) > 4:
    outname = sys.argv[2]

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
        opt = row['Optimization']
        tput = float(row['Throughput'])

        kernels.add(kernel)
        if kernel in data:
            data[kernel].append((opt, tput))
        else:
            data[kernel] = [(opt, tput)]

kernels = sorted(list(kernels), key = lambda w: KERNEL_ORDER.index(w))

plt.figure()

def plot_stacked_bars(cur_x, vals):
    bottom = 0
    for opt,tput in vals:
        if opt == "Initial":
            color = "blue"
        else:
            color = None
        plt.bar(cur_x, tput - bottom, width=barwidth, bottom=bottom, label=opt, color=color)
        bottom = tput

# Plot the huge page stuff
for k in kernels:
    xticks.append(cur_x)
    tick_labels.append(k)

    print(data[k])
    data[k].sort(key=lambda w: w[1])
    print(data[k])
    plot_stacked_bars(cur_x, data[k])

    cur_x += 2 * barwidth

plt.xticks(xticks, tick_labels)
plt.ylabel("Allocation Throughput (GB/s)")
plt.legend()

if outname:
    plt.savefig(outname)
plt.show()

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
colors = ["tab:orange", "tab:green", "tab:red", "tab:purple",
          "tab:brown", "tab:pink", "tab:gray", "tab:olive", "tab:cyan"]

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

plt.figure(figsize=(5, 3.5))

def plot_stacked_bars(cur_x, vals, color_index):
    bottom = 0
    for opt,tput in vals:
        if opt == "Initial":
            color = "tab:blue"
            if color_index == 0:
                label = opt
            else:
                label = None
        else:
            color = colors[color_index]
            label = opt
            color_index += 1
        plt.bar(cur_x, tput - bottom, width=barwidth, bottom=bottom, label=label, color=color)
        bottom = tput
    return color_index

# Plot the huge page stuff
color_index = 0
for k in kernels:
    xticks.append(cur_x)
    tick_labels.append(k)

    data[k].sort(key=lambda w: w[1])
    color_index = plot_stacked_bars(cur_x, data[k], color_index)

    cur_x += 2 * barwidth

plt.xticks(xticks, tick_labels)
plt.ylabel("Allocation Throughput (GB/s)")
plt.title(title)
plt.legend()

if outname:
    plt.savefig(outname)
plt.show()

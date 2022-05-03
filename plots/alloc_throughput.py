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
colors = ["tab:blue", "tab:orange", "tab:green", "tab:red", "tab:purple",
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

plt.figure(figsize=(10, 7))

def plot_stacked_bars(cur_x, vals, color_index):
    bottom = 0
    for opt,tput in vals:
        if opt == "Initial":
            color = colors[0]
            if color_index == 0:
                label = opt
                color_index += 1
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
plt.ylabel("Throughput (GB/s)")
plt.title(title)
# Don't print the legend if there was only one label
if color_index > 1:
    plt.legend(bbox_to_anchor=(1.01, 1), loc="upper left")

if outname:
    plt.savefig(outname, bbox_inches="tight")
plt.show()

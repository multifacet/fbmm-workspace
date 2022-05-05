#!/usr/bin/env python3

import matplotlib.pyplot as plt
import matplotlib.gridspec as gridspec
import matplotlib.transforms as transforms
import numpy as np
import csv
import sys

huge_file = sys.argv[1]
base_file = sys.argv[2]
outname = None
if len(sys.argv) >= 4:
    outname = sys.argv[3]

KERNEL_ORDER = ["Linux", "FOM", "HugeTLBFS"]
colors = {"Baseline": "tab:blue",
    "Nontemporal Zero": "tab:orange",
    "follow_page_mask Fix": "tab:green",
    "Write Zeros Driver": "tab:red",
    "No track_pfn_insert": "tab:purple",
    "Disable Metadata": "tab:brown",
    "Preallocation": "tab:gray"}
used_labels = {}

barwidth = 0.2

def read_file(infile):
    data = {}
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
    return (data, kernels)

def plot_stacked_bars(ax, cur_x, vals):
    bottom = 0
    for opt,tput in vals:
        color = colors[opt]
        if opt in used_labels:
            label = None
        else:
            label = opt
            used_labels[opt] = True
        ax.bar(cur_x, tput - bottom, width=barwidth, bottom=bottom, label=label, color=color)
        bottom = tput

def make_plot(ax, data, kernels):
    kernels = sorted(list(kernels), key = lambda w: KERNEL_ORDER.index(w))
    cur_x = 0.2
    xticks = []
    tick_labels = []

    # Plot the huge page stuff
    for k in kernels:
        xticks.append(cur_x)
        tick_labels.append(k)

        data[k].sort(key=lambda w: w[1])
        plot_stacked_bars(ax, cur_x, data[k])

        cur_x += 2 * barwidth

    ax.set_xticks(xticks)
    ax.set_xticklabels(tick_labels)
    ax.set_ylabel("Throughput (GB/s)")


(huge_data, huge_kernels) = read_file(huge_file)
(base_data, base_kernels) = read_file(base_file)

fig, (base_ax, huge_ax) = plt.subplots(1, 2, figsize=(10,7))
base_ax.set_title("Base Pages")
huge_ax.set_title("Huge Pages")

make_plot(base_ax, base_data, base_kernels)
make_plot(huge_ax, huge_data, huge_kernels)

handles = []
labels = []
for ax in fig.axes:
    (h, l) = ax.get_legend_handles_labels()
    handles.extend(h)
    labels.extend(l)
plt.legend(handles, labels, bbox_to_anchor=(1.01, 1), loc="upper left")

if outname:
    plt.savefig(outname, bbox_inches="tight")
plt.show()

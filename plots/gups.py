#!/usr/bin/env python3

import matplotlib.pyplot as plt
import matplotlib.gridspec as gridspec
import matplotlib.transforms as transforms
import numpy as np
import csv
import sys

infile = sys.argv[1]
outname = None
if len(sys.argv) >= 3:
    outname = sys.argv[2]

KERNEL_ODER = ["Linux", "FOM", "HugeTLBFS"]

barwidth = 0.2

data = {}
kernels = set()
with open(infile, 'r') as f:
    reader = csv.DictReader(f)

    for row in reader:
        kernel = row['Kernel']
        thp = row['THP'] == 'TRUE'
        gups = float(row['GUPS'])

        kernels.add(kernel)
        data[(kernel, thp)] = gups

kernels = sorted(list(kernels), key = lambda w: KERNEL_ODER.index(w))

def make_plot(ax, data, kernels, thp):
    cur_x = 0.2
    xticks = []
    tick_labels = []

    for k in kernels:
        xticks.append(cur_x)
        tick_labels.append(k)

        gups = data[(k, thp)]

        ax.bar(cur_x, gups, width=barwidth, color="tab:blue")

        cur_x += 2 * barwidth
    ax.set_xticks(xticks)
    ax.set_xticklabels(tick_labels)

fig, (base_ax, huge_ax) = plt.subplots(1, 2, sharey=True, figsize=(10,7))
base_ax.set_title("Base Pages")
huge_ax.set_title("Huge Pages")
base_ax.set_ylabel("GUPS")

make_plot(base_ax, data, kernels, False)
make_plot(huge_ax, data, kernels, True)

if outname:
    plt.savefig(outname, bbox_inches="tight")
plt.show()

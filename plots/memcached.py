#!/usr/bin/env python3

import matplotlib.pyplot as plt
import matplotlib.gridspec as gridspec
import matplotlib.transforms as transforms
from matplotlib.patches import Patch
import numpy as np
import csv
import sys

infile = sys.argv[1]
outname = None
if len(sys.argv) >= 3:
    outname = sys.argv[2]

KERNEL_ORDER = ["Linux", "FOM", "HugeTLBFS"]
WKLD_ORDER = ["Read", "Read/Write", "Insert"]
colors = {"Linux": "tab:blue", "FOM": "tab:orange"}

barwidth = 0.2

data = {}
kernels = set()
wklds = set()
with open(infile, 'r') as f:
    reader = csv.DictReader(f)

    for row in reader:
        kernel = row['Kernel']
        wkld = row['Workload']
        thp = row['THP'] == 'TRUE'
        tput = float(row['Throughput'])

        kernels.add(kernel)
        wklds.add(wkld)
        data[(kernel, wkld, thp)] = tput

kernels = sorted(list(kernels), key = lambda w: KERNEL_ORDER.index(w))
wklds = sorted(list(wklds), key = lambda w: WKLD_ORDER.index(w))

plt.figure(figsize=(10,7))

cur_x = 0.2
xticks = []
tick_labels = []

for wkld in wklds:
    start_x = cur_x
    for kernel in kernels:
        tput_base = data[(kernel, wkld, False)]
        tput_huge = data[(kernel, wkld, True)]

        color = colors[kernel]

        plt.bar(cur_x, tput_huge, width=barwidth, color=color,
            edgecolor="black", linewidth=0.5)
        cur_x += barwidth
        plt.bar(cur_x, tput_base, width=barwidth, color=color, hatch="/",
            edgecolor="black", linewidth=0.5)
        cur_x += barwidth
    
    xticks.append((start_x + cur_x - barwidth) / 2)
    tick_labels.append(wkld)
    cur_x += barwidth

# Generate the legend
legend_elements = []
for k in kernels:
    h_label = k + " Huge"
    b_label = k + " Base"
    legend_elements.append(Patch(facecolor=colors[k], edgecolor="k", label=h_label))
    legend_elements.append(Patch(facecolor=colors[k], edgecolor="k", label=b_label,
        hatch="/"))

plt.xticks(ticks=xticks, labels=tick_labels)
plt.ylabel("Throughput (ops/sec)")
plt.legend(handles=legend_elements, bbox_to_anchor=(1.01, 1), loc="upper left")

if outname:
    plt.savefig(outname, bbox_inches="tight")
plt.show()

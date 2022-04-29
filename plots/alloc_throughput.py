#!/usr/bin/env python3

import matplotlib.pyplot as plt
import matplotlib.gridspec as gridspec
import matplotlib.transforms as transforms
import numpy as np

barwidth = 0.2
linux_huge = [1, 2, 3]
fom_huge = [1, 1.5, 2]
hugetlb = [4]

linux_base = [5]
fom_base = [2, 3, 4]

cur_x = 0.2

xticks = []
tick_labels = []

plt.figure()

def plot_stacked_bars(cur_x, vals):
    bottom = 0
    for val in vals:
        plt.bar(cur_x, val - bottom, width=barwidth, bottom=bottom)
        bottom = val

# Plot the huge page stuff
xticks.append(cur_x)
tick_labels.append("Linux Huge")
plot_stacked_bars(cur_x, linux_huge)

cur_x += barwidth
xticks.append(cur_x)
tick_labels.append("FOM Huge")
plot_stacked_bars(cur_x, fom_huge)

cur_x += barwidth
xticks.append(cur_x)
tick_labels.append("HugeTLBFS")
plot_stacked_bars(cur_x, hugetlb)

cur_x += 2*barwidth

xticks.append(cur_x)
tick_labels.append("Linux Base")
plot_stacked_bars(cur_x, linux_base)

cur_x += barwidth
xticks.append(cur_x)
tick_labels.append("FOM Base")
plot_stacked_bars(cur_x, fom_base)

plt.xticks(xticks, tick_labels)
plt.ylabel("Allocation Throughput (GB/s)")

plt.show()

#!/usr/bin/env python3

import sys
import csv
import matplotlib.pyplot as plt
import numpy as np

filename = sys.argv[1]

configs = []
throughputs = {
    "Copy": [],
    "Scale": [],
    "Add": [],
    "Triad": []
}

# Read the data
with open(filename, "r") as csvfile:
    reader = csv.DictReader(csvfile)
    for row in reader:
        configs.append(row['Type'])
        throughputs['Copy'].append(float(row['Copy']))
        throughputs['Add'].append(float(row['Add']))
        throughputs['Scale'].append(float(row['Scale']))
        throughputs['Triad'].append(float(row['Triad']))

x = np.arange(len(configs))
print(x)
width = 0.2
multiplier = 0

plt.figure(figsize=(10, 6))
for attr, measurements in throughputs.items():
    offset = width * multiplier
    plt.bar(x + offset, measurements, width, label=attr)
    multiplier += 1

plt.legend(loc='upper left')
plt.title("Bandwidth Reported By Stream With HMSDK", fontsize=24)
plt.xticks(x + (1.5 * width), configs, fontsize=18, rotation=0)
plt.yticks(fontsize=18)
plt.ylabel("Bandwidth (MB/s)", fontsize=22)

plt.show()

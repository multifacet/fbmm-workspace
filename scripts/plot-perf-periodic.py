#!/usr/bin/env python3

import sys
import os
import re
import matplotlib.pyplot as plt

def eprint(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs)

filename = sys.argv[1]

# The key of this dictionary is the perf counter for the event
# The value is a list of the values
data = {}

for line in open(filename, "r"):
    # Skip lines that are empty or begin with "#"
    if line.strip() == "" or line[0] == "#":
        continue

    split = line.split()
    time = float(split[0])
    count = int(split[1].replace(',', ''))
    event = split[2]

    if event not in data:
        data[event] = ([0], [0])

    # Add the current count to the previous to get a running total
    data[event][0].append(time)
    data[event][1].append(count)

plt.rcParams.update({"font.size": 18})

for event in data:
    plt.plot(data[event][0], data[event][1], label=event.split('.')[-1])
plt.xlabel("Time (s)")
plt.ylabel("Event counts")
plt.legend()
plt.show()

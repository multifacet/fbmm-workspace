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
    count = int(split[1].replace(',', ''))
    event = split[2]

    if event not in data:
        data[event] = [0]

    # Add the current count to the previous to get a running total
    data[event].append(count + data[event][-1])

for event in data:
    plt.plot(data[event], label=event.split('.')[-1])

plt.legend()
plt.show()

#!/usr/bin/env python3

import sys
import os
import re
import matplotlib.pyplot as plt

filename = sys.argv[1]

# The key of this dictionary is the perf counter for the event
# The value is a list of the values
promotions = [0]
demotions = [0]
last_promotions = 0
last_demotions = 0

for line in open(filename, "r"):
    # The lines we want are of the form "Promotions: [0-9]+ Demotions: [0-9]+"
    if "Promotions" not in line:
        continue

    split = line.split()
    cur_promotions = int(split[1])
    cur_demotions = int(split[3])

    # Subtract from the previous to get a diff
    promotions.append(cur_promotions - last_promotions)
    demotions.append(cur_demotions - last_demotions)

    last_promotions = cur_promotions
    last_demotions = cur_demotions

plt.rcParams.update({"font.size": 18})

plt.plot(promotions, label="Promotions")
plt.plot(demotions, label="Demotions")

plt.xlabel("Sample")
plt.ylabel("Event Counts")
plt.legend()
plt.show()

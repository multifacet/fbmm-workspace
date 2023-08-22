#!/usr/bin/env python3

# This script plots the cdf of the runtime for base linux, base linux with the
# memory split between memory nodes, and modified linux (Either FBMM or TPP)
# Takes CSV file as input

import sys
import csv
import numpy as np
from matplotlib import pyplot as plt
from scipy.interpolate import UnivariateSpline

input_file = sys.argv[1]
data = {}

f = open(input_file)
reader = csv.DictReader(f)

for row in reader:
    filename = row['results_path']
    cmd = row['cmd']
    machine_class = row['class']

    kernel_type = "TPP" if machine_class == "tpp" else "FBMM"
    # False if we are using actual TPP or FBMM, True otherwise
    using_base_kernel = not (("--tpp" in cmd) or ("--fbmm" in cmd))
    did_reserve_mem = "--dram_size" in cmd

    experiment_type = kernel_type
    if using_base_kernel:
        experiment_type += " Base "
        experiment_type += " Split" if did_reserve_mem else " Local"

    if experiment_type not in data:
        data[experiment_type] = []

    for line in open(filename, "r"):
        split = line.split()
        value_name = split[1]
        value = split[2]

        if "RunTime" in value_name:
            data[experiment_type].append(float(value))
            break

for exp in data:
    plt.hist(data[exp], bins=50, density=True, histtype='step', cumulative=True, label=exp)

plt.legend(loc="upper left")
plt.show()


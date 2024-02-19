#!/usr/bin/python3
import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns
import sys

# Load the dataset
file_path = sys.argv[1]
data = pd.read_csv(file_path)
data['Throughput'] = data['Throughput'] / 1000

# Setting the style for the plot
sns.set(style="whitegrid")

# Creating the boxplot for the 'Triad' column with color
plt.figure(figsize=(10, 6))
sns.boxplot(x='Type', y='Throughput', data=data, linewidth=4, color='tab:cyan')

plt.title('Comparison of Throughput of Memcached', fontsize=24)
plt.ylabel('Throughput (kOps/s)', fontsize=22)
plt.xlabel('')
plt.ylim(ymin=0)
plt.yticks(fontsize=18)
plt.xticks(rotation=0, fontsize=22)
plt.tight_layout()

# Show the plot
plt.show()

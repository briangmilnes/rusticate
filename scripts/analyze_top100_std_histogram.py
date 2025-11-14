#!/usr/bin/env python3
"""
Generate histogram of std library usage across Top100Rust projects.
Shows how many projects use each std item.
"""

import os
import re
from collections import defaultdict
from pathlib import Path

def parse_project_report(report_path):
    """Extract all std items from a project report."""
    std_items = set()
    
    with open(report_path, 'r') as f:
        for line in f:
            # Match lines like "  std::fmt::Debug"
            match = re.match(r'^\s+(std::\S+)', line)
            if match:
                std_items.add(match.group(1))
    
    return std_items

def main():
    analyses_dir = Path('/home/milnes/projects/rusticate/analyses/top100_std_usage')
    
    # Map: std_item -> set of projects using it
    item_to_projects = defaultdict(set)
    
    project_count = 0
    
    # Process each project report
    for report_file in analyses_dir.glob('*.txt'):
        if report_file.name in ['all_std_items.txt', 'global_std_usage_frequency.txt']:
            continue
        
        project_name = report_file.stem
        project_count += 1
        
        std_items = parse_project_report(report_file)
        
        for item in std_items:
            item_to_projects[item].add(project_name)
    
    # Create histogram: (std_item, project_count)
    histogram = []
    for item, projects in item_to_projects.items():
        histogram.append((item, len(projects), sorted(projects)))
    
    # Sort by project count (descending), then by item name
    histogram.sort(key=lambda x: (-x[1], x[0]))
    
    # Write full histogram
    output_file = analyses_dir / 'std_usage_histogram.txt'
    with open(output_file, 'w') as f:
        f.write(f"Standard Library Usage Histogram\n")
        f.write(f"=================================\n")
        f.write(f"\n")
        f.write(f"Total projects analyzed: {project_count}\n")
        f.write(f"Total unique std items: {len(histogram)}\n")
        f.write(f"\n")
        f.write(f"Format: [project_count] std_item\n")
        f.write(f"\n")
        
        for item, count, projects in histogram:
            f.write(f"[{count:3d}] {item}\n")
    
    # Write detailed version with project names
    output_file_detailed = analyses_dir / 'std_usage_histogram_detailed.txt'
    with open(output_file_detailed, 'w') as f:
        f.write(f"Standard Library Usage Histogram (Detailed)\n")
        f.write(f"============================================\n")
        f.write(f"\n")
        f.write(f"Total projects analyzed: {project_count}\n")
        f.write(f"Total unique std items: {len(histogram)}\n")
        f.write(f"\n")
        f.write(f"Format: [project_count] std_item\n")
        f.write(f"        Projects: project1, project2, ...\n")
        f.write(f"\n")
        
        for item, count, projects in histogram:
            f.write(f"[{count:3d}] {item}\n")
            # Show up to 10 projects per line
            project_list = list(projects)
            for i in range(0, len(project_list), 10):
                chunk = project_list[i:i+10]
                f.write(f"      {', '.join(chunk)}\n")
            f.write(f"\n")
    
    # Print summary
    print(f"Standard Library Usage Histogram")
    print(f"=================================")
    print(f"")
    print(f"Total projects analyzed: {project_count}")
    print(f"Total unique std items: {len(histogram)}")
    print(f"")
    print(f"Output files:")
    print(f"  - {output_file}")
    print(f"  - {output_file_detailed}")
    print(f"")
    print(f"Top 30 most widely used std items:")
    for item, count, _ in histogram[:30]:
        print(f"  [{count:3d}] {item}")
    print(f"")
    print(f"Distribution by project count:")
    
    # Count how many items are used by N projects
    distribution = defaultdict(int)
    for _, count, _ in histogram:
        distribution[count] += 1
    
    print(f"")
    for proj_count in sorted(distribution.keys(), reverse=True):
        item_count = distribution[proj_count]
        print(f"  {proj_count:3d} projects: {item_count:4d} std items")

if __name__ == '__main__':
    main()


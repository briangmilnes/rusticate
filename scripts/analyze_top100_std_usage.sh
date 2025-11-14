#!/bin/bash
# Analyze standard library usage across Top100Rust projects

set -e

TOP100_DIR="/home/milnes/projects/Top100Rust"
RUSTICATE_BIN="/home/milnes/projects/rusticate/target/release/rusticate-review-std-lib"
OUTPUT_DIR="/home/milnes/projects/rusticate/analyses/top100_std_usage"

mkdir -p "$OUTPUT_DIR"

echo "Analyzing std library usage across Top 100 Rust projects..."
echo "============================================================"
echo ""

# Collect all std items across all projects
ALL_STD_ITEMS_FILE="$OUTPUT_DIR/all_std_items.txt"
> "$ALL_STD_ITEMS_FILE"

project_count=0
success_count=0

for project_dir in "$TOP100_DIR"/*; do
    if [ ! -d "$project_dir" ] || [[ "$project_dir" == *.txt ]]; then
        continue
    fi
    
    project_count=$((project_count + 1))
    project_name=$(basename "$project_dir")
    
    echo "[$project_count] Analyzing $project_name..."
    
    # Find source directories (try common patterns)
    src_dirs=()
    for dir in "$project_dir/src" "$project_dir"/*/src; do
        if [ -d "$dir" ]; then
            src_dirs+=("$dir")
        fi
    done
    
    if [ ${#src_dirs[@]} -eq 0 ]; then
        echo "  → No src/ directory found, skipping"
        continue
    fi
    
    # Run analysis and save output
    output_file="$OUTPUT_DIR/${project_name}.txt"
    
    if "$RUSTICATE_BIN" -d "${src_dirs[@]}" > "$output_file" 2>&1; then
        success_count=$((success_count + 1))
        echo "  ✓ Analysis complete"
        
        # Extract std items and add to global list
        grep "^  std::" "$output_file" | sed 's/^[[:space:]]*//' >> "$ALL_STD_ITEMS_FILE" || true
    else
        echo "  ✗ Analysis failed"
    fi
done

echo ""
echo "============================================================"
echo "Analysis Complete"
echo "============================================================"
echo "Projects analyzed: $project_count"
echo "Successful analyses: $success_count"
echo ""
echo "Individual project reports saved to: $OUTPUT_DIR/"
echo ""

# Generate global summary
echo "Generating global std usage summary..."

# Count unique std items across all projects
sort "$ALL_STD_ITEMS_FILE" | uniq -c | sort -rn > "$OUTPUT_DIR/global_std_usage_frequency.txt"

total_unique=$(sort "$ALL_STD_ITEMS_FILE" | uniq | wc -l)

echo ""
echo "Total unique std items across all projects: $total_unique"
echo "See detailed frequency report: $OUTPUT_DIR/global_std_usage_frequency.txt"
echo ""
echo "Top 20 most commonly used std items:"
head -20 "$OUTPUT_DIR/global_std_usage_frequency.txt"



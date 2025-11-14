#!/bin/bash
# Clone all top 100 Rust projects one at a time

set -e

GITHUB_LIST="/home/milnes/projects/Top100Rust/top100rustprojectsgithubs.txt"
TARGET_DIR="/home/milnes/projects/Top100Rust"

if [ ! -f "$GITHUB_LIST" ]; then
    echo "Error: GitHub list not found at $GITHUB_LIST"
    exit 1
fi

mkdir -p "$TARGET_DIR"

echo "Starting to clone repositories from $GITHUB_LIST"
echo "Target directory: $TARGET_DIR"
echo "====================================="

total=$(wc -l < "$GITHUB_LIST")
current=0

while IFS= read -r url; do
    # Skip empty lines
    [ -z "$url" ] && continue
    
    current=$((current + 1))
    
    # Extract repo name from URL (e.g., "dtolnay/syn")
    repo_name=$(echo "$url" | sed 's|https://github.com/||' | sed 's|/$||')
    
    # Create a clean directory name (replace / with _)
    dir_name=$(echo "$repo_name" | tr '/' '_')
    
    echo ""
    echo "[$current/$total] Cloning $repo_name..."
    
    if [ -d "$TARGET_DIR/$dir_name" ]; then
        echo "  → Already exists, skipping"
        continue
    fi
    
    cd "$TARGET_DIR"
    
    if git clone "$url" "$dir_name" 2>&1; then
        echo "  ✓ Successfully cloned to $dir_name"
    else
        echo "  ✗ Failed to clone $repo_name"
    fi
    
    # Small delay to avoid hammering GitHub
    sleep 1
done < "$GITHUB_LIST"

echo ""
echo "====================================="
echo "Clone process complete!"
echo "Repositories cloned to: $TARGET_DIR"

# Count how many were successfully cloned
cloned=$(find "$TARGET_DIR" -mindepth 1 -maxdepth 1 -type d | wc -l)
echo "Total repositories: $cloned"


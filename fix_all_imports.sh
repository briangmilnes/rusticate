#!/bin/bash
# Fix import order for all Rust files in APAS-AI

set -e

PROJECT_DIR="$1"
if [ -z "$PROJECT_DIR" ]; then
    echo "Usage: $0 <project-directory>"
    exit 1
fi

FIXED=0
FAILED=0

echo "Fixing import order in all Rust files..."
echo

# Find all .rs files in src/, tests/, benches/
for dir in src tests benches; do
    if [ -d "$PROJECT_DIR/$dir" ]; then
        find "$PROJECT_DIR/$dir" -name "*.rs" -type f | while read -r file; do
            echo -n "Fixing: $file ... "
            if ./target/release/rusticate-fix-import-order "$file" > /dev/null 2>&1; then
                echo "✓"
                FIXED=$((FIXED + 1))
            else
                echo "✗ (failed)"
                FAILED=$((FAILED + 1))
            fi
        done
    fi
done

echo
echo "Done! Fixed: $FIXED, Failed: $FAILED"


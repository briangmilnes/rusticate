#!/bin/bash
# Copyright (C) Brian G. Milnes 2025
#
# human_review.sh - Run all Rusticate review tools on APAS-AI

APAS_DIR="APAS-AI-copy/apas-ai"
COMMIT="584a672b6a34782766863c5f76a461d3297a741a"

echo "Checking out APAS to: $COMMIT"
(cd "$APAS_DIR" && git checkout "$COMMIT")
echo ""

echo "========================================"
echo "review-no-extern-crate"
echo "========================================"
./target/release/rusticate-review-no-extern-crate "$APAS_DIR"
echo ""

echo "========================================"
echo "review-module-encapsulation"
echo "========================================"
./target/release/rusticate-review-module-encapsulation "$APAS_DIR"
echo ""

echo "========================================"
echo "review-pascal-case-filenames"
echo "========================================"
./target/release/rusticate-review-pascal-case-filenames "$APAS_DIR"
echo ""

echo "========================================"
echo "review-snake-case-filenames"
echo "========================================"
./target/release/rusticate-review-snake-case-filenames "$APAS_DIR"
echo ""

echo "========================================"
echo "review-import-order"
echo "========================================"
./target/release/rusticate-review-import-order "$APAS_DIR"
echo ""

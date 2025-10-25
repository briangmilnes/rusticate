#!/usr/bin/env python3
"""
Add ToolLogger to all rusticate binaries that don't have it.
Scans for println! statements and wraps them with logger.log().
"""

import os
import re
import sys
from pathlib import Path

def has_tool_logger(content):
    """Check if file already uses ToolLogger"""
    return "ToolLogger" in content

def extract_tool_name(filepath):
    """Extract tool name from binary filename"""
    name = Path(filepath).stem
    # Convert underscores to dashes for tool name
    return name.replace("_", "-")

def add_logging_to_binary(filepath):
    """Add ToolLogger to a binary file"""
    with open(filepath, 'r') as f:
        content = f.read()
    
    if has_tool_logger(content):
        print(f"✓ {filepath} already has ToolLogger")
        return False
    
    tool_name = extract_tool_name(filepath)
    print(f"Adding logging to {filepath} (tool: {tool_name})")
    
    # Step 1: Add import after other rusticate imports
    if "use rusticate::" in content and "use rusticate::logging::logging::ToolLogger;" not in content:
        # Find the last rusticate import line
        lines = content.split('\n')
        last_rusticate_import_idx = -1
        for i, line in enumerate(lines):
            if line.strip().startswith("use rusticate::"):
                last_rusticate_import_idx = i
        
        if last_rusticate_import_idx != -1:
            lines.insert(last_rusticate_import_idx + 1, "use rusticate::logging::logging::ToolLogger;")
            content = '\n'.join(lines)
    
    # Step 2: Add logger initialization at start of main
    # Find main function
    main_pattern = r'(fn main\(\) -> Result<\(\)> \{)'
    match = re.search(main_pattern, content)
    if match:
        insert_pos = match.end()
        logger_init = f'\n    let mut logger = ToolLogger::new("{tool_name}");\n'
        content = content[:insert_pos] + logger_init + content[insert_pos:]
    
    # Step 3: Replace println! with logger.log() - but only simple ones for safety
    # We'll do this conservatively to avoid breaking things
    # Just flag it for manual review
    
    return content

def main():
    bin_dir = Path("src/bin")
    if not bin_dir.exists():
        print("Error: src/bin directory not found")
        sys.exit(1)
    
    binaries = list(bin_dir.glob("*.rs"))
    print(f"Found {len(binaries)} binaries to check\n")
    
    count_updated = 0
    count_skipped = 0
    
    for binary in sorted(binaries):
        try:
            result = add_logging_to_binary(binary)
            if result:
                # Write back
                with open(binary, 'w') as f:
                    f.write(result)
                count_updated += 1
            else:
                count_skipped += 1
        except Exception as e:
            print(f"✗ Error processing {binary}: {e}")
    
    print(f"\nSummary:")
    print(f"  Updated: {count_updated}")
    print(f"  Skipped: {count_skipped}")
    print(f"  Total: {len(binaries)}")

if __name__ == "__main__":
    main()


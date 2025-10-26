#!/usr/bin/env python3
"""
Fix imports for logging pattern - add std::fs if not present
"""

import re
from pathlib import Path

BIN_DIR = Path("/home/milnes/projects/rusticate/src/bin")

def has_fs_import(content):
    """Check if file has std::fs import"""
    return re.search(r'use\s+std::fs\s*;', content) is not None

def add_fs_import(content):
    """Add std::fs import after other use statements"""
    # Find the last use statement
    use_statements = list(re.finditer(r'^use\s+.*?;', content, re.MULTILINE))
    
    if not use_statements:
        # No use statements, add after the first line (copyright/comment)
        lines = content.split('\n')
        if lines:
            return '\n'.join([lines[0], 'use std::fs;'] + lines[1:])
        return 'use std::fs;\n' + content
    
    # Insert after the last use statement
    last_use = use_statements[-1]
    insert_pos = last_use.end()
    return content[:insert_pos] + '\nuse std::fs;' + content[insert_pos:]

def has_logging(content):
    """Check if file has logging setup"""
    return '_log_file' in content

def main():
    fixed = []
    
    for rs_file in sorted(BIN_DIR.glob("*.rs")):
        content = rs_file.read_text()
        
        # Only process files with logging
        if not has_logging(content):
            continue
        
        # Check if fs import is needed
        if not has_fs_import(content):
            new_content = add_fs_import(content)
            rs_file.write_text(new_content)
            fixed.append(rs_file.name)
            print(f"✓ Added fs import to {rs_file.name}")
    
    print(f"\nFixed {len(fixed)} files")

if __name__ == '__main__':
    main()


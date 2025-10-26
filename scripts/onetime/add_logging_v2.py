#!/usr/bin/env python3
"""
Add standard logging pattern (stdout + analyses/PROGRAM.log) to all rusticate binaries.

This version:
1. Adds logging setup after fn main()
2. Does NOT automatically replace println! (user code keeps using println!)
3. The log! macro is available for manual use if needed
"""

import os
import re
from pathlib import Path

BIN_DIR = Path("/home/milnes/projects/rusticate/src/bin")

def has_logging(content):
    """Check if file already has proper logging setup"""
    return ('_log_file' in content and 'macro_rules! log' in content) or \
           'Logger::new' in content or \
           'ToolLogger::new' in content

def has_fs_import(content):
    """Check if file has std::fs import"""
    return re.search(r'use\s+std::fs', content) is not None

def get_program_name(filepath):
    """Extract program name from filename (kebab-case)"""
    name = filepath.stem
    return name.replace('_', '-')

def add_logging_to_binary(filepath):
    """Add logging pattern to a binary file"""
    content = filepath.read_text()
    
    # Skip if already has logging
    if has_logging(content):
        return False, "already has logging"
    
    # Skip if no fn main
    if 'fn main' not in content:
        return False, "no fn main"
    
    program_name = get_program_name(filepath)
    
    # Add fs import if needed
    if not has_fs_import(content):
        # Find the last use statement
        use_statements = list(re.finditer(r'^use\s+.*?;', content, re.MULTILINE))
        if use_statements:
            last_use = use_statements[-1]
            insert_pos = last_use.end()
            content = content[:insert_pos] + '\nuse std::fs;' + content[insert_pos:]
    
    # Build the logging code to insert
    logging_code = f'''    let _ = fs::create_dir_all("analyses");
    let mut _log_file = fs::File::create("analyses/{program_name}.log").ok();

    #[allow(unused_macros)]
    macro_rules! log {{
        ($($arg:tt)*) => {{
            let msg = format!($($arg)*);
            println!("{{}}", msg);
            if let Some(ref mut f) = _log_file {{
                use std::io::Write;
                let _ = writeln!(f, "{{}}", msg);
            }}
        }};
    }}

'''
    
    # Find fn main and insert logging after the opening brace
    pattern = r'(fn\s+main\s*\([^)]*\)\s*(?:->\s*[^{]+)?\s*\{\s*\n)'
    match = re.search(pattern, content)
    
    if not match:
        return False, "couldn't find fn main pattern"
    
    # Insert logging code after fn main() {
    new_content = content[:match.end()] + logging_code + content[match.end():]
    
    filepath.write_text(new_content)
    return True, "added logging"

def main():
    if not BIN_DIR.exists():
        print(f"Error: {BIN_DIR} not found")
        return
    
    added = []
    skipped = []
    
    for rs_file in sorted(BIN_DIR.glob("*.rs")):
        success, reason = add_logging_to_binary(rs_file)
        if success:
            added.append(rs_file.name)
            print(f"✓ {rs_file.name}")
        else:
            skipped.append((rs_file.name, reason))
    
    print(f"\n{'='*80}")
    print(f"Added logging to {len(added)} files")
    print(f"Skipped {len(skipped)} files")
    
    if added:
        print(f"\n{'='*80}")
        print("Note: The log! macro is now available but println! calls were NOT automatically")
        print("replaced. You can manually convert important println! calls to log! calls.")
        print(f"\n{'='*80}")
        print("Modified files:")
        for name in added[:20]:  # Show first 20
            print(f"  - {name}")
        if len(added) > 20:
            print(f"  ... and {len(added) - 20} more")

if __name__ == '__main__':
    main()


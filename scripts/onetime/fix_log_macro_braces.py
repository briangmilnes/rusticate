#!/usr/bin/env python3
"""
Fix the log! macro to use correct number of braces (should be 2, not 4)
"""

import re
from pathlib import Path

BIN_DIR = Path("/home/milnes/projects/rusticate/src/bin")

BAD_MACRO = r'''    macro_rules! log \{
        \(\$\(\$arg:tt\)\*\) => \{\{\{\{
            let msg = format!\(\$\(\$arg\)\*\);
            println!\("\{\}", msg\);
            if let Some\(ref mut f\) = _log_file \{
                use std::io::Write;
                let _ = writeln!\(f, "\{\}", msg\);
            \}
        \}\}\}\};
    \}'''

GOOD_MACRO = '''    macro_rules! log {
        ($($arg:tt)*) => {{
            let msg = format!($($arg)*);
            println!("{}", msg);
            if let Some(ref mut f) = _log_file {
                use std::io::Write;
                let _ = writeln!(f, "{}", msg);
            }
        }};
    }'''

def fix_macro(content):
    """Fix the log! macro braces"""
    # Use a simpler approach - just find and replace the pattern
    # The script generated {{{{ but it should be {{
    content = re.sub(
        r'\(\$\(\$arg:tt\)\*\) => \{\{\{\{',
        r'($($arg:tt)*) => {{',
        content
    )
    content = re.sub(
        r'        \}\}\}\};',
        r'        }};',
        content
    )
    return content

def main():
    fixed = []
    
    for rs_file in sorted(BIN_DIR.glob("*.rs")):
        content = rs_file.read_text()
        
        if '{{{{' in content:
            new_content = fix_macro(content)
            rs_file.write_text(new_content)
            fixed.append(rs_file.name)
            print(f"✓ Fixed macro in {rs_file.name}")
    
    print(f"\nFixed {len(fixed)} files")

if __name__ == '__main__':
    main()


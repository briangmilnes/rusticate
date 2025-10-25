#!/usr/bin/env python3
"""
Add comprehensive ToolLogger to all rusticate binaries.
"""

import re
import sys
from pathlib import Path

def has_tool_logger(content):
    """Check if file already uses ToolLogger"""
    return "ToolLogger" in content

def get_tool_name(filepath):
    """Extract tool name from binary filename"""
    name = Path(filepath).stem
    return name.replace("_", "-")

def add_logging(filepath):
    """Add ToolLogger to a binary file"""
    with open(filepath, 'r') as f:
        content = f.read()
    
    if has_tool_logger(content):
        return None  # Already has logging
    
    tool_name = get_tool_name(filepath)
    lines = content.split('\n')
    
    # Step 1: Add import after rusticate imports
    import_added = False
    for i, line in enumerate(lines):
        if line.strip().startswith("use rusticate::") and not import_added:
            # Find the last rusticate import
            j = i
            while j + 1 < len(lines) and lines[j + 1].strip().startswith("use rusticate::"):
                j += 1
            # Insert after the last rusticate import
            lines.insert(j + 1, "use rusticate::logging::logging::ToolLogger;")
            import_added = True
            break
    
    # If no rusticate import found, add after all use statements
    if not import_added:
        for i, line in enumerate(lines):
            if line.strip().startswith("use ") and not lines[i].strip().startswith("use std::"):
                lines.insert(i + 1, "use rusticate::logging::logging::ToolLogger;")
                import_added = True
                break
    
    # Step 2: Add logger initialization at start of main
    content = '\n'.join(lines)
    main_pattern = r'(fn main\(\) -> Result<\(\)> \{\n)'
    if re.search(main_pattern, content):
        content = re.sub(
            main_pattern,
            r'\1    let mut logger = ToolLogger::new("' + tool_name + r'");\n\n',
            content,
            count=1
        )
    
    # Step 3: Convert println! to logger.log() for output lines
    # We'll do this carefully - only for lines that look like output
    lines = content.split('\n')
    output_lines = []
    in_main = False
    
    for line in lines:
        if 'fn main() -> Result<()>' in line:
            in_main = True
        
        # Convert println! in main function
        if in_main and '    println!' in line:
            # Extract the content
            match = re.match(r'(\s*)println!\((.*)\);', line)
            if match:
                indent = match.group(1)
                args = match.group(2)
                if args.strip() == '""' or args.strip() == '':
                    # Empty line
                    line = f'{indent}logger.log("");'
                else:
                    # Has content
                    line = f'{indent}logger.log(&format!({args}));'
        
        output_lines.append(line)
    
    return '\n'.join(output_lines)

def main():
    bin_dir = Path("src/bin")
    if not bin_dir.exists():
        print("Error: src/bin directory not found", file=sys.stderr)
        print("Please run from rusticate project root", file=sys.stderr)
        sys.exit(1)
    
    # Get all binaries
    binaries = sorted(bin_dir.glob("*.rs"))
    
    # Filter to only review/fix/analyze tools
    target_binaries = []
    for binary in binaries:
        name = binary.stem
        if any(keyword in name for keyword in ['review', 'fix', 'analyze', 'count']):
            target_binaries.append(binary)
    
    print(f"Found {len(target_binaries)} target binaries to add logging to\n")
    
    count_updated = 0
    count_skipped = 0
    count_error = 0
    
    for binary in target_binaries:
        try:
            result = add_logging(binary)
            if result is None:
                print(f"✓ {binary.name} - already has logging")
                count_skipped += 1
            else:
                with open(binary, 'w') as f:
                    f.write(result)
                print(f"+ {binary.name} - added logging")
                count_updated += 1
        except Exception as e:
            print(f"✗ {binary.name} - error: {e}")
            count_error += 1
    
    print(f"\nSummary:")
    print(f"  Added logging: {count_updated}")
    print(f"  Already had logging: {count_skipped}")
    print(f"  Errors: {count_error}")
    print(f"  Total: {len(target_binaries)}")

if __name__ == "__main__":
    main()


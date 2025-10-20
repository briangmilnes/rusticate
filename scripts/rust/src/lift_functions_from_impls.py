#!/usr/bin/env python3
"""
Phase 1: Lift pure functions (no self parameter) from inherent impls to module level.
Leave methods (&self, &mut self) in the impl block for later handling.
"""
import re
import sys
from pathlib import Path

def find_impl_block(lines, start_line):
    """Find the complete impl block starting at start_line (0-indexed)."""
    brace_count = 0
    found_opening = False
    
    for i in range(start_line, len(lines)):
        line = lines[i]
        brace_count += line.count('{') - line.count('}')
        
        if '{' in line and not found_opening:
            found_opening = True
        
        if found_opening and brace_count == 0:
            return start_line, i
    
    return start_line, start_line

def parse_function_or_method(lines, start_idx):
    """
    Parse a function/method starting at start_idx.
    Returns (end_idx, is_function, function_lines) or None if not a function.
    is_function = True if no self parameter (pure function)
    """
    # Collect comments before function
    comments = []
    i = start_idx
    while i > 0 and (lines[i-1].strip().startswith('///') or 
                     lines[i-1].strip().startswith('//') or
                     lines[i-1].strip().startswith('/*')):
        i -= 1
    
    if i < start_idx:
        comments = lines[i:start_idx]
    
    # Parse signature (might span multiple lines until opening brace)
    sig_lines = []
    i = start_idx
    while i < len(lines):
        sig_lines.append(lines[i])
        if '{' in lines[i]:
            break
        i += 1
    
    signature = ''.join(sig_lines)
    
    # Check if it's a pure function (no self)
    has_self = '&self' in signature or '&mut self' in signature or 'mut self' in signature or 'self:' in signature
    is_function = not has_self
    
    # Find closing brace of function body
    brace_count = 0
    body_end = i
    for j in range(i, len(lines)):
        brace_count += lines[j].count('{') - lines[j].count('}')
        if brace_count == 0:
            body_end = j
            break
    
    # Collect all lines (comments + signature + body)
    all_lines = lines[start_idx - len(comments):body_end + 1]
    
    return body_end, is_function, all_lines

def adjust_indentation(lines, from_indent=8, to_indent=4):
    """Adjust indentation of code lines."""
    adjusted = []
    indent_diff = from_indent - to_indent
    
    for line in lines:
        if line.startswith(' ' * from_indent):
            # Remove indent_diff spaces
            adjusted.append(line[indent_diff:])
        else:
            # Keep as-is (blank lines, comments, etc.)
            adjusted.append(line)
    
    return adjusted

def process_file(filepath, impl_line_num):
    """Process a single file to lift functions from one inherent impl."""
    path = Path(filepath)
    
    if not path.exists():
        print(f"  ERROR: File not found: {filepath}")
        return False
    
    content = path.read_text()
    lines = content.split('\n')
    
    # Find the impl block near the reported line (search +/- 5 lines)
    impl_start_line = None
    for offset in range(-5, 6):
        check_line = impl_line_num - 1 + offset
        if 0 <= check_line < len(lines) and re.match(r'^\s*impl', lines[check_line]):
            impl_start_line = check_line
            break
    
    if impl_start_line is None:
        print(f"  ERROR: Could not find impl block near line {impl_line_num}")
        return False
    
    impl_start, impl_end = find_impl_block(lines, impl_start_line)
    
    if impl_start == impl_end:
        print(f"  ERROR: Could not find impl block closing brace")
        return False
    
    impl_header = lines[impl_start]
    print(f"  Impl: {impl_header.strip()}")
    
    # Parse all functions/methods in impl block
    functions_to_lift = []
    remaining_impl_content = [impl_header]
    
    i = impl_start + 1
    while i < impl_end:
        line = lines[i]
        
        # Check if this is a function/method definition
        if re.match(r'\s{8}(pub\s+)?fn\s+', line):
            end_idx, is_function, func_lines = parse_function_or_method(lines, i)
            
            if is_function:
                # Extract function name for reporting
                fn_match = re.search(r'fn\s+(\w+)', line)
                fn_name = fn_match.group(1) if fn_match else "unknown"
                print(f"    ✓ Lifting function: {fn_name}()")
                functions_to_lift.append(func_lines)
            else:
                # It's a method, keep in impl
                fn_match = re.search(r'fn\s+(\w+)', line)
                fn_name = fn_match.group(1) if fn_match else "unknown"
                print(f"    - Keeping method: {fn_name}()")
                remaining_impl_content.extend(lines[i:end_idx + 1])
            
            i = end_idx + 1
        else:
            # Not a function definition, keep in impl
            remaining_impl_content.append(line)
            i += 1
    
    # Add closing brace
    remaining_impl_content.append(lines[impl_end])
    
    if not functions_to_lift:
        print(f"  → No pure functions to lift (only methods)")
        return True  # Not an error, just nothing to do
    
    # Build new file content
    new_content_lines = []
    
    # Keep everything before impl
    new_content_lines.extend(lines[:impl_start])
    
    # Add lifted functions at module level (adjust indentation)
    for func_lines in functions_to_lift:
        adjusted = adjust_indentation(func_lines, from_indent=8, to_indent=4)
        new_content_lines.extend(adjusted)
        new_content_lines.append('')  # Blank line between functions
    
    # Add the remaining impl block (with only methods)
    # Check if impl is now empty (only header and closing brace)
    non_empty_impl = any(line.strip() and 
                        line.strip() != impl_header.strip() and 
                        line.strip() != '}'
                        for line in remaining_impl_content)
    
    if non_empty_impl:
        new_content_lines.extend(remaining_impl_content)
    else:
        print(f"  → Impl block now empty, removing it")
    
    # Keep everything after impl
    new_content_lines.extend(lines[impl_end + 1:])
    
    # Write back
    new_content = '\n'.join(new_content_lines)
    path.write_text(new_content)
    
    print(f"  ✓ Lifted {len(functions_to_lift)} functions to module level")
    return True

def main():
    if len(sys.argv) < 2:
        print("Usage: lift_functions_from_impls.py <analysis_file.txt>")
        print()
        print("Phase 1: Lifts pure functions (no self) from inherent impls.")
        print("Methods (with self) are left in the impl for Phase 2.")
        sys.exit(1)
    
    analysis_file = Path(sys.argv[1])
    
    if not analysis_file.exists():
        print(f"ERROR: Analysis file not found: {analysis_file}")
        sys.exit(1)
    
    # Parse analysis file
    content = analysis_file.read_text()
    lines = content.split('\n')
    
    to_process = []
    in_only_private = False
    
    for line in lines:
        if 'ONLY PRIVATE HELPERS' in line:
            in_only_private = True
            continue
        if 'MIXED' in line:
            in_only_private = False
            break
        
        if in_only_private and line.strip().startswith('src/'):
            match = re.match(r'(src/[^:]+):(\d+)', line.strip())
            if match:
                to_process.append({
                    'file': match.group(1),
                    'line': int(match.group(2))
                })
    
    print(f"Found {len(to_process)} inherent impl blocks to process")
    print("=" * 80)
    print()
    
    success = 0
    skipped = 0
    failed = 0
    
    for item in to_process:
        print(f"Processing {item['file']}:{item['line']}")
        
        try:
            result = process_file(item['file'], item['line'])
            if result:
                success += 1
            else:
                failed += 1
        except Exception as e:
            print(f"  ERROR: {e}")
            import traceback
            traceback.print_exc()
            failed += 1
        
        print()
    
    print("=" * 80)
    print(f"SUCCESS: {success} impl blocks processed")
    print(f"FAILED:  {failed} impl blocks")
    print()
    print("Next: Run Phase 2 to handle methods (with self) remaining in impls")

if __name__ == '__main__':
    main()


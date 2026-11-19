#!/usr/bin/env python3
"""Fix the missing closing braces in Phase 4b"""

with open("src/card_events.rs", "r") as f:
    lines = f.read().split('\n')

# Find the Phase 4b section
phase_4b_start = None
for i, line in enumerate(lines):
    if "// Phase 4b: After" in line:
        phase_4b_start = i
        break

if phase_4b_start is not None:
    # Find the Ok(()) after Phase 4b
    ok_idx = None
    for i in range(phase_4b_start, len(lines)):
        if lines[i].strip() == "Ok(())":
            ok_idx = i
            break
    
    if ok_idx is not None:
        # Count opening braces in Phase 4b section
        open_braces = 0
        close_braces = 0
        for i in range(phase_4b_start, ok_idx):
            line = lines[i]
            open_braces += line.count('{')
            close_braces += line.count('}')
        
        print(f"Open braces: {open_braces}, Close braces: {close_braces}, Diff: {open_braces - close_braces}")
        
        # We need (open_braces - close_braces) closing braces before Ok(())
        missing = open_braces - close_braces
        if missing > 0:
            # Insert missing closing braces before Ok(())
            indent = "    "  # 4 spaces for function body level
            for _ in range(missing):
                lines.insert(ok_idx, indent + "}")
                ok_idx += 1  # Adjust for inserted lines
            
            with open("src/card_events.rs", "w") as f:
                f.write('\n'.join(lines))
            
            print(f"Inserted {missing} closing braces!")
        else:
            print("No missing braces")
    else:
        print("Could not find Ok(())")
else:
    print("Could not find Phase 4b section")
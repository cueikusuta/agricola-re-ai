#!/usr/bin/env python3
"""Fix indentation and missing braces in card_events.rs"""

with open("src/card_events.rs", "r") as f:
    lines = f.read().split('\n')

# Find the Phase 4b section and fix indentation
# Look for "// Phase 4b: After" comment
phase_4b_start = None
for i, line in enumerate(lines):
    if "// Phase 4b: After" in line:
        phase_4b_start = i
        break

if phase_4b_start is not None:
    # Find the end of Phase 4b - it should end before "Ok(())"
    phase_4b_end = None
    for i in range(phase_4b_start, len(lines)):
        if lines[i].strip() == "Ok(())" and i > phase_4b_start:
            phase_4b_end = i
            break
    
    if phase_4b_end is not None:
        # Fix indentation for Phase 4b section (should be 4 spaces, not 8)
        for i in range(phase_4b_start, phase_4b_end):
            if lines[i].startswith("        "):  # 8 spaces
                lines[i] = "    " + lines[i][8:]  # Reduce to 4 spaces
        
        # Check if there's a missing closing brace before Ok(())
        # The Phase 4b block should end with a } before Ok(())
        # Check the line before Ok(())
        if phase_4b_end > 0 and lines[phase_4b_end - 1].strip() != "}":
            # Insert closing brace before Ok(())
            lines.insert(phase_4b_end, "    }")
            print("Inserted missing closing brace before Ok(())")
        
        with open("src/card_events.rs", "w") as f:
            f.write('\n'.join(lines))
        
        print("Fixed indentation and braces!")
    else:
        print("Could not find Ok(()) after Phase 4b")
else:
    print("Could not find Phase 4b section")
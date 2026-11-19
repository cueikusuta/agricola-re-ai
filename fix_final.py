#!/usr/bin/env python3
"""Final fix - insert Phase 4b inside dispatch_play and add dispatch_after_worker_any_player after"""

with open("src/card_events.rs.pass1", "r") as f:
    lines = f.read().split('\n')

# Find the markers
phase_4b_start = None
after_fn_start = None

for i, line in enumerate(lines):
    if "### PHASE_4B_CODE ###" in line:
        phase_4b_start = i
    elif "### AFTER_DISPATCH_PLAY_FN ###" in line:
        after_fn_start = i

print(f"Phase 4b marker at: {phase_4b_start}")
print(f"After fn marker at: {after_fn_start}")

# Extract the sections
phase_4b_code = lines[phase_4b_start + 1:after_fn_start]
after_fn_code = lines[after_fn_start + 1:]

print(f"Phase 4b lines: {len(phase_4b_code)}")
print(f"After fn lines: {len(after_fn_code)}")

# Remove markers from main lines
main_lines = lines[:phase_4b_start] + lines[after_fn_start + 1:]

# Now find where to insert Phase 4b - inside dispatch_play before Ok(())
# The dispatch_play function should end with Ok(()) then }
# We need to find the Ok(()) that closes dispatch_play

# Look for the Ok(()) followed by } that ends dispatch_play
# Since dispatch_play returns Result<(), String>, its last line should be Ok(())

insert_idx = None
for i, line in enumerate(main_lines):
    if line.strip() == "Ok(())" and i + 1 < len(main_lines) and main_lines[i + 1].strip() == "}":
        # Check if this is the end of dispatch_play (not a nested function)
        # dispatch_play is a top-level function, so the } should be at indent 0 or 4
        if main_lines[i + 1].strip() == "}":
            # This is likely the end of dispatch_play
            insert_idx = i
            break

print(f"Insert index: {insert_idx}")
print(f"Line at insert_idx: {main_lines[insert_idx] if insert_idx is not None else 'N/A'}")

if insert_idx is not None:
    # Insert Phase 4b code before Ok(())
    # Add proper indentation (4 spaces since we're inside the function)
    indented_phase_4b = ["    " + line if line.strip() else line for line in phase_4b_code]
    final_lines = main_lines[:insert_idx] + [""] + indented_phase_4b + [""] + main_lines[insert_idx:]
else:
    final_lines = main_lines

# Now find where to insert the after_fn_code - after dispatch_play function
# dispatch_play ends with }, then we add the new function
after_dispatch_idx = None
for i, line in enumerate(final_lines):
    if i > 0 and line.strip() == "}" and final_lines[i-1].strip() == "Ok(())":
        # This is the end of dispatch_play
        after_dispatch_idx = i + 1
        break

print(f"After dispatch index: {after_dispatch_idx}")

if after_dispatch_idx is not None:
    final_lines = final_lines[:after_dispatch_idx] + [""] + after_fn_code + [""] + final_lines[after_dispatch_idx:]

# Write final result
with open("src/card_events.rs.fixed", "w") as f:
    f.write('\n'.join(final_lines))

print(f"Final lines: {len(final_lines)}")
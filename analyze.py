import re

with open('src/card_events.rs', 'r') as f:
    content = f.read()

# The problematic section is from the start of dispatch_after_worker_any_player function
# to the start of dispatch_after_worker function

# Find the exact boundaries
fn_start = content.find('fn dispatch_after_worker_any_player(')
fn_end = content.find('pub fn dispatch_after_worker(')

print(f"Function starts at: {fn_start}")
print(f"Next function starts at: {fn_end}")

# Extract the section
section = content[fn_start:fn_end]
print("=== SECTION ===")
print(repr(section[:500]))
print("...")
print(repr(section[-500:]))

# Now let's write the correct version
# The function should be at module level (0 indent for signature, 4 spaces for body)
# But currently everything is at 24 spaces indent

# Let's find where the function signature actually starts (with the fn keyword at module level)
# The function at 76969 seems to be the second occurrence (the first is at 4775)
# Let's check the first one
first_fn = content.find('fn dispatch_after_worker_any_player(')
print(f"First occurrence at: {first_fn}")
print(repr(content[first_fn:first_fn+200]))
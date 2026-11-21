import re

# Read the file
with open(r"C:\Users\cue\AppData\Local\hermes\kanban\boards\game-dev\workspaces\t_b1b2eda9\src\card_events.rs", "r") as f:
    content = f.read()

# Find all conflict markers
conflicts = []
for m in re.finditer(r'(<<<<<<< HEAD\n|=======\n|>>>>>>> origin/main\n)', content):
    conflicts.append((m.start(), m.group()))

print(f"Total conflict markers: {len(conflicts)}")
for i, (pos, marker) in enumerate(conflicts):
    print(f"  {i}: pos={pos}, marker={marker.strip()}")

# The strategy: remove conflict markers and keep both sides
# For test conflicts, we want to keep both HEAD and main tests

# Let's find the test module boundaries
test_section_start = content.find("// --- A118/A145/C066 dispatch tests (PR #385 review) ---")
if test_section_start == -1:
    test_section_start = content.find("/// C083 若牛: on play gain 2 cattle")
print(f"Test section starts at: {test_section_start}")

# Find the mod tests start
mod_tests_start = content.rfind("mod tests\n{\n", 0, test_section_start)
print(f"mod tests starts at: {mod_tests_start}")

# Find the closing brace of the test module
brace_count = 0
mod_end = mod_tests_start
for i, ch in enumerate(content[mod_tests_start:]):
    if ch == '{':
        brace_count += 1
    elif ch == '}':
        brace_count -= 1
        if brace_count == 0:
            mod_end = mod_tests_start + i + 1
            break

print(f"mod tests ends at: {mod_end}")
print(f"Module length: {mod_end - mod_tests_start}")

# Extract the test module
test_module = content[mod_tests_start:mod_end]

# Now let's parse the test module and remove conflict markers while keeping both sides
# We'll do a simple state machine
lines = test_module.split('\n')
output_lines = []
state = 'normal'  # normal, head, main
head_buffer = []
main_buffer = []

for line in lines:
    if line == '<<<<<<< HEAD':
        state = 'head'
        head_buffer = []
    elif line == '=======' and state == 'head':
        state = 'main'
        main_buffer = []
    elif line == '>>>>>>> origin/main' and state == 'main':
        # Output both buffers
        output_lines.extend(head_buffer)
        output_lines.extend(main_buffer)
        state = 'normal'
        head_buffer = []
        main_buffer = []
    else:
        if state == 'head':
            head_buffer.append(line)
        elif state == 'main':
            main_buffer.append(line)
        else:
            output_lines.append(line)

# Check if there are any remaining buffers
if head_buffer or main_buffer:
    print("WARNING: Unclosed conflict markers!")
    output_lines.extend(head_buffer)
    output_lines.extend(main_buffer)

merged_test_module = '\n'.join(output_lines)

# Replace the test module in the content
new_content = content[:mod_tests_start] + merged_test_module + content[mod_end:]

# Count remaining conflict markers
remaining = len(re.findall(r'(<<<<<<< HEAD|=======|>>>>>>> origin/main)', new_content))
print(f"Remaining conflict markers: {remaining}")

# Write the result
with open(r"C:\Users\cue\AppData\Local\hermes\kanban\boards\game-dev\workspaces\t_b1b2eda9\src\card_events.rs", "w") as f:
    f.write(new_content)

print("Done!")
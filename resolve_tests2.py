import re

# Read the file
with open(r"C:\Users\cue\AppData\Local\hermes\kanban\boards\game-dev\workspaces\t_b1b2eda9\src\card_events.rs", "r") as f:
    content = f.read()

# Find all conflict markers
conflicts = []
for m in re.finditer(r'(<<<<<<< HEAD\n|=======\n|>>>>>>> origin/main\n)', content):
    conflicts.append((m.start(), m.group()))

print(f"Total conflict markers: {len(conflicts)}")

# Let's process the file line by line with a proper state machine
lines = content.split('\n')
output_lines = []
state = 'normal'  # normal, head, main
head_buffer = []
main_buffer = []
conflict_count = 0

for i, line in enumerate(lines):
    stripped = line.rstrip()
    
    if stripped == '<<<<<<< HEAD':
        if state != 'normal':
            print(f"WARNING: Nested HEAD at line {i+1}, state={state}")
        state = 'head'
        head_buffer = []
        conflict_count += 1
    elif stripped == '=======' and state == 'head':
        state = 'main'
        main_buffer = []
    elif stripped == '>>>>>>> origin/main' and state == 'main':
        # Output both buffers
        output_lines.extend(head_buffer)
        output_lines.extend([''])  # blank line between test groups
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
    print(f"WARNING: Unclosed conflict markers at end! state={state}")
    output_lines.extend(head_buffer)
    output_lines.extend(main_buffer)

merged_content = '\n'.join(output_lines)

# Count remaining conflict markers
remaining = len(re.findall(r'(<<<<<<< HEAD|=======|>>>>>>> origin/main)', merged_content))
print(f"Remaining conflict markers: {remaining}")
print(f"Conflicts processed: {conflict_count}")

# Write the result
with open(r"C:\Users\cue\AppData\Local\hermes\kanban\boards\game-dev\workspaces\t_b1b2eda9\src\card_events.rs", "w") as f:
    f.write(merged_content)

print("Done!")
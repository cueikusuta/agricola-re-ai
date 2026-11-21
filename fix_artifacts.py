import re

# Read the current file
with open('src/card_events.rs', 'r') as f:
    current = f.read()

# Find all mod tests occurrences
mod_positions = [m.start() for m in re.finditer(r'mod tests\s*\{', current)]
print(f'mod tests positions: {mod_positions}')

# Find all }mod tests { artifacts
artifact_positions = [m.start() for m in re.finditer(r'\}mod tests\s*\{', current)]
print(f'artifact positions: {artifact_positions}')

# The correct test module starts at the first mod_tests (3143)
# We need to find where it ends properly, then remove everything after that
# until the end of the file (since the rest is garbage)

mod_start = mod_positions[0]

# Find the proper end of the first test module
brace_count = 0
mod_end = mod_start
for i, ch in enumerate(current[mod_start:]):
    if ch == '{':
        brace_count += 1
    elif ch == '}':
        brace_count -= 1
        if brace_count == 0:
            mod_end = mod_start + i + 1
            break

print(f'First mod_start: {mod_start}')
print(f'First mod_end: {mod_end}')
print(f'First module length: {mod_end - mod_start}')

# Keep everything up to mod_end, discard the rest
new_content = current[:mod_end]

with open('src/card_events.rs', 'w') as f:
    f.write(new_content)

print('Fixed!')
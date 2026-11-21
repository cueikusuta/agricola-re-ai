import re

# Read the current corrupted file
with open('src/card_events.rs', 'r') as f:
    current = f.read()

# Find the test module boundaries in the current file
# Use the FIRST mod tests, not the last (which is the corrupted artifact)
mod_start = current.find('mod tests\n{\n')
if mod_start == -1:
    mod_start = current.find('mod tests {\n')
print(f'CURRENT mod_start: {mod_start}')

# Find the closing brace by counting from the first mod tests
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
print(f'CURRENT mod_end: {mod_end}')
print(f'CURRENT test module length: {mod_end - mod_start}')

# Read the merged test module from head_tests.rs and main_tests.rs
with open('head_tests.rs', 'r') as f:
    head_test = f.read()
with open('main_tests.rs', 'r') as f:
    main_test = f.read()

# The merged test module: HEAD prefix + HEAD A118/A145/C066 tests + MAIN C083/C147 tests
head_split = head_test.find('// --- A118/A145/C066 dispatch tests (PR #385 review) ---')
if head_split == -1:
    head_split = head_test.find('/// C083 若牛: on play gain 2 cattle')
print(f'HEAD split point: {head_split}')

main_split = main_test.find('/// C083 若牛: on play gain 2 cattle')
print(f'MAIN split point: {main_split}')

head_prefix = head_test[:head_split]
head_suffix = head_test[head_split:]  # A118/A145/C066 tests
main_suffix = main_test[main_split:]  # C083/C147 tests

merged_test = head_prefix + head_suffix + '\n\n' + main_suffix
print(f'Merged test module length: {len(merged_test)}')

# Replace in current file
new_content = current[:mod_start] + merged_test + current[mod_end:]

with open('src/card_events.rs', 'w') as f:
    f.write(new_content)

print('Replacement done!')
import re

# Read the current corrupted file
with open('src/card_events.rs', 'r') as f:
    current = f.read()

# Find the test module boundaries in the current file
mod_start = current.find('mod tests\n{\n')
if mod_start == -1:
    mod_start = current.find('mod tests {\n')
print(f'CURRENT mod_start: {mod_start}')

# The file is corrupted, so brace counting might not work. Let's find the last '}' in the file
# that closes the test module. The test module should end before the file ends.
# Let's search for the end of the file - the test module is the last module.

# Actually, let's find all 'mod tests' occurrences
for m in re.finditer(r'mod tests\s*\{', current):
    print(f'Found mod tests at: {m.start()}')

# Find the closing brace by counting from the last mod tests
last_mod_start = max([m.start() for m in re.finditer(r'mod tests\s*\{', current)])
print(f'Last mod tests at: {last_mod_start}')

brace_count = 0
mod_end = last_mod_start
for i, ch in enumerate(current[last_mod_start:]):
    if ch == '{':
        brace_count += 1
    elif ch == '}':
        brace_count -= 1
        if brace_count == 0:
            mod_end = last_mod_start + i + 1
            break
print(f'CURRENT mod_end: {mod_end}')
print(f'CURRENT test module length: {mod_end - last_mod_start}')

# Read the merged test module from head_tests.rs and main_tests.rs
with open('head_tests.rs', 'r') as f:
    head_test = f.read()
with open('main_tests.rs', 'r') as f:
    main_test = f.read()

# The merged test module should be: head_test up to the conflict point, then both sets of tests
# The conflict in HEAD starts with "// --- A118/A145/C066 dispatch tests (PR #385 review) ---"
# The conflict in MAIN starts with "/// C083 若牛: on play gain 2 cattle"

# Find the split point in HEAD
head_split = head_test.find('// --- A118/A145/C066 dispatch tests (PR #385 review) ---')
if head_split == -1:
    head_split = head_test.find('/// C083 若牛: on play gain 2 cattle')
print(f'HEAD split point: {head_split}')

# Find the split point in MAIN (skip the first test which is C083)
main_split = main_test.find('/// C083 若牛: on play gain 2 cattle')
print(f'MAIN split point: {main_split}')

# The merged module = HEAD up to split + HEAD tests from split + MAIN tests from split
# But we need to be careful - HEAD already has A118/A145/C066 tests, MAIN has C083/C147 tests
# We want both

# Actually, the simplest approach: HEAD's test module up to the A118 tests, then add MAIN's tests after
head_prefix = head_test[:head_split]
head_suffix = head_test[head_split:]  # This includes A118/A145/C066 tests
main_suffix = main_test[main_split:]  # This includes C083/C147 tests

merged_test = head_prefix + head_suffix + '\n\n' + main_suffix

print(f'Merged test module length: {len(merged_test)}')

# Replace in current file
new_content = current[:last_mod_start] + merged_test + current[mod_end:]

with open('src/card_events.rs', 'w') as f:
    f.write(new_content)

print('Replacement done!')
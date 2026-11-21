import re

# Read HEAD version
with open('head_card_events.rs', 'r') as f:
    head = f.read()

# Read main version
with open('main_card_events.rs', 'r') as f:
    main = f.read()

# Extract test module from HEAD
mod_start = head.find('mod tests\n{\n')
if mod_start == -1:
    mod_start = head.find('mod tests {\n')
print(f'HEAD mod_start: {mod_start}')

brace_count = 0
mod_end = mod_start
for i, ch in enumerate(head[mod_start:]):
    if ch == '{':
        brace_count += 1
    elif ch == '}':
        brace_count -= 1
        if brace_count == 0:
            mod_end = mod_start + i + 1
            break
print(f'HEAD mod_end: {mod_end}')
head_test_module = head[mod_start:mod_end]
print(f'HEAD test module length: {len(head_test_module)}')

# Extract test module from main
mod_start_m = main.find('mod tests\n{\n')
if mod_start_m == -1:
    mod_start_m = main.find('mod tests {\n')
print(f'MAIN mod_start: {mod_start_m}')

brace_count = 0
mod_end_m = mod_start_m
for i, ch in enumerate(main[mod_start_m:]):
    if ch == '{':
        brace_count += 1
    elif ch == '}':
        brace_count -= 1
        if brace_count == 0:
            mod_end_m = mod_start_m + i + 1
            break
print(f'MAIN mod_end: {mod_end_m}')
main_test_module = main[mod_start_m:mod_end_m]
print(f'MAIN test module length: {len(main_test_module)}')

# Save for inspection
with open('head_tests.rs', 'w') as f:
    f.write(head_test_module)
with open('main_tests.rs', 'w') as f:
    f.write(main_test_module)
with open('src/card_events.rs', 'r') as f:
    content = f.read()

# The problem: Phase 4b is nested inside the C068 if block, but it should be after the occupation if block
# Let me find the exact structure and fix it

# Current broken structure (around line 74253):
# if new_card.kind == Occupation {
#     // C116 loop
#     // C068 loop
#     // Phase 4b (WRONG - nested inside C068)
#     Ok(())
# }

# Should be:
# if new_card.kind == Occupation {
#     // C116 loop
#     // C068 loop
# }
# // Phase 4b (for MinorImprovement)
# Ok(())

# Find the second "Ok(())" that closes the C068 if block and move Phase 4b after it
# The pattern is: C068 block ends with "}" then Phase 4b, then "Ok(())" then "}" (closes C068 if)
# But there are too many braces. Let me use a more surgical approach.

# Let's find the occupation check block and the Phase 4b
# The occupation check starts at "if let Some(new_card) = card_set.definition(new_card_id) {" (first occurrence)
# and ends with the matching closing brace
# Then Phase 4b should come after that

# Actually, looking at the output, there's a THIRD Phase 4b later:
# "    // Phase 4b: After —" at the end of the output

# Let me check how many Phase 4b sections exist
import re
matches = list(re.finditer(r'Phase 4b', content))
print(f"Found {len(matches)} Phase 4b occurrences")
for m in matches:
    print(f"  Position {m.start()}: {content[m.start():m.start()+100]}")

# Find the end of dispatch_play function
idx = content.find('fn dispatch_play')
brace_count = 0
in_function = False
for i in range(idx, len(content)):
    if content[i] == '{':
        brace_count += 1
        in_function = True
    elif content[i] == '}':
        brace_count -= 1
        if in_function and brace_count == 0:
            print(f"\ndispatch_play ends at {i}")
            # Show last 5000 chars
            print(content[i-5000:i+1])
            break
with open('src/card_events.rs', 'r') as f:
    content = f.read()

# The issue: 
# - dispatch_play ends at position 76163
# - There's a Phase 4b at 73716 (nested inside C068 block - WRONG)
# - There's a duplicate Phase 4b at 76235 (AFTER function ends - WRONG)
# 
# The correct structure should have ONE Phase 4b between the Occupation if block and the final Ok(())
# 
# Let me reconstruct the correct ending of dispatch_play

# Find the start of the Occupation if block
occ_start = content.find('if let Some(new_card) = card_set.definition(new_card_id) {')
# This is the first occurrence - the occupation check

# Find the end of the occupation block (matching brace)
# The occupation block starts with: if let Some(new_card) = card_set.definition(new_card_id) {
#                                    if new_card.kind == crate::cards::CardKind::Occupation {
# We need to find where this block ends

# Let me find the correct insertion point - after the occupation block but before Ok(())
# The function ends at 76163 with Ok(()) }

# Let me extract the function body and rebuild it correctly
fn_start = content.find('fn dispatch_play(')
fn_end = 76163  # from earlier analysis

# The function content
fn_content = content[fn_start:fn_end+1]

# Find the last "Ok(())" before the function end
last_ok = fn_content.rfind('Ok(())')
if last_ok == -1:
    last_ok = fn_content.rfind('Ok(())')
print(f"Last Ok(()) at offset {last_ok}")

# The correct structure: everything up to the occupation block end, then Phase 4b, then Ok(())

# Actually, let me just rewrite the end of the function properly
# The function should end with:
#     }
#     }
#     // Phase 4b...
#     if let Some(new_card) ... { ... }
#     Ok(())
# }

# Let me find the occupation if block end
# Search for the pattern: "        }\r\n    }\r\n    Ok(())" or similar

# Better approach: Find the line with "Ok(())" that's at the right indentation level
lines = fn_content.split('\n')
for i, line in enumerate(lines):
    if 'Ok(())' in line and i > len(lines) - 10:
        print(f"Line {i}: {line} (indent: {len(line) - len(line.lstrip())})")

# Also look for the occupation block closing
for i, line in enumerate(lines):
    if 'new_card.kind == crate::cards::CardKind::Occupation' in line:
        print(f"Occupation check at line {i}: {line}")
with open('src/card_events.rs', 'r') as f:
    content = f.read()

fn_start = content.find('fn dispatch_play(')
fn_end = 76163
fn_content = content[fn_start:fn_end+1]
lines = fn_content.split('\n')

# Print lines 80 to end to see the structure
for i in range(80, len(lines)):
    indent = len(lines[i]) - len(lines[i].lstrip())
    if indent <= 12:  # Only show less indented lines
        print(f"Line {i} (indent {indent}): {lines[i].strip()}")
    elif 'Phase 4b' in lines[i] or 'Ok(())' in lines[i] or 'if new_card.kind' in lines[i]:
        print(f"Line {i} (indent {indent}): {lines[i].strip()}")
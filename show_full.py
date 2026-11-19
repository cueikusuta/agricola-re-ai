with open('src/card_events.rs', 'r') as f:
    content = f.read()

fn_start = content.find('fn dispatch_play(')
fn_end = 76163
fn_content = content[fn_start:fn_end+1]
lines = fn_content.split('\n')

# Print lines 90-190 with full indentation
for i in range(90, 195):
    print(f"Line {i} (indent {len(lines[i]) - len(lines[i].lstrip())}): {lines[i]}")
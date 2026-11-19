with open('src/cards.rs', 'r') as f:
    content = f.read()

# Check the actual content around the conflict markers
lines = content.split('\n')
for i in [137, 138, 139, 237, 238, 239, 467, 468, 469]:
    if 0 <= i < len(lines):
        print(f"Line {i+1}: {repr(lines[i])}")
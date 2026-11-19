with open('src/cards.rs', 'r') as f:
    content = f.read()

# Check more context around each conflict
lines = content.split('\n')
for start in [130, 230, 460]:
    print(f"=== Lines {start+1}-{start+30} ===")
    for i in range(start, min(start+30, len(lines))):
        print(f"Line {i+1}: {lines[i]}")
    print()
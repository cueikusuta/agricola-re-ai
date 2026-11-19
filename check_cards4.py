with open('src/cards.rs', 'r') as f:
    content = f.read()

# Check more context around each conflict - full blocks
lines = content.split('\n')
for start in [130, 230, 460]:
    print(f"=== Lines {start+1}-{start+60} ===")
    for i in range(start, min(start+60, len(lines))):
        print(f"Line {i+1}: {lines[i]}")
    print()
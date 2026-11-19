with open('src/cards.rs', 'r') as f:
    content = f.read()

# Find all three conflict blocks
markers = []
for marker in ['<<<<<<< HEAD', '=======', '>>>>>>> origin/main']:
    idx = 0
    while True:
        idx = content.find(marker, idx)
        if idx == -1:
            break
        markers.append((idx, marker))
        idx += 1

markers.sort()
for pos, marker in markers:
    print(f"Position {pos}: {marker}")
    # Show context
    start = max(0, pos - 30)
    end = min(len(content), pos + 100)
    print(repr(content[start:end]))
    print()
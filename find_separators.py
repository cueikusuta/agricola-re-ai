with open('src/cards.rs', 'r') as f:
    content = f.read()

# Search for ======= and >>>>>>> 
for marker in ['=======', '>>>>>>>']:
    idx = 0
    while True:
        idx = content.find(marker, idx)
        if idx == -1:
            break
        print(f"Position {idx}: {marker}")
        start = max(0, idx - 30)
        end = min(len(content), idx + 100)
        print(repr(content[start:end]))
        print()
        idx += 1
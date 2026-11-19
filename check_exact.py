with open('src/cards.rs', 'r') as f:
    content = f.read()

# Check the exact bytes around the conflict markers
for marker in ['<<<<<<< HEAD', '=======', '>>>>>>> origin/main']:
    idx = content.find(marker)
    if idx >= 0:
        print(f"Found '{marker}' at {idx}:")
        print(repr(content[max(0,idx-50):idx+100]))
        print()
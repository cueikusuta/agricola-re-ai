with open('src/policy.rs', 'r') as f:
    content = f.read()

# Check the conflict context
lines = content.split('\n')
for i in range(590, 625):
    if i < len(lines):
        print(f"Line {i+1}: {lines[i]}")
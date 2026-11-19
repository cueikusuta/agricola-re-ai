with open('src/registry.rs', 'r') as f:
    content = f.read()

# Check the conflict contexts
lines = content.split('\n')
for i in range(300, 330):
    if i < len(lines):
        print(f"Line {i+1}: {lines[i]}")
print()
for i in range(415, 435):
    if i < len(lines):
        print(f"Line {i+1}: {lines[i]}")
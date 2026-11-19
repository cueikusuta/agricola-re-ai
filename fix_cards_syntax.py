with open('src/cards.rs', 'r') as f:
    content = f.read()

# Fix 1: Remove extra closing brace after the enum
content = content.replace('}}\n\nimpl CardEffect {\n    /// Stable public name', '}\n\nimpl CardEffect {\n    /// Stable public name')

# Fix 2: Check for any other duplicate impl blocks
if content.count('impl CardEffect {') > 1:
    print("WARNING: Multiple impl CardEffect blocks found!")

with open('src/cards.rs', 'w') as f:
    f.write(content)

print("Fixed cards.rs syntax")
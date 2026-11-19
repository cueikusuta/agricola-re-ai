with open('src/cards.rs', 'r') as f:
    content = f.read()

# Remove the extra closing brace - there are two `}\n` before the blank line
# Pattern: }\n}\n\n/// What to count
# Should be: }\n\n/// What to count
content = content.replace('        }\n    }\n}\n}\n\n/// What to count', '        }\n    }\n}\n\n/// What to count')

with open('src/cards.rs', 'w') as f:
    f.write(content)

print("Fixed extra brace")
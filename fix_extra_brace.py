with open('src/cards.rs', 'r') as f:
    content = f.read()

# Fix the extra closing brace after public_tag
# The pattern is: match arms... }\n    }\n}\n\n/// What to count
# Should be: match arms... }\n    }\n\n/// What to count
content = content.replace('        }\n    }\n}\n\n/// What to count', '        }\n    }\n\n/// What to count')

with open('src/cards.rs', 'w') as f:
    f.write(content)

print("Fixed extra brace")
with open('src/cards.rs', 'r') as f:
    content = f.read()

# Fix duplicate impl CardEffect
content = content.replace('}\n\nimpl CardEffect {\n    /// Stable public name for this implemented effect category. Used to build\n    /// the neutral draft-AI catalog (#104). It describes only effects the\n    /// engine actually implements, never a guessed printed effect.\n    impl CardEffect {\n    /// Stable public name', 
                   '}\n\nimpl CardEffect {\n    /// Stable public name')

# Also check CardState - it should be defined in cards.rs
if 'pub struct CardState' not in content:
    print("WARNING: CardState not found in cards.rs!")

with open('src/cards.rs', 'w') as f:
    f.write(content)

print("Fixed duplicate impl")
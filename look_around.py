import re

with open('src/card_events.rs', 'r') as f:
    content = f.read()

# Look at the area around dispatch_after_worker (position 82376)
start = 82000
end = 83000
print(repr(content[start:end]))
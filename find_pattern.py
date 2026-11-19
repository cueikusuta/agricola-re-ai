import re

with open('src/card_events.rs', 'r') as f:
    content = f.read()

# Find the pattern with flexible whitespace
pattern = r'Ok\(\)\s*\n\s*\}\s*\n\s*/// Dispatch AfterWorker'
matches = list(re.finditer(pattern, content))
for m in matches:
    start = max(0, m.start()-200)
    end = m.end()+200
    print(f'Found at {m.start()}:')
    print(repr(content[start:end]))
    print('---')

# Also try to find the exact boundary
pattern2 = r'dispatch_after_worker_any_player'
for m in re.finditer(pattern2, content):
    print(f'Function at {m.start()}')

pattern3 = r'pub fn dispatch_after_worker\('
for m in re.finditer(pattern3, content):
    print(f'dispatch_after_worker at {m.start()}')
    print(repr(content[m.start():m.start()+300]))
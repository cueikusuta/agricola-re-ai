import re

with open('src/card_events.rs', 'r') as f:
    content = f.read()

# Look at the area between dispatch_after_worker_any_player end and dispatch_after_worker start
# dispatch_after_worker_any_player at 76969, dispatch_after_worker at 82376
start = 76969
end = 82376
print(repr(content[start:end]))
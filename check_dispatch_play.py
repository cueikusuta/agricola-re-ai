with open('src/card_events.rs', 'r') as f:
    content = f.read()

# Find dispatch_play function
idx = content.find('fn dispatch_play')
if idx >= 0:
    brace_count = 0
    in_function = False
    for i in range(idx, len(content)):
        if content[i] == '{':
            brace_count += 1
            in_function = True
        elif content[i] == '}':
            brace_count -= 1
            if in_function and brace_count == 0:
                section = content[idx:i+1]
                print('=== dispatch_play (last 2000 chars) ===')
                print(section[-2000:])
                break
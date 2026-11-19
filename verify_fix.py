with open('src/card_events.rs', 'r') as f:
    content = f.read()

# Check the area around the two functions
fn_start = content.find('fn dispatch_after_worker_any_player(')
fn_end = content.find('pub fn dispatch_after_worker(')

print('=== dispatch_after_worker_any_player ===')
print(content[fn_start:fn_start+500])
print('...')
print(content[fn_end-200:fn_end+500])
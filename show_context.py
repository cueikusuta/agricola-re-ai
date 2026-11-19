with open('src/card_events.rs', 'r') as f:
    content = f.read()

# Let me see more context - from the start of the occupation check to the end
idx = content.find('if let Some(new_card) = card_set.definition(new_card_id) {')
if idx >= 0:
    # Find the first one (occupation check)
    # Then find the second one (Phase 4b)
    first = content.find('if let Some(new_card) = card_set.definition(new_card_id) {')
    second = content.find('if let Some(new_card) = card_set.definition(new_card_id) {', first + 1)
    print(f"First at {first}, second at {second}")
    
    # Show context around second
    print(content[second-2000:second+2000])
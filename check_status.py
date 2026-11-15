import json
import os

for deck in ['A', 'C', 'D', 'E']:
    for i in range(1, 70):
        card_id = f"{deck}{i:03d}"
        path = f"data/bga-card-specs/{card_id}.json"
        if os.path.exists(path):
            with open(path, 'r', encoding='utf-8') as f:
                data = json.load(f)
            review = data.get('review', {})
            status = review.get('status', 'unknown')
            if status in ('needs_resolution', 'inventory_only', 'partial'):
                name = data.get('identity', {}).get('name_ja', 'unknown')
                unresolved = review.get('unresolved_fields', [])
                print(f"{card_id}: {name} - status={status}, unresolved={unresolved}")
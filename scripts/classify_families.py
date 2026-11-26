#!/usr/bin/env python3
"""
Classify unimplemented cards by mechanism family.
Compares each card's BGA spec vocabulary against current Rust CardEffect variants.
Outputs family breakdown with foundation cards.
"""
import csv
import json
from pathlib import Path
from collections import defaultdict, Counter

LEDGER_PATH = Path("data/bga-card-base-ledger.csv")
SPECS_DIR = Path("data/bga-card-specs")

# Current Rust vocabulary (from family-based-impl.md and cards.rs)
RUST_TRIGGERS = {
    'on_play', 'after_worker_space', 'passive', 'end_scoring', 
    'round_start', 'harvest_field_phase', 'after_clay_construction', 
    'after_harvest', 'after_bake_bread'
}

RUST_OPERATIONS = {
    'gain_resource', 'gain_points', 'modify_capacity', 'discount_cost', 
    'spend_resource', 'exchange'
}

RUST_PREDICATES = {
    'worker_space_used', 'house_is', 'room_count', 'animal_count_at_least',
    'grain_field_count', 'fence_count_at_least_round', 'any_player', 
    'other_player_only', 'has_improvement', 'declined_improvement',
    'sow_unconditional', 'round_in', 'round_has_no_harvest', 
    'resource_at_least', 'resource_greater_than', 'accumulation_at_least'
}

# Passive rules (already extensible)
RUST_PASSIVE_RULES = {
    'pasture_capacity', 'fence_wood_discount', 'improvement_wood_discount',
    'room_costs', 'housing_space'
}

# Family type mapping for missing vocabulary
FAMILY_MAPPING = {
    # New triggers -> new CardEffect variant
    'after_sow': 'sowing_triggers',
    'after_build': 'building_triggers',
    'after_renovation': 'building_triggers',
    'after_fence': 'building_triggers',
    'round_end': 'round_end_triggers',
    'harvest_feeding': 'harvest_feeding_trigger',
    'when_buying': 'when_buying_discount',
    'after_action': 'after_action_triggers',
    
    # New operations -> new Operation kind
    'plow_field': 'plow_field_op',
    'build_stable': 'build_stable_op',
    'build_room': 'build_room_op',
    'build_fence': 'build_fence_op',
    'grant_action': 'grant_action_op',
    'choice': 'choice_kind',
    'stack_goods_on_card': 'stack_goods_on_card',
    'place_on_round_spaces': 'place_on_round_spaces',
    'sow': 'sow_op',
    'bake_bread': 'bake_bread_op',
    'cook': 'cook_op',
    'convert_resource': 'convert_resource_op',
    'take_from_supply': 'take_from_supply_op',
    'pay_resource': 'pay_resource_op',
    
    # New predicates
    'round_space_has_marker': 'round_space_predicates',
    'rooms_exceed_people': 'building_predicates',
    'pasture_size': 'pasture_predicates',
    'harvests_equal_occupations': 'harvest_occupation_predicates',
    'has_animal': 'animal_predicates',
    'field_count': 'field_predicates',
    'improvement_count': 'improvement_predicates',
    'occupation_count': 'occupation_predicates',
    'resource_exact': 'resource_exact_predicates',
    
    # Passive rules
    'pasture_capacity_bonus': 'passive_pasture',
    'building_discount': 'passive_building_discount',
}

def load_ledger():
    """Load ledger and return unimplemented cards."""
    with open(LEDGER_PATH, 'r', encoding='utf-8-sig') as f:
        reader = csv.DictReader(f)
        rows = list(reader)
    
    unimplemented = [r for r in rows if r['implementation_status'] != 'implemented']
    return unimplemented

def load_spec(card_id):
    """Load spec JSON for a card."""
    spec_path = SPECS_DIR / f"{card_id}.json"
    if not spec_path.exists():
        return None
    with open(spec_path, 'r', encoding='utf-8') as f:
        return json.load(f)

def extract_vocabulary(spec):
    """Extract triggers, operations, predicates from spec."""
    triggers = set()
    operations = set()
    predicates = set()
    passive_rules = set()
    
    effects = spec.get('effects') or []
    for effect in effects:
        trigger = effect.get('trigger', '')
        if trigger:
            triggers.add(trigger)
        
        for step in effect.get('steps', []):
            op = step.get('operation', '')
            if op:
                operations.add(op)
        
        for cond in effect.get('conditions', []):
            pred = cond.get('predicate', '')
            if pred:
                predicates.add(pred)
    
    # Check for passive rules in effect (passive trigger)
    for effect in effects:
        if effect.get('trigger') == 'passive':
            for step in effect.get('steps', []):
                op = step.get('operation', '')
                if op:
                    passive_rules.add(op)
    
    return {
        'triggers': triggers,
        'operations': operations,
        'predicates': predicates,
        'passive_rules': passive_rules
    }

def classify_card(card_id, spec):
    """Classify a card into families based on missing vocabulary."""
    vocab = extract_vocabulary(spec)
    
    missing_families = set()
    missing_items = {
        'triggers': [],
        'operations': [],
        'predicates': [],
        'passive_rules': []
    }
    
    # Check triggers
    for t in vocab['triggers']:
        if t not in RUST_TRIGGERS:
            missing_items['triggers'].append(t)
            family = FAMILY_MAPPING.get(t, f'new_trigger_{t}')
            missing_families.add(family)
    
    # Check operations
    for op in vocab['operations']:
        if op not in RUST_OPERATIONS:
            missing_items['operations'].append(op)
            family = FAMILY_MAPPING.get(op, f'new_operation_{op}')
            missing_families.add(family)
    
    # Check predicates
    for p in vocab['predicates']:
        if p not in RUST_PREDICATES:
            missing_items['predicates'].append(p)
            family = FAMILY_MAPPING.get(p, f'new_predicate_{p}')
            missing_families.add(family)
    
    # Check passive rules
    for pr in vocab['passive_rules']:
        if pr not in RUST_PASSIVE_RULES:
            missing_items['passive_rules'].append(pr)
            family = FAMILY_MAPPING.get(pr, f'new_passive_{pr}')
            missing_families.add(family)
    
    is_fully_covered = len(missing_families) == 0
    
    return {
        'card_id': card_id,
        'vocab': vocab,
        'missing_families': missing_families,
        'missing_items': missing_items,
        'is_fully_covered': is_fully_covered
    }

def main():
    print("Loading ledger...")
    unimplemented = load_ledger()
    print(f"Unimplemented cards: {len(unimplemented)}")
    
    print("Loading specs and classifying...")
    results = []
    for row in unimplemented:
        card_id = row['card_id']
        spec = load_spec(card_id)
        if spec is None:
            print(f"  WARNING: No spec for {card_id}")
            continue
        result = classify_card(card_id, spec)
        results.append(result)
    
    print(f"Classified {len(results)} cards")
    
    # Count by family
    family_counts = Counter()
    family_cards = defaultdict(list)
    
    fully_covered = []
    needs_new = []
    
    for r in results:
        if r['is_fully_covered']:
            fully_covered.append(r)
        else:
            needs_new.append(r)
            for fam in r['missing_families']:
                family_counts[fam] += 1
                family_cards[fam].append(r['card_id'])
    
    print(f"\nFully covered by existing vocabulary: {len(fully_covered)}")
    print(f"Need new mechanism(s): {len(needs_new)}")
    
    print("\n=== Family Breakdown ===")
    for fam, count in family_counts.most_common():
        print(f"  {fam}: {count} cards")
    
    # Find foundation cards for each family (lowest difficulty)
    # We'll estimate difficulty by card_id order within deck (A001=1, A168=168, etc.)
    # and by number of missing families
    print("\n=== Foundation Cards (first card in each family by ID) ===")
    for fam in sorted(family_counts.keys()):
        cards = family_cards[fam]
        # Sort by card_id (which encodes deck + number)
        cards_sorted = sorted(cards)
        foundation = cards_sorted[0]
        print(f"  {fam}: {foundation} (total {len(cards)} cards)")
    
    # Cards by number of missing families
    print("\n=== Cards by Missing Family Count ===")
    missing_count_dist = Counter(len(r['missing_families']) for r in needs_new)
    for cnt in sorted(missing_count_dist.keys()):
        print(f"  {cnt} missing families: {missing_count_dist[cnt]} cards")
    
    # Multi-family cards (complex cards)
    print("\n=== Complex Cards (3+ missing families) ===")
    complex_cards = [r for r in needs_new if len(r['missing_families']) >= 3]
    for r in sorted(complex_cards, key=lambda x: (-len(x['missing_families']), x['card_id']))[:20]:
        print(f"  {r['card_id']}: {sorted(r['missing_families'])}")
    
    # Save detailed results
    output = {
        'summary': {
            'total_unimplemented': len(unimplemented),
            'fully_covered': len(fully_covered),
            'needs_new_mechanism': len(needs_new),
            'family_count': len(family_counts),
            'foundation_cards': {fam: sorted(cards)[0] for fam, cards in family_cards.items()}
        },
        'families': {
            fam: {
                'count': count,
                'cards': sorted(cards),
                'foundation': sorted(cards)[0]
            }
            for fam, count in family_counts.items()
            for cards in [family_cards[fam]]
        },
        'fully_covered_cards': [r['card_id'] for r in fully_covered],
        'complex_cards': [
            {'card_id': r['card_id'], 'missing_families': sorted(r['missing_families'])}
            for r in complex_cards
        ]
    }
    
    with open('data/family_classification.json', 'w', encoding='utf-8') as f:
        json.dump(output, f, ensure_ascii=False, indent=2)
    
    print("\nResults saved to data/family_classification.json")

if __name__ == '__main__':
    main()
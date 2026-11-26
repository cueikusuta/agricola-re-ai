#!/usr/bin/env python3
"""
Re-classify 'new_operation_other' and 'new_passive_other' cards by primitive mechanisms.
Reads actual spec JSON and analyzes effects to identify primitive operations needed.
Uses the 'kind' field from 'other' operations to identify true primitives.
"""
import json
from pathlib import Path
from collections import defaultdict, Counter

SPECS_DIR = Path("data/bga-card-specs")

# Load the family classification to get target card IDs
with open('data/family_classification.json', 'r', encoding='utf-8') as f:
    fam_data = json.load(f)

op_other_cards = set(fam_data['families'].get('new_operation_other', {}).get('cards', []))
passive_other_cards = set(fam_data['families'].get('new_passive_other', {}).get('cards', []))

# Union of both sets (279 unique cards)
target_cards = op_other_cards | passive_other_cards
print(f"Target cards: {len(target_cards)} unique cards")

# Map 'other' kinds to higher-level primitive categories
KIND_TO_PRIMITIVE = {
    # Exchange / conversion
    'exchange': 'exchange',
    'gain_matching_improvement': 'exchange',
    'field_internal_exchange': 'exchange',
    'move_resource_pair_to_supply': 'exchange',
    'move_field_good_to_supply': 'exchange',
    'substitute_cost': 'exchange',
    'substitute_reed_with_wood': 'exchange',
    'ignore_cost': 'exchange',
    'buy_animal': 'exchange',
    'buy_stone': 'exchange',
    'pay_animal': 'exchange',
    'pay_for_resource': 'exchange',
    'take_from_other_accumulation': 'exchange',
    'move_person_to_unoccupied_space_and_act': 'exchange',
    'use_other_space_with_same_person': 'exchange',
    
    # Accumulation / stacking
    'stack_goods_on_card': 'accumulate',
    'stack_field_tiles_on_card': 'accumulate',
    'place_rewards_on_card': 'accumulate',
    'place_food_on_card': 'accumulate',
    'place_wood_from_supply_on_rooms': 'accumulate',
    'place_clay_on_unused_farmyard': 'accumulate',
    'place_sheep_on_future_rounds': 'accumulate',
    'place_food_on_future_rounds': 'accumulate',
    'place_on_round_spaces': 'accumulate',
    'place_field_on_round_space': 'accumulate',
    'place_field_tiles_on_card': 'accumulate',
    'move_all_goods_on_card_to_supply': 'accumulate',
    'move_card_animals_to_fields': 'accumulate',
    'move_card': 'accumulate',
    'take_top_good_from_card': 'accumulate',
    'use_resource_from_card': 'accumulate',
    'restock_food_on_card': 'accumulate',
    'take_from_accumulations': 'accumulate',
    'take_good_from_accumulation': 'accumulate',
    'take_clay_from_space': 'accumulate',
    'take_vegetable_from_one_field': 'accumulate',
    'take_field_from_round_space': 'accumulate',
    'take_all_goods_from_round_space': 'accumulate',
    
    # Conditional discount / cost modification
    'discount_cost': 'conditional_discount',
    'build_stable_alternate_cost': 'conditional_discount',
    'build_room_with_discount': 'conditional_discount',
    'allow_wood_to_stone_renovation': 'conditional_discount',
    'buy_major_via_minor_action': 'conditional_discount',
    'allow_major_via_minor': 'conditional_discount',
    'play_via_major_improvement_only': 'conditional_discount',
    'ignore_occupation': 'conditional_discount',
    'play_occupation_without_cost': 'conditional_discount',
    'play_occupation_from_hand_free': 'conditional_discount',
    
    # Granting to/from other players
    'grant_action': 'grant_action',
    'grant_extra_person': 'grant_action',
    'grant_private_action_space': 'grant_action',
    'grant_public_action_space': 'grant_action',
    'grant_residence_slot': 'grant_action',
    'allow_family_member_to_act': 'grant_action',
    'tax_space_use': 'grant_action',
    'reuse_neighbor_used_space': 'grant_action',
    'reuse_second_person_space': 'grant_action',
    'take_from_other_accumulation': 'grant_action',
    'gain_from_other_supply': 'grant_action',
    
    # Marker / tile placement on round spaces
    'mark_round_space': 'marker_placement',
    'place_on_round_spaces': 'marker_placement',
    'place_field_on_round_space': 'marker_placement',
    'place_sheep_on_future_rounds': 'marker_placement',
    'place_food_on_future_rounds': 'marker_placement',
    'place_on_round_spaces': 'marker_placement',
    
    # Copy / move cards or resources
    'copy_unused_round_card_effect': 'copy_move',
    'move_card': 'copy_move',
    'move_person_to_unoccupied_space_and_act': 'copy_move',
    'use_other_space_with_same_person': 'copy_move',
    'return_person_from_space': 'copy_move',
    'recall_first_placed_person': 'copy_move',
    'place_unplaced_person': 'copy_move',
    'use_two_spaces_one_person': 'copy_move',
    'move_card_animals_to_fields': 'copy_move',
    'move_all_goods_on_card_to_supply': 'copy_move',
    'use_resource_from_card': 'copy_move',
    'use_action_space': 'copy_move',
    'use_action_space_even_if_occupied': 'copy_move',
    'use_first_space_of_work_phase': 'copy_move',
    'substitute_action': 'copy_move',
    
    # Turn / action order modification
    'use_cumulative_space_food': 'action_order',
    'use_other_space_with_same_person': 'action_order',
    'reuse_neighbor_used_space': 'action_order',
    'reuse_second_person_space': 'action_order',
    'allow_family_member_to_act': 'action_order',
    
    # Build / construct
    'build_fences_now': 'build',
    'build_improvement': 'build',
    'build_room_with_discount': 'build',
    'build_stable_alternate_cost': 'build',
    'fence_pasture': 'build',
    'build_fence': 'build',
    'free_outer_edge_fences': 'build',
    'plow_extra_from_card': 'build',
    'place_field_tiles_on_card': 'build',
    'rebuild_fences': 'build',
    
    # Cooking / baking
    'bake_single_grain': 'cook_bake',
    'cooking_double_food': 'cook_bake',
    'harvest_grain_without_removing_field_goods': 'cook_bake',
    'skip_field_and_breeding_for_food': 'cook_bake',
    'run_field_phase': 'cook_bake',
    'reduce_sown_goods': 'cook_bake',
    
    # Animal breeding / housing
    'breed_animal': 'animal_housing',
    'free_breeding_if_room': 'animal_housing',
    'gain_animals_in_stabled_pastures': 'animal_housing',
    'place_animals_on_fenced_pasture': 'animal_housing',
    'disable_one_stable_pasture': 'animal_housing',
    'require_empty_pasture': 'animal_housing',
    'forbid_house_animals': 'animal_housing',
    'remove_stables': 'animal_housing',
    'newborn_food_requirement': 'animal_housing',
    'family_growth_without_room': 'animal_housing',
    'remove_person_if_no_housing': 'animal_housing',
    'grant_residence_slot': 'animal_housing',
    
    # Card play modifications
    'play_or_build_improvement': 'card_play',
    'play_occupation_from_hand_free': 'card_play',
    'play_occupation_without_cost': 'card_play',
    'ignore_occupation': 'card_play',
    'play_via_major_improvement_only': 'card_play',
    'allow_major_via_minor': 'card_play',
    'buy_major_via_minor_action': 'card_play',
    
    # Gain resources (conditional/specific)
    'gain_from_common_stock': 'conditional_gain',
    'gain_each_with_at_least': 'conditional_gain',
    'gain_matching_improvement': 'conditional_gain',
    'gain_points_per_max_category': 'conditional_gain',
    'bonus_for_stored_wood': 'conditional_gain',
    'gain_animals_in_stabled_pastures': 'conditional_gain',
    'conditional_gain_after_use': 'conditional_gain',
    'gain_from_other_supply': 'conditional_gain',
    
    # Scoring modifications
    'score_major_improvements': 'scoring',
    'award_most_rooms_bonus': 'scoring',
    'family_score_override': 'scoring',
    'place_reed_for_point': 'scoring',
    'gain_points_per_max_category': 'scoring',
    
    # Field modifications
    'add_good_to_empty_fields': 'field_mod',
    'add_good_to_matching_fields': 'field_mod',
    'add_good_to_single_crop_type_fields': 'field_mod',
    'add_good_to_single_good_fields': 'field_mod',
    'become_vegetable_field': 'field_mod',
    'discard_field_goods_plow_one': 'field_mod',
    'discard_field_grain': 'field_mod',
    'harvest_grain_without_removing_field_goods': 'field_mod',
    'take_field_from_round_space': 'field_mod',
    'take_vegetable_from_one_field': 'field_mod',
    'reduce_sown_goods': 'field_mod',
    
    # Other specific kinds
    'choice': 'choice',
    'pick_random_from_hand': 'random_selection',
    'capacity': 'modify_capacity',
    'combine_reed_bank_and_clay_pit': 'special_combine',
    'forbid_renovation': 'forbid_renovation',
    'forbid_family_growth': 'forbid_family_growth',
    'feeding_requirement_modifier': 'feeding_mod',
}

# Existing Rust vocabulary (for comparison)
EXISTING_OPERATIONS = {
    'gain_resource', 'gain_points', 'modify_capacity', 'discount_cost', 
    'spend_resource', 'exchange'
}

EXISTING_TRIGGERS = {
    'on_play', 'after_worker_space', 'passive', 'end_scoring', 
    'round_start', 'harvest_field_phase', 'after_clay_construction', 
    'after_harvest', 'after_bake_bread'
}

EXISTING_PREDICATES = {
    'worker_space_used', 'house_is', 'room_count', 'animal_count_at_least',
    'grain_field_count', 'fence_count_at_least_round', 'any_player', 
    'other_player_only', 'has_improvement', 'declined_improvement',
    'sow_unconditional', 'round_in', 'round_has_no_harvest', 
    'resource_at_least', 'resource_greater_than', 'accumulation_at_least'
}

def load_spec(card_id):
    spec_path = SPECS_DIR / f"{card_id}.json"
    if not spec_path.exists():
        return None
    with open(spec_path, 'r', encoding='utf-8') as f:
        return json.load(f)

def analyze_card(card_id, spec):
    """Analyze a card's effects and return primitive mechanisms needed."""
    if not spec:
        return {
            'card_id': card_id,
            'primitives': ['no_spec'],
            'details': 'No spec file found',
            'existing_coverage': {}
        }
    
    primitives = set()
    details = []
    
    effects = spec.get('effects') or []
    
    for effect in effects:
        trigger = effect.get('trigger', '')
        steps = effect.get('steps', [])
        conditions = effect.get('conditions', [])
        optional = effect.get('optional', False)
        frequency = effect.get('frequency', '')
        
        # Track trigger
        if trigger and trigger not in EXISTING_TRIGGERS:
            primitives.add(f'new_trigger:{trigger}')
            details.append(f"new trigger: {trigger}")
        
        # Analyze each step/operation
        for step in steps:
            op = step.get('operation', '')
            target = step.get('target', '')
            params = step.get('params', {})
            notes = step.get('notes', '')
            
            if op == 'other':
                # Extract the 'kind' parameter - this is the true primitive
                kind = params.get('kind', '')
                if kind:
                    primitive = KIND_TO_PRIMITIVE.get(kind, f'other:{kind}')
                    primitives.add(primitive)
                    details.append(f"other({kind}) -> {primitive}")
                else:
                    primitives.add('other:unknown')
                    details.append(f"other: unknown kind")
                    
            elif op in EXISTING_OPERATIONS:
                primitives.add(f'existing:{op}')
                details.append(f"existing op: {op} params={params}")
                
            elif op in {'build_stable', 'build_room', 'build_fence', 'plow_field', 
                        'sow', 'bake_bread', 'cook', 'convert_resource',
                        'grant_action', 'choice', 'stack_goods_on_card',
                        'place_on_round_spaces', 'take_from_supply', 'pay_resource',
                        'move_card', 'renovate', 'modify_capacity', 'discount_cost',
                        'gain_points'}:
                # These are known but not in existing vocabulary
                primitives.add(f'known:{op}')
                details.append(f"known op: {op} params={params}")
                
            else:
                primitives.add(f'unknown_op:{op}')
                details.append(f"UNKNOWN op={op}, params={params}")
        
        # Analyze conditions/predicates
        for cond in conditions:
            pred = cond.get('predicate', '')
            params = cond.get('params', {})
            if pred and pred not in EXISTING_PREDICATES:
                primitives.add(f'predicate:{pred}')
                details.append(f"predicate:{pred} params={params}")
    
    # Post-play effects
    post_play = spec.get('post_play', {})
    if post_play.get('destination') == 'left_neighbor_hand':
        primitives.add('pass_to_left')
        details.append("post_play: pass to left neighbor")
    
    # Check for dynamic/complex patterns
    if not primitives:
        primitives.add('no_primitives_found')
        details.append('No recognized primitives')
    
    # Check existing coverage
    existing_ops = [p for p in primitives if p.startswith('existing:')]
    new_ops = [p for p in primitives if not p.startswith('existing:') and not p.startswith('known:')]
    known_ops = [p for p in primitives if p.startswith('known:')]
    predicates = [p for p in primitives if p.startswith('predicate:')]
    triggers = [p for p in primitives if p.startswith('new_trigger:')]
    
    existing_coverage = {
        'existing_operations': existing_ops,
        'known_operations': known_ops,
        'new_primitives': new_ops,
        'predicates': predicates,
        'triggers': triggers
    }
    
    return {
        'card_id': card_id,
        'primitives': sorted(primitives),
        'details': '; '.join(details),
        'existing_coverage': existing_coverage
    }

# Analyze all target cards
results = []
primitive_to_cards = defaultdict(list)

for card_id in sorted(target_cards):
    spec = load_spec(card_id)
    if spec is None:
        print(f"  WARNING: No spec for {card_id}")
        result = {'card_id': card_id, 'primitives': ['no_spec'], 'details': 'No spec file', 'existing_coverage': {}}
    else:
        result = analyze_card(card_id, spec)
    results.append(result)
    for p in result['primitives']:
        primitive_to_cards[p].append(card_id)

# Print summary
print(f"\n=== Primitive Breakdown ({len(primitive_to_cards)} primitives) ===")
for prim, cards in sorted(primitive_to_cards.items(), key=lambda x: -len(x[1])):
    print(f"  {prim}: {len(cards)} cards - {cards[:5]}{'...' if len(cards) > 5 else ''}")

# Cards by primitive count
print("\n=== Cards by Primitive Count ===")
count_dist = Counter(len(r['primitives']) for r in results)
for cnt in sorted(count_dist.keys()):
    print(f"  {cnt} primitives: {count_dist[cnt]} cards")

# Complex cards (3+ primitives)
print("\n=== Complex Cards (3+ primitives) ===")
complex_cards = [r for r in results if len(r['primitives']) >= 3]
for r in sorted(complex_cards, key=lambda x: -len(x['primitives']))[:20]:
    print(f"  {r['card_id']}: {r['primitives']}")

# Existing coverage summary
print("\n=== Existing Coverage Summary ===")
fully_covered = 0
partially_covered = 0
not_covered = 0
for r in results:
    ec = r.get('existing_coverage', {})
    if ec.get('new_primitives') and not ec.get('existing_operations') and not ec.get('known_operations'):
        not_covered += 1
    elif ec.get('existing_operations') or ec.get('known_operations'):
        if ec.get('new_primitives'):
            partially_covered += 1
        else:
            fully_covered += 1
    else:
        not_covered += 1

print(f"  Fully covered by existing/known ops: {fully_covered}")
print(f"  Partially covered (some existing + new): {partially_covered}")
print(f"  Not covered (only new primitives): {not_covered}")

# Cards with no_spec
no_spec = [r for r in results if 'no_spec' in r['primitives']]
print(f"  No spec file: {len(no_spec)}")

# Save detailed results
output = {
    'summary': {
        'total_cards_analyzed': len(target_cards),
        'primitive_count': len(primitive_to_cards),
        'cards_by_primitive_count': dict(count_dist),
        'coverage': {
            'fully_covered': fully_covered,
            'partially_covered': partially_covered,
            'not_covered': not_covered,
            'no_spec': len(no_spec)
        }
    },
    'primitives': {
        prim: {
            'count': len(cards),
            'cards': sorted(cards)
        }
        for prim, cards in primitive_to_cards.items()
    },
    'card_details': results
}

with open('data/primitive_classification.json', 'w', encoding='utf-8') as f:
    json.dump(output, f, ensure_ascii=False, indent=2)

print("\nSaved to data/primitive_classification.json")
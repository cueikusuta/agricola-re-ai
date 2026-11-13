#!/usr/bin/env python3
"""Fix test calls to dispatchers with new parameter"""

with open('src/card_events.rs', 'r') as f:
    content = f.read()

# Fix test calls for dispatch_after_worker - add false parameter
# Pattern: dispatch_after_worker(\n            ...\n            &mut pending,\n        )
old_worker_test = '''        dispatch_after_worker(
            &set,
            0,
            Space::Forest,
            &mut cards,
            &mut players,
            &mut market,
            &mut well,
            1,
            &std::collections::BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )'''

new_worker_test = '''        dispatch_after_worker(
            &set,
            0,
            Space::Forest,
            &mut cards,
            &mut players,
            &mut market,
            &mut well,
            1,
            &std::collections::BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
            false, // test: not post-work chain
        )'''

content = content.replace(old_worker_test, new_worker_test)

# Fix test calls for dispatch_after_clay_construction
old_clay_test = '''        dispatch_after_clay_construction(
            &set,
            0,
            &previous,
            Some(crate::model::Improvement::ClayOven),
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )'''

new_clay_test = '''        dispatch_after_clay_construction(
            &set,
            0,
            &previous,
            Some(crate::model::Improvement::ClayOven),
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false, // test: not post-work chain
        )'''

content = content.replace(old_clay_test, new_clay_test)

# Fix test calls for dispatch_after_harvest_bake
old_bake_test = '''        dispatch_after_harvest_bake(
            &set,
            0,
            &mut c2.cards,
            &mut c2.players,
            &mut market2,
            &mut well2,
            5,
            &std::collections::BTreeSet::new(),
            &mut c2.budget,
            &mut c2.uses,
            &c2.limits,
            &mut pending2,
        )'''

new_bake_test = '''        dispatch_after_harvest_bake(
            &set,
            0,
            &mut c2.cards,
            &mut c2.players,
            &mut market2,
            &mut well2,
            5,
            &std::collections::BTreeSet::new(),
            &mut c2.budget,
            &mut c2.uses,
            &c2.limits,
            &mut pending2,
            false, // test: not post-work chain
        )'''

content = content.replace(old_bake_test, new_bake_test)

# Fix test calls for dispatch_after_worker with used_spaces
old_worker_used = '''        dispatch_after_worker(
                &set,
                0,
                space,
                &mut c.cards,
                &mut c.players,
                &mut market,
                &mut well,
                1,
                &used,
                &mut c.budget,
                &mut c.uses,
                &c.limits,
                &mut pending,
            )'''

new_worker_used = '''        dispatch_after_worker(
                &set,
                0,
                space,
                &mut c.cards,
                &mut c.players,
                &mut market,
                &mut well,
                1,
                &used,
                &mut c.budget,
                &mut c.uses,
                &c.limits,
                &mut pending,
                false, // test: not post-work chain
            )'''

content = content.replace(old_worker_used, new_worker_used)

# Fix test calls for dispatch_after_worker in exhaustive test
old_worker_exhaustive = '''            dispatch_after_worker(
                    &set,
                    0,
                    space,
                    &mut c.cards,
                    &mut c.players,
                    &mut market,
                    &mut well,
                    1,
                    &used,
                    &mut c.budget,
                    &mut c.uses,
                    &c.limits,
                    &mut pending,
                )'''

new_worker_exhaustive = '''            dispatch_after_worker(
                    &set,
                    0,
                    space,
                    &mut c.cards,
                    &mut c.players,
                    &mut market,
                    &mut well,
                    1,
                    &used,
                    &mut c.budget,
                    &mut c.uses,
                    &c.limits,
                    &mut pending,
                    false, // test: not post-work chain
                )'''

content = content.replace(old_worker_exhaustive, new_worker_exhaustive)

# Fix test calls for dispatch_after_harvest_bake in round gate test
old_bake_round = '''        dispatch_after_harvest_bake(
                &set,
                0,
                &mut c.cards,
                &mut c.players,
                &mut market,
                &mut well,
                round,
                &std::collections::BTreeSet::new(),
                &mut c.budget,
                &mut c.uses,
                &c.limits,
                &mut pending,
            )'''

new_bake_round = '''        dispatch_after_harvest_bake(
                &set,
                0,
                &mut c.cards,
                &mut c.players,
                &mut market,
                &mut well,
                round,
                &std::collections::BTreeSet::new(),
                &mut c.budget,
                &mut c.uses,
                &c.limits,
                &mut pending,
                false, // test: not post-work chain
            )'''

content = content.replace(old_bake_round, new_bake_round)

# Fix test calls for dispatch_after_worker in a078 test
old_a078_worker1 = '''        dispatch_after_worker(
            &set,
            0,
            Space::Forest,
            &mut cards,
            &mut players,
            &mut improvement_market,
            &mut delayed_well,
            1,
            &std::collections::BTreeSet::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
        )'''

new_a078_worker1 = '''        dispatch_after_worker(
            &set,
            0,
            Space::Forest,
            &mut cards,
            &mut players,
            &mut improvement_market,
            &mut delayed_well,
            1,
            &std::collections::BTreeSet::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            false, // test: not post-work chain
        )'''

content = content.replace(old_a078_worker1, new_a078_worker1)

old_a078_worker2 = '''        dispatch_after_worker(
            &set,
            0,
            Space::Fishing,
            &mut cards,
            &mut players,
            &mut improvement_market,
            &mut delayed_well,
            1,
            &std::collections::BTreeSet::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
        )'''

new_a078_worker2 = '''        dispatch_after_worker(
            &set,
            0,
            Space::Fishing,
            &mut cards,
            &mut players,
            &mut improvement_market,
            &mut delayed_well,
            1,
            &std::collections::BTreeSet::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            false, // test: not post-work chain
        )'''

content = content.replace(old_a078_worker2, new_a078_worker2)

# Write back
with open('src/card_events.rs', 'w') as f:
    f.write(content)

print("Test calls fixed!")
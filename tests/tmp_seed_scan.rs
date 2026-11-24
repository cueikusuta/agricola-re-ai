//! TEMPORARY seed scan — delete before commit.
use agricola_re_ai::cards::{
    verified_band_a_cards, verified_basic_card_set, CardDefinition, CardKind, CardSet,
};
use agricola_re_ai::model::{Config, Game};

fn set_with_a068() -> CardSet {
    let mut defs: Vec<CardDefinition> = verified_basic_card_set(2)
        .unwrap()
        .definitions
        .into_iter()
        .filter(|c| c.id != "A068")
        .collect();
    let a068 = verified_band_a_cards()
        .into_iter()
        .find(|c| c.id == "A068")
        .expect("A068 must exist in band A");
    defs.push(a068);
    CardSet { definitions: defs }
}

#[test]
fn scan_seeds_for_round_decks() {
    let set = set_with_a068();
    let occs: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| matches!(c.kind, CardKind::Occupation))
        .map(|c| c.id.clone())
        .collect();
    let minors: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| matches!(c.kind, CardKind::MinorImprovement) && c.id != "A068")
        .map(|c| c.id.clone())
        .collect();
    for seed in 0..40u64 {
        let mut hands = vec![occs[..7].to_vec(), occs[7..14].to_vec()];
        hands[0].push("A068".to_string());
        hands[0].extend(minors[0..6].iter().cloned());
        hands[1].extend(minors[7..14].iter().cloned());
        let config = Config {
            players: 2,
            seed,
            additional_action_space_enabled: false,
        };
        let mut game =
            Game::with_cards_and_hands_with_start_player(config, set.clone(), hands, 0).unwrap();
        let mut deck = Vec::new();
        for _ in 0..6 {
            deck.push(game.public.revealed.last().copied().unwrap());
            let _ = game.start_round();
        }
        let s = deck
            .iter()
            .map(|c| format!("{c:?}"))
            .collect::<Vec<_>>()
            .join(",");
        println!("seed {seed}: {s}");
    }
}

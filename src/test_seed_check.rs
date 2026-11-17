use crate::cards::{verified_basic_card_set, CardState};
use crate::model::{Config, Game};

#[test]
fn check_seed_1_start_player() {
    let set = verified_basic_card_set(2).expect("verified card set");
    let game = Game::with_cards(
        Config {
            players: 2,
            seed: 1,
            additional_action_space_enabled: false,
        },
        set,
    )
    .expect("game creation must succeed");

    println!("Start player: {}", game.public.start_player);
    println!("Active player: {}", game.public.active_player);
    println!("Round: {}", game.public.round);
    println!("Player 0 hand: {:?}", game.cards.hands[0]);
    println!("Player 1 hand: {:?}", game.cards.hands[1]);
}

#[test]
fn check_seed_1_deal() {
    let set = verified_basic_card_set(2).expect("verified card set");
    let cards = CardState::deal(&set, 1, 2);
    println!("Player 0 hand: {:?}", cards.hands[0]);
    println!("Player 1 hand: {:?}", cards.hands[1]);
    println!("Undealt: {:?}", cards.undealt);

    // Check if A078 and B121 are in player 0's hand
    let p0 = &cards.hands[0];
    println!("Player 0 has A078: {}", p0.iter().any(|c| c == "A078"));
    println!("Player 0 has B121: {}", p0.iter().any(|c| c == "B121"));
}

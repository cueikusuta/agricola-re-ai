pub struct CardState {
    pub hands: Vec<Vec<String>>,
    pub undealt: Vec<String>,
}

impl CardState {
    pub fn deal(set: &CardSet, seed: u64, players: usize) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0xA5E1_9C32_76D4_50BF);
        let mut hands = vec![Vec::new(); players];
        let mut undealt = Vec::new();
        for kind in [CardKind::Occupation, CardKind::MinorImprovement] {
            let mut pool = set
                .definitions
                .iter()
                .filter(|c| c.kind == kind)
                .map(|c| c.id.clone())
                .collect::<Vec<_>>();
            pool.shuffle(&mut rng);
            for hand in &mut hands {
                for _ in 0..7.min(pool.len()) {
                    hand.push(pool.pop().expect("checked length"));
                }
            }
            undealt.extend(pool);
        }
        Self { hands, undealt }
    }

    /// Build a card state from explicit, fully specified starting hands.
    ///
    /// This is the M11 draft-to-game bridge (#104): it never re-shuffles or
    /// deals randomly. Every card id must already exist in `set`, appear at
    /// most once across all seats, and each seat must hold exactly seven
    /// occupations and seven minor improvements. Cards that no seat was given
    /// become `undealt`, so the usual "every implemented card has exactly one
    /// location" invariant still holds.
    pub fn from_hands(set: &CardSet, hands: &[Vec<String>]) -> Result<Self, String> {
        if !set.enabled() {
            return Err("cannot seed hands from an empty card set".into());
        }
        if hands.is_empty() {
            return Err("at least one seat is required".into());
        }
        let mut seen = BTreeSet::new();
        for (seat, hand) in hands.iter().enumerate() {
            for id in hand {
                let definition = set
                    .definition(id)
                    .ok_or_else(|| format!("seat {seat} was given unknown card {id}"))?;
                if !seen.insert(id.clone()) {
                    return Err(format!("starting card {id} appears more than once"));
                }
                let _ = definition;
            }
            for kind in [CardKind::Occupation, CardKind::MinorImprovement] {
                let count = hand
                    .iter()
                    .filter(|id| set.definition(id).is_some_and(|card| card.kind == kind))
                    .count();
                if count != 7 {
                    return Err(format!(
                        "seat {seat} must start with seven {}, got {count}",
                        match kind {
                            CardKind::Occupation => "occupations",
                            CardKind::MinorImprovement => "minor improvements",
                        }
                    ));
                }
            }
        }
        let undealt = set
            .definitions
            .iter()
            .filter(|card| !seen.contains(&card.id))
            .map(|card| card.id.clone())
            .collect();
        Ok(Self {
            hands: hands.to_vec(),
            undealt,
        })
    }

    /// In-game location validation: every implemented card ID occupies exactly
    /// one zone (undealt, a hand, or a played area), with no duplicates and no
    /// orphans. This is the invariant every card move must preserve, so it
    /// holds for the whole game — including positions where cards changed hands
    /// or were acquired after the initial deal.
    pub fn validate_location_invariant(
        &self,
        set: &CardSet,
        players: &[Player],
    ) -> Result<(), String> {
        if self.hands.len() != players.len() {
            return Err("card hand count mismatch".into());
        }
        let mut all = BTreeSet::new();
        for id in self
            .undealt
            .iter()
            .chain(self.hands.iter().flat_map(|hand| hand.iter()))
            .chain(players.iter().flat_map(|player| player.played_cards.iter()))
        {
            if set.definition(id).is_none() || !all.insert(id.as_str()) {
                return Err("card missing, duplicate or unknown".into());
            }
        }
        if all.len() != set.definitions.len() {
            return Err("card partition does not match card set".into());
        }
        Ok(())
    }

    /// Initial distribution validation: each player starts with exactly seven
    /// occupations and seven minor improvements across hand + played area.
    ///
    /// This is a *setup* property, not a running invariant: once play begins,
    /// cards change hands and can be acquired, so the per-player 7+7 count stops
    /// holding by design. Enforce it at deal/setup time only.
    pub fn validate_initial_distribution(
        &self,
        set: &CardSet,
        players: &[Player],
    ) -> Result<(), String> {
        if self.hands.len() != players.len() {
            return Err("card hand count mismatch".into());
        }
        for (hand, player) in self.hands.iter().zip(players) {
            let own = hand.iter().chain(player.played_cards.iter());
            for kind in [CardKind::Occupation, CardKind::MinorImprovement] {
                let count = own
                    .clone()
                    .filter(|id| set.definition(id).is_some_and(|c| c.kind == kind))
                    .count();
                if count != if set.enabled() { 7 } else { 0 } {
                    return Err("player has incorrect original card allocation".into());
                }
            }
        }
        Ok(())
    }

    /// Card score invariant: a player's cached `card_points` equals the sum of
    /// the points printed on the cards in their played area.
    pub fn validate_card_points(&self, set: &CardSet, players: &[Player]) -> Result<(), String> {
        for (_hand, player) in self.hands.iter().zip(players) {
            let points: i32 = player
                .played_cards
                .iter()
                .filter_map(|id| set.definition(id))
                .map(|card| card.points)
                .sum();
            if points != player.card_points {
                return Err("card score mismatch".into());
            }
        }
        Ok(())
    }

    /// Running-position validation: location partition + card score. Use this
    /// for any state reached by play; it does not assume the initial 7+7 deal.
    pub fn validate_in_game(&self, set: &CardSet, players: &[Player]) -> Result<(), String> {
        self.validate_location_invariant(set, players)?;
        self.validate_card_points(set, players)
    }

    /// Full validation: the running invariants plus the initial-distribution
    /// count. Keep this for setup/deal state; in-game callers use
    /// [`Self::validate_in_game`], because a transfer or a newly acquired card
    /// legitimately breaks the per-player 7+7 count.
    pub fn validate(&self, set: &CardSet, players: &[Player]) -> Result<(), String> {
        self.validate_location_invariant(set, players)?;
        self.validate_initial_distribution(set, players)?;
        self.validate_card_points(set, players)
    }
}
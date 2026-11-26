//! Card-set metadata is supplied by callers; copyrighted card text is never bundled.
//! This is a deliberately small deterministic effect vocabulary, not a claim that
//! the full Agricola Revised deck has been implemented.
use crate::model::{Animal, Crop, Good, House, Improvement, Player, Space, Tile};
use crate::registry::{
    build_default_registry, card_set_from_pool, card_set_from_registry, CardId, CardPool,
    CardRegistry,
};
use rand::{seq::SliceRandom, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::sync::Arc;

mod band_a;
mod band_b;
mod band_c;
mod band_d;
mod band_e;
mod presets;

pub use band_a::*;
pub use band_b::*;
pub use band_c::*;
pub use band_d::*;
pub use band_e::*;
pub use presets::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardKind {
    Occupation,
    MinorImprovement,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardEffect {
    /// Test-only probe: an effect whose resolution requires a player decision.
    /// Compiled only in the crate's own test build, so the production card pool
    /// and `CardSet::content_hash` are unaffected. It exists so the dispatcher's
    /// Interrupt branch (handler -> `EffectResult::Pending` -> `PendingDecision`
    /// -> pause) is exercised end to end instead of being an untested path.
    #[cfg(test)]
    PendingProbe {
        options: Vec<(Good, u32)>,
    },
    OnPlayGain {
        good: Good,
        amount: u32,
    },
    AfterWorkerGain {
        space: Space,
        good: Good,
        amount: u32,
    },
    EndGameBonus {
        rule: ScoreBonusRule,
    },
    Prerequisite {
        rule: CardRequirement,
    },
    OnPlayGainPerStored {
        stored: Good,
        divisor: u32,
        good: Good,
        amount: u32,
    },
    OnPlayGainIfHouse {
        house: House,
        rooms: u8,
        good: Good,
        amount: u32,
    },
    StartRoundGain {
        good: Good,
        amount: u32,
        house: Option<House>,
        rooms: Option<u8>,
    },
    HarvestFieldGain {
        rule: HarvestFieldRule,
        good: Good,
        amount: u32,
    },
    AfterWorkerGainIfUsed {
        space: Space,
        required_space: Space,
        good: Good,
        amount: u32,
    },
    Passive {
        rule: PassiveRule,
    },
    AfterClayConstructionGain {
        good: Good,
        amount: u32,
    },
    AfterHarvestBakeGain {
        amount: u32,
    },
    /// C111 小動物飼育員 (Small Animal Breeder): at each round start, if the
    /// owner's food stock is at least the current round number, gain `amount`
    /// of `good` (the printed card grants 1 food). The threshold is the round
    /// number, so this is expressed as its own variant instead of widening the
    /// serialized shape of `StartRoundGain` (existing saves and content hashes
    /// must keep their exact bytes).
    StartRoundGainIfFoodAtLeastRound {
        good: Good,
        amount: u32,
    },
    /// C116 調度品職人 (Furniture Maker): each time the owner plays an
    /// occupation AFTER this card, gain `amount` of `good` per food paid as
    /// that occupation's occupation cost (the food paid on the Lessons /
    /// ExtraLessons action space). The occupation's own printed cost is the
    /// separate "individual cost" of the 2016 Lookout appendix and never
    /// feeds this multiplier. Fired from the play dispatcher's after-play
    /// scan; the legacy `cards::play` helper does not re-scan earlier cards
    /// and therefore never fires it.
    AfterOccupationGainPerFoodCost {
        good: Good,
        amount: u32,
    },
    /// E111 世捨て人 (Recluse), round-start half: at each round start, if the
    /// owner has played no minor improvements, gain `amount` of `good`. The
    /// condition needs a card-definition lookup, so the generic condition
    /// scan reports it as triggerable and the handler performs the check.
    StartRoundGainIfNoMinorPlayed {
        good: Good,
        amount: u32,
    },
    /// E111 世捨て人 (Recluse), harvest half: at the start of each harvest
    /// (the field phase is the first step of the harvest), if the owner has
    /// played no minor improvements, gain `amount` of `good`. Like the round
    /// variant, the handler performs the definition-dependent check.
    HarvestFieldGainIfNoMinorPlayed {
        good: Good,
        amount: u32,
    },
    /// A117 木材調達人 (Wood Carrier): when playing this card, gain `amount`
    /// of `good` per improvement already in front of the owner (base
    /// improvements plus played minor-improvement cards; this card itself is
    /// an occupation and never counts).
    OnPlayGainPerImprovementPlayed {
        good: Good,
        amount: u32,
    },
    /// On-play gain scaled by a counted quantity: gain `amount` of `good` per
    /// unit of `count_kind` the player owns at play time.
    OnPlayGainPerCount {
        count_kind: CountKind,
        good: Good,
        amount: u32,
    },
    /// C007 刈り鋏 (Blade Shears): on play, the owner chooses one gain option.
    /// `amount` is a fixed gain unless `per_animal` is set, in which case the
    /// gained amount is `amount * player.animal_count(per_animal)` (e.g. "1
    /// food per sheep"). Resolved through the engine's `ChooseGain` pending
    /// decision; the legacy `cards::play` helper applies the first option.
    OnPlayChoiceGain {
        options: Vec<ChoiceGainOption>,
    },
    /// C006 石取り作業 (Stone Clearing): on play, place `amount` of `good`
    /// (stone) on each of the owner's empty fields. Fields count as cultivated
    /// until the stones are harvested at the next field phase.
    OnPlayAddStoneToEmptyFields {
        good: Good,
        amount: u32,
    },
    /// C006 石取り作業 / C007 刈り鋏 (BGA spec `post_play.destination =
    /// "left_neighbor_hand"`): the card does not stay in front of the owner;
    /// after its play resolves, it is passed to the left neighbor's hand.
    /// This is zone-move metadata, not a resolvable effect: the engine's
    /// PlayMinor/PlayOccupation arms move the card after the play chain
    /// completes, and every dispatcher/policy match treats it as a no-op.
    PostPlayPassLeft,
    /// A145 綱作り (Ropemaker): at the end of each harvest (After phase of
    /// EventKind::Harvest), gain `amount` of `good`. The generic condition
    /// scan reports it as triggerable for Harvest; the handler performs the
    /// unconditional gain.
    AfterHarvestGain {
        good: Good,
        amount: u32,
    },
    /// C066 永遠のライ麦畑 (Eternal Rye Cultivation): at the end of each
    /// harvest, if the owner has at least `threshold` of `good`, gain
    /// `amount` of `gain`. The generic condition scan reports it as triggerable
    /// for Harvest; the handler performs the threshold check
    /// (same pattern as E111 世捨て人).
    AfterHarvestGainIfAtLeast {
        good: Good,
        threshold: u32,
        gain: Good,
        amount: u32,
    },
    /// C066 永遠のライ麦畑 (Eternal Rye Cultivation): at the end of each
    /// harvest, if the owner has at least `threshold` of `good`, the owner
    /// gains the printed bonus: below `upgrade_threshold` unconditionally
    /// `amount` of `gain`; at or above it the owner CHOOSES `amount` of
    /// `gain` OR `upgrade_amount` of `upgrade_gain`
    /// （「3以上ある場合は代わりに小麦1を得る」). The default (spec test
    /// C066-three and `SimpleHarvestDetails`) takes the upgrade gain; the
    /// `FeedingPlan` flag `c066_take_food_instead` opts back into the
    /// primary gain. PR #385 review finding (b): the previous two
    /// unconditional effects double-granted at grain>=3.
    AfterHarvestChooseGainIfAtLeast {
        good: Good,
        threshold: u32,
        gain: Good,
        amount: u32,
        upgrade_threshold: u32,
        upgrade_gain: Good,
        upgrade_amount: u32,
    },
    /// A118 樹木庭師 (Treegardener): optional exchange during harvest field
    /// phase. If the owner pays `cost_amount` of `cost`, gain `gain_amount`
    /// of `gain`. Executed plan-driven (FeedingPlan::a118_wood_buys, capped
    /// at 2 per harvest) by the engine's field phase; the dispatcher scan is
    /// dormant for this variant — see effect_condition_holds. The definition
    /// remains so the registry/draft catalog still describe the printed
    /// exchange. PR #385 review finding (c).
    HarvestFieldOptionalExchange {
        cost: Good,
        cost_amount: u32,
        gain: Good,
        gain_amount: u32,
    },
    /// C068 書棚 (Bookcase): each time the owner plays an occupation AFTER
    /// this card, gain `amount` of `good`. Fixed amount per occupation played,
    /// independent of the occupation's cost. Fired from the play dispatcher's
    /// after-play scan; the legacy `cards::play` helper does not re-scan earlier
    /// cards and therefore never fires it.
    AfterOccupationGain {
        good: Good,
        amount: u32,
    },
    /// C141 羊の世話人 / A050 ミルクがめ (self half): each time ANY player uses
    /// the specified space, the card owner gains `amount` of `good`. Unlike
    /// `AfterWorkerGain` which only fires when the card owner themselves uses
    /// the space, this variant fires for any player's use (including the owner).
    AfterAnyWorkerGain {
        space: Space,
        good: Good,
        amount: u32,
    },
    /// A050 ミルクがめ (others half): each time ANY player uses the specified
    /// space, all players EXCEPT the card owner gain `amount` of `good`.
    /// This is a separate variant to keep the effect vocabulary focused and
    /// avoid widening `AfterAnyWorkerGain` with an `others_amount` field.
    AfterAnyWorkerOthersGain {
        space: Space,
        good: Good,
        amount: u32,
    },
    /// D061 わらのたわら (Bale of Straw): at the start of each harvest, if the
    /// card owner has at least `threshold` fields sown with `crop`, gain
    /// `amount` of `good`. The condition is evaluated in the handler because
    /// it requires inspecting the player's farm state.
    HarvestStartGainIfFields {
        crop: Crop,
        threshold: u8,
        good: Good,
        amount: u32,
    },
    /// C083 若牛 (Early Cattle): on play, gain `amount` of `animal` (cattle).
    /// The player must provide an AnimalPlan for placement.
    OnPlayGainAnimal {
        animal: Animal,
        amount: u32,
    },
    /// C147 牛の飼育士 (Cowherd): each time the owner uses `space` (CattleMarket),
    /// gain `amount` of `animal` (cattle). The player must provide an AnimalPlan
    /// for placement.
    AfterWorkerGainAnimal {
        space: Space,
        animal: Animal,
        amount: u32,
    },
    /// B002 小牧場 (Mini Pasture): on play, fence `spaces` empty farmyard
    /// cell(s) into a pasture at no wood cost. The engine picks the cell
    /// deterministically (adjacent to an existing pasture when one exists);
    /// the rule itself lives in `engine::build_free_single_cell_pasture`.
    OnPlayBuildPasture {
        spaces: u8,
    },
    /// B003 密造酒 (Moonshine): on play, take one occupation from the owner's
    /// hand and either play it immediately by paying `food_cost` food, or pass
    /// it to the left neighbour. Hand-dependent: resolved by the play
    /// dispatcher's handler (`card_events`), not by the hand-less
    /// `apply_on_play_effects` projection path.
    OnPlayDrawPlayOrPassOccupation {
        food_cost: u32,
    },
    /// B005 経験の蓄積 (Store of Experience): on play, gain 1 of the good
    /// paired with the first band whose threshold covers the number of
    /// occupations still in the owner's hand (bands ascending, e.g.
    /// `[(4, stone), (5, reed), (6, clay), (7, wood)]`). Hand-dependent:
    /// resolved by the play dispatcher's handler.
    OnPlayGainByHandOccupationCount {
        bands: Vec<(u32, Good)>,
    },
    /// B008 露店 (Market Stall): on play, exchange `give_amount` of `give`
    /// for `receive_amount` of `receive`. If the payment resource is missing,
    /// the exchange does not fire (no partial exchange, no play failure).
    OnPlayExchangeGain {
        give: Good,
        give_amount: u32,
        receive: Good,
        receive_amount: u32,
    },
    /// B009 むち (Beating Rod): on play, gain `gain_amount` of `gain_good`,
    /// or pay `pay_amount` of `pay_good` to gain one `animal`. The engine
    /// deterministically exchanges whenever the payment resource is available;
    /// the animal is placed through the derived AnimalPlan (see the dispatcher
    /// handler for C083's vocabulary).
    OnPlayGainOrExchangeAnimal {
        gain_good: Good,
        gain_amount: u32,
        pay_good: Good,
        pay_amount: u32,
        animal: Animal,
    },
    /// A109 小商人 (Small Trader): when an improvement is played from hand
    /// via a Major or Minor Improvement action, gain `amount` of `good`.
    /// Fired from the play dispatcher's after-play scan for improvements;
    /// the legacy `cards::play` helper does not re-scan earlier cards
    /// and therefore never fires it.
    AfterImprovementPlayedGain {
        good: Good,
        amount: u32,
    },
    /// D065 麦ふるい (Grain Sieve): during each harvest's field phase, if the
    /// grain gained THIS field phase (pre-harvest equal to the number of
    /// grain fields with a growing crop) is at least `threshold`, gain
    /// `amount` of `good`. The engine's field phase harvests exactly one
    /// grain per growing grain field AFTER this dispatch fires, so counting
    /// grain fields with `remaining > 0` here equals the grain this phase
    /// yields (pinned by comment and tests: the count is per FIELD, not per
    /// remaining unit). The condition needs farm state, so the generic
    /// condition scan reports it as triggerable and the handler performs
    /// the threshold check (same pattern as E111 世捨て人).
    HarvestFieldGainIfGrainHarvestedAtLeast {
        threshold: u32,
        good: Good,
        amount: u32,
    },
    /// A090 鋤職人 (Plow Driver): at each round start, if the owner lives in a
    /// stone house, may pay 1 food to plow exactly 1 field. Optional; the
    /// handler checks the house condition and resource availability.
    StartRoundPlowFieldIfHouse {
        house: House,
        food_cost: u32,
        fields: u32,
    },
    /// A151 吟遊楽人 (Minstrel): at each round start in rounds 1-4, if the round
    /// action space for that round is unused (no worker placed), copy its
    /// effect. The copied effect is resolved deterministically without placing
    /// a worker. Handled in the round-start dispatcher.
    StartRoundCopyUnusedRoundSpace {
        max_round: u8,
    },
    /// A152 夜学生 (Night-School Student): at each round start, if the Lessons
    /// space was used by fewer players than any other action space this round,
    /// may pay 1 food to play an occupation from hand. The condition requires
    /// checking all players' used spaces; handled in the handler.
    StartRoundPlayOccupationIfLessonsUnderused {
        food_cost: u32,
    },
    /// B097 学者 (Scholar): at each round start, if the owner has a stone house,
    /// may either pay 1 food to play an occupation from hand, or pay the
    /// normal cost to play a minor improvement from hand. Optional; two
    /// distinct choices handled in the handler.
    StartRoundPlayCardIfStoneHouse {
        occupation_food_cost: u32,
    },
    /// B106 清廉潔白な人 (Moral Crusader): at each round start, if there are
    /// any goods on revealed round action spaces (accumulations), gain 1 food.
    /// Unconditional gain when condition met; checked in effect_condition_holds.
    StartRoundGainIfGoodsOnRoundSpaces {
        good: Good,
        amount: u32,
    },
    /// B135 栄養士 (Nutrition Expert): at each round start, may exchange 1
    /// animal of any owned type, 1 grain, and 1 vegetable for 5 food and 2
    /// points. Optional; the handler verifies all give resources are available
    /// before exchanging. Points are added immediately to card_points.
    StartRoundExchangeForFoodAndPoints {
        give_animal: bool,
        give_grain: u32,
        give_vegetable: u32,
        receive_food: u32,
        receive_points: i32,
    },
    /// C064 穀物蒸留酒製造所 (Corn Schnapps Distillery): at each round start,
    /// may pay 1 grain to place 1 food on each of the next 4 round action
    /// spaces. The food is added to those spaces' accumulations and will be
    /// available when those rounds are reached. Optional; handler checks grain
    /// availability and updates future round accumulations.
    StartRoundPlaceFoodOnNextRoundSpaces {
        grain_cost: u32,
        food_per_space: u32,
        num_spaces: u8,
    },
    /// B114 子なし (Childless): at each round start, if the owner has exactly
    /// `family` family members and at least `rooms_at_least` rooms, gain
    /// `amount` of `good`. Fired from the round-start After scan; the condition
    /// is judged inside the handler (E111/C111 pattern). The spec JSON's second
    /// step (choose 1 grain or 1 vegetable) has no choice-effect slot at this
    /// trigger and is recorded as needs_resolution in the PR — only the fixed
    /// food gain is implemented here.
    StartRoundGainIfFamilyAndRooms {
        family: u32,
        rooms_at_least: u8,
        good: Good,
        amount: u32,
    },
    /// A157 ボヘミアン (Bohemian): at each round start, if the Lessons space
    /// has at least `min` occupants (i.e. `public.used` contains Lessons),
    /// gain `amount` of `good`. The condition is judged inside the handler
    /// (E111/C111 pattern). NOTE: the engine clears `public.used` at the top
    /// of `start_round` before the round-start dispatch, so the handler only
    /// fires when a dispatch carries a non-empty used-space set — see the PR
    /// body for the recorded engine-timing needs_resolution.
    StartRoundGainIfLessonsOccupants {
        min: u32,
        good: Good,
        amount: u32,
    },
    /// A049 営巣地 (Nest Site): at each round start, if the `space`
    /// accumulation holds at least `min` AND grew this round (the per-round
    /// refill landed on a non-empty accumulation), gain `amount` of `good`.
    /// The condition is judged inside the handler from the public
    /// accumulations map threaded through `DispatchContext`.
    StartRoundGainIfAccumulationGrew {
        space: Space,
        min: u32,
        good: Good,
        amount: u32,
    },
    /// C067 ミネラルフィーダー (Mineral Feeder): at each round start, if the
    /// round does not end with a harvest (`!HARVEST_ROUNDS.contains(round)`)
    /// and the owner has at least `min` `animal` in pastures, gain `amount`
    /// of `good`. The condition is judged inside the handler (E111/C111
    /// pattern).
    StartRoundGainIfNoHarvestAndAnimals {
        animal: Animal,
        min: u32,
        good: Good,
        amount: u32,
    },
    /// B073 贈り物かご (Gift Basket): on play, if the owner has exactly
    /// `rooms` rooms, gain `amount` of `good`. The printed table
    /// (2 -> vegetable, 3 -> food, 4 -> grain, 5 -> vegetable; 1 each) is
    /// expressed as four effects of this variant, and only the one row whose
    /// room count matches the owner exactly resolves. A dedicated variant
    /// (instead of widening `OnPlayGainIfHouse`) keeps the serialized shape
    /// of existing variants byte-identical for saves and content hashes.
    OnPlayGainIfRoomCount {
        rooms: u8,
        good: Good,
        amount: u32,
    },
    /// A140 ショベル運搬人 (Shovel Bearer): after the owner uses `space`
    /// (ClayPit or Hollow), gain `good` (food) equal to the clay standing on
    /// the OTHER clay accumulation space (ClayPit use reads Hollow, Hollow
    /// use reads ClayPit). The printed card pays the opposite accumulation's
    /// current clay count, so the amount is dynamic and deliberately not
    /// folded into `AfterWorkerGain`'s fixed `amount`: the handler reads it
    /// from the public accumulations map the after-worker dispatcher threads
    /// read-only through `DispatchContext`. Only available at 3+ players.
    AfterWorkerGainOtherAccumulationFood {
        /// The accumulation space the owner used (clay_pit or hollow).
        space: Space,
        /// The good granted (food on the printed card).
        good: Good,
    },
    /// D012 搾乳所 (Milking Place): +1 food in the feeding phase of each
    /// harvest (automatic, no decision). Resolved plan-free by the engine's
    /// harvest step; the phase scan stays dormant.
    HarvestFeedingGain {
        /// If set, the good granted each feeding phase (D012).
        good: Option<Good>,
        /// Amount of the good granted.
        amount: u32,
    },
    /// Feeding-phase owner-chosen exchanges (A062, C055, C059, C063, D062,
    /// D084): the owner chooses one printed option (or declines). Executed
    /// plan-driven via `FeedingPlan` by the engine's harvest step; the phase
    /// scan stays dormant.
    HarvestFeedingOptionalExchange {
        options: Vec<FeedingExchangeOption>,
    },
    /// A001 雨除け (Shelter) / C002 厩 (Stable) / E001 簡易納屋 (Pole Barns):
    /// on play, build up to `amount` stables, paying `wood_cost` wood per
    /// stable when non-zero (all three cards print wood_cost 0 — the stable
    /// itself is free while the card cost is still paid). Each stable must
    /// stand on a legal site within `scope` (A001 restricts placement to the
    /// cell of a fenced one-cell pasture). The printed effect leaves placement
    /// to the player; the play executor cannot represent a play-time pause
    /// (Issue #268 R2-P0-2), so sites are picked deterministically (first
    /// legal cell in farm order) and the build stops when no legal site
    /// remains — the play itself always succeeds. The rule itself lives in
    /// `engine::free_stable_site_candidates` / `engine::build_free_stable`.
    OnPlayBuildStable {
        amount: u32,
        wood_cost: u32,
        scope: StablePlacementScope,
    },
    /// A066 餌皿 (Feeding Dish): after the owner uses a sheep/pig/cattle
    /// market, if the owner already keeps that animal type
    /// (`player.animal_count(kind) > 0` after the event resolves — animals
    /// taken at that same market count), gain `amount` of `good` (printed:
    /// 1 grain). One variant per market fixes the space→animal mapping; no
    /// new persistent field or PendingDecision is introduced.
    AfterWorkerGainIfOwnsAnimal {
        space: Space,
        animal: Animal,
        good: Good,
        amount: u32,
    },
    /// B143 粘土守 (Clay Warden): on any player's use of `space` (Hollow),
    /// each non-actor owner gains `base_good` x `base_amount`; additionally,
    /// each non-actor owner gains `extra_3p_good` x `extra_3p_amount` in a 3p
    /// game, or `extra_4p_good` x `extra_4p_amount` in a 4p game. The actor
    /// never gains from this effect. 2p ineligibility is expressed by a
    /// separate `Prerequisite { MinimumPlayers(3) }` effect, not by this
    /// variant, so the variant itself is player-count agnostic and reads the
    /// count from `players.len()` at dispatch time.
    AfterAnyWorkerGainByCountIfNotActor {
        space: Space,
        base_good: Good,
        base_amount: u32,
        extra_3p_good: Good,
        extra_3p_amount: u32,
        extra_4p_good: Good,
        extra_4p_amount: u32,
    },
    /// A068 アスパラガスの贈り物 (Asparagus Gift): each time the owner builds a
    /// number of fences equal to or greater than the current round number,
    /// gain `amount` of `good` (printed: 1 vegetable). "A number of fences"
    /// is the number built by THAT fence-building action (official card text),
    /// not the standing fence total, and it is not limited to the Fencing
    /// action space: worker actions that build fences (Fencing,
    /// FarmRedevelopment) fire it. `built == 0` or `built < round` does not
    /// fire. The `space` field records the original space binding for
    /// serialization compatibility and is no longer used as the gate; the
    /// gate is the pre-action fence snapshot carried by the dispatcher. The
    /// owner-side play-time precondition ("at least one empty field") is a
    /// separate `Prerequisite { MinimumEmptyFields(1) }` effect, so this
    /// variant needs no new persistent field or PendingDecision.
    AfterWorkerGainIfFencesAtLeastRound {
        space: Space,
        good: Good,
        amount: u32,
    },
    /// On-play per-good threshold gain: for each good in `goods`, if the
    /// owner already holds at least `threshold` of it, gain `amount` of
    /// that good. Judged per good against the owner's own supply (not
    /// resource totals); goods below threshold grant nothing. No new
    /// persistent field or PendingDecision. Used by B105
    /// (food/grain/vegetable/reed/wood, threshold 2, amount 1) and E006
    /// (wood/clay/reed/stone, threshold 4, amount 1).
    OnPlayGainEachWithAtLeast {
        goods: Vec<Good>,
        threshold: u32,
        amount: u32,
    },
    /// D054 マスの池 (Trout Pool): at each round start, if the `space`
    /// accumulation holds at least `min`, gain `amount` of `good` from the
    /// general supply. Unlike A049 営巣地 (`StartRoundGainIfAccumulationGrew`)
    /// there is no "grew this round" requirement: the printed card pays on the
    /// standing accumulation alone. The condition is judged inside the handler
    /// from the public accumulations map the round-start dispatcher threads
    /// read-only through `DispatchContext` (runtime-only, never serialized);
    /// the map is never mutated, and any dispatch path without the map grants
    /// nothing.
    StartRoundGainIfAccumulationAtLeast {
        space: Space,
        min: u32,
        good: Good,
        amount: u32,
    },
    /// A052 手投げ斧 (Throwing Axe): after the owner uses a wood accumulation
    /// space (`space` = Forest / Copse / Grove) while the boar accumulation on
    /// the `accumulation` animal market (PigMarket) holds at least `min`, gain
    /// `amount` of `good` (printed: 2 food). The space gate is judged by the
    /// generic condition scan; the handler reads the animal-market ledger the
    /// after-worker dispatcher threads read-only through `DispatchContext` —
    /// a missing entry or a count below `min` grants nothing, and the gain
    /// lands on the card owner only (this handler is reached solely for the
    /// owner's own played cards). The count is read after the worker action
    /// resolved (after_event snapshot); the triggering action itself never
    /// touches the pig market, so the standing count is the current one.
    AfterWorkerGainIfAnimalAccumulationAtLeast {
        /// The wood accumulation space the owner used (forest, copse, grove).
        space: Space,
        /// The animal market whose accumulation is read (pig_market).
        accumulation: Space,
        /// Minimum standing animals required (1 on the printed card).
        min: u32,
        good: Good,
        amount: u32,
    },
    /// On-play resource exchange: deduct `give` resources and grant `receive` resources.
    /// Used for A036 ファサード彫刻 (food -> points, capped), D009 獲物の交換
    /// (sheep -> boar + cattle).
    /// If `give_is_cost` is true, the `give` resources are assumed to have been paid as the card's play cost
    /// and are not deducted again. If false, `give` is deducted now.
    /// `cap` limits the exchange amount for optional variable exchanges (A036: completed harvests).
    OnPlayExchange {
        give: Vec<ExchangeResource>,
        receive: Vec<ExchangeResource>,
        give_is_cost: bool,
        optional: bool,
        cap: Option<u32>,
    },
}

/// Resource type for on-play exchange effects. Can represent goods, animals, or points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExchangeResource {
    Good(Good),
    Animal(Animal),
    Points,
}

/// One printed option of a harvest-feeding owner-chosen exchange.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FeedingExchangeOption {
    /// Goods paid from the owner's supply.
    pub give: Vec<(Good, u32)>,
    /// Take 1 grain from a standing grain field instead of from supply.
    pub take_field_grain: bool,
    /// Food granted.
    pub receive_food: u32,
    /// Bonus points granted.
    pub receive_points: i32,
    /// One animal of a type the owner already owns.
    pub receive_animal: bool,
}

impl CardEffect {
    /// Stable public name for this implemented effect category. Used to build
    /// the neutral draft-AI catalog (#104). It describes only effects the
    /// engine actually implements, never a guessed printed effect.
    pub fn public_tag(&self) -> &'static str {
        match self {
            #[cfg(test)]
            CardEffect::PendingProbe { .. } => "pending_probe",
            CardEffect::OnPlayGain { .. } => "on_play_gain",
            CardEffect::AfterWorkerGain { .. } => "after_worker_gain",
            CardEffect::EndGameBonus { .. } => "end_game_bonus",
            CardEffect::Prerequisite { .. } => "prerequisite",
            CardEffect::OnPlayGainPerStored { .. } => "on_play_gain_per_stored",
            CardEffect::OnPlayGainIfHouse { .. } => "on_play_gain_if_house",
            CardEffect::StartRoundGain { .. } => "start_round_gain",
            CardEffect::HarvestFieldGain { .. } => "harvest_field_gain",
            CardEffect::AfterWorkerGainIfUsed { .. } => "after_worker_gain_if_used",
            CardEffect::Passive { .. } => "passive",
            CardEffect::AfterClayConstructionGain { .. } => "after_clay_construction_gain",
            CardEffect::AfterHarvestBakeGain { .. } => "after_harvest_bake_gain",
            CardEffect::StartRoundGainIfFoodAtLeastRound { .. } => {
                "start_round_gain_if_food_at_least_round"
            }
            CardEffect::AfterOccupationGainPerFoodCost { .. } => {
                "after_occupation_gain_per_food_cost"
            }
            CardEffect::AfterOccupationGain { .. } => "after_occupation_gain",
            CardEffect::StartRoundGainIfNoMinorPlayed { .. } => {
                "start_round_gain_if_no_minor_played"
            }
            CardEffect::HarvestFieldGainIfNoMinorPlayed { .. } => {
                "harvest_field_gain_if_no_minor_played"
            }
            CardEffect::OnPlayGainPerImprovementPlayed { .. } => {
                "on_play_gain_per_improvement_played"
            }
            CardEffect::OnPlayGainPerCount { .. } => "on_play_gain_per_count",
            CardEffect::AfterAnyWorkerGain { .. } => "after_any_worker_gain",
            CardEffect::OnPlayChoiceGain { .. } => "on_play_choice_gain",
            CardEffect::OnPlayAddStoneToEmptyFields { .. } => "on_play_add_stone_to_empty_fields",
            CardEffect::PostPlayPassLeft => "post_play_pass_left",
            CardEffect::AfterHarvestGain { .. } => "after_harvest_gain",
            CardEffect::AfterHarvestGainIfAtLeast { .. } => "after_harvest_gain_if_at_least",
            CardEffect::AfterHarvestChooseGainIfAtLeast { .. } => {
                "after_harvest_choose_gain_if_at_least"
            }
            CardEffect::HarvestFieldOptionalExchange { .. } => "harvest_field_optional_exchange",
            CardEffect::HarvestFeedingGain { .. } => "harvest_feeding_gain",
            CardEffect::HarvestFeedingOptionalExchange { .. } => {
                "harvest_feeding_optional_exchange"
            }

            CardEffect::OnPlayGainAnimal { .. } => "on_play_gain_animal",
            CardEffect::OnPlayGainEachWithAtLeast { .. } => "on_play_gain_each_with_at_least",
            CardEffect::AfterWorkerGainAnimal { .. } => "after_worker_gain_animal",
            CardEffect::AfterAnyWorkerOthersGain { .. } => "after_any_worker_others_gain",
            CardEffect::HarvestStartGainIfFields { .. } => "harvest_start_gain_if_fields",
            CardEffect::OnPlayBuildPasture { .. } => "on_play_build_pasture",
            CardEffect::OnPlayDrawPlayOrPassOccupation { .. } => {
                "on_play_draw_play_or_pass_occupation"
            }
            CardEffect::OnPlayGainByHandOccupationCount { .. } => {
                "on_play_gain_by_hand_occupation_count"
            }
            CardEffect::OnPlayExchangeGain { .. } => "on_play_exchange_gain",
            CardEffect::OnPlayExchange { .. } => "on_play_exchange",
            CardEffect::OnPlayGainOrExchangeAnimal { .. } => "on_play_gain_or_exchange_animal",
            CardEffect::AfterImprovementPlayedGain { .. } => "after_improvement_played_gain",
            CardEffect::HarvestFieldGainIfGrainHarvestedAtLeast { .. } => {
                "harvest_field_gain_if_grain_harvested_at_least"
            }
            CardEffect::StartRoundPlowFieldIfHouse { .. } => "start_round_plow_field_if_house",
            CardEffect::StartRoundCopyUnusedRoundSpace { .. } => {
                "start_round_copy_unused_round_space"
            }
            CardEffect::StartRoundPlayOccupationIfLessonsUnderused { .. } => {
                "start_round_play_occupation_if_lessons_underused"
            }
            CardEffect::StartRoundPlayCardIfStoneHouse { .. } => {
                "start_round_play_card_if_stone_house"
            }
            CardEffect::StartRoundGainIfGoodsOnRoundSpaces { .. } => {
                "start_round_gain_if_goods_on_round_spaces"
            }
            CardEffect::StartRoundExchangeForFoodAndPoints { .. } => {
                "start_round_exchange_for_food_and_points"
            }
            CardEffect::StartRoundPlaceFoodOnNextRoundSpaces { .. } => {
                "start_round_place_food_on_next_round_spaces"
            }
            CardEffect::StartRoundGainIfFamilyAndRooms { .. } => {
                "start_round_gain_if_family_and_rooms"
            }
            CardEffect::StartRoundGainIfLessonsOccupants { .. } => {
                "start_round_gain_if_lessons_occupants"
            }
            CardEffect::StartRoundGainIfAccumulationGrew { .. } => {
                "start_round_gain_if_accumulation_grew"
            }
            CardEffect::StartRoundGainIfNoHarvestAndAnimals { .. } => {
                "start_round_gain_if_no_harvest_and_animals"
            }
            CardEffect::OnPlayGainIfRoomCount { .. } => "on_play_gain_if_room_count",
            CardEffect::AfterWorkerGainOtherAccumulationFood { .. } => {
                "after_worker_gain_other_accumulation_food"
            }
            CardEffect::OnPlayBuildStable { .. } => "on_play_build_stable",
            CardEffect::AfterWorkerGainIfOwnsAnimal { .. } => "after_worker_gain_if_owns_animal",
            CardEffect::AfterAnyWorkerGainByCountIfNotActor { .. } => {
                "after_any_worker_gain_by_count_if_not_actor"
            }
            CardEffect::AfterWorkerGainIfFencesAtLeastRound { .. } => {
                "after_worker_gain_if_fences_at_least_round"
            }
            CardEffect::StartRoundGainIfAccumulationAtLeast { .. } => {
                "start_round_gain_if_accumulation_at_least"
            }
            CardEffect::AfterWorkerGainIfAnimalAccumulationAtLeast { .. } => {
                "after_worker_gain_if_animal_accumulation_at_least"
            }
        }
    }
}

/// Where a card-driven free stable build (A001 雨除け / C002 厩 /
/// E001 簡易納屋) may place its stables. Placement detail is a caller
/// decision in the engine; this only records the printed restriction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StablePlacementScope {
    /// Any legal stable site — the ordinary "Build Stables" legality
    /// (`engine::stable_site_available`): an empty farmyard cell or a pasture
    /// cell without a stable, within the 4-stable farm limit. C002 厩 and
    /// E001 簡易納屋 print no placement restriction.
    AnySite,
    /// Only the cell of a fenced pasture spanning exactly `pasture_size`
    /// cells (A001 雨除け: "1農場スペースだけの牧場" — a one-cell pasture only).
    OneCellPasture { pasture_size: u8 },
}

/// What to count for OnPlayGainPerCount effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CountKind {
    /// Number of occupations the player has played.
    Occupations,
    /// Number of clay rooms (clay or stone house rooms).
    ClayRooms,
    /// Number of improvements in front of the player (base + played minor improvements).
    Improvements,
    /// Total family members (workers).
    FamilyMembers,
    /// Number of major improvements in the bottom row (printed major improvements the player owns).
    MajorImprovementsBottomRow,
    /// Number of sown fields (fields with a crop planted).
    SownFields,
}

/// A single option for an on-play choice gain (e.g. C007 刈り鋏 / Blade Shears:
/// "gain 3 food" or "1 food per sheep"). When `per_animal` is `Some`, the gained
/// amount is `amount * player.animal_count(per_animal)` rather than a fixed
/// `amount`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoiceGainOption {
    pub good: Good,
    pub amount: u32,
    pub per_animal: Option<Animal>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PassiveRule {
    PastureCapacity(u32),
    FenceWoodDiscount(u32),
    ImprovementWoodDiscount(u32),
    WoodenRoomCost(u32),
    RoomCost(u32),
    MajorStoneDiscountByRooms,
    StoneDiscount(u32),
    HousingSpace(u8),
    /// D026 大工の敷地 / E024 野心: allows building major improvements
    /// when taking a Minor Improvement action (MeetingPlace).
    /// If `joinery_and_well_only` is true (D026), only Joinery and Well are allowed.
    /// If false (E024), any major improvement may be built.
    AllowMajorViaMinor {
        joinery_and_well_only: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarvestFieldRule {
    Animals {
        sheep_per: u32,
        cattle_per: u32,
    },
    MixedFields,
    /// C106 じゃがいも収穫者 (Potato Harvester): one multiplier step per
    /// vegetable that the field phase is about to yield. The dispatcher runs
    /// before crops are removed, and each planted vegetable field yields
    /// exactly one good per harvest, so the count of vegetable fields with a
    /// growing crop equals the vegetables gained this field phase.
    VegetablesHarvested,
    /// E117 パイプ喫煙家 (Pipe Smoker): 1 if at least one field is planted
    /// with wheat at the start of the harvest (the field phase is the first
    /// step of the harvest, and no state changes between them), else 0.
    HasWheatField,
    /// E107 土地調査士 (Land Surveyor): at the field phase, the amount paired
    /// with the highest threshold the number of plowed field tiles reaches
    /// (2/4/6/7 -> 1/2/3/4 food on the printed card), else 0.
    FieldCountThresholds {
        thresholds: [u32; 4],
        amounts: [u32; 4],
    },
    /// A106 液肥散布人 (Slurry Spreader): at the field phase, one multiplier
    /// step per field whose LAST grain is taken this field phase (a growing
    /// grain field yields exactly one per harvest, so fields with one
    /// remaining crop become empty afterwards).
    LastGrainTaken,
    /// A106 液肥散布人 (Slurry Spreader): same as [`HarvestFieldRule::LastGrainTaken`]
    /// for vegetables (the printed card pays 2 food per last grain and 1 per
    /// last vegetable, hence two rule variants instead of one).
    LastVegetableTaken,
    /// A118 樹木庭師 (Treegardener): fixed amount per harvest field phase,
    /// independent of field count or crop state.
    Fixed,
    /// A112 大鎌使い (Scythe Worker): one multiplier step per grain field
    /// whose crop is still growing. The dispatcher runs before crops are
    /// removed, and each growing grain field yields exactly one grain this
    /// field phase, so counting `remaining >= 1` grain fields here equals
    /// the grain the fields yield — the card pays 1 extra grain per such
    /// field. Grain in the supply and empty fields are not counted.
    GrainFieldsGrowing,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardRequirement {
    MinimumOccupations(u8),
    MaximumOccupations(u8),
    MinimumAnimals {
        animal: Animal,
        number: u32,
    },
    MinimumCropFields {
        crop: Crop,
        number: u8,
    },
    /// C007 刈り鋏 (Blade Shears): prerequisite is `pasture_count = n` — the
    /// player must have at least `n` pastures.
    PastureCount(u8),
    MinimumPlayers(u8),
    MaximumPlayers(u8),
    MinimumImprovements(u8),
    /// A068 アスパラガスの贈り物 (Asparagus Gift): the player must have at
    /// least `n` EMPTY fields — plowed `Tile::Field { crop: None }` cells with
    /// no C006 stone planted on them (`field_stones == 0`; a stone field
    /// counts as sown, not empty, per spec `empty_field_count`).
    MinimumEmptyFields(u8),
    /// E001 簡易納屋 (Pole Barns): at least `fences` fence segments built on
    /// the farm (2016 Lookout caps a farm at 15, so the printed card asks for
    /// all 15). Read from the player's fence set.
    MinimumFences(u8),
}

/// Named formulas keep card descriptions outside source and replay data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreBonusRule {
    PastureArea,
    HouseMaterial,
    UnfencedStables,
    ImprovementCount,
    SparePastureCapacity,
    LaterOccupations,
    /// D100 君主 (Sovereign): 1 bonus point per scoring category that has
    /// reached its maximum of 4 points (fields, pastures, grain, vegetables,
    /// sheep, boar, cattle, fenced stables). Read from the canonical scoring
    /// table in `score.rs` so thresholds can never diverge.
    CategoriesAtMaxFour,
    /// A099 放牧人 (Fellow Grazer): 2 bonus points per pasture covering at
    /// least 3 farm cells, computed from the final farm state.
    LargePastures,
    /// A101 調理器具商 (Cookery Outfitter): 1 bonus point per cooking
    /// improvement in front of the player (the engine's cooking
    /// improvements are the fireplace/hearth set, see `Improvement::is_cooking`).
    CookingImprovements,
    /// D032 木のくまで (Wood Rake): +2 points if at least 7 goods stood in
    /// fields before the final harvest. Threshold stored in the variant.
    FieldsGoodsBeforeFinalHarvest(u32),
    /// D034 豪華な宿泊所 (Luxurious Hostel): +4 points if stone rooms exceed
    /// family members.
    StoneRoomsExceedPeople,
    /// E034 登記簿 (Land Register): +2 points if every farmyard space is used.
    FarmyardAllUsed,
    /// E035 人間不信 (Misanthropy): 2/3/5 points for exactly 4/3/2 family members.
    ExactFamilyMembers,
    /// D030 職人地区 (Artisan District): 2/5/8 bonus points when the bottom
    /// row of the supply board holds 3/4/5 or more major improvements
    /// (2 or fewer -> no bonus). The count reuses the canonical bottom-row
    /// judgment shared with `CountKind::MajorImprovementsBottomRow`
    /// (joinery/pottery/basketry); it never counts the total number of
    /// major improvements. Appended at the enum tail so existing serde
    /// variants and saved-game bytes are unchanged.
    MajorImprovementsBottomRowThresholds,
    /// D035 飼料部屋 (Fodder Chamber): 1 bonus point per `threshold` own
    /// livestock (sheep + boar + cattle) at end-game scoring. The
    /// per-player-count threshold (2p: 7, 3p: 5, 4p: 4 animals per point)
    /// is selected by player-count guard prerequisites on the card
    /// definition and projected at game entry, so exactly one threshold is
    /// active per game. Appended at the enum tail so existing serde
    /// variants and saved-game bytes are unchanged.
    LivestockThresholds(u32),
    /// B153 寮長 (Housemaster): at final scoring, sum the major-improvement
    /// scores after doubling the lowest single one (one only, even when the
    /// lowest score is tied), then take 1/2/3/4 bonus points at a doubled
    /// total of 5/7/9/11 or more. With no major improvement there is
    /// nothing to double and no bonus. The ordinary major-improvement
    /// points stay in their own scoring category (`score.rs`); this bonus
    /// is added separately by `score_with_cards`. Appended at the enum tail
    /// so existing serde variants and saved-game bytes are unchanged.
    MajorImprovementsLowestDoubledThresholds,
    /// E032 身廊 (Nave): 1 bonus point per farmyard column (of the 5) that
    /// contains at least one room, computed from the final farm state. Several
    /// rooms standing in the same column still score that column only once.
    ColumnsWithRoom,
}

/// Where a card goes after being played. Defaults to the owner's played area;
/// the printed "pass left" cards move to the left neighbour's hand instead.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostPlayDestination {
    /// Card stays in the owner's played area (default).
    #[default]
    PlayedArea,
    /// Card moves to the left neighbour's hand after play (2016 Lookout:
    /// the printed card is passed to the player on the left).
    LeftNeighborHand,
}

/// Serde skip predicate for the default post-play destination.
pub fn post_play_is_played_area(dest: &PostPlayDestination) -> bool {
    matches!(dest, PostPlayDestination::PlayedArea)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardDefinition {
    /// A stable programmatic identifier, NOT the card's printed text.
    pub id: String,
    pub kind: CardKind,
    pub cost: Vec<(Good, u32)>,
    pub points: i32,
    pub effects: Vec<CardEffect>,
    /// Where the card goes after being played. `PlayedArea` (the default) is
    /// skipped when serializing, so existing card sets and saved games keep
    /// their exact bytes and `content_hash` is unchanged unless a card
    /// actually passes left.
    #[serde(default, skip_serializing_if = "post_play_is_played_area")]
    pub post_play: PostPlayDestination,
    /// Maximum times this card's effects may resolve per round, for cards
    /// printed with a per-round use limit. `None` = no limit (the default), and
    /// it is skipped when serializing, so existing card sets and saved games
    /// keep their exact bytes and `content_hash` is unchanged unless a limit is
    /// actually declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_limit: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardSet {
    pub definitions: Vec<CardDefinition>,
}

impl CardSet {
    pub fn definition(&self, id: &str) -> Option<&CardDefinition> {
        self.definitions.iter().find(|card| card.id == id)
    }

    pub fn enabled(&self) -> bool {
        !self.definitions.is_empty()
    }

    /// Order-independent content hash of the deck's public metadata.
    ///
    /// Experiment manifests (M12/M13) must show that two runs used the same
    /// card pool. Only `id`, `kind`, `cost`, `points` and `effects` are hashed:
    /// no seed, no dealt hand, no hidden state. The `agricola-card-set-v1`
    /// domain tag keeps an empty set distinct from any populated one.
    pub fn content_hash(&self) -> String {
        let mut entries = self
            .definitions
            .iter()
            .map(|card| {
                (
                    card.id.clone(),
                    serde_json::to_string(card).expect("card definition serialization"),
                )
            })
            .collect::<Vec<_>>();
        entries.sort();
        entries.dedup();
        let mut hasher = Sha256::new();
        hasher.update(b"agricola-card-set-v1\0");
        for (id, definition) in entries {
            hasher.update(id.as_bytes());
            hasher.update([0u8]);
            hasher.update(definition.as_bytes());
            hasher.update([0u8]);
        }
        format!("{:x}", hasher.finalize())
    }

    pub fn validate(&self, players: usize) -> Result<(), String> {
        if !self.enabled() {
            return Ok(());
        }
        let mut ids = BTreeSet::new();
        for card in &self.definitions {
            if card.id.is_empty()
                || card.id.len() > 64
                || !card
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                || !ids.insert(card.id.clone())
            {
                return Err("invalid or duplicate card id".into());
            }
            if !(-10..=20).contains(&card.points)
                || card
                    .cost
                    .iter()
                    .any(|(_, quantity)| *quantity == 0 || *quantity > 30)
                || card.effects.iter().any(|effect| {
                    let amount = match effect {
                        CardEffect::OnPlayGain { amount, .. }
                        | CardEffect::AfterWorkerGain { amount, .. } => *amount,
                        CardEffect::EndGameBonus { .. }
                        | CardEffect::Prerequisite { .. }
                        | CardEffect::Passive { .. } => 1,
                        #[cfg(test)]
                        CardEffect::PendingProbe { .. } => 1,
                        CardEffect::OnPlayGainPerStored { amount, .. }
                        | CardEffect::OnPlayGainIfHouse { amount, .. }
                        | CardEffect::StartRoundGain { amount, .. }
                        | CardEffect::StartRoundGainIfFoodAtLeastRound { amount, .. }
                        | CardEffect::AfterOccupationGainPerFoodCost { amount, .. }
                        | CardEffect::AfterOccupationGain { amount, .. }
                        | CardEffect::StartRoundGainIfNoMinorPlayed { amount, .. }
                        | CardEffect::HarvestFieldGainIfNoMinorPlayed { amount, .. }
                        | CardEffect::OnPlayGainPerImprovementPlayed { amount, .. }
                        | CardEffect::HarvestFieldGain { amount, .. }
                        | CardEffect::AfterWorkerGainIfUsed { amount, .. }
                        | CardEffect::AfterClayConstructionGain { amount, .. }
                        | CardEffect::AfterHarvestBakeGain { amount }
                        | CardEffect::OnPlayGainPerCount { amount, .. }
                        | CardEffect::AfterImprovementPlayedGain { amount, .. }
                        | CardEffect::OnPlayGainAnimal { amount, .. }
                        | CardEffect::AfterWorkerGainAnimal { amount, .. } => *amount,
                        CardEffect::AfterHarvestGain { amount, .. }
                        | CardEffect::AfterHarvestGainIfAtLeast { amount, .. }
                        | CardEffect::StartRoundGainIfFamilyAndRooms { amount, .. }
                        | CardEffect::StartRoundGainIfLessonsOccupants { amount, .. }
                        | CardEffect::StartRoundGainIfAccumulationGrew { amount, .. }
                        | CardEffect::StartRoundGainIfAccumulationAtLeast { amount, .. }
                        | CardEffect::StartRoundGainIfNoHarvestAndAnimals { amount, .. }
                        | CardEffect::OnPlayGainIfRoomCount { amount, .. } => *amount,
                        CardEffect::OnPlayBuildStable { amount, .. } => *amount,
                        CardEffect::OnPlayGainEachWithAtLeast { amount, .. } => *amount,
                        CardEffect::AfterWorkerGainIfOwnsAnimal { amount, .. } => *amount,
                        CardEffect::AfterWorkerGainIfFencesAtLeastRound { amount, .. } => *amount,
                        CardEffect::AfterWorkerGainIfAnimalAccumulationAtLeast {
                            amount, ..
                        } => *amount,
                        CardEffect::AfterHarvestChooseGainIfAtLeast {
                            amount,
                            upgrade_amount,
                            ..
                        } => (*amount).max(*upgrade_amount),
                        CardEffect::HarvestFieldOptionalExchange { gain_amount, .. } => {
                            *gain_amount
                        }

                        CardEffect::AfterAnyWorkerGain { amount, .. }
                        | CardEffect::AfterAnyWorkerOthersGain { amount, .. }
                        | CardEffect::HarvestStartGainIfFields { amount, .. } => *amount,
                        CardEffect::AfterAnyWorkerGainByCountIfNotActor {
                            base_amount: amount,
                            ..
                        }
                        | CardEffect::OnPlayAddStoneToEmptyFields { amount, .. } => *amount,
                        // PostPlayPassLeft is a zone move, not a gain: no
                        // amount to bound.
                        CardEffect::PostPlayPassLeft => 1,
                        CardEffect::OnPlayChoiceGain { options } => {
                            options.iter().map(|o| o.amount).max().unwrap_or(1)
                        }
                        CardEffect::HarvestFieldGainIfGrainHarvestedAtLeast { amount, .. } => {
                            *amount
                        }
                        CardEffect::OnPlayBuildPasture { .. } => 1,
                        CardEffect::OnPlayDrawPlayOrPassOccupation { food_cost } => *food_cost,
                        CardEffect::OnPlayGainByHandOccupationCount { .. } => 1,
                        CardEffect::OnPlayExchangeGain {
                            give_amount,
                            receive_amount,
                            ..
                        } => (*give_amount).max(*receive_amount),
                        CardEffect::OnPlayExchange { receive, .. } => {
                            // Each receive resource represents 1 unit; validate count
                            receive.len().max(1) as u32
                        }
                        CardEffect::OnPlayGainOrExchangeAnimal {
                            gain_amount,
                            pay_amount,
                            ..
                        } => (*gain_amount).max(*pay_amount),
                        // A140: the amount is read from the public
                        // accumulations at resolution time, not fixed here.
                        // Chapter 1 round-start cards: no direct resource amount to validate
                        CardEffect::AfterWorkerGainOtherAccumulationFood { .. }
                        | CardEffect::StartRoundPlowFieldIfHouse { .. }
                        | CardEffect::StartRoundCopyUnusedRoundSpace { .. }
                        | CardEffect::StartRoundPlayOccupationIfLessonsUnderused { .. }
                        | CardEffect::StartRoundPlayCardIfStoneHouse { .. }
                        | CardEffect::StartRoundGainIfGoodsOnRoundSpaces { .. }
                        | CardEffect::StartRoundExchangeForFoodAndPoints { .. }
                        | CardEffect::StartRoundPlaceFoodOnNextRoundSpaces { .. } => 1,
                        CardEffect::HarvestFeedingGain { amount, .. } => *amount,
                        CardEffect::HarvestFeedingOptionalExchange { options, .. } => options
                            .iter()
                            .map(|option| {
                                option.receive_food + if option.receive_animal { 1 } else { 0 }
                            })
                            .max()
                            .unwrap_or(1),
                    };
                    amount == 0 || amount > 30
                })
                || card.effects.iter().any(|effect| {
                    matches!(effect, CardEffect::OnPlayGainPerStored { divisor: 0, .. })
                        || matches!(
                            effect,
                            CardEffect::HarvestFieldGain {
                                rule: HarvestFieldRule::Animals { sheep_per: 0, .. }
                                    | HarvestFieldRule::Animals { cattle_per: 0, .. },
                                ..
                            }
                        )
                })
            {
                return Err("unsupported card cost, points, or effect".into());
            }
        }
        for card in &self.definitions {
            // Parse effects to find prerequisite guards for each effect.
            // A consecutive run of player-count prerequisites
            // (MinimumPlayers/MaximumPlayers) guards every following effect
            // UP TO the next player-count run or the end of the list. A
            // trailing run with no following effect is a whole-card
            // requirement. Non-player prerequisites (occupation/animal/field
            // counts) guard only the immediately following effect and are
            // evaluated at play time, not here.
            let Some(strict_err) = players_guarded_effects_valid(&card.id, &card.effects, players)
            else {
                continue;
            };
            return Err(strict_err);
        }
        for kind in [CardKind::Occupation, CardKind::MinorImprovement] {
            let count = self.definitions.iter().filter(|c| c.kind == kind).count();
            // Minimum: one complete 7-card hand of each kind per player.
            // The 2-player verified preset (16 occ / 19 minor >= 14) meets the full
            // 7*players requirement. 3-4 player catalogs must have sufficient cards
            // to deal full hands; if they don't, the deck is invalid without new
            // card implementations (P1-2 scope excludes adding cards).
            if count < 7 * players {
                return Err(
                    "implemented card set needs at least seven cards of each kind per player"
                        .into(),
                );
            }
        }
        Ok(())
    }

    /// Project this card set to a specific player count, exactly once, at a
    /// game entry point (PR #368 close verdict: 人数投影を全ゲーム入口で固定し
    /// 未投影セットを拒否).
    ///
    /// A consecutive run of player-count prerequisites guards every following
    /// effect up to the next player-count run or the end of the list, and a
    /// trailing run is a whole-card requirement — the same grammar
    /// `CardSet::validate` enforces. Effects guarded for a different player
    /// count are removed. A card that originally had effects but would lose
    /// ALL of them for this player count is not playable here: the set is
    /// unprojectable and rejected rather than silently kept as a dead card
    /// (raw A155 in a 2-player game is the motivating case).
    ///
    /// The projection is idempotent: a set already projected for its player
    /// count (the verified 2-player preset, the v5 save fixtures, every
    /// `card_set_for_game` set) has no unsatisfied guards and passes through
    /// unchanged, so content hashes and recorded replays are preserved.
    pub fn projected_for_players(&self, players: usize) -> Result<CardSet, String> {
        let mut definitions = Vec::with_capacity(self.definitions.len());
        for card in &self.definitions {
            let effects = project_effects_for_players(&card.effects, players);
            if !card.effects.is_empty() && effects.is_empty() {
                return Err(format!(
                    "card {} has no active effects at {players} players",
                    card.id
                ));
            }
            definitions.push(CardDefinition {
                use_limit: card.use_limit,
                id: card.id.clone(),
                kind: card.kind,
                cost: card.cost.clone(),
                points: card.points,
                effects,
                // Preserve the printed pass-left destination: projection only
                // filters effects by player count, it never changes where the
                // card goes after play.
                post_play: card.post_play,
            });
        }
        Ok(CardSet { definitions })
    }
}

/// Whether a player-count requirement player guard is satisfied for `players`.
fn players_guard_satisfied(rule: &CardRequirement, players: usize) -> bool {
    match rule {
        CardRequirement::MinimumPlayers(min) => *min != 0 && usize::from(*min) <= players,
        CardRequirement::MaximumPlayers(max) => *max != 0 && usize::from(*max) >= players,
        _ => true,
    }
}

/// One segment of the player-count guard grammar: a consecutive run of
/// MinimumPlayers/MaximumPlayers prerequisites (`guards`) plus everything that
/// run covers (`covered`): every following effect up to the next player-count
/// run or the end of the list, including non-player play-time prerequisites
/// that stay attached to the effect they precede. The leading segment before
/// any player-count prerequisite has no guards.
#[derive(Default)]
struct PlayerGuardSegment {
    guards: Vec<CardEffect>,
    covered: Vec<CardEffect>,
}

impl PlayerGuardSegment {
    fn has_player_guard(&self) -> bool {
        !self.guards.is_empty()
    }

    fn guards_satisfied(&self, players: usize) -> bool {
        self.guards.iter().all(|effect| match effect {
            CardEffect::Prerequisite { rule } => players_guard_satisfied(rule, players),
            _ => true,
        })
    }

    fn regular_effect_count(&self) -> usize {
        self.covered
            .iter()
            .filter(|effect| !matches!(effect, CardEffect::Prerequisite { .. }))
            .count()
    }
}

/// Segment a card's effect list into player-count guard runs: consecutive
/// player-count prerequisites merge into one run, and a run ends at the next
/// player-count prerequisite once it has covered at least one effect.
fn player_guard_segments(effects: &[CardEffect]) -> Vec<PlayerGuardSegment> {
    let is_player_guard = |rule: &CardRequirement| {
        matches!(
            rule,
            CardRequirement::MinimumPlayers(_) | CardRequirement::MaximumPlayers(_)
        )
    };
    let mut segments: Vec<PlayerGuardSegment> = Vec::new();
    let mut current = PlayerGuardSegment {
        guards: Vec::new(),
        covered: Vec::new(),
    };
    for effect in effects {
        if let CardEffect::Prerequisite { rule } = effect {
            if is_player_guard(rule) {
                if !current.covered.is_empty() {
                    segments.push(std::mem::take(&mut current));
                }
                current.guards.push(effect.clone());
                continue;
            }
            // Non-player prerequisite: play-time guard on the next effect;
            // stays with the effect it precedes.
            current.covered.push(effect.clone());
            continue;
        }
        current.covered.push(effect.clone());
    }
    segments.push(current);
    segments
}

/// Strict player-count guard check for `CardSet::validate` (PR #368 close
/// verdict: 人数投影を全ゲーム入口で固定し未投影セットを拒否). A set handed to
/// a game entry is projected by [`CardSet::projected_for_players`] first, so
/// an unsatisfied player-count guard at validation time means an unprojected
/// (raw) set reached validation and must be rejected instead of being treated
/// as validated.
///
/// Grammar, shared with the entry projection: a consecutive run of
/// MinimumPlayers/MaximumPlayers prerequisites guards every following effect
/// up to the next player-count run or the end of the list; effects before the
/// first player-count run are unguarded; a trailing run with no following
/// regular effect is a whole-card requirement. Non-player prerequisites are
/// play-time checks on the effect they precede and are ignored here.
fn players_guarded_effects_valid(
    card_id: &str,
    effects: &[CardEffect],
    players: usize,
) -> Option<String> {
    for segment in player_guard_segments(effects) {
        if !segment.has_player_guard() || segment.guards_satisfied(players) {
            continue;
        }
        if segment.regular_effect_count() > 0 {
            return Some(format!(
                "card {card_id} carries an effect guarded for a different player count"
            ));
        }
        return Some(format!(
            "card {card_id} is not eligible for this player count"
        ));
    }
    None
}

/// Project a card's effects to `players` with the shared guard grammar
/// (PR #368: every game entry projects exactly once). A run whose guards are
/// unsatisfied loses its covered effects together with the run itself;
/// satisfied runs stay in place, so the projection is idempotent on sets that
/// were already projected (the verified 2-player preset, v5 save fixtures,
/// `card_set_for_game` output). A trailing run covering no effect is a
/// whole-card requirement: it is kept so [`CardSet::validate`] rejects the
/// set instead of silently stripping the requirement.
pub(crate) fn project_effects_for_players(
    effects: &[CardEffect],
    players: usize,
) -> Vec<CardEffect> {
    let mut result: Vec<CardEffect> = Vec::with_capacity(effects.len());
    for segment in player_guard_segments(effects) {
        let keep = !segment.has_player_guard()
            || segment.guards_satisfied(players)
            || segment.regular_effect_count() == 0;
        if keep {
            result.extend(segment.guards);
            result.extend(segment.covered);
        }
    }
    result
}

/// Private: must never be serialized into a player's Observation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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

/// Verify card affordability using aggregate costs (including occupation fees).
fn card_costs(
    card: &CardDefinition,
    player: &Player,
    extra_food: u32,
) -> std::collections::BTreeMap<Good, u32> {
    let mut amounts = std::collections::BTreeMap::<Good, u32>::new();
    for &(good, amount) in &card.cost {
        *amounts.entry(good).or_default() += amount;
    }
    if card.kind == CardKind::MinorImprovement {
        if let Some(wood) = amounts.get_mut(&Good::Wood) {
            *wood = wood.saturating_sub(player.improvement_wood_discount);
        }
    }
    if card.kind == CardKind::MinorImprovement {
        if let Some(stone) = amounts.get_mut(&Good::Stone) {
            *stone = stone.saturating_sub(player.stone_cost_discount);
        }
    }
    *amounts.entry(Good::Food).or_default() += extra_food;
    amounts
}
pub fn affordable(card: &CardDefinition, player: &Player, extra_food: u32) -> bool {
    card_costs(card, player, extra_food)
        .iter()
        .all(|(&good, &amount)| player.supply.get(good) >= amount)
}

pub(crate) fn occupation_count(set: &CardSet, player: &Player) -> usize {
    player
        .played_cards
        .iter()
        .filter(|id| {
            set.definition(id)
                .is_some_and(|card| card.kind == CardKind::Occupation)
        })
        .count()
}

/// Count clay rooms (rooms in clay or stone house).
pub(crate) fn clay_room_count(player: &Player) -> usize {
    if matches!(player.house, House::Clay | House::Stone) {
        player.rooms()
    } else {
        0
    }
}

/// Count major improvements in the bottom row of the major-improvement supply board.
///
/// The 2016 Revised supply board lays major improvements out in rows by cost;
/// its bottom row holds exactly the three good-conversion improvements:
/// Joinery, Pottery and Basketry. `major_improvement_bottom_row` in
/// `docs/bga-card-effect-vocabulary.md` names this count source; the member
/// set is pinned by the B007 / D030 test contracts (B007-normal: bottom row 2
/// -> food_delta 4). This composition is not derivable from repo-internal spec
/// data alone, so it should be confirmed against the official board before
/// shipping (noted as an open rule question in PR t_b954c21d).
pub(crate) fn major_improvements_bottom_row(player: &Player) -> usize {
    player
        .improvements
        .iter()
        .filter(|imp| {
            matches!(
                imp,
                Improvement::Joinery | Improvement::Pottery | Improvement::Basketry
            )
        })
        .count()
}

/// Count sown fields: fields with a crop planted, plus fields carrying C006
/// stones (`field_stones > 0`) that count as cultivated until the field phase
/// reset harvests the stones.
pub(crate) fn sown_field_count(player: &Player) -> usize {
    player
        .farm
        .iter()
        .filter(|cell| {
            matches!(cell.tile, Tile::Field { crop: Some(_), .. }) || cell.field_stones > 0
        })
        .count()
}

pub fn can_play(set: &CardSet, card: &CardDefinition, player: &Player, extra_food: u32) -> bool {
    if !affordable(card, player, extra_food) {
        return false;
    }
    let occupations = occupation_count(set, player);
    card.effects.iter().all(|effect| {
        let CardEffect::Prerequisite { rule } = effect else {
            return true;
        };
        match rule {
            CardRequirement::MinimumOccupations(min) => occupations >= usize::from(*min),
            CardRequirement::MaximumOccupations(max) => occupations <= usize::from(*max),
            CardRequirement::MinimumAnimals { animal, number } => {
                player.animal_count(*animal) >= *number
            }
            CardRequirement::MinimumCropFields { crop, number } => {
                player
                    .farm
                    .iter()
                    .filter(|cell| {
                        matches!(cell.tile, Tile::Field { crop: Some(found), .. } if found == *crop)
                            || cell.field_stones > 0
                    })
                    .count()
                    >= usize::from(*number)
            }
            CardRequirement::MinimumImprovements(min) => {
                improvement_count_in_front(set, player) >= u32::from(*min)
            }
            CardRequirement::MinimumFences(min) => player.fences.len() >= usize::from(*min),
            CardRequirement::MinimumPlayers(_) => true,
            CardRequirement::MaximumPlayers(_) => true,
            CardRequirement::PastureCount(n) => player.pastures.len() >= usize::from(*n),
            // A068 アスパラガスの贈り物: empty fields are plowed fields with no
            // crop and no C006 stone (a stone field counts as sown).
            CardRequirement::MinimumEmptyFields(min) => {
                player
                    .farm
                    .iter()
                    .filter(|cell| {
                        matches!(cell.tile, Tile::Field { crop: None, .. })
                            && cell.field_stones == 0
                    })
                    .count()
                    >= usize::from(*min)
            }
        }
    })
}

fn safe_gain(player: &mut Player, good: Good, amount: u32) -> Result<(), String> {
    let next = player
        .supply
        .get(good)
        .checked_add(amount)
        .ok_or("card resource overflow")?;
    player.supply.set(good, next);
    Ok(())
}

/// Execute one prevalidated card play; the containing Game transaction provides rollback.
/// This function validates the play, pays costs, applies on-play effects (Before phase),
/// moves the card from hand to played, and updates card_points.
/// This is the canonical public API used by evaluation, policy, and unit tests.
pub fn play(
    set: &CardSet,
    hand: &mut Vec<String>,
    player: &mut Player,
    id: &str,
    kind: CardKind,
    extra_food: u32,
) -> Result<(), String> {
    let card = set.definition(id).ok_or("unknown card")?;
    if card.kind != kind || !hand.iter().any(|x| x == id) {
        return Err("card is not in the player's hand or is the wrong kind".into());
    }
    if !can_play(set, card, player, extra_food) {
        return Err("cannot pay card costs or prerequisites are unmet".into());
    }
    for (good, amount) in card_costs(card, player, extra_food) {
        player.supply.pay(good, amount)?;
    }
    // Apply on-play (Before phase) effects
    apply_on_play_effects(set, player, id)?;
    // Move card from hand to played via the single canonical implementation
    crate::card_zones::hand_to_played(hand, player, id).map_err(|e| e.to_string())?;
    // Update card points
    let next_points = player
        .card_points
        .checked_add(card.points)
        .ok_or("card points overflow")?;
    player.card_points = next_points;
    Ok(())
}

/// Internal: validate and pay costs only (no effects, no move, no points).
/// Used by the engine path which then routes through card_zones and dispatch_play.
pub(crate) fn play_validate_pay(
    set: &CardSet,
    hand: &mut [String],
    player: &mut Player,
    id: &str,
    kind: CardKind,
    extra_food: u32,
) -> Result<(), String> {
    let card = set.definition(id).ok_or("unknown card")?;
    if card.kind != kind || !hand.iter().any(|x| x == id) {
        return Err("card is not in the player's hand or is the wrong kind".into());
    }
    if !can_play(set, card, player, extra_food) {
        return Err("cannot pay card costs or prerequisites are unmet".into());
    }
    for (good, amount) in card_costs(card, player, extra_food) {
        player.supply.pay(good, amount)?;
    }
    Ok(())
}

/// Apply on-play (Before phase) effects for a card that has been moved to played.
/// Called by the event dispatcher after the card is in the played zone.
pub fn apply_on_play_effects(set: &CardSet, player: &mut Player, id: &str) -> Result<(), String> {
    let card = set.definition(id).ok_or("unknown card")?;
    for effect in &card.effects {
        match effect {
            CardEffect::OnPlayGain { good, amount } => {
                safe_gain(player, *good, *amount)?;
            }
            CardEffect::OnPlayGainPerStored {
                stored,
                divisor,
                good,
                amount,
            } => {
                let bonus = (player.supply.get(*stored) / *divisor)
                    .checked_mul(*amount)
                    .ok_or("on-play card bonus overflow")?;
                safe_gain(player, *good, bonus)?;
            }
            CardEffect::OnPlayGainIfHouse {
                house,
                rooms,
                good,
                amount,
            } if player.house == *house && player.rooms() == usize::from(*rooms) => {
                safe_gain(player, *good, *amount)?;
            }
            // B073 贈り物かご: only the table row whose room count matches the
            // owner exactly resolves; a non-matching row falls through to the
            // catch-all below and grants nothing.
            CardEffect::OnPlayGainIfRoomCount {
                rooms,
                good,
                amount,
            } if player.rooms() == usize::from(*rooms) => {
                safe_gain(player, *good, *amount)?;
            }
            // A001/C002/E001: free (or card-priced) stable builds; sites are
            // picked deterministically from the legal candidates (A001: the
            // cell of a fenced one-cell pasture only).
            CardEffect::OnPlayBuildStable {
                amount,
                wood_cost,
                scope,
            } => {
                for _ in 0..*amount {
                    let Some(cell) = crate::engine::free_stable_site_candidates(player, *scope)
                        .into_iter()
                        .next()
                    else {
                        break;
                    };
                    crate::engine::build_free_stable(player, cell, *wood_cost)?;
                }
            }
            CardEffect::OnPlayGainEachWithAtLeast {
                goods,
                threshold,
                amount,
            } => {
                for good in goods {
                    if player.supply.get(*good) >= *threshold {
                        safe_gain(player, *good, *amount)?;
                    }
                }
            }
            CardEffect::OnPlayGainPerImprovementPlayed { good, amount } => {
                let bonus = improvement_count_in_front(set, player)
                    .checked_mul(*amount)
                    .ok_or("on-play card bonus overflow")?;
                safe_gain(player, *good, bonus)?;
            }
            CardEffect::OnPlayGainPerCount {
                count_kind,
                good,
                amount,
            } => {
                let count = match count_kind {
                    CountKind::Occupations => occupation_count(set, player) as u32,
                    CountKind::ClayRooms => clay_room_count(player) as u32,
                    CountKind::Improvements => improvement_count_in_front(set, player),
                    CountKind::FamilyMembers => player.family as u32,
                    CountKind::MajorImprovementsBottomRow => {
                        major_improvements_bottom_row(player) as u32
                    }
                    CountKind::SownFields => sown_field_count(player) as u32,
                };
                let bonus = count
                    .checked_mul(*amount)
                    .ok_or("on-play card bonus overflow")?;
                safe_gain(player, *good, bonus)?;
            }
            // B002 小牧場 (Mini Pasture): free single-cell pasture, cell picked
            // deterministically from the legal candidates (player state only).
            CardEffect::OnPlayBuildPasture { spaces } => {
                for _ in 0..*spaces {
                    let Some(cell) = crate::engine::free_single_cell_pasture_candidates(player)
                        .into_iter()
                        .next()
                    else {
                        break;
                    };
                    crate::engine::build_free_single_cell_pasture(player, cell)?;
                }
            }
            // B008 露店 (Market Stall): conditional exchange — no payment
            // resource, no exchange (and never a play failure).
            CardEffect::OnPlayExchangeGain {
                give,
                give_amount,
                receive,
                receive_amount,
            } => {
                if player.supply.get(*give) >= *give_amount {
                    player.supply.pay(*give, *give_amount)?;
                    safe_gain(player, *receive, *receive_amount)?;
                }
            }
            // A036 ファサード彫刻 / D009 獲物の交換: on-play exchange (simplified
            // legacy path). For give_is_cost=true, the cost was already paid;
            // just grant receive. For give_is_cost=false, deduct give and
            // grant receive (A036). Note: optional and cap are not fully
            // handled in this simplified path; the full handler is in
            // card_events.rs.
            CardEffect::OnPlayExchange {
                give,
                receive,
                give_is_cost,
                optional: _optional,
                cap: _cap,
            } => {
                if !give_is_cost {
                    // Deduct give resources
                    for give_res in give {
                        match give_res {
                            ExchangeResource::Good(g) => {
                                player.supply.pay(*g, 1)?;
                            }
                            ExchangeResource::Animal(a) => {
                                player.remove_animals(*a, 1)?;
                            }
                            ExchangeResource::Points => {}
                        }
                    }
                }
                // Grant receive resources
                for receive_res in receive {
                    match receive_res {
                        ExchangeResource::Good(g) => {
                            let next = player
                                .supply
                                .get(*g)
                                .checked_add(1)
                                .ok_or("resource overflow")?;
                            player.supply.set(*g, next);
                        }
                        ExchangeResource::Animal(a) => {
                            player.add_animal(*a)?;
                        }
                        ExchangeResource::Points => {
                            player.card_points = player
                                .card_points
                                .checked_add(1)
                                .ok_or("card points overflow")?;
                        }
                    }
                }
            } // B009 むち (Beating Rod): gain, or pay to gain one animal.
            // Deterministic policy: exchange whenever the payment resource is
            // available; the animal is placed through the derived plan (same
            // vocabulary as OnPlayGainAnimal, overflow discarded).
            CardEffect::OnPlayGainOrExchangeAnimal {
                gain_good,
                gain_amount,
                pay_good,
                pay_amount,
                animal,
            } => {
                if player.supply.get(*pay_good) >= *pay_amount {
                    player.supply.pay(*pay_good, *pay_amount)?;
                    let plan = crate::card_events::auto_animal_plan(player, *animal, 1);
                    crate::engine::place_animals(player, &plan, 1)
                        .map_err(|e| format!("on-play animal placement failed: {e}"))?;
                } else {
                    safe_gain(player, *gain_good, *gain_amount)?;
                }
            }
            // B003 (OnPlayDrawPlayOrPassOccupation) and B005
            // (OnPlayGainByHandOccupationCount) read the owner's HAND, which
            // this hand-less projection path cannot see. They are resolved by
            // the play dispatcher's handlers (card_events.rs) and intentionally
            // no-op here, exactly like the A-deck's hand-dependent A003.
            CardEffect::Passive { rule } => match rule {
                PassiveRule::PastureCapacity(n) => player.pasture_capacity_bonus += n,
                PassiveRule::FenceWoodDiscount(n) => player.free_fence_wood += n,
                PassiveRule::ImprovementWoodDiscount(n) => player.improvement_wood_discount += n,
                PassiveRule::WoodenRoomCost(n) => player.wooden_room_cost = Some(*n),
                PassiveRule::RoomCost(n) => player.room_material_cost = Some(*n),
                PassiveRule::MajorStoneDiscountByRooms => {
                    player.major_stone_discount_by_rooms = true
                }
                PassiveRule::StoneDiscount(n) => player.stone_cost_discount += n,
                PassiveRule::HousingSpace(n) => player.extra_housing += n,
                PassiveRule::AllowMajorViaMinor { .. } => {}
            },
            CardEffect::OnPlayAddStoneToEmptyFields { good: _, amount } => {
                for cell in &mut player.farm {
                    if matches!(cell.tile, Tile::Field { crop: None, .. }) {
                        cell.field_stones = cell.field_stones.saturating_add(*amount);
                    }
                }
            }
            CardEffect::OnPlayChoiceGain { options } => {
                if let Some(opt) = options.first() {
                    let amount = match opt.per_animal {
                        Some(animal) => opt.amount.saturating_mul(player.animal_count(animal)),
                        None => opt.amount,
                    };
                    safe_gain(player, opt.good, amount)?;
                }
            }
            CardEffect::PostPlayPassLeft => {
                // Zone-move metadata: the engine play arm performs the hand
                // pass after the play transaction; nothing to resolve here.
            }
            _ => {}
        }
    }
    Ok(())
}

/// Update card_points after a card is successfully played.
/// Called after move_card and effect dispatch.
pub fn update_card_points_on_play(player: &mut Player, points: i32) -> Result<(), String> {
    let next_points = player
        .card_points
        .checked_add(points)
        .ok_or("card points overflow")?;
    player.card_points = next_points;
    Ok(())
}

/// Deterministic, already-public effects on a recorded worker action.
pub fn after_worker(set: &CardSet, player: &mut Player, space: Space) -> Result<(), String> {
    for id in player.played_cards.clone() {
        let card = set.definition(&id).ok_or("unknown played card")?;
        for effect in &card.effects {
            if let CardEffect::AfterWorkerGain {
                space: trigger,
                good,
                amount,
            } = effect
            {
                if *trigger == space {
                    safe_gain(player, *good, *amount)?;
                }
            }
            if let CardEffect::AfterWorkerGainIfOwnsAnimal {
                space: trigger,
                animal,
                good,
                amount,
            } = effect
            {
                if *trigger == space && player.animal_count(*animal) > 0 {
                    safe_gain(player, *good, *amount)?;
                }
            }
        }
    }
    Ok(())
}

/// Calculated at read time from public played cards; it is never added to the
/// immutable printed-points counter and therefore cannot be counted twice.
pub fn end_game_bonus(set: &CardSet, player: &Player) -> i32 {
    end_game_bonus_for_definitions(&set.definitions, player)
}

/// Same computation as [`end_game_bonus`] over a borrowed definition slice.
///
/// The evaluation hot path owns only `Observation::card_definitions`, and
/// rebuilding a `CardSet` there would clone every definition once per candidate
/// action. This entry point keeps the lookup borrowed so search cost does not
/// scale with the size of the card pool.
pub fn end_game_bonus_for_definitions(definitions: &[CardDefinition], player: &Player) -> i32 {
    let definition = |id: &String| definitions.iter().find(|card| &card.id == id);
    player
        .played_cards
        .iter()
        .filter_map(definition)
        .flat_map(|card| card.effects.iter())
        .map(|effect| match effect {
            CardEffect::EndGameBonus { rule } => match rule {
                ScoreBonusRule::PastureArea => {
                    let squares = player.pastures.iter().map(|p| p.cells.len()).sum::<usize>();
                    [6, 7, 8, 10].iter().filter(|&&n| squares >= n).count() as i32
                }
                ScoreBonusRule::HouseMaterial => match player.house {
                    House::Wood => 3,
                    House::Clay => 2,
                    House::Stone => 0,
                },
                ScoreBonusRule::UnfencedStables => player
                    .farm
                    .iter()
                    .filter(|cell| cell.stable && !matches!(cell.tile, Tile::Pasture { .. }))
                    .count() as i32,
                ScoreBonusRule::ImprovementCount => {
                    let minor = player
                        .played_cards
                        .iter()
                        .filter(|id| {
                            definition(id)
                                .is_some_and(|card| card.kind == CardKind::MinorImprovement)
                        })
                        .count();
                    let total = minor + player.improvements.len();
                    match total {
                        0..=4 => 0,
                        5 => 2,
                        6 => 3,
                        7 => 4,
                        8 => 5,
                        9 => 7,
                        _ => 9,
                    }
                }
                ScoreBonusRule::SparePastureCapacity => player
                    .pastures
                    .iter()
                    .filter(|pasture| {
                        pasture.quantity > 0
                            && player
                                .pasture_capacity(pasture)
                                .saturating_sub(pasture.quantity)
                                >= 3
                    })
                    .count() as i32,
                ScoreBonusRule::LaterOccupations => {
                    let tutor = player.played_cards.iter().position(|id| {
                        definition(id).is_some_and(|card| {
                            card.effects.iter().any(|effect| {
                                matches!(
                                    effect,
                                    CardEffect::EndGameBonus {
                                        rule: ScoreBonusRule::LaterOccupations
                                    }
                                )
                            })
                        })
                    });
                    tutor.map_or(0, |index| {
                        player.played_cards[index + 1..]
                            .iter()
                            .filter(|id| {
                                definition(id).is_some_and(|card| card.kind == CardKind::Occupation)
                            })
                            .count() as i32
                    })
                }
                ScoreBonusRule::CategoriesAtMaxFour => {
                    let categories = &crate::score::score(player).categories;
                    [
                        "fields",
                        "pastures",
                        "grain",
                        "vegetables",
                        "sheep",
                        "boar",
                        "cattle",
                        "fenced_stables",
                    ]
                    .iter()
                    .filter(|name| categories.get(**name).copied().unwrap_or(0) >= 4)
                    .count() as i32
                }
                ScoreBonusRule::LargePastures => (player
                    .pastures
                    .iter()
                    .filter(|pasture| pasture.cells.len() >= 3)
                    .count() as i32)
                    .saturating_mul(2),
                ScoreBonusRule::CookingImprovements => player
                    .improvements
                    .iter()
                    .filter(|improvement| improvement.is_cooking())
                    .count() as i32,
                ScoreBonusRule::FieldsGoodsBeforeFinalHarvest(threshold) => {
                    // Snapshot taken at end of final harvest field phase; if missing, treat as 0.
                    let goods = player.field_goods_before_final_harvest.unwrap_or(0);
                    if goods >= *threshold {
                        2
                    } else {
                        0
                    }
                }
                ScoreBonusRule::StoneRoomsExceedPeople => {
                    let stone_rooms = if matches!(player.house, House::Stone) {
                        player.rooms()
                    } else {
                        0
                    };
                    if stone_rooms > player.family as usize {
                        4
                    } else {
                        0
                    }
                }
                ScoreBonusRule::FarmyardAllUsed => {
                    // A farm cell counts as used when it carries a non-Empty
                    // tile or a stable is built on it: a stable-on-empty space
                    // (tile Empty, stable true) is occupied by the stable, so
                    // it must not be counted as unused.
                    let all_used = player
                        .farm
                        .iter()
                        .all(|cell| !matches!(cell.tile, Tile::Empty) || cell.stable);
                    if all_used {
                        2
                    } else {
                        0
                    }
                }
                ScoreBonusRule::ExactFamilyMembers => match player.family {
                    4 => 2,
                    3 => 3,
                    2 => 5,
                    _ => 0,
                },
                ScoreBonusRule::MajorImprovementsBottomRowThresholds => {
                    // Bottom row only (Joinery/Pottery/Basketry), never the
                    // total major-improvement count.
                    match major_improvements_bottom_row(player) {
                        3 => 2,
                        4 => 5,
                        n if n >= 5 => 8,
                        _ => 0,
                    }
                }
                ScoreBonusRule::LivestockThresholds(threshold) => {
                    // D035: 1 point per `threshold` own livestock of any
                    // species (sheep, boar, cattle).
                    let livestock = player
                        .animal_count(Animal::Sheep)
                        .saturating_add(player.animal_count(Animal::Boar))
                        .saturating_add(player.animal_count(Animal::Cattle));
                    livestock.saturating_div(*threshold) as i32
                }
                ScoreBonusRule::MajorImprovementsLowestDoubledThresholds => {
                    // B153 寮長 (Housemaster): the lowest single score is
                    // doubled (exactly one, even when the lowest score is
                    // tied), then each of the thresholds 5/7/9/11 pays its
                    // point once, so the bonus is the count of thresholds
                    // met (1/2/3/4). No major improvement -> nothing to
                    // double and a total of 0 -> no bonus.
                    let mut scores: Vec<i32> = player
                        .improvements
                        .iter()
                        .map(|improvement| improvement.points())
                        .collect();
                    if let Some(lowest) = scores.iter_mut().min() {
                        *lowest *= 2;
                    }
                    let total: i32 = scores.iter().sum();
                    [5, 7, 9, 11].iter().filter(|&&t| total >= t).count() as i32
                }
                ScoreBonusRule::ColumnsWithRoom => {
                    // The farmyard is a 5-column x 3-row grid (see
                    // `model::neighbors`), so column c holds the cells c, c + 5
                    // and c + 10. A column scores once when it carries at least
                    // one room, no matter how many rooms stand in it.
                    const COLUMNS: usize = 5;
                    (0..COLUMNS)
                        .filter(|&column| {
                            player
                                .farm
                                .iter()
                                .skip(column)
                                .step_by(COLUMNS)
                                .any(|cell| matches!(cell.tile, Tile::Room))
                        })
                        .count() as i32
                }
            },
            _ => 0,
        })
        .sum()
}

#[cfg(test)]
mod card_prerequisite_tests {
    use super::*;

    fn prerequisite_card(rule: CardRequirement) -> (CardSet, CardDefinition) {
        let card = CardDefinition {
            use_limit: None,
            id: "check".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Wood, 1)],
            points: 0,
            effects: vec![CardEffect::Prerequisite { rule }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        };
        (
            CardSet {
                definitions: vec![card.clone()],
            },
            card,
        )
    }

    #[test]
    fn occupations_minimum_and_maximum_are_exact() {
        let (mut set, card) = prerequisite_card(CardRequirement::MinimumOccupations(2));
        set.definitions
            .extend(["one", "two"].into_iter().map(|id| CardDefinition {
                use_limit: None,
                id: id.into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![],
                post_play: crate::cards::PostPlayDestination::PlayedArea,
            }));
        let mut p = Player::new(0, 0);
        p.supply.wood = 1;
        p.played_cards.push("one".into());
        assert!(!can_play(&set, &card, &p, 0));
        p.played_cards.push("two".into());
        assert!(can_play(&set, &card, &p, 0));
        let (_, too_many) = prerequisite_card(CardRequirement::MaximumOccupations(1));
        assert!(!can_play(&set, &too_many, &p, 0));
    }

    #[test]
    fn five_sheep_required_for_a038() {
        let set = CardSet {
            definitions: verified_scoring_cards(2),
        };
        let card = set.definition("A038").unwrap();
        let mut p = Player::new(0, 0);
        p.pet = Some(Animal::Sheep);
        assert!(!can_play(&set, card, &p, 0));
        p.pastures.push(crate::model::Pasture {
            id: 0,
            cells: vec![2, 3, 4],
            animal: Some(Animal::Sheep),
            quantity: 4,
        });
        assert!(can_play(&set, card, &p, 0));
    }

    #[test]
    fn crop_field_prerequisite_counts_only_planted_type() {
        let (set, card) = prerequisite_card(CardRequirement::MinimumCropFields {
            crop: Crop::Grain,
            number: 2,
        });
        let mut p = Player::new(0, 0);
        p.supply.wood = 1;
        p.farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        };
        p.farm[3].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        assert!(!can_play(&set, &card, &p, 0));
        p.farm[3].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        };
        assert!(can_play(&set, &card, &p, 0));
    }

    #[test]
    fn c006_stone_fields_count_as_cultivated_until_harvest() {
        // C006 石取り作業: 石を置かれた空き畑は、次フィールドフェイズで
        // 石が収穫されるまで「栽培中」と数えられる。
        let set = CardSet {
            definitions: verified_band_c_cards(),
        };
        let mut p = Player::new(0, 0);
        p.supply.food = 1;
        p.played_cards.push("occ1".to_string());
        let mut set_with_occ = set.clone();
        set_with_occ.definitions.push(CardDefinition {
            use_limit: None,
            id: "occ1".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        });
        p.farm[0].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        p.farm[1].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        p.farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        assert_eq!(sown_field_count(&p), 1);
        play(
            &set_with_occ,
            &mut vec!["C006".to_string()],
            &mut p,
            "C006",
            CardKind::MinorImprovement,
            0,
        )
        .unwrap();
        assert_eq!(p.farm[0].field_stones, 1);
        assert_eq!(p.farm[1].field_stones, 1);
        // 1 畑に作物 + 2 畑に石 = 3 畑が栽培中
        assert_eq!(sown_field_count(&p), 3);
        // 収穫で石は消費され、栽培中扱いも解除される
        harvest_field_stones(&set_with_occ, &mut p).unwrap();
        assert_eq!(p.farm[0].field_stones, 0);
        assert_eq!(p.farm[1].field_stones, 0);
        assert_eq!(sown_field_count(&p), 1);
    }

    #[test]
    fn minimum_crop_fields_counts_stone_fields_as_cultivated() {
        let (set, card) = prerequisite_card(CardRequirement::MinimumCropFields {
            crop: Crop::Grain,
            number: 2,
        });
        let mut p = Player::new(0, 0);
        p.supply.wood = 1;
        p.farm[0].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        // 1 枚だけでは足りない
        assert!(!can_play(&set, &card, &p, 0));
        // 石を置かれた空き畑も栽培中とみなされ、要件を満たす
        p.farm[1].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        p.farm[1].field_stones = 1;
        assert!(can_play(&set, &card, &p, 0));
    }

    #[test]
    fn player_count_requirement_rejected_by_card_set() {
        let mut cards = verified_scoring_cards(3);
        cards.push(CardDefinition {
            use_limit: None,
            id: "extra".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::Prerequisite {
                rule: CardRequirement::MinimumPlayers(3),
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        });
        let set = CardSet { definitions: cards };
        assert!(set.validate(2).is_err());
    }
}

/// Deterministic public start-of-round rewards. Evaluates the owner's state
/// when the round begins, not when the card was originally played.
pub fn start_round_rewards(set: &CardSet, player: &mut Player) -> Result<(), String> {
    for id in player.played_cards.clone() {
        let card = set.definition(&id).ok_or("unknown played card")?;
        for effect in &card.effects {
            if let CardEffect::StartRoundGain {
                good,
                amount,
                house,
                rooms,
            } = effect
            {
                if house.is_none_or(|h| player.house == h)
                    && rooms.is_none_or(|r| player.rooms() == usize::from(r))
                {
                    safe_gain(player, *good, *amount)?;
                }
            } else if let CardEffect::StartRoundGainIfNoMinorPlayed { good, amount } = effect {
                // E111 世捨て人, round-start half: the condition is checked
                // here because it needs card definitions.
                if no_minor_improvements_played(set, player) {
                    safe_gain(player, *good, *amount)?;
                }
            }
        }
    }
    Ok(())
}

/// Vegetables the field phase is about to yield: one per planted vegetable
/// field whose crop is still growing (each yields exactly one per harvest).
/// Read pre-harvest state, before any crops are removed.
fn vegetable_yield(player: &Player) -> u32 {
    player
        .farm
        .iter()
        .filter(|cell| {
            matches!(
                cell.tile,
                Tile::Field {
                    crop: Some(Crop::Vegetable),
                    remaining: 1..,
                }
            )
        })
        .count() as u32
}

/// Grain fields the field phase is about to harvest: fields planted with
/// grain whose crop is still growing (one grain per field). Read pre-harvest
/// state, before any crops are removed. Supply grain and empty fields are
/// not counted.
fn grain_fields_growing(player: &Player) -> u32 {
    player
        .farm
        .iter()
        .filter(|cell| {
            matches!(
                cell.tile,
                Tile::Field {
                    crop: Some(Crop::Grain),
                    remaining: 1..,
                }
            )
        })
        .count() as u32
}

/// Called before taking crops from fields; livestock and field layouts still
/// represent the state at the start of the field phase.
pub fn harvest_field_rewards(set: &CardSet, player: &mut Player) -> Result<(), String> {
    for id in player.played_cards.clone() {
        let card = set.definition(&id).ok_or("unknown played card")?;
        for effect in &card.effects {
            if let CardEffect::HarvestFieldGain { rule, good, amount } = effect {
                let multiplier = match rule {
                    HarvestFieldRule::Animals {
                        sheep_per,
                        cattle_per,
                    } => (player.animal_count(Animal::Sheep) / *sheep_per)
                        .checked_add(player.animal_count(Animal::Cattle) / *cattle_per)
                        .ok_or("harvest animal bonus overflow")?,
                    HarvestFieldRule::MixedFields => {
                        let has = |crop: Option<Crop>| {
                            player.farm.iter().any(|cell| {
                            matches!(cell.tile, Tile::Field { crop: found, .. } if found == crop)
                        })
                        };
                        u32::from(has(Some(Crop::Grain)) && has(Some(Crop::Vegetable)) && has(None))
                    }
                    HarvestFieldRule::VegetablesHarvested => vegetable_yield(player),
                    HarvestFieldRule::HasWheatField => u32::from(player.farm.iter().any(|cell| {
                        matches!(
                            cell.tile,
                            Tile::Field {
                                crop: Some(Crop::Grain),
                                ..
                            }
                        )
                    })),
                    HarvestFieldRule::FieldCountThresholds {
                        thresholds,
                        amounts,
                    } => {
                        let fields = player
                            .farm
                            .iter()
                            .filter(|cell| matches!(cell.tile, Tile::Field { .. }))
                            .count() as u32;
                        let mut best = 0;
                        for (threshold, gained) in thresholds.iter().zip(amounts.iter()) {
                            if fields >= *threshold {
                                best = *gained;
                            }
                        }
                        best
                    }
                    HarvestFieldRule::LastGrainTaken | HarvestFieldRule::LastVegetableTaken => {
                        let crop = if rule == &HarvestFieldRule::LastGrainTaken {
                            Crop::Grain
                        } else {
                            Crop::Vegetable
                        };
                        player
                            .farm
                            .iter()
                            .filter(|cell| {
                                matches!(
                                    cell.tile,
                                    Tile::Field {
                                        crop: Some(found),
                                        remaining: 1,
                                    } if found == crop
                                )
                            })
                            .count() as u32
                    }
                    HarvestFieldRule::Fixed => 1,
                    // A112 大鎌使い: one step per growing grain field (the
                    // same pre-harvest count the dispatcher handler uses).
                    HarvestFieldRule::GrainFieldsGrowing => grain_fields_growing(player),
                };
                safe_gain(
                    player,
                    *good,
                    multiplier
                        .checked_mul(*amount)
                        .ok_or("harvest bonus overflow")?,
                )?;
            } else if let CardEffect::HarvestFieldGainIfNoMinorPlayed { good, amount } = effect {
                // E111 世捨て人, harvest half: the condition is checked here
                // because it needs card definitions.
                if no_minor_improvements_played(set, player) {
                    safe_gain(player, *good, *amount)?;
                }
            } else if let CardEffect::HarvestFieldGainIfGrainHarvestedAtLeast {
                threshold,
                good,
                amount,
            } = effect
            {
                // D065 麦ふるい: same pre-harvest equivalence as the dispatcher
                // handler (card_events.rs) — this legacy path also runs before
                // crops are taken, so growing grain fields (remaining >= 1)
                // equal the grain this field phase yields, one per field.
                let grain_fields = player
                    .farm
                    .iter()
                    .filter(|cell| {
                        matches!(
                            cell.tile,
                            Tile::Field {
                                crop: Some(Crop::Grain),
                                remaining: 1..,
                            }
                        )
                    })
                    .count() as u32;
                if grain_fields >= *threshold {
                    safe_gain(player, *good, *amount)?;
                }
            }
        }
    }
    // Harvest field_stones placed by C006 石取り作業 / Stone Clearing:
    // stones on fields are converted to stone goods at the field phase,
    // and the field_stones counter is reset. Single source of truth — also
    // called from the engine's `harvest_fields` (production path).
    harvest_field_stones(set, player)?;
    Ok(())
}

/// Apply one gain option of an `OnPlayChoiceGain` effect (C007 刈り鋏):
/// the option at `chosen` is resolved against the player's current state.
/// `None` (out of range) fails closed. This is the single resolution point
/// shared by the engine's `PlayMinorChoice` arm and tests.
pub fn apply_choice_gain(
    player: &mut Player,
    options: &[ChoiceGainOption],
    chosen: usize,
) -> Result<(), String> {
    let opt = options
        .get(chosen)
        .ok_or("choice index out of range for OnPlayChoiceGain")?;
    let amount = match opt.per_animal {
        Some(animal) => opt.amount.saturating_mul(player.animal_count(animal)),
        None => opt.amount,
    };
    safe_gain(player, opt.good, amount)
}

/// Collect stones placed on the player's fields (C006 石取り作業):
/// each stone becomes 1 stone good for the owner and the per-field
/// counter is reset. Shared by the engine field phase and the legacy
/// `harvest_field_rewards` wrapper so both path families agree.
pub fn harvest_field_stones(_set: &CardSet, player: &mut Player) -> Result<(), String> {
    let stones_to_harvest: Vec<u32> = player
        .farm
        .iter()
        .map(|cell| cell.field_stones)
        .filter(|&s| s > 0)
        .collect();
    for cell in &mut player.farm {
        cell.field_stones = 0;
    }
    for stones in stones_to_harvest {
        safe_gain(player, Good::Stone, stones)?;
    }
    Ok(())
}

/// True when the player has played no minor improvements (E111 世捨て人
/// condition). Improvements handed over by other players (cards not played by
/// this player) do not appear in played_cards, so they are naturally excluded.
fn no_minor_improvements_played(set: &CardSet, player: &Player) -> bool {
    player.played_cards.iter().all(|id| {
        set.definition(id)
            .is_none_or(|card| card.kind != CardKind::MinorImprovement)
    })
}

/// Work-phase rewards that depend on a different space already being occupied.
pub fn after_worker_if_used(
    set: &CardSet,
    player: &mut Player,
    space: Space,
    used: &BTreeSet<Space>,
) -> Result<(), String> {
    for id in player.played_cards.clone() {
        let card = set.definition(&id).ok_or("unknown played card")?;
        for effect in &card.effects {
            if let CardEffect::AfterWorkerGainIfUsed {
                space: trigger,
                required_space,
                good,
                amount,
            } = effect
            {
                if *trigger == space && used.contains(required_space) {
                    safe_gain(player, *good, *amount)?;
                }
            }
        }
    }
    Ok(())
}

/// One-time reward on the completion of qualifying clay construction.
pub fn after_clay_construction(
    set: &CardSet,
    previous: &Player,
    current: &mut Player,
) -> Result<(), String> {
    let qualifies = (previous.house == House::Clay && current.rooms() > previous.rooms())
        || (previous.house == House::Clay && current.house == House::Stone);
    if !qualifies {
        return Ok(());
    }
    for id in current.played_cards.clone() {
        let card = set.definition(&id).ok_or("unknown played card")?;
        for effect in &card.effects {
            if let CardEffect::AfterClayConstructionGain { good, amount } = effect {
                safe_gain(current, *good, *amount)?;
            }
        }
    }
    Ok(())
}

/// Includes ONLY cards whose effects are actually implemented. The official
/// 7+7 hands require 14 cards of each kind at two players.
pub fn verified_basic_card_set(players: usize) -> Result<CardSet, String> {
    if players != 2 {
        return Err("built-in A/B card pool currently supports two players only".into());
    }
    let mut definitions = Vec::new();
    definitions.extend(verified_scoring_cards(players));
    definitions.extend(verified_minor_worker_cards(players));
    // Exclude expansion (E-prefixed) and D-deck cards from the A/B verified pool.
    definitions.extend(
        verified_immediate_resource_cards()
            .into_iter()
            .filter(|c| !c.id.starts_with('E') && !c.id.starts_with('D')),
    );
    definitions.extend(verified_worker_bonus_cards(players));
    // A145 綱作り is printed for 3+ players (spec eligibility "2": "no"), so
    // keep it out of the built-in two-player pool. PR #385 review finding (e).
    // A157 ボヘミアン is printed for 4 players only (spec eligibility "2"/"3":
    // "no", "4": "yes"), so it gets the same treatment.
    definitions.extend(
        verified_round_and_harvest_cards()
            .into_iter()
            .filter(|c| c.id != "A145" && c.id != "A157"),
    );
    definitions.extend(verified_passive_cards());
    definitions.extend(verified_simple_occupations(players));
    definitions.extend(verified_extra_simple_cards());
    let set = CardSet { definitions };
    set.validate(players)?;
    Ok(set)
}

/// Improvements in front of a player: base improvements plus played
/// minor-improvement cards (A117 木材調達人 condition).
pub fn improvement_count_in_front(set: &CardSet, player: &Player) -> u32 {
    let minor = player
        .played_cards
        .iter()
        .filter(|id| {
            set.definition(id)
                .is_some_and(|card| card.kind == CardKind::MinorImprovement)
        })
        .count() as u32;
    minor + player.improvements.len() as u32
}

/// Extra food when a player takes Bake Bread in a round directly following a
/// harvest round. The work action is already validated and applied.
pub fn after_harvest_baking(
    set: &CardSet,
    player: &mut Player,
    round: u8,
    baked: bool,
) -> Result<(), String> {
    if !baked || round < 2 || !crate::model::HARVEST_ROUNDS.contains(&(round - 1)) {
        return Ok(());
    }
    for id in player.played_cards.clone() {
        let card = set.definition(&id).ok_or("unknown played card")?;
        for effect in &card.effects {
            if let CardEffect::AfterHarvestBakeGain { amount } = effect {
                safe_gain(player, Good::Food, *amount)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod a063_baking_tests {
    use super::*;
    #[test]
    fn after_harvest_bread_action_only_and_exact_three_food() {
        let set = CardSet {
            definitions: verified_extra_simple_cards(),
        };
        let mut player = Player::new(0, 0);
        player.played_cards.push("A063".into());
        after_harvest_baking(&set, &mut player, 4, true).unwrap();
        after_harvest_baking(&set, &mut player, 5, false).unwrap();
        after_harvest_baking(&set, &mut player, 6, true).unwrap();
        assert_eq!(player.supply.food, 0);
        for round in [5, 8, 10, 12, 14] {
            after_harvest_baking(&set, &mut player, round, true).unwrap();
        }
        assert_eq!(player.supply.food, 15);
    }
    #[test]
    fn correct_improvement_cost_and_card_points() {
        let set = CardSet {
            definitions: verified_extra_simple_cards(),
        };
        let mut player = Player::new(0, 0);
        player.supply.wood = 2;
        player.supply.stone = 2;
        let mut hand = vec!["A063".into()];
        play(
            &set,
            &mut hand,
            &mut player,
            "A063",
            CardKind::MinorImprovement,
            0,
        )
        .unwrap();
        assert_eq!(player.card_points, 2);
        assert_eq!(player.supply.wood, 0);
        assert_eq!(player.supply.stone, 0);
    }
}

#[cfg(test)]
mod card_set_content_hash_tests {
    use super::*;

    fn card(id: &str, kind: CardKind, points: i32) -> CardDefinition {
        CardDefinition {
            use_limit: None,
            id: id.into(),
            kind,
            cost: Vec::new(),
            points,
            effects: vec![CardEffect::OnPlayGain {
                good: Good::Food,
                amount: 1,
            }],
            post_play: crate::cards::PostPlayDestination::PlayedArea,
        }
    }

    #[test]
    fn empty_and_populated_sets_have_distinct_hashes() {
        let empty = CardSet::default();
        let one = CardSet {
            definitions: vec![card("X1", CardKind::Occupation, 1)],
        };
        assert_eq!(empty.content_hash().len(), 64);
        assert_ne!(empty.content_hash(), one.content_hash());
        // Deterministic across calls.
        assert_eq!(one.content_hash(), one.content_hash());
    }

    #[test]
    fn hash_ignores_definition_order_but_tracks_every_field() {
        let a = card("X1", CardKind::Occupation, 1);
        let b = card("X2", CardKind::MinorImprovement, 2);
        let forward = CardSet {
            definitions: vec![a.clone(), b.clone()],
        };
        let reverse = CardSet {
            definitions: vec![b.clone(), a.clone()],
        };
        assert_eq!(forward.content_hash(), reverse.content_hash());

        // Each individual field change must change the hash.
        let mut renamed = a.clone();
        renamed.id = "X9".into();
        let mut rekinded = a.clone();
        rekinded.kind = CardKind::MinorImprovement;
        let mut rescored = a.clone();
        rescored.points = 7;
        let mut recosted = a.clone();
        recosted.cost = vec![(Good::Wood, 2)];
        let mut reeffected = a.clone();
        reeffected.effects = vec![CardEffect::EndGameBonus {
            rule: ScoreBonusRule::PastureArea,
        }];
        for changed in [renamed, rekinded, rescored, recosted, reeffected] {
            let other = CardSet {
                definitions: vec![changed],
            };
            assert_ne!(
                forward.content_hash(),
                other.content_hash(),
                "a changed field must change the deck hash"
            );
        }
    }

    #[test]
    fn verified_two_player_pool_hashes_stably() {
        let first = verified_basic_card_set(2).unwrap();
        let second = verified_basic_card_set(2).unwrap();
        assert!(!first.definitions.is_empty());
        assert_eq!(first.content_hash(), second.content_hash());
        // The hash must not depend on how the pool was assembled.
        let mut shuffled = first.clone();
        shuffled.definitions.reverse();
        assert_eq!(first.content_hash(), shuffled.content_hash());
    }
}

/// Get the default immutable card registry (built once, shared via Arc).
///
/// This is the primary entry point for new code. The registry contains all
/// implemented cards with stable CardId assignments and validated definitions.
pub fn default_registry() -> Arc<CardRegistry> {
    build_default_registry().expect("default registry must build")
}

/// Get a CardPool for the given player count and mode.
///
/// - `experimental_mode = false`: Official pool (verified + eligible only, fail-closed)
/// - `experimental_mode = true`: Experimental pool (all implemented cards)
pub fn card_pool(players: usize, experimental_mode: bool) -> CardPool {
    CardPool::new(&default_registry(), players, experimental_mode)
}

/// Get a legacy CardSet for the given player count and mode (compat adapter).
///
/// This builds a CardSet from the pool's eligible cards, suitable for
/// existing APIs that expect CardSet (saves, observations, legacy engine paths).
pub fn card_set_for_game(players: usize, experimental_mode: bool) -> CardSet {
    let registry = default_registry();
    let pool = card_pool(players, experimental_mode);
    card_set_from_pool(&registry, &pool)
}

/// Get a legacy CardSet with ALL implemented cards (for full-catalog observations).
pub fn card_set_full_catalog() -> CardSet {
    card_set_from_registry(&default_registry())
}

/// Look up a card definition by external ID from the default registry.
pub fn card_definition(external_id: &str) -> Option<CardDefinition> {
    default_registry()
        .get_by_external(external_id)
        .map(|c| c.to_card_definition())
}

/// Look up a card definition by CardId from the default registry.
pub fn card_definition_by_id(card_id: CardId) -> Option<CardDefinition> {
    default_registry()
        .get(card_id)
        .map(|c| c.to_card_definition())
}

/// Convert external ID to CardId using the default registry.
pub fn external_to_card_id(external_id: &str) -> Option<CardId> {
    default_registry().card_id(external_id)
}

/// Convert CardId to external ID using the default registry.
pub fn card_id_to_external(card_id: CardId) -> Option<String> {
    default_registry()
        .external_id(card_id)
        .map(|s| s.to_string())
}

#[cfg(test)]
mod p1_2_repro_tests {
    use super::*;
    use crate::card_pool::builtin_implemented_cards;
    use crate::model::Good;

    #[test]
    fn repro_builtin_vs_registry_count() {
        let builtin = builtin_implemented_cards();
        let registry = default_registry();

        println!("builtin_implemented_cards count: {}", builtin.len());
        println!("registry count: {}", registry.len());

        let builtin_ids: std::collections::BTreeSet<_> =
            builtin.iter().map(|c| c.id.clone()).collect();
        let registry_ids: std::collections::BTreeSet<_> =
            registry.iter().map(|c| c.external_id.clone()).collect();

        let missing: Vec<_> = registry_ids.difference(&builtin_ids).collect();
        let extra: Vec<_> = builtin_ids.difference(&registry_ids).collect();

        println!("Missing from builtin: {:?}", missing);
        println!("Extra in builtin: {:?}", extra);

        // Both must carry exactly the same catalog. The hard count is
        // intentionally not pinned: the implemented catalog grows as cards
        // land on main (65 at the original P1-2 audit, 75 after the M7-C
        // implementation wave merged there); what must not drift is the
        // builtin/registry identity itself.
        assert_eq!(
            builtin.len(),
            registry.len(),
            "builtin_implemented_cards and registry must cover the same cards"
        );
        assert!(
            missing.is_empty(),
            "builtin should not miss any registry cards"
        );
        assert!(extra.is_empty(), "builtin should not have extra cards");
    }

    #[test]
    fn debug_four_player_minor_improvements() {
        // Informational: the 4-player experimental pool deals partial hands because
        // the minor improvement count is below 7*4=28. This test documents the
        // current count without enforcing a specific number (which would brittle).
        let registry = default_registry();
        let pool = crate::registry::CardPool::new(&registry, 4, true);
        let minor_ids: Vec<_> = pool.eligible_kind(&registry, CardKind::MinorImprovement);
        println!(
            "4p experimental minors (implemented catalog): {}",
            minor_ids.len()
        );
        // No assertion - this is an informational test only
    }

    #[test]
    fn repro_card_set_for_game_validate() {
        for players in [2, 3, 4] {
            for experimental in [false, true] {
                let set = card_set_for_game(players, experimental);
                let occ = set
                    .definitions
                    .iter()
                    .filter(|c| c.kind == CardKind::Occupation)
                    .count();
                let minor = set
                    .definitions
                    .iter()
                    .filter(|c| c.kind == CardKind::MinorImprovement)
                    .count();
                let result = set.validate(players);
                println!(
                    "card_set_for_game({}, {}): {} ({} cards, occ={}, minor={})",
                    players,
                    experimental,
                    if result.is_ok() { "OK" } else { "FAIL" },
                    set.definitions.len(),
                    occ,
                    minor
                );
                if let Err(e) = &result {
                    println!("  Error: {}", e);
                }
                // After fix, all should pass
                assert!(
                    result.is_ok(),
                    "card_set_for_game({}, {}) should validate: {}",
                    players,
                    experimental,
                    result.unwrap_err()
                );
            }
        }
    }

    #[test]
    fn repro_b121_effects_per_player_count() {
        for players in [2, 3, 4] {
            let set = card_set_for_game(players, true);
            if let Some(b121) = set.definition("B121") {
                let active: Vec<_> = b121
                    .effects
                    .iter()
                    .filter(|e| !matches!(e, CardEffect::Prerequisite { .. }))
                    .collect();
                let has_claypit = b121.effects.iter().any(|e| matches!(e, CardEffect::AfterWorkerGain { space, .. } if *space == Space::ClayPit));
                println!(
                    "B121 at {} players: {} total effects ({} active), has ClayPit: {}",
                    players,
                    b121.effects.len(),
                    active.len(),
                    has_claypit
                );

                if players == 2 {
                    assert_eq!(
                        active.len(),
                        2,
                        "B121 at 2 players should have 2 active effects"
                    );
                    assert!(!has_claypit, "B121 at 2 players should NOT have ClayPit");
                } else {
                    assert_eq!(
                        active.len(),
                        3,
                        "B121 at {} players should have 3 active effects",
                        players
                    );
                    assert!(
                        has_claypit,
                        "B121 at {} players should have ClayPit",
                        players
                    );
                }
            } else {
                panic!("B121 at {} players: NOT FOUND", players);
            }
        }
    }

    #[test]
    fn repro_b102_per_player_count() {
        for players in [2, 3, 4] {
            let set = card_set_for_game(players, true);
            if let Some(b102) = set.definition("B102") {
                let active: Vec<_> = b102
                    .effects
                    .iter()
                    .filter(|e| !matches!(e, CardEffect::Prerequisite { .. }))
                    .collect();
                println!(
                    "B102 at {} players: {} total effects ({} active)",
                    players,
                    b102.effects.len(),
                    active.len()
                );

                match players {
                    2 => {
                        assert_eq!(active.len(), 1);
                        assert!(matches!(
                            &active[0],
                            CardEffect::OnPlayGain {
                                good: Good::Clay,
                                amount: 3
                            }
                        ));
                    }
                    3 => {
                        assert_eq!(active.len(), 1);
                        assert!(matches!(
                            &active[0],
                            CardEffect::OnPlayGain {
                                good: Good::Reed,
                                amount: 2
                            }
                        ));
                    }
                    4 => {
                        // 4 players: Sheep 2 through the existing OnPlayGainAnimal
                        // on-play auto-placement path (C083's vocabulary).
                        assert_eq!(active.len(), 1);
                        assert!(matches!(
                            &active[0],
                            CardEffect::OnPlayGainAnimal {
                                animal: Animal::Sheep,
                                amount: 2
                            }
                        ));
                    }
                    _ => {}
                }
            } else {
                panic!("B102 at {players} players: NOT FOUND (must exist at 2/3/4)");
            }
        }
    }

    /// Registry-merge forms (B121/B102 via `merge_card_variants`) carry
    /// player-count prerequisite guards that the cards.rs raw variants do not,
    /// so their projected effects are pinned per player count instead of being
    /// compared against the raw catalog.
    fn pinned_registry_effects(players: usize, id: &str) -> Option<Vec<CardEffect>> {
        let gain = |space: Space, good: Good, amount: u32| CardEffect::AfterWorkerGain {
            space,
            good,
            amount,
        };
        match id {
            "B102" => match players {
                2 => Some(vec![
                    CardEffect::Prerequisite {
                        rule: CardRequirement::MinimumPlayers(2),
                    },
                    CardEffect::Prerequisite {
                        rule: CardRequirement::MaximumPlayers(2),
                    },
                    CardEffect::OnPlayGain {
                        good: Good::Clay,
                        amount: 3,
                    },
                ]),
                3 => Some(vec![
                    CardEffect::Prerequisite {
                        rule: CardRequirement::MinimumPlayers(3),
                    },
                    CardEffect::Prerequisite {
                        rule: CardRequirement::MaximumPlayers(3),
                    },
                    CardEffect::OnPlayGain {
                        good: Good::Reed,
                        amount: 2,
                    },
                ]),
                4 => Some(vec![
                    CardEffect::Prerequisite {
                        rule: CardRequirement::MinimumPlayers(4),
                    },
                    CardEffect::Prerequisite {
                        rule: CardRequirement::MaximumPlayers(4),
                    },
                    CardEffect::OnPlayGainAnimal {
                        animal: Animal::Sheep,
                        amount: 2,
                    },
                ]),
                _ => None,
            },
            "B121" => Some({
                let mut effects = vec![
                    gain(Space::Forest, Good::Clay, 1),
                    gain(Space::ReedBank, Good::Clay, 1),
                ];
                if players >= 3 {
                    effects.push(CardEffect::Prerequisite {
                        rule: CardRequirement::MinimumPlayers(3),
                    });
                    effects.push(gain(Space::ClayPit, Good::Clay, 1));
                }
                effects
            }),
            "D035" => match players {
                2 => Some(vec![
                    CardEffect::Prerequisite {
                        rule: CardRequirement::MaximumPlayers(2),
                    },
                    CardEffect::EndGameBonus {
                        rule: ScoreBonusRule::LivestockThresholds(7),
                    },
                ]),
                3 => Some(vec![
                    CardEffect::Prerequisite {
                        rule: CardRequirement::MinimumPlayers(3),
                    },
                    CardEffect::Prerequisite {
                        rule: CardRequirement::MaximumPlayers(3),
                    },
                    CardEffect::EndGameBonus {
                        rule: ScoreBonusRule::LivestockThresholds(5),
                    },
                ]),
                4 => Some(vec![
                    CardEffect::Prerequisite {
                        rule: CardRequirement::MinimumPlayers(4),
                    },
                    CardEffect::EndGameBonus {
                        rule: ScoreBonusRule::LivestockThresholds(4),
                    },
                ]),
                _ => None,
            },
            _ => None,
        }
    }

    #[test]
    fn repro_all_cards_per_player_count_parity() {
        use std::collections::BTreeMap;

        let registry = default_registry();
        // 実測 @ b10a61a: デフォルトレジストリ = 77 枚
        // (2026-11-21 の Actions main+A109 マージビルドでは 78 枚)。
        // 総数は main のカード追加で変動しうるためここでは固定しない: 主アサーションは下の
        // field-parity ループ（cards.rs 生定義 vs registry 投影の id/kind/cost/points/effects、
        // 欠落・過剰ゼロ）であり、レジストリ組込自体は repro_builtin_vs_registry_count で
        // builtin_implemented_cards との一致を別途保証する。
        let _ = registry.len();

        // cards.rs 生カタログ: external_id -> (person count -> raw CardDefinition)
        let mut catalog: BTreeMap<String, BTreeMap<usize, CardDefinition>> = BTreeMap::new();
        let mut insert_cards = |players: usize, cards: &[CardDefinition]| {
            for card in cards {
                let entry = catalog.entry(card.id.clone()).or_default();
                assert!(
                    entry.insert(players, card.clone()).is_none(),
                    "cards.rs catalog duplicate: {} at {players} players",
                    card.id
                );
            }
        };
        for players in [2usize, 3, 4] {
            insert_cards(players, &verified_scoring_cards(players));
            insert_cards(players, &verified_minor_worker_cards(players));
            insert_cards(players, &verified_worker_bonus_cards(players));
            insert_cards(players, &verified_simple_occupations(players));
            insert_cards(players, &verified_multiplayer_passive_cards(players));
        }
        let count_independent: [Vec<CardDefinition>; 12] = [
            verified_immediate_resource_cards(),
            verified_round_and_harvest_cards(),
            verified_passive_cards(),
            verified_extra_simple_cards(),
            // Chapter 1 round-start cards (A090/A151/A152/B097/B106/B135/C064)
            verified_round_start_cards(),
            verified_band_a_cards(),
            verified_band_b_cards(),
            verified_band_c_cards(),
            verified_band_d_cards(),
            verified_band_e_cards(),
            // on_play build_stable cards (A001/C002/E001)
            verified_build_stable_cards(),
            // B-deck chapter 1: on-play simple cards (B002/B003/B005/B008/B009)
            verified_b_onplay_simple_cards(),
        ];
        for cards in count_independent {
            for players in [2usize, 3, 4] {
                insert_cards(players, &cards);
            }
        }

        for players in [2usize, 3, 4] {
            let pool = CardPool::new(&registry, players, true);
            let set = card_set_for_game(players, true);
            assert!(
                set.validate(players).is_ok(),
                "card_set_for_game({players}, true) must validate"
            );

            let occ = set
                .definitions
                .iter()
                .filter(|c| c.kind == CardKind::Occupation)
                .count();
            let minor = set
                .definitions
                .iter()
                .filter(|c| c.kind == CardKind::MinorImprovement)
                .count();
            // 実測 @ b10a61a: 2p = 72 枚 (occ 34 / minor 38)、3p = 76 枚 (occ 38 / minor 38)、
            // 4p = 76 枚 (occ 38 / minor 38)。main のカード追加（2026-11-21 の Actions マージ
            // ビルドでは A109 により 73/77/77）で変動しうるため、ここでは総数を固定しない。
            // 主アサーションは下の field-parity ループ（cards.rs 生定義 vs registry 投影の
            // id/kind/cost/points/effects、欠落・過剰ゼロ）であり、数はコメントの
            // 実測値を参照する。
            let _ = (occ, minor);

            // --- cards.rs 生定義 vs registry 投影 (id, kind, cost, points, effects) ---
            for (id, raw_by_count) in &catalog {
                let Some(raw) = raw_by_count.get(&players) else {
                    continue;
                };
                let reg = registry.get_by_external(id).unwrap_or_else(|| {
                    panic!("{id} is in the cards.rs catalog but missing from the default registry")
                });
                let def = reg.to_card_definition();
                assert_eq!(def.kind, raw.kind, "{id} kind mismatch at {players}p");
                assert_eq!(def.cost, raw.cost, "{id} cost mismatch at {players}p");
                assert_eq!(def.points, raw.points, "{id} points mismatch at {players}p");

                if pool.contains(reg.id) {
                    let expected_effects =
                        pinned_registry_effects(players, id).unwrap_or_else(|| raw.effects.clone());
                    let actual_effects = reg.to_card_definition_for_players(players).effects;
                    assert_eq!(
                        actual_effects, expected_effects,
                        "{id} effects at {players}p diverge from the cards.rs raw definition \
                     (registry-merge forms B121/B102 are pinned exceptions)"
                    );
                }
            }

            // --- registry プール eligibility vs cards.rs カタログ ---
            let mut unexpected_eligible = Vec::new();
            for reg in registry.iter() {
                let in_catalog = catalog
                    .get(&reg.external_id)
                    .map(|raw_by_count| raw_by_count.contains_key(&players))
                    .unwrap_or(false);
                if pool.contains(reg.id) && !in_catalog {
                    unexpected_eligible.push(reg.external_id.clone());
                }
            }
            assert!(
            unexpected_eligible.is_empty(),
            "registry-eligible but absent from cards.rs catalog at {players}p: {unexpected_eligible:?}"
        );

            let ineligible: Vec<_> = registry
                .iter()
                .filter(|c| !pool.contains(c.id))
                .map(|c| c.external_id.clone())
                .collect();
            println!("{players}p pool-ineligible cards: {ineligible:?}");
        }

        // 修正1: `filter_effects_for_players` を run-grammar に統一後の 2p 投影。
        // 統一前は A155 -> [Grain@TravelingPlayers]、B144 -> [Wood@Hollow, Reed@ClayPit] と
        // 守衛直後の1効果のみを守衛して無防備化する設計欠陥があった。正しくは全員 []。
        for id in ["A155", "B142", "B144"] {
            let effects = registry
                .get_by_external(id)
                .unwrap_or_else(|| panic!("{id} missing from registry"))
                .to_card_definition_for_players(2)
                .effects;
            assert!(
            effects.is_empty(),
            "{id} 2-player projection must be [] after unifying with the run grammar, got {effects:?}"
        );
        }

        // 2p プリセットのハッシュは D054（main 側 +1 minor）と D026/E024（本 PR +2 minors）
        // の追加により 39 -> 41 defs（content_hash 再計測済み、意図的変更）。
        let preset = verified_basic_card_set(2).unwrap();
        assert_eq!(
            preset.content_hash(),
            "2a78347a92fd2a940a5ca7258ddc3a4971378cde3d74f730c41602b0313b26fa",
            "2-player preset content hash (StartRoundGainIf* wave: re-measured after B114/A049/C067/D054 joined the preset and A157 was excluded as 4p-only)"
        );

        // 実測の pool 不適格セット @ b10a61a（main 追加で変動可）。
        // 2p: [A133, A143, A155, B142, B144]（A133/A143 は 3人+、A155 は 4人のみ、B142/B144 は 3人+ のみ；
        // Round 2 指摘対象は A155/B142/B144）。3p: [A155] のみ。4p: B102 は sheep2
        // 実装により eligible になり不適格セットから外れた。
        // A109（main 2026-11-21 追加）は全人数 eligible なのでいずれの不適格セットにも入らない。
        // 主アサーションは「cards.rs カタログと registry の person-count 整合」の欠落/過剰ゼロ
        //（上のループの field-parity チェック）であり、総数・不適格セットの厳密な列挙は
        // コメントの実測値を参照する。
        let pool2 = CardPool::new(&registry, 2, true);
        let ineligible_2: Vec<_> = registry
            .iter()
            .filter(|c| !pool2.contains(c.id))
            .map(|c| c.external_id.clone())
            .collect();
        for id in ["A133", "A143", "A155", "B142", "B144"] {
            assert!(
                ineligible_2.contains(&id.to_string()),
                "{id} must be pool-ineligible at 2 players"
            );
        }
        let pool3 = CardPool::new(&registry, 3, true);
        let ineligible_3: Vec<_> = registry
            .iter()
            .filter(|c| !pool3.contains(c.id))
            .map(|c| c.external_id.clone())
            .collect();
        assert!(
            ineligible_3.contains(&"A155".to_string()),
            "A155 must be pool-ineligible at 3 players"
        );
        let pool4 = CardPool::new(&registry, 4, true);
        let ineligible_4: Vec<_> = registry
            .iter()
            .filter(|c| !pool4.contains(c.id))
            .map(|c| c.external_id.clone())
            .collect();
        // B102 女性顧問 is now eligible at 4 players too (4p = Sheep 2 via the
        // existing OnPlayGainAnimal on-play auto-placement path), so it appears
        // in every player-count pool instead of being the only 4p dropout.
        let b102_id = registry
            .card_id("B102")
            .expect("B102 is in the default registry");
        assert!(
            !ineligible_4.contains(&"B102".to_string()),
            "B102 must be pool-eligible at 4 players (4p = sheep 2)"
        );
        assert!(
            pool2.contains(b102_id) && pool3.contains(b102_id) && pool4.contains(b102_id),
            "B102 must be pool-eligible at 2, 3 and 4 players"
        );
    }
}

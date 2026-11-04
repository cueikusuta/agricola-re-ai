//! Breeding is a decision made AFTER feeding, once per eligible species.
//! Decisions are public and replayable; the engine validates their destinations.
use crate::model::{Animal, Player, Tile};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BirthDestination {
    Pasture(u8),
    Stable(usize),
    Pet,
    Discard,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BirthChoice {
    pub species: Animal,
    pub destination: BirthDestination,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreedingPlan {
    pub births: Vec<BirthChoice>,
}

impl BreedingPlan {
    pub fn simple(player: &Player) -> Self {
        let mut preview = player.clone();
        let mut births = Vec::new();
        for species in [Animal::Sheep, Animal::Boar, Animal::Cattle] {
            if preview.animal_count(species) < 2 {
                continue;
            }
            let location = preview
                .pastures
                .iter()
                .find(|pasture| {
                    (pasture.animal.is_none() || pasture.animal == Some(species))
                        && pasture.quantity < preview.pasture_capacity(pasture)
                })
                .map(|pasture| BirthDestination::Pasture(pasture.id))
                .or_else(|| {
                    preview.farm.iter().position(|cell| {
                        cell.stable
                            && matches!(cell.tile, Tile::Empty)
                            && cell.stable_animal.is_none()
                    }).map(BirthDestination::Stable)
                })
                .unwrap_or_else(|| {
                    if preview.pet.is_none() {
                        BirthDestination::Pet
                    } else {
                        BirthDestination::Discard
                    }
                });
            let choice = BirthChoice { species, destination: location };
            apply_one(&mut preview, &choice).expect("simple placement must be legal");
            births.push(choice);
        }
        Self { births }
    }

    pub fn apply(&self, player: &mut Player) -> Result<(), String> {
        let eligible: BTreeSet<_> = [Animal::Sheep, Animal::Boar, Animal::Cattle]
            .into_iter()
            .filter(|species| player.animal_count(*species) >= 2)
            .collect();
        let selected: BTreeSet<_> = self.births.iter().map(|birth| birth.species).collect();
        if selected != eligible || selected.len() != self.births.len() {
            return Err("one birth decision required for each breeding species".into());
        }
        let mut changed = player.clone();
        for birth in &self.births {
            apply_one(&mut changed, birth)?;
        }
        *player = changed;
        Ok(())
    }
}

fn apply_one(player: &mut Player, birth: &BirthChoice) -> Result<(), String> {
    match birth.destination {
        BirthDestination::Pasture(id) => {
            let index = player.pastures.iter().position(|p| p.id == id)
                .ok_or("birth pasture not found")?;
            let pasture = &player.pastures[index];
            if pasture.animal.is_some() && pasture.animal != Some(birth.species) {
                return Err("different species cannot share a pasture".into());
            }
            if pasture.quantity >= player.pasture_capacity(pasture) {
                return Err("birth exceeds pasture capacity".into());
            }
            let target = &mut player.pastures[index];
            target.quantity += 1;
            target.animal = Some(birth.species);
        }
        BirthDestination::Stable(index) => {
            let cell = player.farm.get_mut(index).ok_or("invalid stable index")?;
            if !cell.stable || !matches!(cell.tile, Tile::Empty)
                || cell.stable_animal.is_some()
            {
                return Err("unfenced stable is not empty".into());
            }
            cell.stable_animal = Some(birth.species);
        }
        BirthDestination::Pet => {
            if player.pet.is_some() {
                return Err("house already contains a pet".into());
            }
            player.pet = Some(birth.species);
        }
        BirthDestination::Discard => {
            // The newborn is returned to supply, never turned into food here.
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Pasture, Player, Tile};

    fn two_species() -> Player {
        let mut p = Player::new(0, 0);
        p.pet = Some(Animal::Sheep);
        p.farm[13].stable = true;
        p.farm[13].stable_animal = Some(Animal::Sheep);
        p.farm[14].stable = true;
        p.pastures.push(Pasture {
            id: 0, cells: vec![12], animal: Some(Animal::Boar), quantity: 2,
        });
        p.farm[12].tile = Tile::Pasture { id: 0 };
        p
    }

    #[test]
    fn birth_locations_are_policy_choices_not_fixed_priority() {
        let mut p = two_species();
        let custom = BreedingPlan { births: vec![
            BirthChoice { species: Animal::Sheep, destination: BirthDestination::Stable(14) },
            BirthChoice { species: Animal::Boar, destination: BirthDestination::Discard },
        ] };
        custom.apply(&mut p).unwrap();
        assert_eq!(p.animal_count(Animal::Sheep), 3);
        assert_eq!(p.animal_count(Animal::Boar), 2);
        assert_eq!(p.farm[14].stable_animal, Some(Animal::Sheep));
    }

    #[test]
    fn illegal_birth_choices_are_atomic() {
        let mut p = two_species();
        let before = p.clone();
        let invalid = BreedingPlan { births: vec![
            BirthChoice { species: Animal::Sheep, destination: BirthDestination::Pasture(0) },
            BirthChoice { species: Animal::Boar, destination: BirthDestination::Discard },
        ] };
        assert!(invalid.apply(&mut p).is_err());
        assert_eq!(p, before);
        assert!(BreedingPlan { births: vec![] }.apply(&mut p).is_err());
        assert!(BreedingPlan { births: vec![
            BirthChoice { species: Animal::Sheep, destination: BirthDestination::Discard },
            BirthChoice { species: Animal::Sheep, destination: BirthDestination::Discard },
        ] }.apply(&mut p).is_err());
        assert_eq!(p, before);
    }
}

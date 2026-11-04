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

/// Optional legal rearrangement of existing animals after feeding and
/// before breeding. This is not a second worker action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimalRearrangement {
    pub pasture: Vec<(u8, Option<Animal>, u32)>,
    pub stables: Vec<(usize, Animal)>,
    pub pet: Option<Animal>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreedingPlan {
    pub births: Vec<BirthChoice>,
    #[serde(default)]
    pub before_birth: Option<AnimalRearrangement>,
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
                    pasture.animal == Some(species)
                        && pasture.quantity < preview.pasture_capacity(pasture)
                })
                .or_else(|| {
                    preview.pastures.iter().find(|pasture| {
                        pasture.animal.is_none()
                            && pasture.quantity < preview.pasture_capacity(pasture)
                    })
                })
                .map(|pasture| BirthDestination::Pasture(pasture.id))
                .or_else(|| {
                    preview
                        .farm
                        .iter()
                        .position(|cell| {
                            cell.stable
                                && matches!(cell.tile, Tile::Empty)
                                && cell.stable_animal.is_none()
                        })
                        .map(BirthDestination::Stable)
                })
                .unwrap_or_else(|| {
                    if preview.pet.is_none() {
                        BirthDestination::Pet
                    } else {
                        BirthDestination::Discard
                    }
                });
            let choice = BirthChoice {
                species,
                destination: location,
            };
            apply_one(&mut preview, &choice).expect("simple placement must be legal");
            births.push(choice);
        }
        Self {
            births,
            before_birth: None,
        }
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
        if let Some(arrange) = &self.before_birth {
            crate::engine::rearrange_animals(
                &mut changed,
                &arrange.pasture,
                &arrange.stables,
                arrange.pet,
            )?;
        }
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
            let index = player
                .pastures
                .iter()
                .position(|p| p.id == id)
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
            if !cell.stable || !matches!(cell.tile, Tile::Empty) || cell.stable_animal.is_some() {
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
            id: 0,
            cells: vec![12],
            animal: Some(Animal::Boar),
            quantity: 2,
        });
        p.farm[12].tile = Tile::Pasture { id: 0 };
        p
    }

    #[test]
    fn birth_locations_are_policy_choices_not_fixed_priority() {
        let mut p = two_species();
        let custom = BreedingPlan {
            births: vec![
                BirthChoice {
                    species: Animal::Sheep,
                    destination: BirthDestination::Stable(14),
                },
                BirthChoice {
                    species: Animal::Boar,
                    destination: BirthDestination::Discard,
                },
            ],
            before_birth: None,
        };
        custom.apply(&mut p).unwrap();
        assert_eq!(p.animal_count(Animal::Sheep), 3);
        assert_eq!(p.animal_count(Animal::Boar), 2);
        assert_eq!(p.farm[14].stable_animal, Some(Animal::Sheep));
    }

    #[test]
    fn illegal_birth_choices_are_atomic() {
        let mut p = two_species();
        let before = p.clone();
        let invalid = BreedingPlan {
            births: vec![
                BirthChoice {
                    species: Animal::Sheep,
                    destination: BirthDestination::Pasture(0),
                },
                BirthChoice {
                    species: Animal::Boar,
                    destination: BirthDestination::Discard,
                },
            ],
            before_birth: None,
        };
        assert!(invalid.apply(&mut p).is_err());
        assert_eq!(p, before);
        assert!(BreedingPlan {
            births: vec![],
            before_birth: None,
        }
        .apply(&mut p)
        .is_err());
        assert!(BreedingPlan {
            births: vec![
                BirthChoice {
                    species: Animal::Sheep,
                    destination: BirthDestination::Discard
                },
                BirthChoice {
                    species: Animal::Sheep,
                    destination: BirthDestination::Discard
                },
            ],
            before_birth: None,
        }
        .apply(&mut p)
        .is_err());
        assert_eq!(p, before);
    }
}

#[cfg(test)]
mod animal_relocation_before_birth {
    use super::*;

    #[test]
    fn can_move_a_parent_to_make_room_for_a_newborn() {
        let mut player = Player::new(0, 0);
        player.pet = Some(Animal::Sheep);
        player.farm[14].stable = true;
        player.farm[14].stable_animal = Some(Animal::Sheep);
        player.farm[13].tile = Tile::Pasture { id: 0 };
        player.pastures.push(crate::model::Pasture {
            id: 0,
            cells: vec![13],
            animal: None,
            quantity: 0,
        });
        let plan = BreedingPlan {
            before_birth: Some(AnimalRearrangement {
                pasture: vec![(0, Some(Animal::Sheep), 1)],
                stables: vec![(14, Animal::Sheep)],
                pet: None,
            }),
            births: vec![BirthChoice {
                species: Animal::Sheep,
                destination: BirthDestination::Pet,
            }],
        };
        plan.apply(&mut player).unwrap();
        assert_eq!(player.animal_count(Animal::Sheep), 3);
        assert_eq!(player.pastures[0].quantity, 1);
        assert_eq!(player.pet, Some(Animal::Sheep));
    }

    #[test]
    fn illegal_parent_rearrangement_is_atomic() {
        let mut player = Player::new(0, 0);
        player.pet = Some(Animal::Sheep);
        player.farm[14].stable = true;
        player.farm[14].stable_animal = Some(Animal::Sheep);
        let before = player.clone();
        let plan = BreedingPlan {
            before_birth: Some(AnimalRearrangement {
                pasture: vec![],
                stables: vec![],
                pet: None,
            }),
            births: vec![BirthChoice {
                species: Animal::Sheep,
                destination: BirthDestination::Discard,
            }],
        };
        assert!(plan.apply(&mut player).is_err());
        assert_eq!(player, before);
    }

    #[test]
    fn old_breeding_plan_json_defaults_to_no_pre_birth_rearrangement() {
        let original = BreedingPlan {
            births: vec![],
            before_birth: None,
        };
        let mut value = serde_json::to_value(original).unwrap();
        value.as_object_mut().unwrap().remove("before_birth");
        let restored: BreedingPlan = serde_json::from_value(value).unwrap();
        assert_eq!(restored.before_birth, None);
    }
}

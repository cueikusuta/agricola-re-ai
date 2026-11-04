//! Six harvests: one crop per planted field, feeding, then animal breeding.
//! The default M1 feeding decisions are deliberately simple.
use crate::engine::Result;
use crate::model::*;

impl Game {
    pub(crate) fn harvest(&mut self)->Result<()> {
        if !HARVEST_ROUNDS.contains(&self.public.round){return Err("harvest in non-harvest round".into())}
        for p in &mut self.public.players {
            // Field phase. Crops are inaccessible prior to harvest.
            for cell in &mut p.farm{
                if let Tile::Field {crop:Some(kind),remaining}=&mut cell.tile {
                    if *remaining>0 {
                        let good=if *kind==Crop::Grain{Good::Grain}else{Good::Vegetable};
                        p.supply.gain(good,1);
                        *remaining-=1;
                        if *remaining==0 {cell.tile=Tile::Field{crop:None,remaining:0}}
                    }
                }
            }
            feed(p)?;
            breed(p)?;
        }
        Ok(())
    }
}
fn use_food(p:&mut Player,owed:&mut u32,created:u32){
    let spent=created.min(*owed);
    *owed-=spent;
    p.supply.food+=created-spent;
}
fn feed(p:&mut Player)->Result<()> {
    let mut owed=(p.family-p.newborns) as u32*2+p.newborns as u32;
    let from_food=owed.min(p.supply.food);owed-=from_food;p.supply.food-=from_food;
    // Resource conversion in harvest: each workshop can process one matching good once.
    for (improvement,good,yield_food) in [(Improvement::Joinery,Good::Wood,2),(Improvement::Pottery,Good::Clay,2),(Improvement::Basketry,Good::Reed,3)] {
        if owed>0&&p.improvements.contains(&improvement)&&p.supply.get(good)>0{
            p.supply.pay(good,1)?;use_food(p,&mut owed,yield_food);
        }
    }
    // Vegetables can be eaten raw (1) or cooked if equipped (2/3).
    let vegetable_value=if p.improvements.iter().any(|i|matches!(i,Improvement::Hearth4|Improvement::Hearth5)){3}
        else if p.improvements.iter().any(|i|matches!(i,Improvement::Fireplace2|Improvement::Fireplace3)){2}else{1};
    while owed>0&&p.supply.vegetable>0 {p.supply.vegetable-=1;use_food(p,&mut owed,vegetable_value)}
    while owed>0&&p.supply.grain>0 {p.supply.grain-=1;owed-=1;}
    // Prefer preserving breeding pairs when possible, but do not beg while
    // immediately edible livestock can cover a remaining feeding shortfall.
    if owed>0&&p.improvements.iter().any(|i|i.is_cooking()) {
        for preserve_pairs in [true,false] {
            for species in [Animal::Sheep,Animal::Boar,Animal::Cattle] {
                if owed==0{break}
                let food=if p.improvements.iter().any(|i|matches!(i,Improvement::Hearth4|Improvement::Hearth5)) {
                    match species{Animal::Sheep=>2,Animal::Boar=>3,Animal::Cattle=>4}
                } else {match species{Animal::Sheep=>2,Animal::Boar=>2,Animal::Cattle=>3}};
                let mut removable=if preserve_pairs{p.animal_count(species).saturating_sub(2)}else{p.animal_count(species)};
                for place in &mut p.pastures {
                    if owed==0||removable==0{break}
                    if place.animal==Some(species){
                        while owed>0&&removable>0&&place.quantity>0 {
                            place.quantity-=1;removable-=1;
                            // use_food requires Player borrowing; update the balance inline.
                            let spent=owed.min(food);owed-=spent;p.supply.food+=food-spent;
                        }
                        if place.quantity==0{place.animal=None}
                    }
                }
                if owed>0&&removable>0&&p.pet==Some(species){
                    p.pet=None;removable-=1;use_food(p,&mut owed,food);
                }
                if owed>0&&removable>0{
                    for c in &mut p.farm{
                        if owed==0||removable==0{break}
                        if c.stable_animal==Some(species){
                            c.stable_animal=None;removable-=1;
                            let spent=owed.min(food);owed-=spent;p.supply.food+=food-spent;
                        }
                    }
                }
            }
        }
    }
    p.begging+=owed;
    Ok(())
}
fn breed(p:&mut Player)->Result<()> {
    for species in [Animal::Sheep,Animal::Boar,Animal::Cattle]{
        if p.animal_count(species)<2{continue}
        if let Some(index)=p.pastures.iter().position(|q|q.animal==Some(species)&&q.quantity<p.pasture_capacity(q)) {
            p.pastures[index].quantity+=1;continue;
        }
        if let Some(index)=p.pastures.iter().position(|q|q.animal.is_none()&&q.quantity==0){
            p.pastures[index].animal=Some(species);p.pastures[index].quantity=1;continue;
        }
        if let Some(cell)=p.farm.iter_mut().find(|c|c.stable&&!matches!(c.tile,Tile::Pasture{..})&&c.stable_animal.is_none()) {
            cell.stable_animal=Some(species);continue;
        }
        if p.pet.is_none(){p.pet=Some(species)}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn newborn_costs_one_food_and_is_reset_after_harvest() {
        let mut player = Player::new(0, 3);
        player.family = 3;
        player.newborns = 1;
        feed(&mut player).unwrap();
        // Two adults 2 each, one newborn 1: five food, so two begging markers.
        assert_eq!(player.begging, 2);
    }
    #[test]
    fn edible_breeding_pair_must_be_used_to_feed() {
        let mut player = Player::new(0, 0);
        player.improvements.push(Improvement::Hearth4);
        player.pet = Some(Animal::Sheep);
        player.farm[14].stable = true;
        player.farm[14].stable_animal = Some(Animal::Sheep);
        feed(&mut player).unwrap();
        assert_eq!(player.begging, 0);
        assert_eq!(player.animal_count(Animal::Sheep), 0);
    }
}

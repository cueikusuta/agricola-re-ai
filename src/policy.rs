//! Replaceable AI and subaction detail policies. Only Observation is accepted.
use crate::engine::{Action,AnimalPlan,BakeAllocation,Detail,Plant};
use crate::model::*;
use crate::harvest::{HarvestDetailPolicy,SimpleHarvestDetails};
use rand::{SeedableRng,seq::SliceRandom};
use rand_chacha::ChaCha8Rng;

pub trait SpacePolicy {
    fn select(&self,obs:&Observation,legal:&[Space],rng:&mut ChaCha8Rng)->Option<Space>;
}
pub trait DetailPolicy {
    fn produce(&self,obs:&Observation,space:Space)->Option<Action>;
}
pub struct RandomPolicy;
pub struct GreedyPolicy;
pub struct SimpleDetails;
impl SpacePolicy for RandomPolicy {
    fn select(&self,_:&Observation,legal:&[Space],rng:&mut ChaCha8Rng)->Option<Space>{legal.choose(rng).copied()}
}
fn weight(obs:&Observation,space:Space)->i32 {
    let p=&obs.public.players[obs.player_id];
    let r=obs.public.round;
    match space {
        Space::Forest|Space::Copse|Space::ClayPit|Space::ReedBank|Space::WesternQuarry|Space::EasternQuarry|Space::Fishing|Space::MeetingPlace=>{
            let stored=obs.public.accumulations.get(&space).copied().unwrap_or(0) as i32;
            let value=match space{Space::Forest|Space::Copse=>3,Space::ReedBank=>4,Space::WesternQuarry|Space::EasternQuarry=>4,Space::Fishing|Space::MeetingPlace=>5, _=>3};
            stored*value+if space==Space::MeetingPlace&&obs.public.start_player!=obs.player_id{3}else{0}
        }
        Space::DayLaborer=>if p.supply.food<4{15}else{7},
        Space::FarmExpansion=>if p.supply.reed>=2&&p.supply.wood>=5&&p.family==p.rooms() as u8{70}else{9},
        Space::FamilyGrowthWithRoom|Space::ModestFamilyGrowth=>80-2*r as i32,
        Space::FamilyGrowthNoRoom=>75,
        Space::Fencing=>14,
        Space::Farmland=>if p.supply.grain>0 {19}else{12},
        Space::GrainSeeds=>if p.supply.grain==0{23}else{11},
        Space::VegetableSeeds=>25,
        Space::GrainUtilization|Space::Cultivation=>26,
        Space::MajorImprovement=>if p.improvements.iter().all(|i|!i.is_cooking()){24}else{12},
        Space::SheepMarket|Space::PigMarket|Space::CattleMarket=>obs.public.animal_accumulations.get(&space).copied().unwrap_or(0) as i32*3,
        Space::AnimalMarket=>10,
        Space::ResourceMarket=>13,
        Space::HouseRedevelopment=>if r>=11{35}else{8},
        Space::FarmRedevelopment=>35,
    }
}
impl SpacePolicy for GreedyPolicy {
    fn select(&self,obs:&Observation,legal:&[Space],rng:&mut ChaCha8Rng)->Option<Space>{
        let max=legal.iter().map(|&s|weight(obs,s)).max()?;
        legal.iter().copied().filter(|&s|weight(obs,s)==max).collect::<Vec<_>>().choose(rng).copied()
    }
}
fn empty_room_target(p:&Player)->Option<usize>{(0..FARM_CELLS).find(|&i|matches!(p.farm[i].tile,Tile::Empty)&&!p.farm[i].stable&&neighbors(i).iter().any(|&j|matches!(p.farm[j].tile,Tile::Room)))}
fn empty_field_target(p:&Player)->Option<usize>{(0..FARM_CELLS).find(|&i|matches!(p.farm[i].tile,Tile::Empty)&&!p.farm[i].stable&&(p.fields()==0||neighbors(i).iter().any(|&j|matches!(p.farm[j].tile,Tile::Field{..}))))}
fn small_pasture(p:&Player)->Option<Vec<usize>> {
    crate::engine::single_cell_pasture(p).map(|i|vec![i])
}
fn plantings(p:&Player,new_field:Option<usize>)->Vec<Plant>{
    let mut gr=p.supply.grain;let mut ve=p.supply.vegetable;
    let mut fields=(0..FARM_CELLS).filter(|&i|matches!(p.farm[i].tile,Tile::Field{crop:None,..})).collect::<Vec<_>>();
    if let Some(i)=new_field{fields.push(i)}
    let mut result=vec![];
    for cell in fields {if gr>0 {result.push(Plant{cell,crop:Crop::Grain});gr-=1}else if ve>0{result.push(Plant{cell,crop:Crop::Vegetable});ve-=1}}
    result
}
fn animal_plan(p:&Player,animal:Animal,quantity:u32)->AnimalPlan{
    let mut plan=AnimalPlan::discard_all(animal,0);
    let mut remaining=quantity;
    if remaining>0&&p.pet.is_none(){plan.pet=true;remaining-=1;}
    for pasture in &p.pastures {
        if remaining==0{break}
        if pasture.animal.is_none()||pasture.animal==Some(animal){
            let free=p.pasture_capacity(pasture)-pasture.quantity;
            let add=remaining.min(free);if add>0 {plan.to_pastures.push((pasture.id,add));remaining-=add}
        }
    }
    for (cell_index,cell) in p.farm.iter().enumerate(){
        if remaining==0{break}
        if matches!(cell.tile,Tile::Empty)&&cell.stable&&cell.stable_animal.is_none(){plan.to_stables.push(cell_index);remaining-=1}
    }
    // If none of the animals can be kept, a fire lets the player convert rather than lose them.
    if remaining>0&&p.improvements.iter().any(|i|i.is_cooking()){plan.cook=remaining;remaining=0}
    plan.discard=remaining;plan
}
fn improvement_choice(obs:&Observation)->Option<Improvement>{
    let p=&obs.public.players[obs.player_id];
    let market=&obs.public.improvement_market;
    market.iter().copied().filter(|&i|{
        i.cost().iter().all(|(g,n)|p.supply.get(*g)>=*n)
        ||matches!(i,Improvement::Hearth4|Improvement::Hearth5)&&p.improvements.iter().any(|x|matches!(x,Improvement::Fireplace2|Improvement::Fireplace3))
    }).max_by_key(|i|match i{
        Improvement::Hearth4|Improvement::Hearth5=>50,
        Improvement::Fireplace2|Improvement::Fireplace3=>45,
        Improvement::Well=>40,
        Improvement::ClayOven|Improvement::StoneOven=>35,
        _=>20,
    })
}
impl DetailPolicy for SimpleDetails {
    fn produce(&self,obs:&Observation,space:Space)->Option<Action> {
        let p=&obs.public.players[obs.player_id];
        let detail=match space {
            Space::Forest|Space::ClayPit|Space::ReedBank|Space::Fishing|Space::DayLaborer|Space::GrainSeeds|Space::VegetableSeeds|Space::MeetingPlace|Space::WesternQuarry|Space::EasternQuarry|Space::Copse|Space::ResourceMarket|Space::FamilyGrowthWithRoom|Space::ModestFamilyGrowth|Space::FamilyGrowthNoRoom=>Detail::None,
            Space::Farmland=>Detail::Plow{cell:empty_field_target(p)?},
            Space::FarmExpansion=>{
                if p.supply.reed>=2&&p.supply.get(match p.house{House::Wood=>Good::Wood,House::Clay=>Good::Clay,House::Stone=>Good::Stone})>=5{
                    if let Some(cell)=empty_room_target(p){Detail::Build{rooms:vec![cell],stables:vec![]}}
                    else if p.supply.wood>=2 {Detail::Build{rooms:vec![],stables:vec![(0..FARM_CELLS).find(|&i|matches!(p.farm[i].tile,Tile::Empty)&&!p.farm[i].stable)?]}}
                    else{return None}
                } else if p.supply.wood>=2 {Detail::Build{rooms:vec![],stables:vec![(0..FARM_CELLS).find(|&i|matches!(p.farm[i].tile,Tile::Empty)&&!p.farm[i].stable)?]}}
                else{return None}
            }
            Space::Fencing=>Detail::Fence{pastures:if let Some(group)=small_pasture(p){vec![group]}
                else{crate::engine::split_candidate(p)?}},
            Space::GrainUtilization=>{
                let plant=plantings(p,None);
                // Baking is allocated by the replaceable detail policy, not
                // chosen by rule application. A default policy may still use
                // the oven(s) and then one cooking card.
                let unplanted_grain=p.supply.grain.saturating_sub(plant.iter().filter(|x|x.crop==Crop::Grain).count() as u32);
                let mut remaining=if plant.is_empty(){unplanted_grain}else{0};
                let mut baking=Vec::new();
                for (improvement,cap) in [(Improvement::ClayOven,1),(Improvement::StoneOven,2)] {
                    if p.improvements.contains(&improvement)&&remaining>0 {
                        let grain=remaining.min(cap);remaining-=grain;
                        baking.push(BakeAllocation{improvement,grain});
                    }
                }
                if remaining>0 {
                    if let Some(improvement)=p.improvements.iter().copied().find(|i|matches!(i,Improvement::Hearth4|Improvement::Hearth5))
                        .or_else(||p.improvements.iter().copied().find(|i|matches!(i,Improvement::Fireplace2|Improvement::Fireplace3))) {
                        baking.push(BakeAllocation{improvement,grain:remaining});
                    }
                }
                Detail::SowDetailed{plantings:plant,baking}
            }
            Space::Cultivation=>{let cell=empty_field_target(p);let plant=plantings(p,cell);Detail::Cultivate{plow:cell,plantings:plant}}
            Space::MajorImprovement=>Detail::Buy{improvement:improvement_choice(obs)?,instant_bake:0},
            Space::HouseRedevelopment|Space::FarmRedevelopment=>Detail::Renovate{bonus_improvement:None,instant_bake:0,pastures:vec![]},
            Space::SheepMarket|Space::PigMarket|Space::CattleMarket=>{
                let species=match space{Space::SheepMarket=>Animal::Sheep,Space::PigMarket=>Animal::Boar,_=>Animal::Cattle};
                let count=obs.public.animal_accumulations.get(&space).copied().unwrap_or(0);
                Detail::Animals(animal_plan(p,species,count))
            }
            Space::AnimalMarket=>Detail::Animals(animal_plan(p,Animal::Sheep,1)),
        };
        Some(Action{space,detail})
    }
}

pub fn play_to_end(game:&mut Game,space_policy:&dyn SpacePolicy,detail_policy:&dyn DetailPolicy)->Result<(),String>{
    play_to_end_with_harvest_policy(game,space_policy,detail_policy,&SimpleHarvestDetails)
}
pub fn play_to_end_with_harvest_policy(game:&mut Game,space_policy:&dyn SpacePolicy,detail_policy:&dyn DetailPolicy,harvest_policy:&dyn HarvestDetailPolicy)->Result<(),String>{
    for _ in 0..250 {
        if game.public.finished{return Ok(())}
        let obs=game.observation(game.public.active_player)?;
        let legal=game.legal_spaces();
        if legal.is_empty(){return Err(format!("no legal move in round {} for player {}",game.public.round,game.public.active_player))}
        // Deterministic per-decision RNG: the policy never advances the rule engine RNG.
        // This preserves action-only replay, and reveals neither hidden deck nor round RNG.
        let decision_index=game.log.events.len() as u64;
        let mut choice_rng=ChaCha8Rng::seed_from_u64(game.log.action_seed^decision_index.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let chosen=space_policy.select(&obs,&legal,&mut choice_rng).ok_or("policy refused selection")?;
        let action=detail_policy.produce(&obs,chosen).ok_or("detail policy declined")?;
        game.apply_with_harvest_policy(action,harvest_policy).map_err(|e|format!("policy action {:?} failed: {}",chosen,e))?;
    }
    if game.public.finished{Ok(())}else{Err("move limit exceeded".into())}
}

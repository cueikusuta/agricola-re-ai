//! Transactional rule application. Detail policies live outside this module.
use crate::model::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub type Result<T> = std::result::Result<T,String>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plant { pub cell:usize, pub crop:Crop }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimalPlan {
    pub animal:Animal,
    pub to_pastures:Vec<(u8,u32)>,
    pub to_stables:Vec<usize>,
    pub pet:bool,
    pub cook:u32,
    pub discard:u32,
}
impl AnimalPlan {pub fn discard_all(animal:Animal,n:u32)->Self{Self{animal,to_pastures:vec![],to_stables:vec![],pet:false,cook:0,discard:n}}}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Detail {
    None,
    Plow { cell:usize },
    Build { rooms:Vec<usize>,stables:Vec<usize> },
    Fence { pastures:Vec<Vec<usize>> },
    Sow { plantings:Vec<Plant>,bake:u32 },
    Cultivate { plow:Option<usize>,plantings:Vec<Plant> },
    Buy { improvement:Improvement,instant_bake:u32 },
    Renovate { bonus_improvement:Option<Improvement>, instant_bake:u32, pastures:Vec<Vec<usize>> },
    Animals(AnimalPlan),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action { pub space:Space, pub detail:Detail }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimalLocation { Pet, Stable(usize), Pasture(u8) }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FreeAction {
    ConvertCrop { good:Good, quantity:u32 },
    CookAnimal { animal:Animal, source:AnimalLocation, quantity:u32 },
    DiscardGood { good:Good, quantity:u32 },
    DiscardAnimal { animal:Animal, source:AnimalLocation, quantity:u32 },
    RearrangeAnimals { pasture:Vec<(u8,Option<Animal>,u32)>, stables:Vec<(usize,Animal)>, pet:Option<Animal> },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordedAction { Work { player:usize,action:Action }, Free { player:usize,action:FreeAction } }

fn group_occupied(used:&BTreeSet<Space>)->bool{used.iter().any(|s|s.additional())}
fn room_material(p:&Player)->Good {match p.house{House::Wood=>Good::Wood,House::Clay=>Good::Clay,House::Stone=>Good::Stone}}
fn next_house(h:House)->Option<(House,Good)>{match h{House::Wood=>Some((House::Clay,Good::Clay)),House::Clay=>Some((House::Stone,Good::Stone)),House::Stone=>None}}
fn can_renovate(p:&Player)->bool{next_house(p.house).map(|(_,g)|p.supply.reed>=1&&p.supply.get(g)>=p.rooms() as u32).unwrap_or(false)}
fn available_room(p:&Player)->bool{p.farm.iter().enumerate().any(|(i,c)|matches!(c.tile,Tile::Empty)&&!c.stable&&neighbors(i).iter().any(|&j|matches!(p.farm[j].tile,Tile::Room)))}
fn available_field(p:&Player)->bool{p.farm.iter().enumerate().any(|(i,c)|matches!(c.tile,Tile::Empty)&&!c.stable&&(!p.farm.iter().any(|x|matches!(x.tile,Tile::Field{..}))||neighbors(i).iter().any(|&j|matches!(p.farm[j].tile,Tile::Field{..}))))}
fn can_build(p:&Player)->bool{
    (p.supply.reed>=2&&p.supply.get(room_material(p))>=5&&available_room(p)) || (p.occupied_stables()<4&&p.supply.wood>=2&&p.farm.iter().any(|c|matches!(c.tile,Tile::Empty)&&!c.stable))
}
fn can_fence(p:&Player)->bool{single_cell_pasture(p).is_some()}
fn can_sow(p:&Player)->bool{p.farm.iter().any(|c|matches!(c.tile,Tile::Field{crop:None,..}))&&(p.supply.grain>0||p.supply.vegetable>0)}
fn can_bake(p:&Player)->bool{p.supply.grain>0&&p.improvements.iter().any(|i|matches!(i,Improvement::Fireplace2|Improvement::Fireplace3|Improvement::Hearth4|Improvement::Hearth5|Improvement::ClayOven|Improvement::StoneOven))}
fn can_buy(p:&Player,market:&[Improvement])->bool{market.iter().any(|&i|affordable(p,i)||i.is_cooking()&&matches!(i,Improvement::Hearth4|Improvement::Hearth5)&&p.improvements.iter().any(|c|matches!(c,Improvement::Fireplace2|Improvement::Fireplace3)))}
fn affordable(p:&Player,i:Improvement)->bool{i.cost().iter().all(|&(g,n)|p.supply.get(g)>=n)}

impl Game {
    pub fn legal_spaces(&self)->Vec<Space>{
        if self.public.finished {return vec![]}
        let me=&self.public.players[self.public.active_player];
        let mut list=Space::BASE.to_vec();list.extend(self.public.revealed.iter().copied().map(Space::from));
        if self.config.additional_action_space_enabled{list.extend([Space::Copse,Space::ResourceMarket,Space::AnimalMarket,Space::ModestFamilyGrowth]);}
        list.retain(|&s|{
            if self.public.used.contains(&s)||s.additional()&&group_occupied(&self.public.used){return false}
            match s {
                Space::Farmland=>available_field(me),Space::FarmExpansion=>can_build(me),
                Space::Fencing=>can_fence(me),Space::GrainUtilization=>can_sow(me)||can_bake(me),
                Space::MajorImprovement=>can_buy(me,&self.public.improvement_market),
                Space::FamilyGrowthWithRoom=>me.family<5&&me.rooms()>me.family as usize,
                Space::FamilyGrowthNoRoom=>me.family<5,
                Space::ModestFamilyGrowth=>self.public.round>=5&&me.family<5&&me.rooms()>me.family as usize,
                Space::Cultivation=>available_field(me)||can_sow(me),
                Space::HouseRedevelopment|Space::FarmRedevelopment=>can_renovate(me),
                _=>true,
            }
        });
        list
    }
    pub fn apply(&mut self,action:Action)->Result<()> {
        let player=self.public.active_player;
        if !self.legal_spaces().contains(&action.space){return Err(format!("illegal space {:?}",action.space))}
        let mut next=self.clone();let before=self.state_hash();
        next.execute_work(player,&action)?;
        next.public.used.insert(action.space);
        next.public.workers_placed[player]+=1;
        next.next_worker_or_round()?;
        next.validate()?;
        let after=next.state_hash();
        next.log.events.push(Event{player:Some(player),kind:"work".into(),payload:serde_json::to_value(RecordedAction::Work{player,action}).map_err(|e|e.to_string())?,before,after});
        *self=next;Ok(())
    }
    pub fn apply_free(&mut self,player:usize,free:FreeAction)->Result<()> {
        if self.public.finished||player>=self.public.players.len(){return Err("not an active game/player".into())}
        let mut next=self.clone();let before=self.state_hash();
        next.execute_free(player,&free)?;
        next.validate()?;
        let after=next.state_hash();
        next.log.events.push(Event{player:Some(player),kind:"free".into(),payload:serde_json::to_value(RecordedAction::Free{player,action:free}).map_err(|e|e.to_string())?,before,after});
        *self=next;Ok(())
    }
    pub fn replay(config:Config,events:&[Event])->Result<Self>{
        let mut g=Game::new(config);
        for event in events {
            if g.state_hash()!=event.before{return Err("replay prestate hash mismatch".into())}
            let action:RecordedAction=serde_json::from_value(event.payload.clone()).map_err(|e|e.to_string())?;
            match action {
                RecordedAction::Work{player,action}=>{if player!=g.public.active_player{return Err("replay player mismatch".into())}g.apply(action)?},
                RecordedAction::Free{player,action}=>g.apply_free(player,action)?,
            }
            if g.state_hash()!=event.after{return Err("replay poststate hash mismatch".into())}
        }
        Ok(g)
    }
    fn next_worker_or_round(&mut self)->Result<()> {
        let n=self.public.players.len();let mut next=self.public.active_player;
        for _ in 0..n{next=(next+1)%n;if self.public.workers_placed[next]<self.public.work_quota[next]{self.public.active_player=next;return Ok(())}}
        if HARVEST_ROUNDS.contains(&self.public.round){self.harvest()?;}
        if self.public.round==14{self.public.finished=true}else{self.start_round()}
        Ok(())
    }
    fn execute_work(&mut self,id:usize,act:&Action)->Result<()> {
        let space=act.space;let detail=&act.detail;
        let p=&mut self.public.players[id];
        match space {
            Space::Forest|Space::ClayPit|Space::ReedBank|Space::Fishing|Space::WesternQuarry|Space::EasternQuarry|Space::Copse=>{
                require_none(detail)?;
                let n=self.public.accumulations.remove(&space).unwrap_or(0);
                let (kind,_) = space.accumulation().ok_or("not an accumulation space")?;
                p.supply.gain(kind,n);
            }
            Space::MeetingPlace=>{require_none(detail)?;let n=self.public.accumulations.remove(&space).unwrap_or(0);p.supply.food+=n;self.public.start_player=id;}
            Space::DayLaborer=>{require_none(detail)?;p.supply.food+=2;}
            Space::GrainSeeds=>{require_none(detail)?;p.supply.grain+=1;}
            Space::VegetableSeeds=>{require_none(detail)?;p.supply.vegetable+=1;}
            Space::ResourceMarket=>{require_none(detail)?;p.supply.stone+=1;p.supply.food+=1;}
            Space::Farmland=>{if let Detail::Plow{cell}=detail{plow(p,*cell)?}else{return Err("plow details required".into())}}
            Space::FarmExpansion=>{if let Detail::Build{rooms,stables}=detail{
                if rooms.is_empty()&&stables.is_empty(){return Err("build at least one item".into())}
                for &i in rooms{build_room(p,i)?}for &i in stables{build_stable(p,i)?}
            }else{return Err("build details required".into())}}
            Space::Fencing=>{if let Detail::Fence{pastures}=detail{build_pastures(p,pastures)?}else{return Err("pasture details required".into())}}
            Space::FamilyGrowthWithRoom|Space::ModestFamilyGrowth=>{require_none(detail)?;grow(p,true)?;}
            Space::FamilyGrowthNoRoom=>{require_none(detail)?;grow(p,false)?;}
            Space::GrainUtilization=>{if let Detail::Sow{plantings,bake}=detail{
                if plantings.is_empty()&&*bake==0 {return Err("must sow or bake".into())}
                sow(p,plantings)?;bake_bread(p,*bake)?;
            }else{return Err("sow/bake details required".into())}}
            Space::Cultivation=>{if let Detail::Cultivate{plow:cell,plantings}=detail{
                if cell.is_none()&&plantings.is_empty(){return Err("must cultivate".into())}
                if let Some(i)=cell{plow(p,*i)?};sow(p,plantings)?;
            }else{return Err("cultivation details required".into())}}
            Space::MajorImprovement=>{if let Detail::Buy{improvement,instant_bake}=detail{buy_improvement(p,&mut self.public.improvement_market,&mut self.delayed_well,self.public.round,*improvement,*instant_bake)?}else{return Err("improvement required".into())}}
            Space::HouseRedevelopment=>{if let Detail::Renovate{bonus_improvement,instant_bake,pastures}=detail{
                if !pastures.is_empty(){return Err("no fences on house redevelopment".into())}
                renovate(p)?;if let Some(i)=bonus_improvement {buy_improvement(p,&mut self.public.improvement_market,&mut self.delayed_well,self.public.round,*i,*instant_bake)?}else if *instant_bake>0{return Err("instant bake without oven".into())}
            }else{return Err("renovation details required".into())}}
            Space::FarmRedevelopment=>{if let Detail::Renovate{bonus_improvement,instant_bake,pastures}=detail{
                if bonus_improvement.is_some()||*instant_bake>0{return Err("no improvement on farm redevelopment".into())}
                renovate(p)?;if !pastures.is_empty(){build_pastures(p,pastures)?}
            }else{return Err("renovation details required".into())}}
            Space::SheepMarket|Space::PigMarket|Space::CattleMarket|Space::AnimalMarket=>{
                if let Detail::Animals(plan)=detail {
                    let species=match space{Space::SheepMarket=>Some(Animal::Sheep),Space::PigMarket=>Some(Animal::Boar),Space::CattleMarket=>Some(Animal::Cattle),_=>None};
                    if let Some(kind)=species{if plan.animal!=kind{return Err("wrong species".into())}}
                    let quantity=if space==Space::AnimalMarket {if plan.animal==Animal::Cattle{p.supply.pay(Good::Food,1)?;}else if plan.animal==Animal::Sheep{p.supply.food+=1;}1}else{self.public.animal_accumulations.remove(&space).unwrap_or(0)};
                    place_animals(p,plan,quantity)?;
                }else{return Err("animal placement required".into())}
            }
        }
        Ok(())
    }
    fn execute_free(&mut self,id:usize,act:&FreeAction)->Result<()> {
        let p=&mut self.public.players[id];
        match act{
            FreeAction::ConvertCrop{good,quantity}=>{
                if !matches!(good,Good::Grain|Good::Vegetable)||*quantity==0{return Err("invalid crop conversion".into())}
                let food_per_good=match good {
                    Good::Grain=>1,
                    Good::Vegetable=>if p.improvements.iter().any(|i|matches!(i,Improvement::Hearth4|Improvement::Hearth5)){3}
                        else if p.improvements.iter().any(|i|matches!(i,Improvement::Fireplace2|Improvement::Fireplace3)){2}
                        else{1},
                    _=>return Err("only raw grain/vegetables can be converted".into()),
                };
                p.supply.pay(*good,*quantity)?;p.supply.gain(Good::Food,quantity.checked_mul(food_per_good).ok_or("food overflow")?);
            }
            FreeAction::DiscardGood{good,quantity}=>{if *quantity==0{return Err("discard zero".into())}p.supply.pay(*good,*quantity)?;}
            FreeAction::CookAnimal{animal,source,quantity}=>{
                if *quantity==0{return Err("cook zero".into())}
                let worth=cook_value(p,*animal).ok_or("no cooking improvement")?;
                take_animals(p,*animal,source,*quantity)?;p.supply.food+=quantity*worth;
            }
            FreeAction::DiscardAnimal{animal,source,quantity}=>{
                if *quantity==0{return Err("discard zero".into())}
                take_animals(p,*animal,source,*quantity)?;
            }
            FreeAction::RearrangeAnimals{pasture,stables,pet}=>{rearrange_animals(p,pasture,stables,*pet)?;}
        }Ok(())
    }
}
fn require_none(d:&Detail)->Result<()> {if !matches!(d,Detail::None){Err("unexpected detail".into())}else{Ok(())}}
fn plow(p:&mut Player,i:usize)->Result<()>{
    if i>=FARM_CELLS||!matches!(p.farm[i].tile,Tile::Empty)||p.farm[i].stable{return Err("field needs empty tile".into())}
    if p.fields()>0&&!neighbors(i).iter().any(|&j|matches!(p.farm[j].tile,Tile::Field{..})){return Err("field must touch existing field".into())}
    p.farm[i].tile=Tile::Field{crop:None,remaining:0};Ok(())
}
fn build_room(p:&mut Player,i:usize)->Result<()>{
    if i>=FARM_CELLS||!matches!(p.farm[i].tile,Tile::Empty)||p.farm[i].stable||!neighbors(i).iter().any(|&j|matches!(p.farm[j].tile,Tile::Room)){return Err("room location illegal".into())}
    let material=room_material(p);p.supply.pay(material,5)?;p.supply.pay(Good::Reed,2)?;p.farm[i].tile=Tile::Room;Ok(())
}
fn build_stable(p:&mut Player,i:usize)->Result<()>{
    if i>=FARM_CELLS||p.occupied_stables()>=4||p.farm[i].stable||!matches!(p.farm[i].tile,Tile::Empty|Tile::Pasture{..}){return Err("stable location illegal".into())}
    p.supply.pay(Good::Wood,2)?;p.farm[i].stable=true;Ok(())
}
fn grow(p:&mut Player,room_only:bool)->Result<()> {if p.family>=5||room_only&&p.rooms()<=p.family as usize{return Err("family growth not permitted".into())}p.family+=1;p.newborns+=1;Ok(())}
fn renovate(p:&mut Player)->Result<()> {let (new,g)=next_house(p.house).ok_or("stone is final house")?;p.supply.pay(g,p.rooms() as u32)?;p.supply.pay(Good::Reed,1)?;p.house=new;Ok(())}
fn sow(p:&mut Player,assignments:&[Plant])->Result<()>{
    let mut visited=BTreeSet::new();
    for a in assignments {
        if a.cell>=FARM_CELLS||!visited.insert(a.cell)||!matches!(p.farm[a.cell].tile,Tile::Field{crop:None,remaining:0}){return Err("invalid empty field".into())}
        let good=match a.crop{Crop::Grain=>Good::Grain,Crop::Vegetable=>Good::Vegetable};p.supply.pay(good,1)?;
        p.farm[a.cell].tile=Tile::Field{crop:Some(a.crop),remaining:if a.crop==Crop::Grain{3}else{2}};
    }Ok(())
}
/// Apply one simple, deterministic baking allocation (M1 policy).
/// Each Clay Oven can bake one grain and each Stone Oven two grain per
/// Bake Bread action; all remaining grain requires a cooking improvement.
fn bake_bread(p:&mut Player,amount:u32)->Result<()> {
    if amount==0{return Ok(())}
    let has_clay=p.improvements.contains(&Improvement::ClayOven);
    let has_stone=p.improvements.contains(&Improvement::StoneOven);
    let hearth=if p.improvements.iter().any(|i|matches!(i,Improvement::Hearth4|Improvement::Hearth5)){3}
        else if p.improvements.iter().any(|i|matches!(i,Improvement::Fireplace2|Improvement::Fireplace3)){2}else{0};
    if hearth==0 && amount>(u32::from(has_clay)+2*u32::from(has_stone)) {
        return Err("bake amount exceeds oven capacities".into());
    }
    p.supply.pay(Good::Grain,amount)?;
    let mut left=amount;
    let mut gain=0u32;
    if has_clay && left>0 {left-=1;gain+=5;}
    if has_stone {let n=left.min(2);left-=n;gain+=4*n;}
    gain+=left*hearth;
    p.supply.gain(Good::Food,gain);
    Ok(())
}
fn buy_improvement(p:&mut Player,market:&mut Vec<Improvement>,well:&mut BTreeMap<u8,Vec<usize>>,round:u8,item:Improvement,instant_bake:u32)->Result<()> {
    let pos=market.iter().position(|&x|x==item).ok_or("improvement unavailable")?;
    let fireplace=p.improvements.iter().position(|i|matches!(i,Improvement::Fireplace2|Improvement::Fireplace3));
    let exchange=matches!(item,Improvement::Hearth4|Improvement::Hearth5)&&fireplace.is_some();
    if exchange {let old=p.improvements.remove(fireplace.expect("fireplace checked"));market.push(old)}
    else {for (good,n) in item.cost(){p.supply.pay(good,n)?}}
    market.remove(pos);
    p.improvements.push(item);
    if item==Improvement::Well {for r in round+1..=14u8.min(round.saturating_add(5)){well.entry(r).or_default().push(p.id)}}
    if instant_bake>0&&!matches!(item,Improvement::ClayOven|Improvement::StoneOven){return Err("only ovens grant immediate bake".into())}
    if instant_bake > (match item{Improvement::ClayOven=>1,Improvement::StoneOven=>2,_=>0}) {return Err("instant oven bake limit exceeded".into())}
    bake_bread(p,instant_bake)?;Ok(())
}
/// Canonical fence segments, 6x4 lattice vertices. 5x3 cell grid.
fn border(i:usize)->[(u8,u8);4] {
    let x=(i%5) as u8;let y=(i/5) as u8;
    let pts=[y*6+x,y*6+x+1,(y+1)*6+x,(y+1)*6+x+1];
    [(pts[0],pts[1]),(pts[2],pts[3]),(pts[0],pts[2]),(pts[1],pts[3])]
}
fn boundary(cells:&[usize])->BTreeSet<(u8,u8)>{
    let mut edges=BTreeSet::new();for &cell in cells{for edge in border(cell){if !edges.insert(edge){edges.remove(&edge);}}}edges
}
/// Legal one-cell pasture option used by the default detail policy.
/// Check fresh fence cost, the 15 fence limit, and adjacency.
pub fn single_cell_pasture(p:&Player)->Option<usize>{
    (0..FARM_CELLS).find(|&i|{
        if !matches!(p.farm[i].tile,Tile::Empty) {return false}
        if !p.pastures.is_empty() && !neighbors(i).iter().any(|&j|matches!(p.farm[j].tile,Tile::Pasture{..})) {return false}
        let edges=boundary(&[i]);
        let extra=edges.difference(&p.fences).count();
        extra <= p.supply.wood as usize && p.fences.len()+extra<=15
    })
}
fn connected(cells:&[usize])->bool{
    if cells.is_empty(){return false}let set: BTreeSet<_>=cells.iter().copied().collect();
    let mut found=BTreeSet::new();let mut q=VecDeque::from([cells[0]]);
    while let Some(i)=q.pop_front(){if found.insert(i){for j in neighbors(i){if set.contains(&j){q.push_back(j)}}}}
    found.len()==set.len()
}
fn build_pastures(p:&mut Player,pastures:&[Vec<usize>])->Result<()> {
    if pastures.is_empty(){return Err("must add a pasture".into())}
    for group in pastures{
        let uniq:BTreeSet<_>=group.iter().copied().collect();
        if group.is_empty()||uniq.len()!=group.len()||group.iter().any(|&i|i>=FARM_CELLS)||!connected(group){return Err("noncontiguous pasture".into())}
        for &i in group{if !matches!(p.farm[i].tile,Tile::Empty){return Err("pasture requires empty cells".into())}}
        if !p.pastures.is_empty()&&!group.iter().any(|&i|neighbors(i).iter().any(|&j|matches!(p.farm[j].tile,Tile::Pasture{..}))){return Err("new pasture must touch existing".into())}
        let segments=boundary(group);let fresh=segments.difference(&p.fences).count();
        if p.fences.len()+fresh>15{return Err("fence limit 15".into())}
        p.supply.pay(Good::Wood,fresh as u32)?;
        p.fences.extend(segments);
        // Animals occupying an unfenced stable inside the new enclosure now
        // occupy that pasture. No animal may be lost during fencing.
        let species: BTreeSet<Animal> = group.iter().filter_map(|&i|p.farm[i].stable_animal).collect();
        if species.len()>1{return Err("new pasture would mix animal species".into())}
        let quantity=group.iter().filter(|&&i|p.farm[i].stable_animal.is_some()).count() as u32;
        let animal=species.iter().next().copied();
        let id=p.pastures.iter().map(|p|p.id).max().map(|x|x+1).unwrap_or(0);
        for &i in group{
            p.farm[i].tile=Tile::Pasture{id};
            p.farm[i].stable_animal=None;
        }
        p.pastures.push(Pasture{id,cells:group.clone(),animal,quantity});
    }Ok(())
}
fn cook_value(p:&Player,animal:Animal)->Option<u32>{
    if p.improvements.iter().any(|i|matches!(i,Improvement::Hearth4|Improvement::Hearth5)){
        Some(match animal{Animal::Sheep=>2,Animal::Boar=>3,Animal::Cattle=>4})
    }else if p.improvements.iter().any(|i|matches!(i,Improvement::Fireplace2|Improvement::Fireplace3)){
        Some(match animal{Animal::Sheep=>2,Animal::Boar=>2,Animal::Cattle=>3})
    }else{None}
}
fn take_animals(p:&mut Player,animal:Animal,location:&AnimalLocation,count:u32)->Result<()> {
    match location {
        AnimalLocation::Pet=>{if count!=1||p.pet!=Some(animal){return Err("pet mismatch".into())}p.pet=None;}
        AnimalLocation::Stable(i)=>{if count!=1||*i>=FARM_CELLS||p.farm[*i].stable_animal!=Some(animal){return Err("stable animal mismatch".into())}p.farm[*i].stable_animal=None;}
        AnimalLocation::Pasture(id)=>{
            let pasture=p.pastures.iter_mut().find(|x|x.id==*id).ok_or("pasture absent")?;
            if pasture.animal!=Some(animal)||pasture.quantity<count{return Err("pasture animal mismatch".into())}
            pasture.quantity-=count;if pasture.quantity==0{pasture.animal=None}
        }
    }Ok(())
}
fn place_animals(p:&mut Player,plan:&AnimalPlan,n:u32)->Result<()> {
    let assigned:u32=plan.to_pastures.iter().map(|x|x.1).sum::<u32>()+plan.to_stables.len() as u32+u32::from(plan.pet)+plan.cook+plan.discard;
    if assigned!=n{return Err("animal quantity mismatch".into())}
    if plan.cook>0&&cook_value(p,plan.animal).is_none(){return Err("cannot cook without improvement".into())}
    let mut seen=BTreeSet::new();for (id,n) in &plan.to_pastures{
        if *n==0||!seen.insert(*id){return Err("repeated/zero pasture assignment".into())}
        let past=p.pastures.iter_mut().find(|x|x.id==*id).ok_or("unknown pasture id")?;
        if past.animal.is_some()&&past.animal!=Some(plan.animal){return Err("mixed pasture species".into())}
        past.animal=Some(plan.animal);past.quantity+=n;
    }
    for i in &plan.to_stables{
        if *i>=FARM_CELLS||!seen.insert(200+*i as u8)||!p.farm[*i].stable||!matches!(p.farm[*i].tile,Tile::Empty)||p.farm[*i].stable_animal.is_some(){return Err("invalid stable assignment".into())}
        p.farm[*i].stable_animal=Some(plan.animal)
    }
    if plan.pet {if p.pet.is_some(){return Err("pet already occupied".into())}p.pet=Some(plan.animal)}
    for past in &p.pastures {if past.quantity>p.pasture_capacity(past){return Err("pasture overflow".into())}}
    if plan.cook>0{p.supply.food+=plan.cook*cook_value(p,plan.animal).expect("checked above")}
    Ok(())
}
fn rearrange_animals(p:&mut Player,pasture:&[(u8,Option<Animal>,u32)],stables:&[(usize,Animal)],pet:Option<Animal>)->Result<()> {
    let before=[Animal::Sheep,Animal::Boar,Animal::Cattle].map(|a|p.animal_count(a));
    if pasture.len()!=p.pastures.len() ||
        pasture.iter().map(|(id,_,_)|*id).collect::<BTreeSet<_>>().len()!=pasture.len(){
        return Err("must name each pasture exactly once".into())
    }
    for old in &mut p.pastures {let (_,kind,n)=pasture.iter().find(|(id,_,_)|*id==old.id).ok_or("missing pasture id")?;if (*n>0)!=kind.is_some(){return Err("invalid pasture species/count".into())}old.animal = *kind; old.quantity = *n;}
    for c in &mut p.farm{c.stable_animal=None;}
    let mut used=BTreeSet::new();for (i,kind) in stables {if *i>=FARM_CELLS||!used.insert(*i)||!p.farm[*i].stable||!matches!(p.farm[*i].tile,Tile::Empty){return Err("bad unfenced stable".into())}p.farm[*i].stable_animal=Some(*kind);}
    p.pet=pet;
    let after=[Animal::Sheep,Animal::Boar,Animal::Cattle].map(|a|p.animal_count(a));
    if before!=after{return Err("rearrange must conserve animals".into())}
    if p.pastures.iter().any(|q|q.quantity>p.pasture_capacity(q)){return Err("pasture overflow".into())}
    Ok(())
}

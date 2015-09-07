//! Port of the Complete Roguelike Tutorial for Python + libtcod to Rust
//!

extern crate tcod;
extern crate rand;
extern crate rustc_serialize;

use std::ascii::AsciiExt;
use std::cmp::{self, Ordering};
use std::fs::File;
use std::io::{Read, Write, Error};
use tcod::console::*;
use tcod::colors::{self, Color};
use tcod::input::{self, Key, Event, Mouse};
use tcod::map::Map as FovMap;
use tcod::map::FovAlgorithm;
use rand::Rng;
use rustc_serialize::{json, Encodable, Encoder};


// actual size of the window
const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;

// size of the map
const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 43;

// sizes and coordinates relevant for the GUI
const BAR_WIDTH: i32 = 20;
const PANEL_HEIGHT: i32 = 7;
const PANEL_Y: i32 = SCREEN_HEIGHT - PANEL_HEIGHT;
const MSG_X: i32 = BAR_WIDTH + 2;
const MSG_WIDTH: i32 = SCREEN_WIDTH - BAR_WIDTH - 2;
const MSG_HEIGHT: usize = PANEL_HEIGHT as usize - 1;
const INVENTORY_WIDTH: i32 = 50;
const CHARACTER_SCREEN_WIDTH: i32 = 30;
const LEVEL_SCREEN_WIDTH: i32 = 40;

//parameters for dungeon generator
const ROOM_MAX_SIZE: i32 = 10;
const ROOM_MIN_SIZE: i32 = 6;
const MAX_ROOMS: i32 = 30;

// spell values
const HEAL_AMOUNT: i32 = 40;
const LIGHTNING_DAMAGE: i32 = 40;
const LIGHTNING_RANGE: i32 = 5;
const CONFUSE_RANGE: i32 = 8;
const CONFUSE_NUM_TURNS: i32 = 10;
const FIREBALL_RADIUS: i32 = 3;
const FIREBALL_DAMAGE: i32 = 25;

// experience and level-ups
const LEVEL_UP_BASE: i32 = 200;
const LEVEL_UP_FACTOR: i32 = 150;


const FOV_ALGO: FovAlgorithm = FovAlgorithm::Basic;
const FOV_LIGHT_WALLS: bool = true;
const TORCH_RADIUS: i32 = 10;

const LIMIT_FPS: i32 = 20;  // 20 frames-per-second maximum

const COLOR_DARK_WALL: Color = Color { r: 0, g: 0, b: 100 };
const COLOR_LIGHT_WALL: Color = Color { r: 130, g: 110, b: 50 };
const COLOR_DARK_GROUND: Color = Color { r: 50, g: 50, b: 150 };
const COLOR_LIGHT_GROUND: Color = Color { r: 200, g: 180, b: 50 };

const PLAYER: usize = 0;

type Map = Vec<Vec<Tile>>;

#[derive(Copy, Clone, RustcEncodable, RustcDecodable)]
struct Tile {
    blocked: bool,
    explored: bool,
    block_sight: bool,
}

#[derive(Copy, Clone)]
struct Rect {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect { x1: x, y1: y, x2: x + w, y2: y + h }
    }

    pub fn center(&self) -> (i32, i32) {
        let center_x = (self.x1 + self.x2) / 2;
        let center_y = (self.y1 + self.y2) / 2;
        (center_x, center_y)
    }

    pub fn intersect(&self, other: &Rect) -> bool {
        // returns true if this rectangle intersects with another one
        (self.x1 <= other.x2) && (self.x2 >= other.x1) && (self.y1 <= other.y2) &&
        (self.y2 >= other.y1)
    }
}

#[derive(Clone, Debug, PartialEq, RustcEncodable, RustcDecodable)]
struct Object {
    x: i32,
    y: i32,
    char: char,
    name: String,
    color: Color,
    blocks: bool,
    always_visible: bool,
    level: i32,
    fighter: Option<Fighter>,
    ai: Option<MonsterAI>,
    item: Option<Item>,
    equipment: Option<Equipment>,
}

impl Object {
    pub fn new(x: i32, y: i32, char: char, name: &str, color: Color, blocks: bool) -> Self {
        Object {
            x: x,
            y: y,
            char: char,
            name: name.to_owned(),
            color: color,
            blocks: blocks,
            always_visible: false,
            level: 0,
            fighter: None,
            ai: None,
            item: None,
            equipment: None,
        }
    }

    pub fn is_player(&self) -> bool {
        self.name == "player"
    }

    pub fn pos(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn set_pos(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    /// return the distance to another object
    pub fn distance_to(&self, other: &Object) -> f32 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        ((dx.pow(2) + dy.pow(2)) as f32).sqrt()
    }

    /// return the distance to some coordinates
    pub fn distance(&self, x: i32, y: i32) -> f32 {
        (((x - self.x).pow(2) + (y - self.y).pow(2)) as f32).sqrt()
    }

    /// Set the color and then draw the character that represents this object at its position.
    pub fn draw(&self, con: &mut Console, map: &Map, fov: &FovMap) {
        // only show if it's visible to the player; or it's set to
        // "always visible" and on an explored tile
        if fov.is_in_fov(self.x, self.y) ||
           (self.always_visible && map[self.x as usize][self.y as usize].explored) {
            con.set_default_foreground(self.color);
            con.put_char(self.x, self.y, self.char, BackgroundFlag::None);
        }
    }

    /// Erase the character that represents this object.
    pub fn clear(&self, con: &mut Console) {
        con.put_char(self.x, self.y, ' ', BackgroundFlag::None);
    }

    pub fn take_damage(&mut self, damage: i32, game: &mut Game) -> Option<i32> {
        let death = self.fighter.as_mut().map_or(None, |fighter| {
            // apply damage if possible
            if damage > 0 {
                fighter.hp -= damage;
            }
            if fighter.hp <= 0 {
                fighter.death.map(|d| (d, fighter.xp))
            } else {
                None
            }
        });
        death.map(|(death, xp)| {
            death.callback(self, game);
            xp
        })
    }

    fn attack(&mut self, target: &mut Object, game: &mut Game) {
        // a simple formula for attack damage
        let damage = self.full_power(game) - target.full_defense(game);
        if damage > 0 {
            // make the target take some damage
            game.log.add(format!("{} attacks {} for {} hit points.",
                                 self.name, target.name, damage),
                         colors::WHITE);
            target.take_damage(damage, game).map(|xp| {
                if self.is_player() {
                    self.fighter.as_mut().unwrap().xp += xp;
                }
            });
        } else {
            game.log.add(format!("{} attacks {} but it has no effect!", self.name, target.name),
                         colors::WHITE);
        }
    }

    fn full_power(&self, game: &Game) -> i32 {
        let base_power = self.fighter.as_ref().map_or(0, |f| f.base_power);
        // TODO: this is unstable, but maps closer to the Python tutorial and is easier to understand:
        //let bonus: i32 = get_all_equipped(id, game).iter().map(|e| e.power_bonus).sum();
        let bonus = self.get_all_equipped(game).iter().fold(0, |sum, e| sum + e.power_bonus);
        base_power + bonus
    }

    fn full_defense(&self, game: &Game) -> i32 {
        let base_defense = self.fighter.as_ref().map_or(0, |f| f.base_defense);
        let bonus = self.get_all_equipped(game).iter().fold(0, |sum, e| sum + e.defense_bonus);
        base_defense + bonus
    }

    fn full_max_hp(&self, game: &Game) -> i32 {
        let base_max_hp = self.fighter.as_ref().map_or(0, |f| f.base_max_hp);
        let bonus = self.get_all_equipped(game).iter().fold(0, |sum, e| sum + e.max_hp_bonus);
        base_max_hp + bonus
    }

    /// returns a list of equipped items
    fn get_all_equipped(&self, game: &Game) -> Vec<Equipment> {
        if self.is_player() {
            game.inventory
                .iter()
                .filter(|item| {
                    item.equipment.as_ref().map_or(false, |e| e.is_equipped)
                })
                .map(|item| item.equipment.clone().unwrap())
                .collect()
        } else {
            vec![]  // other objects have no equipment
        }
    }
}


/// move by the given amount, if the destination is not blocked
fn move_by(id: usize, dx: i32, dy: i32, objects: &mut [Object], game: &mut Game) {
    let (x, y) = objects[id].pos();
    if !is_blocked(x + dx, y + dy, &game.map, &objects) {
        objects[id].set_pos(x + dx, y + dy);
    }
}

fn move_towards(id: usize, target_x: i32, target_y: i32, objects: &mut [Object], game: &mut Game) {
    // vector from this object to the target, and distance
    let (dx, dy) = {
        let (ox, oy) = objects[id].pos();
        (target_x - ox, target_y - oy)
    };
    let distance = ((dx.pow(2) + dy.pow(2)) as f32).sqrt();

    // normalize it to length 1 (preserving direction), then round it and
    // convert to integer so the movement is restricted to the map grid
    let dx = (dx as f32 / distance).round() as i32;
    let dy = (dy as f32 / distance).round() as i32;
    move_by(id, dx, dy, objects, game);
}

/// Mutably borrow two *separate* elements from the given slice.
/// Panics when the indexes are equal or out of bounds.
fn mut_two<T>(first_index: usize, second_index: usize, items: &mut [T]) -> (&mut T, &mut T) {
    assert!(first_index != second_index);
    let split_at_index = if first_index < second_index {
        second_index
    } else {
        first_index
    };
    let (first_slice, second_slice) = items.split_at_mut(split_at_index);
    if first_index < second_index {
        (&mut first_slice[first_index], &mut second_slice[0])
    } else {
        (&mut second_slice[0], &mut first_slice[second_index])
    }
}

// an item that can be picked up and used.
fn pick_item_up(object_id: usize, objects: &mut Vec<Object>, game: &mut Game) {
    // add to the player's inventory and remove from the map
    if game.inventory.len() >= 26 {
        game.log.add(format!("Your inventory is full, cannot pick up {}.", objects[object_id].name),
                     colors::RED);
    } else {
        let item = objects.swap_remove(object_id);
        game.log.add(format!("You picked up a {}!", item.name), colors::GREEN);
        let inventory_id = game.inventory.len();
        let equipment_slot = item.equipment.as_ref().map(|e| e.slot.clone());
        game.inventory.push(item);

        // special case: automatically equip, if the corresponding equipment slot is unused
        if let Some(equipment_slot) = equipment_slot {
            if get_equipped_in_slot(&equipment_slot, &game.inventory).is_none() {
                equip(inventory_id, game);
            }
        }
    }
}

fn use_item(inventory_id: usize, objects: &mut [Object], game: &mut Game, tcod: &mut TcodState) {
    // special case: if the object has the Equipment component, the "use" action is to equip/dequip
    if game.inventory[inventory_id].item.is_some() && game.inventory[inventory_id].equipment.is_some() {
        toggle_equip(inventory_id, game);
        return
    }
    // just call the "use_item" if it is defined
    if let Some(item) = game.inventory[inventory_id].item {
        match item.use_item(objects, game, tcod) {
            UseResult::Used => {
                // destroy after use, unless it was cancelled for some reason
                game.inventory.remove(inventory_id);
            }
            UseResult::Cancelled => {
                game.log.add("Cancelled", colors::WHITE);
            }
        };
    } else {
        game.log.add(format!("The {} cannot be used.", game.inventory[inventory_id].name), colors::WHITE);
    }
}

fn drop_item(inventory_id: usize, objects: &mut Vec<Object>, game: &mut Game) {
    if game.inventory[inventory_id].equipment.is_some() {
        dequip(inventory_id, game);
    }
    let mut item = game.inventory.remove(inventory_id);
    let (px, py) = objects[PLAYER].pos();
    item.set_pos(px, py);
    game.log.add(format!("You dropped a {}.", item.name), colors::YELLOW);
    objects.push(item);
}

fn toggle_equip(inventory_id: usize, game: &mut Game) {
    if game.inventory[inventory_id].equipment.as_ref().map_or(false, |e| e.is_equipped) {
        dequip(inventory_id, game);
    } else {
        equip(inventory_id, game);
    }
}

fn equip(inventory_id: usize, game: &mut Game) {
    // if the slot is already being used, dequip whatever is there first
    // TODO: treat empty String as a slot that fails to get a match.
    // This will have to be changed if we switch to a slot enum.
    let slot = game.inventory[inventory_id].equipment.as_ref().map_or("".into(), |e| e.slot.clone());
    if let Some(old_equipment_id) = get_equipped_in_slot(&slot, &game.inventory) {
        dequip(old_equipment_id, game);
    }
    // equip object and show a message about it
    if let Some(mut equipment) = game.inventory[inventory_id].equipment.take() {
        equipment.is_equipped = true;
        game.log.add(format!("Equipped {} on {}.", game.inventory[inventory_id].name, equipment.slot),
                     colors::LIGHT_GREEN);

        game.inventory[inventory_id].equipment = Some(equipment);
    }
}

fn dequip(inventory_id: usize, game: &mut Game) {
    // dequip object and show a message about it
    if let Some(mut equipment) = game.inventory[inventory_id].equipment.take() {
        if equipment.is_equipped {
            equipment.is_equipped = false;
            game.log.add(format!("Dequipped {} from {}.", game.inventory[inventory_id].name, equipment.slot),
                         colors::LIGHT_YELLOW);
        }

        game.inventory[inventory_id].equipment = Some(equipment);
    }
}


#[derive(Clone, Debug, PartialEq, RustcEncodable, RustcDecodable)]
struct Fighter {
    base_max_hp: i32,
    hp: i32,
    base_defense: i32,
    base_power: i32,
    xp: i32,
    death: Option<DeathCallback>,
}

impl Fighter {
    fn heal(&mut self, amount: i32) {
        // heal by the given amount, without going over the maximum
        self.hp += amount;
        if self.hp > self.base_max_hp {
            self.hp = self.base_max_hp;
        }
    }
}

#[derive(Debug, PartialEq, Copy, Clone, RustcEncodable, RustcDecodable)]
enum DeathCallback {
    Monster,
    Player,
}

impl DeathCallback {
    fn callback(&self, object: &mut Object, game: &mut Game) {
        use DeathCallback::*;
        let callback: fn(&mut Object, &mut Game) = match *self {
            Monster => monster_death,
            Player => player_death,
        };
        callback(object, game);
    }
}



#[derive(Copy, Clone, Debug, PartialEq, RustcEncodable, RustcDecodable)]
enum MonsterAIType {
    Basic,
    Confused {
        num_turns: i32,
    },
}

#[derive(Clone, Debug, PartialEq, RustcEncodable, RustcDecodable)]
struct MonsterAI {
    old_ai: Option<Box<MonsterAI>>,
    ai_type: MonsterAIType,
}

impl MonsterAI {
    fn take_turn(&mut self, monster_id: usize, objects: &mut [Object], game: &mut Game, tcod: &mut TcodState) -> Option<MonsterAI> {
        use MonsterAIType::*;
        match self.ai_type {
            Basic => self.monster_basic_ai(monster_id, objects, game, tcod),
            Confused{..} => self.monster_confused_ai(monster_id, objects, game, tcod),
        }
    }

    fn monster_basic_ai(&mut self, monster_id: usize, objects: &mut [Object], game: &mut Game, tcod: &mut TcodState) -> Option<MonsterAI> {
        // a basic monster takes its turn. If you can see it, it can see you
        let (monster_x, monster_y) = objects[monster_id].pos();
        if tcod.fov_map.is_in_fov(monster_x, monster_y) {
            // move towards player if far away
            let distance = {
                let monster = &objects[monster_id];
                let player = &objects[PLAYER];
                monster.distance_to(player)
            };
            if distance >= 2.0 {
                let (player_x, player_y) = objects[PLAYER].pos();
                move_towards(monster_id, player_x, player_y, objects, game);
            } else if objects[PLAYER].fighter.as_ref().map_or(
                false, |fighter| fighter.hp > 0) {
                // close enough, attack! (if the player is still alive.)
                let (monster, player) = mut_two(monster_id, PLAYER, objects);
                monster.attack(player, game);
            }
        }
        None
    }

    fn monster_confused_ai(&mut self, monster_id: usize, objects: &mut [Object], game: &mut Game, _tcod: &mut TcodState) -> Option<MonsterAI> {
        use MonsterAIType::*;
        match self.ai_type {
            Confused{num_turns} => {
                if num_turns > 0 {  // still confused...
                    // move in a random direction, and decrease the number of turns confused
                    move_by(monster_id, range(-1, 1), range(-1, 1), objects, game);
                    self.ai_type = Confused{num_turns: num_turns - 1};
                    None
                } else {  // restore the previous AI (this one will be deleted)
                    game.log.add(format!("The {} is no longer confused!",
                                         objects[monster_id].name),
                                 colors::RED);
                    self.old_ai.take().map(|ai| *ai)
                }
            }
            _ => unreachable!(),
        }
    }
}


#[derive(Debug, PartialEq, Copy, Clone, RustcEncodable, RustcDecodable)]
enum Item {
    Heal,
    Lightning,
    Fireball,
    Confuse,
    None,
}

impl Item {
    fn use_item(&self, objects: &mut [Object], game: &mut Game, tcod: &mut TcodState) -> UseResult {
        use Item::*;
        let callback: fn(&mut [Object], &mut Game, &mut TcodState) -> UseResult = match *self {
            Heal => cast_heal,
            Lightning => cast_lightning,
            Fireball => cast_fireball,
            Confuse => cast_confuse,
            Item::None => cast_nothing,
        };
        callback(objects, game, tcod)
    }
}

enum UseResult {
    Used,
    Cancelled,
}

#[derive(Debug, PartialEq, Clone, RustcEncodable, RustcDecodable)]
struct Equipment {
    slot: String, // TODO: replace this with an enum?
    is_equipped: bool,
    power_bonus: i32,
    defense_bonus: i32,
    max_hp_bonus: i32,
}

fn get_equipped_in_slot(slot: &str, inventory: &[Object]) -> Option<usize> {
    for (inventory_id, item) in inventory.iter().enumerate() {
        if item.equipment.as_ref().map_or(false, |e| e.is_equipped && e.slot == slot) {
            return Some(inventory_id)
        }
    }
    None
}

fn is_blocked(x: i32, y: i32, map: &Map, objects: &[Object]) -> bool {
    // first test the map tile
    if map[x as usize][y as usize].blocked {
        return true;
    }
    // now check for any blocking objects
    objects.iter().any(|object| {
        object.blocks && object.pos() == (x, y)
    })
}

fn create_room(room: Rect, map: &mut Map) {
    // go through the tiles in the rectangle and make them passable
    for x in (room.x1 + 1)..room.x2 {
        for y in (room.y1 + 1)..room.y2 {
            let (x, y) = (x as usize, y as usize);
            map[x][y].blocked = false;
            map[x][y].block_sight = false;
        }
    }
}

fn create_h_tunnel(x1: i32, x2: i32, y: i32, map: &mut Map) {
    // horizontal tunnel. `min()` and `max()` are used in case `x1 > x2`
    for x in cmp::min(x1, x2)..(cmp::max(x1, x2) + 1) {
        let (x, y) = (x as usize, y as usize);
        map[x][y].blocked = false;
        map[x][y].block_sight = false;
    }
}

fn create_v_tunnel(y1: i32, y2: i32, x: i32, map: &mut Map) {
    for y in cmp::min(y1, y2)..(cmp::max(y1, y2) + 1) {
        let (x, y) = (x as usize, y as usize);
        map[x][y].blocked = false;
        map[x][y].block_sight = false;
    }
}

/// Generate range in the [low, high] (i.e. inclusive on both sides) interval.
fn range(min: i32, max: i32) -> i32 {
    // Rng::gen_range excludes the `max` value so we want to increment by one:
    rand::thread_rng().gen_range(min, max + 1)
}

fn make_map(objects: &mut Vec<Object>,
            level: i32)
            -> Map {
    // fill map with "blocked" tiles
    let mut map = vec![vec![Tile{blocked: true, explored: false, block_sight: true};
                            MAP_HEIGHT as usize];
                       MAP_WIDTH as usize];

    objects.truncate(1);  // Player is the first element, remove everything else

    let mut rooms = vec![];

    for _ in 0..MAX_ROOMS {
        // random width and height
        let w = range(ROOM_MIN_SIZE, ROOM_MAX_SIZE);
        let h = range(ROOM_MIN_SIZE, ROOM_MAX_SIZE);
        // random position without going out of the boundaries of the map
        let x = range(0, MAP_WIDTH - w - 1);
        let y = range(0, MAP_HEIGHT - h - 1);

        // "Rect" struct makes rectangles easier to work with
        let new_room = Rect::new(x, y, w, h);

        // run through the other rooms and see if they intersect with this one
        let failed = rooms.iter().any(|other_room| new_room.intersect(other_room));
        if !failed {
            // this means there are no intersections, so this room is valid

            // "paint" it to the map's tiles
            create_room(new_room, &mut map);

            // TODO: first time through, the player's position is "unitialised"
            // to (0, 0) here. Therefore, it's possible to place a monster or
            // item at the same position:

            // add some contents to this room, such as monsters
            place_objects(new_room, &map, objects, level);

            // center coordinates of the new room, will be useful later
            let (new_x, new_y) = new_room.center();

            if rooms.is_empty() {
                let player = &mut objects[PLAYER];
                // TODO: this is where we set player's position for the first
                // time. This should happen before we place any objects,
                // otherwise something could spawn here already.

                // this is the first room, where the player starts at
                player.set_pos(new_x, new_y);
            } else {
                // all rooms after the first:
                // connect it to the previous room with a tunnel

                // center coordinates of the previous room
                let (prev_x, prev_y) = rooms[rooms.len() - 1].center();

                // draw a coin (random bool value -- either true or false)
                if rand::random() {
                    // first move horizontally, then vertically
                    create_h_tunnel(prev_x, new_x, prev_y, &mut map);
                    create_v_tunnel(prev_y, new_y, new_x, &mut map);
                } else {
                    // first move vertically, then horizontally
                    create_v_tunnel(prev_y, new_y, prev_x, &mut map);
                    create_h_tunnel(prev_x, new_x, new_y, &mut map);
                }
            }

            // finally, append the new room to the list
            rooms.push(new_room);
        }
    }

    // create stairs at the center of the last room
    let (last_room_x, last_room_y) = rooms[rooms.len() - 1].center();
    let mut stairs = Object::new(last_room_x, last_room_y, '<', "stairs", colors::WHITE, false);
    stairs.always_visible = true;
    objects.push(stairs);

    map
}

#[derive(Clone, Copy)]
enum MonsterType {
    Orc,
    Troll,
}

#[derive(Clone, Copy)]
enum ItemType {
    Heal,
    Lighting,
    Fireball,
    Confuse,
    Sword,
    Shield,
}

fn from_dungeon_level(table: &[(u32, i32)], level: i32) -> u32 {
    // returns a value that depends on level. the table specifies
    // what value occurs after each level, default is 0.
    for &(value, table_level) in table.iter().rev() {
        if level >= table_level {
            return value;
        }
    }
    return 0;
}

fn place_objects(room: Rect, map: &Map, objects: &mut Vec<Object>, level: i32) {
    use rand::distributions::{Weighted, WeightedChoice, IndependentSample};
    let rng = &mut rand::thread_rng();

    // maximum number of monsters per room
    let max_monsters = from_dungeon_level(&[(2, 1), (3, 4), (5, 6)], level) as i32;


    // choose random number of monsters
    let num_monsters = range(0, max_monsters);

    // chance of each monster
    let troll_chance = from_dungeon_level(&[(15, 3), (30, 5), (60, 7)], level);
    let monster_chances = &mut [Weighted {weight: 80, item: MonsterType::Orc},
                                Weighted {weight: troll_chance, item: MonsterType::Troll}];
    let monster_choice = WeightedChoice::new(monster_chances);

    // maximum number of items per room
    let max_items = from_dungeon_level(&[(1, 1), (2, 4)], level) as i32;

    // chance of each item (by default they have a chance of 0 at level 1, which then goes up)
    let item_chances = &mut [Weighted {weight: 35, item: ItemType::Heal},
                             Weighted {weight: from_dungeon_level(&[(25, 4)], level),
                                       item: ItemType::Lighting},
                             Weighted {weight: from_dungeon_level(&[(25, 6)], level),
                                       item: ItemType::Fireball},
                             Weighted {weight: from_dungeon_level(&[(10, 2)], level),
                                       item: ItemType::Confuse},
                             Weighted {weight: from_dungeon_level(&[(5, 4)], level),
                                       item: ItemType::Sword},
                             Weighted {weight: from_dungeon_level(&[(15, 8)], level),
                                       item: ItemType::Shield}];
    let item_choice = WeightedChoice::new(item_chances);

    for _ in 0..num_monsters {
        // choose random spot for this monster
        let x = range(room.x1 + 1, room.x2 - 1);
        let y = range(room.y1 + 1, room.y2 - 1);

        // only place it if the tile is not blocked
        if !is_blocked(x, y, map, objects) {
            let monster = match monster_choice.ind_sample(rng) {
                MonsterType::Orc => {
                    // create an orc
                    let mut orc = Object::new(x, y, 'o', "orc", colors::DESATURATED_GREEN, true);
                    orc.fighter = Some(
                        Fighter{hp: 20, base_max_hp: 20, base_defense: 0, base_power: 4, xp: 35,
                                death: Some(DeathCallback::Monster)});
                    orc.ai = Some(MonsterAI{
                        old_ai: None,
                        ai_type: MonsterAIType::Basic,
                    });
                    orc
                },
                MonsterType::Troll => {
                    // create a troll
                    let mut troll = Object::new(x, y, 'T', "troll", colors::DARKER_GREEN, true);
                    troll.fighter = Some(
                        Fighter{hp: 30, base_max_hp: 30, base_defense: 2, base_power: 8, xp: 100,
                                death: Some(DeathCallback::Monster)});
                    troll.ai = Some(MonsterAI{
                        old_ai: None,
                        ai_type: MonsterAIType::Basic,
                    });
                    troll
                },
            };

            objects.push(monster);
        }
    }

    // choose random number of items
    let num_items = range(0, max_items);
    for _ in 0..num_items {
        // choose random spot for this item
        let x = range(room.x1 + 1, room.x2 - 1);
        let y = range(room.y1 + 1, room.y2 - 1);

        // only place it if the tile is not blocked
        if !is_blocked(x, y, map, objects) {
            // create a healing potion
            let item = match item_choice.ind_sample(rng) {
                ItemType::Heal => {
                    // create a healing potion
                    let item_component = Item::Heal;
                    let mut object = Object::new(x, y, '!', "healing potion",
                                                 colors::VIOLET, false);
                    object.item = Some(item_component);
                    object
                }
                ItemType::Lighting => {
                    // create a lightning bolt scroll
                    let item_component = Item::Lightning;
                    let mut object = Object::new(x, y, '#', "scroll of lightning bolt",
                                                 colors::LIGHT_YELLOW, false);
                    object.item = Some(item_component);
                    object
                }
                ItemType::Fireball => {
                    // create a fireball scroll
                    let item_component = Item::Fireball;
                    let mut object = Object::new(x, y, '#', "scroll of fireball",
                                                 colors::LIGHT_YELLOW, false);
                    object.item = Some(item_component);
                    object
                }
                ItemType::Confuse => {
                    // create a confuse scroll
                    let item_component = Item::Confuse;
                    let mut object = Object::new(x, y, '#', "scroll of confusion",
                                                 colors::LIGHT_YELLOW, false);
                    object.item = Some(item_component);
                    object
                }
                ItemType::Sword => {
                    // create a sword
                    let equipment_component = Equipment{
                        slot: "right hand".into(),
                        is_equipped: false,
                        power_bonus: 3,
                        defense_bonus: 0,
                        max_hp_bonus: 0,
                    };
                    let mut object = Object::new(x, y, '/', "sword", colors::SKY, false);
                    object.equipment = Some(equipment_component);
                    object.item = Some(Item::None);
                    object
                }
                ItemType::Shield => {
                    // create a sword
                    let equipment_component = Equipment{
                        slot: "left hand".into(),
                        is_equipped: false,
                        power_bonus: 0,
                        defense_bonus: 1,
                        max_hp_bonus: 0,
                    };
                    let mut object = Object::new(x, y, '[', "shield", colors::DARKER_ORANGE, false);
                    object.equipment = Some(equipment_component);
                    object.item = Some(Item::None);
                    object
                }
            };
            objects.push(item);
        }
    }
}

fn render_bar(panel: &mut Offscreen,
              x: i32,
              y: i32,
              total_width: i32,
              name: &str,
              value: i32,
              maximum: i32,
              bar_color: Color,
              back_color: Color) {
    // render a bar (HP, experience, etc). first calculate the width of the bar
    let bar_width = (value as f32 / maximum as f32 * total_width as f32) as i32;

    // render the background first
    panel.set_default_background(back_color);
    panel.rect(x, y, total_width, 1, false, BackgroundFlag::Screen);

    // now render the bar on top
    panel.set_default_background(bar_color);
    if bar_width > 0 {
        panel.rect(x, y, bar_width, 1, false, BackgroundFlag::Screen);
    }

    // finally, some centered text with the values
    panel.set_default_foreground(colors::WHITE);
    panel.print_ex(x + total_width / 2, y, BackgroundFlag::None, TextAlignment::Center,
                   &format!("{}: {}/{}", name, value, maximum));
}

fn get_names_under_mouse(mouse: Mouse, objects: &[Object], fov_map: &FovMap) -> String {
    // return a string with the names of all objects under the mouse
    let (x, y) = (mouse.cx as i32, mouse.cy as i32);

    // create a list with the names of all objects at the mouse's coordinates and in FOV
    objects.iter().filter(
        |obj| {
            obj.pos() == (x, y) && fov_map.is_in_fov(obj.x, obj.y)
        }).map(|obj| obj.name.clone()).collect::<Vec<_>>().connect(", ")
}

fn render_all(objects: &[Object], game: &mut Game, tcod: &mut TcodState) {
    if game.fov_recompute {
        game.fov_recompute = false;
        let (player_x, player_y) = objects[PLAYER].pos();
        tcod.fov_map.compute_fov(player_x, player_y, TORCH_RADIUS, FOV_LIGHT_WALLS, FOV_ALGO);

        // go through all tiles, and set their background color according to the FOV
        for y in 0..MAP_HEIGHT {
            for x in 0..MAP_WIDTH {
                let visible = tcod.fov_map.is_in_fov(x, y);
                let wall = game.map[x as usize][y as usize].block_sight;
                if !visible {
                    // if it's not visible right now, the player can only see if it's explored
                    if game.map[x as usize][y as usize].explored {
                        if wall {
                            tcod.con.set_char_background(
                                x, y, COLOR_DARK_WALL, BackgroundFlag::Set);
                        } else {
                            tcod.con.set_char_background(
                                x, y, COLOR_DARK_GROUND, BackgroundFlag::Set);
                        }
                    }
                } else {
                    // it's visible
                    if wall {
                        tcod.con.set_char_background(x, y, COLOR_LIGHT_WALL, BackgroundFlag::Set);
                    } else {
                        tcod.con.set_char_background(x, y, COLOR_LIGHT_GROUND, BackgroundFlag::Set);
                    }
                    // since it's visible, explore it
                    game.map[x as usize][y as usize].explored = true;
                }
            }
        }
    }

    // Grab all renderable objects
    let mut render_objects: Vec<_> = objects.iter().collect();
    // Put the fighters first, then items, then everything else. This will not
    // affect the order of the original objects vector.
    render_objects.sort_by(|o1, o2| {
        if o1.fighter.is_some() || o2.fighter.is_some() {
            return o1.fighter.is_some().cmp(&o2.fighter.is_some());
        }
        if o1.item.is_some() || o2.item.is_some() {
            return o1.item.is_some().cmp(&o2.item.is_some());
        }
        Ordering::Equal
    });
    for object in &render_objects {
        object.draw(&mut tcod.con, &game.map, &tcod.fov_map);
    }

    // blit the contents of "con" to the root console
    tcod::console::blit(&mut tcod.con,
                        (0, 0),
                        (MAP_WIDTH, MAP_HEIGHT),
                        &mut tcod.root,
                        (0, 0),
                        1.0,
                        1.0);

    // prepare to render the GUI panel
    tcod.panel.set_default_background(colors::BLACK);
    tcod.panel.clear();

    // print the game messages, one line at a time
    let mut y = MSG_HEIGHT as i32;
    for &(ref msg, color) in game.log.messages().iter().rev() {
        let msg_height = tcod.panel.get_height_rect(MSG_X, y, MSG_WIDTH, 0, msg);
        y -= msg_height;
        // TODO: this won't print a partial message if it crosses multiple lines. Can we fix that?
        if y < 0 {
            break;
        }
        tcod.panel.set_default_foreground(color);
        tcod.panel.print_rect_ex(MSG_X, y, MSG_WIDTH, 0,
                            BackgroundFlag::None, TextAlignment::Left, msg);
    }

    // show the player's stats
    let hp = objects[PLAYER].fighter.as_ref().map_or(0, |f| f.hp);
    let max_hp = objects[PLAYER].full_max_hp(game);
    render_bar(&mut tcod.panel,
               1,
               1,
               BAR_WIDTH,
               "HP",
               hp,
               max_hp,
               colors::LIGHT_RED,
               colors::DARKER_RED);
    tcod.panel.print_ex(1, 3, BackgroundFlag::None, TextAlignment::Left,
                        format!("Dungeon level: {}", game.dungeon_level));

    // display names of objects under the mouse
    tcod.panel.set_default_foreground(colors::LIGHT_GREY);
    let names = get_names_under_mouse(tcod.mouse, objects, &tcod.fov_map);
    tcod.panel.print_ex(1, 0, BackgroundFlag::None, TextAlignment::Left, names);

    // blit the contents of `panel` to the root console
    tcod::console::blit(&mut tcod.panel,
                        (0, 0),
                        (SCREEN_WIDTH, PANEL_HEIGHT),
                        &mut tcod.root,
                        (0, PANEL_Y),
                        1.0,
                        1.0);
}

fn player_move_or_attack(dx: i32, dy: i32, objects: &mut [Object], game: &mut Game) {
    // the coordinates the player is moving to/attacking
    let (x, y) = {
        let p = &objects[PLAYER];
        (p.x + dx, p.y + dy)
    };

    // try to find an attackable object there
    let target_id = objects.iter().position(|object| {
        object.fighter.is_some() && object.pos() == (x, y)
    });

    // attack if target found, move otherwise
    match target_id {
        Some(target_id) => {
            let (player, target) = mut_two(PLAYER, target_id, objects);
            player.attack(target, game);
        }
        None => {
            move_by(PLAYER, dx, dy, objects, game);
            game.fov_recompute = true;
        }
    }
}

fn handle_keys(objects: &mut Vec<Object>, game: &mut Game, tcod: &mut TcodState, event: Option<Event>) -> PlayerAction {
    use tcod::input::KeyCode::*;
    let key = if let Some(Event::Key(key)) = event {
        key
    } else {
        return PlayerAction::DidntTakeTurn;
    };
    // Alt+Enter: toggle fullscreen
    if let Key { code: Enter, alt: true, .. } = key {
        let fullscreen = !tcod.root.is_fullscreen();
        tcod.root.set_fullscreen(fullscreen);
    } else if key.code == Escape {
        return PlayerAction::Exit;  // exit game
    }
    if game.state == GameState::Playing {
        match key {
            // movement keys
            Key { code: Up, .. } | Key { code: NumPad8, .. } => {
                player_move_or_attack(0, -1, objects, game);
                return PlayerAction::None;
            }
            Key { code: Down, .. } | Key { code: NumPad2, .. } => {
                player_move_or_attack(0, 1, objects, game);
                return PlayerAction::None;
            }
            Key { code: Left, .. } | Key { code: NumPad4, .. } => {
                player_move_or_attack(-1, 0, objects, game);
                return PlayerAction::None;
            }
            Key { code: Right, .. } | Key { code: NumPad6, .. } => {
                player_move_or_attack(1, 0, objects, game);
                return PlayerAction::None;
            }
            Key { code: Home, .. } | Key { code: NumPad7, .. } => {
                player_move_or_attack(-1, -1, objects, game);
                return PlayerAction::None;
            }
            Key { code: PageUp, .. } | Key { code: NumPad9, .. } => {
                player_move_or_attack(1, -1, objects, game);
                return PlayerAction::None;
            }
            Key { code: End, .. } | Key { code: NumPad1, .. } => {
                player_move_or_attack(-1, 1, objects, game);
                return PlayerAction::None;
            }
            Key { code: PageDown, .. } | Key { code: NumPad3, .. } => {
                player_move_or_attack(1, 1, objects, game);
                return PlayerAction::None;
            }
            Key { code: NumPad5, .. } => {
                return PlayerAction::None;  // do nothing ie wait for the monster to come to you
            }
            Key { printable: 'g', .. } => {
                let (px, py) = objects[PLAYER].pos();
                let item_id = objects.iter().position(|object| {
                    object.pos() == (px, py) && object.item.is_some()
                });
                // pick up an item
                if let Some(item_id) = item_id {
                    pick_item_up(item_id, objects, game);
                }
            }
            Key { printable: 'i', .. } => {
                // show the inventory; if an item is selected, use it
                let inventory_index = tcod.inventory_menu(
                    game,
                    "Press the key next to an item to use it, or any other to cancel.\n");
                if let Some(inventory_index) = inventory_index {
                    use_item(inventory_index, objects, game, tcod);
                }
            }
            Key { printable: 'd', .. } => {
                // show the inventory; if an item is selected, drop it
                let inventory_index = tcod.inventory_menu(
                    game,
                    "Press the key next to an item to drop it, or any other to cancel.\n");
                if let Some(inventory_index) = inventory_index {
                    drop_item(inventory_index, objects, game);
                }
            }
            Key { printable: 'c', .. } => {
                // show character information
                let player = &objects[PLAYER];
                let level = player.level;
                let level_up_xp = LEVEL_UP_BASE + level * LEVEL_UP_FACTOR;
                let power = player.full_power(game);
                let defense = player.full_defense(game);
                let max_hp = player.full_max_hp(game);
                if let Some(fighter) = player.fighter.as_ref() {
                    let msg = format!(
                        "Character information\n\nLevel: {}\nExperience: {}\nExperience to level \
                         up: {}\n\nMaximum HP: {}\nAttack: {}\nDefense: {}",
                        level, fighter.xp, level_up_xp, max_hp, power, defense);
                    tcod.msgbox(&msg, CHARACTER_SCREEN_WIDTH);
                }
            }
            Key { printable: '<', .. } => {
                // go down stairs, if the player is on them
                let player_pos = objects[PLAYER].pos();
                let player_stands_on_stairs = objects.iter().any(|object| {
                    object.pos() == player_pos && object.name == "stairs"
                });
                if player_stands_on_stairs {
                    game.next_level(objects, tcod);
                }
            }
            _ => { }
        }
    }
    return PlayerAction::DidntTakeTurn;
}

fn check_level_up(objects: &mut [Object], game: &mut Game, tcod: &mut TcodState) {
    // see if the player's experience is enough to level-up
    let level_up_xp = LEVEL_UP_BASE + objects[PLAYER].level * LEVEL_UP_FACTOR;
    // TODO: NOTE: We have to pull max_hp etc. out because since it's taken
    // out inside the block, we'd get back zero. Maybe reconsider the `take` strategy?
    let player = &mut objects[PLAYER];
    let power = player.full_power(game);
    let defense = player.full_defense(game);
    let max_hp = player.full_max_hp(game);
    let mut fighter = player.fighter.take().unwrap();
    if fighter.xp >= level_up_xp {
        // it is! level up
        player.level += 1;
        fighter.xp -= level_up_xp;
        game.log.add(format!("Your battle skills grow stronger! You reached level {}!",
                             player.level),
                     colors::YELLOW);

        loop {  // keep asking until a choice is made
            let choice = tcod.menu("Level up! Choose a stat to raise:\n",
                                   &[format!("Constitution (+20 HP, from {})", max_hp),
                                     format!("Strength (+1 attack, from {})", power),
                                     format!("Agility (+1 defense, from {})", defense)],
                                   LEVEL_SCREEN_WIDTH);
            match choice {
                Some(0) => {
                    fighter.base_max_hp += 20;
                    fighter.hp += 20;
                    break;
                }
                Some(1) => {
                    fighter.base_power += 1;
                    break;
                }
                Some(2) => {
                    fighter.base_defense += 1;
                    break;
                }
                _ => continue
            }
        }
    }
    player.fighter = Some(fighter);
}

#[derive(Copy, Clone, PartialEq)]
enum PlayerAction {
    None,
    DidntTakeTurn,
    Exit,
}

#[derive(Copy, Clone, PartialEq, RustcEncodable, RustcDecodable)]
enum GameState {
    Playing,
    Death,
}

fn player_death(player: &mut Object, game: &mut Game) {
    // the game ended!
    game.log.add("You died!", colors::RED);
    game.state = GameState::Death;

    // for added effect, transform the player into a corpse!
    player.char = '%';
    player.color = colors::DARK_RED;
}

fn monster_death(monster: &mut Object, game: &mut Game) {
    // transform it into a nasty corpse! it doesn't block, can't be
    // attacked and doesn't move
    game.log.add(format!("{} is dead! You gain {} experience points.",
                         monster.name,
                         monster.fighter.as_ref().unwrap().xp),
                 colors::ORANGE);
    monster.char = '%';
    monster.color = colors::DARK_RED;
    monster.blocks = false;
    monster.fighter = None;
    monster.ai = None;
    monster.name = format!("remains of {}", monster.name);
}

/// return the position of a tile left-clicked in player's FOV (optionally in a
/// range), or (None,None) if right-clicked.
fn target_tile(objects: &[Object],
               game: &mut Game,
               tcod: &mut TcodState,
               max_range: Option<f32>)
               -> Option<(i32, i32)> {
    use tcod::input::KeyCode::Escape;
    loop {
        // render the screen. this erases the inventory and shows the names of
        // objects under the mouse.
        tcod.root.flush();
        let event = input::check_for_event(input::KEY_PRESS | input::MOUSE).map(|e| e.1);
        let mut key = None;
        match event {
            Some(Event::Mouse(m)) => tcod.mouse = m,
            Some(Event::Key(k)) => key = Some(k),
            None => {}
        }
        render_all(objects, game, tcod);

        let (x, y) = (tcod.mouse.cx as i32, tcod.mouse.cy as i32);

        // accept the target if the player clicked in FOV, and in case a range
        // is specified, if it's in that range
        let in_fov = tcod.fov_map.is_in_fov(x, y);
        let in_range = max_range.map_or(
            true, |range| objects[PLAYER].distance(x, y) <= range);
        if tcod.mouse.lbutton_pressed && in_fov && in_range {
            return Some((x, y))
        }

        let escape = key.map_or(false, |k| k.code == Escape);
        if tcod.mouse.rbutton_pressed || escape {
            return None  // cancel if the player right-clicked or pressed Escape
        }
    }
}


/// returns a clicked monster inside FOV up to a range, or None if right-clicked
fn target_monster(objects: &[Object], game: &mut Game, tcod: &mut TcodState, max_range: Option<f32>) -> Option<usize> {
    loop {
        match target_tile(objects, game, tcod, max_range) {
            None => return None,
            Some((x, y)) => {
                // return the first clicked monster, otherwise continue looping
                for (id, obj) in objects.iter().enumerate() {
                    if obj.pos() == (x, y) && obj.fighter.is_some() && id != PLAYER {
                        return Some(id)
                    }
                }
            }
        }
    }
}

fn closest_monster(max_range: i32, objects: &mut [Object], tcod: &TcodState) -> Option<usize> {
    // find closest enemy, up to a maximum range, and in the player's FOV
    let mut closest_enemy = None;
    let mut closest_dist = (max_range + 1) as f32;  // start with (slightly more than) maximum range

    // TODO: this could be done more succinctly with Iter::min_by but that's unstable now.
    for (id, object) in objects.iter().enumerate() {
        if id != PLAYER && object.fighter.is_some() &&
           tcod.fov_map.is_in_fov(object.x, object.y) {
            // calculate distance between this object and the player
            let dist = objects[PLAYER].distance_to(object);
            if dist < closest_dist {  // it's closer, so remember it
                closest_enemy = Some(id);
                closest_dist = dist;
            }
        }
    }
    closest_enemy
}

fn cast_heal(objects: &mut [Object], game: &mut Game, _tcod: &mut TcodState) -> UseResult {
    let player = &mut objects[PLAYER];
    // heal the player
    let max_hp = player.full_max_hp(game);
    // TODO: NOTE: We have to pull max_hp out because since it's taken
    // out inside the block, we'd get back zero. Maybe reconsider the `take` strategy?
    if let Some(mut fighter) = player.fighter.take() {
        if fighter.hp == max_hp {
            game.log.add("You are already at full health.", colors::RED);
            player.fighter = Some(fighter);
            return UseResult::Cancelled;
        }
        game.log.add("Your wounds start to feel better!", colors::LIGHT_VIOLET);
        fighter.heal(HEAL_AMOUNT);
        player.fighter = Some(fighter);
        return UseResult::Used;
    }
    return UseResult::Cancelled;
}

fn cast_lightning(objects: &mut [Object], game: &mut Game, tcod: &mut TcodState) -> UseResult {
    // find closest enemy (inside a maximum range) and damage it
    let monster_id = closest_monster(LIGHTNING_RANGE, objects, tcod);
    if let Some(monster_id) = monster_id {
        // zap it!
        game.log.add(format!("A lightning bolt strikes the {} with a loud thunder! \
                              The damage is {} hit points.",
                             objects[monster_id].name, LIGHTNING_DAMAGE),
                     colors::LIGHT_BLUE);
        objects[monster_id].take_damage(LIGHTNING_DAMAGE, game).map(|xp| {
            objects[PLAYER].fighter.as_mut().unwrap().xp += xp;
        });
        UseResult::Used
    } else {  // no enemy found within maximum range
        game.log.add("No enemy is close enough to strike.", colors::RED);
        UseResult::Cancelled
    }
}

fn cast_fireball(objects: &mut [Object], game: &mut Game, tcod: &mut TcodState) -> UseResult {
    // ask the player for a target tile to throw a fireball at
    game.log.add("Left-click a target tile for the fireball, or right-click to cancel.",
                 colors::LIGHT_CYAN);
    let (x, y) = match target_tile(objects, game, tcod, None) {
        Some(tile_pos) => tile_pos,
        None => { return UseResult::Cancelled },
    };
    game.log.add(format!("The fireball explodes, burning everything within {} tiles!",
                         FIREBALL_RADIUS),
                 colors::ORANGE);

    // find every fighter in range, including the player
    let burned_objects: Vec<_> = objects.iter()
        .enumerate()
        .filter(|&(_id, obj)| obj.distance(x, y) <= FIREBALL_RADIUS as f32 && obj.fighter.is_some())
        .map(|(id, _obj)| id)
        .collect();
    for &id in &burned_objects {
        game.log.add(format!("The {} gets burned for {} hit points.",
                             objects[id].name, FIREBALL_DAMAGE),
                     colors::ORANGE);
        objects[id].take_damage(FIREBALL_DAMAGE, game).map(|xp| {
            if id != PLAYER {
                objects[PLAYER].fighter.as_mut().unwrap().xp += xp;
            }
        });
    }
    UseResult::Used
}

fn cast_confuse(objects: &mut [Object], game: &mut Game, tcod: &mut TcodState) -> UseResult {
    // ask the player for a target to confuse
    game.log.add("Left-click an enemy to confuse it, or right-click to cancel.",
                 colors::LIGHT_CYAN);
    target_monster(objects, game, tcod, Some(CONFUSE_RANGE as f32)).map_or(UseResult::Cancelled, |id| {
        // replace the monster's AI with a "confused" one; after some
        // turns it will restore the old AI
        {
            let mut monster = &mut objects[id];
            let old_ai = monster.ai.take();
            let confuse_ai = MonsterAI {
                old_ai: old_ai.map(|ai| Box::new(ai)),
                ai_type: MonsterAIType::Confused{num_turns: CONFUSE_NUM_TURNS},
            };
            monster.ai = Some(confuse_ai);
        }
        game.log.add(format!("The eyes of the {} look vacant, as he starts to stumble around!",
                             objects[id].name),
                     colors::GREEN);
        UseResult::Used
    })
}

// This is a no-op function for items that have any effect by
// themselves. E.g. Equimpent is also an item, but its use action is
// special-cased.
fn cast_nothing(_objects: &mut [Object], _game: &mut Game, _tcod: &mut TcodState) -> UseResult {
    UseResult::Used
}


struct TcodState {
    root: Root,
    con: Offscreen,
    panel: Offscreen,
    fov_map: FovMap,
    mouse: Mouse,
}

impl TcodState {
    fn new(root: Root, con: Offscreen, panel: Offscreen) -> Self {
        TcodState {
            root: root,
            con: con,
            panel: panel,
            fov_map: FovMap::new(MAP_WIDTH, MAP_HEIGHT),
            mouse: Default::default(),
        }
    }

    fn menu<T: AsRef<str>>(&mut self, header: &str, options: &[T], width: i32) -> Option<usize> {
        assert!(options.len() <= 26, "Cannot have a menu with more than 26 options.");

        // calculate total height for the header (after auto-wrap) and one line per option
        let header_height = self.con.get_height_rect(0, 0, width, SCREEN_HEIGHT, header);
        let height = options.len() as i32 + header_height;

        // create an off-screen console that represents the menu's window
        let mut window = Offscreen::new(width, height);

        // print the header, with auto-wrap
        window.set_default_foreground(colors::WHITE);
        window.print_rect_ex(0, 0, width, height, BackgroundFlag::None, TextAlignment::Left, header);

        // print all the options
        let first_letter = 'A' as u8;
        for (index, option_text) in options.iter().enumerate() {
            let text = format!("({}) {}", (first_letter + index as u8) as char, option_text.as_ref());
            window.print_ex(0, header_height + index as i32,
                            BackgroundFlag::None, TextAlignment::Left, text);
        }

        // blit the contents of "window" to the root console
        let x = SCREEN_WIDTH / 2 - width / 2;
        let y = SCREEN_HEIGHT / 2 - height / 2;
        tcod::console::blit(&mut window, (0, 0), (width, height), &mut self.root, (x, y), 1.0, 0.7);

        // present the root console to the player and wait for a key-press
        self.root.flush();
        let key = self.root.wait_for_keypress(true);
        if key.printable.is_alphabetic() {
            let index = key.printable.to_ascii_uppercase() as usize - 'A' as usize;
            if index < options.len() {
                Some(index)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn inventory_menu(&mut self, game: &mut Game, header: &str) -> Option<usize> {
        // how a menu with each item of the inventory as an option
        let options = if game.inventory.len() == 0 {
            vec!["Inventory is empty.".into()]
        } else {
            game.inventory.iter().map(|item| {
                // show additional information, in case it's equipped
                let text = match item.equipment.as_ref() {
                    Some(equipment) if equipment.is_equipped => {
                        format!("{} (on {})", item.name, equipment.slot)
                    }
                    _ => {
                        item.name.clone()
                    }
                };
                text
            }).collect()
        };
        let inventory_index = self.menu(header, &options, INVENTORY_WIDTH);

        // if an item was chosen, return it
        if game.inventory.len() > 0 {
            inventory_index
        } else {
            None
        }
    }

    fn msgbox(&mut self, text: &str, width: i32) {
        let options: &[&str; 0] = &[];  // Need to annotate the type here else Rust gets confused :-(
        self.menu(text, options, width);  // use menu() as a sort of "message_box"
    }
}

#[derive(RustcEncodable, RustcDecodable)]
struct MessageLog {
    messages: Vec<(String, Color)>,
}

impl MessageLog {
    fn new() -> Self {
        MessageLog { messages: vec![] }
    }

    fn add<T: Into<String>>(&mut self, message: T, color: Color) {
        // if the buffer is full, remove the first message to make room for the new one
        if self.messages.len() == MSG_HEIGHT {
            self.messages.remove(0);
        }
        // add the new line as a tuple, with the text and the color
        self.messages.push((message.into(), color));
    }

    fn messages(&self) -> &Vec<(String, Color)> {
        &self.messages
    }
}

#[derive(RustcEncodable, RustcDecodable)]
struct Game {
    state: GameState,
    dungeon_level: i32,
    map: Map,
    fov_recompute: bool,
    log: MessageLog,
    inventory: Vec<Object>,
}

impl Game {
    // TODO: this should not return the objects vec as well!
    fn new(tcod: &mut TcodState) -> (Self, Vec<Object>) {
        // create object representing the player
        let mut player = Object::new(0, 0, '@', "player", colors::WHITE, true);
        player.fighter = Some(
            Fighter{
                hp: 100, base_max_hp: 100, base_defense: 1, base_power: 2, xp: 0,
                death: Some(DeathCallback::Player)});
        player.level = 1;

        let mut objects = vec![player];
        let dungeon_level = 1;

        // Generate map (at this point it's not drawn to the screen)
        let mut game = Game {
            state: GameState::Playing,
            dungeon_level: dungeon_level,
            map: make_map(&mut objects,
                          dungeon_level),
            fov_recompute: false,
            // create the list of game messages and their colors, starts empty
            log: MessageLog::new(),
            inventory: vec![],
        };
        game.initialize_fov(tcod);
        // a warm welcoming message!
        game.log.add("Welcome stranger! Prepare to perish in the Tombs of the Ancient Kings.",
                          colors::RED);

        // initial equipment: a dagger
        let mut dagger = Object::new(0, 0, '-', "dagger", colors::SKY, false);
        let equipment_component = Equipment {
            slot: "right hand".into(),
            is_equipped: false,
            power_bonus: 2,
            defense_bonus: 0,
            max_hp_bonus: 0,
        };
        dagger.equipment = Some(equipment_component);
        dagger.item = Some(Item::None);
        let dagger_id = game.inventory.len();
        game.inventory.push(dagger);
        equip(dagger_id, &mut game);

        (game, objects)
    }

    fn next_level(&mut self, objects: &mut Vec<Object>, tcod: &mut TcodState) {
        // advance to the next level
        self.log.add(
            "You take a moment to rest, and recover your strength.", colors::LIGHT_VIOLET);
        {
            let player = &mut objects[PLAYER];
            let max_hp = player.full_max_hp(self);
            player.fighter.as_mut().map(|f| {
                let heal_hp = max_hp / 2;
                f.heal(heal_hp);
            });  // heal the player by 50%
        }

        self.log.add(
            "After a rare moment of peace, you descend deeper into the heart of the dungeon...",
            colors::RED);
        self.dungeon_level += 1;
        // create a fresh new level!
        self.map = make_map(objects, self.dungeon_level);
        self.initialize_fov(tcod);
    }

    fn initialize_fov(&mut self, tcod: &mut TcodState) {
        self.fov_recompute = true;
        // create the FOV map, according to the generated map
        for y in 0..MAP_HEIGHT {
            for x in 0..MAP_WIDTH {
                tcod.fov_map.set(x, y,
                                 !self.map[x as usize][y as usize].block_sight,
                                 !self.map[x as usize][y as usize].blocked);
            }
        }

        tcod.con.clear();  // unexplored areas start black (which is the default background color)
    }

    fn save_game(&self, objects: &[Object]) {
        let json_save_state = json::encode(&(self, objects)).unwrap();
        let mut file = File::create("savegame").unwrap();
        file.write_all(json_save_state.as_bytes()).unwrap();
    }

    fn load_game(tcod: &mut TcodState) -> Result<(Self, Vec<Object>), Error> {
        use std::io::ErrorKind::InvalidData;
        let mut json_save_state = String::new();
        let mut file = try!{ File::open("savegame") };
        try!{ file.read_to_string(&mut json_save_state) };
        let (mut game, objects) = try!{
            json::decode::<(Game, Vec<Object>)>(&json_save_state).map_err(|e| Error::new(InvalidData, e))
        };
        game.initialize_fov(tcod);
        Ok((game, objects))
    }

    fn play_game(&mut self, objects: &mut Vec<Object>, tcod: &mut TcodState) {
        let mut player_action;
        while !tcod.root.window_closed() {
            let event = input::check_for_event(input::KEY_PRESS | input::MOUSE).map(|e| e.1);
            if let Some(Event::Mouse(m)) = event {
                tcod.mouse = m;
            }
            // render the screen
            render_all(objects, self, tcod);

            tcod.root.flush();

            // level up if needed
            check_level_up(objects, self, tcod);

            // erase all objects at their old location, before they move
            for object in objects.iter_mut() {
                object.clear(&mut tcod.con);
            }

            // handle keys and exit game if needed
            player_action = handle_keys(objects, self, tcod, event);
            if player_action == PlayerAction::Exit {
                self.save_game(objects);
                break;
            }

            // let monsters take their turn
            if self.state == GameState::Playing && player_action != PlayerAction::DidntTakeTurn {
                // We have to use indexes here otherwise we get a double borrow of `objects`
                // TODO: this will fail if we reorder objects or remove some!!!
                // NOTE: reversing the order lets us remove the
                // current item. But we're no doing that, soo...
                for id in (0..objects.len()).rev() {
                    let ai = objects[id].ai.take();
                    if let Some(mut old_ai) = ai {
                        let new_ai = old_ai.take_turn(id, objects, self, tcod);
                        objects[id].ai = new_ai.or(Some(old_ai));
                    }
                }
            }
        }
    }
}

fn main_menu(root: Root, con: Offscreen, panel: Offscreen) {
    let img = tcod::image::Image::from_file("menu_background.png").ok().expect(
        "Background image not found");

    let mut tcod = TcodState::new(root, con, panel);

    while !tcod.root.window_closed() {
        // show the background image, at twice the regular console resolution
        tcod::image::blit_2x(&img, (0, 0), (-1, -1), &mut tcod.root, (0, 0));

        // show options and wait for the player's choice
        let choices = &["Play a new game", "Continue last game", "Quit"];
        let choice = tcod.menu("", choices, 24);

        match choice {
            Some(0) => {  // new game
                let (mut game, mut objects) = Game::new(&mut tcod);
                return game.play_game(&mut objects, &mut tcod);
            }
            Some(1) => {  // load last game
                match Game::load_game(&mut tcod) {
                    Ok((mut game, mut objects)) => {
                        return game.play_game(&mut objects, &mut tcod);
                    }
                    Err(_) => {
                        tcod.msgbox("\n No saved game to load.\n", 24);
                    }
                }
            }
            Some(2) => {  // quit
                break
            }
            _ => {}
        }
    }
}


fn main() {
    let root = Root::initializer()
        .font("arial10x10.png", FontLayout::Tcod)
        .font_type(FontType::Greyscale)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Rust/libtcod tutorial")
        .init();
    tcod::system::set_fps(LIMIT_FPS);
    let con = Offscreen::new(MAP_WIDTH, MAP_HEIGHT);
    let panel = Offscreen::new(SCREEN_WIDTH, PANEL_HEIGHT);

    main_menu(root, con, panel);
}

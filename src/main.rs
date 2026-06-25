use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::{mpsc};
use serde::Deserialize;


enum ViewScope {
	Global,
	Room,
	Group
}

enum Trigger<'npc> {
	Reach,
	Collect,
	Defeat(&'npc str),
	Talk(&'npc str)
}

#[derive(Clone, Debug, PartialEq)]
enum Status {
	Poison
}

impl Status {
	fn as_str(&self) -> &str {
		match self {
			Status::Poison => "poisoned",
		}
	}
}

#[derive(Clone)]
struct Player {
    name: String,
    room_id: String,
    group_id: Option<String>,
	invites: Vec<String>,
    hp: i32,
    max_hp: i32,
    inventory: Vec<String>,
    active_quests: HashMap<String, usize>,
    completed_quests: HashSet<String>,
    statuses: Vec<Status>
}

#[derive(Deserialize, Debug, Clone)]
struct Item {
    name: String,
    description: String,
    #[serde(default = "default_true")]
    obtainable: bool,
}

fn default_true() -> bool { true }

#[derive(Deserialize, Debug, Clone)]
struct Npc {
    name: String,
    description: String,
    dialogue: Vec<String>,
    hp: i32,
    #[serde(default = "default_friendly")]
    npc_type: String,
}

fn default_friendly() -> String { "friendly".to_string() }

#[derive(Deserialize, Debug, Clone)]
struct Room {
    name: String,
    description: String,
    exits: HashMap<String, String>,
    #[serde(default)]
    items: HashSet<String>,
    #[serde(default)]
    npcs: HashSet<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
enum StepKind {
    Reach { room: String },
    Collect { item: String, #[serde(default = "default_count")] count: u32 },
    Talk { npc: String },
    Defeat { npc: String }
}

fn default_count() -> u32 { 1 }

#[derive(Deserialize, Debug, Clone)]
struct QuestStep {
    description: String,
    kind: StepKind
}

#[derive(Deserialize, Debug, Clone)]
struct Quest {
    giver: String,
    description: String,
    steps: Vec<QuestStep>,
    reward: String
}

#[derive(Deserialize, Debug)]
struct WorldConfig {
    rooms: HashMap<String, Room>,
    #[serde(default)]
    items: HashMap<String, Item>,
    #[serde(default)]
    npcs: HashMap<String, Npc>,
    #[serde(default)]
    quests: HashMap<String, Quest>,
}

struct World {
    players: HashMap<String, Player>,
    rooms: HashMap<String, Room>,
    items: HashMap<String, Item>,
    npcs: HashMap<String, Npc>,
    quests: HashMap<String, Quest>,
}

impl World {
    fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let file = File::open(path)?;
        let config: WorldConfig = serde_yaml::from_reader(file)?;

        for (room_id, room) in &config.rooms {
            for (direction, target_room) in &room.exits {
                if !config.rooms.contains_key(target_room) {
                    return Err(format!(
                        "Validation Error: Dans la pièce '{}', l'exit '{}' pointe vers '{}' (inexistant)!",
                        room_id, direction, target_room
                    ).into());
                }
            }
            for item_id in &room.items {
                if !config.items.contains_key(item_id) {
                    return Err(format!(
                        "Validation Error: La pièce '{}' contient l'item '{}' absent du registre global !",
                        room_id, item_id
                    ).into());
                }
            }
            for npc_id in &room.npcs {
                if !config.npcs.contains_key(npc_id) {
                    return Err(format!(
                        "Validation Error: La pièce '{}' contient le NPC '{}' absent du registre global !",
                        room_id, npc_id
                    ).into());
                }
            }
        }

        for (quest_id, quest) in &config.quests {
            if !config.npcs.contains_key(&quest.giver) {
                return Err(format!(
                    "Validation Error: La quête '{}' a un giver '{}' inexistant !",
                    quest_id, quest.giver
                ).into());
            }
            if !config.items.contains_key(&quest.reward) {
                return Err(format!(
                    "Validation Error: La quête '{}' a une récompense '{}' inexistante !",
                    quest_id, quest.reward
                ).into());
            }
            for step in &quest.steps {
                let (exists, target) = match &step.kind {
                    StepKind::Reach { room } => (config.rooms.contains_key(room), room),
                    StepKind::Collect { item, .. } => (config.items.contains_key(item), item),
                    StepKind::Talk { npc } | StepKind::Defeat { npc } => (config.npcs.contains_key(npc), npc),
                };
                if !exists {
                    return Err(format!(
                        "Validation Error: La quête '{}' référence une cible '{}' inexistante !",
                        quest_id, target
                    ).into());
                }
            }
        }

        println!(
            "Monde validé ! {} pièces, {} objets, {} NPCs et {} quêtes chargés.",
            config.rooms.len(), config.items.len(), config.npcs.len(), config.quests.len()
        );

        Ok(World {
            players: HashMap::new(),
            rooms: config.rooms,
            items: config.items,
            npcs: config.npcs,
            quests: config.quests,
        })
    }
}


type Event = (Vec<String>, String);
type SharedWorld = Arc<Mutex<World>>;
type Mailboxes = Arc<Mutex<HashMap<String, mpsc::UnboundedSender<String>>>>;


fn players_in_scope(
	player: &Player,
	world: &World,
	scope: ViewScope
) -> Vec<String> {
	match scope {
		ViewScope::Global => world.players.keys().cloned().collect(),
		ViewScope::Room => {
			world.players
				.iter()
				.filter(|(_, p)| p.room_id == player.room_id)
				.map(|(name, _)| name.clone()).collect()
		},
		ViewScope::Group => match &player.group_id {
			Some(_) => world.players
				.iter()
				.filter(|(_, p)| p.group_id == player.group_id)
				.map(|(name, _)| name.clone()).collect(),
			None => Vec::new()
		}
	}
}


fn leave_group(world: &mut World, name: &str) -> Vec<Event> {
	let Some(player) = world.players.get(name) else {
		return Vec::new();
	};
	let Some(group_name) = player.group_id.clone() else {
		return Vec::new();
	};

	if name == group_name.as_str() {
		let members: Vec<String> = world.players.iter()
			.filter(|(_, p)| p.group_id.as_deref() == Some(group_name.as_str()))
			.map(|(n, _)| n.clone())
			.collect();

		for member in &members {
			if let Some(p) = world.players.get_mut(member) {
				p.group_id = None;
			}
		}

		let recipients: Vec<String> = members.into_iter()
			.filter(|n| n.as_str() != name).collect();
		vec![(recipients, format!("EVT GROUP LEAVE {}\n", name))]
	} else {
		let recipients: Vec<String> = {
			let player = world.players.get(name).unwrap();
			players_in_scope(player, world, ViewScope::Group)
				.into_iter().filter(|n| n.as_str() != name).collect()
		};

		if let Some(p) = world.players.get_mut(name) {
			p.group_id = None;
		}

		vec![(recipients, format!("EVT GROUP LEAVE {}\n", name))]
	}
}


fn advance_quests(world: &mut World, name: &str, trigger: Trigger<'_>) -> Vec<Event> {
	let mut events: Vec<Event> = Vec::new();
	let active_quests: Vec<String> = match world.players.get(name) {
		Some(player) => player.active_quests.keys().cloned().collect(),
		None => return events
	};

	for quest_id in active_quests {
		let quest = match world.quests.get(&quest_id) {
			Some(q) => q,
			None => continue
		};

		let step = match world.players.get(name).unwrap().active_quests.get(&quest_id) {
			Some(s) => *s,
			None => continue
		};		
		let total = quest.steps.len();
		if step >= total {
			continue;
		}

		let kind = quest.steps[step].kind.clone();
		let player = world.players.get(name).unwrap();

		let satisfied = match (&trigger, &kind) {
			(Trigger::Reach, StepKind::Reach { room }) => player.room_id == *room,
			(Trigger::Collect, StepKind::Collect { item, count }) => {
				player.inventory.iter().filter(|i| *i == item).count() as u32 >= *count
			}
			(Trigger::Defeat(killed), StepKind::Defeat { npc }) => *killed == npc.as_str(),
			(Trigger::Talk(talked), StepKind::Talk { npc }) => *talked == npc.as_str(),
			_ => false
		};
		if !satisfied {
			continue;
		}

		let new_step = step + 1;
		let reward = quest.reward.clone();
		let player = world.players.get_mut(name).unwrap();

		if new_step >= total {
			player.active_quests.remove(&quest_id);
			player.completed_quests.insert(quest_id.clone());
			player.inventory.push(reward);
			events.push((vec![name.to_string()], format!("EVT QUEST COMPLETED {}\n", quest_id)));
		} else {
			player.active_quests.insert(quest_id.clone(), new_step);
			events.push((vec![name.to_string()], format!("EVT QUEST PROGRESSED {} {}/{}\n", quest_id, new_step, total)));
		}
	}

	events
}


fn handle_command(
    line: &str,
    player_name: &mut Option<String>,
    world: &SharedWorld,
) -> (String, Vec<Event>) {
    let trimmed = line.trim();
    let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();

    match parts.as_slice() {
        ["CONNECT", name] => {
            let mut w = world.lock().unwrap();
            let player = Player {
                name: name.to_string(),
                room_id: "square".to_string(),
				group_id: None,
				invites: Vec::new(),
                hp: 100,
                max_hp: 100,
                inventory: Vec::new(),
                active_quests: HashMap::new(),
                completed_quests: HashSet::new(),
                statuses: Vec::new(),
            };
			let player_clone: Player = player.clone();
            w.players.insert(name.to_string(), player);
            *player_name = Some(name.to_string());
            println!("Player {} connected", name);

            let response = "OK connected\n".to_string();
            let events: Vec<Event> = vec![(
				players_in_scope(&player_clone, &w, ViewScope::Room),
				format!("EVT ROOM PRESENCE ENTER {}\n", name)
			)];
            (response, events)
        }

        ["LOOK"] => {
            if let Some(name) = player_name {
                let w = world.lock().unwrap();
                if let Some(player) = w.players.get(name) {
                    if let Some(room) = w.rooms.get(&player.room_id) {
                        let players_here: Vec<&String> = w.players.iter()
                            .filter(|(_, p)| p.room_id == player.room_id)
                            .map(|(n, _)| n)
                            .collect();

                        let quest_givers: Vec<&String> = room.npcs.iter()
                            .filter(|npc_id| w.quests.iter().any(|(quest_id, quest)|
                                quest.giver.as_str() == npc_id.as_str()
                                && !player.completed_quests.contains(quest_id)
                                && !player.active_quests.contains_key(quest_id)))
                            .collect();

                        let res = format!(
                            "OK {{\"room\": \"{}\", \"desc\": \"{}\", \"exits\": {:?}, \"players\": {:?}, \"items\": {:?}, \"npcs\": {:?}, \"available_quests\": {:?}, \"your_hp\": {}}}\n",
                            room.name,
                            room.description,
                            room.exits.keys().collect::<Vec<_>>(),
                            players_here,
                            room.items.iter().collect::<Vec<_>>(),
                            room.npcs.iter().collect::<Vec<_>>(),
                            quest_givers,
                            player.hp
                        );
                        (res, Vec::new())
                    } else {
                        ("ERR room_not_found\n".to_string(), Vec::new())
                    }
                } else {
                    ("ERR not_connected\n".to_string(), Vec::new())
                }
            } else {
                ("ERR not_connected\n".to_string(), Vec::new())
            }
        }

        ["MOVE", direction] => {
            if let Some(name) = player_name {
                let mut w = world.lock().unwrap();

                let current_room_id = match w.players.get(name) {
                    Some(p) => p.room_id.clone(),
                    None => return ("ERR not_connected\n".to_string(), Vec::new()),
                };

                let next_room_id = w.rooms.get(&current_room_id)
                    .and_then(|r| r.exits.get(*direction))
                    .cloned();

                if let Some(next) = next_room_id {
                    let leavers: Vec<String> = {
                        let me = w.players.get(name).unwrap();
                        players_in_scope(me, &w, ViewScope::Room)
                            .into_iter().filter(|n| n.as_str() != name.as_str()).collect()
                    };

                    if let Some(p) = w.players.get_mut(name) {
                        p.room_id = next.clone();
                    }

                    let enterers: Vec<String> = {
                        let me = w.players.get(name).unwrap();
                        players_in_scope(me, &w, ViewScope::Room)
                            .into_iter().filter(|n| n.as_str() != name.as_str()).collect()
                    };

                    let res = format!("OK room={}\n", next);
                    let mut events: Vec<Event> = vec![
                        (leavers, format!("EVT ROOM PRESENCE LEAVE {}\n", name)),
                        (enterers, format!("EVT ROOM PRESENCE ENTER {}\n", name)),
                    ];
                    events.extend(advance_quests(&mut w, name, Trigger::Reach));
                    (res, events)
                } else {
                    ("ERR no_exit\n".to_string(), Vec::new())
                }
            } else {
                ("ERR not_connected\n".to_string(), Vec::new())
            }
        }

        ["TAKE ", ..] | _ if trimmed.starts_with("TAKE ") => {
            if let Some(name) = player_name {
                let item_query = trimmed["TAKE ".len()..].trim();
                let mut w = world.lock().unwrap();
                let room_id = w.players.get(name).unwrap().room_id.clone();

                let target_item_id = w.rooms.get(&room_id).unwrap().items.iter().find(|id| {
                    if **id == item_query { return true; }
                    if let Some(it) = w.items.get(*id) {
                        if it.name.eq_ignore_ascii_case(item_query) { return true; }
                    }
                    false
                }).cloned();

                if let Some(id) = target_item_id {
                    let obtainable = w.items.get(&id).unwrap().obtainable;
                    if !obtainable {
                        return ("ERR item_not_obtainable\n".to_string(), Vec::new());
                    }

                    w.rooms.get_mut(&room_id).unwrap().items.remove(&id);
                    w.players.get_mut(name).unwrap().inventory.push(id.clone());

                    let events = advance_quests(&mut w, name, Trigger::Collect);
                    (format!("OK taken={}\n", id), events)
                } else {
                    ("ERR item_not_found\n".to_string(), Vec::new())
                }
            } else {
                ("ERR not_connected\n".to_string(), Vec::new())
            }
        }

        ["DROP ", ..] | _ if trimmed.starts_with("DROP ") => {
            if let Some(name) = player_name {
                let item_query = trimmed["DROP ".len()..].trim();
                let mut w = world.lock().unwrap();

                let item_index = w.players.get(name).unwrap().inventory.iter().position(|id| {
                    if *id == item_query { return true; }
                    if let Some(it) = w.items.get(id) {
                        if it.name.eq_ignore_ascii_case(item_query) { return true; }
                    }
                    false
                });

                if let Some(idx) = item_index {
                    let player = w.players.get_mut(name).unwrap();
                    let id = player.inventory.remove(idx);
                    let room_id = player.room_id.clone();

                    w.rooms.get_mut(&room_id).unwrap().items.insert(id.clone());

                    (format!("OK dropped={}\n", id), Vec::new())
                } else {
                    ("ERR item_not_in_inventory\n".to_string(), Vec::new())
                }
            } else {
                ("ERR not_connected\n".to_string(), Vec::new())
            }
        }

        ["INVENTORY"] => {
            if let Some(name) = player_name {
                let w = world.lock().unwrap();
                let player = w.players.get(name).unwrap();
                (format!("OK {:?}\n", player.inventory), Vec::new())
            } else {
                ("ERR not_connected\n".to_string(), Vec::new())
            }
        }

        ["TALK ", ..] | _ if trimmed.starts_with("TALK ") => {
            if let Some(name) = player_name {
                let npc_query = trimmed["TALK ".len()..].trim();
                let mut w = world.lock().unwrap();

                let room_id = w.players.get(name).unwrap().room_id.clone();
                let room = w.rooms.get(&room_id).unwrap();

                let target_npc_id = room.npcs.iter().find(|id| {
                    if **id == npc_query { return true; }
                    if let Some(n) = w.npcs.get(*id) {
                        if n.name.eq_ignore_ascii_case(npc_query) { return true; }
                    }
                    false
                }).cloned();

                if let Some(id) = target_npc_id {
                    let npc_data = w.npcs.get(&id).unwrap();
                    let response_text = if !npc_data.dialogue.is_empty() {
                        &npc_data.dialogue[0]
                    } else {
                        "..."
                    };
                    let response = format!("OK npc=\"{}\" talk=\"{}\"\n", npc_data.name, response_text);

                    let events = advance_quests(&mut w, name, Trigger::Talk(&id));
                    (response, events)
                } else {
                    ("ERR npc_not_found\n".to_string(), Vec::new())
                }
            } else {
                ("ERR not_connected\n".to_string(), Vec::new())
            }
        }

        _ if trimmed.starts_with("ATTACK ") => {
            if let Some(name) = player_name {
                let npc_query = trimmed["ATTACK ".len()..].trim();
                let mut w = world.lock().unwrap();

                let room_id = w.players.get(name).unwrap().room_id.clone();

                let target_npc_id = w.rooms.get(&room_id).unwrap().npcs.iter().find(|id| {
                    if **id == npc_query { return true; }
                    if let Some(n) = w.npcs.get(*id) {
                        if n.name.eq_ignore_ascii_case(npc_query) { return true; }
                    }
                    false
                }).cloned();

                if let Some(id) = target_npc_id {
                    let mut npc_hp = w.npcs.get(&id).unwrap().hp;
                    let npc_type = w.npcs.get(&id).unwrap().npc_type.clone();
                    let npc_name = w.npcs.get(&id).unwrap().name.clone();

                    npc_hp -= 25;
                    w.npcs.get_mut(&id).unwrap().hp = npc_hp;

                    let mut log_msg = format!("Tu attaques {} ! Ses HP descendent à {}.", npc_name, npc_hp);

                    if npc_hp <= 0 {
                        w.rooms.get_mut(&room_id).unwrap().npcs.remove(&id);
                        log_msg.push_str(" Le monstre est mort !");
                        let events = advance_quests(&mut w, name, Trigger::Defeat(&id));
                        return (format!("OK combat=\"{}\"\n", log_msg), events);
                    }

                    if npc_type == "enemy" {
                        let mut player_died = false;
                        let mut dropped_items = Vec::new();

                        if let Some(player) = w.players.get_mut(name) {
                            player.hp -= 15;
                            log_msg.push_str(&format!(" {} contre-attaque et t'inflige 15 dégâts !", npc_name));

                            if player.hp <= 0 {
                                player_died = true;
                                log_msg.push_str(" Tu es mort ! Tu réapparais sur la place du village et ton inventaire tombe au sol.");
                                dropped_items = std::mem::take(&mut player.inventory);

                                player.hp = 100;
                                player.room_id = "square".to_string();
                            }
                        }

                        if player_died {
                            if let Some(room) = w.rooms.get_mut(&room_id) {
                                for item_id in dropped_items {
                                    room.items.insert(item_id);
                                }
                            }
                        }
                    }

                    (format!("OK combat=\"{}\"\n", log_msg), Vec::new())
                } else {
                    ("ERR target_not_found\n".to_string(), Vec::new())
                }
            } else {
                ("ERR not_connected\n".to_string(), Vec::new())
            }
        }

		_ if trimmed.starts_with("QUEST ") => {
            if let Some(name) = player_name {
                let npc_id = trimmed["QUEST ".len()..].trim();
                let mut w = world.lock().unwrap();
				let player = w.players.get(name).unwrap();
				
				if  w.npcs.get(npc_id).is_none() {
					return ("ERR 404 NPC_NOT_FOUND\n".to_string(), Vec::new())
				}
				if !w.rooms.get(&player.room_id).unwrap().npcs.contains(npc_id) {
					return ("ERR 404 NPC_NOT_FOUND\n".to_string(), Vec::new())
				}

                let mut found_quest: Option<String> = None;
                for (quest_id, quest) in &w.quests {
                    if quest.giver == npc_id
						&& !player.completed_quests.contains(quest_id)
						&& !player.active_quests.contains_key(quest_id)
					{
                        found_quest = Some(quest_id.clone());
                        break;
                    }
                }
                let quest_id = match found_quest {
                    Some(id) => id,
                    None => return ("ERR 406 NO_QUEST_AVAILABLE\n".to_string(), Vec::new())
                };

                let quest = w.quests.get(&quest_id).unwrap();
                let response = format!(
                    "OK {{\"quest_id\": \"{}\", \"description\": \"{}\", \"reward\": \"{}\", \"status\": \"{}\"}}\n",
                    quest_id, quest.description, quest.reward, "received"
                );
				w.players.get_mut(name).unwrap().active_quests.insert(quest_id.clone(), 0);
                (response, Vec::new())
            } else {
                ("ERR not_connected\n".to_string(), Vec::new())
            }
        }

        ["QUESTS"] => {
            if let Some(name) = player_name {
                let w = world.lock().unwrap();
                let player = w.players.get(name).unwrap();

                let mut quests_strs: Vec<String> = Vec::new();
                for (quest_id, step) in &player.active_quests {
                    let quest = match w.quests.get(quest_id) {
                        Some(q) => q,
                        None => continue,
                    };
                    let total = quest.steps.len();
                    let task = match quest.steps.get(*step) {
						Some(qs) => qs.description.as_str(),
						None => ""
					};
                    quests_strs.push(format!(
                        "{{\"quest_id\": \"{}\", \"status\": \"active\", \"progress\": \"{}/{}\", \"task\": \"{}\"}}",
                        quest_id, step, total, task
                    ));
                }
                for quest_id in &player.completed_quests {
                    quests_strs.push(format!(
                        "{{\"quest_id\": \"{}\", \"status\": \"completed\"}}",
                        quest_id
                    ));
                }

                (format!("OK [{}]\n", quests_strs.join(", ")), Vec::new())
            } else {
                ("ERR not_connected\n".to_string(), Vec::new())
            }
        }

		["CHAT", _] => ("ERR EMPTY_MESSAGE\n".to_string(), Vec::new()),
        ["CHAT", channel, msg] => {
            if let Some(name) = player_name {
				let w = world.lock().unwrap();
        		let player = w.players.get(name).unwrap();
				let scope = match *channel {
					"GLOBAL" => ViewScope::Global,
					"ROOM"   => ViewScope::Room,
					"GROUP"  => ViewScope::Group,
					_ => return ("ERR BAD_SCOPE\n".to_string(), Vec::new())
				};

				if matches!(scope, ViewScope::Group) && player.group_id.is_none() {
					return ("ERR 401 NOT_IN_GROUP\n".to_string(), Vec::new());
				}

				let response = "OK\n".to_string();
				let event: Vec<Event> = vec![(
					players_in_scope(player, &w, scope),
					format!("EVT {} CHAT {} {}\n", channel, name, msg)
				)];
				(response, event)
            } else {
                ("ERR not_connected\n".to_string(), Vec::new())
            }
        }

        ["WHO"] => {
            let w = world.lock().unwrap();
            let names: Vec<&String> = w.players.keys().collect();
            let res = format!(
                "OK {{\"server\": {}, \"players\": {:?}}}\n",
                names.len(),
                names
            );
            (res, Vec::new())
        }

		["STATUS"] => {
			if let Some(name) = player_name {
				let w = world.lock().unwrap();
				let player = w.players.get(name).unwrap();

				let status = if player.statuses.is_empty() {
					"healthy".to_string()
				} else {
					let str_statuses: Vec<&str> = player.statuses.iter().map(|s| s.as_str()).collect();
					str_statuses.join(", ")
				};

				let res = format!(
					"OK {{\"hp\": {}, \"max_hp\": {}, \"status\": \"{}\"}}\n",
					player.hp,
					player.max_hp,
					status
				);
				(res, Vec::new())
			} else {
                ("ERR not_connected\n".to_string(), Vec::new())
            }
        }

		["GROUP", "CREATE"] => {
			if let Some(name) = player_name {
				let mut w = world.lock().unwrap();
				let player = w.players.get_mut(name).unwrap();

				if player.group_id.is_some() {
					return ("ERR 402 ALREADY_IN_GROUP\n".to_string(), Vec::new());
				}

				player.group_id = Some(name.clone());
				(format!("OK group={}\n", name), Vec::new())
            } else {
				("ERR not_connected\n".to_string(), Vec::new())
			}
		}

		["GROUP", "INVITE", target_name] => {
			if let Some(name) = player_name {
				let mut w = world.lock().unwrap();
				let player = w.players.get(name).unwrap();
				let group_name = match &player.group_id {
					Some(n) => n.clone(),
					None => return ("ERR 401 NOT_IN_GROUP\n".to_string(), Vec::new())
				};
				if *target_name == name.as_str() {
					return ("ERR CANT_INVITE_SELF\n".to_string(), Vec::new());
				}
				let target = match w.players.get_mut(*target_name) {
					Some(t) => t,
					None => return ("ERR PLAYER_NOT_FOUND\n".to_string(), Vec::new())
				};

				if target.group_id.is_some() {
					return ("ERR 402 ALREADY_IN_GROUP\n".to_string(), Vec::new());
				}
				if target.invites.contains(&group_name) {
					return ("ERR ALREADY_INVITED\n".to_string(), Vec::new())
				}
				target.invites.push(group_name.clone());

				let response = "OK\n".to_string();
				let events: Vec<Event> = vec![(
					vec![target_name.to_string()],
					format!("EVT GROUP INVITE {}\n", group_name)
				)];
				(response, events)
            } else {
				("ERR not_connected\n".to_string(), Vec::new())
			}
		}

		["GROUP", "JOIN", leader_name] => {
			if let Some(name) = player_name {
				let mut w = world.lock().unwrap();

				let player = w.players.get(name).unwrap();
				if player.group_id.is_some() {
					return ("ERR 402 ALREADY_IN_GROUP\n".to_string(), Vec::new());
				}
				if !player.invites.iter().any(|invite| invite.as_str() == *leader_name) {
					return ("ERR NOT_INVITED\n".to_string(), Vec::new());
				}

				let group_exists = w.players.values()
					.any(|p| p.group_id.as_deref() == Some(*leader_name));
				if !group_exists {
					return ("ERR GROUP_NOT_FOUND\n".to_string(), Vec::new());
				}

				let player = w.players.get_mut(name).unwrap();
				player.group_id = Some(leader_name.to_string());
				player.invites.retain(|invite| invite.as_str() != *leader_name);

				let player = w.players.get(name).unwrap();
				let recipients: Vec<String> = players_in_scope(player, &w, ViewScope::Group)
					.into_iter().filter(|n| n.as_str() != name.as_str()).collect();

				let response = format!("OK group={}\n", leader_name);
				let events: Vec<Event> = vec![(
					recipients,
					format!("EVT GROUP JOIN {}\n", name)
				)];
				(response, events)
            } else {
				("ERR not_connected\n".to_string(), Vec::new())
			}
		}

		["GROUP", "LEAVE"] => {
			if let Some(name) = player_name {
				let mut w = world.lock().unwrap();
				let in_group = w.players.get(name).map(|p| p.group_id.is_some()).unwrap_or(false);
				if !in_group {
					return ("ERR 401 NOT_IN_GROUP\n".to_string(), Vec::new());
				}
				("OK\n".to_string(), leave_group(&mut w, name))
			} else {
				("ERR not_connected\n".to_string(), Vec::new())
			}
		}

        ["QUIT"] => {
            ("OK bye\n".to_string(), Vec::new())
        }

        _ => ("ERR unknown_command\n".to_string(), Vec::new()),
    }
}


#[tokio::main]
async fn main() {
    let world_data = match World::from_file("world.yaml") {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Erreur critique au démarrage du serveur : {}", e);
            std::process::exit(1);
        }
    };

    let world: SharedWorld = Arc::new(Mutex::new(world_data));
	let mailboxes: Mailboxes = Arc::new(Mutex::new(HashMap::new()));

    let listener = TcpListener::bind("0.0.0.0:4242").await.unwrap();
    println!("Server listening on port 4242");

    loop {
        let (socket, addr) = listener.accept().await.unwrap();
        println!("New connection from {}", addr);

        let world_clone = Arc::clone(&world);
		let boxes_clone: Mailboxes = Arc::clone(&mailboxes);

        tokio::spawn(async move {
            handle_client(socket, addr, world_clone, boxes_clone).await;
        });
    }
}

async fn handle_client(
    socket: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    world: SharedWorld,
	mailboxes: Mailboxes
) {
    let (reader, mut writer) = socket.into_split();
    let mut reader = BufReader::new(reader);
	let (mailbox_tx, mut mailbox_rx) = mpsc::unbounded_channel::<String>();

    let mut line = String::new();
    let mut player_name: Option<String> = None;

    if let Err(_) = writer.write_all(b"OK hello proto=1\n").await {
        return;
    }

    loop {
        line.clear();

        tokio::select! {
            result = reader.read_line(&mut line) => {
                match result {
                    Ok(0) | Err(_) => {
                        handle_disconnect(&player_name, &world, &mailboxes).await;
                        break;
                    }
                    Ok(_) => {
                        if line.trim() == "QUIT" {
                            let _ = writer.write_all(b"OK bye\n").await;
                            handle_disconnect(&player_name, &world, &mailboxes).await;
                            break;
                        }

						let was_connected = player_name.is_some();
                        let (response, event) = handle_command(&line, &mut player_name, &world);
						if !was_connected {
							if let Some(name) = &player_name {
								mailboxes.lock().unwrap().insert(name.clone(), mailbox_tx.clone());
							}
						}

                        if let Err(e) = writer.write_all(response.as_bytes()).await {
                            println!("Write error for {}: {}", addr, e);
                            break;
                        }

                        for (recipients, msg) in event {
                            let boxes = mailboxes.lock().unwrap();
							for name in recipients {
								if let Some(box_tx) = boxes.get(&name) {
									let _ = box_tx.send(msg.clone());
								}
							}
                        }
                    }
                }
            }

            event = mailbox_rx.recv() => {
                match event {
                    Some(msg) => {
                        if let Err(_) = writer.write_all(msg.as_bytes()).await {
                            break;
                        }
                    }
					None => break
                }
            }
        }
    }

    println!("Connection closed: {}", addr);
}

async fn handle_disconnect(
    player_name: &Option<String>,
    world: &SharedWorld,
    mailboxes: &Mailboxes
) {
    if let Some(name) = player_name {
		let events: Vec<Event> = {
			let mut w = world.lock().unwrap();
			let mut events = Vec::new();
			if let Some(player) = w.players.get(name) {
				let room: Vec<String> = players_in_scope(player, &w, ViewScope::Room)
					.into_iter().filter(|n| n.as_str() != name.as_str()).collect();
				events.push((room, format!("EVT ROOM PRESENCE LEAVE {}\n", name)));
			}
			events.extend(leave_group(&mut w, name));
			w.players.remove(name);
			events
		};

		mailboxes.lock().unwrap().remove(name);

        let boxes = mailboxes.lock().unwrap();
        for (recipients, line) in events {
            for recipient in recipients {
                if let Some(box_tx) = boxes.get(&recipient) {
					let _ = box_tx.send(line.clone());
				}
            }
        }
        println!("Player {} disconnected", name);
    }
}

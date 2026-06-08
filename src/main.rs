use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use serde::Deserialize;

#[derive(Clone)]
struct Player {
    name: String,
    room_id: String,
    hp: i32,
    inventory: Vec<String>,
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

#[derive(Deserialize, Debug)]
struct WorldConfig {
    rooms: HashMap<String, Room>,
    #[serde(default)]
    items: HashMap<String, Item>,
    #[serde(default)]
    npcs: HashMap<String, Npc>,
}

struct World {
    players: HashMap<String, Player>,
    rooms: HashMap<String, Room>,
    items: HashMap<String, Item>,
    npcs: HashMap<String, Npc>,
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

        println!(
            "Monde validé ! {} pièces, {} objets et {} NPCs chargés.",
            config.rooms.len(), config.items.len(), config.npcs.len()
        );

        Ok(World {
            players: HashMap::new(),
            rooms: config.rooms,
            items: config.items,
            npcs: config.npcs,
        })
    }
}

type SharedWorld = Arc<Mutex<World>>;
type EventSender = broadcast::Sender<String>;


fn handle_command(
    line: &str,
    player_name: &mut Option<String>,
    world: &SharedWorld,
) -> (String, Option<String>) {
    let trimmed = line.trim();
    let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();

    match parts.as_slice() {
        ["CONNECT", name] => {
            let mut w = world.lock().unwrap();
            let player = Player {
                name: name.to_string(),
                room_id: "square".to_string(),
                hp: 100,
                inventory: Vec::new(),
            };
            w.players.insert(name.to_string(), player);
            *player_name = Some(name.to_string());
            println!("Player {} connected", name);

            let res = "OK connected\n".to_string();
            let evt = Some(format!("EVT ROOM PRESENCE ENTER {}\n", name));
            (res, evt)
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

                        let res = format!(
                            "OK {{\"room\": \"{}\", \"desc\": \"{}\", \"exits\": {:?}, \"players\": {:?}, \"items\": {:?}, \"npcs\": {:?}, \"your_hp\": {}}}\n",
                            room.name,
                            room.description,
                            room.exits.keys().collect::<Vec<_>>(),
                            players_here,
                            room.items.iter().collect::<Vec<_>>(),
                            room.npcs.iter().collect::<Vec<_>>(),
                            player.hp
                        );
                        (res, None)
                    } else {
                        ("ERR room_not_found\n".to_string(), None)
                    }
                } else {
                    ("ERR not_connected\n".to_string(), None)
                }
            } else {
                ("ERR not_connected\n".to_string(), None)
            }
        }

        ["MOVE", direction] => {
            if let Some(name) = player_name {
                let mut w = world.lock().unwrap();

                let current_room_id = match w.players.get(name) {
                    Some(p) => p.room_id.clone(),
                    None => return ("ERR not_connected\n".to_string(), None),
                };

                let next_room_id = w.rooms.get(&current_room_id)
                    .and_then(|r| r.exits.get(*direction))
                    .cloned();

                if let Some(next) = next_room_id {
                    if let Some(p) = w.players.get_mut(name) {
                        p.room_id = next.clone();
                    }

                    let res = format!("OK room={}\n", next);
                    let evt = Some(format!("EVT ROOM PRESENCE ENTER {}\n", name));
                    (res, evt)
                } else {
                    ("ERR no_exit\n".to_string(), None)
                }
            } else {
                ("ERR not_connected\n".to_string(), None)
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
                        return ("ERR item_not_obtainable\n".to_string(), None);
                    }

                    w.rooms.get_mut(&room_id).unwrap().items.remove(&id);
                    w.players.get_mut(name).unwrap().inventory.push(id.clone());

                    (format!("OK taken={}\n", id), None)
                } else {
                    ("ERR item_not_found\n".to_string(), None)
                }
            } else {
                ("ERR not_connected\n".to_string(), None)
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

                    (format!("OK dropped={}\n", id), None)
                } else {
                    ("ERR item_not_in_inventory\n".to_string(), None)
                }
            } else {
                ("ERR not_connected\n".to_string(), None)
            }
        }

        ["INVENTORY"] => {
            if let Some(name) = player_name {
                let w = world.lock().unwrap();
                let player = w.players.get(name).unwrap();
                (format!("OK {:?}\n", player.inventory), None)
            } else {
                ("ERR not_connected\n".to_string(), None)
            }
        }

        ["TALK ", ..] | _ if trimmed.starts_with("TALK ") => {
            if let Some(name) = player_name {
                let npc_query = trimmed["TALK ".len()..].trim();
                let w = world.lock().unwrap();

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

                    (format!("OK npc=\"{}\" talk=\"{}\"\n", npc_data.name, response_text), None)
                } else {
                    ("ERR npc_not_found\n".to_string(), None)
                }
            } else {
                ("ERR not_connected\n".to_string(), None)
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
                        return (format!("OK combat=\"{}\"\n", log_msg), None);
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

                    (format!("OK combat=\"{}\"\n", log_msg), None)
                } else {
                    ("ERR target_not_found\n".to_string(), None)
                }
            } else {
                ("ERR not_connected\n".to_string(), None)
            }
        }

        ["CHAT", channel, message] => {
            if let Some(name) = player_name {
                let res = "OK\n".to_string();
                let evt = Some(format!("EVT {} CHAT {} {}\n", channel, name, message));
                (res, evt)
            } else {
                ("ERR not_connected\n".to_string(), None)
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
            (res, None)
        }

        ["QUIT"] => {
            ("OK bye\n".to_string(), None)
        }

        _ => ("ERR unknown_command\n".to_string(), None),
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

    let (tx, _rx) = broadcast::channel::<String>(100);
    let tx = Arc::new(tx);

    let listener = TcpListener::bind("0.0.0.0:4242").await.unwrap();
    println!("Server listening on port 4242");

    loop {
        let (socket, addr) = listener.accept().await.unwrap();
        println!("New connection from {}", addr);

        let world_clone = Arc::clone(&world);
        let tx_clone = Arc::clone(&tx);

        tokio::spawn(async move {
            handle_client(socket, addr, world_clone, tx_clone).await;
        });
    }
}

async fn handle_client(
    socket: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    world: SharedWorld,
    tx: Arc<EventSender>,
) {
    let (reader, mut writer) = socket.into_split();
    let mut reader = BufReader::new(reader);
    let mut rx = tx.subscribe();

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
                        handle_disconnect(&player_name, &world, &tx).await;
                        break;
                    }
                    Ok(_) => {
                        if line.trim() == "QUIT" {
                            let _ = writer.write_all(b"OK bye\n").await;
                            handle_disconnect(&player_name, &world, &tx).await;
                            break;
                        }

                        let (response, event) = handle_command(&line, &mut player_name, &world);

                        if let Err(e) = writer.write_all(response.as_bytes()).await {
                            println!("Write error for {}: {}", addr, e);
                            break;
                        }

                        if let Some(evt_msg) = event {
                            let _ = tx.send(evt_msg);
                        }
                    }
                }
            }

            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        if let Err(_) = writer.write_all(event.as_bytes()).await {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                }
            }
        }
    }

    println!("Connection closed: {}", addr);
}

async fn handle_disconnect(
    player_name: &Option<String>,
    world: &SharedWorld,
    tx: &Arc<EventSender>,
) {
    if let Some(name) = player_name {
        world.lock().unwrap().players.remove(name);

        let event = format!("EVT ROOM PRESENCE LEAVE {}\n", name);
        let _ = tx.send(event);

        println!("Player {} disconnected", name);
    }
}

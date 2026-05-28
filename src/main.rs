use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::broadcast;



struct World {
    players: HashMap<String, String>,
    rooms: HashMap<String, Room>,
}

struct Room {
    name: String,
    description: String,
    exits: HashMap<String, String>,
}

impl World {
    fn new() -> Self {
        let mut rooms = HashMap::new();

        rooms.insert(
            "square".to_string(),
            Room {
                name: "Village Square".to_string(),
                description: "La grand place comme a bx".to_string(),
                exits: HashMap::from([
                    ("north".to_string(), "tavern".to_string()),
                    ("east".to_string(), "shop".to_string()),
                ]),
            },
        );

        rooms.insert(
            "tavern".to_string(),
            Room {
                name: "The Tavern".to_string(),
                description: "Une taverne".to_string(),
                exits: HashMap::from([("south".to_string(), "square".to_string())]),
            },
        );

        rooms.insert(
            "shop".to_string(),
            Room {
                name: "General Store".to_string(),
                description: "Un delhaize".to_string(),
                exits: HashMap::from([("west".to_string(), "square".to_string())]),
            },
        );

        World {
            players: HashMap::new(),
            rooms,
        }
    }

    fn player_room<'a>(&'a self, name: &str) -> Option<&'a Room> {
        let room_id = self.players.get(name)?;
        self.rooms.get(room_id)
    }

    fn player_room_id(&self, name: &str) -> Option<String> {
        self.players.get(name).cloned()
    }
}


type SharedWorld = Arc<Mutex<World>>;

type EventSender = broadcast::Sender<String>;


#[tokio::main]
async fn main() {
    let world: SharedWorld = Arc::new(Mutex::new(World::new()));

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

    writer.write_all(b"OK hello proto=1\n").await.unwrap();

    loop {
        line.clear();

        tokio::select! {

            result = reader.read_line(&mut line) => {
                match result {
                    Ok(0) => {
                        handle_disconnect(&player_name, &world, &tx).await;
                        break;
                    }
                    Err(_) => {
                        handle_disconnect(&player_name, &world, &tx).await;
                        break;
                    }
                    Ok(_) => {
                        let parts: Vec<&str> = line.trim().splitn(3, ' ').collect();

                        let response = match parts.as_slice() {

                            ["CONNECT", name] => {
                                let mut w = world.lock().unwrap();
                                w.players.insert(
                                    name.to_string(),
                                    "square".to_string(),
                                );
                                player_name = Some(name.to_string());
                                println!("Player {} connected", name);

                                let event = format!(
                                    "EVT ROOM PRESENCE ENTER {}\n",
                                    name
                                );
                                tx.send(event).ok();

                                "OK connected\n".to_string()
                            }

                            ["LOOK"] => {
                                if let Some(ref name) = player_name {
                                    let w = world.lock().unwrap();
                                    if let Some(room) = w.player_room(name) {
                                        let room_id = w.player_room_id(name)
                                            .unwrap_or_default();
                                        let players_here: Vec<&String> = w
                                            .players
                                            .iter()
                                            .filter(|(_, rid)| **rid == room_id)
                                            .map(|(n, _)| n)
                                            .collect();

                                        format!(
                                            "OK {{\"room\": \"{}\", \
                                            \"desc\": \"{}\", \
                                            \"exits\": {:?}, \
                                            \"players\": {:?}}}\n",
                                            room.name,
                                            room.description,
                                            room.exits.keys()
                                                .collect::<Vec<_>>(),
                                            players_here,
                                        )
                                    } else {
                                        "ERR room_not_found\n".to_string()
                                    }
                                } else {
                                    "ERR not_connected\n".to_string()
                                }
                            }

                            ["MOVE", direction] => {
                                if let Some(ref name) = player_name {
                                    let mut w = world.lock().unwrap();
                                    let room_id = w
                                        .player_room_id(name)
                                        .unwrap_or_default();

                                    let next_room_id = w
                                        .rooms
                                        .get(&room_id)
                                        .and_then(|r| r.exits.get(*direction))
                                        .cloned();

                                    if let Some(next) = next_room_id {
                                        let leave_event = format!(
                                            "EVT ROOM PRESENCE LEAVE {}\n",
                                            name
                                        );
                                        tx.send(leave_event).ok();

                                        w.players.insert(
                                            name.clone(),
                                            next.clone(),
                                        );

                                        let enter_event = format!(
                                            "EVT ROOM PRESENCE ENTER {}\n",
                                            name
                                        );
                                        tx.send(enter_event).ok();

                                        format!("OK room={}\n", next)
                                    } else {
                                        "ERR no_exit\n".to_string()
                                    }
                                } else {
                                    "ERR not_connected\n".to_string()
                                }
                            }

                            ["CHAT", channel, message] => {
                                if let Some(ref name) = player_name {
                                    let event = format!(
                                        "EVT {} CHAT {} {}\n",
                                        channel, name, message
                                    );
                                    tx.send(event).ok();
                                    "OK\n".to_string()
                                } else {
                                    "ERR not_connected\n".to_string()
                                }
                            }
                            ["WHO"] => {
                                let w = world.lock().unwrap();
                                let names: Vec<&String> =
                                    w.players.keys().collect();
                                format!(
                                    "OK {{\"server\": {}, \"players\": {:?}}}\n",
                                    names.len(),
                                    names
                                )
                            }

                            ["QUIT"] => {
                                writer.write_all(b"OK bye\n").await.unwrap();
                                handle_disconnect(&player_name, &world, &tx)
                                    .await;
                                break;
                            }

                            _ => "ERR unknown_command\n".to_string(),
                        };

                        if let Err(e) = writer
                            .write_all(response.as_bytes())
                            .await
                        {
                            println!("Write error for {}: {}", addr, e);
                            break;
                        }
                    }
                }
            }

            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        if let Err(_) = writer
                            .write_all(event.as_bytes())
                            .await
                        {
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
        tx.send(event).ok();

        println!("Player {} disconnected", name);
    }
}

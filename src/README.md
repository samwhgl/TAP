*This project has been created as part of the 42 curriculum by kprist, baderwae, shaegels.*

# TAP — The Answer Protocol

A shared-world, retro, multiplayer text adventure. A single asynchronous TCP
server hosts a persistent-feeling world (rooms, items, NPCs, quests, combat)
that several players can explore at the same time, through either a
command-line client or a graphical client.

## Description

TAP recreates the spirit of early 90's MUDs (Multi-User Dungeons): players
connect to a shared world over a simple line-based TCP protocol, move
between rooms, chat, fight NPCs, collect items and complete quests together
in real time.

This implementation is written in **Rust** and delivers:

- **`tap_server`**: the authoritative TCP server. It loads and validates
  the world from a YAML file, keeps all game state (players, rooms, items,
  NPCs, quests, combat), and speaks the RFC 42TAP protocol over line-based
  TCP (UTF-8, `\n`-terminated messages).
- **`tap_cli`**: a minimal command-line client. It forwards raw protocol
  commands typed by the user straight to the server and prints every
  incoming line (replies and asynchronous events) as it arrives.
- **A GUI client** (`gui_layout.rs`, built with [`egui`](https://www.egui.rs/)
  / [`eframe`](https://crates.io/crates/eframe)) providing room details,
  separate Global/Room/Group chat panes, inventory and action buttons.
- **`world.yaml`**: the static world data (rooms, items, NPCs, quests).


## Instructions

Requirements: Rust (2024 edition) and Cargo, installed via
[rustup](https://rustup.rs/).

```bash
# fetch dependencies & build everything
cargo build --release

# run the server (from the repository root, where world.yaml lives)
cargo run --release --bin tap_server

# in another terminal, run the CLI client
cargo run --release --bin tap_cli
```

See [Building and Running](#building-and-running) below for the full set of
commands (lint, clean, GUI, etc.).


## Architecture

- **Runtime:** the server (`tap_server`) is built on **Tokio**. It binds a
  `TcpListener` on `0.0.0.0:4242` and spawns one asynchronous task per
  connected client (`tokio::spawn`).
- **Framing:** each connection is read line-by-line with
  `tokio_util::codec::LinesCodec` (max line length 1024 bytes), matching the
  protocol's `\n`-terminated, UTF-8 requirement.
- **Shared state:** the whole world (players, rooms, items, NPCs, quests) is
  stored in a single `World` struct behind `Arc<Mutex<World>>`, so every
  client task can read and mutate it safely. Given the small scale of the
  game, a global mutex was chosen over per-room locking for simplicity.
- **Command handling:** commands are handled **inline** (a single large
  `match` over the split command line in `handle_command`), rather than
  through a dispatcher/router abstraction — this keeps the mapping between
  RFC commands and behaviour easy to follow in one place.
- **Push events:** each connection owns an `mpsc::unbounded_channel`
  ("mailbox"). A shared `Mailboxes` map (`Arc<Mutex<HashMap<player,
  Sender>>>`) lets any task push asynchronous `EVT ...` lines to any other
  player (room presence, chat, combat, quests, groups). Inside
  `handle_client`, `tokio::select!` concurrently awaits the next line from
  the socket **and** the next mailbox message, so a client keeps receiving
  events while it is otherwise idle or typing.
- **Connection limiting:** a `tokio::sync::Semaphore` bounds the number of
  simultaneous connections; once exhausted, new sockets receive
  `ERR 503 SERVER_FULL` and are closed.
- **Rate limiting:** each connection maintains a small token bucket
  (10 tokens, refilled at 5 tokens/second); commands sent faster than that
  are rejected with `ERR RATE_LIMITED` instead of being processed, to guard
  against command flooding.
- **Clients:** `tap_cli` is a thin pass-through client built on
  `tokio::select!` over stdin and the socket, so it can print incoming
  server events while waiting for user input. The GUI client
  (`gui_layout.rs`) keeps a similar split: a background task owns the
  socket and communicates with the `egui` UI thread through `mpsc`
  channels.

## Protocol Implementation

The server implements the full RFC 42TAP command set: `CONNECT`, `LOOK`,
`MOVE`, `CHAT`, `TAKE`, `DROP`, `INVENTORY`, `TALK`, `ATTACK`, `DEFEND`,
`FLEE`, `STATUS`, `QUEST`, `QUESTS`, `WHO`, `GROUP` (`CREATE`/`INVITE`/
`JOIN`/`LEAVE`), and `QUIT`, plus the corresponding `EVT` push events
(`ROOM PRESENCE`, `ROOM COMBAT`, `GLOBAL/ROOM/GROUP CHAT`, `QUEST
PROGRESSED`/`COMPLETED`, `GROUP INVITE`/`JOIN`/`LEAVE`).

Documented deviations / extensions:

- **Reply payloads:** most `OK` replies carry a JSON body (room state,
  inventory, quest list, combat result, etc.) so clients can parse
  structured data instead of ad-hoc text.
- **Error codes:** some errors carry a numeric code as specified by the RFC
  (`ERR 201 NAME_IN_USE`, `ERR 301 NO_EXIT`, `ERR 401 NOT_IN_GROUP`,
  `ERR 402 ALREADY_IN_GROUP`, `ERR 404 NPC_NOT_FOUND`,
  `ERR 405 NPC_NOT_HOSTILE`, `ERR 406 NO_QUEST_AVAILABLE`,
  `ERR 503 SERVER_FULL`); other, more generic errors are returned as plain
  string codes only (e.g. `ERR not_connected`, `ERR item_not_found`,
  `ERR item_not_in_inventory`, `ERR item_not_obtainable`,
  `ERR npc_not_found`, `ERR room_not_found`, `ERR unknown_command`,
  `ERR BAD_SCOPE`, `ERR EMPTY_MESSAGE`, `ERR CONTROL_CHARS`,
  `ERR LINE_TOO_LONG`, `ERR RATE_LIMITED`).
- **Rate limiting** (`ERR RATE_LIMITED`) and the **connection cap**
  (`ERR 503 SERVER_FULL`) are additions on top of the base RFC to keep the
  server responsive under abuse.
- Input lines are sanitized (control characters are filtered / rejected via
  `ERR CONTROL_CHARS`) before being parsed as commands.

## Combat System

- Players start with **100 HP** (`max_hp`) and a base attack **power of
  10**. Enemy NPCs have their own HP defined in `world.yaml` and a fixed
  counter-attack power (`NPC_POWER = 15`).
- `ATTACK <npc>` engages combat: the target must be an NPC of type
  `"enemy"` present in the player's room (`ERR 405 NPC_NOT_HOSTILE`
  otherwise), and the player's `combat_target` is set. `ATTACK` (no
  argument) continues the fight already in progress.
- **Turn resolution** (`play_turn`) is symmetric each round:
  - `ATTACK`: the player deals damage equal to their `power` to the NPC.
    If the NPC's HP drops to 0 or below, it is removed from the room, the
    fight ends in `"victory"`, and any `Defeat` quest step is advanced.
    Otherwise the NPC counter-attacks for full `NPC_POWER`.
  - `DEFEND`: the player deals no damage but only takes **half** of
    `NPC_POWER` from the counter-attack.
  - `FLEE`: combat ends immediately (`combat_target` cleared, status
    `"fled"`), but the NPC still gets one free hit for full `NPC_POWER` as
    the player disengages.
- **Status effects:** fighting the `goblin` NPC has a 20% chance per turn
  of inflicting `Poison` on the player, dealing an extra 5 damage per
  subsequent turn until it is cleared (on death/respawn).
- **Death & respawn:** when a player's HP reaches 0, they are teleported
  back to the `square` room, respawn with **50 HP**, and lose all combat
  statuses and their current combat target.
- `STATUS` reports the player's current `hp`, `max_hp`, and status list
  (e.g. `"poisoned"` or `"healthy"`).
- Every combat action is broadcast to the room as
  `EVT ROOM COMBAT <player> <attack|defend|flee|defeated by ...> <npc>`.

## Quest System

- Each quest (`world.yaml`) has a `giver` NPC, a description, a `reward`
  item, and an ordered list of `steps`. A step's `kind` is one of `reach`
  (enter a room), `collect` (hold N of an item), `talk` (talk to an NPC) or
  `defeat` (kill an NPC).
- `QUEST <npc>` asks the given NPC for the next quest they offer that the
  player hasn't already completed or accepted; on success the quest is
  added to the player's `active_quests` at step `0` and the quest
  description/reward are returned.
- **Progression validation:** after every relevant action (`MOVE`, `TAKE`,
  `TALK`, defeating an NPC), the server calls `advance_quests`, which
  checks whether the *current* step of each active quest is satisfied by
  that action. If it is, the quest either advances to the next step
  (`EVT QUEST PROGRESSED <id> <step>/<total>`) or, if it was the last step,
  completes (`EVT QUEST COMPLETED <id>`).
- **Rewards:** on completion, the quest's reward item is added directly to
  the player's inventory and the quest moves from `active_quests` to
  `completed_quests` (completed quests cannot be taken again).
- `QUESTS` returns the player's active quests (with progress and the
  current task description) and completed quests as JSON.

## World Design

The shipped `world.yaml` describes a small connected map centered on a
village square, respecting the mandatory size requirements:

- **8 rooms**, forming two loops plus one optional dead-end branch:
  - Loop 1: `square → tavern → castle_gates → shop → square`.
  - Loop 2: `square → forest_path → goblin_camp → secret_shrine → square`.
  - Branch: `forest_path → abandoned_mine` (dead end).
- **5 NPCs** covering 3 distinct roles: two friendly dialogue / quest-givers
  (`old_man`, `innkeeper`) and three hostile enemies (`goblin`,
  `goblin_scout`, `mining_drone`).
- **8 items**, 6 of which are obtainable in the world (`old_sword`,
  `frothy_ale`, `healing_herb`, `rusty_shield`, `iron_ore`,
  `ancient_relic`) and 2 that only exist as quest rewards
  (`gold_coin`, `hero_medal`, `obtainable: false`).
- **2 quests**: `herb_hunt` (fetch `healing_herb` from the shop and bring
  it back to `old_man`, reward `gold_coin`) and `goblin_slayer` (defeat the
  `goblin` in the shop for the `innkeeper`, reward `hero_medal`).

Items are dynamic, unique world instances: `TAKE` removes them from the
room's item set and pushes them into the player's inventory; `DROP` puts
them back into the current room, available to other players. Both
commands accept an item's internal ID or its display `name`
(case-insensitive), including multi-word names.

## Server Logging

Logging is centralized in `utils.rs` around `log_msg`, which prints a
single structured JSON line per event to `stdout` (level `INFO`) or
`stderr` (levels `WARN`/`ERROR`), each including a `time` timestamp
(`%Y-%m-%d %H:%M:%S`), a `level`, a `type`, and a `content` payload:

- `log_connection` — client connect / disconnect / lost-connection /
  refused-for-server-full events, with the peer's socket address.
- `log_cmd` — every command received, tagged with the player name (or
  `"Undefined"` before `CONNECT`) and the raw command text.
- `log_response` — every response sent back to a client; responses
  starting with `ERR` are logged at `ERROR`, others at `INFO`.
- `log_items` — item `TAKE`/`DROP` events with item, room and player.
- `log_npc_death` / `log_player_death` — combat outcomes.
- `log_quest` — quest lifecycle events (new/progress/finished).

The **rate limiter** (token bucket) and **connection semaphore** described
in [Architecture](#architecture) constitute the abuse-monitoring mechanism:
refused connections are logged as `WARN` `REFUSED` events, and clients
exceeding their command budget receive `ERR RATE_LIMITED` (visible in the
response log) instead of being processed.

## Group Contributions

| Member | Responsibilities |
|---|---|
| `shaegels` | server implementation (protocol, world loading, combat, quests), CLI client |
| `kprist` |  GUI client (egui/eframe), world design (world.yaml) |
| `baderwae` |  logging, protocol documentation, testing |


## Building and Running

Build tool: **Cargo** (Rust 2024 edition), as declared in `Cargo.toml`.

| Task | Command |
|---|---|
| Build (debug) | `cargo build` |
| Build (release) | `cargo build --release` |
| Run the server | `cargo run --bin tap_server` *(run from the directory containing `world.yaml`)* |
| Run the CLI client | `cargo run --bin tap_cli` |
| Run the GUI client | `cargo run --bin tap_gui`|
| Clean | `cargo clean` |

The server listens on `0.0.0.0:4242`; both clients connect to
`127.0.0.1:4242` by default.

## Testing

Manual multiplayer testing is the primary strategy, since no automated
test suite is included yet:

1. Start the server: `cargo run --bin tap_server`.
2. Open two or more terminals and run `cargo run --bin tap_cli` (and/or the
   GUI client) in each to simulate several concurrent players.
3. **Protocol / world:** use `CONNECT <name>`, `LOOK`, `MOVE <dir>` to walk
   the full room loop and confirm every exit resolves and presence events
   (`EVT ROOM PRESENCE ENTER/LEAVE`) are broadcast to the right players.
4. **Items:** `TAKE`/`DROP` an item from two different clients to confirm
   it disappears from the room for everyone once taken, and reappears once
   dropped (no duplication).
5. **Combat:** `ATTACK <enemy npc>` repeatedly (optionally `DEFEND`/`FLEE`)
   until the NPC or the player dies; check `STATUS`, HP changes, the
   poison status against `goblin`, and the respawn-at-`square`-with-50-HP
   behaviour.
6. **Quests:** `QUEST <npc>` to accept `herb_hunt` / `goblin_slayer`,
   perform the required steps, and check `QUESTS` progress plus the
   `EVT QUEST PROGRESSED`/`COMPLETED` events and reward delivery.
7. **Groups / chat:** test `GROUP CREATE`/`INVITE`/`JOIN`/`LEAVE` and
   `CHAT GLOBAL|ROOM|GROUP` across several clients to confirm messages only
   reach the intended scope.
8. **Robustness:** disconnect a client mid-session (kill the terminal) and
   confirm the server logs the lost connection and broadcasts a
   `PRESENCE LEAVE`; open more than the connection limit at once to see
   `ERR 503 SERVER_FULL`; send commands rapidly to trigger
   `ERR RATE_LIMITED`.
   
## Resources

- [RFC 42TAP — The Answer Protocol](.) (attached subject document): the
  authoritative protocol specification this project implements.
- [Tokio documentation](https://tokio.rs/) — asynchronous runtime, TCP
  networking, channels, `tokio-util` `LinesCodec` framing.
- [serde / serde_yaml documentation](https://serde.rs/) — world data
  (de)serialization.
- [egui / eframe documentation](https://www.egui.rs/) — immediate-mode GUI
  toolkit used for the graphical client.
- General references on MUDs and line-based TCP protocols (Telnet, classic
  MUD RFCs) for background on the genre.
- **AI usage:** AI was used to draft the initial Tokio
  event loop skeleton in `main.rs`, which was then reviewed, corrected and
  extended by the team
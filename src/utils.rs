use chrono::Local;

pub enum LogLvl {
    INFO,
    WARN,
    ERROR,
}
pub enum LogType {
    CONNECTION,
    CMD,
    RESPONSE,
    QUEST,
    ITEM,
}

pub enum ConnectEvent {
    CONNECTION,
    DISCONNECTION,
    LOSTCONNECT,
    REFUSED,
}
pub enum QuestEvent {
    NEW, 
    PROGRESS,
    FINISHED,
}

pub enum ItemEvent {
    TAKE,
    DROP,
}

pub fn get_timestamp()-> String
{

    let current_time = Local::now();

    return current_time.format("%Y-%m-%d %H:%M:%S").to_string();
}

pub fn log_msg(msg: String, log_lvl: LogLvl, ltype: LogType)
{
    let log_type = match ltype {
        LogType::CONNECTION => "CONNECTION",
        LogType::CMD => "COMMAND",
        LogType::RESPONSE => "RESPONSE",
        LogType::QUEST => "QUEST",
        LogType::ITEM => "ITEM",
    };
    match log_lvl {
        LogLvl::INFO => 
            println!(
                "{{\"time\": \"{}\", \"level\": \"INFO\", \"type\": \"{}\", \"content\": {}}}",
                get_timestamp(), log_type, msg),
        LogLvl::WARN =>
            eprintln!(
                "{{\"time\": \"{}\", \"level\": \"WARN\", \"type\": \"{}\", \"content\": {}}}",
                get_timestamp(), log_type, msg),
        LogLvl::ERROR => 
            eprintln!(
                "{{\"time\": \"{}\", \"level\": \"ERROR\", \"type\": \"{}\", \"content\": {}}}",
                get_timestamp(), log_type, msg),
    }
}

pub fn log_cmd(user: String, cmd:String, lvl: LogLvl)
{
    let content = format!("{{\"user\": \"{}\", \"cmd\": \"{}\"}}", user, cmd);

    log_msg(content, lvl, LogType::CMD);
}

pub fn log_connection(ip: core::net::SocketAddr, event: ConnectEvent, lvl: LogLvl)
{
    let c_event = match event {
        ConnectEvent::CONNECTION => "New Connection",
        ConnectEvent::DISCONNECTION => "Disconnection",
        ConnectEvent::LOSTCONNECT => "Lost Connection",
        ConnectEvent::REFUSED => "Refused Connection (server full)",
    };
    let content = format!("{{\"event\": \"{}\", \"ip\": \"{}\"}}", c_event, ip);

    log_msg(content, lvl, LogType::CONNECTION);
}

pub fn log_response(msg: &String, user: String)
{
    let lvl = if msg.starts_with("ERR") {LogLvl::ERROR } else {LogLvl::INFO};

    let content = format!("{{\"response\": \"{}\", \"user\": \"{}\"}}", msg, user);
    log_msg(content, lvl, LogType::RESPONSE);
}

pub fn log_quest(event: QuestEvent, quest_id: String, name: String)
{
    let c_event = match event {
        QuestEvent::NEW => "New Quest",
        QuestEvent::PROGRESS => "Quest Progress",
        QuestEvent::FINISHED => "Finished",
    };
    let lvl = LogLvl::INFO;

    let content = format!("{{\"event\": {}, \"user\": {}, \"quest_id\": \"{}\"}}", c_event, name, quest_id);
    log_msg(content, lvl, LogType::QUEST);
}

pub fn log_items(event: ItemEvent, item_id: &String, room_id:&String, name: &String)
{
    let c_event = match event {
        ItemEvent::DROP => "Item Drop",
        ItemEvent::TAKE => "Item Take",
    };
    let lvl = LogLvl::INFO;
    let content = format!(
        "{{\"event\": {}, \"user\": {}, \"item_id\": \"{}\", \"room_id\": \"{}\"}}",
        c_event, name, room_id, item_id);
    log_msg(content, lvl, LogType::ITEM);
}

pub fn log_npc_death(npc: &String, room_id: &String, name: &str) {

    let lvl = LogLvl::INFO;

    let content = format!(
        "{{\"event\": \"NPC_DEATH\", \"npc\": \"{}\", \"user\": {}, \"room_id\": \"{}\"}}",
        npc, name, room_id);
    log_msg(content, lvl, LogType::ITEM);
}

pub fn log_player_death(user: &str, room_id: &String, cause: &str) {
    let lvl = LogLvl::INFO;

    let content: String = format!(
        "{{\"event\": \"PLAYER_DEATH\", \"user\": {}, \"room_id\": \"{}\",  \"cause\": \"{}\"}}",
        user, room_id, cause);
    log_msg(content, lvl, LogType::ITEM);
}
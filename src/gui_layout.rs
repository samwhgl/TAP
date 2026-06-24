use eframe::egui;
use eframe::egui::RichText;

enum ButtonAction {
    ImmediateSend,
    Fill,
}
pub struct ChatLog {
    username: String,
    message: String,
}

pub struct GuiWindow {
    input: String,
    tx: tokio::sync::mpsc::Sender<String>,
    rx: std::sync::mpsc::Receiver<String>,
    display_logs: Vec<String>,

    chats_inputs: [String; 3],
    chats_logs: [Vec<ChatLog>; 3],

    focus_main_input: bool,
}

impl GuiWindow {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        tx: tokio::sync::mpsc::Sender<String>,
        rx: std::sync::mpsc::Receiver<String>) -> Self {

        Self {
            input: String::new(),
            tx,
            rx,
            display_logs: Vec::new(),

            chats_inputs: [String::new(), String::new(), String::new()],
            chats_logs: [Vec::new(), Vec::new(), Vec::new()],

            focus_main_input: false,
        }
    }

    fn draw_left_panel(&mut self, ui: &mut egui::Ui, new_chat_msgs: [bool; 3]) {
      egui::Panel::left("left_panel")
            .default_size(400.0)
            .resizable(false)
            .show_inside(ui, |ui| {

                let chat_height = ui.available_height() / 3.0;
                let chat_labels = ["Global", "Room", "Group"];
                let chat_commands = ["CHAT GLOBAL", "CHAT ROOM", "CHAT GROUP"];

                for i in 0..3 {
                    ui.allocate_ui_with_layout(
                        egui::Vec2::new(ui.available_width(), chat_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                                ui.label(RichText::new(chat_labels[i]).size(22.0));
                            });

                            let scroll_height = chat_height - 72.0;
                            egui::Frame::NONE
                                .fill(egui::Color32::from_rgb(0, 0, 0))
                                .corner_radius(4.0)
                                .inner_margin(egui::Margin::same(6))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    egui::ScrollArea::vertical()
                                        .id_salt(format!("chat{}", i))
                                        .max_height(scroll_height)
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            let color = match i {
                                                0 => egui::Color32::LIGHT_BLUE,
                                                1 => egui::Color32::DARK_GREEN,
                                                2 => egui::Color32::ORANGE,
                                                _ => egui::Color32::LIGHT_GRAY,
                                            };
                                            for log in &self.chats_logs[i] {
                                                ui.label(RichText::new( format!("{}: {} ",log.username, log.message)).color(color)); 
                                            }
                                            if new_chat_msgs[i] {
                                                ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                                            }
                                    });
                                });
                            
                            ui.horizontal(|ui| {
                                let button_width= 50.0;
                                let input_width = ui.available_width() - button_width - ui.spacing().item_spacing.x;
                                let input_widget = egui::TextEdit::singleline(&mut self.chats_inputs[i]);
                                let text_edit_response = ui.add_sized(
                                    [input_width, ui.spacing().interact_size.y],
                                    input_widget
                                );
                                let button_response = ui.add_sized(
                                    [button_width, ui.spacing().interact_size.y],
                                    egui::Button::new("Send")
                                );

                                let button_clicked = button_response.clicked();
                                let enter_pressed = text_edit_response.lost_focus() && ui.input(|input_state| input_state.key_pressed(egui::Key::Enter));
                                if button_clicked || enter_pressed {
                                    if !self.chats_inputs[i].is_empty() {
                                        let command = format!("{} {}\n", chat_commands[i], self.chats_inputs[i].trim());
                                        let _ = self.tx.try_send(command.clone());
                                        self.display_logs.push(format!("=> {}",command));
                                        self.chats_inputs[i].clear();
                                    }
                                    text_edit_response.request_focus();
                                }
                            });
                        },
                    );
                    if i< 2 {
                        ui.separator();
                    }
                }
            });
    }
    
    fn draw_right_panel(&mut self, ui: &mut egui::Ui) {
        let commands = [
            ("CONNECT ...", ButtonAction::Fill),
            ("STATUS", ButtonAction::ImmediateSend),
            ("WHO", ButtonAction::ImmediateSend),
            ("LOOK", ButtonAction::ImmediateSend),
            ("TAKE ...", ButtonAction::Fill),
            ("INVENTORY", ButtonAction::ImmediateSend),
            ("DROP ...", ButtonAction::Fill),
            ("TALK ...", ButtonAction::Fill),
            ("QUEST ...", ButtonAction::Fill),
            ("QUESTS", ButtonAction::ImmediateSend),
            ("ATTACK ...", ButtonAction::Fill),
            ("GROUP CREATE ...", ButtonAction::Fill),
            ("GROUP INVITE ...", ButtonAction::Fill),
            ("GROUP JOIN ...", ButtonAction::Fill),
            ("GROUP LEAVE ...", ButtonAction::Fill),
            ("QUIT", ButtonAction::ImmediateSend),
        ];
        egui::Panel::right("right_panel")
            .default_size(200.0)
            .resizable(false)
            .show_inside(ui, |ui| {
                ui.allocate_ui_with_layout(
                    egui::Vec2::splat(ui.available_width()),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        egui::Frame::NONE
                            .fill(egui::Color32::from_gray(30))
                            .corner_radius(4.0)
                            .show(ui, |ui| {
                                let square_size = egui::Vec2::splat(ui.available_width() - 10.0);
                                let (rect, _) = ui.allocate_exact_size(square_size, egui::Sense::click());

                                let painter = ui.painter_at(rect);
                                let center = rect.center();
                                let circle_radius = square_size.x / 2.0 - 10.0;
                                painter.circle_filled(
                                    center, 
                                    circle_radius, 
                                    egui::Color32::from_rgb(15, 15, 15)
                                );
                                painter.text(
                                    center,
                                    egui::Align2::CENTER_CENTER,
                                    "MOVE",
                                    egui::FontId::proportional(14.0),
                                    egui::Color32::from_gray(180)
                                );

                                let half_base = square_size.x / 6.0;       // Size of the base of a triangle
                                let height = square_size.x / 6.0;          // Height of the triangle button
                                let offset = circle_radius * 0.5;           // Distance from the center of the circle

                                let directions = [
                                    ("north", [center + egui::Vec2::new(0.0, -offset - height), center + egui::Vec2::new(-half_base, -offset), center + egui::Vec2::new(half_base, -offset)]),
                                    ("south", [center + egui::Vec2::new(0.0, offset + height), center + egui::Vec2::new(-half_base, offset), center + egui::Vec2::new(half_base, offset)]),
                                    ("west",  [center + egui::Vec2::new(-offset - height, 0.0), center + egui::Vec2::new(-offset, -half_base), center + egui::Vec2::new(-offset, half_base)]),
                                    ("east",  [center + egui::Vec2::new(offset + height, 0.0), center + egui::Vec2::new(offset, -half_base), center + egui::Vec2::new(offset, half_base)]),
                                ];

                                for (cmd, points) in directions {
                                    let bounding_rect = egui::Rect::from_points(&points);
                                    let is_hovered = ui.rect_contains_pointer(bounding_rect);
                                    let is_clicking = is_hovered && ui.input(|i| i.pointer.any_down());
                                    
                                    let color = if is_clicking {
                                        egui::Color32::LIGHT_BLUE
                                    } else if is_hovered {
                                        egui::Color32::LIGHT_GRAY
                                    } else {
                                        egui::Color32::GRAY
                                    };

                                    if is_hovered && ui.input(|i| i.pointer.any_pressed()) {
                                        let command_to_send = format!("MOVE {}\n", cmd);
                                        let _ = self.tx.try_send(command_to_send);
                                        
                                        self.display_logs.push(format!("=> MOVE {}", cmd));
                                    }

                                    painter.add(egui::Shape::convex_polygon(
                                        points.to_vec(),
                                        color,
                                        egui::Stroke::NONE,
                                    ));
                                }
                            })
                    });
                ui.separator();
                
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui|{
                    for (command, action) in commands {
                        let cmd_button = ui.add_sized([120.0, 30.0], egui::Button::new(command));
                        if cmd_button.clicked() {
                            match action {
                                ButtonAction::ImmediateSend => {
                                    let command_to_send = format!("{}\n", command);
                                    let _ = self.tx.try_send(command_to_send.clone());
                                    if command_to_send == "QUIT\n" {
                                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                                    }
                                    self.display_logs.push(format!("=> {}", command));
                                }
                                ButtonAction::Fill => {
                                    let clean_text = command.replace("...", "");
                                    self.input = clean_text;
                                    self.focus_main_input = true;
                                }
                            }      
                        }
                        ui.add_space(20.0);
                    }
                });
            });

    }
    
    fn draw_bottom_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("bottom_panel")
            .resizable(false)
            .show_inside(ui, |ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let button_width = 50.0;
                    let input_width = ui.available_width() - button_width - ui.spacing().item_spacing.x;
                    
                    let id = egui::Id::new("main_input");

                    let command_input = ui.add_sized(
                        [input_width, ui.spacing().interact_size.y],
                        egui::TextEdit::singleline(&mut self.input).id(id),
                    );

                    if self.focus_main_input {
                        command_input.request_focus();

                        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                            let len = self.input.chars().count();

                            state.cursor.set_char_range(Some(
                                egui::text::CCursorRange::one(
                                    egui::text::CCursor::new(len)
                                )
                            ));

                            state.store(ui.ctx(), id);
                        }

                        self.focus_main_input = false;
                    }
                    let button_clicked = ui.add_sized(
                        [button_width, ui.spacing().interact_size.y],
                        egui::Button::new("Send"),
                    )
                    .clicked();

                    let enter_pressed = command_input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                    if button_clicked || enter_pressed {
                        if !self.input.is_empty() {
                            let command = format!("{}\n", self.input);
                            let _ = self.tx.try_send(command.clone());
                            if command == "QUIT\n" {
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            self.display_logs.push(format!("=> {}", command));
                            self.input.clear();
                        }
                        command_input.request_focus();
                    }
                });
            ui.add_space(10.0);
        });

    }
}

impl eframe::App for GuiWindow {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut new_msg = false;
        let mut new_chat_msgs = [false; 3];
        while let Ok(msg) = self.rx.try_recv() {
            if msg.starts_with("EVT GLOBAL CHAT") {
                let split_msg: Vec<&str> = msg.splitn(5, ' ').collect();
                self.chats_logs[0].push(ChatLog{username: split_msg[3].to_string(), message: split_msg[4].to_string()});
                new_chat_msgs[0] = true;
            } else if msg.starts_with("EVT ROOM CHAT") {
                let split_msg: Vec<&str> = msg.splitn(5, ' ').collect();
                self.chats_logs[1].push(ChatLog{username: split_msg[3].to_string(), message: split_msg[4].to_string()});
                new_chat_msgs[1] = true;
            } else if msg.starts_with("EVT GROUP CHAT") {
                let split_msg: Vec<&str> = msg.splitn(5, ' ').collect();
                self.chats_logs[2].push(ChatLog{username: split_msg[3].to_string(), message: split_msg[4].to_string()});
                new_chat_msgs[2] = true;
            } else {
                self.display_logs.push(msg);
                new_msg = true;
            }
        }

        self.draw_left_panel(ui, new_chat_msgs);
        self.draw_right_panel(ui);
        self.draw_bottom_panel(ui);

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.vertical(|ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    ui.label(RichText::new("The Answer Protocol").size(22.0));
                });
                egui::Frame::NONE
                    .fill(egui::Color32::from_rgb(0, 0, 0))
                    .corner_radius(4.0)
                    // .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        // ui.set_height(ui.available_height() - 10.0);
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                for log in &self.display_logs {
                                    let couleur = if log.starts_with("OK") {
                                        egui::Color32::GREEN
                                    } else if log.starts_with("ERR") {
                                        egui::Color32::RED
                                    } else if log.starts_with("EVT") {
                                        egui::Color32::PURPLE
                                    } else if log.starts_with("NB_PLAYERS") {
                                        egui::Color32::ORANGE
                                    } else {
                                        egui::Color32::LIGHT_GRAY
                                    };
                                    ui.label(RichText::new(log.trim()).color(couleur));
                                }
                            if new_msg {
                                ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                            }
                        })
                    });
                });
        });
    }
}

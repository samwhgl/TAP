use eframe::egui;
use eframe::egui::RichText;
use egui::text_edit;

pub struct GuiWindow {
    input: String,
    connected: bool,
    tx: tokio::sync::mpsc::Sender<String>,
    rx: std::sync::mpsc::Receiver<String>,
    display_logs: Vec<String>,

    chats_inputs: [String; 3],
    chats_logs: [Vec<String>; 3],
}

impl GuiWindow {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        tx: tokio::sync::mpsc::Sender<String>,
        rx: std::sync::mpsc::Receiver<String>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        Self {
            input: String::new(),
            connected: false,
            tx,
            rx,
            display_logs: Vec::new(),

            chats_inputs: [String::new(), String::new(), String::new()],
            chats_logs: [Vec::new(), Vec::new(), Vec::new()],
        }
    }

    fn draw_left_panel(&mut self, ui: &mut egui::Ui) {
      egui::Panel::left("left_panel")
            .default_size(400.0)
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
                                            ui.small("message de test...");
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
                                let enter_pressed = text_edit_response.lost_focus() && ui.input(|ins| ins.key_pressed(egui::Key::Enter));
                                if button_clicked || enter_pressed {
                                    if !self.chats_inputs[i].is_empty() {
                                        let command = format!("{} {}\n", chat_commands[i], self.chats_inputs[i].trim());
                                        let _ = self.tx.try_send(command.clone());
                                        self.chats_logs[i].push(format!("=> {}",self.chats_inputs[i]));
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
}

impl eframe::App for GuiWindow {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut new_msg = false;
        while let Ok(msg) = self.rx.try_recv() {
            self.display_logs.push(msg);
            new_msg = true;
        }

        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui
        self.draw_left_panel(ui);
        egui::Panel::right("right_panel")
            .default_size(150.0)
            .resizable(true)
            .show_inside(ui, |ui| {
                ui.heading("Chat");
                ui.separator();
            });
        egui::Panel::bottom("bottom_panel").show_inside(ui, |ui| {
            // The top panel is often a good place for a menu bar:

            ui.horizontal(|ui| {
                let button_width = 50.0;
                let input_width = ui.available_width() - button_width - ui.spacing().item_spacing.x;

                let command_input = ui.add_sized(
                    [input_width, ui.spacing().interact_size.y],
                    egui::TextEdit::singleline(&mut self.input),
                );
                let button_clicked = ui.add_sized(
                        [button_width, ui.spacing().interact_size.y],
                        egui::Button::new("Send"),
                    )
                    .clicked();

                let enter_pressed = command_input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                if button_clicked || enter_pressed
                {
                    if !self.input.is_empty() {
                        let command = format!("{}\n", self.input);
                        let _ = self.tx.try_send(command.clone());
                        self.display_logs.push(format!("=> {}", command));
                        self.input.clear();
                    }
                    command_input.request_focus();
                }
            });
        });

        let central_area = ui.available_rect_before_wrap();
        egui::CentralPanel::default().show_inside(ui, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            egui::Frame::NONE
                .fill(egui::Color32::from_rgb(0, 0, 0))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.expand_to_include_rect(central_area);
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

            ui.separator();
        });
    }
}

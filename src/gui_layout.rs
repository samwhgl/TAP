use eframe::egui;
use eframe::egui::RichText;

pub struct GuiWindow {
    input: String,
    connected: bool,
    tx: tokio::sync::mpsc::Sender<String>,
    rx: std::sync::mpsc::Receiver<String>,
    display_logs: Vec<String>
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
        }
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

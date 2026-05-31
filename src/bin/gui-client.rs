use tokio::net::TcpStream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tap_server::gui_layout::GuiWindow;

#[tokio::main]
async fn main() -> eframe::Result {
    let socket = TcpStream::connect("127.0.0.1:4242").await.unwrap();
    let (reader, mut writer) = socket.into_split();
    let mut socket_reader = BufReader::new(reader);

    let (tx_to_server, mut rx_from_gui) = tokio::sync::mpsc::channel::<String>(32);
    let (tx_to_gui, rx_from_server) = std::sync::mpsc::channel::<String>();

    tokio::spawn(async move {
        loop{
            tokio::select! {
                msg_from_gui =  rx_from_gui.recv() => {
                    if let Some(msg) = msg_from_gui {
                        println!("{}", msg);
                        if let Err(e) = writer.write_all(msg.as_bytes()).await {
                            eprintln!("Error while sending to server : {}", e);
                            break;
                        }
                        if msg == "QUIT\n" { return ; }
                    } else { break ; }
                }

                msg_from_server = async {
                    let mut buffer = String::new();
                    match socket_reader.read_line(&mut buffer).await {
                        Ok(0) => Err("closed".to_string()),
                        Ok(_) => Ok(buffer),
                        Err(e) => Err(e.to_string()),
                    }
                } => {
                    match msg_from_server {
                        Ok(output) => {
                            println!("{}", output);
                            if tx_to_gui.send(output).is_err() {
                                break;
                            }
                        }
                        Err(e) if e == "closed" => {
                            println!("Connection to server closed.");
                            break;
                        }
                        Err(e) => {
                            eprintln!("Error while reading from server: {}", e);
                            break;
                        }
                    }
                }
            }
        }
    });

    let native_option = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([780.0, 640.0])
            .with_min_inner_size([300.0, 220.0]),
        ..Default::default()
    };
    eframe::run_native(
        "The Answer Protocol",
        native_option,
        Box::new(|cc| {
            Ok(Box::new(GuiWindow::new(cc, tx_to_server, rx_from_server)))
        }),
    )
}

use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== TAP CLIENT CLI ===");
    println!("Tentative de connexion au serveur sur localhost:4242...");

    let stream = match TcpStream::connect("127.0.0.1:4242").await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Erreur : Impossible de se connecter au serveur. Vérifie qu'il est bien lancé !");
            return Err(e.into());
        }
    };
    println!("Connecté au serveur avec succès !\n");

    let (reader, mut writer) = stream.into_split();
    let mut server_reader = BufReader::new(reader);

    let stdin = io::stdin();
    let mut stdin_reader = BufReader::new(stdin);

    let mut server_line = String::new();
    let mut stdin_line = String::new();

    loop {
        server_line.clear();
        stdin_line.clear();

        tokio::select! {
            result = server_reader.read_line(&mut server_line) => {
                match result {
                    Ok(0) => {
                        println!("\n[Système] Le serveur a fermé la connexion.");
                        break;
                    }
                    Ok(_) => {
                        print!("{}", server_line);
                    }
                    Err(e) => {
                        eprintln!("\n[Erreur] Erreur de lecture réseau : {}", e);
                        break;
                    }
                }
            }

            result = stdin_reader.read_line(&mut stdin_line) => {
                match result {
                    Ok(0) => break,
                    Ok(_) => {
                        let cmd = stdin_line.trim();
                        if cmd.is_empty() {
                            continue;
                        }

                        let formatted_cmd = format!("{}\n", cmd);
                        if let Err(e) = writer.write_all(formatted_cmd.as_bytes()).await {
                            eprintln!("[Erreur] Impossible d'envoyer la commande : {}", e);
                            break;
                        }

                        if cmd == "QUIT" {
                            println!("[Système] Déconnexion demandée...");
                        }
                    }
                    Err(e) => {
                        eprintln!("[Erreur] Erreur de lecture clavier : {}", e);
                        break;
                    }
                }
            }
        }
    }

    println!("Fin du client CLI. À bientôt !");
    Ok(())
}

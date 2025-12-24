use common::{ChatMessage, MessageType, Handshake};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use chrono::Utc;

// Core structures
struct ClientInfo {
    tx: tokio::sync::mpsc::UnboundedSender<String>,
    username: String,
    room: String,
    _joined_at: chrono::DateTime<Utc>,
}

struct ChatServer {
    // Map of Username -> Client Data
    clients: Arc<Mutex<HashMap<String, ClientInfo>>>,
    // Broadcast channel for internal event bus
    broadcast_tx: broadcast::Sender<ChatMessage>,
    // History per room
    history: Arc<Mutex<HashMap<String, Vec<ChatMessage>>>>,
}

impl ChatServer {
    fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            broadcast_tx: tx,
            history: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn add_history(&self, room: &str, msg: ChatMessage) {
        let mut hist = self.history.lock().await;
        let room_hist = hist.entry(room.to_string()).or_insert_with(Vec::new);
        room_hist.push(msg);
        if room_hist.len() > 50 {
            room_hist.remove(0);
        }
    }

    async fn get_users_in_room(&self, room: &str) -> Vec<String> {
        let clients = self.clients.lock().await;
        clients.values()
            .filter(|c| c.room == room)
            .map(|c| c.username.clone())
            .collect()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    println!("╔══════════════════════════════════════════════╗");
    println!("║   🚀 Chat Server Running on Port 8080        ║");
    println!("╚══════════════════════════════════════════════╝");
    
    let server = Arc::new(ChatServer::new());

    loop {
        let (socket, addr) = listener.accept().await?;
        let server = server.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(socket, addr, server).await {
                eprintln!("Client error {}: {}", addr, e);
            }
        });
    }
}

async fn handle_client(
    socket: TcpStream,
    addr: SocketAddr,
    server: Arc<ChatServer>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (reader, mut writer) = socket.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    // 1. Handshake
    reader.read_line(&mut line).await?;
    let handshake: Handshake = match serde_json::from_str(line.trim()) {
        Ok(h) => h,
        Err(_) => {
            // Fallback for raw text (legacy support or telnet)
            Handshake { username: line.trim().to_string() }
        }
    };

    let username = handshake.username.trim().to_string();
    
    // Validate username
    if username.is_empty() || username.len() > 15 || !username.chars().all(char::is_alphanumeric) {
        let _ = writer.write_all(b"Error: Invalid username (alphanumeric, max 15)\n").await;
        return Ok(());
    }

    {
        let clients = server.clients.lock().await;
        if clients.contains_key(&username) {
            let _ = writer.write_all(b"Error: Username taken\n").await;
            return Ok(());
        }
    }

    println!("(+) {} connected from {}", username, addr);
    
    // 2. Setup channels
    let (client_tx, mut client_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut broadcast_rx = server.broadcast_tx.subscribe();
    
    // Default room
    let current_room = "general".to_string();

    // 3. Register Client
    {
        let mut clients = server.clients.lock().await;
        clients.insert(username.clone(), ClientInfo {
            tx: client_tx.clone(),
            username: username.clone(),
            room: current_room.clone(),
            _joined_at: Utc::now(),
        });
    }

    // Send initial history
    {
        let hist_lock = server.history.lock().await;
        if let Some(msgs) = hist_lock.get(&current_room) {
            for msg in msgs {
                let _ = client_tx.send(msg.to_json());
            }
        }
    }

    // Announce join
    let join_msg = ChatMessage::new("System".into(), format!("{} joined room '{}'", username, current_room), current_room.clone(), MessageType::UserJoin);
    let _ = server.broadcast_tx.send(join_msg.clone());
    server.add_history(&current_room, join_msg).await;

    // 4. Writer Task (Forwarding logic)
    // This task takes messages from the mpsc channel AND the broadcast channel
    // and writes them to the TCP socket.
    let writer_handle = {
        let username = username.clone();
        let server = server.clone();
        
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Receive personal messages (history, errors, PMs)
                    Some(msg_json) = client_rx.recv() => {
                         if writer.write_all(format!("{}\n", msg_json).as_bytes()).await.is_err() { break; }
                    }
                    // Receive global broadcasts
                    Ok(msg) = broadcast_rx.recv() => {
                        // FILTERING LOGIC: Only show messages for my room or PMs
                        let my_room = {
                            let clients = server.clients.lock().await;
                            clients.get(&username).map(|c| c.room.clone()).unwrap_or_default()
                        };

                        let should_send = match msg.msg_type {
                            MessageType::PrivateMessage => false, // PMs handled via send_private
                            _ => msg.room == my_room
                        };

                        if should_send {
                            if writer.write_all(format!("{}\n", msg.to_json()).as_bytes()).await.is_err() { break; }
                        }
                    }
                }
            }
        })
    };

    // 5. Reader Loop
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break, // EOF
            Ok(_) => {
                let input = line.trim();
                if input.is_empty() { continue; }

                // Get current state
                let my_room = {
                    let clients = server.clients.lock().await;
                    clients.get(&username).map(|c| c.room.clone()).unwrap_or("general".to_string())
                };

                if input.starts_with('/') {
                    let parts: Vec<&str> = input.split_whitespace().collect();
                    match parts[0] {
                        "/join" => {
                            if parts.len() < 2 {
                                let _ = client_tx.send(ChatMessage::error("Usage: /join <room>".into()).to_json());
                            } else {
                                let new_room = parts[1].to_string();
                                
                                // Leave old room message
                                let leave = ChatMessage::new("System".into(), format!("{} left", username), my_room.clone(), MessageType::UserLeave);
                                let _ = server.broadcast_tx.send(leave);
                                
                                // Update state
                                {
                                    let mut clients = server.clients.lock().await;
                                    if let Some(c) = clients.get_mut(&username) {
                                        c.room = new_room.clone();
                                    }
                                }

                                // Send history of new room
                                {
                                    // Clear client screen hack by sending system msg? No, client handles clears.
                                    let hist_lock = server.history.lock().await;
                                    if let Some(msgs) = hist_lock.get(&new_room) {
                                        for msg in msgs {
                                            let _ = client_tx.send(msg.to_json());
                                        }
                                    }
                                }

                                // Announce join new room
                                let join = ChatMessage::new("System".into(), format!("{} joined room '{}'", username, new_room), new_room.clone(), MessageType::RoomChange);
                                let _ = server.broadcast_tx.send(join.clone());
                                server.add_history(&new_room, join).await;
                            }
                        }
                        "/msg" | "/w" => {
                            if parts.len() < 3 {
                                let _ = client_tx.send(ChatMessage::error("Usage: /msg <user> <text>".into()).to_json());
                            } else {
                                let target = parts[1];
                                let content = parts[2..].join(" ");
                                let pm = ChatMessage::private(username.clone(), target.to_string(), content);
                                
                                // Send to target
                                let clients = server.clients.lock().await;
                                if let Some(c) = clients.get(target) {
                                    let _ = c.tx.send(pm.to_json());
                                    // Echo to self
                                    let _ = client_tx.send(pm.to_json());
                                } else {
                                    let _ = client_tx.send(ChatMessage::error("User not found".into()).to_json());
                                }
                            }
                        }
                        "/users" => {
                            let users = server.get_users_in_room(&my_room).await;
                            let msg = ChatMessage::system(format!("Users in {}: {}", my_room, users.join(", ")), my_room);
                            let _ = client_tx.send(msg.to_json());
                        }
                        "/kick" => {
                            // Simple admin check: anyone named "admin" is admin
                            if username == "admin" && parts.len() > 1 {
                                let target = parts[1];
                                let mut clients = server.clients.lock().await;
                                if let Some(c) = clients.remove(target) {
                                    // The drop of 'c' will close the channel, but let's be nice
                                    let _ = c.tx.send(ChatMessage::error("You have been kicked.".into()).to_json());
                                    let msg = ChatMessage::system(format!("{} kicked {}", username, target), my_room.clone());
                                    let _ = server.broadcast_tx.send(msg); // Incorrect type, fix below
                                }
                            }
                        }
                        _ => {
                            let _ = client_tx.send(ChatMessage::error("Unknown command".into()).to_json());
                        }
                    }
                } else {
                    // Normal Chat
                    let msg = ChatMessage::chat(username.clone(), input.to_string(), my_room.clone());
                    server.add_history(&my_room, msg.clone()).await;
                    let _ = server.broadcast_tx.send(msg);
                }
            }
            Err(_) => break,
        }
    }

    // Cleanup
    writer_handle.abort();
    {
        let mut clients = server.clients.lock().await;
        // Check room one last time for leave message
        if let Some(c) = clients.remove(&username) {
            let msg = ChatMessage::new("System".into(), format!("{} disconnected", username), c.room, MessageType::UserLeave);
            let _ = server.broadcast_tx.send(msg);
        }
    }
    
    println!("(-) {} disconnected", username);
    Ok(())
}

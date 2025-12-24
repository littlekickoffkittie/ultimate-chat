use common::{ChatMessage, MessageType, Handshake};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph, BorderType, Clear},
};
use std::io;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tui_input::{backend::crossterm::EventHandler, Input};

// UI State
struct App {
    messages: Vec<ChatMessage>,
    input: Input,
    username: String,
    current_room: String,
    users_in_room: Vec<String>, // Maintained via system messages for simplicity in this demo
    connected: bool,
    scroll_offset: usize,
    auto_scroll: bool,
    show_help: bool,
}

impl App {
    fn new(username: String) -> Self {
        Self {
            messages: vec![],
            input: Input::default(),
            username,
            current_room: "general".to_string(),
            users_in_room: vec![], 
            connected: false,
            scroll_offset: 0,
            auto_scroll: true,
            show_help: false,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup Terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Login Screen
    terminal.clear()?;
    let username = login_screen(&mut terminal)?;
    
    // Connect
    let stream = match TcpStream::connect("127.0.0.1:8080").await {
        Ok(s) => s,
        Err(e) => {
            disable_raw_mode()?;
            execute!(io::stdout(), LeaveAlternateScreen)?;
            eprintln!("Failed to connect: {}", e);
            return Ok(());
        }
    };

    let (reader, writer) = stream.into_split();
    let writer = Arc::new(Mutex::new(writer));

    // Send Handshake
    let handshake = Handshake { username: username.clone() };
    writer.lock().await.write_all(format!("{}\n", serde_json::to_string(&handshake)?).as_bytes()).await?;

    // Init App State
    let app = Arc::new(Mutex::new(App::new(username)));
    app.lock().await.connected = true;

    // Network Reader Task
    let app_clone = app.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(reader);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break,
                Ok(_) => {
                    let text = line.trim();
                    if text.starts_with("Error:") {
                        // Handle raw errors
                        // In a real app, handle gracefully. Here just print to chat.
                    } else if let Ok(msg) = ChatMessage::from_json(text) {
                        let mut state = app_clone.lock().await;
                        
                        // Handle room changes to clear/update UI state
                        if msg.msg_type == MessageType::RoomChange && msg.username == state.username {
                            state.current_room = msg.room.clone();
                            state.messages.clear(); // Clear history on room switch
                        }
                        
                        // Handle joins/leaves for user list (Naive implementation)
                        if msg.msg_type == MessageType::UserJoin {
                           if !state.users_in_room.contains(&msg.username) {
                               state.users_in_room.push(msg.username.clone());
                           }
                        }

                        state.messages.push(msg);
                        if state.auto_scroll {
                            state.scroll_offset = 0;
                        }
                    }
                }
                Err(_) => break,
            }
        }
        app_clone.lock().await.connected = false;
    });

    // Main UI Loop
    loop {
        let mut app_guard = app.lock().await;
        
        // Draw
        terminal.draw(|f| draw_ui(f, &mut app_guard))?;

        if !app_guard.connected {
            break; // Exit if server dies
        }

        // Input Handling
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Esc => {
                        app_guard.show_help = !app_guard.show_help;
                    },
                    KeyCode::Enter => {
                        let input: String = app_guard.input.value().into();
                        if !input.is_empty() {
                            // Command handling on client side if needed, otherwise send
                            if input == "/quit" {
                                drop(app_guard);
                                break;
                            }
                            let payload = format!("{}\n", input);
                            writer.lock().await.write_all(payload.as_bytes()).await?;
                            app_guard.input.reset();
                        }
                    },
                    KeyCode::PageUp => {
                        app_guard.auto_scroll = false;
                        app_guard.scroll_offset = app_guard.scroll_offset.saturating_add(5);
                    },
                    KeyCode::PageDown => {
                        app_guard.scroll_offset = app_guard.scroll_offset.saturating_sub(5);
                        if app_guard.scroll_offset == 0 {
                            app_guard.auto_scroll = true;
                        }
                    },
                    _ => {
                        app_guard.input.handle_event(&Event::Key(key));
                    }
                }
            }
        }
    }

    // Cleanup
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn login_screen(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<String, io::Error> {
    let mut input = Input::default();
    loop {
        terminal.draw(|f| {
            let area = centered_rect(60, 20, f.area());
            let block = Block::default().borders(Borders::ALL).title(" Login ").border_type(BorderType::Rounded).style(Style::default().fg(Color::Cyan));
            f.render_widget(block, area);

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(2)
                .constraints([Constraint::Length(1), Constraint::Length(3), Constraint::Min(1)])
                .split(area);
            
            f.render_widget(Paragraph::new("Welcome to Ultimate Chat").alignment(Alignment::Center), chunks[0]);
            
            let input_block = Block::default().borders(Borders::ALL).title(" Username ");
            f.render_widget(Paragraph::new(input.value()).block(input_block), chunks[1]);
            
            f.render_widget(Paragraph::new("Press Enter to join\nEsc to quit").style(Style::default().fg(Color::DarkGray)), chunks[2]);
        })?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Enter => {
                    if !input.value().is_empty() {
                        return Ok(input.value().to_string());
                    }
                }
                KeyCode::Esc => return Err(io::Error::new(io::ErrorKind::Interrupted, "Quit")),
                _ => { input.handle_event(&Event::Key(key)); }
            }
        }
    }
}

fn draw_ui(f: &mut Frame, app: &mut App) {
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.area());

    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20), // Sidebar
            Constraint::Percentage(80), // Chat
        ])
        .split(main_layout[0]);

    // --- Sidebar (Left) ---
    let sidebar_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Info ")
        .style(Style::default().fg(Color::Blue));

    let room_info = vec![
        Line::from(vec![Span::raw("Room: "), Span::styled(&app.current_room, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))]),
        Line::from(""),
        Line::from(Span::styled("Users:", Style::default().add_modifier(Modifier::UNDERLINED))),
        // Note: Real user list requires syncing from server, using simplified placeholder or captured joins
        Line::from(vec![Span::raw("• "), Span::raw(&app.username)]),
    ];

    let info_paragraph = Paragraph::new(room_info).block(sidebar_block);
    f.render_widget(info_paragraph, content_layout[0]);

    // --- Chat Area (Right) ---
    let chat_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(format!(" Messages ({}) ", app.messages.len()));
    
    let messages: Vec<ListItem> = app.messages.iter().rev().skip(app.scroll_offset).take(f.area().height as usize).map(|msg| {
        let (sender_style, content_style) = match msg.msg_type {
            MessageType::Chat => if msg.username == app.username {
                (Style::default().fg(Color::Green).add_modifier(Modifier::BOLD), Style::default())
            } else {
                (Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD), Style::default())
            },
            MessageType::System | MessageType::UserJoin | MessageType::UserLeave | MessageType::RoomChange => 
                (Style::default().fg(Color::Yellow), Style::default().fg(Color::Yellow)),
            MessageType::PrivateMessage => 
                (Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD), Style::default().fg(Color::LightMagenta)),
            MessageType::Error => 
                (Style::default().fg(Color::Red), Style::default().fg(Color::Red)),
        };

        let prefix = match msg.msg_type {
            MessageType::PrivateMessage => "🔒 ",
            MessageType::System => "ℹ ",
            _ => ""
        };

        let line = Line::from(vec![
            Span::styled(format!("{} ", msg.format_time()), Style::default().fg(Color::DarkGray)),
            Span::raw(prefix),
            Span::styled(format!("{}: ", msg.username), sender_style),
            Span::styled(&msg.content, content_style),
        ]);
        ListItem::new(line)
    }).collect();

    // Reverse list for chat effect (newest at bottom)
    // Actually, we are iterating rev(), so we need to render them top-down but logic is inverted.
    // Ratatui List renders top to bottom.
    // To implement a "stick to bottom" chat, we usually reverse the iterator.
    let list = List::new(messages)
        .block(chat_block)
        .direction(ratatui::widgets::ListDirection::BottomToTop); // New feature in Ratatui
    
    f.render_widget(list, content_layout[1]);

    // --- Input Area (Bottom) ---
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Input ");
    
    let input_para = Paragraph::new(app.input.value())
        .block(input_block)
        .style(Style::default().fg(Color::Yellow));
    
    f.render_widget(input_para, main_layout[1]);

    // Cursor
    f.set_cursor_position(Position::new(
        main_layout[1].x + 1 + app.input.visual_cursor() as u16,
        main_layout[1].y + 1,
    ));

    // Help Overlay
    if app.show_help {
        let area = centered_rect(60, 60, f.area());
        let help_text = vec![
            "Commands:",
            "/join <room> - Switch rooms",
            "/msg <user> <msg> - Private Message",
            "/users - List users",
            "/quit - Exit",
            "",
            "Keys:",
            "PgUp/PgDn - Scroll History",
            "Esc - Toggle Help",
        ].join("\n");
        
        let block = Paragraph::new(help_text)
            .block(Block::default().borders(Borders::ALL).title(" Help ").style(Style::default().bg(Color::DarkGray)));
        f.render_widget(Clear, area);
        f.render_widget(block, area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

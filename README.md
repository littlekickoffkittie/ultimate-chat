# Ultimate CLI Chat

A highly advanced terminal chat application featuring multi-room support, sidebars, and robust networking.

## Features
- 🏠 **Multi-Room Support**: Join different channels with `/join <room_name>`
- 🎨 **Modern TUI**: Split view with Sidebar Info and Main Chat
- 🔒 **Private Messaging**: `/msg <user> <message>`
- 📜 **History**: Server remembers last 50 messages per room
- ⚡ **Async**: Built on Tokio for high concurrency

## Commands
- `/join <room>` - Switch to a different chat room
- `/msg <user> <text>` - Send a private message (Whisper)
- `/users` - List users in current room
- `/kick <user>` - (Admin only) Kick a user
- `/quit` - Exit the application

## Running
1. Start Server: `cargo run -p server`
2. Start Client: `cargo run -p client`

use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::fs;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::collections::HashSet;
use std::path::Path;

#[tokio::main]
async fn main(){
    let listener = TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("server listening on port 8080....");
    let active_users = Arc::new(Mutex::new(HashSet::new()));
    loop {
        let (socket,addr) = listener.accept().await.unwrap();
        println!("new connection from: {}", addr);
        let users = Arc::clone(&active_users);
        // Spawn a new task to handle the client
        tokio::spawn(async move {
            if let Err(e) = handle_client(socket,users).await {
                eprintln!("Error handling client {}: {}", addr, e);
            }
        });

    }
}

async fn handle_client(mut socket : TcpStream,users : Arc<Mutex<HashSet<String>>>) -> Result<(), Box<dyn std::error::Error>>{
    let mut buffer = [0;1024]; // buffer to store recived data

    // Request username from the client
    socket.write_all(b"Enter your username: ").await?;
    let n = socket.read(&mut buffer).await?;
    if n == 0 {
        return Ok(()); // Client disconnected
    }
    let username = String::from_utf8_lossy(&buffer[..n]).trim().to_string();

    // Ensure username is unique
    {
        let mut active_users = users.lock().await;
        if active_users.contains(&username) {
            socket.write_all(b"Username already taken. Try again.\n").await?;
            return Ok(());
        }
        active_users.insert(username.clone());
    }

    // Create user's directory if not exists
    let user_dir = format!("users/{}", username);
    if !Path::new(&user_dir).exists() {
        fs::create_dir_all(&user_dir).await?;
    }
    let mut text_buffer = String::new();
    // Send a "virtual notepad" interface on connection
    let welcome_message = "\n--- Virtual Notepad ---\nType to edit. Send 'SAVE' to save. Send 'EXIT' to quit.\n\n";
    socket.write_all(welcome_message.as_bytes()).await?;


    loop{
        match  socket.read(& mut buffer).await {
            Ok(0) => {
                println!("client disconnected.");
                return Ok(());
            }
            Ok(n) => {
                let received_data = String::from_utf8_lossy(&buffer[..n]).trim().to_string();
            match received_data.split_whitespace().next() {
                Some("SAVE") => {
                    let parts: Vec<&str> = received_data.split_whitespace().collect();
                    let filename = if parts.len() > 1 { parts[1] } else { "notepad.txt" };
                    let filepath = format!("{}/{}", user_dir, filename);
                    println!("Saving to file: {}", filepath);
                    fs::write(filepath, &text_buffer).await?;
                    socket.write_all(b"\n[file saved]\n").await?;
                    text_buffer.clear();
                }
                Some("LOAD") => {
                    let parts: Vec<&str> = received_data.split_whitespace().collect();
                    if parts.len() > 1 {
                        let filename = parts[1];
                        let filepath = format!("{}/{}", user_dir, filename);

                        match fs::read_to_string(filepath).await {
                            Ok(contents) => {
                                text_buffer = contents;
                                let file_loaded_msg = format!("\n[File '{}' Loaded]\n", filename);
                                socket.write_all(file_loaded_msg.as_bytes()).await?;
                            }
                            Err(_) => {
                                socket.write_all(b"\n[Error: File Not Found]\n").await?;
                            }
                        }
                    } else {
                        socket.write_all(b"\n[Usage: LOAD filename.txt]\n").await?;
                    }
                }
                Some("LS") => {
                    match fs::read_dir(&user_dir).await {
                        Ok(mut entries) => {
                            let mut file_list = String::new();
                            while let Some(entry) = entries.next_entry().await? {
                                if let Some(name) = entry.file_name().to_str() {
                                    file_list.push_str(name);
                                    file_list.push('\n');
                                }
                            }
                            socket.write_all(file_list.as_bytes()).await?;
                        }
                        Err(_) => {
                            socket.write_all(b"\n[Error: Could not list files]\n").await?;
                        }
                    }
                }
                
                Some("EXIT") => {
                    println!("Exiting notepad.");
                    {
                        let mut active_users = users.lock().await;
                        active_users.remove(&username);
                    }
                    return Ok(());
                }
                _ => {
                    text_buffer.push_str(&received_data);
                    text_buffer.push('\n');
                    // Clear screen and resend the updated buffer to refresh the client UI
                    let clear_screen = "\x1B[2J\x1B[H"; // ANSI escape code to clear screen
                    let updated_view = format!("{}--- Virtual Notepad ---\n{}\n", clear_screen, text_buffer);
                    socket.write_all(updated_view.as_bytes()).await?;
                }
            }
        
        }
            Err(e) =>{
                eprintln!("Error reading from socket:{}",e);
                return Err(e.into());
            }
        }
    }

}
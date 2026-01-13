//! Shell command handlers

use crate::drivers;
use crate::drivers::drives::DRIVE_MANAGER;
use super::{Console, Shell};
use alloc::string::String;
use alloc::vec::Vec;

pub fn handle_command(input: &str, console: &mut Console, shell: &mut Shell) {
    let parts: Vec<&str> = input.split(' ').collect();
    
    if parts.is_empty() || parts[0].is_empty() {
        return;
    }

    match parts[0] {
        "help" => cmd_help(console),
        "ls" => cmd_ls(console, shell),
        "cd" => cmd_cd(&parts, console, shell),
        "pwd" => cmd_pwd(console, shell),
        "mkdir" => cmd_mkdir(&parts, console, shell),
        "save" => cmd_save(&parts, console, shell),
        "cat" => cmd_cat(&parts, console, shell),
        "rm" => cmd_rm(&parts, console, shell),
        "stat" => cmd_stat(&parts, console, shell),
        "df" => cmd_df(console, shell),
        "disk" => cmd_disk(&parts, console, shell),
        "clear" => cmd_clear(console),
        _ => console.print("Unknown command. Type 'help'\n"),
    }
}

fn cmd_help(console: &mut Console) {
    console.print("--- NAVIGATION ---\n");
    console.print("  cd <dir>     - Change directory\n");
    console.print("  cd ..        - Go up one level\n");
    console.print("  cd /         - Go to root\n");
    console.print("  pwd          - Print current path\n");
    console.print("  ls           - List files/drives\n");
    console.print("\n--- FILE SYSTEM ---\n");
    console.print("  save <name>  - Create/save file\n");
    console.print("  cat <name>   - Show file content\n");
    console.print("  mkdir <name> - Create directory\n");
    console.print("  rm <name>    - Delete file\n");
    console.print("  stat <name>  - Show file info\n");
    console.print("  df           - Show disk usage\n");
    console.print("\n--- DISK ---\n");
    console.print("  disk list    - List drives\n");
    console.print("  disk format  - Format current drive\n");
    console.print("\n--- SYSTEM ---\n");
    console.print("  clear        - Clear screen\n");
    console.print("  help         - Show this help\n");
}

fn cmd_cd(parts: &[&str], console: &mut Console, shell: &mut Shell) {
    if parts.len() < 2 {
        console.print("Usage: cd <directory>\n");
        return;
    }
    
    if !shell.cd(parts[1]) {
        if shell.at_root() {
            console.print("Drive not found. Use 'ls' to see drives.\n");
        } else {
            console.print("Directory not found\n");
        }
    }
}

fn cmd_pwd(console: &mut Console, shell: &Shell) {
    console.print(&alloc::format!("/{}\n", shell.get_prompt()));
}

fn cmd_ls(console: &mut Console, shell: &Shell) {
    // If at root, show drives
    if shell.at_root() {
        let manager = DRIVE_MANAGER.lock();
        let drives = manager.list_drives();
        
        if drives.is_empty() {
            console.print("No drives detected\n");
            console.print("(Running in memory-only mode)\n");
        } else {
            for (name, size_mb) in drives {
                console.print(&alloc::format!("[Drv] {} ({}MB)\n", name, size_mb));
            }
        }
        return;
    }
    
    // Show ".." to go back
    console.print("[Dir] ..\n");
    
    // Get current drive
    let drive_name = shell.current_path[0].clone();
    let manager = DRIVE_MANAGER.lock();
    
    if let Some(drive) = manager.get_drive(&drive_name) {
        let files = drive.list_files();
        
        // Build current path prefix (exclude drive name)
        let current_prefix = if shell.current_path.len() <= 1 {
            String::new()
        } else {
            alloc::format!("{}/", shell.current_path[1..].join("/"))
        };
        
        let mut found = false;
        
        for (name, is_dir) in files {
            if shell.current_path.len() <= 1 {
                // At drive root - show files without '/' in name
                if !name.contains('/') {
                    found = true;
                    if is_dir {
                        console.print(&alloc::format!("[Dir] {}\n", name));
                    } else {
                        console.print(&alloc::format!("[Txt] {}\n", name));
                    }
                }
            } else {
                // In subfolder
                if name.starts_with(&current_prefix) {
                    let rel_name = &name[current_prefix.len()..];
                    if !rel_name.contains('/') && !rel_name.is_empty() {
                        found = true;
                        if is_dir {
                            console.print(&alloc::format!("[Dir] {}\n", rel_name));
                        } else {
                            console.print(&alloc::format!("[Txt] {}\n", rel_name));
                        }
                    }
                }
            }
        }
        
        if !found {
            console.print("(empty)\n");
        }
    } else {
        console.print("Drive not found\n");
    }
}

fn cmd_mkdir(parts: &[&str], console: &mut Console, shell: &Shell) {
    if parts.len() < 2 {
        console.print("Usage: mkdir <name>\n");
        return;
    }
    
    if shell.at_root() {
        console.print("Cannot create directory here. cd into a drive first.\n");
        return;
    }
    
    let drive_name = shell.current_path[0].clone();
    let full_path = shell.full_path(parts[1]);
    
    let mut manager = DRIVE_MANAGER.lock();
    if let Some(drive) = manager.get_drive_mut(&drive_name) {
        if drive.create_directory(&full_path) {
            console.print("Directory created\n");
        } else {
            console.print("Failed: name already exists\n");
        }
    } else {
        console.print("Drive not found\n");
    }
}

fn cmd_save(parts: &[&str], console: &mut Console, shell: &Shell) {
    if parts.len() < 2 {
        console.print("Usage: save <name>\n");
        return;
    }
    
    if shell.at_root() {
        console.print("Cannot create file here. cd into a drive first.\n");
        return;
    }
    
    let drive_name = shell.current_path[0].clone();
    let full_path = shell.full_path(parts[1]);
    
    // Create file
    {
        let mut manager = DRIVE_MANAGER.lock();
        if let Some(drive) = manager.get_drive_mut(&drive_name) {
            if !drive.create_file(&full_path) {
                console.print("Failed: file already exists\n");
                return;
            }
        } else {
            console.print("Drive not found\n");
            return;
        }
    }
    
    console.print("Type content (enter twice to finish):\n");
    
    let mut content = String::new();
    let mut empty_lines = 0;
    
    loop {
        if let Some(c) = drivers::keyboard::try_read_char() {
            match c {
                '\n' => {
                    empty_lines += 1;
                    console.print("\n");
                    if empty_lines >= 2 { break; }
                    content.push(c);
                }
                '\u{0008}' => {
                    if !content.is_empty() {
                        content.pop();
                        console.backspace();
                        empty_lines = 0;
                    }
                }
                _ => {
                    content.push(c);
                    console.print_char(c);
                    empty_lines = 0;
                }
            }
        }
        core::hint::spin_loop();
    }
    
    let mut manager = DRIVE_MANAGER.lock();
    if let Some(drive) = manager.get_drive_mut(&drive_name) {
        drive.write_file(&full_path, content.as_bytes());
        console.print("File saved\n");
    }
}

fn cmd_cat(parts: &[&str], console: &mut Console, shell: &Shell) {
    if parts.len() < 2 {
        console.print("Usage: cat <name>\n");
        return;
    }
    
    if shell.at_root() {
        console.print("No file to read. cd into a drive first.\n");
        return;
    }
    
    let drive_name = shell.current_path[0].clone();
    let full_path = shell.full_path(parts[1]);
    
    let manager = DRIVE_MANAGER.lock();
    if let Some(drive) = manager.get_drive(&drive_name) {
        if let Some(content) = drive.read_file(&full_path) {
            let text = alloc::string::String::from_utf8_lossy(&content);
            console.print(&text);
            console.print("\n");
        } else {
            console.print("File not found\n");
        }
    } else {
        console.print("Drive not found\n");
    }
}

fn cmd_rm(parts: &[&str], console: &mut Console, shell: &Shell) {
    if parts.len() < 2 {
        console.print("Usage: rm <name>\n");
        return;
    }
    
    if shell.at_root() {
        console.print("Cannot delete here. cd into a drive first.\n");
        return;
    }
    
    let drive_name = shell.current_path[0].clone();
    let full_path = shell.full_path(parts[1]);
    
    let mut manager = DRIVE_MANAGER.lock();
    if let Some(drive) = manager.get_drive_mut(&drive_name) {
        if drive.delete_file(&full_path) {
            console.print("Deleted\n");
        } else {
            console.print("Not found\n");
        }
    } else {
        console.print("Drive not found\n");
    }
}

fn cmd_stat(parts: &[&str], console: &mut Console, shell: &Shell) {
    if parts.len() < 2 {
        console.print("Usage: stat <name>\n");
        return;
    }
    
    if shell.at_root() {
        // Show drive info
        let manager = DRIVE_MANAGER.lock();
        if let Some(drive) = manager.get_drive(parts[1]) {
            console.print(&alloc::format!("Drive: {}\n", drive.name));
            console.print(&alloc::format!("Size:  {}MB\n", drive.size_mb));
            let (total, free, entries, used) = drive.get_stats();
            console.print(&alloc::format!("Blocks: {} / {}\n", total - free, total));
            console.print(&alloc::format!("Files:  {} / {}\n", used, entries));
        } else {
            console.print("Drive not found\n");
        }
        return;
    }
    
    let drive_name = shell.current_path[0].clone();
    let full_path = shell.full_path(parts[1]);
    
    let manager = DRIVE_MANAGER.lock();
    if let Some(drive) = manager.get_drive(&drive_name) {
        if let Some((size, is_dir)) = drive.get_file_info(&full_path) {
            console.print(&alloc::format!("Name:  {}\n", parts[1]));
            console.print(&alloc::format!("Path:  {}/{}\n", drive_name, full_path));
            console.print(&alloc::format!("Type:  {}\n", if is_dir { "Directory" } else { "File" }));
            console.print(&alloc::format!("Size:  {} bytes\n", size));
        } else {
            console.print("Not found\n");
        }
    } else {
        console.print("Drive not found\n");
    }
}

fn cmd_df(console: &mut Console, shell: &Shell) {
    let manager = DRIVE_MANAGER.lock();
    
    if shell.at_root() {
        // Show all drives
        console.print("=== ALL DRIVES ===\n");
        for (name, size_mb) in manager.list_drives() {
            if let Some(drive) = manager.get_drive(&name) {
                let (total, free, entries, used) = drive.get_stats();
                let used_blocks = total - free;
                let percent = if total > 0 { (used_blocks * 100) / total } else { 0 };
                console.print(&alloc::format!(
                    "{}: {}MB  [{}/{}] {}%  {} files\n",
                    name, size_mb, used_blocks, total, percent, used
                ));
            }
        }
    } else {
        // Show current drive
        let drive_name = &shell.current_path[0];
        if let Some(drive) = manager.get_drive(drive_name) {
            console.print(&alloc::format!("=== {} ===\n", drive_name.to_uppercase()));
            let (total, free, entries, used) = drive.get_stats();
            console.print(&alloc::format!("Storage: {} / {} blocks\n", total - free, total));
            console.print(&alloc::format!("Files:   {} / {} entries\n", used, entries));
            console.print(&alloc::format!("Size:    {}MB\n", drive.size_mb));
        }
    }
}

fn cmd_disk(parts: &[&str], console: &mut Console, shell: &Shell) {
    if parts.len() < 2 {
        console.print("Usage: disk <list|format>\n");
        return;
    }
    
    match parts[1] {
        "list" => {
            console.print("=== MOUNTED DRIVES ===\n");
            let manager = DRIVE_MANAGER.lock();
            let drives = manager.list_drives();
            if drives.is_empty() {
                console.print("No drives mounted\n");
            } else {
                for (name, size_mb) in drives {
                    console.print(&alloc::format!("  {} - {}MB\n", name, size_mb));
                }
            }
        }
        "format" => {
            if shell.at_root() {
                console.print("cd into a drive first to format it.\n");
                return;
            }
            
            let drive_name = shell.current_path[0].clone();
            console.print(&alloc::format!("Formatting {}...\n", drive_name));
            
            let mut manager = DRIVE_MANAGER.lock();
            if let Some(drive) = manager.get_drive_mut(&drive_name) {
                match drive.format() {
                    Ok(()) => console.print("Done!\n"),
                    Err(e) => console.print(&alloc::format!("Error: {}\n", e)),
                }
            } else {
                console.print("Drive not found\n");
            }
        }
        _ => console.print("Unknown disk command\n"),
    }
}

fn cmd_clear(console: &mut Console) {
    console.clear();
}

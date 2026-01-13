//! Shell command handlers

use crate::drivers;
use crate::drivers::filesystem::FILESYSTEM;
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
        "df" => cmd_df(console),
        "disk" => cmd_disk(&parts, console),
        "clear" => cmd_clear(console),
        _ => console.print("Unknown command. Type 'help'\n"),
    }
}

fn cmd_help(console: &mut Console) {
    console.print("--- NAVIGATION ---\n");
    console.print("  cd <dir>     - Change directory\n");
    console.print("  cd ..        - Go up one level\n");
    console.print("  pwd          - Print current path\n");
    console.print("  ls           - List files\n");
    console.print("\n--- FILE SYSTEM ---\n");
    console.print("  save <name>  - Create/save file\n");
    console.print("  cat <name>   - Show file content\n");
    console.print("  mkdir <name> - Create directory\n");
    console.print("  rm <name>    - Delete file\n");
    console.print("  stat <name>  - Show file info\n");
    console.print("  df           - Show disk usage\n");
    console.print("\n--- DISK ---\n");
    console.print("  disk list    - List drives\n");
    console.print("  disk info    - Disk info\n");
    console.print("  disk format  - Format filesystem\n");
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
        console.print("Directory not found\n");
    }
}

fn cmd_pwd(console: &mut Console, shell: &Shell) {
    console.print(&alloc::format!("/{}\n", shell.get_prompt()));
}

fn cmd_ls(console: &mut Console, shell: &Shell) {
    let fs = FILESYSTEM.lock();
    let files = fs.list_files();
    let current_prefix = if shell.current_path.is_empty() {
        String::new()
    } else {
        alloc::format!("{}/", shell.current_path.join("/"))
    };
    
    let mut found = false;
    
    if !shell.current_path.is_empty() {
        console.print("[Dir] ..\n");
    }
    
    for (name, is_dir) in files {
        if shell.current_path.is_empty() {
            if !name.contains('/') {
                found = true;
                if is_dir {
                    console.print(&alloc::format!("[Dir] {}\n", name));
                } else {
                    console.print(&alloc::format!("[Txt] {}\n", name));
                }
            }
        } else {
            if name.starts_with(&current_prefix) {
                let rel_name = &name[current_prefix.len()..];
                if !rel_name.contains('/') {
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
    
    if !found && shell.current_path.is_empty() {
        console.print("(empty)\n");
    }
}

fn cmd_mkdir(parts: &[&str], console: &mut Console, shell: &Shell) {
    if parts.len() < 2 {
        console.print("Usage: mkdir <name>\n");
    } else {
        let full_path = shell.full_path(parts[1]);
        let mut fs = FILESYSTEM.lock();
        if fs.create_directory(full_path) {
            console.print("Directory created\n");
        } else {
            console.print("Failed: name already exists\n");
        }
    }
}

fn cmd_save(parts: &[&str], console: &mut Console, shell: &Shell) {
    if parts.len() < 2 {
        console.print("Usage: save <name>\n");
        return;
    }
    
    let full_path = shell.full_path(parts[1]);
    let mut fs = FILESYSTEM.lock();
    if fs.create_file(full_path.clone()) {
        console.print("Type content (enter twice to finish):\n");
        drop(fs);
        
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
        
        let mut fs = FILESYSTEM.lock();
        fs.write_file(&full_path, content.as_bytes());
        console.print("File saved\n");
    } else {
        console.print("Failed: file already exists\n");
    }
}

fn cmd_cat(parts: &[&str], console: &mut Console, shell: &Shell) {
    if parts.len() < 2 {
        console.print("Usage: cat <name>\n");
    } else {
        let full_path = shell.full_path(parts[1]);
        let fs = FILESYSTEM.lock();
        if let Some(content) = fs.read_file(&full_path) {
            let text = alloc::string::String::from_utf8_lossy(&content);
            console.print(&text);
            console.print("\n");
        } else {
            console.print("File not found\n");
        }
    }
}

fn cmd_rm(parts: &[&str], console: &mut Console, shell: &Shell) {
    if parts.len() < 2 {
        console.print("Usage: rm <name>\n");
    } else {
        let full_path = shell.full_path(parts[1]);
        let mut fs = FILESYSTEM.lock();
        if fs.delete_file(&full_path) {
            console.print("Deleted\n");
        } else {
            console.print("Not found\n");
        }
    }
}

fn cmd_stat(parts: &[&str], console: &mut Console, shell: &Shell) {
    if parts.len() < 2 {
        console.print("Usage: stat <name>\n");
    } else {
        let full_path = shell.full_path(parts[1]);
        let fs = FILESYSTEM.lock();
        if let Some((size, is_dir)) = fs.get_file_info(&full_path) {
            console.print(&alloc::format!("Name: {}\n", parts[1]));
            console.print(&alloc::format!("Path: {}\n", full_path));
            console.print(&alloc::format!("Type: {}\n", if is_dir { "Directory" } else { "File" }));
            console.print(&alloc::format!("Size: {} bytes\n", size));
        } else {
            console.print("Not found\n");
        }
    }
}

fn cmd_df(console: &mut Console) {
    let fs = FILESYSTEM.lock();
    let (total_blocks, free_blocks, total_entries, used_entries) = fs.get_stats();
    let used_blocks = total_blocks - free_blocks;
    console.print("=== FILESYSTEM USAGE ===\n");
    console.print(&alloc::format!("Storage: {} / {} blocks\n", used_blocks, total_blocks));
    console.print(&alloc::format!("Files:   {} / {} entries\n", used_entries, total_entries));
    console.print(&alloc::format!("Disk:    {}\n", 
        if fs.is_using_disk() { "persistent" } else { "memory only" }));
}

fn cmd_disk(parts: &[&str], console: &mut Console) {
    if parts.len() < 2 {
        console.print("Usage: disk <info|list|format>\n");
        return;
    }
    
    match parts[1] {
        "info" => {
            console.print("=== DISK INFO ===\n");
            console.print(&drivers::ata::get_disk_info());
            console.print("\n");
        }
        "list" => {
            console.print("=== DETECTED DRIVES ===\n");
            let drives = drivers::ata::list_detected_drives();
            if drives.is_empty() {
                console.print("No ATA drives detected\n");
            } else {
                for (name, sectors) in drives {
                    let size_mb = (sectors as u64 * 512) / (1024 * 1024);
                    console.print(&alloc::format!("  {} - {}MB\n", name, size_mb));
                }
            }
        }
        "format" => {
            console.print("Formatting...\n");
            let mut fs = FILESYSTEM.lock();
            match fs.format() {
                Ok(()) => console.print("Done!\n"),
                Err(e) => console.print(&alloc::format!("Error: {}\n", e)),
            }
        }
        _ => console.print("Unknown disk command\n"),
    }
}

fn cmd_clear(console: &mut Console) {
    console.clear();
}

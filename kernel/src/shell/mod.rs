mod console;
mod commands;

pub use console::Console;
pub use commands::handle_command;

use alloc::string::String;
use alloc::vec::Vec;
use crate::drivers::drives::DRIVE_MANAGER;

pub struct Shell {
    pub current_path: Vec<String>,
}

impl Shell {
    pub fn new() -> Self {
        Shell {
            current_path: Vec::new(),
        }
    }

    pub fn current_drive(&self) -> Option<&String> {
        self.current_path.first()
    }

    pub fn at_root(&self) -> bool {
        self.current_path.is_empty()
    }

    pub fn get_prompt(&self) -> String {
        if self.current_path.is_empty() {
            String::from("System")
        } else {
            alloc::format!("System/{}", self.current_path.join("/"))
        }
    }

    pub fn full_path(&self, name: &str) -> String {
        if self.current_path.len() <= 1 {
            String::from(name)
        } else {
            let path_in_drive = self.current_path[1..].join("/");
            alloc::format!("{}/{}", path_in_drive, name)
        }
    }

    pub fn cd(&mut self, path: &str) -> bool {
        if path == ".." {
            self.current_path.pop();
            return true;
        }
        
        if path == "/" || path == "~" {
            self.current_path.clear();
            return true;
        }
        
        if self.at_root() {
            let manager = DRIVE_MANAGER.lock();
            if manager.get_drive(path).is_some() {
                self.current_path.push(String::from(path));
                return true;
            }
            return false;
        }
        
        let drive_name = self.current_path[0].clone();
        let full = self.full_path(path);
        
        let manager = DRIVE_MANAGER.lock();
        if let Some(drive) = manager.get_drive(&drive_name) {
            if let Some((_, is_dir)) = drive.get_file_info(&full) {
                if is_dir {
                    drop(manager);
                    self.current_path.push(String::from(path));
                    return true;
                }
            }
        }
        
        false
    }
}


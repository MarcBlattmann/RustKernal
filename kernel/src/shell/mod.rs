mod console;
mod commands;

pub use console::Console;
pub use commands::handle_command;

use alloc::string::String;
use alloc::vec::Vec;

pub struct Shell {
    pub current_path: Vec<String>,
}

impl Shell {
    pub fn new() -> Self {
        Shell {
            current_path: Vec::new(),
        }
    }

    pub fn get_prompt(&self) -> String {
        if self.current_path.is_empty() {
            String::from("System")
        } else {
            alloc::format!("System/{}", self.current_path.join("/"))
        }
    }

    pub fn full_path(&self, name: &str) -> String {
        if self.current_path.is_empty() {
            String::from(name)
        } else {
            alloc::format!("{}/{}", self.current_path.join("/"), name)
        }
    }

    pub fn cd(&mut self, path: &str) -> bool {
        if path == ".." {
            self.current_path.pop();
            true
        } else if path == "/" || path == "~" {
            self.current_path.clear();
            true
        } else {
            let full = self.full_path(path);
            let fs = crate::drivers::filesystem::FILESYSTEM.lock();
            if let Some((_, is_dir)) = fs.get_file_info(&full) {
                if is_dir {
                    self.current_path.push(String::from(path));
                    return true;
                }
            }
            false
        }
    }
}

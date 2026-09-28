use super::{calculator, editor, fs_commands, theme, Shell};
use crate::drivers::display::Canvas;
use crate::drivers::{pci, power, rtc, timer};
use crate::memory;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

pub struct Command {
    pub name: &'static str,
    pub usage: &'static str,
    pub description: &'static str,
    /// Heading `help` lists the command under.
    pub group: Group,
    pub run: fn(&mut Shell, &[&str]),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Shell,
    System,
    Files,
    Storage,
}

const GROUPS: [(Group, &str); 4] =
    [(Group::Shell, "Shell"), (Group::Files, "Files"), (Group::Storage, "Storage"), (Group::System, "System")];

pub const COMMANDS: &[Command] = &[
    Command { name: "help", usage: "help [command]", description: "List commands, or explain one command", group: Group::Shell, run: help },
    Command { name: "clear", usage: "clear", description: "Clear the screen (also Ctrl+L)", group: Group::Shell, run: clear },
    Command { name: "echo", usage: "echo <text>", description: "Print text (echo hi > file.txt writes a file)", group: Group::Shell, run: echo },
    Command { name: "history", usage: "history", description: "Show previously entered commands", group: Group::Shell, run: history },
    Command { name: "color", usage: "color [name]", description: "Change the text color, or list colors", group: Group::Shell, run: color },
    Command { name: "calc", usage: "calc <expression>", description: "Integer calculator, e.g. calc (3 + 4) * 0x10", group: Group::Shell, run: calc },
    Command { name: "demo", usage: "demo", description: "60 fps graphics demo (any key exits)", group: Group::Shell, run: demo },
    Command { name: "ls", usage: "ls [-l] [-a] [path...]", description: "List a directory (-l: size and date, -a: hidden files too)", group: Group::Files, run: fs_commands::ls },
    Command { name: "cd", usage: "cd [path | -]", description: "Change the working directory (no path: /, -: previous)", group: Group::Files, run: fs_commands::cd },
    Command { name: "pwd", usage: "pwd", description: "Print the working directory", group: Group::Files, run: fs_commands::pwd },
    Command { name: "tree", usage: "tree [-a] [path]", description: "Show a directory and everything below it", group: Group::Files, run: fs_commands::tree },
    Command { name: "cat", usage: "cat <file>...", description: "Print files", group: Group::Files, run: fs_commands::cat },
    Command { name: "head", usage: "head [-n lines] <file>", description: "Print the first lines of a file (10 by default)", group: Group::Files, run: fs_commands::head },
    Command { name: "tail", usage: "tail [-n lines] <file>", description: "Print the last lines of a file (10 by default)", group: Group::Files, run: fs_commands::tail },
    Command { name: "hexdump", usage: "hexdump <file>", description: "Show a file as hex bytes and ASCII (first 64 KiB)", group: Group::Files, run: fs_commands::hexdump },
    Command { name: "stat", usage: "stat <path>...", description: "Size, dates and attributes of files or directories", group: Group::Files, run: fs_commands::stat },
    Command { name: "edit", usage: "edit <file>", description: "Edit a text file full screen (Ctrl+S saves, Ctrl+Q quits)", group: Group::Files, run: editor::edit },
    Command { name: "write", usage: "write <file> [text...]", description: "Replace a file's contents with a line of text", group: Group::Files, run: fs_commands::write },
    Command { name: "touch", usage: "touch <file>...", description: "Create empty files, or update their modified time", group: Group::Files, run: fs_commands::touch },
    Command { name: "mkdir", usage: "mkdir [-p] <dir>...", description: "Create directories (-p: with missing parents)", group: Group::Files, run: fs_commands::mkdir },
    Command { name: "rmdir", usage: "rmdir <dir>...", description: "Remove empty directories", group: Group::Files, run: fs_commands::rmdir },
    Command { name: "rm", usage: "rm [-r] [-f] <path>...", description: "Remove files (-r: directories too, -f: ignore missing)", group: Group::Files, run: fs_commands::rm },
    Command { name: "mv", usage: "mv <from>... <to>", description: "Move or rename files and directories", group: Group::Files, run: fs_commands::mv },
    Command { name: "cp", usage: "cp [-r] <from>... <to>", description: "Copy files (-r: directories too)", group: Group::Files, run: fs_commands::cp },
    Command { name: "df", usage: "df", description: "Size and free space of the mounted volume", group: Group::Storage, run: fs_commands::df },
    Command { name: "lsblk", usage: "lsblk", description: "List drives, partitions and file systems", group: Group::Storage, run: fs_commands::lsblk },
    Command { name: "mount", usage: "mount [drive | partition]", description: "Show the mounted volume, or mount one on / (e.g. mount hd1)", group: Group::Storage, run: fs_commands::mount },
    Command { name: "umount", usage: "umount [/ | drive]", description: "Write everything and unmount the volume", group: Group::Storage, run: fs_commands::umount },
    Command { name: "sync", usage: "sync", description: "Write all cached changes to the disk now", group: Group::Storage, run: fs_commands::sync },
    Command { name: "fsck", usage: "fsck", description: "Check the mounted volume for errors (read-only)", group: Group::Storage, run: fs_commands::fsck },
    Command { name: "format", usage: "format <drive> [label]", description: "Erase a drive and create a FAT32 file system on it", group: Group::Storage, run: fs_commands::format_drive },
    Command { name: "about", usage: "about", description: "Summary of the system", group: Group::System, run: about },
    Command { name: "uptime", usage: "uptime", description: "Time since boot", group: Group::System, run: uptime },
    Command { name: "date", usage: "date", description: "Current date and time from the hardware clock (UTC)", group: Group::System, run: date },
    Command { name: "mem", usage: "mem", description: "Memory and kernel heap usage", group: Group::System, run: mem },
    Command { name: "cpu", usage: "cpu", description: "Processor model and features", group: Group::System, run: cpu },
    Command { name: "lspci", usage: "lspci", description: "List PCI devices", group: Group::System, run: lspci },
    Command { name: "display", usage: "display", description: "Screen resolution and display driver", group: Group::System, run: display },
    Command { name: "reboot", usage: "reboot", description: "Save everything and restart the computer", group: Group::System, run: reboot },
    Command { name: "shutdown", usage: "shutdown", description: "Save everything and power off (virtual machines only for now)", group: Group::System, run: shutdown },
];

pub fn find(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|command| command.name == name)
}

/// The command closest to a mistyped name, if any is close enough.
pub fn suggest(name: &str) -> Option<&'static str> {
    COMMANDS
        .iter()
        .map(|command| (edit_distance(name, command.name), command.name))
        .filter(|(distance, _)| *distance <= 2)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, name)| name)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, a_char) in a.chars().enumerate() {
        let mut current = Vec::with_capacity(b.len() + 1);
        current.push(i + 1);
        for (j, b_char) in b.iter().enumerate() {
            let substitution = previous[j] + (a_char != *b_char) as usize;
            current.push(substitution.min(previous[j + 1] + 1).min(current[j] + 1));
        }
        previous = current;
    }
    previous[b.len()]
}

pub fn print_banner(shell: &mut Shell) {
    shell.print_colored("RustKernal ", theme::ACCENT);
    shell.println_colored(env!("CARGO_PKG_VERSION"), theme::MUTED);
    let summary = format!(
        "{}x{} {} | {} MiB RAM | {}",
        shell.terminal.display().width(),
        shell.terminal.display().height(),
        shell.terminal.display().output_name(),
        memory::paging::usable_memory_bytes() / MIB,
        crate::cpu::info::read().brand
    );
    shell.println_colored(&summary, theme::MUTED);
    fs_commands::print_storage_status(shell);
    shell.print("Type ");
    shell.print_colored("help", theme::GREEN);
    shell.println(" to list commands.\n");
}

const MIB: u64 = 1024 * 1024;

fn format_bytes(bytes: u64) -> String {
    if bytes >= MIB {
        format!("{}.{} MiB", bytes / MIB, bytes % MIB * 10 / MIB)
    } else {
        format!("{} KiB", bytes / 1024)
    }
}

fn format_duration(nanoseconds: u64) -> String {
    let seconds = nanoseconds / 1_000_000_000;
    format!("{}h {:02}m {:02}s", seconds / 3600, seconds / 60 % 60, seconds % 60)
}

fn usage_bar(used: u64, total: u64, width: usize) -> String {
    let filled = (used * width as u64 / total.max(1)) as usize;
    let mut bar = String::from("[");
    (0..width).for_each(|i| bar.push(if i < filled { '#' } else { '.' }));
    bar.push(']');
    bar
}

fn help(shell: &mut Shell, arguments: &[&str]) {
    if let Some(&name) = arguments.first() {
        match find(name) {
            Some(command) => {
                shell.print_colored("usage: ", theme::MUTED);
                shell.println_colored(command.usage, theme::GREEN);
                shell.println(command.description);
            }
            None => shell.println_colored(&format!("no command named '{}'", name), theme::RED),
        }
        return;
    }

    for (index, (group, title)) in GROUPS.iter().enumerate() {
        if index > 0 {
            shell.println("");
        }
        shell.println_colored(title, theme::ACCENT);
        for command in COMMANDS.iter().filter(|command| command.group == *group) {
            shell.print_colored(&format!("  {:<12}", command.name), theme::GREEN);
            shell.println(command.description);
        }
    }
    shell.println_colored("\nSyntax", theme::ACCENT);
    let syntax = [
        ("cmd > file", "write the output of a command to a file (>> appends)"),
        ("\"my file.txt\"", "quotes keep spaces in one argument"),
        ("~  .  ..", "the root, this and the parent directory in paths"),
    ];
    for (example, description) in syntax {
        shell.print_colored(&format!("  {:<20}", example), theme::YELLOW);
        shell.println(description);
    }
    shell.println_colored("\nKeys", theme::ACCENT);
    let keys = [
        ("Up / Down", "browse command history"),
        ("Tab", "complete a command, file or directory name"),
        ("Left / Right", "move the cursor (Home / End jump)"),
        ("PageUp / PageDown", "scroll the screen (Shift+Up / Down: one line)"),
        ("Ctrl+C", "cancel the current line"),
        ("Ctrl+U / Esc", "erase the current line"),
        ("Ctrl+L", "clear the screen"),
    ];
    for (key, description) in keys {
        shell.print_colored(&format!("  {:<20}", key), theme::YELLOW);
        shell.println(description);
    }
}

fn clear(shell: &mut Shell, _: &[&str]) {
    shell.terminal.clear();
}

fn echo(shell: &mut Shell, arguments: &[&str]) {
    shell.println(&arguments.join(" "));
}

fn history(shell: &mut Shell, _: &[&str]) {
    let entries = shell.history.clone();
    for (index, entry) in entries.iter().enumerate() {
        shell.print_colored(&format!("{:>4}  ", index + 1), theme::MUTED);
        shell.println(entry);
    }
}

fn about(shell: &mut Shell, _: &[&str]) {
    let cpu = crate::cpu::info::read();
    let (heap_used, heap_total) = memory::heap_usage();
    let display = shell.terminal.display();
    let display_text = format!("{}x{}, {}", display.width(), display.height(), display.output_name());

    shell.print_colored("RustKernal ", theme::ACCENT);
    shell.println_colored(env!("CARGO_PKG_VERSION"), theme::MUTED);
    shell.print_field("kernel", "x86_64, written in Rust (no_std)");
    shell.print_field("cpu", &cpu.brand);
    shell.print_field("memory", &format!("{} RAM", format_bytes(memory::paging::usable_memory_bytes())));
    shell.print_field("heap", &format!("{} of {} used", format_bytes(heap_used as u64), format_bytes(heap_total as u64)));
    shell.print_field("display", &display_text);
    let storage = match crate::fs::volume_info() {
        Some(info) => format!(
            "{} on / ({}), {} of {} used",
            if info.label.is_empty() { "unlabelled volume" } else { info.label.as_str() },
            info.source,
            fs_commands::human_size(info.used_bytes()),
            fs_commands::human_size(info.total_bytes)
        ),
        None => String::from("no file system mounted"),
    };
    shell.print_field("storage", &storage);
    shell.print_field("uptime", &format_duration(timer::uptime_nanoseconds()));
}

fn uptime(shell: &mut Shell, _: &[&str]) {
    shell.println(&format!("up {}", format_duration(timer::uptime_nanoseconds())));
}

fn date(shell: &mut Shell, _: &[&str]) {
    let now = rtc::now();
    shell.println(&format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        now.year, now.month, now.day, now.hour, now.minute, now.second
    ));
}

fn mem(shell: &mut Shell, _: &[&str]) {
    let (used, total) = memory::heap_usage();
    let (used, total) = (used as u64, total as u64);
    shell.print_field("RAM", &format_bytes(memory::paging::usable_memory_bytes()));
    shell.print_field("heap used", &format!("{} {}", format_bytes(used), usage_bar(used, total, 30)));
    shell.print_field("heap free", &format_bytes(total - used));
    shell.print_field("heap size", &format_bytes(total));
}

fn cpu(shell: &mut Shell, _: &[&str]) {
    let cpu = crate::cpu::info::read();
    shell.print_field("model", &cpu.brand);
    shell.print_field("vendor", &cpu.vendor);
    shell.print_field("family", &format!("{} (model {})", cpu.family, cpu.model));
    shell.print_field("threads", &format!("{}", cpu.logical_processors));
    shell.print_field("features", &cpu.features.join(" "));
}

fn lspci(shell: &mut Shell, _: &[&str]) {
    for device in pci::devices() {
        shell.print_colored(&format!("{:02x}:{:02x}.{}  ", device.bus, device.slot, device.function), theme::MUTED);
        shell.print_colored(&format!("{:04x}:{:04x}  ", device.vendor_id, device.device_id), theme::YELLOW);
        shell.print(&format!("{:<26}", device.class_name()));
        shell.println_colored(pci::vendor_name(device.vendor_id), theme::MUTED);
    }
}

fn display(shell: &mut Shell, _: &[&str]) {
    let terminal = &shell.terminal;
    let resolution = format!("{}x{}", terminal.display().width(), terminal.display().height());
    let output = terminal.display().output_name();
    let text = format!("{} columns x {} rows, font scale {}", terminal.columns(), terminal.rows(), terminal.scale());
    shell.print_field("resolution", &resolution);
    shell.print_field("driver", output);
    shell.print_field("terminal", &text);
}

fn color(shell: &mut Shell, arguments: &[&str]) {
    let Some(&name) = arguments.first() else {
        for (name, color) in theme::NAMED_COLORS {
            shell.print_colored(&format!("  {:<10}", name), color);
        }
        shell.println("");
        return;
    };

    match theme::NAMED_COLORS.iter().find(|(color_name, _)| *color_name == name) {
        Some((_, color)) => shell.terminal.set_color(*color),
        None => shell.println_colored(&format!("unknown color '{}' (type 'color' to list them)", name), theme::RED),
    }
}

fn calc(shell: &mut Shell, arguments: &[&str]) {
    if arguments.is_empty() {
        shell.println_colored("usage: calc <expression>", theme::MUTED);
        return;
    }
    match calculator::evaluate(&arguments.join(" ")) {
        Ok(value) => {
            shell.print_colored("= ", theme::MUTED);
            shell.print_colored(&format!("{}", value), theme::GREEN);
            shell.println_colored(&format!("   (0x{:X})", value), theme::MUTED);
        }
        Err(error) => shell.println_colored(&format!("error: {}", error), theme::RED),
    }
}

fn demo(shell: &mut Shell, _: &[&str]) {
    crate::demo::run(shell.terminal.display_mut());
    shell.terminal.redraw();
}

/// Writes every cached change to disk before the power goes away. Returns false (after printing
/// why) when that failed and the user chose not to continue.
fn save_before_power_off(shell: &mut Shell, action: &str) -> bool {
    let Err(error) = crate::fs::prepare_power_off() else { return true };
    shell.print_error(&format!("{}: saving files failed: {}", action, error));
    let answer = shell.read_input(&format!("Unsaved changes may be lost. Type yes to {} anyway: ", action));
    answer.is_some_and(|answer| answer.trim() == "yes")
}

fn reboot(shell: &mut Shell, _: &[&str]) {
    if !save_before_power_off(shell, "reboot") {
        return;
    }
    shell.println_colored("Rebooting...", theme::YELLOW);
    shell.terminal.flush();
    power::reboot();
}

fn shutdown(shell: &mut Shell, _: &[&str]) {
    if !save_before_power_off(shell, "shutdown") {
        return;
    }
    shell.println_colored("Shutting down...", theme::YELLOW);
    shell.terminal.flush();
    power::shutdown();
    shell.println_colored("This machine did not power off; shutdown needs ACPI support on real hardware.", theme::RED);
}

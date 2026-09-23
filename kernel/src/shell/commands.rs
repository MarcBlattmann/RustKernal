use super::{calculator, theme, Shell};
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
    pub run: fn(&mut Shell, &[&str]),
}

pub const COMMANDS: &[Command] = &[
    Command { name: "help", usage: "help [command]", description: "List commands, or explain one command", run: help },
    Command { name: "clear", usage: "clear", description: "Clear the screen (also Ctrl+L)", run: clear },
    Command { name: "echo", usage: "echo <text>", description: "Print text", run: echo },
    Command { name: "history", usage: "history", description: "Show previously entered commands", run: history },
    Command { name: "about", usage: "about", description: "Summary of the system", run: about },
    Command { name: "uptime", usage: "uptime", description: "Time since boot", run: uptime },
    Command { name: "date", usage: "date", description: "Current date and time from the hardware clock (UTC)", run: date },
    Command { name: "mem", usage: "mem", description: "Memory and kernel heap usage", run: mem },
    Command { name: "cpu", usage: "cpu", description: "Processor model and features", run: cpu },
    Command { name: "lspci", usage: "lspci", description: "List PCI devices", run: lspci },
    Command { name: "display", usage: "display", description: "Screen resolution and display driver", run: display },
    Command { name: "color", usage: "color [name]", description: "Change the text color, or list colors", run: color },
    Command { name: "calc", usage: "calc <expression>", description: "Integer calculator, e.g. calc (3 + 4) * 0x10", run: calc },
    Command { name: "demo", usage: "demo", description: "60 fps graphics demo (any key exits)", run: demo },
    Command { name: "reboot", usage: "reboot", description: "Restart the computer", run: reboot },
    Command { name: "shutdown", usage: "shutdown", description: "Power off (virtual machines only for now)", run: shutdown },
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

    shell.println_colored("Commands", theme::ACCENT);
    for command in COMMANDS {
        shell.print_colored(&format!("  {:<12}", command.name), theme::GREEN);
        shell.println(command.description);
    }
    shell.println_colored("\nKeys", theme::ACCENT);
    let keys = [
        ("Up / Down", "browse command history"),
        ("Tab", "complete a command name"),
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

fn reboot(shell: &mut Shell, _: &[&str]) {
    shell.println_colored("Rebooting...", theme::YELLOW);
    shell.terminal.flush();
    power::reboot();
}

fn shutdown(shell: &mut Shell, _: &[&str]) {
    shell.println_colored("Shutting down...", theme::YELLOW);
    shell.terminal.flush();
    power::shutdown();
    shell.println_colored("This machine did not power off; shutdown needs ACPI support on real hardware.", theme::RED);
}

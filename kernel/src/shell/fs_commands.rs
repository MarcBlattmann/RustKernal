//! File and storage commands: ls, cd, cat, cp, mv, rm, mkdir, df, lsblk, mount, format, ...
//!
//! Errors are printed in red as `<command>: <path>: <error>` and never go into a redirect.

use super::{Shell, theme};
use crate::fs::{self, FsError, Status, Volume, path};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use fat32::{Metadata, Timestamp};

/// `cat` output on screen stops after this much; redirected output is not limited here.
const CAT_SCREEN_LIMIT: u64 = 256 * 1024;
/// Piece size when `cat` copies a file into a redirect.
const CAT_CHUNK: u64 = 64 * 1024;
const HEXDUMP_LIMIT: u64 = 64 * 1024;
/// How much of a file `head` and `tail` look at.
const HEAD_TAIL_WINDOW: u64 = 1024 * 1024;
const TREE_MAX_ENTRIES: usize = 5000;
const TREE_MAX_DEPTH: usize = 64;
const COPY_MAX_DEPTH: usize = 64;

/// An error from a command's own checks, or from the file system.
enum OpError {
    Fs(FsError),
    Message(&'static str),
}

impl From<fat32::Error> for OpError {
    fn from(error: fat32::Error) -> Self {
        OpError::Fs(FsError::Fs(error))
    }
}

impl From<FsError> for OpError {
    fn from(error: FsError) -> Self {
        OpError::Fs(error)
    }
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpError::Fs(error) => write!(f, "{}", error),
            OpError::Message(message) => f.write_str(message),
        }
    }
}

/// Runs `f` on the mounted volume.
fn on_volume<T>(f: impl FnOnce(&mut Volume) -> Result<T, OpError>) -> Result<T, OpError> {
    fs::lock_volume(f)?
}

fn is_not_found(error: &fat32::Error) -> bool {
    matches!(error, fat32::Error::NotFound)
}

fn fail(shell: &mut Shell, command: &str, subject: &str, error: impl fmt::Display) {
    shell.print_error(&format!("{}: {}: {}", command, subject, error));
}

fn usage(shell: &mut Shell, command: &str) {
    let usage = super::commands::find(command).map(|command| command.usage).unwrap_or(command);
    shell.print_error(&format!("usage: {}", usage));
}

/// Separates `-x` options (anywhere before `--`) from operands. Prints an error and returns
/// `None` for an option not in `allowed`.
fn options<'a>(shell: &mut Shell, command: &str, arguments: &[&'a str], allowed: &str) -> Option<(Vec<char>, Vec<&'a str>)> {
    let mut flags = Vec::new();
    let mut operands = Vec::new();
    let mut only_operands = false;
    for &argument in arguments {
        if only_operands || argument == "-" || !argument.starts_with('-') {
            operands.push(argument);
        } else if argument == "--" {
            only_operands = true;
        } else {
            for flag in argument[1..].chars() {
                if !allowed.contains(flag) {
                    shell.print_error(&format!("{}: unknown option -{}", command, flag));
                    usage(shell, command);
                    return None;
                }
                flags.push(flag);
            }
        }
    }
    Some((flags, operands))
}

fn resolve(shell: &Shell, argument: &str) -> String {
    fs::normalize(&shell.cwd, argument)
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{} B", bytes);
    }
    let mut unit = 0;
    let mut scaled = bytes as u128 * 10 / 1024;
    while scaled >= 10240 && unit + 1 < UNITS.len() {
        scaled /= 1024;
        unit += 1;
    }
    format!("{}.{} {}", scaled / 10, scaled % 10, UNITS[unit])
}

fn format_timestamp(timestamp: &Timestamp, seconds: bool) -> String {
    if timestamp.year == 0 {
        return String::from("-");
    }
    let mut text = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        timestamp.year, timestamp.month, timestamp.day, timestamp.hour, timestamp.minute
    );
    if seconds {
        text.push_str(&format!(":{:02}", timestamp.second));
    }
    text
}

/// Directories first, then by name ignoring case.
fn sort_entries(entries: &mut [Metadata]) {
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
}

/// `path` with every component spelled the way it is stored on disk.
fn canonical_case(volume: &mut Volume, path: &str) -> fat32::Result<String> {
    let mut canonical = String::from("/");
    for component in path.split('/').filter(|component| !component.is_empty()) {
        let metadata = volume.metadata(&path::join(&canonical, component))?;
        canonical = path::join(&canonical, &metadata.name);
    }
    Ok(canonical)
}

// ----- startup and completion ------------------------------------------------------------------

/// One line about the storage state, printed when the shell starts.
pub fn print_storage_status(shell: &mut Shell) {
    shell.print_colored("Storage: ", theme::ACCENT);
    match fs::status() {
        Status::Mounted(info) => {
            shell.println(&format!(
                "/ mounted from {} ({}, {}, {} free)",
                info.source,
                if info.label.is_empty() { "no label" } else { info.label.as_str() },
                human_size(info.total_bytes),
                human_size(info.free_bytes)
            ));
            if !info.was_clean {
                shell.println_colored(
                    &format!("{} was not cleanly unmounted; free space was recounted. Run 'fsck' to check it.", info.source),
                    theme::YELLOW,
                );
            }
        }
        Status::MountFailed { source, error } => {
            shell.println_colored(&format!("cannot mount {}: {}", source, error), theme::RED);
        }
        Status::NoFileSystem(drive) => match drive.check_formattable() {
            Ok(()) => shell.println_colored(
                &format!("{} has no file system: run 'format {}' to create one", drive.name, drive.name),
                theme::YELLOW,
            ),
            Err(error) => shell.println_colored(&format!("{} has no FAT32 file system; {}", drive.name, error), theme::YELLOW),
        },
        Status::NoDataDrive => {
            shell.println_colored("no data drive found; files cannot be saved (attach a disk as hd1)", theme::MUTED);
        }
    }
}

/// Drive names for Tab completion: drives and partitions for `mount`, whole drives for `format`.
pub fn drive_names(with_partitions: bool) -> Vec<String> {
    let mut names = Vec::new();
    for drive in fs::drives().into_iter().filter(|drive| !drive.is_boot) {
        names.push(String::from(drive.name));
        if with_partitions {
            for partition in &drive.partitions {
                names.push(format!("{}p{}", drive.name, partition.number));
            }
        }
    }
    names
}

// ----- navigation ------------------------------------------------------------------------------

pub fn pwd(shell: &mut Shell, _: &[&str]) {
    let cwd = shell.cwd.clone();
    shell.println(&cwd);
}

pub fn cd(shell: &mut Shell, arguments: &[&str]) {
    let argument = match arguments {
        [] => "/",
        [argument] => argument,
        _ => return usage(shell, "cd"),
    };
    let target = if argument == "-" { shell.previous_cwd.clone() } else { resolve(shell, argument) };
    if target == "/" {
        shell.previous_cwd = core::mem::replace(&mut shell.cwd, target);
        return;
    }
    let result = on_volume(|volume| {
        if !volume.metadata(&target)?.is_dir {
            return Err(fat32::Error::NotADirectory.into());
        }
        Ok(canonical_case(volume, &target)?)
    });
    match result {
        Ok(canonical) => {
            shell.previous_cwd = core::mem::replace(&mut shell.cwd, canonical);
            if argument == "-" {
                let cwd = shell.cwd.clone();
                shell.println(&cwd);
            }
        }
        Err(error) => fail(shell, "cd", argument, error),
    }
}

pub fn ls(shell: &mut Shell, arguments: &[&str]) {
    let Some((flags, operands)) = options(shell, "ls", arguments, "la") else { return };
    let long = flags.contains(&'l');
    let all = flags.contains(&'a');
    let targets = if operands.is_empty() { vec!["."] } else { operands };

    for (index, &target) in targets.iter().enumerate() {
        let path = resolve(shell, target);
        let listing = on_volume(|volume| {
            let metadata = volume.metadata(&path)?;
            if metadata.is_dir {
                Ok((true, volume.read_dir(&path)?))
            } else {
                Ok((false, vec![Metadata { name: String::from(target), ..metadata }]))
            }
        });
        let (is_dir, mut entries) = match listing {
            Ok(listing) => listing,
            Err(error) => {
                fail(shell, "ls", target, error);
                continue;
            }
        };
        if targets.len() > 1 {
            if index > 0 {
                shell.println("");
            }
            shell.println_colored(&format!("{}:", target), theme::MUTED);
        }
        if is_dir && !all {
            entries.retain(|entry| !entry.hidden && !entry.system);
        }
        sort_entries(&mut entries);
        if long {
            print_long_listing(shell, &entries);
        } else {
            print_short_listing(shell, &entries);
        }
    }
}

fn entry_color(entry: &Metadata) -> u32 {
    if entry.is_dir { theme::ACCENT } else { theme::FOREGROUND }
}

fn display_name(entry: &Metadata) -> String {
    if entry.is_dir { format!("{}/", entry.name) } else { entry.name.clone() }
}

fn print_short_listing(shell: &mut Shell, entries: &[Metadata]) {
    if shell.is_capturing() {
        for entry in entries {
            shell.println(&display_name(entry));
        }
        return;
    }
    let width = entries.iter().map(|entry| display_name(entry).chars().count()).max().unwrap_or(0) + 2;
    let per_row = (shell.terminal.columns() / width.max(1)).max(1);
    for (index, entry) in entries.iter().enumerate() {
        let last_in_row = (index + 1) % per_row == 0 || index + 1 == entries.len();
        let name = display_name(entry);
        let text = if last_in_row { name } else { format!("{:<width$}", name, width = width) };
        shell.print_colored(&text, entry_color(entry));
        if last_in_row {
            shell.println("");
        }
    }
}

fn print_long_listing(shell: &mut Shell, entries: &[Metadata]) {
    let (mut directories, mut files, mut bytes) = (0, 0, 0u64);
    for entry in entries {
        let attributes = format!(
            "{}{}{}",
            if entry.read_only { 'r' } else { '-' },
            if entry.hidden { 'h' } else { '-' },
            if entry.system { 's' } else { '-' }
        );
        let size = if entry.is_dir { String::from("-") } else { entry.size.to_string() };
        shell.print_colored(&format!("{:<5}{}  ", if entry.is_dir { "dir" } else { "file" }, attributes), theme::MUTED);
        shell.print(&format!("{:>11}  ", size));
        shell.print_colored(&format!("{:<16}  ", format_timestamp(&entry.modified, false)), theme::MUTED);
        shell.println_colored(&display_name(entry), entry_color(entry));
        if entry.is_dir {
            directories += 1;
        } else {
            files += 1;
            bytes += entry.size;
        }
    }
    shell.println_colored(
        &format!(
            "{} director{}, {} file{}, {}",
            directories,
            if directories == 1 { "y" } else { "ies" },
            files,
            if files == 1 { "" } else { "s" },
            human_size(bytes)
        ),
        theme::MUTED,
    );
}

pub fn tree(shell: &mut Shell, arguments: &[&str]) {
    let Some((flags, operands)) = options(shell, "tree", arguments, "a") else { return };
    let all = flags.contains(&'a');
    let target = match operands.as_slice() {
        [] => ".",
        [target] => target,
        _ => return usage(shell, "tree"),
    };
    let root = resolve(shell, target);
    let mut lines: Vec<(String, Metadata)> = Vec::new();
    let result = on_volume(|volume| {
        if !volume.metadata(&root)?.is_dir {
            return Err(fat32::Error::NotADirectory.into());
        }
        walk_tree(volume, &root, "", all, 0, &mut lines)?;
        Ok(())
    });
    if let Err(error) = result {
        return fail(shell, "tree", target, error);
    }

    let heading = if operands.is_empty() { shell.cwd.clone() } else { String::from(target) };
    shell.println_colored(&heading, theme::ACCENT);
    let (mut directories, mut files) = (0, 0);
    for (prefix, entry) in &lines {
        shell.print_colored(prefix, theme::MUTED);
        shell.println_colored(&display_name(entry), entry_color(entry));
        if entry.is_dir {
            directories += 1;
        } else {
            files += 1;
        }
    }
    if lines.len() >= TREE_MAX_ENTRIES {
        shell.print_warning(&format!("tree: stopped after {} entries", TREE_MAX_ENTRIES));
    }
    shell.println_colored(
        &format!("\n{} director{}, {} file{}", directories, if directories == 1 { "y" } else { "ies" }, files, if files == 1 { "" } else { "s" }),
        theme::MUTED,
    );
}

fn walk_tree(volume: &mut Volume, directory: &str, prefix: &str, all: bool, depth: usize, lines: &mut Vec<(String, Metadata)>) -> fat32::Result<()> {
    let mut entries = volume.read_dir(directory)?;
    if !all {
        entries.retain(|entry| !entry.hidden && !entry.system);
    }
    sort_entries(&mut entries);
    let count = entries.len();
    for (index, entry) in entries.into_iter().enumerate() {
        if lines.len() >= TREE_MAX_ENTRIES {
            return Ok(());
        }
        let last = index + 1 == count;
        let child = path::join(directory, &entry.name);
        let is_dir = entry.is_dir;
        lines.push((format!("{}{}", prefix, if last { "`-- " } else { "|-- " }), entry));
        if is_dir && depth + 1 < TREE_MAX_DEPTH {
            let child_prefix = format!("{}{}", prefix, if last { "    " } else { "|   " });
            walk_tree(volume, &child, &child_prefix, all, depth + 1, lines)?;
        }
    }
    Ok(())
}

// ----- creating and removing -------------------------------------------------------------------

pub fn mkdir(shell: &mut Shell, arguments: &[&str]) {
    let Some((flags, operands)) = options(shell, "mkdir", arguments, "p") else { return };
    if operands.is_empty() {
        return usage(shell, "mkdir");
    }
    let parents = flags.contains(&'p');
    for argument in operands {
        let path = resolve(shell, argument);
        let result = fs::with_volume(|volume| if parents { volume.create_dir_all(&path) } else { volume.create_dir(&path) });
        if let Err(error) = result {
            fail(shell, "mkdir", argument, error);
        }
    }
}

pub fn rmdir(shell: &mut Shell, arguments: &[&str]) {
    if arguments.is_empty() {
        return usage(shell, "rmdir");
    }
    for &argument in arguments {
        let path = resolve(shell, argument);
        if let Err(error) = fs::with_volume(|volume| volume.remove_dir(&path)) {
            fail(shell, "rmdir", argument, error);
        }
    }
}

pub fn touch(shell: &mut Shell, arguments: &[&str]) {
    if arguments.is_empty() {
        return usage(shell, "touch");
    }
    for &argument in arguments {
        let path = resolve(shell, argument);
        if let Err(error) = fs::with_volume(|volume| volume.touch(&path)) {
            fail(shell, "touch", argument, error);
        }
    }
}

pub fn write(shell: &mut Shell, arguments: &[&str]) {
    let Some((&file, words)) = arguments.split_first() else { return usage(shell, "write") };
    let mut text = words.join(" ");
    if !words.is_empty() {
        text.push('\n');
    }
    let path = resolve(shell, file);
    if let Err(error) = fs::with_volume(|volume| volume.write_file(&path, text.as_bytes())) {
        fail(shell, "write", file, error);
    }
}

pub fn rm(shell: &mut Shell, arguments: &[&str]) {
    let Some((flags, operands)) = options(shell, "rm", arguments, "rf") else { return };
    if operands.is_empty() {
        return usage(shell, "rm");
    }
    let recursive = flags.contains(&'r');
    let force = flags.contains(&'f');
    for argument in operands {
        let path = resolve(shell, argument);
        if path == "/" {
            fail(shell, "rm", argument, "refusing to remove the root directory");
            continue;
        }
        let result = on_volume(|volume| {
            let metadata = match volume.metadata(&path) {
                Err(error) if force && is_not_found(&error) => return Ok(()),
                other => other?,
            };
            if metadata.is_dir {
                if !recursive {
                    return Err(OpError::Message("is a directory (use rm -r)"));
                }
                volume.remove_dir_all(&path)?;
            } else {
                volume.remove_file(&path)?;
            }
            Ok(())
        });
        if let Err(error) = result {
            fail(shell, "rm", argument, error);
        }
    }
}

// ----- moving and copying ----------------------------------------------------------------------

/// Where `source` ends up for `mv`/`cp` with destination `destination`: inside it when it is an
/// existing directory.
fn destination_for(volume: &mut Volume, source: &str, destination: &str) -> fat32::Result<String> {
    if path::same_path(source, destination) {
        return Ok(String::from(destination));
    }
    match volume.metadata(destination) {
        Ok(metadata) if metadata.is_dir => Ok(path::join(destination, path::file_name(source))),
        Ok(_) => Ok(String::from(destination)),
        Err(error) if is_not_found(&error) => Ok(String::from(destination)),
        Err(error) => Err(error),
    }
}

/// Splits `mv`/`cp` operands into sources and the destination; several sources need an
/// existing directory as destination.
fn sources_and_destination<'a>(shell: &mut Shell, command: &str, operands: &[&'a str]) -> Option<(Vec<&'a str>, &'a str)> {
    let [sources @ .., destination] = operands else { return None };
    if sources.is_empty() {
        usage(shell, command);
        return None;
    }
    if sources.len() > 1 {
        let path = resolve(shell, destination);
        match fs::with_volume(|volume| volume.metadata(&path)) {
            Ok(metadata) if metadata.is_dir => {}
            Ok(_) => {
                fail(shell, command, destination, "not a directory");
                return None;
            }
            Err(error) => {
                fail(shell, command, destination, error);
                return None;
            }
        }
    }
    Some((sources.to_vec(), destination))
}

pub fn mv(shell: &mut Shell, arguments: &[&str]) {
    if arguments.len() < 2 {
        return usage(shell, "mv");
    }
    let Some((sources, destination)) = sources_and_destination(shell, "mv", arguments) else { return };
    let destination = resolve(shell, destination);
    for source_argument in sources {
        let source = resolve(shell, source_argument);
        let result = on_volume(|volume| {
            if source == "/" {
                return Err(fat32::Error::RootDirectory.into());
            }
            let source_metadata = volume.metadata(&source)?;
            let target = destination_for(volume, &source, &destination)?;
            if target == source {
                return Err(OpError::Message("source and destination are the same"));
            }
            if !path::same_path(&source, &target) {
                match volume.metadata(&target) {
                    Ok(existing) if existing.is_dir => return Err(fat32::Error::AlreadyExists.into()),
                    Ok(_) if source_metadata.is_dir => return Err(OpError::Message("cannot replace a file with a directory")),
                    // Like mv on Unix, an existing file is replaced.
                    Ok(_) => volume.remove_file(&target)?,
                    Err(error) if is_not_found(&error) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            volume.rename(&source, &target)?;
            Ok(())
        });
        if let Err(error) = result {
            fail(shell, "mv", source_argument, error);
        }
    }
}

pub fn cp(shell: &mut Shell, arguments: &[&str]) {
    let Some((flags, operands)) = options(shell, "cp", arguments, "r") else { return };
    if operands.len() < 2 {
        return usage(shell, "cp");
    }
    let recursive = flags.contains(&'r');
    let Some((sources, destination)) = sources_and_destination(shell, "cp", &operands) else { return };
    let destination = resolve(shell, destination);
    for source_argument in sources {
        let source = resolve(shell, source_argument);
        let result = on_volume(|volume| {
            let source_metadata = volume.metadata(&source)?;
            let target = destination_for(volume, &source, &destination)?;
            if path::same_path(&source, &target) {
                return Err(OpError::Message("source and destination are the same"));
            }
            if source_metadata.is_dir {
                if !recursive {
                    return Err(OpError::Message("is a directory (use cp -r)"));
                }
                if path::is_within(&target, &source) {
                    return Err(OpError::Message("cannot copy a directory into itself"));
                }
                copy_tree(volume, &source, &target, 0)
            } else {
                copy_file_replacing(volume, &source, &target)
            }
        });
        if let Err(error) = result {
            fail(shell, "cp", source_argument, error);
        }
    }
}

fn copy_file_replacing(volume: &mut Volume, source: &str, target: &str) -> Result<(), OpError> {
    match volume.metadata(target) {
        Ok(existing) if existing.is_dir => return Err(fat32::Error::IsADirectory.into()),
        Ok(_) => volume.remove_file(target)?,
        Err(error) if is_not_found(&error) => {}
        Err(error) => return Err(error.into()),
    }
    volume.copy_file(source, target)?;
    Ok(())
}

fn copy_tree(volume: &mut Volume, source: &str, target: &str, depth: usize) -> Result<(), OpError> {
    if depth > COPY_MAX_DEPTH {
        return Err(OpError::Message("directories nested too deeply"));
    }
    match volume.metadata(target) {
        Ok(existing) if !existing.is_dir => return Err(fat32::Error::AlreadyExists.into()),
        Ok(_) => {}
        Err(error) if is_not_found(&error) => volume.create_dir(target)?,
        Err(error) => return Err(error.into()),
    }
    for entry in volume.read_dir(source)? {
        let from = path::join(source, &entry.name);
        let to = path::join(target, &entry.name);
        if entry.is_dir {
            copy_tree(volume, &from, &to, depth + 1)?;
        } else {
            copy_file_replacing(volume, &from, &to)?;
        }
    }
    Ok(())
}

// ----- reading ---------------------------------------------------------------------------------

/// Reads up to `limit` bytes of a file starting at `offset`.
fn read_range(volume: &mut Volume, path: &str, offset: u64, limit: u64) -> fat32::Result<Vec<u8>> {
    let mut buffer = vec![0u8; limit as usize];
    let mut filled = 0;
    while filled < buffer.len() {
        let read = volume.read_at(path, offset + filled as u64, &mut buffer[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    buffer.truncate(filled);
    Ok(buffer)
}

/// Opens a regular file for reading: its size, or an error for directories.
fn file_size(volume: &mut Volume, path: &str) -> fat32::Result<u64> {
    let metadata = volume.metadata(path)?;
    if metadata.is_dir {
        return Err(fat32::Error::IsADirectory);
    }
    Ok(metadata.size)
}

pub fn cat(shell: &mut Shell, arguments: &[&str]) {
    if arguments.is_empty() {
        return usage(shell, "cat");
    }
    for &argument in arguments {
        let path = resolve(shell, argument);
        let capturing = shell.is_capturing();
        let size = match on_volume(|volume| Ok(file_size(volume, &path)?)) {
            Ok(size) => size,
            Err(error) => {
                fail(shell, "cat", argument, error);
                continue;
            }
        };
        if capturing {
            if size > super::MAX_CAPTURE_BYTES as u64 {
                fail(shell, "cat", argument, format!("too large to redirect (limit {}; use cp to copy files)", human_size(super::MAX_CAPTURE_BYTES as u64)));
                continue;
            }
            // Raw bytes, in chunks, so only the redirect buffer holds the whole file.
            let mut offset = 0;
            while offset < size {
                match on_volume(|volume| Ok(read_range(volume, &path, offset, CAT_CHUNK.min(size - offset))?)) {
                    Ok(chunk) if chunk.is_empty() => break,
                    Ok(chunk) => {
                        offset += chunk.len() as u64;
                        shell.print_bytes(&chunk);
                    }
                    Err(error) => {
                        fail(shell, "cat", argument, error);
                        break;
                    }
                }
            }
            continue;
        }

        let data = match on_volume(|volume| Ok(read_range(volume, &path, 0, size.min(CAT_SCREEN_LIMIT))?)) {
            Ok(data) => data,
            Err(error) => {
                fail(shell, "cat", argument, error);
                continue;
            }
        };
        if data.iter().take(4096).any(|&byte| byte == 0) {
            shell.print_warning(&format!("cat: {}: binary file, non-printable bytes are shown as '.' (try hexdump)", argument));
        }
        shell.print_bytes(&data);
        // After the text, so it is still on screen when the output stops.
        if size > data.len() as u64 {
            shell.print_warning(&format!(
                "cat: {}: showed the first {} of {} (use head, tail or hexdump for the rest)",
                argument,
                human_size(data.len() as u64),
                human_size(size)
            ));
        }
    }
}

/// Parses `-n N` / `-N` for head and tail. Returns (lines, file).
fn line_count_arguments<'a>(shell: &mut Shell, command: &str, arguments: &[&'a str]) -> Option<(usize, &'a str)> {
    let mut lines = 10;
    let mut file = None;
    let mut index = 0;
    while index < arguments.len() {
        let argument = arguments[index];
        let number = if argument == "-n" {
            index += 1;
            arguments.get(index).copied()
        } else if let Some(digits) = argument.strip_prefix('-') {
            Some(digits)
        } else {
            if file.replace(argument).is_some() {
                usage(shell, command);
                return None;
            }
            None
        };
        if let Some(number) = number {
            match number.parse::<usize>() {
                Ok(count) => lines = count,
                Err(_) => {
                    shell.print_error(&format!("{}: invalid line count '{}'", command, number));
                    return None;
                }
            }
        } else if argument == "-n" {
            usage(shell, command);
            return None;
        }
        index += 1;
    }
    match file {
        Some(file) => Some((lines, file)),
        None => {
            usage(shell, command);
            None
        }
    }
}

pub fn head(shell: &mut Shell, arguments: &[&str]) {
    let Some((lines, file)) = line_count_arguments(shell, "head", arguments) else { return };
    let path = resolve(shell, file);
    let result = on_volume(|volume| {
        let size = file_size(volume, &path)?;
        Ok(read_range(volume, &path, 0, size.min(HEAD_TAIL_WINDOW))?)
    });
    match result {
        Ok(data) => {
            let end = data.iter().enumerate().filter(|(_, byte)| **byte == b'\n').nth(lines.wrapping_sub(1)).map(|(index, _)| index + 1);
            let end = if lines == 0 { 0 } else { end.unwrap_or(data.len()) };
            shell.print_bytes(&data[..end]);
        }
        Err(error) => fail(shell, "head", file, error),
    }
}

pub fn tail(shell: &mut Shell, arguments: &[&str]) {
    let Some((lines, file)) = line_count_arguments(shell, "tail", arguments) else { return };
    let path = resolve(shell, file);
    let result = on_volume(|volume| {
        let size = file_size(volume, &path)?;
        let length = size.min(HEAD_TAIL_WINDOW);
        Ok(read_range(volume, &path, size - length, length)?)
    });
    match result {
        Ok(data) => {
            // A final newline ends the last line rather than starting an empty one.
            let body = data.strip_suffix(b"\n").unwrap_or(&data);
            let start = if lines == 0 {
                data.len()
            } else {
                body.iter().enumerate().rev().filter(|(_, byte)| **byte == b'\n').nth(lines - 1).map_or(0, |(index, _)| index + 1)
            };
            shell.print_bytes(&data[start..]);
        }
        Err(error) => fail(shell, "tail", file, error),
    }
}

pub fn hexdump(shell: &mut Shell, arguments: &[&str]) {
    let [file] = arguments else { return usage(shell, "hexdump") };
    let path = resolve(shell, file);
    let result = on_volume(|volume| {
        let size = file_size(volume, &path)?;
        Ok((size, read_range(volume, &path, 0, size.min(HEXDUMP_LIMIT))?))
    });
    let (size, data) = match result {
        Ok(result) => result,
        Err(error) => return fail(shell, "hexdump", file, error),
    };

    let mut previous: Option<&[u8]> = None;
    let mut skipping = false;
    for (index, line) in data.chunks(16).enumerate() {
        if previous == Some(line) && line.len() == 16 {
            if !skipping {
                shell.println_colored("*", theme::MUTED);
                skipping = true;
            }
            continue;
        }
        skipping = false;
        previous = Some(line);

        let mut hex = String::with_capacity(50);
        for column in 0..16 {
            match line.get(column) {
                Some(byte) => hex.push_str(&format!("{:02x} ", byte)),
                None => hex.push_str("   "),
            }
            if column == 7 {
                hex.push(' ');
            }
        }
        let ascii: String = line.iter().map(|&byte| if byte.is_ascii_graphic() || byte == b' ' { byte as char } else { '.' }).collect();
        shell.print_colored(&format!("{:08x}  ", index * 16), theme::MUTED);
        shell.print(&hex);
        shell.print_colored(" |", theme::MUTED);
        shell.print_colored(&ascii, theme::GREEN);
        shell.println_colored("|", theme::MUTED);
    }
    shell.println_colored(&format!("{:08x}", data.len()), theme::MUTED);
    if size > data.len() as u64 {
        shell.print_warning(&format!("hexdump: {}: showing the first {} of {}", file, human_size(data.len() as u64), human_size(size)));
    }
}

pub fn stat(shell: &mut Shell, arguments: &[&str]) {
    if arguments.is_empty() {
        return usage(shell, "stat");
    }
    for (index, &argument) in arguments.iter().enumerate() {
        let path = resolve(shell, argument);
        let result = on_volume(|volume| {
            let metadata = volume.metadata(&path)?;
            let canonical = canonical_case(volume, &path)?;
            let entries = if metadata.is_dir { Some(volume.read_dir(&path)?.len()) } else { None };
            Ok((metadata, canonical, entries))
        });
        let (metadata, canonical, entries) = match result {
            Ok(result) => result,
            Err(error) => {
                fail(shell, "stat", argument, error);
                continue;
            }
        };
        if index > 0 {
            shell.println("");
        }
        let mut attributes = Vec::new();
        if metadata.read_only {
            attributes.push("read-only");
        }
        if metadata.hidden {
            attributes.push("hidden");
        }
        if metadata.system {
            attributes.push("system");
        }
        shell.print_field("path", &canonical);
        shell.print_field("type", if metadata.is_dir { "directory" } else { "file" });
        match entries {
            Some(entries) => shell.print_field("entries", &entries.to_string()),
            None => shell.print_field("size", &format!("{} bytes ({})", metadata.size, human_size(metadata.size))),
        }
        shell.print_field("created", &format_timestamp(&metadata.created, true));
        shell.print_field("modified", &format_timestamp(&metadata.modified, true));
        shell.print_field("attributes", &if attributes.is_empty() { String::from("none") } else { attributes.join(", ") });
    }
}

// ----- volumes and drives ----------------------------------------------------------------------

fn usage_bar(used: u64, total: u64, width: usize) -> String {
    let filled = (used as u128 * width as u128 / total.max(1) as u128) as usize;
    let mut bar = String::from("[");
    (0..width).for_each(|i| bar.push(if i < filled { '#' } else { '.' }));
    bar.push(']');
    bar
}

pub fn df(shell: &mut Shell, _: &[&str]) {
    let Some(info) = fs::volume_info() else {
        return shell.print_error(&format!("df: {}", FsError::NotMounted));
    };
    let used = info.used_bytes();
    let percent = (used as u128 * 100 / info.total_bytes.max(1) as u128) as u64;
    shell.print_field("mounted", &format!("{} on / (FAT32)", info.source));
    shell.print_field("label", if info.label.is_empty() { "-" } else { &info.label });
    shell.print_field("size", &human_size(info.total_bytes));
    shell.print_field("used", &format!("{} {} {}%", human_size(used), usage_bar(used, info.total_bytes, 30), percent));
    shell.print_field("free", &human_size(info.free_bytes));
    shell.print_field("clusters", &format!("{} x {} bytes", info.total_bytes / info.cluster_size.max(1) as u64, info.cluster_size));
    shell.print_field("volume id", &format!("{:04X}-{:04X}", info.volume_id >> 16, info.volume_id & 0xFFFF));
}

pub fn lsblk(shell: &mut Shell, _: &[&str]) {
    let drives = fs::drives();
    if drives.is_empty() {
        shell.println_colored("no ATA drives found", theme::MUTED);
        return;
    }
    let mounted = fs::mounted_source();
    let mount_point = |name: &str| if mounted.as_deref() == Some(name) { "/" } else { "" };
    let volume_label = fs::volume_info().map(|info| info.label);
    let label_for = |name: &str, probed: Option<&String>| -> String {
        if mounted.as_deref() == Some(name) {
            volume_label.clone().unwrap_or_default()
        } else {
            probed.cloned().unwrap_or_default()
        }
    };

    shell.println_colored(&format!("{:<10}{:>11}  {:<6}{:<13}{:<13}{:<7}{}", "NAME", "SIZE", "TYPE", "FS", "LABEL", "MOUNT", "MODEL"), theme::MUTED);
    for drive in &drives {
        let (fs_kind, label) = match &drive.whole_disk {
            Some(probe) => (probe.kind.clone(), label_for(drive.name, probe.label.as_ref())),
            None => (String::new(), String::new()),
        };
        shell.print_colored(&format!("{:<10}", drive.name), theme::GREEN);
        shell.print(&format!("{:>11}  {:<6}{:<13}{:<13}", human_size(drive.size_bytes()), "disk", fs_kind, label));
        shell.print_colored(&format!("{:<7}", mount_point(drive.name)), theme::GREEN);
        shell.print_colored(&drive.model, theme::MUTED);
        if drive.is_boot {
            shell.print_colored("  (boot drive)", theme::YELLOW);
        }
        shell.println("");
        if let Some(error) = &drive.error {
            shell.println_colored(&format!("  cannot read the partition table: {}", error), theme::RED);
        }
        for (index, partition) in drive.partitions.iter().enumerate() {
            let name = format!("{}p{}", drive.name, partition.number);
            let branch = if index + 1 == drive.partitions.len() { "`-" } else { "|-" };
            let fs_kind = partition.fs.as_ref().map_or(String::from("-"), |probe| probe.kind.clone());
            let label = label_for(&name, partition.fs.as_ref().and_then(|probe| probe.label.as_ref()));
            shell.print_colored(branch, theme::MUTED);
            shell.print(&format!("{:<8}{:>11}  {:<6}{:<13}{:<13}", name, human_size(partition.size_bytes()), "part", fs_kind, label));
            shell.print_colored(&format!("{:<7}", mount_point(&name)), theme::GREEN);
            let mut notes = Vec::new();
            if partition.fs.is_none() {
                notes.push(format!("type 0x{:02x} ({})", partition.partition_type, partition.type_name()));
            }
            if partition.bootable {
                notes.push(String::from("active"));
            }
            shell.println_colored(&notes.join(", "), theme::MUTED);
        }
    }
}

pub fn mount(shell: &mut Shell, arguments: &[&str]) {
    match arguments {
        [] => match fs::volume_info() {
            Some(info) => {
                shell.println(&format!("{} on / type fat32 ({})", info.source, if info.label.is_empty() { "no label" } else { &info.label }));
            }
            None => {
                shell.println_colored("nothing is mounted", theme::MUTED);
                let candidates: Vec<String> = fs::drives()
                    .iter()
                    .filter(|drive| !drive.is_boot && drive.has_fat32())
                    .map(|drive| String::from(drive.name))
                    .collect();
                if !candidates.is_empty() {
                    shell.println_colored(&format!("FAT32 volumes found on: {} (mount <drive>)", candidates.join(", ")), theme::MUTED);
                }
            }
        },
        [target] => match fs::mount(target) {
            Ok(info) => {
                shell.cwd = String::from("/");
                shell.print_colored("mounted ", theme::GREEN);
                shell.println(&format!(
                    "{} ({}, {} free) on /",
                    info.source,
                    if info.label.is_empty() { "no label" } else { &info.label },
                    human_size(info.free_bytes)
                ));
                if !info.was_clean {
                    shell.print_warning(&format!("{} was not cleanly unmounted; run 'fsck' to check it", info.source));
                }
            }
            Err(error) => shell.print_error(&format!("mount: {}", error)),
        },
        _ => usage(shell, "mount"),
    }
}

pub fn umount(shell: &mut Shell, arguments: &[&str]) {
    if arguments.len() > 1 {
        return usage(shell, "umount");
    }
    if let Some(&target) = arguments.first() {
        let source = fs::mounted_source();
        if target != "/" && source.as_deref().is_some_and(|source| !source.eq_ignore_ascii_case(target)) {
            return shell.print_error(&format!("umount: {}: not mounted", target));
        }
    }
    match fs::unmount() {
        Ok(source) => {
            shell.cwd = String::from("/");
            shell.println_colored(&format!("unmounted {}; it is safe to detach", source), theme::MUTED);
        }
        Err(error) => shell.print_error(&format!("umount: {}", error)),
    }
}

pub fn sync(shell: &mut Shell, _: &[&str]) {
    if !fs::is_mounted() {
        return shell.print_error(&format!("sync: {}", FsError::NotMounted));
    }
    match fs::sync() {
        Ok(()) => {
            let source = fs::mounted_source().unwrap_or_default();
            shell.println_colored(&format!("all changes written to {}", source), theme::MUTED);
        }
        Err(error) => shell.print_error(&format!("sync: {}", error)),
    }
}

/// Checks a volume label: at most 11 characters FAT accepts in a label. Returns it upper-case.
fn validate_label(label: &str) -> Result<String, &'static str> {
    if label.chars().count() > 11 {
        return Err("labels have at most 11 characters");
    }
    if label.trim().is_empty() {
        return Err("label is empty");
    }
    let forbidden = |c: char| !c.is_ascii() || c.is_ascii_control() || "\"*+,./:;<=>?[\\]|".contains(c);
    if label.chars().any(forbidden) {
        return Err("labels may use letters, digits, spaces and ! # $ % & ' ( ) - @ ^ _ ` { } ~");
    }
    Ok(label.to_ascii_uppercase())
}

pub fn format_drive(shell: &mut Shell, arguments: &[&str]) {
    let (name, label) = match arguments {
        [name] => (*name, String::from(fs::DEFAULT_LABEL)),
        [name, label] => match validate_label(label) {
            Ok(label) => (*name, label),
            Err(error) => return fail(shell, "format", label, error),
        },
        _ => return usage(shell, "format"),
    };
    if shell.is_capturing() {
        return shell.print_error("format: asks for confirmation, so its output cannot be redirected");
    }
    let drive = match fs::drive(name) {
        Ok(drive) => drive,
        Err(error) => return shell.print_error(&format!("format: {}", error)),
    };
    if let Err(error) = drive.check_formattable() {
        return shell.print_error(&format!("format: {}", error));
    }

    shell.print_colored(&format!("{}: ", drive.name), theme::GREEN);
    shell.println(&format!("{}, {}", drive.model, human_size(drive.size_bytes())));
    let mounted = fs::mounted_source();
    for partition in &drive.partitions {
        let partition_name = format!("{}p{}", drive.name, partition.number);
        let contents = match &partition.fs {
            Some(probe) => format!("{} {}", probe.kind, probe.label.clone().unwrap_or_default()),
            None => String::from(partition.type_name()),
        };
        let mounted_note = if mounted.as_deref() == Some(partition_name.as_str()) { ", mounted on /" } else { "" };
        shell.println_colored(
            &format!("  {}  {}  {}{}", partition_name, human_size(partition.size_bytes()), contents.trim_end(), mounted_note),
            theme::MUTED,
        );
    }
    shell.println_colored(
        &format!("Formatting erases EVERYTHING on {} and creates one FAT32 partition labelled '{}'.", drive.name, label),
        theme::RED,
    );
    let Some(answer) = shell.read_input(&format!("Type {} again to confirm: ", drive.name)) else {
        return shell.println_colored("format cancelled", theme::MUTED);
    };
    if answer.trim() != drive.name {
        return shell.println_colored("format cancelled (the name did not match)", theme::MUTED);
    }

    shell.println_colored(&format!("Formatting {}...", drive.name), theme::MUTED);
    shell.terminal.flush();
    match fs::format(drive.name, &label) {
        Ok(Some(info)) => {
            shell.cwd = String::from("/");
            shell.print_colored("done: ", theme::GREEN);
            shell.println(&format!("{} is an empty FAT32 volume '{}' ({}), mounted on /", info.source, info.label, human_size(info.total_bytes)));
        }
        Ok(None) => {
            let source = fs::mounted_source().unwrap_or_default();
            shell.print_colored("done: ", theme::GREEN);
            shell.println(&format!("{}p1 formatted; not mounted because {} is mounted on /", drive.name, source));
        }
        Err(error) => shell.print_error(&format!("format: {}: {}", drive.name, error)),
    }
}

pub fn fsck(shell: &mut Shell, _: &[&str]) {
    let Some(source) = fs::mounted_source() else {
        return shell.print_error(&format!("fsck: {}", FsError::NotMounted));
    };
    shell.println_colored(&format!("checking {} (read-only)...", source), theme::MUTED);
    shell.terminal.flush();
    let report = match fs::with_volume(|volume| volume.check()) {
        Ok(report) => report,
        Err(error) => return shell.print_error(&format!("fsck: {}: {}", source, error)),
    };
    shell.print_field("files", &report.files.to_string());
    shell.print_field("directories", &report.directories.to_string());
    shell.print_field("used", &format!("{} clusters", report.used_clusters));
    shell.print_field("lost", &format!("{} clusters", report.lost_clusters));
    shell.print_field(
        "at mount",
        if report.was_cleanly_unmounted { "clean" } else { "not clean (the last session did not end with a sync)" },
    );
    if report.is_clean() {
        shell.println_colored("no problems found", theme::GREEN);
    } else {
        shell.println_colored(&format!("{} problem(s) found:", report.problems.len()), theme::RED);
        for problem in &report.problems {
            shell.println(&format!("  - {}", problem));
        }
        shell.println_colored("fsck only reports problems; it does not repair them", theme::MUTED);
    }
}

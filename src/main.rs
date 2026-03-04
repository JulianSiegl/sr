//! Speedread command line tool sr
//! This tool is inspired by a trick in speed reading, where lines are 
//! shown word by word with visual guidance for the eye to enable 
//! reading speeds in excess of 500 words per minute.
//! This tool is distributed under MIT License and was developed with 
//! the help of Claude Code's Sonnet 4.6 on the 4th of March 2026.
//! Usage: Call the compiled application with up to two command line 
//! elements, the first being a mandatory path to a .txt file 
//! supporting UTF-8 and the second optional one being the target 
//! words per minute.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;
use unicode_width::UnicodeWidthChar;

// ANSI escape codes
const RESET: &str = "\x1b[0m";
const RED_BOLD: &str = "\x1b[1;31m";
const WHITE: &str = "\x1b[97m";
const CLEAR_LINE: &str = "\x1b[2K\r";

/// Fetch width of current bash terminal on both unixoid 
/// and windows OSs to place words in the center
fn get_terminal_width() -> usize {
    // Depending on OS, use an unsafe systemcall to query terminal size
    // Try to read terminal width via ioctl on unix
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        unsafe {
            let mut ws: libc::winsize = std::mem::zeroed();
            if libc::ioctl(io::stdout().as_raw_fd(), libc::TIOCGWINSZ, &mut ws) == 0
                && ws.ws_col > 0
            {
                return ws.ws_col as usize;
            }
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;

        #[allow(non_camel_case_types)] type HANDLE = *mut std::ffi::c_void;
        #[allow(non_camel_case_types)] type BOOL   = i32;
        #[allow(non_camel_case_types)] type SHORT  = i16;
        #[allow(non_camel_case_types)] type WORD   = u16;

        #[repr(C)] #[allow(non_snake_case)]
        struct COORD { X: SHORT, Y: SHORT }

        #[repr(C)] #[allow(non_snake_case)]
        struct SMALL_RECT { Left: SHORT, Top: SHORT, Right: SHORT, Bottom: SHORT }

        #[repr(C)] #[allow(non_snake_case)]
        struct CONSOLE_SCREEN_BUFFER_INFO {
            dwSize:              COORD,
            dwCursorPosition:    COORD,
            wAttributes:         WORD,
            srWindow:            SMALL_RECT,
            dwMaximumWindowSize: COORD,
        }

        extern "system" {
            fn GetConsoleScreenBufferInfo(
                hConsoleOutput: HANDLE,
                lpConsoleScreenBufferInfo: *mut CONSOLE_SCREEN_BUFFER_INFO,
            ) -> BOOL;
        }

        unsafe {
            let handle = io::stdout().as_raw_handle();
            let mut info: CONSOLE_SCREEN_BUFFER_INFO = std::mem::zeroed();
            if GetConsoleScreenBufferInfo(handle as HANDLE, &mut info) != 0 {
                let width = (info.srWindow.Right - info.srWindow.Left + 1) as usize;
                if width > 0 {
                    return width;
                }
            }
        }

        // Git Bash / mintty running a Windows binary may not expose a real Win32
        // console (GetConsoleScreenBufferInfo fails).  These emulators usually set
        // the COLUMNS environment variable so terminal-aware programs can adapt.
        if let Ok(val) = std::env::var("COLUMNS") {
            if let Ok(n) = val.trim().parse::<usize>() {
                if n > 0 {
                    return n;
                }
            }
        }
    }
    // fallback value of 80
    80
}

/// A single display-aware "glyph cluster": one base char plus any following
/// zero-width combining characters (e.g. accents, diacritics).
struct Cluster {
    chars: Vec<char>,
    /// Terminal column width of this cluster (0, 1, or 2).
    width: usize,
}

/// Split a word into display clusters.
/// A combining character (display width 0) is merged into the preceding cluster.
fn clusters(word: &str) -> Vec<Cluster> {
    let mut result: Vec<Cluster> = Vec::new();
    for ch in word.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if w == 0 && !result.is_empty() {
            // Zero-width: attach to the previous cluster (combining mark, ZWJ, etc.)
            result.last_mut().unwrap().chars.push(ch);
        } else {
            result.push(Cluster { chars: vec![ch], width: w });
        }
    }
    result
}

/// Returns an ANSI-coloured string.
/// The pivot cluster (middle by cluster count) is rendered in bold red;
/// all others in bright white.
fn colour_word(word: &str) -> String {
    let cls = clusters(word);
    let n = cls.len();
    if n == 0 {
        return String::new();
    }
    let mid = (n - 1) / 2; // favour left-centre for even lengths

    let mut out = String::new();
    for (i, cluster) in cls.iter().enumerate() {
        if i == mid {
            out.push_str(RED_BOLD);
        } else {
            out.push_str(WHITE);
        }
        for &ch in &cluster.chars {
            out.push(ch);
        }
        out.push_str(RESET);
    }
    out
}

/// Centre a visible string of `visible_len` characters in a field of `width`.
/// Returns the number of spaces to prepend.

/// Display a word by placing it centered in the current terminal
fn display_word(word: &str, term_width: usize) {
    let cls = clusters(word);
    let n = cls.len();
    if n == 0 {
        return;
    }
    let mid = (n - 1) / 2;

    // Column offset of the pivot cluster's left edge = sum of widths before it.
    let pivot_col: usize = cls[..mid].iter().map(|c| c.width).sum();

    // We want the pivot's left edge at the terminal centre column.
    let centre_col = term_width / 2;
    let pad = if centre_col >= pivot_col { centre_col - pivot_col } else { 0 };

    let coloured = colour_word(word);

    print!("{}{}{}", CLEAR_LINE, " ".repeat(pad), coloured);
    io::stdout().flush().unwrap();
}

fn print_usage(prog: &str) {
    eprintln!("Usage: {} <file.txt> [wpm]", prog);
    eprintln!();
    eprintln!("  file.txt  Path to a plain-text file");
    eprintln!("  wpm       Words per minute (default: 300)");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage(&args[0]);
        std::process::exit(1);
    }

    let path = &args[1];
    let wpm: u64 = if args.len() >= 3 {
        args[2].parse().unwrap_or_else(|_| {
            eprintln!("Invalid wpm value '{}', using 300.", args[2]);
            300
        })
    } else {
        300
    };
    let term_width = get_terminal_width();
    let content = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("Error reading '{}': {}", path, e);
        std::process::exit(1);
    });

    let delay_ms = 60_000 / wpm; // milliseconds per word

    // Hide the cursor for a cleaner look
    print!("\x1b[?25l");
    io::stdout().flush().unwrap();

    // Restore cursor on Ctrl-C
    ctrlc_setup();

    for word in content.split_whitespace(){
        display_word(word, term_width);
        thread::sleep(Duration::from_millis(delay_ms));
    }

    // After last word: move to new line and restore cursor
    println!();
    print!("\x1b[?25h");
    io::stdout().flush().unwrap();
}

/// Register a Ctrl-C handler that restores the cursor before exiting.
fn ctrlc_setup() {
    // Use the standard Unix signal approach — no external crate needed.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGINT, handle_sigint as *const () as libc::sighandler_t);
    }
}

#[cfg(unix)]
extern "C" fn handle_sigint(_: libc::c_int) {
    // Restore cursor, print newline, exit cleanly
    print!("\n\x1b[?25h");
    let _ = io::stdout().flush();
    std::process::exit(0);
}

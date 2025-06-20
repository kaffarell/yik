use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::{
    error::Error,
    fs,
    io::{self, Write},
    path::Path,
    process::Command,
};

#[derive(Debug, Clone, PartialEq)]
enum AppState {
    SelectingKernel,
    ConfirmingSwitch(String), // Contains the selected kernel version
}

struct App {
    kernel_versions: Vec<String>,
    current_kernel: Option<String>,
    list_state: ListState,
    state: AppState,
}

impl App {
    fn new() -> Result<App, Box<dyn Error>> {
        let kernel_versions = get_kernel_versions()?;
        let current_kernel = get_current_kernel().ok();
        let mut list_state = ListState::default();
        if !kernel_versions.is_empty() {
            list_state.select(Some(0));
        }

        Ok(App {
            kernel_versions,
            current_kernel,
            list_state,
            state: AppState::SelectingKernel,
        })
    }

    fn next(&mut self) {
        if self.state != AppState::SelectingKernel {
            return;
        }

        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.kernel_versions.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn previous(&mut self) {
        if self.state != AppState::SelectingKernel {
            return;
        }

        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.kernel_versions.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn select_current(&mut self) -> Result<(), Box<dyn Error>> {
        if self.state != AppState::SelectingKernel {
            return Ok(());
        }

        if let Some(i) = self.list_state.selected() {
            if let Some(version) = self.kernel_versions.get(i) {
                print!("Loading kernel version: {}... ", version);
                io::stdout().flush()?;

                match execute_kexec_load(version) {
                    Ok(_) => {
                        println!("Success!");
                        self.state = AppState::ConfirmingSwitch(version.clone());
                    }
                    Err(e) => {
                        println!("Failed: {}", e);
                        eprintln!("Press Enter to continue...");
                        let mut input = String::new();
                        io::stdin().read_line(&mut input)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn confirm_switch(&mut self) -> Result<(), Box<dyn Error>> {
        if let AppState::ConfirmingSwitch(version) = &self.state {
            // Simple yes/no prompt
            println!("\nKernel {} has been loaded successfully!", version);
            print!("\nDo you want to proceed with the kernel switch? (Y/n): ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim().to_lowercase();

            if ["n", "no", "N"].contains(&input.as_str()) {
                self.state = AppState::SelectingKernel;
            } else {
                println!("\nRunning kexec ... (The console will hang shortly, press any key to continue)");
                // Execute kexec -e and exit
                match execute_kexec_execute() {
                    Ok(_) => {
                        // This shouldn't return, but just in case
                        eprintln!("Kernel switch should have rebooted the system");
                        self.state = AppState::SelectingKernel;
                    }
                    Err(e) => {
                        eprintln!("Failed to execute kernel switch: {}", e);
                        self.state = AppState::SelectingKernel;
                    }
                }
            }
        }
        Ok(())
    }
}

fn get_kernel_versions() -> Result<Vec<String>, Box<dyn Error>> {
    let boot_path = Path::new("/boot");

    if !boot_path.exists() {
        return Err("Boot directory /boot does not exist".into());
    }

    let entries = fs::read_dir(boot_path)?;
    let mut kernel_versions = Vec::new();

    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        // Look for vmlinuz files which indicate kernel versions
        if file_name_str.starts_with("vmlinuz-") {
            let version = file_name_str
                .strip_prefix("vmlinuz-")
                .unwrap_or(&file_name_str);
            kernel_versions.push(version.to_string());
        }
    }

    // Sort versions
    kernel_versions.sort();

    if kernel_versions.is_empty() {
        return Err("No kernel versions found in /boot directory".into());
    }

    Ok(kernel_versions)
}

fn get_current_kernel() -> Result<String, Box<dyn Error>> {
    let output = Command::new("uname").arg("-r").output()?;

    if !output.status.success() {
        return Err("Failed to get current kernel version".into());
    }

    let version = String::from_utf8(output.stdout)?;
    Ok(version.trim().to_string())
}

fn get_cmdline() -> Result<String, Box<dyn Error>> {
    let cmdline = fs::read_to_string("/proc/cmdline")?;
    Ok(cmdline.trim().to_string())
}

fn find_initrd_file(version: &str) -> Result<String, Box<dyn Error>> {
    let boot_path = Path::new("/boot");
    let entries = fs::read_dir(boot_path)?;

    // Look for initrd files that match the version
    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        // Check for different initrd naming patterns
        if (file_name_str.starts_with("initrd.img-") && file_name_str.contains(version))
            || (file_name_str.starts_with("initramfs-") && file_name_str.contains(version))
        {
            return Ok(format!("/boot/{}", file_name_str));
        }
    }

    Err(format!("No initrd file found for version {}", version).into())
}

fn execute_kexec_load(version: &str) -> Result<(), Box<dyn Error>> {
    let vmlinuz_path = format!("/boot/vmlinuz-{}", version);
    let initrd_path = find_initrd_file(version)?;
    let cmdline = get_cmdline()?;

    let output = Command::new("sudo")
        .arg("kexec")
        .arg("-l")
        .arg(&vmlinuz_path)
        .arg(format!("--initrd={}", initrd_path))
        .arg(format!("--command-line={}", cmdline))
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("kexec load failed: {}", stderr).into());
    }

    Ok(())
}

fn execute_kexec_execute() -> Result<(), Box<dyn Error>> {
    let output = Command::new("sudo").arg("kexec").arg("-e").output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("kexec execute failed: {}", stderr).into());
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run it
    let app = App::new();
    let res = match app {
        Ok(mut app) => run_app(&mut terminal, &mut app),
        Err(e) => {
            // Restore terminal before showing error
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )?;
            terminal.show_cursor()?;
            Err(e)
        }
    };

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

fn run_app<B: Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<(), Box<dyn Error>> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        // Handle confirmation with simple prompt
        if let AppState::ConfirmingSwitch(_) = &app.state {
            // Restore terminal before showing prompt
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )?;
            terminal.show_cursor()?;

            app.confirm_switch()?;

            // If we're still here, the user said no or there was an error
            // Re-enable terminal for TUI
            enable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                EnterAlternateScreen,
                EnableMouseCapture
            )?;
            continue;
        }

        if let Event::Key(key) = event::read()? {
            match (&app.state, key.code) {
                (AppState::SelectingKernel, KeyCode::Char('q') | KeyCode::Esc) => return Ok(()),
                (AppState::SelectingKernel, KeyCode::Down | KeyCode::Char('j')) => app.next(),
                (AppState::SelectingKernel, KeyCode::Up | KeyCode::Char('k')) => app.previous(),
                (AppState::SelectingKernel, KeyCode::Enter) => {
                    // Temporarily exit TUI for loading
                    disable_raw_mode()?;
                    execute!(
                        terminal.backend_mut(),
                        LeaveAlternateScreen,
                        DisableMouseCapture
                    )?;
                    terminal.show_cursor()?;

                    // Handle kernel selection
                    app.select_current()?;

                    // Re-enable TUI if we're still in SelectingKernel state
                    if app.state == AppState::SelectingKernel {
                        enable_raw_mode()?;
                        execute!(
                            terminal.backend_mut(),
                            EnterAlternateScreen,
                            EnableMouseCapture
                        )?;
                    }
                }

                _ => {}
            }
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    render_kernel_selection(f, app);
}

fn render_kernel_selection(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.area());

    // Title
    let title = Paragraph::new("Kernel Version Selector")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Kernel versions list
    let items: Vec<ListItem> = app
        .kernel_versions
        .iter()
        .map(|version| {
            let is_current = app.current_kernel.as_ref() == Some(version);
            let display_text = if is_current {
                format!("  {} (current)", version)
            } else {
                format!("  {}", version)
            };

            let style = if is_current {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(Span::styled(display_text, style)))
        })
        .collect();

    let items = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Available Kernel Versions"),
        )
        .highlight_style(
            Style::default()
                .bg(Color::LightGreen)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(items, chunks[1], &mut app.list_state.clone());

    // Instructions
    let instructions = Paragraph::new("Use ↑/↓ or j/k to navigate, Enter to select, q/Esc to quit")
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL).title("Instructions"));
    f.render_widget(instructions, chunks[2]);
}

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
    MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph};
use ratatui::{Frame, Terminal};
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{Resize, StatefulImage};

use crate::config::{self, Settings};
use crate::library::{scan, write_info_atomic, Song, SongInfo};
use crate::modules;
use crate::paths::AppPaths;
use crate::player;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    Normal,
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Normal,
    Compact,
}

#[derive(Debug, Clone, Copy)]
struct Palette {
    foreground: Color,
    background: Color,
    accent: Color,
    muted: Color,
    highlight: Color,
    playing: Color,
    border: Color,
}

impl Palette {
    fn from_settings(settings: &Settings) -> Self {
        let name = settings.theme.to_ascii_lowercase();
        let built_in = config::builtin_theme(&name).unwrap_or_default();
        let theme = settings.themes.get(&name).unwrap_or(&built_in);
        Self {
            foreground: parse_color(&theme.foreground, Color::White),
            background: parse_color(&theme.background, Color::Reset),
            accent: parse_color(&theme.accent, Color::Cyan),
            muted: parse_color(&theme.muted, Color::DarkGray),
            highlight: parse_color(&theme.highlight, Color::DarkGray),
            playing: parse_color(&theme.playing, Color::Green),
            border: parse_color(&theme.border, Color::Gray),
        }
    }
}

struct ImageRequest {
    generation: u64,
    path: PathBuf,
    picker: Picker,
    cells: (u16, u16),
}

struct ImageResult {
    generation: u64,
    path: PathBuf,
    protocol: Option<Box<dyn StatefulProtocol>>,
}

struct ActiveAnimation {
    animation: modules::ModuleAnimation,
    started: Instant,
    frame_index: usize,
}

enum CanvasLayerRef<'a> {
    Static(&'a modules::ModuleCanvas),
    Animated(&'a ActiveAnimation),
}

impl CanvasLayerRef<'_> {
    fn z(&self) -> i32 {
        match self {
            Self::Static(canvas) => canvas.z,
            Self::Animated(animation) => animation.animation.z,
        }
    }

    fn key(&self) -> String {
        match self {
            Self::Static(canvas) => format!("{}::{}", canvas.module, canvas.id),
            Self::Animated(animation) => {
                format!("{}::{}", animation.animation.module, animation.animation.id)
            }
        }
    }
}

struct App {
    paths: AppPaths,
    settings: Settings,
    songs: Vec<Song>,
    visible: Vec<usize>,
    selected: usize,
    input_mode: InputMode,
    view_mode: ViewMode,
    input: String,
    input_prefix: char,
    show_help: bool,
    module_info: Option<String>,
    query: String,
    tags: Vec<String>,
    status: String,
    pending_g: bool,
    player: Option<Child>,
    audio_filter: Option<String>,
    playing: Option<usize>,
    playback_started: Option<Instant>,
    playback_offset_seconds: u64,
    paused: bool,
    paused_at: Option<Instant>,
    repeat: bool,
    follow_playback: bool,
    queue: Vec<usize>,
    queue_position: Option<usize>,
    should_quit: bool,
    image_picker: Picker,
    image_protocol: Option<Box<dyn StatefulProtocol>>,
    image_path: Option<PathBuf>,
    image_cells: Option<(u16, u16)>,
    image_pending: bool,
    image_generation: u64,
    image_requests: Sender<ImageRequest>,
    image_results: Receiver<ImageResult>,
    module_outputs: Receiver<modules::ModuleOutput>,
    module_runtime: modules::ModuleRuntime,
    module_widget: Option<modules::ModuleWidgetNode>,
    module_canvases: HashMap<String, modules::ModuleCanvas>,
    module_animations: HashMap<String, ActiveAnimation>,
    pending_terminal: Option<modules::TerminalCommand>,
    module_border: Option<Color>,
}

impl App {
    fn palette(&self) -> Palette {
        let mut palette = Palette::from_settings(&self.settings);
        if let Some(border) = self.module_border {
            palette.border = border;
        }
        palette
    }

    fn new(paths: AppPaths, settings: Settings, songs: Vec<Song>) -> Self {
        let (image_requests, image_results) = spawn_image_worker();
        let (module_output_tx, module_outputs) = mpsc::channel();
        let module_runtime = modules::ModuleRuntime::new(paths.clone(), module_output_tx.clone());
        let repeat = settings.repeat;
        let follow_playback = settings.follow_playback;
        let mut app = Self {
            paths,
            settings,
            songs,
            visible: Vec::new(),
            selected: 0,
            input_mode: InputMode::Normal,
            view_mode: ViewMode::Normal,
            input: String::new(),
            input_prefix: ':',
            show_help: false,
            module_info: None,
            query: String::new(),
            tags: Vec::new(),
            status: "ready".into(),
            pending_g: false,
            player: None,
            audio_filter: None,
            playing: None,
            playback_started: None,
            playback_offset_seconds: 0,
            paused: false,
            paused_at: None,
            repeat,
            follow_playback,
            queue: Vec::new(),
            queue_position: None,
            should_quit: false,
            image_picker: Picker::new((8, 16)),
            image_protocol: None,
            image_path: None,
            image_cells: None,
            image_pending: false,
            image_generation: 0,
            image_requests,
            image_results,
            module_outputs,
            module_runtime,
            module_widget: None,
            module_canvases: HashMap::new(),
            module_animations: HashMap::new(),
            pending_terminal: None,
            module_border: None,
        };
        app.refresh_visible();
        app
    }

    fn configure_image_picker(&mut self) {
        // Halfblocks are the only protocol guaranteed by an ANSI terminal.
        // Kitty, Sixel and iTerm2 remain explicit opt-ins, so opening groov in
        // SSH, tmux, macOS Terminal, Linux console, or an IDE never emits an
        // unsupported graphics escape sequence.
        self.image_picker = Picker::new((8, 16));
        match self.settings.image_mode.to_ascii_lowercase().as_str() {
            "ansi" | "ascii" => self.image_picker.protocol_type = ProtocolType::Halfblocks,
            "sixel" => self.image_picker.protocol_type = ProtocolType::Sixel,
            "kitty" => self.image_picker.protocol_type = ProtocolType::Kitty,
            "iterm" | "iterm2" => self.image_picker.protocol_type = ProtocolType::Iterm2,
            "none" => {}
            _ => self.image_picker.protocol_type = ProtocolType::Halfblocks,
        }
    }

    fn refresh_visible(&mut self) {
        let query = self.query.to_ascii_lowercase();
        self.visible = self
            .songs
            .iter()
            .enumerate()
            .filter_map(|(index, song)| {
                let text_match = query.is_empty()
                    || [
                        song.info.title.as_str(),
                        song.info.artist.as_str(),
                        song.info.album.as_str(),
                        song.info.id.as_str(),
                    ]
                    .iter()
                    .any(|field| field.to_ascii_lowercase().contains(&query));
                if text_match && song.info.matches_tags(&self.tags) {
                    Some(index)
                } else {
                    None
                }
            })
            .collect();
        if self.visible.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.visible.len() {
            self.selected = self.visible.len() - 1;
        }
    }

    fn selected_song(&self) -> Option<&Song> {
        self.visible
            .get(self.selected)
            .and_then(|index| self.songs.get(*index))
    }

    fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut stdout = io::stdout();
        let capture_mouse = self.mouse_capture_enabled();
        terminal::enable_raw_mode()?;
        let enter = if capture_mouse {
            execute!(stdout, EnterAlternateScreen, Hide, EnableMouseCapture)
        } else {
            execute!(stdout, EnterAlternateScreen, Hide)
        };
        if let Err(error) = enter {
            let _ = terminal::disable_raw_mode();
            return Err(error.into());
        }
        self.configure_image_picker();
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = match Terminal::new(backend) {
            Ok(terminal) => terminal,
            Err(error) => {
                let mut cleanup_stdout = io::stdout();
                if capture_mouse {
                    let _ = execute!(
                        cleanup_stdout,
                        Show,
                        DisableMouseCapture,
                        LeaveAlternateScreen
                    );
                } else {
                    let _ = execute!(cleanup_stdout, Show, LeaveAlternateScreen);
                }
                let _ = terminal::disable_raw_mode();
                return Err(error.into());
            }
        };
        let result = self.event_loop(&mut terminal);
        self.module_runtime.shutdown(&self.settings);
        player::stop(&mut self.player);
        player::clear_control_endpoint(&self.paths.player_ipc_path());
        let cursor_error = terminal.show_cursor().err();
        let screen_error = if capture_mouse {
            execute!(
                terminal.backend_mut(),
                Show,
                DisableMouseCapture,
                LeaveAlternateScreen
            )
            .err()
        } else {
            execute!(terminal.backend_mut(), Show, LeaveAlternateScreen).err()
        };
        let raw_error = terminal::disable_raw_mode().err();
        if let Some(error) = cursor_error.or(screen_error).or(raw_error) {
            return Err(error.into());
        }
        result
    }

    fn mouse_capture_enabled(&self) -> bool {
        self.settings
            .modules
            .iter()
            .any(|module| module.enabled && module.rpc && module.session && module.mouse)
    }

    fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut dirty = true;
        let mut frame = 0_u64;
        loop {
            let now = Instant::now();
            if self.advance_module_animations(now) {
                dirty = true;
            }
            self.module_runtime.tick(&self.settings, now, frame);
            frame = frame.wrapping_add(1);
            if self.receive_image_results() {
                dirty = true;
            }
            if self.receive_module_outputs() {
                dirty = true;
            }
            if dirty {
                terminal.draw(|frame| self.draw(frame))?;
                dirty = false;
            }
            if event::poll(self.module_poll_timeout(now))? {
                match event::read()? {
                    Event::Key(key) => {
                        if self.handle_key(key) || self.should_quit {
                            break;
                        }
                        if let Some(command) = self.pending_terminal.take() {
                            self.run_terminal_module(terminal, command)?;
                        }
                        dirty = true;
                    }
                    Event::Resize(width, height) => {
                        self.module_runtime.resize(&self.settings, width, height);
                        dirty = true;
                    }
                    Event::Mouse(mouse) if self.handle_mouse(mouse) => dirty = true,
                    Event::Mouse(_) => {}
                    _ => {}
                }
            }
            let finished = if let Some(process) = self.player.as_mut() {
                process.try_wait()?.is_some()
            } else {
                false
            };
            if finished {
                let finished_song = self.playing;
                self.player = None;
                self.playing = None;
                self.playback_started = None;
                self.playback_offset_seconds = 0;
                self.paused = false;
                self.paused_at = None;
                self.image_protocol = None;
                self.image_path = None;
                self.image_cells = None;
                self.image_pending = false;
                self.image_generation = self.image_generation.wrapping_add(1);
                player::clear_control_endpoint(&self.paths.player_ipc_path());
                if self.repeat {
                    if let Some(index) = finished_song {
                        self.start_song_at(index, 0);
                    } else {
                        self.play_next();
                    }
                } else {
                    self.play_next();
                }
                dirty = true;
            }
            if self.receive_image_results() {
                dirty = true;
            }
            if self.receive_module_outputs() {
                dirty = true;
            }
            if self.player.is_some() && self.playback_started.is_some() && !self.paused {
                dirty = true;
            }
        }
        Ok(())
    }

    fn run_terminal_module(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        command: modules::TerminalCommand,
    ) -> Result<(), Box<dyn std::error::Error>> {
        terminal::disable_raw_mode()?;
        execute!(terminal.backend_mut(), Show, LeaveAlternateScreen)?;

        let result = (|| -> Result<std::process::ExitStatus, Box<dyn std::error::Error>> {
            let mut process = Command::new(&command.command);
            if let Some(cwd) = command.cwd.as_deref() {
                process.current_dir(cwd);
            }
            process.args(&command.args);
            process.env("GROOV_CONFIG_DIR", &self.paths.config_dir);
            process.env("GROOV_SETTINGS_FILE", &self.paths.settings_file);
            process.env("GROOV_DATA_DIR", &self.paths.data_dir);
            process.env("GROOV_SONGS_DIR", &self.paths.songs_dir);
            process.env("GROOV_CACHE_DIR", &self.paths.cache_dir);
            process.env("GROOV_PLAYER_IPC", self.paths.player_ipc_path());
            for (key, value) in &command.env {
                process.env(key, value);
            }
            if let Some(song_id) = command.song_id.as_deref() {
                process.env("GROOV_SONG_ID", song_id);
            }
            if let Some(song_file) = command.song_file.as_deref() {
                process.env("GROOV_SONG_FILE", song_file);
            }
            Ok(process.status()?)
        })();

        terminal::enable_raw_mode()?;
        execute!(terminal.backend_mut(), EnterAlternateScreen, Hide)?;
        terminal.clear()?;
        match result {
            Ok(status) if status.success() => {
                self.status = format!("terminal module {} finished", command.module);
            }
            Ok(status) => {
                self.status = format!(
                    "terminal module {} exited with {}",
                    command.module,
                    status
                        .code()
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "a signal".into())
                );
            }
            Err(error) => {
                self.status = format!("cannot run terminal module {}: {error}", command.module);
            }
        }
        Ok(())
    }

    fn advance_module_animations(&mut self, now: Instant) -> bool {
        let mut changed = false;
        for active in self.module_animations.values_mut() {
            let elapsed = now.duration_since(active.started).as_millis() as u64;
            let frame_index = if !active.animation.looped && elapsed >= active.animation.duration_ms
            {
                active.animation.frames.len().saturating_sub(1)
            } else {
                animation_frame_index(&active.animation, elapsed)
            };
            if frame_index != active.frame_index {
                active.frame_index = frame_index;
                changed = true;
            }
        }
        changed
    }

    fn module_poll_timeout(&self, now: Instant) -> Duration {
        let mut timeout = if self.module_animations.is_empty()
            && !self.module_runtime.has_tick_modules(&self.settings)
        {
            Duration::from_millis(self.settings.refresh_ms.max(50))
        } else {
            Duration::from_millis(4)
        };
        for active in self.module_animations.values() {
            let elapsed = now.duration_since(active.started).as_millis() as u64;
            if !active.animation.looped && elapsed >= active.animation.duration_ms {
                continue;
            }
            let interval =
                Duration::from_nanos(1_000_000_000 / u64::from(active.animation.fps.max(1)));
            let next_frame = elapsed / interval.as_millis().max(1) as u64 + 1;
            let deadline = active.started + interval.saturating_mul(next_frame as u32);
            timeout = timeout.min(deadline.saturating_duration_since(now));
        }
        if let Some(rate) = self
            .settings
            .modules
            .iter()
            .filter(|module| module.enabled && module.rpc && module.session && module.fps > 0)
            .map(|module| module.fps.clamp(1, 240))
            .max()
        {
            timeout = timeout.min(Duration::from_nanos(1_000_000_000 / u64::from(rate)));
        }
        timeout.max(Duration::from_millis(1))
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.view_mode == ViewMode::Compact && self.input_mode == InputMode::Normal {
            self.show_help = false;
            self.module_info = None;
            self.module_widget = None;
        }
        if self.input_mode == InputMode::Normal {
            if let Some((module_name, _)) = self.active_modal_canvas() {
                if key.code == KeyCode::Esc
                    || (key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c'))
                {
                    let _ = self
                        .module_runtime
                        .send_input(&self.settings, &module_name, "escape");
                    self.close_modal_canvas(&module_name);
                    return false;
                }
                if key.code == KeyCode::Char(':') {
                    self.input_mode = InputMode::Command;
                    self.input.clear();
                    self.input_prefix = ':';
                    return false;
                }
                if !self
                    .module_runtime
                    .send_input(&self.settings, &module_name, &key_name(&key))
                {
                    self.close_modal_canvas(&module_name);
                }
                return false;
            }
        }
        if self.module_widget.is_some() && self.input_mode == InputMode::Normal {
            if key.code == KeyCode::Esc
                || (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c'))
            {
                self.close_module_view();
                return false;
            }
            if key.code == KeyCode::Char(':') {
                self.input_mode = InputMode::Command;
                self.input.clear();
                self.input_prefix = ':';
                return false;
            }
            let module_name = self
                .module_widget
                .as_ref()
                .map(|widget| widget.module.clone())
                .unwrap_or_default();
            if !self
                .module_runtime
                .send_input(&self.settings, &module_name, &key_name(&key))
            {
                self.close_module_view();
            }
            return false;
        }
        if self.module_info.is_some() && self.input_mode == InputMode::Normal {
            if key.code == KeyCode::Esc {
                self.close_module_view();
                return false;
            }
            if key.code == KeyCode::Char(':') {
                self.input_mode = InputMode::Command;
                self.input.clear();
                self.input_prefix = ':';
                return false;
            }
            self.close_module_view();
            return false;
        }
        if self.show_help {
            self.show_help = false;
            return false;
        }
        match self.input_mode {
            InputMode::Normal => self.handle_normal(key),
            InputMode::Command => self.handle_input(key),
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> bool {
        let module_name = self
            .active_modal_canvas()
            .map(|(module_name, _)| module_name)
            .or_else(|| {
                self.module_widget
                    .as_ref()
                    .map(|widget| widget.module.clone())
            });
        let Some(module_name) = module_name else {
            return false;
        };
        let request = serde_json::json!({
            "api_version": 1,
            "type": "input",
            "event": "mouse",
            "mouse": mouse_request(&mouse)
        });
        if !self
            .module_runtime
            .send_input_event(&self.settings, &module_name, request)
        {
            self.close_modal_canvas(&module_name);
        }
        true
    }

    fn close_module_view(&mut self) {
        if let Some(widget) = self.module_widget.as_ref() {
            let _ = self
                .module_runtime
                .send_input(&self.settings, &widget.module, "escape");
        }
        if let Some((module_name, _)) = self.active_modal_canvas() {
            let _ = self
                .module_runtime
                .send_input(&self.settings, &module_name, "escape");
            self.close_modal_canvas(&module_name);
        }
        self.module_widget = None;
        self.module_info = None;
    }

    fn active_modal_canvas(&self) -> Option<(String, String)> {
        self.module_canvases
            .values()
            .filter(|canvas| canvas.modal)
            .map(|canvas| (canvas.z, canvas.module.clone(), canvas.id.clone()))
            .chain(
                self.module_animations
                    .values()
                    .filter(|animation| animation.animation.modal)
                    .map(|animation| {
                        (
                            animation.animation.z,
                            animation.animation.module.clone(),
                            animation.animation.id.clone(),
                        )
                    }),
            )
            .max_by_key(|candidate| candidate.clone())
            .map(|(_, module, id)| (module, id))
    }

    fn close_modal_canvas(&mut self, module_name: &str) {
        self.module_canvases
            .retain(|_, canvas| canvas.module != module_name || !canvas.modal);
        self.module_animations.retain(|_, animation| {
            animation.animation.module != module_name || !animation.animation.modal
        });
        if self.input_mode == InputMode::Command {
            self.input_mode = InputMode::Normal;
            self.input.clear();
            self.input_prefix = ':';
        }
    }

    fn handle_normal(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return true;
        }
        if configured_key(&key, &self.settings.keymap.quit) {
            return true;
        }
        if key.code == KeyCode::Char(' ')
            || (!self.settings.keymap.play.eq_ignore_ascii_case("enter")
                && configured_key(&key, &self.settings.keymap.play))
        {
            self.toggle_playback();
            return false;
        }
        if configured_key(&key, &self.settings.keymap.stop) {
            self.stop_playback();
            return false;
        }
        if configured_key(&key, &self.settings.keymap.search) {
            self.input_mode = InputMode::Command;
            self.input = "search ".into();
            self.input_prefix = '/';
            return false;
        }
        if self.view_mode == ViewMode::Compact {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.compact_step(1);
                    return false;
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.compact_step(-1);
                    return false;
                }
                KeyCode::Char('h') => {
                    self.seek(-(self.settings.skip_seconds as i64));
                    return false;
                }
                KeyCode::Char('l') => {
                    self.seek(self.settings.skip_seconds as i64);
                    return false;
                }
                _ => {}
            }
        }
        let key_name = key_name(&key);
        if let Some(action) = modules::key_action(&self.settings, &key_name) {
            self.execute_module_action(&action);
            return false;
        }
        match key.code {
            KeyCode::Esc => {
                self.pending_g = false;
                false
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_selection(1);
                false
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_selection(-1);
                false
            }
            KeyCode::Char('g') => {
                if self.pending_g {
                    self.selected = 0;
                    self.pending_g = false;
                } else {
                    self.pending_g = true;
                }
                false
            }
            KeyCode::Char('G') => {
                if !self.visible.is_empty() {
                    self.selected = self.visible.len() - 1;
                }
                false
            }
            KeyCode::Char(']') | KeyCode::Char('n') => {
                self.play_next();
                false
            }
            KeyCode::Char('[') => {
                self.play_previous();
                false
            }
            KeyCode::Char('h') => {
                self.seek(-(self.settings.skip_seconds as i64));
                false
            }
            KeyCode::Char('l') => {
                self.seek(self.settings.skip_seconds as i64);
                false
            }
            KeyCode::Char('a') => {
                self.enqueue_selected();
                false
            }
            KeyCode::Char(':') => {
                self.input_mode = InputMode::Command;
                self.input.clear();
                self.input_prefix = ':';
                false
            }
            KeyCode::Char('?') => {
                self.show_help = !self.show_help;
                false
            }
            _ => {
                self.pending_g = false;
                false
            }
        }
    }

    fn handle_input(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input.clear();
                self.input_prefix = ':';
            }
            KeyCode::Enter => {
                self.execute_command();
                self.input.clear();
                self.input_mode = InputMode::Normal;
                self.input_prefix = ':';
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Tab => self.complete_command(),
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.push(character)
            }
            _ => {}
        }
        false
    }

    fn complete_command(&mut self) {
        let input = self.input.clone();
        let trimmed = input.trim_start();
        if !trimmed.contains(char::is_whitespace) {
            let prefix = trimmed.to_ascii_lowercase();
            let matches = command_names()
                .iter()
                .filter(|name| name.starts_with(&prefix))
                .map(|name| (*name).to_string())
                .collect::<Vec<_>>();
            self.apply_word_completion(&input, trimmed, matches);
            return;
        }

        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("modules") {
            let after_modules = trimmed[7..].trim_start();
            if !after_modules.contains(char::is_whitespace) {
                let prefix = after_modules.to_ascii_lowercase();
                let matches = ["info", "list", "off", "on"]
                    .iter()
                    .filter(|name| name.starts_with(&prefix))
                    .map(|name| (*name).to_string())
                    .collect::<Vec<_>>();
                self.apply_word_completion(&input, after_modules, matches);
                return;
            }
            let mut words = trimmed.split_whitespace();
            let _ = words.next();
            let action = words.next().unwrap_or("").to_ascii_lowercase();
            let prefix = words.next().unwrap_or("");
            if matches!(action.as_str(), "on" | "off" | "info") {
                let names = self
                    .settings
                    .modules
                    .iter()
                    .map(|module| module.name.clone())
                    .filter(|name| {
                        name.to_ascii_lowercase()
                            .starts_with(&prefix.to_ascii_lowercase())
                    })
                    .collect::<Vec<_>>();
                self.apply_word_completion(&input, prefix, names);
            }
            return;
        }
        if lower.starts_with("mode") {
            let after_mode = trimmed[4..].trim_start();
            if !after_mode.contains(char::is_whitespace) {
                let prefix = after_mode.to_ascii_lowercase();
                let matches = ["compact", "normal"]
                    .iter()
                    .filter(|name| name.starts_with(&prefix))
                    .map(|name| (*name).to_string())
                    .collect::<Vec<_>>();
                self.apply_word_completion(&input, after_mode, matches);
            }
            return;
        }
        if lower.starts_with("follow") {
            let after_follow = trimmed[6..].trim_start();
            if !after_follow.contains(char::is_whitespace) {
                let prefix = after_follow.to_ascii_lowercase();
                let matches = ["off", "on"]
                    .iter()
                    .filter(|name| name.starts_with(&prefix))
                    .map(|name| (*name).to_string())
                    .collect::<Vec<_>>();
                self.apply_word_completion(&input, after_follow, matches);
            }
            return;
        }
        if lower.starts_with("set") {
            let after_set = trimmed[3..].trim_start();
            if !after_set.contains(char::is_whitespace) {
                let prefix = after_set.to_ascii_lowercase();
                let matches = set_field_names()
                    .iter()
                    .filter(|name| name.starts_with(&prefix))
                    .map(|name| (*name).to_string())
                    .collect::<Vec<_>>();
                let base = format!("set {}", after_set);
                self.apply_word_completion(&input, &base[4..], matches);
                return;
            }
            let mut words = trimmed.splitn(3, char::is_whitespace);
            let _ = words.next();
            let field = words.next().unwrap_or("").to_ascii_lowercase();
            if field == "image" || field == "lyrics" {
                self.complete_path(&input, &field);
            }
        }
    }

    fn apply_word_completion(&mut self, input: &str, current: &str, matches: Vec<String>) {
        if matches.is_empty() {
            self.status = "no completion".into();
            return;
        }
        let common = common_prefix(&matches);
        let current_start = input.len() - current.len();
        let completed = if matches.len() == 1 {
            format!("{} ", matches[0])
        } else {
            common.clone()
        };
        self.input = format!("{}{}", &input[..current_start], completed);
        if matches.len() > 1 {
            self.status = matches.join("  ");
        } else {
            self.status.clear();
        }
    }

    fn complete_path(&mut self, input: &str, field: &str) {
        let marker = format!("set {field}");
        let Some(marker_start) = input.to_ascii_lowercase().find(&marker) else {
            return;
        };
        let value_start = marker_start + marker.len() + input[marker_start + marker.len()..].len()
            - input[marker_start + marker.len()..].trim_start().len();
        let typed = &input[value_start..];
        let quoted = typed.starts_with('"') || typed.starts_with('\'');
        let raw_typed = typed
            .strip_prefix('"')
            .or_else(|| typed.strip_prefix('\''))
            .unwrap_or(typed);
        let matches = path_matches(raw_typed);
        if matches.is_empty() {
            self.status = "no path completion".into();
            return;
        }
        let common = common_prefix(
            &matches
                .iter()
                .map(|(path, _)| path.clone())
                .collect::<Vec<_>>(),
        );
        let selected = matches.len() == 1;
        let mut rendered = if selected {
            matches[0].0.clone()
        } else {
            common
        };
        if quoted || rendered.contains(' ') {
            rendered = format!("\"{}\"", rendered.replace('"', "\\\""));
        }
        self.input = format!("{}{}", &input[..value_start], rendered);
        if selected && !matches[0].1 {
            self.input.push(' ');
        }
        if matches.len() > 1 {
            self.status = matches
                .iter()
                .map(|(path, _)| path.clone())
                .collect::<Vec<_>>()
                .join("  ");
        } else {
            self.status.clear();
        }
    }

    fn move_selection(&mut self, delta: i32) {
        self.pending_g = false;
        if self.visible.is_empty() {
            return;
        }
        let current = self.selected as i32;
        self.selected = (current + delta).clamp(0, self.visible.len() as i32 - 1) as usize;
    }

    fn play_selected(&mut self) {
        let Some(index) = self.visible.get(self.selected).copied() else {
            self.status = "library is empty".into();
            return;
        };
        if self.queue.is_empty() {
            self.queue = self.visible.clone();
        }
        if !self.queue.contains(&index) {
            self.queue.push(index);
        }
        self.queue_position = self.queue.iter().position(|item| *item == index);
        self.start_song_at(index, 0);
    }

    fn start_song_at(&mut self, index: usize, offset_seconds: u64) {
        let audio_filter = self.audio_filter.clone();
        self.start_song_at_with_filter(index, offset_seconds, audio_filter);
    }

    fn start_song_at_with_filter(
        &mut self,
        index: usize,
        offset_seconds: u64,
        audio_filter: Option<String>,
    ) {
        player::stop(&mut self.player);
        self.paused = false;
        self.paused_at = None;
        let song = &self.songs[index];
        let player_ipc = self.paths.player_ipc_path();
        let player = player::start_with_filter_and_control(
            &self.settings,
            &song.audio,
            offset_seconds,
            audio_filter.as_deref(),
            Some(&player_ipc),
        );
        match player {
            Ok(child) => {
                self.player = Some(child);
                self.audio_filter = audio_filter;
                self.playing = Some(index);
                if self.follow_playback {
                    if let Some(selected) = self.visible.iter().position(|item| *item == index) {
                        self.selected = selected;
                    }
                }
                self.playback_started = Some(Instant::now());
                self.playback_offset_seconds = offset_seconds;
                self.status = format!("playing {} — {}", song.info.artist, song.info.title);
                self.module_runtime.emit(&self.settings, "play", Some(song));
            }
            Err(error) => {
                self.playback_started = None;
                self.playback_offset_seconds = 0;
                self.paused = false;
                self.paused_at = None;
                self.status = error;
            }
        }
    }

    fn toggle_playback(&mut self) {
        let selected = self.visible.get(self.selected).copied();
        if self.player.is_none() || (selected.is_some() && selected != self.playing) {
            self.play_selected();
            return;
        }
        let Some(player_process) = self.player.as_ref() else {
            return;
        };
        if self.paused {
            match player::resume(player_process) {
                Ok(()) => {
                    if let (Some(started), Some(paused_at)) =
                        (self.playback_started, self.paused_at)
                    {
                        self.playback_started = Some(started + paused_at.elapsed());
                    }
                    self.paused = false;
                    self.paused_at = None;
                    self.status = "resumed".into();
                }
                Err(error) => self.status = error,
            }
        } else {
            match player::pause(player_process) {
                Ok(()) => {
                    self.paused = true;
                    self.paused_at = Some(Instant::now());
                    self.status = "paused".into();
                }
                Err(error) => self.status = error,
            }
        }
    }

    fn stop_playback(&mut self) {
        let current = self.playing.and_then(|index| self.songs.get(index));
        self.module_runtime.emit(&self.settings, "stop", current);
        player::stop(&mut self.player);
        player::clear_control_endpoint(&self.paths.player_ipc_path());
        self.playing = None;
        self.playback_started = None;
        self.playback_offset_seconds = 0;
        self.paused = false;
        self.paused_at = None;
        self.status = "stopped".into();
    }

    fn seek(&mut self, delta_seconds: i64) {
        let Some(index) = self.playing else {
            self.status = "nothing is playing".into();
            return;
        };
        let current = self.playback_elapsed();
        let duration = self.songs[index].info.duration_seconds;
        let target = if duration == 0 {
            current.saturating_add_signed(delta_seconds)
        } else {
            current
                .saturating_add_signed(delta_seconds)
                .min(duration.saturating_sub(1))
        };
        self.start_song_at(index, target);
        self.status = format!("seeked to {}", format_time(target));
    }

    fn playback_elapsed(&self) -> u64 {
        self.playback_offset_seconds
            + self
                .playback_started
                .map(|started| {
                    self.paused_at
                        .map(|paused_at| paused_at.duration_since(started).as_secs())
                        .unwrap_or_else(|| started.elapsed().as_secs())
                })
                .unwrap_or(0)
    }

    fn timeline_song(&self) -> Option<&Song> {
        self.playing
            .and_then(|index| self.songs.get(index))
            .or_else(|| self.selected_song())
    }

    fn play_next(&mut self) {
        let Some(position) = self.queue_position else {
            self.status = "queue is empty".into();
            self.playing = None;
            return;
        };
        let Some(next) = next_visible_queue_position(&self.queue, position, &self.visible) else {
            self.playing = None;
            self.status = "queue finished".into();
            return;
        };
        self.queue_position = Some(next);
        self.start_song_at(self.queue[next], 0);
    }

    fn play_previous(&mut self) {
        let Some(position) = self.queue_position else {
            self.status = "queue is empty".into();
            return;
        };
        if position == 0 {
            self.status = "already at queue start".into();
            return;
        }
        let previous = position - 1;
        self.queue_position = Some(previous);
        self.start_song_at(self.queue[previous], 0);
    }

    fn enqueue_selected(&mut self) {
        let Some(index) = self.visible.get(self.selected).copied() else {
            self.status = "library is empty".into();
            return;
        };
        if !self.queue.contains(&index) {
            self.queue.push(index);
        }
        self.status = format!("queued {}", self.songs[index].info.title);
    }

    fn compact_step(&mut self, delta: i32) {
        if self.visible.is_empty() {
            self.status = "library is empty".into();
            return;
        }
        if self.queue.is_empty() {
            self.queue = self.visible.clone();
        }
        let current = self
            .playing
            .or_else(|| self.visible.get(self.selected).copied());
        if self.playing.is_none() && current.is_some_and(|index| !self.queue.contains(&index)) {
            self.queue = self.visible.clone();
        }
        let position = if self.playing.is_some() {
            self.queue_position.or_else(|| {
                current.and_then(|index| self.queue.iter().position(|item| *item == index))
            })
        } else {
            current.and_then(|index| self.queue.iter().position(|item| *item == index))
        };
        let Some(position) = position else {
            return;
        };
        let target = if delta > 0 {
            next_visible_queue_position(&self.queue, position, &self.visible)
        } else {
            previous_visible_queue_position(&self.queue, position, &self.visible)
        };
        let Some(target) = target else {
            self.status = if delta > 0 {
                "already at queue end".into()
            } else {
                "already at queue start".into()
            };
            return;
        };
        self.queue_position = Some(target);
        let index = self.queue[target];
        if let Some(selected) = self.visible.iter().position(|item| *item == index) {
            self.selected = selected;
        }
        self.start_song_at(index, 0);
    }

    fn execute_command(&mut self) {
        let input = self.input.trim().to_string();
        let tokens = match parse_command_line(&input) {
            Ok(tokens) => tokens,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let command = tokens.first().map(String::as_str).unwrap_or("");
        let args = tokens.get(1..).unwrap_or(&[]);
        if matches!(
            command.to_ascii_lowercase().as_str(),
            "q" | "quit" | "close"
        ) && (self.module_widget.is_some()
            || self.module_info.is_some()
            || self.active_modal_canvas().is_some())
        {
            self.close_module_view();
            self.status = "module view closed".into();
            return;
        }
        if command.eq_ignore_ascii_case("mode") {
            self.set_view_mode(args);
            return;
        }
        if command.eq_ignore_ascii_case("follow") {
            self.set_follow_playback(args);
            return;
        }
        if command.eq_ignore_ascii_case("modules") {
            self.handle_modules_command(args);
            return;
        }
        if self.request_terminal_module(command, args) {
            return;
        }
        if let Some(result) = self.module_runtime.dispatch_command(
            &self.settings,
            command,
            args,
            self.selected_song(),
        ) {
            self.status = result.unwrap_or_else(|error| error);
            return;
        }
        match command {
            "q" | "quit" => self.should_quit = true,
            "close" => self.status = "no module view is open".into(),
            "tag" => {
                let tags = split_tag_args(args);
                if tags.is_empty() {
                    self.status = "usage: :tag name".into();
                } else {
                    self.update_selected_info("metadata", move |info| {
                        for tag in &tags {
                            info.add_tag(tag);
                        }
                    });
                }
            }
            "untag" => {
                let tags = split_tag_args(args);
                if tags.is_empty() {
                    self.status = "usage: :untag name".into();
                } else {
                    self.update_selected_info("metadata", move |info| {
                        for tag in &tags {
                            info.remove_tag(tag);
                        }
                    });
                }
            }
            "tags" => {
                self.tags = split_tag_args(args);
                self.refresh_visible();
                self.status = if self.tags.is_empty() {
                    "tag filter cleared".into()
                } else {
                    format!("tag filter: {}", self.tags.join(", "))
                };
            }
            "clear" => {
                self.tags.clear();
                self.query.clear();
                self.refresh_visible();
                self.status = "filters cleared".into();
            }
            "search" => {
                self.query = args.join(" ");
                self.refresh_visible();
                self.status = format!("search: {}", self.query);
            }
            "themes" => {
                let mut names = config::builtin_themes()
                    .iter()
                    .map(|name| (*name).to_string())
                    .collect::<Vec<_>>();
                names.extend(self.settings.themes.keys().cloned());
                self.status = format!(
                    "themes: {} (active: {})",
                    names.join(", "),
                    self.settings.theme
                );
            }
            "play" => self.toggle_playback(),
            "queue" => match args.first().map(String::as_str) {
                Some("clear") => {
                    self.queue.clear();
                    self.queue_position = None;
                    self.status = "queue cleared".into();
                }
                Some("visible") | None => {
                    self.queue = self.visible.clone();
                    self.queue_position = self
                        .playing
                        .and_then(|playing| self.queue.iter().position(|item| *item == playing));
                    self.status = format!("queue: {} song(s)", self.queue.len());
                }
                Some(other) => self.status = format!("unknown queue option: {other}"),
            },
            "enqueue" => self.enqueue_selected(),
            "next" => self.play_next(),
            "prev" | "previous" => self.play_previous(),
            "stop" => self.stop_playback(),
            "repeat" => self.set_repeat(args),
            "rescan" => match scan(&self.paths) {
                Ok(songs) => {
                    player::stop(&mut self.player);
                    player::clear_control_endpoint(&self.paths.player_ipc_path());
                    self.songs = songs;
                    self.queue.clear();
                    self.queue_position = None;
                    self.playing = None;
                    self.playback_started = None;
                    self.playback_offset_seconds = 0;
                    self.paused = false;
                    self.paused_at = None;
                    self.refresh_visible();
                    self.status = "library rescanned".into();
                    self.module_runtime.emit(&self.settings, "rescan", None);
                }
                Err(error) => self.status = error.to_string(),
            },
            "reload" => match config::load(&self.paths) {
                Ok(settings) => {
                    self.close_module_view();
                    self.module_runtime.shutdown(&self.settings);
                    self.repeat = settings.repeat;
                    self.follow_playback = settings.follow_playback;
                    self.settings = settings;
                    self.module_border = None;
                    self.module_canvases.clear();
                    self.module_animations.clear();
                    self.configure_image_picker();
                    self.status = "configuration and modules reloaded".into();
                }
                Err(error) => self.status = error.to_string(),
            },
            "set" => {
                if args
                    .first()
                    .map(|value| value.eq_ignore_ascii_case("skips"))
                    .unwrap_or(false)
                {
                    self.set_skip_seconds(args);
                } else if args
                    .first()
                    .map(|value| {
                        value.eq_ignore_ascii_case("coversize")
                            || value.eq_ignore_ascii_case("cover_size")
                    })
                    .unwrap_or(false)
                {
                    self.set_cover_size(args);
                } else if args
                    .first()
                    .map(|value| value.eq_ignore_ascii_case("theme"))
                    .unwrap_or(false)
                {
                    self.set_theme(args);
                } else {
                    self.set_selected_field(args);
                }
            }
            _ => {
                self.status = "commands: :mode, :search, :tag, :untag, :tags, :clear, :play, :stop, :repeat, :follow, :queue, :next, :prev, :set, :themes, :reload, :close".into();
            }
        }
    }

    fn handle_modules_command(&mut self, args: &[String]) {
        let action = args.first().map(|value| value.to_ascii_lowercase());
        match action.as_deref() {
            None | Some("list") => {
                if self.settings.modules.is_empty() {
                    self.module_info = None;
                    self.status = "no modules installed".into();
                    return;
                }
                let modules = self
                    .settings
                    .modules
                    .iter()
                    .map(|module| {
                        format!(
                            "{}  {}  {}",
                            module.name,
                            if module.enabled { "on" } else { "off" },
                            if module.version.is_empty() {
                                "(no version)"
                            } else {
                                &module.version
                            }
                        )
                    })
                    .collect::<Vec<_>>();
                self.module_info = Some(format!("installed modules\n\n{}", modules.join("\n")));
                self.status = "modules (press any key to close)".into();
            }
            Some(command) if command == "on" || command == "off" => {
                let Some(name) = args.get(1) else {
                    self.status = format!("usage: :modules {command} module_name");
                    return;
                };
                let enabled = command == "on";
                let Some(module) = self
                    .settings
                    .modules
                    .iter_mut()
                    .find(|module| module.name.eq_ignore_ascii_case(name))
                else {
                    self.status = format!("module not found: {name}");
                    return;
                };
                module.enabled = enabled;
                let module_name = module.name.clone();
                match config::write_settings(&self.paths, &self.settings) {
                    Ok(()) => {
                        self.status = format!(
                            "module {module_name}: {}",
                            if enabled { "on" } else { "off" }
                        )
                    }
                    Err(error) => self.status = error.to_string(),
                }
            }
            Some("info") => {
                let Some(name) = args.get(1) else {
                    self.status = "usage: :modules info module_name".into();
                    return;
                };
                let Some(module) = self
                    .settings
                    .modules
                    .iter()
                    .find(|module| module.name.eq_ignore_ascii_case(name))
                else {
                    self.status = format!("module not found: {name}");
                    return;
                };
                self.module_info = Some(self.format_module_info(module));
                self.status = format!("module info: {name} (press any key to close)");
            }
            Some("reload") => {
                self.input = "reload".into();
                self.execute_command();
                self.input.clear();
            }
            Some(_) => {
                self.status = "usage: :modules [on|off|info] [module_name]".into();
            }
        }
    }

    fn format_module_info(&self, module: &config::ModuleConfig) -> String {
        let source = module
            .manifest_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| self.paths.settings_file.display().to_string());
        let json = serde_json::to_string_pretty(module).unwrap_or_else(|_| "{}".into());
        format!(
            "module: {}\nsource: {source}\nworking directory: {}\n\n{json}",
            module.name,
            module.cwd.as_deref().unwrap_or("(inherited)")
        )
    }

    fn set_selected_field(&mut self, args: &[String]) {
        let Some(field) = args.first().map(|value| value.to_ascii_lowercase()) else {
            self.status = "usage: :set field value".into();
            return;
        };
        let values = &args[1..];
        let Some(index) = self.visible.get(self.selected).copied() else {
            self.status = "library is empty".into();
            return;
        };
        let (path, id, directory, mut info) = {
            let song = &self.songs[index];
            (
                song.directory.join("info.json"),
                song.info.id.clone(),
                song.directory.clone(),
                song.info.clone(),
            )
        };
        let result = match field.as_str() {
            "title" => set_text_field(values, "title", |value| info.title = value),
            "artist" => set_text_field(values, "artist", |value| info.artist = value),
            "album" => set_text_field(values, "album", |value| info.album = value),
            "desc" | "description" => {
                set_text_field(values, "description", |value| info.description = value)
            }
            "image" => {
                let Some(source) = values.first() else {
                    return self.status = "usage: :set image path".into();
                };
                match copy_cover_for_song(&directory, source) {
                    Ok(name) => {
                        info.image = name;
                        Ok(())
                    }
                    Err(error) => Err(error),
                }
            }
            "lyrics" => {
                let Some(source) = values.first() else {
                    return self.status = "usage: :set lyrics path".into();
                };
                let source_path = expand_tilde(source);
                let destination = directory.join("lyrics.txt");
                if !source_path.is_file() {
                    Err(format!("lyrics not found: {source}"))
                } else {
                    if source_path != destination {
                        if let Err(error) = std::fs::copy(&source_path, &destination) {
                            return self.status = format!("cannot copy lyrics: {error}");
                        }
                    }
                    info.lyrics = "lyrics.txt".into();
                    Ok(())
                }
            }
            "duration" => {
                let Some(value) = values.first() else {
                    return self.status = "usage: :set duration seconds".into();
                };
                match value.parse::<u64>() {
                    Ok(seconds) => {
                        info.duration_seconds = seconds;
                        Ok(())
                    }
                    Err(_) => Err("duration must be an unsigned integer".into()),
                }
            }
            _ => Err(format!(
                "unknown set field '{field}'; use title, artist, album, desc, image, lyrics, or duration"
            )),
        };
        if let Err(error) = result {
            self.status = error;
            return;
        }
        let info = info.normalized(&id);
        match write_info_atomic(&path, &info) {
            Ok(()) => {
                self.songs[index].info = info;
                self.songs[index].refresh_optional_files();
                self.refresh_visible();
                self.status = format!("updated song {id}");
                self.module_runtime
                    .emit(&self.settings, "metadata", self.songs.get(index));
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn set_skip_seconds(&mut self, args: &[String]) {
        let Some(value) = args.get(1) else {
            self.status = "usage: :set skips seconds".into();
            return;
        };
        let Ok(seconds) = value.parse::<u64>() else {
            self.status = "skip interval must be an unsigned integer".into();
            return;
        };
        if seconds == 0 {
            self.status = "skip interval must be greater than zero".into();
            return;
        }
        self.settings.skip_seconds = seconds;
        match config::write_settings(&self.paths, &self.settings) {
            Ok(()) => self.status = format!("skip interval: {seconds}s"),
            Err(error) => self.status = error.to_string(),
        }
    }

    fn set_repeat(&mut self, args: &[String]) {
        let Some(value) = args.first().map(|value| value.to_ascii_lowercase()) else {
            self.status = format!(
                "repeat: {} (use :repeat on|off)",
                if self.repeat { "on" } else { "off" }
            );
            return;
        };
        let enabled = match value.as_str() {
            "on" | "1" | "true" => true,
            "off" | "0" | "false" => false,
            _ => {
                self.status = "usage: :repeat on|off".into();
                return;
            }
        };
        self.repeat = enabled;
        self.settings.repeat = enabled;
        match config::write_settings(&self.paths, &self.settings) {
            Ok(()) => self.status = format!("repeat: {}", if enabled { "on" } else { "off" }),
            Err(error) => self.status = error.to_string(),
        }
    }

    fn set_follow_playback(&mut self, args: &[String]) {
        let Some(value) = args.first().map(|value| value.to_ascii_lowercase()) else {
            self.status = format!(
                "follow: {} (use :follow on|off)",
                if self.follow_playback { "on" } else { "off" }
            );
            return;
        };
        if args.len() != 1 {
            self.status = "usage: :follow on|off".into();
            return;
        }
        let enabled = match value.as_str() {
            "on" | "1" | "true" => true,
            "off" | "0" | "false" => false,
            _ => {
                self.status = "usage: :follow on|off".into();
                return;
            }
        };
        self.follow_playback = enabled;
        self.settings.follow_playback = enabled;
        match config::write_settings(&self.paths, &self.settings) {
            Ok(()) => self.status = format!("follow: {}", if enabled { "on" } else { "off" }),
            Err(error) => self.status = error.to_string(),
        }
    }

    fn set_view_mode(&mut self, args: &[String]) {
        let Some(value) = args.first().map(|value| value.to_ascii_lowercase()) else {
            self.status = format!(
                "mode: {} (use :mode normal|compact)",
                view_mode_name(self.view_mode).to_ascii_lowercase()
            );
            return;
        };
        if args.len() != 1 {
            self.status = "usage: :mode normal|compact".into();
            return;
        }
        let mode = match value.as_str() {
            "normal" => ViewMode::Normal,
            "compact" => ViewMode::Compact,
            _ => {
                self.status = "usage: :mode normal|compact".into();
                return;
            }
        };
        self.view_mode = mode;
        self.show_help = false;
        self.module_info = None;
        self.module_widget = None;
        self.image_protocol = None;
        self.image_path = None;
        self.image_cells = None;
        self.image_pending = false;
        self.image_generation = self.image_generation.wrapping_add(1);
        self.status = format!("mode: {}", view_mode_name(mode).to_ascii_lowercase());
    }

    fn set_cover_size(&mut self, args: &[String]) {
        let Some(value) = args.get(1) else {
            self.status = "usage: :set coversize cells".into();
            return;
        };
        let Ok(size) = value.parse::<u16>() else {
            self.status = "cover size must be an unsigned integer".into();
            return;
        };
        if !(4..=64).contains(&size) {
            self.status = "cover size must be between 4 and 64 cells".into();
            return;
        }
        self.settings.cover_size = size;
        self.image_protocol = None;
        self.image_path = None;
        self.image_cells = None;
        self.image_pending = false;
        self.image_generation = self.image_generation.wrapping_add(1);
        match config::write_settings(&self.paths, &self.settings) {
            Ok(()) => self.status = format!("cover size: {size} cells"),
            Err(error) => self.status = error.to_string(),
        }
    }

    fn set_theme(&mut self, args: &[String]) {
        let Some(name) = args.get(1).map(|value| value.to_ascii_lowercase()) else {
            self.status = "usage: :set theme name".into();
            return;
        };
        if !config::theme_exists(&self.settings, &name) {
            self.status = format!("unknown theme '{name}'; use :themes or install a theme JSON");
            return;
        }
        self.settings.theme = name.clone();
        match config::write_settings(&self.paths, &self.settings) {
            Ok(()) => self.status = format!("theme: {name}"),
            Err(error) => self.status = error.to_string(),
        }
    }

    fn execute_module_action(&mut self, action: &str) {
        let Ok(tokens) = parse_command_line(action) else {
            self.status = "module key action has invalid quoting".into();
            return;
        };
        let Some(command) = tokens.first() else {
            return;
        };
        if self.request_terminal_module(command, tokens.get(1..).unwrap_or(&[])) {
            return;
        }
        let result = self.module_runtime.dispatch_command(
            &self.settings,
            command,
            tokens.get(1..).unwrap_or(&[]),
            self.selected_song(),
        );
        if result.is_none() {
            self.input = action.to_string();
            self.execute_command();
            self.input.clear();
            return;
        }
        self.status = match result {
            Some(Ok(status)) => status,
            Some(Err(error)) => error,
            None => unreachable!(),
        };
    }

    fn request_terminal_module(&mut self, command: &str, args: &[String]) -> bool {
        let Some(result) = self.module_runtime.terminal_command(
            &self.settings,
            command,
            args,
            self.selected_song(),
        ) else {
            return false;
        };
        match result {
            Ok(command) => {
                self.status = format!("starting terminal module {}", command.module);
                self.pending_terminal = Some(command);
            }
            Err(error) => self.status = error,
        }
        true
    }

    fn update_selected_info<F>(&mut self, event: &str, update: F)
    where
        F: FnOnce(&mut SongInfo),
    {
        let Some(index) = self.visible.get(self.selected).copied() else {
            self.status = "library is empty".into();
            return;
        };
        let (path, id, mut info) = {
            let song = &self.songs[index];
            (
                song.directory.join("info.json"),
                song.info.id.clone(),
                song.info.clone(),
            )
        };
        update(&mut info);
        let info = info.normalized(&id);
        match write_info_atomic(&path, &info) {
            Ok(()) => {
                self.songs[index].info = info;
                self.songs[index].refresh_optional_files();
                self.refresh_visible();
                self.status = format!("updated song {id}");
                self.module_runtime
                    .emit(&self.settings, event, self.songs.get(index));
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn prepare_image(&mut self, area: Rect, path: Option<PathBuf>) {
        if self.settings.image_mode.eq_ignore_ascii_case("none") {
            self.image_protocol = None;
            self.image_path = None;
            self.image_cells = None;
            self.image_pending = false;
            self.image_generation = self.image_generation.wrapping_add(1);
            return;
        }
        if area.width == 0 || area.height == 0 {
            self.image_protocol = None;
            self.image_path = None;
            self.image_cells = None;
            self.image_pending = false;
            self.image_generation = self.image_generation.wrapping_add(1);
            return;
        }
        let cells = (area.width, area.height);
        if path == self.image_path && self.image_cells == Some(cells) {
            return;
        }
        self.image_path = path.clone();
        self.image_cells = Some(cells);
        self.image_protocol = None;
        self.image_generation = self.image_generation.wrapping_add(1);
        if let Some(path) = path {
            self.image_pending = true;
            let request = ImageRequest {
                generation: self.image_generation,
                path,
                picker: self.image_picker,
                cells,
            };
            if self.image_requests.send(request).is_err() {
                self.image_pending = false;
            }
        } else {
            self.image_pending = false;
        }
    }

    fn receive_image_results(&mut self) -> bool {
        let mut changed = false;
        while let Ok(result) = self.image_results.try_recv() {
            if result.generation == self.image_generation
                && Some(&result.path) == self.image_path.as_ref()
            {
                self.image_protocol = result.protocol;
                self.image_pending = false;
                changed = true;
            }
        }
        changed
    }

    fn receive_module_outputs(&mut self) -> bool {
        let mut changed = false;
        let mut latest_layers = HashMap::new();
        while let Ok(output) = self.module_outputs.try_recv() {
            if matches!(output.kind.as_str(), "canvas" | "animation") {
                let layer_id = output
                    .data
                    .get("id")
                    .or_else(|| output.data.get("layer"))
                    .or_else(|| {
                        output
                            .data
                            .get("animation")
                            .and_then(|value| value.get("id"))
                    })
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                latest_layers.insert(
                    format!("{}::{}::{layer_id}", output.module, output.kind),
                    output,
                );
                continue;
            }
            changed |= output.flush;
            if output.kind == "canvas_patch" {
                changed |= self.apply_canvas_patch(&output.module, &output.data);
                continue;
            }
            match output.kind.as_str() {
                "status" => {
                    self.status = if output.text.is_empty() {
                        format!("module {} completed", output.module)
                    } else {
                        output.text
                    };
                }
                "error" => {
                    self.status = if output.text.is_empty() {
                        format!("module {} reported an error", output.module)
                    } else {
                        format!("module {}: {}", output.module, output.text)
                    };
                }
                "overlay" => {
                    let body = if output.text.is_empty() {
                        output.lines.join("\n")
                    } else {
                        output.text
                    };
                    self.module_info = Some(format!("module: {}\n\n{}", output.module, body));
                }
                "paragraph" | "list" | "gauge" | "view" => {
                    self.module_widget = output.widget.or_else(|| {
                        Some(modules::ModuleWidgetNode {
                            module: output.module,
                            kind: output.kind,
                            title: output.title,
                            text: output.text,
                            lines: output.lines,
                            value: output.value,
                            max: output.max,
                            direction: "column".into(),
                            flex: 1,
                            children: Vec::new(),
                        })
                    });
                }
                "palette" => {
                    if output
                        .data
                        .get("clear")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                    {
                        self.module_border = None;
                    } else if let Some(border) = output
                        .data
                        .get("border")
                        .and_then(serde_json::Value::as_str)
                    {
                        self.module_border = Some(parse_color(border, Color::Gray));
                    }
                }
                "close" => {
                    self.module_info = None;
                    self.module_widget = None;
                    self.module_canvases
                        .retain(|_, canvas| canvas.module != output.module || !canvas.modal);
                    self.module_animations.retain(|_, animation| {
                        animation.animation.module != output.module || !animation.animation.modal
                    });
                }
                "playback" => self.handle_module_playback(&output.module, &output.data),
                "rescan" => match scan(&self.paths) {
                    Ok(songs) => {
                        self.songs = songs;
                        self.refresh_visible();
                        self.status = "library rescanned by module".into();
                    }
                    Err(error) => self.status = error.to_string(),
                },
                "redraw" => {}
                _ => {
                    self.status = format!(
                        "module {} sent unknown view type {}: {}",
                        output.module, output.kind, output.data
                    );
                }
            }
        }
        for output in latest_layers.into_values() {
            changed |= output.flush;
            changed |= self.apply_module_layer_output(&output);
        }
        changed
    }

    fn apply_module_layer_output(&mut self, output: &modules::ModuleOutput) -> bool {
        if output.kind == "canvas" {
            if let Some(canvas) = modules::ModuleCanvas::from_value(&output.module, &output.data) {
                let key = format!("{}::{}", canvas.module, canvas.id);
                if canvas.clear {
                    self.module_canvases.remove(&key);
                } else {
                    self.module_animations.remove(&key);
                    self.module_canvases.insert(key, canvas);
                }
            } else {
                self.status = format!("module {} sent an invalid canvas", output.module);
            }
            return true;
        }
        if let Some(animation) = modules::ModuleAnimation::from_value(&output.module, &output.data)
        {
            let key = format!("{}::{}", animation.module, animation.id);
            if animation.clear {
                self.module_animations.remove(&key);
                self.module_canvases.remove(&key);
            } else {
                self.module_canvases.remove(&key);
                self.module_animations.insert(
                    key,
                    ActiveAnimation {
                        animation,
                        started: Instant::now(),
                        frame_index: 0,
                    },
                );
            }
        } else {
            self.status = format!("module {} sent an invalid animation", output.module);
        }
        true
    }

    fn apply_canvas_patch(&mut self, module: &str, data: &serde_json::Value) -> bool {
        let Some(object) = data.as_object() else {
            self.status = format!("module {module} sent an invalid canvas patch");
            return true;
        };
        let id = object
            .get("id")
            .or_else(|| object.get("layer"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(module);
        if id.is_empty() || id.len() > 128 {
            self.status = format!("module {module} sent an invalid canvas patch id");
            return true;
        }
        let key = format!("{module}::{id}");
        if object
            .get("clear")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            self.module_canvases.remove(&key);
            return true;
        }
        self.module_animations.remove(&key);
        if !self.module_canvases.contains_key(&key) {
            let seed = serde_json::json!({
                "type": "canvas",
                "id": id,
                "z": object.get("z").cloned().unwrap_or_else(|| serde_json::json!(0)),
                "modal": object.get("modal").cloned().unwrap_or(serde_json::json!(false)),
                "opaque": object.get("opaque").cloned().unwrap_or(serde_json::json!(false)),
                "rect": object.get("rect").cloned().unwrap_or_else(|| serde_json::json!({})),
                "commands": []
            });
            if let Some(canvas) = modules::ModuleCanvas::from_value(module, &seed) {
                self.module_canvases.insert(key.clone(), canvas);
            }
        }
        let Some(canvas) = self.module_canvases.get_mut(&key) else {
            return false;
        };
        if let Some(z) = object.get("z").and_then(serde_json::Value::as_i64) {
            canvas.z = z.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        }
        if let Some(rect) = object.get("rect") {
            canvas.rect = rect.clone();
        }
        if let Some(modal) = object.get("modal").and_then(serde_json::Value::as_bool) {
            canvas.modal = modal;
        }
        if let Some(opaque) = object.get("opaque").and_then(serde_json::Value::as_bool) {
            canvas.opaque = opaque;
        }
        if let Some(remove) = object.get("remove").and_then(serde_json::Value::as_array) {
            let remove = remove
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<std::collections::HashSet<_>>();
            canvas.commands.retain(|command| {
                command
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .is_none_or(|command_id| !remove.contains(command_id))
            });
        }
        if let Some(updates) = object
            .get("updates")
            .or_else(|| object.get("commands"))
            .and_then(serde_json::Value::as_array)
        {
            for update in updates.iter().take(4096) {
                let Some(update_id) = update.get("id").and_then(serde_json::Value::as_str) else {
                    canvas.commands.push(update.clone());
                    continue;
                };
                if let Some(existing) = canvas.commands.iter_mut().find(|command| {
                    command.get("id").and_then(serde_json::Value::as_str) == Some(update_id)
                }) {
                    *existing = update.clone();
                } else if canvas.commands.len() < 4096 {
                    canvas.commands.push(update.clone());
                }
            }
        }
        true
    }

    fn handle_module_playback(&mut self, module: &str, data: &serde_json::Value) {
        let action = data
            .get("action")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("restart");
        if let (Some(song_id), Some(index)) = (
            data.get("song_id")
                .or_else(|| data.get("id"))
                .and_then(serde_json::Value::as_str),
            self.playing,
        ) {
            if song_id != self.songs[index].info.id {
                self.status = format!("module {module}: playback request is stale");
                return;
            }
        }
        match action {
            "pause" => {
                let Some(player_process) = self.player.as_ref() else {
                    self.status = format!("module {module}: nothing is playing");
                    return;
                };
                if self.paused {
                    self.status = format!("module {module}: playback is already paused");
                } else {
                    match player::pause(player_process) {
                        Ok(()) => {
                            self.paused = true;
                            self.paused_at = Some(Instant::now());
                            self.status = format!("module {module}: paused");
                        }
                        Err(error) => self.status = error,
                    }
                }
            }
            "resume" => {
                let Some(player_process) = self.player.as_ref() else {
                    self.status = format!("module {module}: nothing is playing");
                    return;
                };
                if !self.paused {
                    self.status = format!("module {module}: playback is already running");
                } else {
                    match player::resume(player_process) {
                        Ok(()) => {
                            if let (Some(started), Some(paused_at)) =
                                (self.playback_started, self.paused_at)
                            {
                                self.playback_started = Some(started + paused_at.elapsed());
                            }
                            self.paused = false;
                            self.paused_at = None;
                            self.status = format!("module {module}: resumed");
                        }
                        Err(error) => self.status = error,
                    }
                }
            }
            "stop" => self.stop_playback(),
            "restart" | "seek" | "set_filter" | "clear_filter" | "reset" => {
                let audio_filter = match module_audio_filter(data) {
                    Ok(filter) => filter,
                    Err(error) => {
                        self.status = format!("module {module}: {error}");
                        return;
                    }
                };
                let audio_filter = if matches!(action, "clear_filter" | "reset") {
                    None
                } else if data.get("audio_filter").is_some() {
                    audio_filter
                } else {
                    self.audio_filter.clone()
                };
                let Some(index) = self.playing else {
                    self.audio_filter = audio_filter;
                    self.status = format!("module {module}: playback filter stored");
                    return;
                };
                let offset = data
                    .get("offset_seconds")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or_else(|| self.playback_elapsed());
                let was_paused = self.paused;
                self.start_song_at_with_filter(index, offset, audio_filter);
                if was_paused {
                    if let Some(player_process) = self.player.as_ref() {
                        if player::pause(player_process).is_ok() {
                            self.paused = true;
                            self.paused_at = Some(Instant::now());
                        }
                    }
                }
                self.status = match action {
                    "clear_filter" | "reset" => format!("module {module}: audio filter cleared"),
                    "seek" => format!("module {module}: playback position updated"),
                    _ => format!("module {module}: playback restarted"),
                };
            }
            other => self.status = format!("module {module}: unknown playback action {other}"),
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>) {
        if self.view_mode == ViewMode::Compact {
            self.draw_compact(frame);
            return;
        }
        let palette = self.palette();
        frame.render_widget(
            Block::default().style(Style::default().bg(palette.background)),
            frame.area(),
        );
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(frame.area());
        let header = Line::from(vec![
            Span::styled(
                " groov ",
                Style::default()
                    .fg(palette.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                mode_name(self.input_mode),
                Style::default().fg(palette.accent),
            ),
            Span::raw("  "),
            Span::styled(
                self.paths.data_dir.to_string_lossy(),
                Style::default().fg(palette.muted),
            ),
        ]);
        frame.render_widget(Paragraph::new(header), layout[0]);
        let filter = format!(
            " {} songs | filter: {}{} | queue: {}",
            self.visible.len(),
            if self.query.is_empty() {
                "-"
            } else {
                &self.query
            },
            if self.tags.is_empty() {
                String::new()
            } else {
                format!(" tags={}", self.tags.join(","))
            },
            format_args!(
                "{} | repeat: {}",
                queue_label(self.queue.len(), self.queue_position),
                if self.repeat { "on" } else { "off" }
            ),
        );
        frame.render_widget(Paragraph::new(filter), layout[1]);

        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
            .split(layout[2]);
        let items: Vec<ListItem> = self
            .visible
            .iter()
            .map(|index| {
                let song = &self.songs[*index];
                let active = if self.playing == Some(*index) {
                    "*"
                } else {
                    " "
                };
                ListItem::new(Line::from(vec![
                    Span::styled(active, Style::default().fg(palette.playing)),
                    Span::raw(" "),
                    Span::styled(song.info.id.clone(), Style::default().fg(palette.muted)),
                    Span::raw("  "),
                    Span::styled(
                        truncate(&song.info.artist, 28),
                        Style::default().fg(palette.accent),
                    ),
                    Span::raw(" — "),
                    Span::raw(truncate(&song.info.title, 48)),
                ]))
            })
            .collect();
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(palette.border))
                    .title(" Library "),
            )
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(palette.foreground)
                    .bg(palette.highlight),
            )
            .highlight_symbol("> ");
        let mut list_state = ListState::default();
        list_state.select((!self.visible.is_empty()).then_some(self.selected));
        frame.render_stateful_widget(list, body[0], &mut list_state);

        self.draw_details(frame, body[1]);
        let elapsed = self.playback_elapsed();
        let (ratio, timeline_label) = if let Some(song) = self.timeline_song() {
            let duration = song.info.duration_seconds;
            let ratio = if duration == 0 {
                0.0
            } else {
                (elapsed.min(duration) as f64) / (duration as f64)
            };
            let duration_label = if duration == 0 {
                "--:--".into()
            } else {
                format_time(duration)
            };
            (
                ratio,
                format!(
                    "{} — {}  {} / {}",
                    truncate(&song.info.artist, 20),
                    truncate(&song.info.title, 32),
                    format_time(elapsed),
                    duration_label
                ),
            )
        } else {
            (0.0, "no track".into())
        };
        frame.render_widget(
            Gauge::default()
                .gauge_style(Style::default().fg(palette.accent))
                .ratio(ratio)
                .label(timeline_label),
            layout[3],
        );
        let footer = match self.input_mode {
            InputMode::Command => format!("{}{}", self.input_prefix, self.input),
            InputMode::Normal => self.status.clone(),
        };
        frame.render_widget(
            Paragraph::new(footer).style(Style::default().fg(palette.muted)),
            layout[4],
        );
        self.draw_module_canvases(frame, palette);
        if self.show_help {
            self.draw_help(frame, palette);
        }
        if let Some(info) = self.module_info.as_deref() {
            self.draw_module_info(frame, palette, info);
        }
        if let Some(widget) = self.module_widget.as_ref() {
            self.draw_module_widget(frame, palette, widget);
        }
        if self.input_mode == InputMode::Command {
            let prompt = Rect::new(
                frame.area().x,
                frame.area().bottom().saturating_sub(1),
                frame.area().width,
                1,
            );
            frame.render_widget(
                Paragraph::new(format!("{}{}", self.input_prefix, self.input))
                    .style(Style::default().fg(palette.muted).bg(palette.background)),
                prompt,
            );
        }
    }

    fn draw_compact(&mut self, frame: &mut Frame<'_>) {
        let palette = self.palette();
        let area = frame.area();
        frame.render_widget(
            Block::default().style(Style::default().bg(palette.background)),
            area,
        );
        let cover_area = centered_cover(area, area.width, self.image_picker.font_size);
        let image_path = self.timeline_song().and_then(|song| song.image.clone());
        self.prepare_image(cover_area, image_path);
        self.receive_image_results();
        if let Some(protocol) = self.image_protocol.as_mut() {
            frame.render_stateful_widget(
                StatefulImage::new(None).resize(Resize::Crop(None)),
                cover_area,
                protocol,
            );
        }
        self.draw_module_canvases(frame, palette);
        if self.input_mode == InputMode::Command {
            let prompt = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
            frame.render_widget(
                Paragraph::new(format!("{}{}", self.input_prefix, self.input))
                    .style(Style::default().fg(palette.muted).bg(palette.background)),
                prompt,
            );
        }
    }

    fn draw_details(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let palette = self.palette();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(palette.border))
            .title(" Track ");
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(1)])
            .split(inner);
        let cover_area = centered_cover(
            parts[1],
            self.settings.cover_size,
            self.image_picker.font_size,
        );
        let image_path = self.selected_song().and_then(|song| song.image.clone());
        self.prepare_image(cover_area, image_path);
        self.receive_image_results();
        if let Some(song) = self.selected_song() {
            let metadata = vec![
                Line::from(Span::styled(
                    truncate(&song.info.title, 36),
                    Style::default()
                        .fg(palette.foreground)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(format!(
                    "{} — {}",
                    truncate(&song.info.artist, 22),
                    truncate(&song.info.album, 22)
                )),
                Line::from(format!(
                    "tags: {}",
                    if song.info.tags.is_empty() {
                        "-".into()
                    } else {
                        song.info.tags.join(", ")
                    }
                )),
                Line::from(format!(
                    "desc: {}",
                    if song.info.description.is_empty() {
                        "-"
                    } else {
                        &song.info.description
                    }
                )),
                Line::from(format!("id: {}", song.info.id)),
            ];
            frame.render_widget(Paragraph::new(Text::from(metadata)), parts[0]);
        }
        if let Some(protocol) = self.image_protocol.as_mut() {
            frame.render_stateful_widget(
                StatefulImage::new(None).resize(Resize::Crop(None)),
                cover_area,
                protocol,
            );
        } else {
            frame.render_widget(
                Paragraph::new(if self.image_pending {
                    "loading cover"
                } else {
                    "no cover"
                })
                .style(Style::default().fg(palette.muted)),
                parts[1],
            );
        }
    }

    fn draw_help(&self, frame: &mut Frame<'_>, palette: Palette) {
        let area = centered_rect(78, 38, frame.area());
        let text = vec![
            Line::from(Span::styled(
                "groov controls",
                Style::default()
                    .fg(palette.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("j/k or arrows move   gg/G first/last   Enter play   x stop"),
            Line::from("h/l seek by skips    n or ] next       [ previous"),
            Line::from("/ search              : commands       q quit"),
            Line::from(""),
            Line::from(":tag live   :untag draft   :tags sad rock"),
            Line::from(":set title/artist/album/desc/image/lyrics value"),
            Line::from(":set skips seconds"),
            Line::from(":set coversize cells"),
            Line::from(":set theme name"),
            Line::from(":mode normal|compact"),
            Line::from(":repeat on|off   :follow on|off"),
            Line::from(":modules   :modules on|off name   :modules info name"),
            Line::from(":queue   :enqueue   :next   :prev"),
        ];
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(Text::from(text))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(palette.border))
                        .title(" Help "),
                )
                .style(
                    Style::default()
                        .fg(palette.foreground)
                        .bg(palette.background),
                )
                .wrap(ratatui::widgets::Wrap { trim: true }),
            area,
        );
    }

    fn draw_module_info(&self, frame: &mut Frame<'_>, palette: Palette, info: &str) {
        let area = centered_rect(86, 72, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(info)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(palette.border))
                        .title(" Modules "),
                )
                .style(
                    Style::default()
                        .fg(palette.foreground)
                        .bg(palette.background),
                )
                .wrap(ratatui::widgets::Wrap { trim: false }),
            area,
        );
    }

    fn draw_module_widget(
        &self,
        frame: &mut Frame<'_>,
        palette: Palette,
        widget: &modules::ModuleWidgetNode,
    ) {
        let area = centered_rect(76, 48, frame.area());
        frame.render_widget(Clear, area);
        self.render_module_node(frame, palette, area, widget);
    }

    fn draw_module_canvases(&self, frame: &mut Frame<'_>, palette: Palette) {
        let mut layers = self
            .module_canvases
            .values()
            .map(CanvasLayerRef::Static)
            .chain(
                self.module_animations
                    .values()
                    .map(CanvasLayerRef::Animated),
            )
            .collect::<Vec<_>>();
        layers.sort_by_key(|layer| (layer.z(), layer.key()));
        for layer in layers {
            match layer {
                CanvasLayerRef::Static(canvas) => self.draw_canvas_layer(
                    frame,
                    palette,
                    &canvas.rect,
                    canvas.opaque,
                    &canvas.commands,
                ),
                CanvasLayerRef::Animated(active) => {
                    let commands = &active.animation.frames[active.frame_index].commands;
                    self.draw_canvas_layer(
                        frame,
                        palette,
                        &active.animation.rect,
                        active.animation.opaque,
                        commands,
                    );
                }
            }
        }
    }

    fn draw_canvas_layer(
        &self,
        frame: &mut Frame<'_>,
        palette: Palette,
        rect: &serde_json::Value,
        opaque: bool,
        commands: &[serde_json::Value],
    ) {
        let area = canvas_area(rect, frame.area());
        if area.width == 0 || area.height == 0 {
            return;
        }
        if opaque {
            frame.render_widget(Clear, area);
            fill_canvas_rect(
                frame.buffer_mut(),
                area,
                Style::default().bg(palette.background),
                " ",
            );
        }
        render_canvas_commands(frame.buffer_mut(), area, palette, commands);
    }

    fn render_module_node(
        &self,
        frame: &mut Frame<'_>,
        palette: Palette,
        area: Rect,
        widget: &modules::ModuleWidgetNode,
    ) {
        if !widget.children.is_empty() || matches!(widget.kind.as_str(), "row" | "column" | "stack")
        {
            let inner = if widget.title.is_empty() {
                area
            } else {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(palette.border))
                    .title(widget.title.clone());
                let inner = block.inner(area);
                frame.render_widget(block, area);
                inner
            };
            if widget.kind == "stack" {
                for child in &widget.children {
                    self.render_module_node(frame, palette, inner, child);
                }
                return;
            }
            let total: u32 = widget
                .children
                .iter()
                .map(|child| u32::from(child.flex))
                .sum();
            let constraints = widget
                .children
                .iter()
                .map(|child| {
                    Constraint::Percentage(
                        ((u32::from(child.flex) * 100) / total.max(1)).clamp(1, 100) as u16,
                    )
                })
                .collect::<Vec<_>>();
            let direction = if widget.kind == "row" || widget.direction == "horizontal" {
                Direction::Horizontal
            } else {
                Direction::Vertical
            };
            for (child, child_area) in widget.children.iter().zip(
                Layout::default()
                    .direction(direction)
                    .constraints(constraints)
                    .split(inner)
                    .iter(),
            ) {
                self.render_module_node(frame, palette, *child_area, child);
            }
            return;
        }
        let title = if widget.title.is_empty() {
            format!(" Module: {} ", widget.module)
        } else {
            format!(" {} ", widget.title)
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(palette.border))
            .title(title);
        match widget.kind.as_str() {
            "gauge" => {
                let ratio = if widget.max <= 0.0 {
                    0.0
                } else {
                    (widget.value / widget.max).clamp(0.0, 1.0)
                };
                frame.render_widget(
                    Gauge::default()
                        .block(block)
                        .gauge_style(Style::default().fg(palette.accent))
                        .ratio(ratio)
                        .label(widget.text.clone()),
                    area,
                );
            }
            "list" => frame.render_widget(
                Paragraph::new(widget.lines.join("\n"))
                    .block(block)
                    .style(
                        Style::default()
                            .fg(palette.foreground)
                            .bg(palette.background),
                    )
                    .wrap(ratatui::widgets::Wrap { trim: false }),
                area,
            ),
            _ => frame.render_widget(
                Paragraph::new(widget.text.clone())
                    .block(block)
                    .style(
                        Style::default()
                            .fg(palette.foreground)
                            .bg(palette.background),
                    )
                    .wrap(ratatui::widgets::Wrap { trim: false }),
                area,
            ),
        }
    }
}

pub fn run(
    paths: AppPaths,
    settings: Settings,
    songs: Vec<Song>,
) -> Result<(), Box<dyn std::error::Error>> {
    App::new(paths, settings, songs).run()
}

fn mode_name(mode: InputMode) -> &'static str {
    match mode {
        InputMode::Normal => "NORMAL",
        InputMode::Command => "COMMAND",
    }
}

fn view_mode_name(mode: ViewMode) -> &'static str {
    match mode {
        ViewMode::Normal => "NORMAL",
        ViewMode::Compact => "COMPACT",
    }
}

fn parse_color(value: &str, fallback: Color) -> Color {
    let value = value.trim();
    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.len() == 6 && hex.chars().all(|character| character.is_ascii_hexdigit()) {
        if let (Ok(red), Ok(green), Ok(blue)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return Color::Rgb(red, green, blue);
        }
    }
    match value.to_ascii_lowercase().as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        "gray" | "grey" => Color::Gray,
        "darkgray" | "dark-grey" => Color::DarkGray,
        "reset" => Color::Reset,
        _ => fallback,
    }
}

fn animation_frame_index(animation: &modules::ModuleAnimation, elapsed_ms: u64) -> usize {
    let position = if animation.looped {
        elapsed_ms % animation.duration_ms.max(1)
    } else {
        elapsed_ms.min(animation.duration_ms.saturating_sub(1))
    };
    animation
        .frames
        .iter()
        .enumerate()
        .take_while(|(_, frame)| frame.at_ms <= position)
        .map(|(index, _)| index)
        .last()
        .unwrap_or(0)
}

fn canvas_area(rect_value: &serde_json::Value, screen: Rect) -> Rect {
    let object = rect_value.as_object();
    let x = canvas_metric(object.and_then(|value| value.get("x")), screen.width, 0);
    let y = canvas_metric(object.and_then(|value| value.get("y")), screen.height, 0);
    let width = canvas_metric(
        object.and_then(|value| value.get("width").or_else(|| value.get("w"))),
        screen.width,
        i32::from(screen.width),
    )
    .clamp(0, i32::from(screen.width)) as u16;
    let height = canvas_metric(
        object.and_then(|value| value.get("height").or_else(|| value.get("h"))),
        screen.height,
        i32::from(screen.height),
    )
    .clamp(0, i32::from(screen.height)) as u16;
    let x = x.clamp(0, i32::from(screen.width)) as u16;
    let y = y.clamp(0, i32::from(screen.height)) as u16;
    let x = screen.x.saturating_add(x).min(screen.right());
    let y = screen.y.saturating_add(y).min(screen.bottom());
    Rect::new(
        x,
        y,
        width.min(screen.right().saturating_sub(x)),
        height.min(screen.bottom().saturating_sub(y)),
    )
}

fn canvas_metric(value: Option<&serde_json::Value>, total: u16, default: i32) -> i32 {
    let Some(value) = value else {
        return default;
    };
    if let Some(number) = value.as_i64() {
        return number.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    }
    let Some(text) = value.as_str().map(str::trim) else {
        return default;
    };
    let Some(percent) = text.strip_suffix('%') else {
        return text.parse::<i32>().unwrap_or(default);
    };
    percent
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .map(|value| (value * f64::from(total) / 100.0).round() as i32)
        .unwrap_or(default)
}

fn canvas_command_rect(
    area: Rect,
    object: &serde_json::Map<String, serde_json::Value>,
) -> Option<Rect> {
    let x = canvas_command_coord(object.get("x"), area.width);
    let y = canvas_command_coord(object.get("y"), area.height);
    let width = canvas_metric(
        object.get("width").or_else(|| object.get("w")),
        area.width,
        1,
    )
    .clamp(0, i32::from(area.width).saturating_add(1));
    let height = canvas_metric(
        object.get("height").or_else(|| object.get("h")),
        area.height,
        1,
    )
    .clamp(0, i32::from(area.height).saturating_add(1));
    let left = i32::from(area.x) + x;
    let top = i32::from(area.y) + y;
    let right = left.saturating_add(width);
    let bottom = top.saturating_add(height);
    let clip_left = left.max(i32::from(area.x));
    let clip_top = top.max(i32::from(area.y));
    let clip_right = right.min(i32::from(area.right()));
    let clip_bottom = bottom.min(i32::from(area.bottom()));
    if clip_right <= clip_left || clip_bottom <= clip_top {
        return None;
    }
    Some(Rect::new(
        clip_left as u16,
        clip_top as u16,
        (clip_right - clip_left) as u16,
        (clip_bottom - clip_top) as u16,
    ))
}

fn canvas_command_coord(value: Option<&serde_json::Value>, total: u16) -> i32 {
    let limit = i32::from(total).saturating_add(1);
    canvas_metric(value, total, 0).clamp(-limit, limit)
}

fn canvas_style(value: &serde_json::Value, palette: Palette) -> Style {
    let style_value = value.get("style").unwrap_or(value);
    let object = style_value.as_object();
    let foreground = object
        .and_then(|value| {
            value
                .get("fg")
                .or_else(|| value.get("foreground"))
                .or_else(|| value.get("color"))
        })
        .and_then(serde_json::Value::as_str)
        .map(|value| parse_color(value, palette.foreground))
        .unwrap_or(palette.foreground);
    let mut style = Style::default().fg(foreground);
    if let Some(background) = object
        .and_then(|value| value.get("bg").or_else(|| value.get("background")))
        .and_then(serde_json::Value::as_str)
    {
        style = style.bg(parse_color(background, palette.background));
    }
    let mut modifiers = Modifier::empty();
    if let Some(values) = object
        .and_then(|value| value.get("modifiers"))
        .and_then(serde_json::Value::as_array)
    {
        for modifier in values.iter().filter_map(serde_json::Value::as_str) {
            modifiers.insert(match modifier.to_ascii_lowercase().as_str() {
                "bold" => Modifier::BOLD,
                "dim" => Modifier::DIM,
                "italic" => Modifier::ITALIC,
                "underlined" | "underline" => Modifier::UNDERLINED,
                "reversed" | "reverse" => Modifier::REVERSED,
                "crossed_out" | "crossed-out" | "strike" => Modifier::CROSSED_OUT,
                "slow_blink" | "slow-blink" => Modifier::SLOW_BLINK,
                "rapid_blink" | "rapid-blink" => Modifier::RAPID_BLINK,
                "hidden" => Modifier::HIDDEN,
                _ => Modifier::empty(),
            });
        }
    }
    for (name, modifier) in [
        ("bold", Modifier::BOLD),
        ("dim", Modifier::DIM),
        ("italic", Modifier::ITALIC),
        ("underlined", Modifier::UNDERLINED),
        ("reversed", Modifier::REVERSED),
        ("hidden", Modifier::HIDDEN),
    ] {
        if object
            .and_then(|value| value.get(name))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            modifiers.insert(modifier);
        }
    }
    style.add_modifier(modifiers)
}

fn fill_canvas_rect(buffer: &mut Buffer, area: Rect, style: Style, symbol: &str) {
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            buffer[(x, y)].set_symbol(symbol).set_style(style);
        }
    }
}

fn clear_canvas_rect(buffer: &mut Buffer, area: Rect) {
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            buffer[(x, y)].reset();
        }
    }
}

fn set_canvas_cell(buffer: &mut Buffer, area: Rect, x: i32, y: i32, symbol: &str, style: Style) {
    let x = i32::from(area.x) + x;
    let y = i32::from(area.y) + y;
    if x < i32::from(area.x)
        || y < i32::from(area.y)
        || x >= i32::from(area.right())
        || y >= i32::from(area.bottom())
    {
        return;
    }
    buffer[(x as u16, y as u16)]
        .set_symbol(symbol)
        .set_style(style);
}

fn canvas_symbol(object: &serde_json::Map<String, serde_json::Value>, default: &str) -> String {
    object
        .get("symbol")
        .or_else(|| object.get("char"))
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.chars().next())
        .map(|value| value.to_string())
        .unwrap_or_else(|| default.to_string())
}

fn render_canvas_commands(
    buffer: &mut Buffer,
    area: Rect,
    palette: Palette,
    commands: &[serde_json::Value],
) {
    for command in commands {
        let Some(object) = command.as_object() else {
            continue;
        };
        let operation = object
            .get("op")
            .or_else(|| object.get("kind"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        let style = canvas_style(command, palette);
        match operation.as_str() {
            "clear" => {
                if let Some(rect) = canvas_command_rect(area, object) {
                    clear_canvas_rect(buffer, rect);
                }
            }
            "fill" => {
                if let Some(rect) = canvas_command_rect(area, object) {
                    let symbol = canvas_symbol(object, " ");
                    fill_canvas_rect(buffer, rect, style, &symbol);
                }
            }
            "text" | "print" => {
                let x = canvas_command_coord(object.get("x"), area.width);
                let y = canvas_command_coord(object.get("y"), area.height);
                if x < 0 || y < 0 {
                    continue;
                }
                let x = i32::from(area.x) + x;
                let y = i32::from(area.y) + y;
                if x >= i32::from(area.right()) || y >= i32::from(area.bottom()) {
                    continue;
                }
                let text = object
                    .get("text")
                    .or_else(|| object.get("value"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let max_width = canvas_metric(
                    object.get("max_width").or_else(|| object.get("width")),
                    area.width,
                    i32::from(area.right()).saturating_sub(x),
                )
                .max(0) as usize;
                buffer.set_stringn(x as u16, y as u16, text, max_width, style);
            }
            "cell" | "glyph" => {
                let x = canvas_command_coord(object.get("x"), area.width);
                let y = canvas_command_coord(object.get("y"), area.height);
                let symbol = canvas_symbol(object, " ");
                set_canvas_cell(buffer, area, x, y, &symbol, style);
            }
            "hline" | "horizontal" => {
                let x = canvas_command_coord(object.get("x"), area.width);
                let y = canvas_command_coord(object.get("y"), area.height);
                let width = canvas_metric(
                    object.get("width").or_else(|| object.get("w")),
                    area.width,
                    i32::from(area.width),
                )
                .clamp(0, i32::from(area.width).saturating_add(1));
                let symbol = canvas_symbol(object, "─");
                for offset in 0..width {
                    set_canvas_cell(buffer, area, x + offset, y, &symbol, style);
                }
            }
            "vline" | "vertical" => {
                let x = canvas_command_coord(object.get("x"), area.width);
                let y = canvas_command_coord(object.get("y"), area.height);
                let height = canvas_metric(
                    object.get("height").or_else(|| object.get("h")),
                    area.height,
                    i32::from(area.height),
                )
                .clamp(0, i32::from(area.height).saturating_add(1));
                let symbol = canvas_symbol(object, "│");
                for offset in 0..height {
                    set_canvas_cell(buffer, area, x, y + offset, &symbol, style);
                }
            }
            "rect" | "border" => {
                let Some(rect) = canvas_command_rect(area, object) else {
                    continue;
                };
                let right = rect.width.saturating_sub(1) as i32;
                let bottom = rect.height.saturating_sub(1) as i32;
                let horizontal = canvas_symbol(object, "─");
                let vertical = object
                    .get("vertical")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| value.chars().next())
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "│".into());
                for x in 0..=right {
                    set_canvas_cell(buffer, rect, x, 0, &horizontal, style);
                    set_canvas_cell(buffer, rect, x, bottom, &horizontal, style);
                }
                for y in 0..=bottom {
                    set_canvas_cell(buffer, rect, 0, y, &vertical, style);
                    set_canvas_cell(buffer, rect, right, y, &vertical, style);
                }
                set_canvas_cell(buffer, rect, 0, 0, "┌", style);
                set_canvas_cell(buffer, rect, right, 0, "┐", style);
                set_canvas_cell(buffer, rect, 0, bottom, "└", style);
                set_canvas_cell(buffer, rect, right, bottom, "┘", style);
            }
            "line" => {
                let mut x0 =
                    canvas_command_coord(object.get("x1").or_else(|| object.get("x")), area.width);
                let mut y0 =
                    canvas_command_coord(object.get("y1").or_else(|| object.get("y")), area.height);
                let x1 = object
                    .get("x2")
                    .map(|value| canvas_command_coord(Some(value), area.width))
                    .unwrap_or(x0);
                let y1 = object
                    .get("y2")
                    .map(|value| canvas_command_coord(Some(value), area.height))
                    .unwrap_or(y0);
                let symbol = canvas_symbol(object, "•");
                let dx = (x1 - x0).abs();
                let sx = if x0 < x1 { 1 } else { -1 };
                let dy = -(y1 - y0).abs();
                let sy = if y0 < y1 { 1 } else { -1 };
                let mut error = dx + dy;
                loop {
                    set_canvas_cell(buffer, area, x0, y0, &symbol, style);
                    if x0 == x1 && y0 == y1 {
                        break;
                    }
                    let doubled = error * 2;
                    if doubled >= dy {
                        error += dy;
                        x0 += sx;
                    }
                    if doubled <= dx {
                        error += dx;
                        y0 += sy;
                    }
                }
            }
            _ => {}
        }
    }
}

fn module_audio_filter(data: &serde_json::Value) -> Result<Option<String>, String> {
    let Some(value) = data.get("audio_filter") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(filter) = value.as_str() else {
        return Err("audio_filter must be a string or null".into());
    };
    let filter = filter.trim();
    if filter.is_empty() {
        return Ok(None);
    }
    if filter.len() > 4096 {
        return Err("audio_filter is too long (maximum 4096 bytes)".into());
    }
    if filter
        .chars()
        .any(|character| character == '\0' || character == '\n' || character == '\r')
    {
        return Err("audio_filter contains a forbidden control character".into());
    }
    Ok(Some(filter.to_string()))
}

fn key_name(key: &KeyEvent) -> String {
    let base = match key.code {
        KeyCode::Char(character) => character.to_string(),
        KeyCode::Enter => "enter".into(),
        KeyCode::Esc => "escape".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        _ => String::new(),
    };
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        format!("ctrl-{base}")
    } else if key.modifiers.contains(KeyModifiers::ALT) {
        format!("alt-{base}")
    } else {
        base
    }
}

fn mouse_request(mouse: &MouseEvent) -> serde_json::Value {
    let (kind, button) = match mouse.kind {
        MouseEventKind::Down(button) => ("down", Some(mouse_button_name(button))),
        MouseEventKind::Up(button) => ("up", Some(mouse_button_name(button))),
        MouseEventKind::Drag(button) => ("drag", Some(mouse_button_name(button))),
        MouseEventKind::Moved => ("move", None),
        MouseEventKind::ScrollDown => ("scroll_down", None),
        MouseEventKind::ScrollUp => ("scroll_up", None),
        MouseEventKind::ScrollLeft => ("scroll_left", None),
        MouseEventKind::ScrollRight => ("scroll_right", None),
    };
    serde_json::json!({
        "kind": kind,
        "button": button,
        "x": mouse.column,
        "y": mouse.row,
        "modifiers": {
            "shift": mouse.modifiers.contains(KeyModifiers::SHIFT),
            "control": mouse.modifiers.contains(KeyModifiers::CONTROL),
            "alt": mouse.modifiers.contains(KeyModifiers::ALT)
        }
    })
}

fn mouse_button_name(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
    }
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        format!(
            "{}…",
            value
                .chars()
                .take(max.saturating_sub(1))
                .collect::<String>()
        )
    }
}

fn configured_key(key: &KeyEvent, configured: &str) -> bool {
    match configured.to_ascii_lowercase().as_str() {
        "enter" => key.code == KeyCode::Enter,
        "esc" | "escape" => key.code == KeyCode::Esc,
        "space" => key.code == KeyCode::Char(' '),
        value if value.chars().count() == 1 => {
            key.code == KeyCode::Char(value.chars().next().unwrap_or('\0'))
        }
        _ => false,
    }
}

fn queue_label(length: usize, position: Option<usize>) -> String {
    match position {
        Some(position) => format!("{}/{}", position + 1, length),
        None if length == 0 => "-".into(),
        None => format!("-/{}", length),
    }
}

fn format_time(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds / 60) % 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn next_visible_queue_position(
    queue: &[usize],
    position: usize,
    visible: &[usize],
) -> Option<usize> {
    (position.saturating_add(1)..queue.len()).find(|candidate| visible.contains(&queue[*candidate]))
}

fn previous_visible_queue_position(
    queue: &[usize],
    position: usize,
    visible: &[usize],
) -> Option<usize> {
    (0..position)
        .rev()
        .find(|candidate| visible.contains(&queue[*candidate]))
}

fn fit_image_to_cells(
    image: image::DynamicImage,
    font_size: (u16, u16),
    cells: (u16, u16),
) -> image::DynamicImage {
    let width = u32::from(cells.0.max(1)) * u32::from(font_size.0.max(1));
    let height = u32::from(cells.1.max(1)) * u32::from(font_size.1.max(1));
    image.resize_exact(width, height, image::imageops::FilterType::Triangle)
}

fn spawn_image_worker() -> (Sender<ImageRequest>, Receiver<ImageResult>) {
    let (requests_tx, requests_rx) = mpsc::channel::<ImageRequest>();
    let (results_tx, results_rx) = mpsc::channel::<ImageResult>();
    thread::spawn(move || {
        while let Ok(mut request) = requests_rx.recv() {
            while let Ok(newer_request) = requests_rx.try_recv() {
                request = newer_request;
            }
            let protocol = image::open(&request.path).ok().map(|image| {
                let image = fit_image_to_cells(image, request.picker.font_size, request.cells);
                let mut protocol = request.picker.new_resize_protocol(image);
                protocol.resize_encode(
                    &Resize::Crop(None),
                    None,
                    Rect::new(0, 0, request.cells.0, request.cells.1),
                );
                protocol
            });
            let result = ImageResult {
                generation: request.generation,
                path: request.path,
                protocol,
            };
            if results_tx.send(result).is_err() {
                break;
            }
        }
    });
    (requests_tx, results_rx)
}

fn split_tag_args(args: &[String]) -> Vec<String> {
    let mut tags: Vec<String> = args
        .iter()
        .flat_map(|part| part.split(','))
        .map(|tag| tag.trim().to_ascii_lowercase())
        .filter(|tag| !tag.is_empty())
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

fn command_names() -> &'static [&'static str] {
    &[
        "add", "clear", "close", "enqueue", "next", "prev", "play", "q", "queue", "quit", "rescan",
        "mode", "follow", "modules", "reload", "repeat", "search", "set", "themes", "stop", "tag",
        "tags", "untag",
    ]
}

fn set_field_names() -> &'static [&'static str] {
    &[
        "album",
        "artist",
        "desc",
        "description",
        "duration",
        "image",
        "lyrics",
        "skips",
        "title",
        "coversize",
        "theme",
    ]
}

fn common_prefix(values: &[String]) -> String {
    let Some(first) = values.first() else {
        return String::new();
    };
    first
        .chars()
        .enumerate()
        .take_while(|(index, character)| {
            values
                .iter()
                .all(|value| value.chars().nth(*index) == Some(*character))
        })
        .map(|(_, character)| character)
        .collect()
}

fn expand_tilde(value: &str) -> PathBuf {
    if value == "~" || value.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            let suffix = value.strip_prefix('~').unwrap_or("");
            return PathBuf::from(home).join(suffix.trim_start_matches('/'));
        }
    }
    PathBuf::from(value)
}

fn shorten_home(path: &Path) -> String {
    let path_string = path.to_string_lossy().into_owned();
    let Some(home) = std::env::var_os("HOME") else {
        return path_string;
    };
    let home = PathBuf::from(home).to_string_lossy().into_owned();
    if path_string == home {
        "~".into()
    } else if let Some(suffix) = path_string.strip_prefix(&(home + "/")) {
        format!("~/{suffix}")
    } else {
        path_string
    }
}

fn path_matches(prefix: &str) -> Vec<(String, bool)> {
    let expanded = expand_tilde(prefix);
    let (directory, fragment) = if prefix.ends_with('/') || expanded.is_dir() {
        (expanded.clone(), String::new())
    } else {
        (
            expanded
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf(),
            expanded
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_string(),
        )
    };
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut matches = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with(&fragment) {
                return None;
            }
            let path = entry.path();
            let is_directory = path.is_dir();
            let display = if prefix.starts_with('~') {
                shorten_home(&path)
            } else if prefix.is_empty() || (!prefix.starts_with('/') && directory == Path::new("."))
            {
                name
            } else {
                path.to_string_lossy().into_owned()
            };
            Some((
                if is_directory {
                    format!("{display}/")
                } else {
                    display
                },
                is_directory,
            ))
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| left.0.cmp(&right.0));
    matches
}

fn set_text_field<F>(values: &[String], name: &str, set: F) -> Result<(), String>
where
    F: FnOnce(String),
{
    if values.is_empty() {
        return Err(format!("usage: :set {name} value"));
    }
    set(values.join(" "));
    Ok(())
}

fn copy_cover_for_song(directory: &std::path::Path, source: &str) -> Result<String, String> {
    let source_path = expand_tilde(source);
    if !source_path.is_file() {
        return Err(format!("cover not found: {source}"));
    }
    let extension = source_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !["jpg", "jpeg", "png", "webp"].contains(&extension.as_str()) {
        return Err("cover must be jpg, jpeg, png, or webp".into());
    }
    let name = format!("image.{extension}");
    let destination = directory.join(&name);
    if source_path != destination {
        std::fs::copy(&source_path, &destination)
            .map_err(|error| format!("cannot copy cover: {error}"))?;
    }
    Ok(name)
}

fn parse_command_line(line: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut token_started = false;
    for character in line.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            token_started = true;
            continue;
        }
        if character == '\\' && quote != Some('\'') {
            escaped = true;
            token_started = true;
            continue;
        }
        if let Some(delimiter) = quote {
            if character == delimiter {
                quote = None;
            } else {
                current.push(character);
            }
            token_started = true;
            continue;
        }
        match character {
            '\'' | '"' => {
                quote = Some(character);
                token_started = true;
            }
            character if character.is_whitespace() => {
                if token_started {
                    tokens.push(std::mem::take(&mut current));
                    token_started = false;
                }
            }
            character => {
                current.push(character);
                token_started = true;
            }
        }
    }
    if escaped {
        current.push('\\');
    }
    if quote.is_some() {
        return Err("unterminated quote".into());
    }
    if token_started {
        tokens.push(current);
    }
    Ok(tokens)
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn centered_cover(area: Rect, requested_width: u16, font_size: (u16, u16)) -> Rect {
    if area.width == 0 || area.height == 0 {
        return area;
    }
    let char_width = u32::from(font_size.0.max(1));
    let char_height = u32::from(font_size.1.max(1));
    let mut width = requested_width.min(area.width).max(1);
    let mut height =
        ((u32::from(width) * char_width + char_height / 2) / char_height).max(1) as u16;
    if height > area.height {
        height = area.height.max(1);
        width = ((u32::from(height) * char_height + char_width / 2) / char_width)
            .max(1)
            .min(u32::from(width)) as u16;
    }
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        animation_frame_index, canvas_area, module_audio_filter, next_visible_queue_position,
        parse_command_line, previous_visible_queue_position, render_canvas_commands,
        split_tag_args, App, InputMode, Palette, ViewMode,
    };
    use crate::config::Settings;
    use crate::library::{Song, SongInfo};
    use crate::paths::AppPaths;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Color;
    use std::fs;
    use std::time::Instant;
    use tempfile::tempdir;

    #[test]
    fn command_parser_keeps_quoted_values() {
        assert_eq!(
            parse_command_line(r#"set title "title bla bla bla""#).unwrap(),
            vec!["set", "title", "title bla bla bla"]
        );
    }

    #[test]
    fn tag_arguments_are_lowercase_and_unique() {
        assert_eq!(
            split_tag_args(&["Rock,night".into(), "rock".into()]),
            vec!["night", "rock"]
        );
    }

    #[test]
    fn module_audio_filter_accepts_text_and_rejects_unsafe_shape() {
        assert_eq!(
            module_audio_filter(&serde_json::json!({"audio_filter": "atempo=0.8"})).unwrap(),
            Some("atempo=0.8".into())
        );
        assert_eq!(
            module_audio_filter(&serde_json::json!({"audio_filter": null})).unwrap(),
            None
        );
        assert!(module_audio_filter(&serde_json::json!({"audio_filter": 3})).is_err());
        assert!(module_audio_filter(&serde_json::json!({
            "audio_filter": "atempo=0.8\nvolume=2"
        }))
        .is_err());
    }

    #[test]
    fn module_views_use_the_shared_command_line_to_close() {
        let temp = tempdir().unwrap();
        let paths = AppPaths {
            config_dir: temp.path().join("config"),
            settings_file: temp.path().join("config/settings.json"),
            data_dir: temp.path().join("data"),
            songs_dir: temp.path().join("data/songs"),
            cache_dir: temp.path().join("cache"),
        };
        let mut app = App::new(paths, Settings::default(), Vec::new());
        app.module_widget = Some(crate::modules::ModuleWidgetNode {
            module: "demo".into(),
            kind: "paragraph".into(),
            title: String::new(),
            text: String::new(),
            lines: Vec::new(),
            value: 0.0,
            max: 1.0,
            direction: "column".into(),
            flex: 1,
            children: Vec::new(),
        });
        app.handle_key(KeyEvent::new(KeyCode::Char(':'), KeyModifiers::NONE));
        assert_eq!(app.input_mode, InputMode::Command);
        app.input = "q".into();
        app.execute_command();
        assert!(app.module_widget.is_none());
        assert!(!app.should_quit);
    }

    #[test]
    fn escape_in_normal_mode_cancels_pending_input_without_quitting() {
        let temp = tempdir().unwrap();
        let paths = AppPaths {
            config_dir: temp.path().join("config"),
            settings_file: temp.path().join("config/settings.json"),
            data_dir: temp.path().join("data"),
            songs_dir: temp.path().join("data/songs"),
            cache_dir: temp.path().join("cache"),
        };
        let mut app = App::new(paths, Settings::default(), Vec::new());
        app.pending_g = true;
        assert!(!app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(!app.pending_g);
        assert!(!app.should_quit);
    }

    #[test]
    fn queue_skips_tracks_removed_by_filter() {
        assert_eq!(next_visible_queue_position(&[1, 2, 3], 0, &[3]), Some(2));
        assert_eq!(next_visible_queue_position(&[1, 2], 1, &[]), None);
        assert_eq!(
            previous_visible_queue_position(&[1, 2, 3], 2, &[1]),
            Some(0)
        );
    }

    #[test]
    fn transparent_theme_preserves_terminal_background() {
        let settings = Settings {
            theme: "transparent".into(),
            ..Settings::default()
        };
        let palette = super::Palette::from_settings(&settings);
        assert_eq!(palette.background, Color::Reset);
        assert_eq!(palette.highlight, Color::Reset);
        assert_eq!(palette.foreground, Color::Rgb(0xd0, 0xd0, 0xd0));
    }

    #[test]
    fn mode_command_switches_view_mode_and_reports_usage() {
        let temp = tempdir().unwrap();
        let paths = AppPaths {
            config_dir: temp.path().join("config"),
            settings_file: temp.path().join("config/settings.json"),
            data_dir: temp.path().join("data"),
            songs_dir: temp.path().join("data/songs"),
            cache_dir: temp.path().join("cache"),
        };
        let mut app = App::new(paths, Settings::default(), Vec::new());

        app.input = "mode compact".into();
        app.execute_command();
        assert_eq!(app.view_mode, ViewMode::Compact);
        assert_eq!(app.input_mode, InputMode::Normal);

        app.input = "mode".into();
        app.execute_command();
        assert!(app.status.contains("compact"));

        app.input = "mode widescreen".into();
        app.execute_command();
        assert_eq!(app.view_mode, ViewMode::Compact);
        assert_eq!(app.status, "usage: :mode normal|compact");

        app.input = "mode normal extra".into();
        app.execute_command();
        assert_eq!(app.view_mode, ViewMode::Compact);
        assert_eq!(app.status, "usage: :mode normal|compact");

        app.input = "mode c".into();
        app.complete_command();
        assert_eq!(app.input, "mode compact ");
    }

    #[test]
    fn follow_command_persists_and_validates_setting() {
        let temp = tempdir().unwrap();
        let paths = AppPaths {
            config_dir: temp.path().join("config"),
            settings_file: temp.path().join("config/settings.json"),
            data_dir: temp.path().join("data"),
            songs_dir: temp.path().join("data/songs"),
            cache_dir: temp.path().join("cache"),
        };
        fs::create_dir_all(&paths.config_dir).unwrap();
        let mut app = App::new(paths.clone(), Settings::default(), Vec::new());

        app.input = "follow on".into();
        app.execute_command();
        assert!(app.follow_playback);
        assert!(app.settings.follow_playback);
        assert!(fs::read_to_string(&paths.settings_file)
            .unwrap()
            .contains("\"follow_playback\": true"));

        app.input = "follow".into();
        app.execute_command();
        assert!(app.status.contains("follow: on"));

        app.input = "follow maybe".into();
        app.execute_command();
        assert_eq!(app.status, "usage: :follow on|off");
        assert!(app.follow_playback);

        app.input = "follow off".into();
        app.execute_command();
        assert!(!app.follow_playback);
        assert!(!app.settings.follow_playback);
    }

    #[test]
    fn compact_navigation_starts_adjacent_tracks_immediately() {
        let temp = tempdir().unwrap();
        let paths = AppPaths {
            config_dir: temp.path().join("config"),
            settings_file: temp.path().join("config/settings.json"),
            data_dir: temp.path().join("data"),
            songs_dir: temp.path().join("data/songs"),
            cache_dir: temp.path().join("cache"),
        };
        let song = |id: &str| Song {
            info: SongInfo {
                id: id.into(),
                artist: "Artist".into(),
                lyrics: String::new(),
                title: id.into(),
                album: String::new(),
                description: String::new(),
                image: String::new(),
                tags: Vec::new(),
                duration_seconds: 0,
                added_at: String::new(),
            },
            directory: temp.path().join(id),
            audio: temp.path().join(format!("{id}.mp3")),
            image: None,
            lyrics: None,
        };
        let settings = Settings {
            player_command: "true {file}".into(),
            follow_playback: true,
            ..Settings::default()
        };
        let mut app = App::new(
            paths,
            settings,
            vec![song("one"), song("two"), song("three")],
        );

        app.queue = app.visible.clone();
        app.queue_position = Some(0);
        app.play_next();
        assert_eq!(app.playing, Some(1));
        assert_eq!(app.selected, 1);
        crate::player::stop(&mut app.player);
        app.playing = None;
        app.playback_started = None;
        app.playback_offset_seconds = 0;
        app.queue_position = Some(0);
        app.selected = 0;

        app.view_mode = ViewMode::Compact;
        app.compact_step(1);
        assert_eq!(app.playing, Some(1));
        assert_eq!(app.selected, 1);
        app.compact_step(-1);
        assert_eq!(app.playing, Some(0));
        assert_eq!(app.selected, 0);
        crate::player::stop(&mut app.player);
    }

    #[test]
    fn covers_are_fitted_to_fixed_cells_without_cropping() {
        let image = image::DynamicImage::new_rgb8(120, 40);
        let fitted = super::fit_image_to_cells(image, (8, 16), (12, 6));
        assert_eq!((fitted.width(), fitted.height()), (96, 96));
    }

    #[test]
    fn canvas_commands_draw_text_shapes_and_clip_to_layer() {
        let area = Rect::new(0, 0, 12, 5);
        let mut buffer = Buffer::empty(area);
        let palette = Palette {
            foreground: Color::White,
            background: Color::Black,
            accent: Color::Cyan,
            muted: Color::Gray,
            highlight: Color::DarkGray,
            playing: Color::Green,
            border: Color::Gray,
        };
        let commands = vec![
            serde_json::json!({"op":"fill","x":1,"y":1,"width":4,"height":2,"symbol":"."}),
            serde_json::json!({"op":"text","x":2,"y":1,"text":"hello","fg":"cyan"}),
            serde_json::json!({"op":"rect","x":0,"y":0,"width":12,"height":5}),
            serde_json::json!({"op":"text","x":50,"y":50,"text":"outside"}),
        ];
        render_canvas_commands(&mut buffer, area, palette, &commands);
        assert_eq!(buffer[(2, 1)].symbol(), "h");
        assert_eq!(buffer[(1, 2)].symbol(), ".");
        assert_eq!(buffer[(0, 0)].symbol(), "┌");
        assert_eq!(buffer[(11, 4)].symbol(), "┘");
    }

    #[test]
    fn canvas_area_accepts_percent_geometry() {
        let area = canvas_area(
            &serde_json::json!({
                "x": "10%",
                "y": "20%",
                "width": "50%",
                "height": "40%"
            }),
            Rect::new(0, 0, 100, 50),
        );
        assert_eq!(area, Rect::new(10, 10, 50, 20));
    }

    #[test]
    fn host_animation_selects_frames_without_rpc_round_trips() {
        let animation = crate::modules::ModuleAnimation::from_value(
            "demo",
            &serde_json::json!({
                "type":"animation",
                "id":"pulse",
                "fps":60,
                "duration_ms":300,
                "frames":[
                    {"at_ms":0,"commands":[]},
                    {"at_ms":100,"commands":[{"op":"text","text":"one"}]},
                    {"at_ms":200,"commands":[{"op":"text","text":"two"}]}
                ]
            }),
        )
        .unwrap();
        assert_eq!(animation_frame_index(&animation, 0), 0);
        assert_eq!(animation_frame_index(&animation, 150), 1);
        assert_eq!(animation_frame_index(&animation, 350), 0);
    }

    #[test]
    fn modules_command_lists_info_and_toggles_modules() {
        let temp = tempdir().unwrap();
        let paths = AppPaths {
            config_dir: temp.path().join("config"),
            settings_file: temp.path().join("config/settings.json"),
            data_dir: temp.path().join("data"),
            songs_dir: temp.path().join("data/songs"),
            cache_dir: temp.path().join("cache"),
        };
        fs::create_dir_all(&paths.config_dir).unwrap();
        let mut settings = Settings::default();
        settings.modules.push(
            serde_json::from_str(
                r#"{
                    "name":"demo",
                    "version":"1.2.3",
                    "description":"test module",
                    "enabled":false,
                    "command":"demo"
                }"#,
            )
            .unwrap(),
        );
        let mut app = App::new(paths.clone(), settings, Vec::new());

        app.input = "modules".into();
        app.execute_command();
        assert!(app.status.contains("modules"));
        assert!(app.module_info.as_deref().unwrap().contains("demo  off"));

        app.input = "modules info demo".into();
        app.execute_command();
        assert!(app.module_info.as_deref().unwrap().contains("version"));
        assert!(app
            .module_info
            .as_deref()
            .unwrap()
            .contains("settings.json"));

        app.module_info = None;
        app.input = "modules on demo".into();
        app.execute_command();
        assert!(app.settings.modules[0].enabled);
        assert!(paths.settings_file.is_file());
    }

    #[test]
    fn rpc_views_are_applied_to_tui_state() {
        let temp = tempdir().unwrap();
        let paths = AppPaths {
            config_dir: temp.path().join("config"),
            settings_file: temp.path().join("config/settings.json"),
            data_dir: temp.path().join("data"),
            songs_dir: temp.path().join("data/songs"),
            cache_dir: temp.path().join("cache"),
        };
        fs::create_dir_all(&paths.config_dir).unwrap();
        let mut app = App::new(paths, Settings::default(), Vec::new());
        app.module_runtime
            .output_sender()
            .send(crate::modules::ModuleOutput {
                module: "rpc".into(),
                kind: "gauge".into(),
                title: "Progress".into(),
                text: "50%".into(),
                lines: Vec::new(),
                value: 5.0,
                max: 10.0,
                data: serde_json::json!({"type":"gauge"}),
                flush: true,
                widget: None,
            })
            .unwrap();
        assert!(app.receive_module_outputs());
        assert_eq!(app.module_widget.as_ref().unwrap().kind, "gauge");
        assert_eq!(app.module_widget.as_ref().unwrap().value, 5.0);

        app.module_runtime
            .output_sender()
            .send(crate::modules::ModuleOutput {
                module: "rpc".into(),
                kind: "palette".into(),
                title: String::new(),
                text: String::new(),
                lines: Vec::new(),
                value: 0.0,
                max: 0.0,
                data: serde_json::json!({"type":"palette","border":"#ff9f43"}),
                flush: true,
                widget: None,
            })
            .unwrap();
        assert!(app.receive_module_outputs());
        assert_eq!(app.module_border, Some(Color::Rgb(0xff, 0x9f, 0x43)));

        app.module_runtime
            .output_sender()
            .send(crate::modules::ModuleOutput {
                module: "rpc".into(),
                kind: "close".into(),
                title: String::new(),
                text: String::new(),
                lines: Vec::new(),
                value: 0.0,
                max: 0.0,
                data: serde_json::json!({"type":"close"}),
                flush: true,
                widget: None,
            })
            .unwrap();
        assert!(app.receive_module_outputs());
        assert!(app.module_widget.is_none());
    }

    #[test]
    fn canvas_outputs_are_composited_and_can_be_cleared() {
        let temp = tempdir().unwrap();
        let paths = AppPaths {
            config_dir: temp.path().join("config"),
            settings_file: temp.path().join("config/settings.json"),
            data_dir: temp.path().join("data"),
            songs_dir: temp.path().join("data/songs"),
            cache_dir: temp.path().join("cache"),
        };
        let mut app = App::new(paths, Settings::default(), Vec::new());
        app.module_runtime
            .output_sender()
            .send(crate::modules::ModuleOutput {
                module: "canvas".into(),
                kind: "canvas".into(),
                title: String::new(),
                text: String::new(),
                lines: Vec::new(),
                value: 0.0,
                max: 0.0,
                data: serde_json::json!({
                    "type": "canvas",
                    "id": "hud",
                    "z": 4,
                    "commands": [{"op":"text","x":1,"y":1,"text":"hud"}]
                }),
                flush: true,
                widget: None,
            })
            .unwrap();
        assert!(app.receive_module_outputs());
        assert_eq!(app.module_canvases.len(), 1);
        assert_eq!(app.active_modal_canvas(), None);

        app.module_runtime
            .output_sender()
            .send(crate::modules::ModuleOutput {
                module: "canvas".into(),
                kind: "canvas_patch".into(),
                title: String::new(),
                text: String::new(),
                lines: Vec::new(),
                value: 0.0,
                max: 0.0,
                data: serde_json::json!({
                    "type":"canvas_patch",
                    "id":"hud",
                    "updates":[{"id":"label","op":"text","x":1,"y":1,"text":"updated"}]
                }),
                flush: true,
                widget: None,
            })
            .unwrap();
        assert!(app.receive_module_outputs());
        assert_eq!(app.module_canvases["canvas::hud"].commands.len(), 2);

        app.module_runtime
            .output_sender()
            .send(crate::modules::ModuleOutput {
                module: "canvas".into(),
                kind: "canvas".into(),
                title: String::new(),
                text: String::new(),
                lines: Vec::new(),
                value: 0.0,
                max: 0.0,
                data: serde_json::json!({"type":"canvas","id":"hud","clear":true}),
                flush: true,
                widget: None,
            })
            .unwrap();
        assert!(app.receive_module_outputs());
        assert!(app.module_canvases.is_empty());
    }

    #[test]
    fn module_playback_response_updates_the_generic_audio_filter() {
        let temp = tempdir().unwrap();
        let paths = AppPaths {
            config_dir: temp.path().join("config"),
            settings_file: temp.path().join("config/settings.json"),
            data_dir: temp.path().join("data"),
            songs_dir: temp.path().join("data/songs"),
            cache_dir: temp.path().join("cache"),
        };
        let audio = temp.path().join("song.mp3");
        fs::write(&audio, b"audio").unwrap();
        let song = Song {
            info: SongInfo {
                id: "1".into(),
                artist: "Artist".into(),
                lyrics: String::new(),
                title: "Song".into(),
                album: String::new(),
                description: String::new(),
                image: String::new(),
                tags: Vec::new(),
                duration_seconds: 100,
                added_at: String::new(),
            },
            directory: temp.path().to_path_buf(),
            audio,
            image: None,
            lyrics: None,
        };
        let settings = Settings {
            player_command: "true {audio_filter} {file}".into(),
            ..Settings::default()
        };
        let mut app = App::new(paths, settings, vec![song]);
        app.playing = Some(0);
        app.playback_started = Some(Instant::now());
        app.module_runtime
            .output_sender()
            .send(crate::modules::ModuleOutput {
                module: "effect".into(),
                kind: "playback".into(),
                title: String::new(),
                text: String::new(),
                lines: Vec::new(),
                value: 0.0,
                max: 0.0,
                data: serde_json::json!({
                    "type": "playback",
                    "action": "set_filter",
                    "song_id": "1",
                    "audio_filter": "atempo=0.8"
                }),
                flush: true,
                widget: None,
            })
            .unwrap();
        assert!(app.receive_module_outputs());
        assert_eq!(app.audio_filter.as_deref(), Some("atempo=0.8"));
    }
}

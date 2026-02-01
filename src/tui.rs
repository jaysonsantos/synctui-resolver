use crate::model::ConflictGroup;
use crate::ops::{archive_dir_for, ensure_dir, move_file, unique_name};
use crate::scan::{rel_path, scan_conflicts};
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Terminal;
use std::collections::BTreeSet;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

#[derive(Parser, Debug, Clone)]
#[command(
    name = "synctui-resolver",
    about = "Resolve Syncthing sync-conflict files via TUI"
)]
pub struct Args {
    /// Root directory to scan
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Apply changes to the filesystem (otherwise dry-run)
    #[arg(long)]
    pub apply: bool,

    /// Include hidden files and dot-directories
    #[arg(long)]
    pub include_hidden: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    List,
    Pick,
    Confirm,
    Done,
}

struct App {
    root: PathBuf,
    apply: bool,
    include_hidden: bool,
    mode: Mode,
    groups: Vec<ConflictGroup>,
    list_state: ListState,
    pick_state: ListState,
    selected_groups: BTreeSet<usize>,
    message: String,
    planned_ops: Vec<String>,
    planned_targets: Vec<usize>,
}

pub fn run(args: Args) -> Result<()> {
    let root = args
        .path
        .canonicalize()
        .with_context(|| format!("open {:?}", args.path))?;
    let groups = scan_conflicts(&root, args.include_hidden)?;

    let mut app = App {
        root,
        apply: args.apply,
        include_hidden: args.include_hidden,
        mode: Mode::List,
        groups,
        list_state: ListState::default(),
        pick_state: ListState::default(),
        selected_groups: BTreeSet::new(),
        message: String::new(),
        planned_ops: Vec::new(),
        planned_targets: Vec::new(),
    };

    if !app.groups.is_empty() {
        app.list_state.select(Some(0));
    }

    let mut terminal = setup_terminal()?;
    let res = run_loop(&mut terminal, &mut app);
    restore_terminal(&mut terminal)?;
    res
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if app.mode == Mode::Done {
            return Ok(());
        }

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(k) = event::read()? {
                if k.kind != KeyEventKind::Press {
                    continue;
                }
                if handle_key(app, k.code, k.modifiers)? {
                    return Ok(());
                }
            }
        }
    }
}

fn handle_key(app: &mut App, code: KeyCode, mods: KeyModifiers) -> Result<bool> {
    match (app.mode, code, mods) {
        (_, KeyCode::Char('c'), KeyModifiers::CONTROL) => return Ok(true),

        (Mode::List, KeyCode::Char('t'), _)
        | (Mode::Pick, KeyCode::Char('t'), _)
        | (Mode::Confirm, KeyCode::Char('t'), _) => {
            app.apply = !app.apply;
            app.message = if app.apply {
                "Mode: APPLY (will move files)".to_string()
            } else {
                "Mode: DRY-RUN (no filesystem changes)".to_string()
            };
        }

        (Mode::List, KeyCode::Char('q'), _) => return Ok(true),
        (Mode::Pick, KeyCode::Esc, _) => {
            app.mode = Mode::List;
            app.pick_state = ListState::default();
        }
        (Mode::Confirm, KeyCode::Esc, _) => {
            app.mode = Mode::List;
            app.planned_ops.clear();
            app.planned_targets.clear();
            app.message.clear();
        }
        (Mode::List, KeyCode::Down, _) => list_down(&mut app.list_state, app.groups.len()),
        (Mode::List, KeyCode::Up, _) => list_up(&mut app.list_state, app.groups.len()),
        (Mode::Pick, KeyCode::Down, _) => {
            let len = current_group_len(app);
            list_down(&mut app.pick_state, len)
        }
        (Mode::Pick, KeyCode::Up, _) => {
            let len = current_group_len(app);
            list_up(&mut app.pick_state, len)
        }
        (Mode::List, KeyCode::Char(' '), _) => toggle_selected(app),

        // Quick-pick from list view
        (Mode::List, KeyCode::Char('c'), _) | (Mode::List, KeyCode::Char('o'), _) => {
            pick_kind_for_targets(app, PickKind::Current, false)?;
        }
        (Mode::List, KeyCode::Char('n'), _) => {
            pick_kind_for_targets(app, PickKind::Newest, false)?;
        }
        (Mode::List, KeyCode::Char('p'), _) => {
            pick_kind_for_targets(app, PickKind::Oldest, false)?;
        }
        (Mode::List, KeyCode::Char('C'), _) | (Mode::List, KeyCode::Char('O'), _) => {
            pick_kind_for_targets(app, PickKind::Current, true)?;
        }
        (Mode::List, KeyCode::Char('N'), _) => {
            pick_kind_for_targets(app, PickKind::Newest, true)?;
        }
        (Mode::List, KeyCode::Char('P'), _) => {
            pick_kind_for_targets(app, PickKind::Oldest, true)?;
        }

        (Mode::List, KeyCode::Enter, _) => enter_pick(app)?,
        (Mode::Pick, KeyCode::Enter, _) => pick_current(app)?,
        (Mode::Pick, KeyCode::Char('o'), _) => pick_original(app)?,
        (Mode::Pick, KeyCode::Char('n'), _) => pick_newest(app)?,
        (Mode::Pick, KeyCode::Char('p'), _) => pick_oldest(app)?,
        (Mode::List, KeyCode::Char('A'), _) => plan_and_confirm(app, true)?,
        (Mode::List, KeyCode::Char('a'), _) => plan_and_confirm(app, false)?,
        (Mode::Confirm, KeyCode::Char('y'), _) => apply_plan(app)?,
        (Mode::Confirm, KeyCode::Char('n'), _) => {
            app.mode = Mode::List;
            app.planned_ops.clear();
            app.planned_targets.clear();
            app.message = "Cancelled".to_string();
        }
        _ => {}
    }
    Ok(false)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickKind {
    Current,
    Newest,
    Oldest,
}

fn pick_kind_for_targets(app: &mut App, kind: PickKind, selected_only: bool) -> Result<()> {
    let mut targets: Vec<usize> = if selected_only {
        app.selected_groups.iter().copied().collect()
    } else {
        app.list_state.selected().into_iter().collect()
    };
    targets.sort_unstable();
    if targets.is_empty() {
        app.message = "No groups selected".to_string();
        return Ok(());
    }

    for gi in targets {
        let idx = match kind {
            PickKind::Current => Some(0),
            PickKind::Newest => app.groups[gi].newest_idx().or(Some(0)),
            PickKind::Oldest => app.groups[gi].oldest_idx().or(Some(0)),
        };
        app.groups[gi].chosen = idx;

        // Selecting a choice from the main list implies selecting the group.
        // This matches the Space behavior so the user can batch-confirm quickly.
        app.selected_groups.insert(gi);
    }

    app.message = match (kind, selected_only) {
        (PickKind::Current, false) => "Picked current".to_string(),
        (PickKind::Newest, false) => "Picked newest".to_string(),
        (PickKind::Oldest, false) => "Picked oldest".to_string(),
        (PickKind::Current, true) => "Picked current for selected".to_string(),
        (PickKind::Newest, true) => "Picked newest for selected".to_string(),
        (PickKind::Oldest, true) => "Picked oldest for selected".to_string(),
    };
    Ok(())
}

fn list_down(state: &mut ListState, len: usize) {
    if len == 0 {
        state.select(None);
        return;
    }
    let next = match state.selected() {
        None => 0,
        Some(i) => (i + 1).min(len - 1),
    };
    state.select(Some(next));
}

fn list_up(state: &mut ListState, len: usize) {
    if len == 0 {
        state.select(None);
        return;
    }
    let next = match state.selected() {
        None => 0,
        Some(i) => i.saturating_sub(1),
    };
    state.select(Some(next));
}

fn toggle_selected(app: &mut App) {
    let Some(i) = app.list_state.selected() else {
        return;
    };
    if app.selected_groups.contains(&i) {
        app.selected_groups.remove(&i);
    } else {
        app.selected_groups.insert(i);
    }
}

fn enter_pick(app: &mut App) -> Result<()> {
    let gi = app
        .list_state
        .selected()
        .ok_or_else(|| anyhow!("no selection"))?;
    let g = app.groups.get(gi).ok_or_else(|| anyhow!("bad index"))?;
    app.mode = Mode::Pick;
    app.pick_state = ListState::default();
    // Default to newest if it exists, else original.
    let default_idx = g.newest_idx().unwrap_or(0);
    app.pick_state.select(Some(default_idx));
    Ok(())
}

fn pick_current(app: &mut App) -> Result<()> {
    let gi = app
        .list_state
        .selected()
        .ok_or_else(|| anyhow!("no selection"))?;
    let ci = app
        .pick_state
        .selected()
        .ok_or_else(|| anyhow!("no candidate"))?;
    app.groups[gi].chosen = Some(ci);
    app.mode = Mode::List;
    app.message = "Picked".to_string();
    Ok(())
}

fn pick_original(app: &mut App) -> Result<()> {
    let gi = app
        .list_state
        .selected()
        .ok_or_else(|| anyhow!("no selection"))?;
    // Original is always candidates[0]
    app.groups[gi].chosen = Some(0);
    app.mode = Mode::List;
    app.message = "Picked original".to_string();
    Ok(())
}

fn pick_newest(app: &mut App) -> Result<()> {
    let gi = app
        .list_state
        .selected()
        .ok_or_else(|| anyhow!("no selection"))?;
    let idx = app.groups[gi]
        .newest_idx()
        .ok_or_else(|| anyhow!("no mtime"))?;
    app.groups[gi].chosen = Some(idx);
    app.mode = Mode::List;
    app.message = "Picked newest".to_string();
    Ok(())
}

fn pick_oldest(app: &mut App) -> Result<()> {
    let gi = app
        .list_state
        .selected()
        .ok_or_else(|| anyhow!("no selection"))?;
    let idx = app.groups[gi]
        .oldest_idx()
        .ok_or_else(|| anyhow!("no mtime"))?;
    app.groups[gi].chosen = Some(idx);
    app.mode = Mode::List;
    app.message = "Picked oldest".to_string();
    Ok(())
}

fn plan_and_confirm(app: &mut App, all_selected: bool) -> Result<()> {
    let mut targets: Vec<usize> = if all_selected {
        app.selected_groups.iter().copied().collect()
    } else {
        app.list_state.selected().into_iter().collect()
    };
    targets.sort_unstable();

    if targets.is_empty() {
        app.message = "No groups selected".to_string();
        return Ok(());
    }

    app.planned_ops.clear();
    app.planned_targets.clear();
    for &gi in &targets {
        let g = &app.groups[gi];
        let Some(ci) = g.chosen else {
            app.message = "Pick a version first (Enter)".to_string();
            app.planned_ops.clear();
            app.planned_targets.clear();
            return Ok(());
        };
        plan_group_ops(app, gi, ci)?;
    }
    app.planned_targets = targets;
    app.mode = Mode::Confirm;
    Ok(())
}

fn plan_group_ops(app: &mut App, gi: usize, chosen_idx: usize) -> Result<()> {
    let g = &app.groups[gi];
    let base = &g.base_path;
    let chosen = &g.candidates[chosen_idx].path;
    let archive_dir = archive_dir_for(base)?;

    app.planned_ops
        .push(format!("Group: {}", rel_path(&app.root, base).display()));
    app.planned_ops.push(format!(
        "  keep -> {}",
        rel_path(&app.root, chosen).display()
    ));
    app.planned_ops.push(format!(
        "  archive -> {}",
        rel_path(&app.root, &archive_dir).display()
    ));

    Ok(())
}

fn apply_plan(app: &mut App) -> Result<()> {
    let targets = app.planned_targets.clone();
    if targets.is_empty() {
        app.message = "Nothing planned".to_string();
        app.mode = Mode::List;
        app.planned_ops.clear();
        return Ok(());
    }

    let mut errors = Vec::new();
    for gi in targets {
        let chosen_idx = match app.groups[gi].chosen {
            Some(v) => v,
            None => continue,
        };
        if let Err(e) = apply_group(app, gi, chosen_idx) {
            errors.push(format!(
                "{}: {e:#}",
                rel_path(&app.root, &app.groups[gi].base_path).display()
            ));
        }
    }

    if errors.is_empty() {
        if app.apply {
            app.planned_ops.clear();
            app.planned_targets.clear();
            rescan(app)?;
            app.mode = Mode::List;
            app.message = "Applied".to_string();
            return Ok(());
        }

        // Dry-run: keep the confirmation open so the user can toggle apply and run again.
        app.mode = Mode::Confirm;
        app.message =
            "Dry-run complete. Toggle apply with 't', then press 'y' to apply.".to_string();
        return Ok(());
    }

    app.planned_ops.clear();
    app.planned_targets.clear();

    app.message = format!(
        "Some groups failed ({}). See details in the log panel.",
        errors.len()
    );
    app.planned_ops = errors;
    app.mode = Mode::Confirm;
    Ok(())
}

fn rescan(app: &mut App) -> Result<()> {
    let groups = scan_conflicts(&app.root, app.include_hidden)?;
    app.groups = groups;
    app.selected_groups.clear();
    app.list_state = ListState::default();
    app.pick_state = ListState::default();
    if !app.groups.is_empty() {
        app.list_state.select(Some(0));
    }
    Ok(())
}

fn apply_group(app: &mut App, gi: usize, chosen_idx: usize) -> Result<()> {
    let g = &app.groups[gi];
    let base = g.base_path.clone();
    let archive_dir = archive_dir_for(&base)?;

    let chosen_path = g.candidates[chosen_idx].path.clone();

    // Determine which file ends up at base path.
    let make_base_from = if chosen_path == base {
        None
    } else {
        Some(chosen_path.clone())
    };

    if !app.apply {
        // Dry-run: don't touch FS.
        return Ok(());
    }

    ensure_dir(&archive_dir)?;

    // Move all non-chosen candidates (including the old base if we are replacing it) into archive.
    for c in &g.candidates {
        if c.path == chosen_path {
            continue;
        }
        if !c.exists {
            continue;
        }

        let file_name = c.path.file_name().ok_or_else(|| anyhow!("bad name"))?;
        let dest = archive_dir.join(unique_name(file_name.to_string_lossy().as_ref()));
        move_file(&c.path, &dest).with_context(|| format!("archive {:?} -> {:?}", c.path, dest))?;
    }

    // If chosen is not base, move chosen into base.
    if let Some(src) = make_base_from {
        move_file(&src, &base).with_context(|| format!("set base {:?} <- {:?}", base, src))?;
    }

    Ok(())
}

fn current_group_len(app: &App) -> usize {
    let Some(i) = app.list_state.selected() else {
        return 0;
    };
    app.groups.get(i).map(|g| g.candidates.len()).unwrap_or(0)
}

fn ui(f: &mut ratatui::Frame, app: &mut App) {
    let area = f.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(5),
        ])
        .split(area);

    let mode_badge = Span::styled(
        if app.apply { "APPLY" } else { "DRY-RUN" },
        Style::default().fg(if app.apply {
            Color::Green
        } else {
            Color::Yellow
        }),
    );

    let help = match app.mode {
        Mode::List => "List: Up/Down | Enter pick specific | Space select | c current, n newest, p oldest (uppercase = selected) | a/A confirm | t toggle apply | q quit",
        Mode::Pick => "Pick: Up/Down | Enter choose | o current | n newest | p oldest | t toggle apply | Esc back",
        Mode::Confirm => "Confirm: y run | t toggle apply | n cancel | Esc back",
        Mode::Done => "Done",
    };

    let root_str = app.root.display().to_string();
    let root_max = (area.width as usize).saturating_sub(26).max(20);
    let root_short = shorten_middle(&root_str, root_max);

    let selected = app.selected_groups.len();
    let chosen = app.groups.iter().filter(|g| g.chosen.is_some()).count();
    let counts = format!(
        "groups:{}  picked:{}  selected:{}",
        app.groups.len(),
        chosen,
        selected
    );

    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                "synctui-resolver",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            mode_badge,
            Span::raw("  "),
            Span::raw(format!("root: {root_short}")),
        ]),
        Line::from(Span::raw(counts)),
        Line::from(Span::raw(help)),
    ])
    .wrap(Wrap { trim: true });
    f.render_widget(header, chunks[0]);

    match app.mode {
        Mode::List | Mode::Confirm | Mode::Done => draw_list(f, app, chunks[1]),
        Mode::Pick => draw_pick(f, app, chunks[1]),
    }

    draw_footer(f, app, chunks[2]);

    if app.mode == Mode::Confirm {
        draw_confirm_modal(f, app, chunks[1]);
    }
}

fn shorten_middle(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        return s.to_string();
    }
    if max_chars <= 5 {
        return chars.into_iter().take(max_chars).collect();
    }

    let keep = (max_chars - 3) / 2;
    let head: String = chars.iter().take(keep).collect();
    let tail: String = chars
        .iter()
        .rev()
        .take(keep)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{head}...{tail}")
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn draw_confirm_modal(f: &mut ratatui::Frame, app: &mut App, area: Rect) {
    let rect = centered_rect(80, 70, area);
    f.render_widget(Clear, rect);

    let title = if app.apply {
        Span::styled(
            "CONFIRM APPLY",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            "CONFIRM DRY-RUN",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    };

    let mut lines = Vec::new();
    if app.apply {
        lines.push(Line::from(Span::styled(
            "This will move files on disk.",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Dry-run: no filesystem changes.",
            Style::default().fg(Color::Yellow),
        )));
    }
    lines.push(Line::from(""));

    let planned = if app.planned_targets.is_empty() {
        "No groups planned".to_string()
    } else {
        format!("Planned groups: {}", app.planned_targets.len())
    };
    lines.push(Line::from(planned));
    lines.push(Line::from(""));

    lines.push(Line::from(vec![
        Span::styled("y", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(": run   "),
        Span::styled("t", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(": toggle apply   "),
        Span::styled("n", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(": cancel"),
    ]));
    lines.push(Line::from(""));

    if !app.planned_ops.is_empty() {
        for l in app
            .planned_ops
            .iter()
            .take((rect.height as usize).saturating_sub(9))
        {
            lines.push(Line::from(l.as_str()));
        }
    }

    let p = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false });
    f.render_widget(p, rect);
}

fn draw_list(f: &mut ratatui::Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let sel = if app.selected_groups.contains(&i) {
                "[*]"
            } else {
                "[ ]"
            };
            let picked = match g.chosen {
                None => "(unpicked)".to_string(),
                Some(ci) => format!("(keep: {})", g.candidates[ci].label),
            };
            let rel = rel_path(&app.root, &g.base_path).display().to_string();
            let cnt = g.candidates.len().saturating_sub(1);
            let orig = if g.candidates.first().map(|c| c.exists).unwrap_or(false) {
                "orig"
            } else {
                "no-orig"
            };
            ListItem::new(Line::from(format!(
                "{sel} {rel}  [{cnt} conflicts, {orig}] {picked}"
            )))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Conflicts"))
        .highlight_style(Style::default().bg(Color::Blue).fg(Color::White));
    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_pick(f: &mut ratatui::Frame, app: &mut App, area: Rect) {
    let gi = match app.list_state.selected() {
        Some(i) => i,
        None => {
            let p = Paragraph::new("No group selected")
                .block(Block::default().borders(Borders::ALL).title("Pick"));
            f.render_widget(p, area);
            return;
        }
    };
    let g = &app.groups[gi];

    let items: Vec<ListItem> = g
        .candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let rel = rel_path(&app.root, &c.path).display().to_string();
            let size = c
                .size
                .map(|s| format!("{s}b"))
                .unwrap_or_else(|| "?".to_string());
            let m = c
                .modified
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs().to_string())
                .unwrap_or_else(|| "?".to_string());
            let exists = if c.exists { "" } else { "(missing) " };
            let chosen = if g.chosen == Some(i) { "(picked)" } else { "" };
            ListItem::new(Line::from(format!(
                "{}{}  {}  size:{}  mtime:{}  {}",
                exists, c.label, rel, size, m, chosen
            )))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(rel_path(&app.root, &g.base_path).display().to_string()),
        )
        .highlight_style(Style::default().bg(Color::Blue).fg(Color::White));

    f.render_stateful_widget(list, area, &mut app.pick_state);
}

fn draw_footer(f: &mut ratatui::Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    let msg = Paragraph::new(app.message.clone())
        .block(Block::default().borders(Borders::ALL).title("Message"))
        .wrap(Wrap { trim: true });
    f.render_widget(msg, chunks[0]);

    let plan_text = if app.planned_ops.is_empty() {
        "".to_string()
    } else {
        app.planned_ops.join("\n")
    };
    let plan = Paragraph::new(plan_text)
        .block(Block::default().borders(Borders::ALL).title("Plan / Log"))
        .wrap(Wrap { trim: false });
    f.render_widget(plan, chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_group_len_empty_when_none_selected() {
        let mut app = App {
            root: PathBuf::from("/"),
            apply: false,
            include_hidden: false,
            mode: Mode::List,
            groups: vec![],
            list_state: ListState::default(),
            pick_state: ListState::default(),
            selected_groups: BTreeSet::new(),
            message: String::new(),
            planned_ops: vec![],
            planned_targets: vec![],
        };
        app.list_state.select(None);
        assert_eq!(current_group_len(&app), 0);
    }

    #[test]
    fn list_nav_bounds() {
        let mut state = ListState::default();
        list_down(&mut state, 0);
        assert_eq!(state.selected(), None);

        list_down(&mut state, 3);
        assert_eq!(state.selected(), Some(0));
        list_down(&mut state, 3);
        assert_eq!(state.selected(), Some(1));
        list_down(&mut state, 3);
        assert_eq!(state.selected(), Some(2));
        list_down(&mut state, 3);
        assert_eq!(state.selected(), Some(2));

        list_up(&mut state, 3);
        assert_eq!(state.selected(), Some(1));
        list_up(&mut state, 3);
        assert_eq!(state.selected(), Some(0));
        list_up(&mut state, 3);
        assert_eq!(state.selected(), Some(0));
    }

    #[test]
    fn list_quick_pick_also_selects_group() {
        let g0 = ConflictGroup {
            base_path: PathBuf::from("a"),
            candidates: vec![],
            chosen: None,
        };
        let g1 = ConflictGroup {
            base_path: PathBuf::from("b"),
            candidates: vec![],
            chosen: None,
        };

        let mut app = App {
            root: PathBuf::from("/"),
            apply: false,
            include_hidden: false,
            mode: Mode::List,
            groups: vec![g0, g1],
            list_state: ListState::default(),
            pick_state: ListState::default(),
            selected_groups: BTreeSet::new(),
            message: String::new(),
            planned_ops: vec![],
            planned_targets: vec![],
        };
        app.list_state.select(Some(1));

        pick_kind_for_targets(&mut app, PickKind::Newest, false).unwrap();
        assert!(app.selected_groups.contains(&1));
    }
}

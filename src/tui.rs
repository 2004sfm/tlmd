pub mod screen_login;
pub mod screen_users;

use std::io::{self, Write};

use crossterm::{
    cursor, queue,
    style::{Color, Print, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
};

use crate::{App, Screen};

// ── Aesthetic tokens ─────────────────────────────────

pub const BG: Color = Color::Reset;
pub const FG: Color = Color::Reset;
pub const DIM: Color = Color::Rgb {
    r: 136,
    g: 136,
    b: 136,
};
pub const BOX_WIDTH: u16 = 60;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IconStyle {
    None,
    Filled,
    Outline,
}

/// Render the current screen.
pub fn render(w: &mut impl Write, app: &App) -> io::Result<()> {
    queue!(w, SetBackgroundColor(BG), Clear(ClearType::All))?;

    let (cols, rows) = terminal::size()?;

    // Draw the top header at absolute zero without padding
    draw_text(w, 0, 0, "Terminal Login Manager Daemon", FG, BG)?;

    let cursor_pos = match &app.screen {
        Screen::UserSelect => {
            let filtered = app.filtered_users();
            screen_users::render(
                w,
                cols,
                rows,
                &filtered,
                app.selected_index,
                app.search_active,
                &app.search_query,
                app.button_focus,
                app.confirm_action.as_ref(),
                app.confirm_focus,
                app.icon_style,
            )?
        }
        Screen::Login => {
            let username = app.selected_username().unwrap_or("?");
            let show_last_char = app
                .unmasked_until
                .map(|until| std::time::Instant::now() < until)
                .unwrap_or(false);

            let show_delete_flash = app
                .deleted_until
                .map(|until| std::time::Instant::now() < until)
                .unwrap_or(false);

            screen_login::render(
                w,
                cols,
                rows,
                username,
                &app.password,
                app.auth_error,
                app.authenticating,
                app.spinner_frame,
                app.button_focus,
                show_last_char,
                show_delete_flash,
                app.filtered_users().len(),
                app.icon_style,
            )?
        }
    };

    if let Some((cx, cy)) = cursor_pos {
        queue!(w, cursor::Show, cursor::MoveTo(cx, cy))?;
    } else {
        queue!(w, cursor::Hide)?;
    }

    w.flush()
}

/// Truncate a string to a maximum length, appending ".." if truncated.
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        let mut truncated: String = s.chars().take(max_len.saturating_sub(2)).collect();
        truncated.push_str("..");
        truncated
    } else {
        s.to_string()
    }
}

// ── Box drawing primitives ──────────────────────────────────────

/// Draw a box with a centered title on the top border.
///
/// ```text
/// ┌──────┤ Title ├──────┐
/// │                     │
/// └─────────────────────┘
/// ```
pub fn draw_box(
    w: &mut impl Write,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    title: &str,
) -> io::Result<()> {
    queue!(w, SetForegroundColor(FG), SetBackgroundColor(BG))?;

    // Top border with title
    let title_decorated = format!("┤ {title} ├");
    let title_len = title_decorated.chars().count() as u16;
    let remaining = width.saturating_sub(2).saturating_sub(title_len);
    let left_pad = remaining / 2;
    let right_pad = remaining - left_pad;

    queue!(w, cursor::MoveTo(x, y))?;
    queue!(w, Print("┌"))?;
    queue!(w, Print("─".repeat(left_pad as usize)))?;
    queue!(w, Print(&title_decorated))?;
    queue!(w, Print("─".repeat(right_pad as usize)))?;
    queue!(w, Print("┐"))?;

    // Side borders with cleared interior
    for row in 1..height.saturating_sub(1) {
        queue!(w, cursor::MoveTo(x, y + row))?;
        queue!(w, Print("│"))?;
        queue!(w, Print(" ".repeat(width.saturating_sub(2) as usize)))?;
        queue!(w, cursor::MoveTo(x + width - 1, y + row))?;
        queue!(w, Print("│"))?;
    }

    // Bottom border
    queue!(w, cursor::MoveTo(x, y + height - 1))?;
    queue!(w, Print("└"))?;
    queue!(w, Print("─".repeat(width.saturating_sub(2) as usize)))?;
    queue!(w, Print("┘"))?;

    Ok(())
}

/// Draw a pair of buttons centered in the box, with optional focus highlight.
///
/// `button_focus`: None = both dim, Some(0) = left highlighted, Some(1) = right highlighted.
pub fn draw_button_pair(
    w: &mut impl Write,
    x: u16,
    y: u16,
    box_width: u16,
    left: &str,
    right: &str,
    button_focus: Option<usize>,
) -> io::Result<()> {
    use crossterm::style::Attribute;

    // Sum of the visual length of the texts, including the brackets "[]"
    let buttons_len = (left.len() + 2 + right.len() + 2) as u16;

    // If the parity of the box width does not match the parity of the buttons + base gap of 4,
    // we add 1 extra space to the gap so it fits perfectly and can be centered perfectly.
    let gap = if box_width % 2 != (buttons_len + 4) % 2 {
        "     " // 5 spaces
    } else {
        "    " // 4 spaces
    };

    let total_len = buttons_len + gap.len() as u16;
    let btn_x = x + (box_width.saturating_sub(total_len)) / 2;

    queue!(w, cursor::MoveTo(btn_x, y))?;

    // Left button
    if button_focus == Some(0) {
        queue!(w, crossterm::style::SetAttribute(Attribute::Reverse))?;
        queue!(w, Print(format!("[{left}]")))?;
        queue!(w, crossterm::style::SetAttribute(Attribute::Reset))?;
        queue!(w, SetForegroundColor(FG), SetBackgroundColor(BG))?;
    } else {
        queue!(w, SetForegroundColor(FG))?;
        queue!(w, Print(format!("[{left}]")))?;
    }

    queue!(w, SetForegroundColor(FG), SetBackgroundColor(BG))?;
    queue!(w, Print(gap))?;

    // Right button
    if button_focus == Some(1) {
        queue!(w, crossterm::style::SetAttribute(Attribute::Reverse))?;
        queue!(w, Print(format!("[{right}]")))?;
        queue!(w, crossterm::style::SetAttribute(Attribute::Reset))?;
        queue!(w, SetForegroundColor(FG), SetBackgroundColor(BG))?;
    } else {
        queue!(w, SetForegroundColor(FG))?;
        queue!(w, Print(format!("[{right}]")))?;
    }

    queue!(w, SetForegroundColor(FG), SetBackgroundColor(BG))?;
    Ok(())
}

/// Draw the hints bar below the box, centered on the terminal.
///
/// Builds the full string first, measures its display width with CJK-aware
/// unicode (covers "East Asian Ambiguous" chars like ↑↓ that some terminals
/// render as 2 columns), then positions it once.
pub fn draw_hints(w: &mut impl Write, cols: u16, y: u16, hints: &[(&str, &str)]) -> io::Result<()> {
    // Measure total visible columns of the rendered hints string
    let total_width: usize = hints
        .iter()
        .enumerate()
        .map(|(i, (key, action))| {
            let sep = if i > 0 { 2 } else { 0 };
            sep + key.chars().count() + 1 + action.chars().count()
        })
        .sum();

    let x = (cols as usize).saturating_sub(total_width) / 2;
    queue!(w, cursor::MoveTo(x as u16, y))?;

    for (i, (key, action)) in hints.iter().enumerate() {
        if i > 0 {
            queue!(w, Print("  "))?;
        }
        queue!(w, SetForegroundColor(FG), Print(key))?;
        queue!(w, SetForegroundColor(DIM), Print(format!(" {action}")))?;
    }

    Ok(())
}

/// Draw text at a position with specified colors.
pub fn draw_text(
    w: &mut impl Write,
    x: u16,
    y: u16,
    text: &str,
    fg: Color,
    bg: Color,
) -> io::Result<()> {
    queue!(
        w,
        cursor::MoveTo(x, y),
        SetForegroundColor(fg),
        SetBackgroundColor(bg),
        Print(text),
    )?;
    Ok(())
}

/// Draw a full-width highlighted row (for selected items).
pub fn draw_highlighted_row(
    w: &mut impl Write,
    x: u16,
    y: u16,
    text: &str,
    width: u16,
) -> io::Result<()> {
    use crossterm::style::Attribute;

    let inner = width.saturating_sub(4) as usize;
    let padded = format!("{text:<inner$}");

    queue!(
        w,
        cursor::MoveTo(x + 2, y),
        crossterm::style::SetAttribute(Attribute::Reverse),
        crossterm::style::SetAttribute(Attribute::Bold),
        Print(&padded),
        crossterm::style::SetAttribute(Attribute::Reset),
        SetForegroundColor(FG),
        SetBackgroundColor(BG),
    )?;
    Ok(())
}

/// Draw a normal row.
pub fn draw_normal_row(w: &mut impl Write, x: u16, y: u16, text: &str) -> io::Result<()> {
    draw_text(w, x + 2, y, text, FG, BG)
}

pub const ICON_FILLED: &[&str] = &[
    "   ▄███████████▄   ",
    "  ▀▀▀█████████▀▀▀  ",
    " ▄▀▀▄ ▀█████▀ ▄▀▀▄ ",
    "▀▄  ▄▀ █▀▀▀█ ▀▄  ▄▀",
    "▄ ▀▀ ▄▀     ▀▄ ▀▀ ▄",
    "██████▄     ▄██████",
    "████████▄ ▄████████",
    "███████████████████",
    "▀█████████████████▀",
    "  ▀█████████████▀  ",
];

pub const ICON_OUTLINE: &[&str] = &[
    "    ▄▀▀▀▀▀▀▀▀▀▀▀▄    ",
    "   ▀             ▀   ",
    " ▄▀▀▀▀▄       ▄▀▀▀▀▄ ",
    "█ ▄██▄ █     █ ▄██▄ █",
    "█▄ ▀▀ ▄▀▄▀▀▀▄▀▄ ▀▀ ▄█",
    "█ ▀▀▀▀ █     █ ▀▀▀▀ █",
    "█       ▀▄ ▄▀       █",
    "█         ▀         █",
    "█                   █",
    "▀▄                 ▄▀",
    "  ▀▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▀  ",
];

/// Draws the icon centered in the empty space above, ensuring it doesn't jump when switching screens.
pub fn draw_icon(w: &mut impl Write, cols: u16, rows: u16, num_users: usize, style: IconStyle) -> io::Result<()> {
    let icon = match style {
        IconStyle::None => return Ok(()),
        IconStyle::Filled => ICON_FILLED,
        IconStyle::Outline => ICON_OUTLINE,
    };
    
    let icon_height = icon.len() as u16;
    
    // Calculate the highest point the users box reaches
    let visible_count = num_users.min(10) as u16;
    let user_box_height = visible_count + 6;
    
    // safe_box_y is the Y line where the users box starts.
    let safe_box_y = rows.saturating_sub(user_box_height) / 2;

    // If there is enough space for the icon, center it in that space
    if safe_box_y > icon_height {
        // We add +1 because visually the center of mass of the logo requires lowering it a bit (optical compensation)
        let start_y = (safe_box_y - icon_height) / 2 + 1;
        
        let icon_width = icon[0].chars().count() as u16;
        let start_x = cols.saturating_sub(icon_width) / 2;

        for (i, line) in icon.iter().enumerate() {
            draw_text(w, start_x, start_y + i as u16, line, FG, BG)?;
        }
    }
    Ok(())
}

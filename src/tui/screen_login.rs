use std::io::{self, Write};

use crossterm::style::Color;

use super::{draw_box, draw_button_pair, draw_hints, draw_text, BG, BOX_WIDTH, DIM, FG};

/// Color for error messages.
const ERROR_FG: Color = Color::Rgb {
    r: 255,
    g: 100,
    b: 100,
};

/// Render the login/password screen.
pub fn render(
    w: &mut impl Write,
    cols: u16,
    rows: u16,
    username: &str,
    password: &str,
    auth_error: bool,
    authenticating: bool,
    spinner_frame: usize,
    button_focus: Option<usize>,
    show_last_char: bool,
    show_delete_flash: bool,
    num_users: usize,
    icon_style: crate::tui::IconStyle,
) -> io::Result<Option<(u16, u16)>> {
    const SPINNER: [char; 8] = ['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];

    // Box grows by 2 rows when error is shown
    let box_height: u16 = if auth_error { 11 } else { 9 };
    let safe_username = crate::tui::truncate(username, 32);
    let title = format!("Login as {safe_username}");

    let x = cols.saturating_sub(BOX_WIDTH) / 2;
    let y = rows.saturating_sub(box_height) / 2;

    crate::tui::draw_icon(w, cols, rows, num_users, icon_style)?;
    draw_box(w, x, y, BOX_WIDTH, box_height, &title)?;

    // Error section — 3 rows reserved: [pad][error centered][pad]
    // Content offset: shifts down by 2 when error area is present
    let offset: u16 = if auth_error { 2 } else { 0 };

    if auth_error {
        let err_msg = "Invalid password";
        let err_x = x + (BOX_WIDTH.saturating_sub(err_msg.len() as u16)) / 2;
        draw_text(w, err_x, y + 2, err_msg, ERROR_FG, BG)?;
    }

    let cursor_pos = if authenticating {
        let spin_char = SPINNER[spinner_frame % 8];
        let msg = format!("{spin_char} Authenticating...");
        let msg_x = x + (BOX_WIDTH.saturating_sub(msg.chars().count() as u16)) / 2;
        let msg_y = y + box_height / 2;
        draw_text(w, msg_x, msg_y, &msg, DIM, BG)?;
        None
    } else {
        // Password prompt (centered)
        let prompt = "Enter your password:";
        let prompt_x = x + (BOX_WIDTH.saturating_sub(prompt.len() as u16)) / 2;
        draw_text(w, prompt_x, y + 2 + offset, prompt, FG, BG)?;

        // Password input area limit
        let max_length = 26;
        let pw_len = password.chars().count();
        let display_len = pw_len.min(max_length);
        
        let mut masked = String::new();
        if pw_len > 0 {
            if show_last_char {
                let n = display_len.saturating_sub(1);
                masked.push_str(&"*".repeat(n));
                masked.push(password.chars().last().unwrap());
            } else {
                masked.push_str(&"*".repeat(display_len));
            }
        }
        
        // Calculate the visual width of ONLY the asterisks (ignoring "> " and the cursor "█")
        let ast_width = masked.chars().count() as u16;
        let ast_x = x + (BOX_WIDTH.saturating_sub(ast_width)) / 2;
        
        let prefix_x = ast_x.saturating_sub(2);
        let input_y = y + 4 + offset;

        // Draw the prefix ("> ") which hangs to the left
        draw_text(w, prefix_x, input_y, "> ", FG, BG)?;
        
        // Draw the asterisks in their position
        if !masked.is_empty() {
            draw_text(w, ast_x, input_y, &masked, FG, BG)?;
        }

        // Action buttons
        draw_button_pair(
            w,
            x,
            y + 6 + offset,
            BOX_WIDTH,
            "Back",
            "Confirm",
            button_focus,
        )?;

        // Hints bar
        draw_hints(
            w,
            cols,
            y + box_height + 1,
            &[("Esc", "Back"), ("Enter", "Confirm")],
        )?;

        if button_focus.is_none() && !show_delete_flash {
            Some((ast_x + ast_width, input_y))
        } else {
            None
        }
    };

    Ok(cursor_pos)
}

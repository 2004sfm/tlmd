use std::io::{self, Write};

use crossterm::{queue, style::SetBackgroundColor};

use super::{
    draw_box, draw_button_pair, draw_highlighted_row, draw_hints, draw_normal_row, draw_text, BG,
    BOX_WIDTH, DIM, FG,
};

/// Maximum number of users visible at once before scrolling kicks in.
const MAX_VISIBLE: usize = 6;

/// Render the user selection screen with navigable, scrollable user list.
pub fn render(
    w: &mut impl Write,
    cols: u16,
    rows: u16,
    users: &[&str],
    selected_index: usize,
    search_active: bool,
    search_query: &str,
    button_focus: Option<usize>,
    confirm_action: Option<&crate::system::SystemAction>,
    confirm_focus: usize,
    icon_style: crate::tui::IconStyle,
) -> io::Result<Option<(u16, u16)>> {
    let mut cursor_pos = None;
    
    if let Some(action) = confirm_action {
        let modal_w = 30;
        let modal_h = 7;
        let mx = cols.saturating_sub(modal_w) / 2;
        let my = rows.saturating_sub(modal_h) / 2;
        
        draw_box(w, mx, my, modal_w, modal_h, action.label())?;
        
        let msg = "Are you sure?";
        let msg_x = mx + (modal_w.saturating_sub(msg.len() as u16)) / 2;
        draw_text(w, msg_x, my + 2, msg, FG, BG)?;
        
        draw_button_pair(w, mx, my + 4, modal_w, "No", "Yes", Some(confirm_focus))?;
        
        return Ok(None);
    }

    let visible_count = users.len().min(MAX_VISIBLE).max(1);
    
    // Box height is strictly constant. 
    // Layout: top(1) + pad(1) + users(visible) + pad(1) + buttons_or_search(1) + pad(1) + bottom(1) = visible + 6
    let box_height = visible_count as u16 + 6;

    let title = "Choose user";
    // We add 1 to the width if the title length parity doesn't match BOX_WIDTH, to ensure perfect centering.
    let actual_width = BOX_WIDTH + (title.chars().count() % 2 != BOX_WIDTH as usize % 2) as u16;

    let x = cols.saturating_sub(actual_width) / 2;
    let y = rows.saturating_sub(box_height) / 2;

    crate::tui::draw_icon(w, cols, rows, users.len(), icon_style)?;
    draw_box(w, x, y, actual_width, box_height, title)?;

    // User list with scroll
    if users.is_empty() {
        let msg = if search_active {
            "No matches"
        } else {
            "No users found"
        };
        draw_text(w, x + 2, y + 2, msg, DIM, BG)?;
    } else {
        let scroll_offset = compute_scroll_offset(selected_index, users.len(), MAX_VISIBLE);

        let end = (scroll_offset + visible_count).min(users.len());
        for (i, user) in users[scroll_offset..end].iter().enumerate() {
            let actual_index = scroll_offset + i;
            let row_y = y + 2 + i as u16;
            
            if actual_index == selected_index && button_focus.is_none() {
                draw_highlighted_row(w, x, row_y, user, actual_width)?;
            } else {
                draw_normal_row(w, x, row_y, user)?;
            }
        }
    }

    // Reset background after user list
    queue!(w, SetBackgroundColor(BG))?;

    // The bottom section: either search input OR action buttons
    let bottom_y = y + 2 + visible_count as u16 + 1;
    
    if search_active {
        let search_display = format!("/ {search_query}");
        draw_text(w, x + 2, bottom_y, &search_display, FG, BG)?;
        cursor_pos = Some((x + 2 + search_display.chars().count() as u16, bottom_y));
    } else {
        draw_button_pair(w, x, bottom_y, actual_width, "Reboot", "Shutdown", button_focus)?;
    }

    // Hints bar (changes based on search state)
    if search_active {
        draw_hints(
            w,
            cols,
            y + box_height + 1,
            &[
                ("Esc", "Cancel"), 
                ("↑↓", "Navigate"), 
                ("Enter", "Select"),
                ("Ctrl-R", "Reboot"),
                ("Ctrl-P", "Shutdown"),
            ],
        )?;
    } else {
        draw_hints(
            w,
            cols,
            y + box_height + 1,
            &[
                ("↑↓", "Navigate"),
                ("Enter", "Select"),
                ("/", "Search"),
                ("Ctrl-R", "Reboot"),
                ("Ctrl-P", "Shutdown"),
            ],
        )?;
    }

    Ok(cursor_pos)
}

/// Calculate the scroll offset to keep the selected item visible.
fn compute_scroll_offset(selected: usize, total: usize, max_visible: usize) -> usize {
    if total <= max_visible {
        return 0;
    }

    let half = max_visible / 2;

    if selected <= half {
        0
    } else if selected >= total - half {
        total - max_visible
    } else {
        selected - half
    }
}

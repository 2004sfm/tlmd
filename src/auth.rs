use std::ffi::{CStr, CString};

use pam_client2::{Context, ConversationHandler, ErrorCode, Flag};

/// A PAM conversation handler that supplies the password non-interactively.
struct Silent {
    password: CString,
}

impl Silent {
    fn new(password: &str) -> Self {
        Self {
            password: CString::new(password).unwrap_or_default(),
        }
    }
}

impl ConversationHandler for Silent {
    // Visible prompt (username, etc.) — not used in our flow
    fn prompt_echo_on(&mut self, _msg: &CStr) -> Result<CString, ErrorCode> {
        Ok(CString::default())
    }

    // Masked prompt = password prompt
    fn prompt_echo_off(&mut self, _msg: &CStr) -> Result<CString, ErrorCode> {
        Ok(self.password.clone())
    }

    fn text_info(&mut self, _msg: &CStr) {}
    fn error_msg(&mut self, _msg: &CStr) {}
}

/// Attempt to authenticate `username` with `password` via PAM.
/// If successful, opens a PAM session, signals the main thread, waits for TTY cleanup, and execs.
pub fn authenticate_and_launch(
    username: &str,
    password: &str,
    auth_tx: std::sync::mpsc::Sender<bool>,
    exec_rx: std::sync::mpsc::Receiver<()>,
) {
    let conv = Silent::new(password);

    if let Ok(mut ctx) = Context::new("login", Some(username), conv) {
        if matches!(ctx.authenticate(Flag::NONE), Ok(())) && matches!(ctx.acct_mgmt(Flag::NONE), Ok(())) {
            if let Ok(_session) = ctx.open_session(Flag::NONE) {
                // Tell main thread we succeeded
                let _ = auth_tx.send(true);
                
                // Wait for main thread to cleanup TTY
                let _ = exec_rx.recv();
                
                // Exec into uwsm!
                crate::system::launch_uwsm(username, _session.envlist());
            }
        }
    }

    // If we get here, authentication or session opening failed
    let _ = auth_tx.send(false);
}

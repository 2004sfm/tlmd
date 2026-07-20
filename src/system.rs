use std::process::Command;

/// System action that requires confirmation.
#[derive(Debug, Clone, PartialEq)]
pub enum SystemAction {
    Shutdown,
    Reboot,
}

impl SystemAction {
    /// Human-readable label for the confirmation dialog.
    pub fn label(&self) -> &str {
        match self {
            SystemAction::Shutdown => "Shutdown",
            SystemAction::Reboot => "Reboot",
        }
    }
}

/// Power off the machine via systemctl.
pub fn shutdown() {
    let _ = Command::new("systemctl").arg("poweroff").status();
}

/// Reboot the machine via systemctl.
pub fn reboot() {
    let _ = Command::new("systemctl").arg("reboot").status();
}

/// Set up user credentials and exec into uwsm.
pub fn launch_uwsm(username: &str, envlist: pam_client2::env_list::EnvList) {
    let user = match uzers::get_user_by_name(username) {
        Some(u) => u,
        None => std::process::exit(1),
    };

    use std::os::unix::process::CommandExt;
    use nix::unistd::{initgroups, Gid};
    use std::ffi::CString;
    use uzers::os::unix::UserExt;

    if let Ok(c_user) = CString::new(username) {
        let _ = initgroups(&c_user, Gid::from_raw(user.primary_group_id()));
    }

    // Attempt to launch uwsm select
    let err = std::process::Command::new("uwsm")
        .arg("select")
        .uid(user.uid())
        .gid(user.primary_group_id())
        .envs(envlist.iter_tuples())
        .exec();

    // If uwsm is not installed or fails, we fall back to a graceful backup plan:
    // We launch the user's default shell (e.g., /bin/bash or /bin/zsh) so they aren't left without a system.
    let shell_err = std::process::Command::new(user.shell())
        .uid(user.uid())
        .gid(user.primary_group_id())
        // Important: We do not pass the full envlist here because we might break the clean terminal,
        // but for simplicity, we use the basic variables.
        .exec();

    eprintln!("Failed to exec uwsm: {err}");
    eprintln!("Failed to exec fallback shell: {shell_err}");
    std::process::exit(1);
}

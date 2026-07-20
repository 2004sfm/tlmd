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

    // Change to user's home directory so they don't start in '/'
    if let Err(e) = std::env::set_current_dir(user.home_dir()) {
        eprintln!("Warning: Failed to change directory to home: {e}");
    }

    // Make sure basic environment variables are present just in case PAM missed them
    let env_home = user.home_dir().to_string_lossy().into_owned();
    let env_user = username.to_string();

    // 1. Run uwsm select (allows user to select compositor if needed)
    let select_status = std::process::Command::new("uwsm")
        .arg("select")
        .uid(user.uid())
        .gid(user.primary_group_id())
        .envs(envlist.iter_tuples())
        .env("HOME", &env_home)
        .env("USER", &env_user)
        .env("LOGNAME", &env_user)
        .status();
    
    if let Ok(sel_st) = select_status {
        if sel_st.success() {
            // 2. Exec into the selected compositor
            let err = std::process::Command::new("uwsm")
                .arg("start")
                .arg("default")
                .uid(user.uid())
                .gid(user.primary_group_id())
                .envs(envlist.iter_tuples())
                .env("HOME", &env_home)
                .env("USER", &env_user)
                .env("LOGNAME", &env_user)
                .exec();
            eprintln!("Failed to exec uwsm start default: {err}");
        }
    }

    // If uwsm is not installed, fails, or we didn't start it, fallback to the user's default login shell
    let shell_err = std::process::Command::new(user.shell())
        .arg("-l") // Launch as login shell
        .uid(user.uid())
        .gid(user.primary_group_id())
        .envs(envlist.iter_tuples()) // Pass the PAM env variables
        .env("HOME", &env_home)
        .env("USER", &env_user)
        .env("LOGNAME", &env_user)
        .exec();

    eprintln!("Failed to exec fallback shell: {shell_err}");
    std::process::exit(1);
}

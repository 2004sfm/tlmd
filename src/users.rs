use std::fs;

/// List real system users (UID >= 1000, with a valid login shell).
///
/// Reads `/etc/passwd` directly — no NSS/LDAP dependency, no unsafe code.
/// Intentional for a display manager that runs before network services are up.
pub fn list_real_users() -> Vec<String> {
    let content = fs::read_to_string("/etc/passwd").unwrap_or_default();

    content
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() < 7 {
                return None;
            }

            let name = fields[0];
            let uid: u32 = fields[2].parse().ok()?;
            let shell = fields[6];

            // UID >= 1000, not nobody (65534), has a real shell
            if uid >= 1000
                && uid != 65534
                && !shell.ends_with("/nologin")
                && !shell.ends_with("/false")
            {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect()
}

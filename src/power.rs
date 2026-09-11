pub fn logout() {
    match crate::compositor::Compositor::detect() {
        crate::compositor::Compositor::Niri => {
            let _ = std::process::Command::new("niri")
                .args(["msg", "action", "quit"])
                .spawn();
        }
        _ => {
            let _ = std::process::Command::new("hyprctl")
                .args(["dispatch", "exit"])
                .spawn();
        }
    }
}

// ----- needs nopasswd sudoers -----
pub fn suspend() {
    let _ = std::process::Command::new("sudo")
        .args(["-n", "zzz"])
        .spawn();
}

pub fn reboot() {
    let _ = std::process::Command::new("sudo")
        .args(["-n", "reboot"])
        .spawn();
}

pub fn poweroff() {
    let _ = std::process::Command::new("sudo")
        .args(["-n", "poweroff"])
        .spawn();
}

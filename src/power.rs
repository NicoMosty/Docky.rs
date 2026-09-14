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

// ----- intenta cada comando en orden: el primero que spawnee gana. `spawn`
// falla si el binario no existe, que es justo la señal de distro que nos
// interesa (systemd vs. Void): no hace falta detectar nada más. -----
fn run_first(cmds: &[(&str, &[&str])]) {
    for (cmd, args) in cmds {
        if std::process::Command::new(cmd).args(*args).spawn().is_ok() {
            return;
        }
    }
    log::warn!("power: ningún comando disponible");
}

pub fn suspend() {
    // ----- systemd no pide sudo (polkit); `zzz` es el fallback de Void, donde
    // se espera el NOPASSWD de siempre -----
    run_first(&[
        ("systemctl", &["suspend"]),
        ("loginctl", &["suspend"]),
        ("sudo", &["-n", "zzz"]),
    ]);
}

pub fn reboot() {
    run_first(&[("systemctl", &["reboot"]), ("sudo", &["-n", "reboot"])]);
}

pub fn poweroff() {
    run_first(&[("systemctl", &["poweroff"]), ("sudo", &["-n", "poweroff"])]);
}

#[cfg(test)]
mod power_tests {
    use super::run_first;

    /// El fallback funciona: el primer comando no existe, el segundo sí y corre.
    #[test]
    fn encadena_hasta_el_que_existe() {
        let dir = std::env::temp_dir().join("dockyrs-power-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let testigo = dir.join("corrio");
        run_first(&[
            ("dockyrs-comando-que-no-existe", &[]),
            ("touch", &[testigo.to_string_lossy().as_ref()]),
        ]);
        // ----- `run_first` es fire-and-forget (`spawn` no espera): se sondea el
        // testigo en vez de asumir que el hijo ya corrió -----
        let mut ok = false;
        for _ in 0..100 {
            if testigo.exists() {
                ok = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(ok);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

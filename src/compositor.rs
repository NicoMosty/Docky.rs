#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Compositor {
    Hyprland,
    Niri,
}

impl Compositor {
    pub fn detect() -> Self {
        if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
            Self::Hyprland
        } else if std::env::var("NIRI_SOCKET").is_ok() {
            Self::Niri
        } else if std::process::Command::new("niri")
            .args(["msg", "--json", "workspaces"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            // ponytail: `niri msg` probea el socket; solo corre al arrancar
            Self::Niri
        } else {
            Self::Hyprland
        }
    }
}

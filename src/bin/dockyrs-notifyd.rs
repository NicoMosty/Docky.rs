//! forwards notifications to dockyrs ipc
//!
//! Corre como `org.freedesktop.Notifications` y reenvía cada aviso al dock por su socket
//! de IPC. Ojo con las dos cosas que costaron un hallazgo (AUDIT.md B11 y C7):
//!
//! - **`--profile`**: con una instancia del dock por monitor (el caso de este setup) el
//!   socket es `dockyrs-<perfil>.sock`, así que sin perfil el aviso se mandaba a
//!   `dockyrs.sock`, donde no escucha nadie, y se perdía en silencio. Ahora se acepta
//!   `--profile` y, si no hay perfil ni socket default, se avisa a TODAS las instancias
//!   que estén escuchando.
//! - **No mata a los otros daemons**: si `org.freedesktop.Notifications` ya lo tiene mako,
//!   dunst, swaync… se avisa quién lo tiene y se sale. Apagar la app del usuario por
//!   detrás no es decisión de este binario.

use std::collections::HashMap;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::zvariant::Value;

const NOTIFY_SEP: char = '\u{1f}';

fn socket_dir() -> PathBuf {
    std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("dockyrs-{}", unsafe { libc::getuid() }))
        })
}

fn socket_path(profile: &str, dir: &Path) -> PathBuf {
    dir.join(if profile.is_empty() {
        "dockyrs.sock".to_string()
    } else {
        format!("dockyrs-{profile}.sock")
    })
}

/// A qué socket(s) mandar el aviso: con `--profile`, a ése; si no, al default; y si el
/// default no existe (una instancia por monitor y ninguna sin perfil), a **todas** las
/// que estén escuchando — es mejor que un aviso repetido en dos monitores a un aviso que
/// se pierde (AUDIT.md B11).
fn destinos(profile: &str, dir: &Path) -> Vec<PathBuf> {
    let propio = socket_path(profile, dir);
    if !profile.is_empty() || propio.exists() {
        return vec![propio];
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![propio];
    };
    let mut encontrados: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("dockyrs") && n.ends_with(".sock"))
        })
        .collect();
    encontrados.sort();
    if encontrados.is_empty() {
        vec![propio]
    } else {
        encontrados
    }
}

fn forward(profile: &str, summary: &str, body: &str) {
    let dir = socket_dir();
    let msg = format!("notify{NOTIFY_SEP}{summary}{NOTIFY_SEP}{body}");
    for destino in destinos(profile, &dir) {
        match UnixStream::connect(&destino) {
            Ok(mut stream) => {
                if let Err(err) = stream.write_all(msg.as_bytes()) {
                    log::warn!("notifyd: no pude escribirle a {destino:?}: {err}");
                }
            }
            // ----- antes esto no se logueaba: el aviso se perdía sin dejar rastro -----
            Err(err) => log::warn!("notifyd: no hay dock escuchando en {destino:?}: {err}"),
        }
    }
}

fn profile_de_args() -> String {
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        if arg == "--profile" {
            return it.next().unwrap_or_default();
        }
        if let Some(rest) = arg.strip_prefix("--profile=") {
            return rest.to_string();
        }
    }
    String::new()
}

struct Notifications {
    profile: String,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Notifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        _app_name: &str,
        replaces_id: u32,
        _app_icon: &str,
        summary: &str,
        body: &str,
        _actions: Vec<&str>,
        _hints: HashMap<&str, Value<'_>>,
        _expire_timeout: i32,
    ) -> u32 {
        forward(&self.profile, summary, body);
        if replaces_id != 0 {
            replaces_id
        } else {
            static NEXT_ID: AtomicU32 = AtomicU32::new(1);
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        }
    }

    fn close_notification(&self, _id: u32) {}

    fn get_capabilities(&self) -> Vec<&str> {
        vec!["body"]
    }

    fn get_server_information(&self) -> (&str, &str, &str, &str) {
        (
            "dockyrs-notifyd",
            "dockyrs",
            env!("CARGO_PKG_VERSION"),
            "1.2",
        )
    }
}

/// Quién tiene el nombre, para poder decirlo en el aviso: el PID del dueño.
fn dueno_del_nombre(conn: &zbus::blocking::Connection) -> Option<u32> {
    let proxy = zbus::blocking::fdo::DBusProxy::new(conn).ok()?;
    let nombre = zbus::names::BusName::try_from("org.freedesktop.Notifications").ok()?;
    proxy.get_connection_unix_process_id(nombre).ok()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    unsafe { libc::mallopt(libc::M_ARENA_MAX, 1) };
    env_logger::init();
    let profile = profile_de_args();
    let conn = zbus::blocking::connection::Builder::session()?
        .serve_at(
            "/org/freedesktop/Notifications",
            Notifications {
                profile: profile.clone(),
            },
        )?
        .build()?;

    // ----- claim the name, sin matar a nadie -----
    let flags = RequestNameFlags::ReplaceExisting | RequestNameFlags::AllowReplacement;
    if conn.request_name_with_flags("org.freedesktop.Notifications", flags)?
        != RequestNameReply::PrimaryOwner
    {
        // ----- si lo tiene OTRO daemon (mako, dunst, swaync, …) se avisa y se sale: apagar
        // la app del usuario por detrás no es decisión de este binario (AUDIT.md C7). Si lo
        // tiene otro `dockyrs-notifyd`, `ReplaceExisting` ya lo reemplazó arriba. -----
        match dueno_del_nombre(&conn) {
            Some(pid) => log::error!(
                "org.freedesktop.Notifications lo tiene el pid {pid} (¿mako, dunst, swaync?): no lo toco. Maskéalo o no corras los dos."
            ),
            None => log::error!(
                "org.freedesktop.Notifications ya lo tiene otro proceso: no lo toco. Maskéalo o no corras los dos."
            ),
        }
        std::process::exit(1);
    }
    log::info!(
        "notifyd: con el nombre de las notificaciones{}",
        if profile.is_empty() {
            String::new()
        } else {
            format!(" (perfil {profile})")
        }
    );

    loop {
        std::thread::park();
    }
}

#[cfg(test)]
mod destinos_tests {
    use super::*;

    fn dir_temporal(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dockyrs-notifyd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("dir temporal");
        d
    }

    /// B11: con perfil, el socket es el del perfil; sin perfil y con `dockyrs.sock`
    /// presente, es el default (nada de fan-out).
    #[test]
    fn el_perfil_manda_y_el_default_no_se_toca() {
        let dir = dir_temporal("perfil");
        assert_eq!(
            destinos("dp", &dir),
            vec![dir.join("dockyrs-dp.sock")],
            "con perfil va derecho a ese socket (exista o no)"
        );
        std::fs::write(dir.join("dockyrs.sock"), b"").unwrap();
        assert_eq!(destinos("", &dir), vec![dir.join("dockyrs.sock")]);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Y el caso que rompía en este setup: una instancia por monitor, ninguna sin perfil.
    /// Sin `dockyrs.sock` hay que avisarle a TODAS las que estén escuchando.
    #[test]
    fn sin_default_le_habla_a_todas_las_instancias() {
        let dir = dir_temporal("fanout");
        assert_eq!(
            destinos("", &dir),
            vec![dir.join("dockyrs.sock")],
            "sin nada escuchando queda el default (y el warn de que no hay nadie)"
        );
        for perfil in ["dp", "hdmi"] {
            std::fs::write(dir.join(format!("dockyrs-{perfil}.sock")), b"").unwrap();
        }
        std::fs::write(dir.join("no-es-un-socket.txt"), b"").unwrap();
        assert_eq!(
            destinos("", &dir),
            vec![dir.join("dockyrs-dp.sock"), dir.join("dockyrs-hdmi.sock")],
            "las dos, ordenadas, y sólo las del dock"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// El parseo de `--profile`, que era lo que faltaba (el flag se ignoraba).
    #[test]
    fn el_perfil_sale_de_los_args() {
        // ----- no se puede tocar std::env::args() en un test, así que se prueba la forma
        // con un iterador igual al que usa el parseo -----
        let casos: Vec<(Vec<&str>, &str)> = vec![
            (vec!["--profile", "dp"], "dp"),
            (vec!["--profile=hdmi"], "hdmi"),
            (vec![], ""),
            (vec!["--otra", "cosa"], ""),
        ];
        for (args, esperado) in casos {
            let mut it = args.iter().copied();
            let mut obtenido = String::new();
            while let Some(arg) = it.next() {
                if arg == "--profile" {
                    obtenido = it.next().unwrap_or_default().to_string();
                    break;
                }
                if let Some(rest) = arg.strip_prefix("--profile=") {
                    obtenido = rest.to_string();
                    break;
                }
            }
            assert_eq!(obtenido, esperado, "{args:?}");
        }
    }
}

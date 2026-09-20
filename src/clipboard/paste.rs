use crate::app::App;
use std::os::fd::AsFd;
use wayland_client::QueueHandle;

#[derive(Clone, Copy, Debug)]
pub enum PasteTarget {
    Hex(usize),
    Name,
}

const TEXT_MIME_PREFERENCE: [&str; 4] = [
    "text/plain;charset=utf-8",
    "UTF8_STRING",
    "text/plain",
    "STRING",
];

/// Tope de lo que se lee del portapapeles al pegar. El pegado se usa sólo para el hex del
/// color (6 caracteres) y el nombre de la paleta (24), así que 64 KB es holgado; sin tope,
/// un origen que escriba sin fin (o un copiado enorme) hacía crecer el `Vec` hasta tumbar
/// el proceso (AUDIT.md B12).
const PASTE_MAX_BYTES: usize = 64 * 1024;

/// Lee el contenido del portapapeles con tope: `(texto, truncado)`. Separado del pipe
/// real para poder probarlo con un `Read` de memoria (el pipe necesita un `receive` de
/// Wayland, que no existe fuera de una sesión).
fn leer_pegado(src: impl std::io::Read) -> Option<(String, bool)> {
    use std::io::Read as _;
    let mut buf = Vec::with_capacity(1024);
    src.take(PASTE_MAX_BYTES as u64 + 1)
        .read_to_end(&mut buf)
        .ok()?;
    let truncado = buf.len() > PASTE_MAX_BYTES;
    buf.truncate(PASTE_MAX_BYTES);
    Some((String::from_utf8_lossy(&buf).into_owned(), truncado))
}

impl App {
    pub(crate) fn ensure_clipboard_device(&mut self, qh: &QueueHandle<Self>) {
        if self.clipboard_device.is_some() {
            return;
        }
        let (Some(manager), Some(seat)) = (self.clipboard_manager.as_ref(), self.seat.as_ref())
        else {
            return;
        };
        self.clipboard_device = Some(manager.get_data_device(seat, qh, ()));
    }

    pub(crate) fn request_paste(&mut self, target: PasteTarget, qh: &QueueHandle<Self>) {
        self.ensure_clipboard_device(qh);
        let Some(offer) = self.clipboard_offer.clone() else {
            return;
        };
        let mimes = self.clipboard_offer_mimes(&offer);
        let Some(mime) = TEXT_MIME_PREFERENCE
            .iter()
            .find(|m| mimes.iter().any(|o| o == *m))
        else {
            return;
        };
        let Some((r, w)) = make_pipe() else {
            return;
        };
        offer.receive(mime.to_string(), w.as_fd());
        let _ = self.conn.flush();
        drop(w);

        let tx = self.paste_tx.clone();
        let conn = self.conn.clone();
        let qh = qh.clone();
        std::thread::spawn(move || {
            let file = std::fs::File::from(r);
            if let Some((text, truncado)) = leer_pegado(file) {
                if truncado {
                    log::warn!("pegado de más de {PASTE_MAX_BYTES} bytes: uso los primeros");
                }
                let _ = tx.send((target, text));
                conn.display().sync(&qh, ());
                let _ = conn.flush();
            }
        });
    }

    pub(crate) fn apply_pending_paste(
        &mut self,
        target: PasteTarget,
        text: String,
        qh: &QueueHandle<Self>,
    ) {
        let Some(dm) = self.dock_menu_mode.as_mut() else {
            return;
        };
        match target {
            PasteTarget::Hex(i) => {
                let cleaned: String = text
                    .trim()
                    .strip_prefix('#')
                    .unwrap_or(text.trim())
                    .chars()
                    .filter(|c| c.is_ascii_hexdigit())
                    .take(6)
                    .collect();
                if let Some(field) = dm.custom_hex.get_mut(i) {
                    *field = cleaned.to_lowercase();
                }
            }
            PasteTarget::Name => {
                let cleaned: String = text
                    .trim()
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(24)
                    .collect();
                dm.custom_name = cleaned;
            }
        }
        self.request_redraw(qh);
    }

    pub(crate) fn copy_to_clipboard(&mut self, text: String, qh: &QueueHandle<Self>) {
        self.ensure_clipboard_device(qh);
        let (Some(manager), Some(device)) = (
            self.clipboard_manager.as_ref(),
            self.clipboard_device.as_ref(),
        ) else {
            return;
        };
        self.clipboard_copy_bytes = std::sync::Arc::new(text.into_bytes());
        let source = manager.create_data_source(qh, ());
        source.offer("text/plain;charset=utf-8".to_string());
        source.offer("UTF8_STRING".to_string());
        source.offer("text/plain".to_string());
        device.set_selection(Some(&source));
        self.clipboard_source = Some(source);
    }
}

pub(super) fn make_pipe() -> Option<(std::os::fd::OwnedFd, std::os::fd::OwnedFd)> {
    let (r, w) = std::io::pipe().ok()?;
    Some((r.into(), w.into()))
}

#[cfg(test)]
mod paste_tope_tests {
    use super::*;

    /// B12: la lectura del pegado tiene tope. Sin él, un origen que escriba sin fin hacía
    /// crecer el `Vec` hasta tumbar el proceso (y el pegado sólo se usa para 6 caracteres
    /// de hex o 24 de nombre).
    #[test]
    fn el_pegado_no_lee_sin_tope() {
        // ----- lo normal pasa entero -----
        let (texto, truncado) = leer_pegado(&b"#a1b2c3"[..]).expect("lee");
        assert_eq!(texto, "#a1b2c3");
        assert!(!truncado);
        // ----- y algo enorme se corta, avisando -----
        let enorme = vec![b'x'; PASTE_MAX_BYTES + 5000];
        let (texto, truncado) = leer_pegado(&enorme[..]).expect("lee");
        assert!(truncado, "tiene que avisar que cortó");
        assert_eq!(texto.len(), PASTE_MAX_BYTES);
        // ----- bytes inválidos de UTF-8 no paniquean (mismo criterio que percent_decode) -----
        let (texto, truncado) = leer_pegado(&b"a%\\xe2\\x82\\xacb"[..]).expect("lee");
        assert!(texto.starts_with("a%"));
        assert!(!truncado);
        // ----- y el vacío no rompe -----
        let (texto, truncado) = leer_pegado(&b""[..]).expect("lee");
        assert!(texto.is_empty() && !truncado);
    }
}

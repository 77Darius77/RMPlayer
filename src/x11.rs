//! Intégration X11 minimale : « fenêtre toujours au-dessus ».
//!
//! **Pourquoi ne pas utiliser `ViewportCommand::WindowLevel` ?**
//! eframe/egui-winit l'exposent, mais leur implémentation X11 (winit 0.30) fait
//! `.expect("Failed to set window-level state")` : si la requête échoue, le
//! processus **panique** — le lecteur s'arrête brutalement au moment où
//! l'utilisateur active l'option. Constaté en test sur KWin/X11.
//!
//! On envoie donc nous-mêmes le message EWMH `_NET_WM_STATE` au gestionnaire de
//! fenêtres, ce qui est la méthode standard et n'arrête jamais le lecteur : en
//! cas d'échec, on renvoie une erreur que l'interface affiche.
//!
//! Hors X11 (Wayland par exemple), la fonction échoue proprement : la fenêtre
//! reste simplement dans son état normal.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageData, ClientMessageEvent, ConnectionExt, EventMask,
};

/// Actions EWMH de `_NET_WM_STATE`
const NET_WM_STATE_REMOVE: u32 = 0;
const NET_WM_STATE_ADD: u32 = 1;

/// Active ou désactive « toujours au-dessus » pour la fenêtre du processus courant.
pub fn set_always_on_top(enabled: bool) -> Result<(), String> {
    let (conn, screen_num) =
        x11rb::connect(None).map_err(|e| format!("serveur X11 injoignable ({})", e))?;
    let root = conn.setup().roots[screen_num].root;

    let intern = |name: &str| -> Result<u32, String> {
        conn.intern_atom(false, name.as_bytes())
            .map_err(|e| format!("requête X11 impossible: {}", e))?
            .reply()
            .map(|r| r.atom)
            .map_err(|e| format!("atome « {} » introuvable: {}", name, e))
    };

    let state_atom = intern("_NET_WM_STATE")?;
    let above_atom = intern("_NET_WM_STATE_ABOVE")?;
    let client_list_atom = intern("_NET_CLIENT_LIST")?;
    let pid_atom = intern("_NET_WM_PID")?;

    // Retrouver notre fenêtre : on parcourt la liste gérée par le gestionnaire de
    // fenêtres et on compare `_NET_WM_PID` au PID du processus.
    let window = find_own_window(&conn, root, client_list_atom, pid_atom)?
        .ok_or_else(|| "fenêtre du lecteur introuvable dans _NET_CLIENT_LIST".to_string())?;

    let action = if enabled {
        NET_WM_STATE_ADD
    } else {
        NET_WM_STATE_REMOVE
    };
    let event = ClientMessageEvent {
        response_type: x11rb::protocol::xproto::CLIENT_MESSAGE_EVENT,
        format: 32,
        sequence: 0,
        window,
        type_: state_atom,
        // data = [action, propriété visée, 0, indication de source (application)]
        data: ClientMessageData::from([action, above_atom, 0, 1, 0]),
    };

    // Le message s'adresse au gestionnaire de fenêtres, via la fenêtre racine.
    conn.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    )
    .map_err(|e| format!("envoi du message X11 impossible: {}", e))?
    .check()
    .map_err(|e| format!("gestionnaire de fenêtres injoignable: {}", e))?;

    conn.flush().map_err(|e| format!("envoi X11 impossible: {}", e))?;
    tracing::info!("Toujours au-dessus: {} (via _NET_WM_STATE)", enabled);
    Ok(())
}

/// Cherche la fenêtre dont `_NET_WM_PID` correspond au processus courant.
fn find_own_window(
    conn: &impl Connection,
    root: u32,
    client_list_atom: u32,
    pid_atom: u32,
) -> Result<Option<u32>, String> {
    let reply = conn
        .get_property(false, root, client_list_atom, AtomEnum::WINDOW, 0, u32::MAX)
        .map_err(|e| format!("lecture de _NET_CLIENT_LIST impossible: {}", e))?
        .reply()
        .map_err(|e| format!("_NET_CLIENT_LIST illisible: {}", e))?;

    let Some(windows) = reply.value32() else {
        return Ok(None);
    };
    let own_pid = std::process::id();

    for window in windows {
        let Ok(cookie) = conn.get_property(false, window, pid_atom, AtomEnum::CARDINAL, 0, 1) else {
            continue;
        };
        let Ok(prop) = cookie.reply() else {
            continue;
        };
        // `value32()` emprunte la réponse : on matérialise la valeur avant de sortir
        // de la portée de `prop`.
        let pid = prop.value32().and_then(|mut v| v.next());
        if pid == Some(own_pid) {
            return Ok(Some(window));
        }
    }
    Ok(None)
}


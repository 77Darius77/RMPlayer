//! Intégration au système : démarrage automatique à l'ouverture de session et priorité du
//! processus. Deux gestes sans rapport avec la lecture elle-même, regroupés ici parce qu'ils
//! parlent au système et non au moteur.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Démarrage automatique
// ---------------------------------------------------------------------------

/// `$XDG_CONFIG_HOME/autostart/rmplayer.desktop` : ce que lisent KDE, GNOME, XFCE… à l'ouverture
/// de session.
pub fn chemin_autostart() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("autostart").join("rmplayer.desktop"))
}

/// Commande à lancer : le lanceur `run.sh` du dépôt s'il existe (il pose `LD_LIBRARY_PATH`,
/// le binaire est lié à `/opt/ffmpeg-7.1`), sinon le binaire courant.
pub fn commande_de_lancement() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    for ancetre in exe.ancestors().skip(1).take(4) {
        let lanceur = ancetre.join("run.sh");
        if lanceur.is_file() {
            return Some(lanceur);
        }
    }
    Some(exe)
}

fn contenu_desktop(commande: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=RMPlayer\nComment=Lecteur vidéo\n\
         Exec={}\nIcon=rmplayer\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        commande.display()
    )
}

pub fn autostart_actif() -> bool {
    chemin_autostart().map(|p| p.is_file()).unwrap_or(false)
}

/// Active (`true`) ou désactive (`false`) le démarrage automatique.
pub fn regler_autostart(actif: bool) -> Result<(), String> {
    let chemin = chemin_autostart().ok_or("dossier de configuration introuvable")?;
    if actif {
        let commande = commande_de_lancement().ok_or("impossible de retrouver la commande de lancement")?;
        if let Some(dossier) = chemin.parent() {
            std::fs::create_dir_all(dossier).map_err(|e| e.to_string())?;
        }
        std::fs::write(&chemin, contenu_desktop(&commande)).map_err(|e| e.to_string())
    } else {
        match std::fs::remove_file(&chemin) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// Priorité du processus
// ---------------------------------------------------------------------------

/// Priorité d'ordonnancement du lecteur. « Élevée » sert en 4K, quand le décodage et le rendu
/// se disputent le processeur ; « Basse » laisse la main aux autres programmes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Priorite {
    Basse,
    #[default]
    Normale,
    Elevee,
}

impl Priorite {
    /// Valeur `nice` (Linux : plus petit = plus prioritaire).
    pub fn nice(self) -> i32 {
        match self {
            Priorite::Basse => 10,
            Priorite::Normale => 0,
            Priorite::Elevee => -5,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Priorite::Basse => "Basse",
            Priorite::Normale => "Normale",
            Priorite::Elevee => "Élevée",
        }
    }
}

/// Applique la priorité à **tous les fils d'exécution** du processus : sous Linux, `setpriority`
/// ne vise qu'un fil, et le lecteur décode, mixe le son et dessine dans des fils distincts.
/// Un refus du système (monter en priorité demande `RLIMIT_NICE`) est rapporté, pas fatal.
pub fn appliquer_priorite(priorite: Priorite) -> Result<(), String> {
    let nice = priorite.nice();
    let mut refus: Option<String> = None;
    let entrees = std::fs::read_dir("/proc/self/task").map_err(|e| e.to_string())?;
    for entree in entrees.flatten() {
        let Some(tid) = entree.file_name().to_str().and_then(|t| t.parse::<u32>().ok()) else { continue };
        // SAFETY : appel système sans pointeur ; un identifiant périmé renvoie ESRCH, ignoré.
        let r = unsafe { libc::setpriority(libc::PRIO_PROCESS, tid as libc::id_t, nice) };
        if r != 0 {
            let erreur = std::io::Error::last_os_error();
            if erreur.raw_os_error() != Some(libc::ESRCH) {
                refus = Some(erreur.to_string());
            }
        }
    }
    match refus {
        Some(e) => Err(format!("priorité {} refusée par le système ({e})", priorite.label())),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_priorites_donnent_des_valeurs_nice_ordonnees() {
        assert!(Priorite::Elevee.nice() < Priorite::Normale.nice());
        assert!(Priorite::Normale.nice() < Priorite::Basse.nice());
        assert_eq!(Priorite::default(), Priorite::Normale);
    }

    #[test]
    fn le_fichier_autostart_est_un_lanceur_valide() {
        let t = contenu_desktop(Path::new("/opt/rmplayer/run.sh"));
        assert!(t.starts_with("[Desktop Entry]\n"));
        assert!(t.contains("Exec=/opt/rmplayer/run.sh\n"));
        assert!(t.contains("Type=Application\n"));
    }
}

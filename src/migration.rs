//! Reprise des données de l'ancien nom du lecteur (KPlayer, devenu RMPlayer).
//!
//! La configuration, les positions de reprise, les signets, les thèmes et le journal de crash
//! vivent dans `~/.config/<nom>/` et `~/.cache/<nom>/`. Changer de nom sans les reprendre ferait
//! repartir l'utilisateur de zéro. Au premier lancement, si le nouveau dossier n'existe pas et
//! l'ancien oui, **l'ancien est COPIÉ** (jamais déplacé ni supprimé : un retour en arrière reste
//! possible) vers le nouveau.
//!
//! La copie se fait dans un dossier temporaire puis par renommage : un lancement interrompu en
//! cours de copie ne laisse pas un dossier à moitié rempli que le lancement suivant prendrait
//! pour une migration terminée.

use std::io;
use std::path::Path;

pub const ANCIEN: &str = "kplayer";
pub const NOUVEAU: &str = "rmplayer";

/// Un fichier de l'ancien dossier, sous son nom dans le nouveau (`kplayer.png` → `rmplayer.png`).
fn nom_migre(nom: &str) -> String {
    match nom.strip_prefix(ANCIEN) {
        Some(reste) if reste.starts_with('.') || reste.starts_with('-') => format!("{NOUVEAU}{reste}"),
        _ => nom.to_string(),
    }
}

fn copier(source: &Path, destination: &Path) -> io::Result<()> {
    std::fs::create_dir_all(destination)?;
    for entree in std::fs::read_dir(source)? {
        let entree = entree?;
        let nom = entree.file_name();
        let cible = destination.join(nom_migre(&nom.to_string_lossy()));
        if entree.file_type()?.is_dir() {
            copier(&entree.path(), &cible)?;
        } else {
            std::fs::copy(entree.path(), cible)?;
        }
    }
    Ok(())
}

/// Reprend `<base>/kplayer` dans `<base>/rmplayer`. `Ok(true)` si une copie a eu lieu.
pub fn migrer_dossier(base: &Path) -> io::Result<bool> {
    let ancien = base.join(ANCIEN);
    let nouveau = base.join(NOUVEAU);
    if !ancien.is_dir() || nouveau.exists() {
        return Ok(false);
    }
    let provisoire = base.join(format!("{NOUVEAU}.migration"));
    let _ = std::fs::remove_dir_all(&provisoire);
    copier(&ancien, &provisoire)?;
    std::fs::rename(&provisoire, &nouveau)?;
    Ok(true)
}

/// À appeler au tout début de `main`, avant tout accès à la configuration ou au cache.
pub fn migrer_donnees() {
    for base in [dirs::config_dir(), dirs::cache_dir()].into_iter().flatten() {
        match migrer_dossier(&base) {
            Ok(true) => eprintln!("RMPlayer : données de KPlayer reprises dans {}", base.join(NOUVEAU).display()),
            Ok(false) => {}
            Err(e) => eprintln!("RMPlayer : reprise des données de KPlayer impossible dans {} : {e}", base.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un dossier de travail unique par test (les tests tournent en parallèle ; on ne touche
    /// jamais à l'environnement du processus).
    fn dossier(nom: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("rmplayer-test-migration-{}-{nom}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn l_ancien_dossier_est_copie_avec_ses_sous_dossiers_et_reste_en_place() {
        let base = dossier("copie");
        let ancien = base.join(ANCIEN);
        std::fs::create_dir_all(ancien.join("themes")).unwrap();
        std::fs::write(ancien.join("config.json"), b"{\"a\":1}").unwrap();
        std::fs::write(ancien.join("themes/nuit.json"), b"{}").unwrap();
        std::fs::write(ancien.join("kplayer.png"), b"png").unwrap();

        assert!(migrer_dossier(&base).unwrap());
        let nouveau = base.join(NOUVEAU);
        assert_eq!(std::fs::read(nouveau.join("config.json")).unwrap(), b"{\"a\":1}");
        assert!(nouveau.join("themes/nuit.json").is_file());
        assert!(nouveau.join("rmplayer.png").is_file(), "l'icône prend le nouveau nom");
        assert!(!nouveau.join("kplayer.png").exists());
        assert!(ancien.join("config.json").is_file(), "l'ancien dossier n'est jamais touché");
        assert!(!base.join("rmplayer.migration").exists(), "pas de dossier provisoire laissé");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn un_nouveau_dossier_existant_n_est_jamais_ecrase() {
        let base = dossier("existant");
        std::fs::create_dir_all(base.join(ANCIEN)).unwrap();
        std::fs::write(base.join(ANCIEN).join("config.json"), b"ancien").unwrap();
        std::fs::create_dir_all(base.join(NOUVEAU)).unwrap();
        std::fs::write(base.join(NOUVEAU).join("config.json"), b"nouveau").unwrap();

        assert!(!migrer_dossier(&base).unwrap());
        assert_eq!(std::fs::read(base.join(NOUVEAU).join("config.json")).unwrap(), b"nouveau");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn sans_ancien_dossier_il_n_y_a_rien_a_faire() {
        let base = dossier("vide");
        assert!(!migrer_dossier(&base).unwrap());
        assert!(!base.join(NOUVEAU).exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn seuls_les_noms_commencant_par_l_ancien_nom_sont_renommes() {
        assert_eq!(nom_migre("kplayer.png"), "rmplayer.png");
        assert_eq!(nom_migre("kplayer-configuration.json"), "rmplayer-configuration.json");
        assert_eq!(nom_migre("config.json"), "config.json");
        assert_eq!(nom_migre("kplayers.txt"), "kplayers.txt");
    }
}

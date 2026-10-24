//! Mémorisation de la piste de sous-titres choisie à la main, média par média.
//!
//! Même mécanique que `positions.rs` (JSON dans `~/.cache/kplayer/`, relu au chargement,
//! écrit à chaque choix) : perdre ce fichier ne coûte qu'un nouveau choix, pas un réglage.
//! Activé par `subtitles.remember_track`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Ce que l'utilisateur avait choisi pour un média.
#[derive(Debug, Clone, PartialEq)]
pub enum Choix {
    /// Un fichier de sous-titres externe, par son chemin.
    Externe(PathBuf),
    /// Une piste intégrée au conteneur, par son index de flux.
    Integree(i32),
}

impl Choix {
    fn encoder(&self) -> String {
        match self {
            Choix::Externe(p) => format!("ext:{}", p.display()),
            Choix::Integree(i) => format!("int:{i}"),
        }
    }

    fn decoder(texte: &str) -> Option<Choix> {
        if let Some(p) = texte.strip_prefix("ext:") {
            return Some(Choix::Externe(PathBuf::from(p)));
        }
        texte.strip_prefix("int:")?.parse().ok().map(Choix::Integree)
    }
}

pub struct PistesChoisies {
    path: Option<PathBuf>,
    entries: HashMap<String, String>,
}

impl PistesChoisies {
    /// Charge le fichier (silencieusement vide s'il n'existe pas ou s'il est illisible).
    pub fn load() -> Self {
        let path = dirs::cache_dir().map(|d| d.join("kplayer").join("pistes.json"));
        let entries = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str::<HashMap<String, String>>(&t).ok())
            .unwrap_or_default();
        Self { path, entries }
    }

    pub fn get(&self, media: &Path) -> Option<Choix> {
        Choix::decoder(self.entries.get(&media.to_string_lossy().to_string())?)
    }

    /// Mémorise le choix ; ne réécrit pas le fichier si rien ne change.
    pub fn set(&mut self, media: &Path, choix: &Choix) {
        let cle = media.to_string_lossy().to_string();
        let valeur = choix.encoder();
        if self.entries.get(&cle) == Some(&valeur) {
            return;
        }
        self.entries.insert(cle, valeur);
        if let Err(e) = self.save() {
            tracing::warn!("Mémoire des pistes non enregistrée : {}", e);
        }
    }

    fn save(&self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Err("répertoire de cache introuvable".into());
        };
        if let Some(dossier) = path.parent() {
            std::fs::create_dir_all(dossier).map_err(|e| e.to_string())?;
        }
        let texte = serde_json::to_string_pretty(&self.entries).map_err(|e| e.to_string())?;
        std::fs::write(path, texte).map_err(|e| e.to_string())
    }

    #[cfg(test)]
    fn en_memoire() -> Self {
        Self { path: None, entries: HashMap::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_choix_s_encode_et_se_relit() {
        for choix in [Choix::Externe(PathBuf::from("/x/film.fre.srt")), Choix::Integree(3)] {
            assert_eq!(Choix::decoder(&choix.encoder()), Some(choix));
        }
        assert_eq!(Choix::decoder("n'importe quoi"), None);
        assert_eq!(Choix::decoder("int:pas-un-nombre"), None);
    }

    #[test]
    fn la_memoire_distingue_les_medias() {
        let mut m = PistesChoisies::en_memoire();
        m.set(Path::new("/a.mkv"), &Choix::Integree(2));
        m.set(Path::new("/b.mkv"), &Choix::Externe(PathBuf::from("/b.srt")));
        assert_eq!(m.get(Path::new("/a.mkv")), Some(Choix::Integree(2)));
        assert_eq!(m.get(Path::new("/b.mkv")), Some(Choix::Externe(PathBuf::from("/b.srt"))));
        assert_eq!(m.get(Path::new("/c.mkv")), None);
    }
}

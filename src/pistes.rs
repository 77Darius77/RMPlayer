//! Mémorisation de la piste de sous-titres choisie à la main, média par média.
//!
//! Même mécanique que `positions.rs` (JSON dans `~/.cache/rmplayer/`, relu au chargement,
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

/// Extensions de fichiers son qu'on propose comme piste audio externe.
pub const EXTENSIONS_AUDIO: &[&str] = &[
    "mka", "ac3", "eac3", "dts", "aac", "flac", "mp3", "mp2", "ogg", "opus", "wav", "m4a", "thd", "ape", "wv",
];

/// Fichiers son placés à côté d'un film et qui lui appartiennent : même dossier, nom commençant par celui du
/// film (`film.mkv` → `film.mka`, `film.fr.mka`, `film - français.ac3`). Triés par nom ; le film lui-même et
/// les fichiers qui ne commencent pas par son nom sont écartés. Un film réseau (sans dossier) n'en a pas.
pub fn audios_voisins(media: &Path) -> Vec<PathBuf> {
    let (Some(dossier), Some(nom)) = (media.parent(), media.file_stem().and_then(|n| n.to_str())) else {
        return Vec::new();
    };
    let nom = nom.to_lowercase();
    let Ok(entrees) = std::fs::read_dir(dossier) else {
        return Vec::new();
    };
    let mut voisins: Vec<PathBuf> = entrees
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p != media && p.is_file())
        .filter(|p| {
            let extension = p.extension().and_then(|e| e.to_str()).map(str::to_lowercase).unwrap_or_default();
            let debut = p.file_stem().and_then(|n| n.to_str()).map(str::to_lowercase).unwrap_or_default();
            EXTENSIONS_AUDIO.contains(&extension.as_str()) && debut.starts_with(&nom)
        })
        .collect();
    voisins.sort();
    voisins
}

/// Langue que le nom d'un fichier son voisin annonce : `film.fr.mka` pour `film.mkv` → « fr ». Vide si le nom
/// n'en dit rien (deux ou trois lettres entre le nom du film et l'extension, rien d'autre).
pub fn langue_du_nom(media: &Path, fichier: &Path) -> String {
    let (Some(film), Some(nom)) = (
        media.file_stem().and_then(|n| n.to_str()),
        fichier.file_stem().and_then(|n| n.to_str()),
    ) else {
        return String::new();
    };
    let Some(reste) = nom.to_lowercase().strip_prefix(&film.to_lowercase()).map(str::to_string) else {
        return String::new();
    };
    let jeton = reste.trim_matches(|c: char| !c.is_alphanumeric());
    if (2..=3).contains(&jeton.chars().count()) && jeton.chars().all(|c| c.is_alphabetic()) {
        jeton.to_string()
    } else {
        String::new()
    }
}

pub struct PistesChoisies {
    path: Option<PathBuf>,
    entries: HashMap<String, String>,
}

impl PistesChoisies {
    /// Charge le fichier (silencieusement vide s'il n'existe pas ou s'il est illisible).
    pub fn load() -> Self {
        Self::load_fichier("pistes.json")
    }

    /// Mémoire des pistes **audio** choisies à la main (fichier distinct de celui des sous-titres).
    pub fn load_audio() -> Self {
        Self::load_fichier("pistes-audio.json")
    }

    fn load_fichier(nom: &str) -> Self {
        let path = dirs::cache_dir().map(|d| d.join("rmplayer").join(nom));
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
    fn les_fichiers_son_voisins_sont_trouves_et_triés() {
        let dossier = std::env::temp_dir().join(format!("rmplayer-voisins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier);
        std::fs::create_dir_all(&dossier).unwrap();
        for nom in [
            "film.mkv", "film.mka", "film.fr.MKA", "film - VF.ac3", "film.srt", "autre.mka", "film2.flac",
            "mon film.mka", "film.txt",
        ] {
            std::fs::write(dossier.join(nom), b"x").unwrap();
        }
        std::fs::create_dir(dossier.join("film.dts")).unwrap(); // un dossier n'est pas un fichier son
        let trouves: Vec<String> = audios_voisins(&dossier.join("film.mkv"))
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        let _ = std::fs::remove_dir_all(&dossier);
        // « film2.flac » commence bien par « film » : c'est le prix d'une règle simple et prévisible.
        assert_eq!(trouves, vec!["film - VF.ac3", "film.fr.MKA", "film.mka", "film2.flac"]);
    }

    #[test]
    fn la_langue_se_lit_dans_le_nom_du_fichier_voisin() {
        let film = Path::new("/v/film.mkv");
        for (voisin, attendu) in [
            ("/v/film.fr.mka", "fr"),
            ("/v/Film.ENG.ac3", "eng"),
            ("/v/film - fre.dts", "fre"),
            ("/v/film.mka", ""),
            ("/v/film VF.ac3", "vf"), // deux lettres : retenu comme étiquette, sans prétendre que c'est une langue ISO
            ("/v/film.2.mka", ""),
            ("/v/autre.fr.mka", ""),
        ] {
            assert_eq!(langue_du_nom(film, Path::new(voisin)), attendu, "{voisin}");
        }
    }

    #[test]
    fn un_media_sans_dossier_n_a_pas_de_voisins() {
        assert!(audios_voisins(Path::new("/dossier/qui/n/existe/pas/film.mkv")).is_empty());
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

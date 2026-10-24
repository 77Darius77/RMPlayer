//! Marque-pages (signets) : des positions nommées dans un média, retrouvées à la réouverture.
//!
//! Fichier JSON `signets.json` à côté de `config.json` — dans le dossier de **configuration** :
//! contrairement aux positions de reprise (`positions.rs`, dans le cache), des signets posés à
//! la main ne doivent pas disparaître quand on vide un cache. Un média peut en avoir
//! plusieurs (une liste, pas une valeur unique).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Deux signets plus proches que ceci sont le même : un double appui n'en pose pas deux.
const PROXIMITE_MS: u64 = 1_000;
/// Marge pour « suivant » / « précédent » : on est « sur » un signet à moins de ceci.
const MARGE_NAVIGATION_MS: u64 = 600;
/// Plafond par média (un fichier édité à la main ne doit pas peser des mégaoctets).
const MAX_PAR_MEDIA: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Signet {
    pub ms: u64,
    #[serde(default)]
    pub nom: String,
}

pub struct Signets {
    path: Option<PathBuf>,
    par_media: BTreeMap<String, Vec<Signet>>,
}

fn cle(media: &Path) -> String {
    media.to_string_lossy().to_string()
}

impl Signets {
    /// Charge `signets.json` (vide s'il n'existe pas ou s'il est illisible).
    pub fn load() -> Self {
        let path = dirs::config_dir().map(|d| d.join("kplayer").join("signets.json"));
        let mut par_media: BTreeMap<String, Vec<Signet>> = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        for liste in par_media.values_mut() {
            liste.sort_by_key(|s| s.ms);
            liste.truncate(MAX_PAR_MEDIA);
        }
        Self { path, par_media }
    }

    #[cfg(test)]
    fn en_memoire() -> Self {
        Self { path: None, par_media: BTreeMap::new() }
    }

    pub fn liste(&self, media: &Path) -> &[Signet] {
        self.par_media.get(&cle(media)).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Ajoute un signet à `ms`. Faux si un signet existe déjà à moins d'une seconde ou si la
    /// liste est pleine.
    pub fn ajouter(&mut self, media: &Path, ms: u64) -> bool {
        let liste = self.par_media.entry(cle(media)).or_default();
        if liste.len() >= MAX_PAR_MEDIA || liste.iter().any(|s| s.ms.abs_diff(ms) < PROXIMITE_MS) {
            return false;
        }
        let nom = format!("Signet {}", liste.len() + 1);
        liste.push(Signet { ms, nom });
        liste.sort_by_key(|s| s.ms);
        self.enregistrer();
        true
    }

    /// Supprime le signet d'index `index`.
    pub fn supprimer(&mut self, media: &Path, index: usize) -> bool {
        let Some(liste) = self.par_media.get_mut(&cle(media)) else { return false };
        if index >= liste.len() {
            return false;
        }
        liste.remove(index);
        if liste.is_empty() {
            self.par_media.remove(&cle(media));
        }
        self.enregistrer();
        true
    }

    /// Index du signet le plus proche de `ms`, s'il est à moins de `PROXIMITE_MS * 1,5`.
    pub fn index_proche(&self, media: &Path, ms: u64) -> Option<usize> {
        self.liste(media)
            .iter()
            .enumerate()
            .filter(|(_, s)| s.ms.abs_diff(ms) <= PROXIMITE_MS * 3 / 2)
            .min_by_key(|(_, s)| s.ms.abs_diff(ms))
            .map(|(i, _)| i)
    }

    /// Premier signet strictement après la position courante (à la marge près).
    pub fn suivant(&self, media: &Path, ms: u64) -> Option<u64> {
        self.liste(media).iter().map(|s| s.ms).find(|&m| m > ms + MARGE_NAVIGATION_MS)
    }

    /// Dernier signet strictement avant la position courante (à la marge près).
    pub fn precedent(&self, media: &Path, ms: u64) -> Option<u64> {
        self.liste(media).iter().map(|s| s.ms).rev().find(|&m| m + MARGE_NAVIGATION_MS < ms)
    }

    fn enregistrer(&self) {
        let Some(path) = &self.path else { return };
        let ecrire = || -> Result<(), String> {
            if let Some(dossier) = path.parent() {
                std::fs::create_dir_all(dossier).map_err(|e| e.to_string())?;
            }
            let texte = serde_json::to_string_pretty(&self.par_media).map_err(|e| e.to_string())?;
            std::fs::write(path, texte).map_err(|e| e.to_string())
        };
        if let Err(e) = ecrire() {
            tracing::warn!("Signets non enregistrés : {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_signets_restent_tries_et_sans_doublon() {
        let mut s = Signets::en_memoire();
        let m = Path::new("/film.mkv");
        assert!(s.ajouter(m, 12_000));
        assert!(s.ajouter(m, 5_000));
        assert!(!s.ajouter(m, 5_400), "à moins d'une seconde d'un signet existant");
        let positions: Vec<u64> = s.liste(m).iter().map(|x| x.ms).collect();
        assert_eq!(positions, [5_000, 12_000]);
        assert!(s.liste(Path::new("/autre.mkv")).is_empty(), "par média");
    }

    #[test]
    fn suivant_et_precedent_ignorent_le_signet_sur_lequel_on_se_trouve() {
        let mut s = Signets::en_memoire();
        let m = Path::new("/film.mkv");
        for ms in [5_000, 12_000, 20_000] {
            s.ajouter(m, ms);
        }
        assert_eq!(s.suivant(m, 0), Some(5_000));
        assert_eq!(s.suivant(m, 5_100), Some(12_000), "on est sur 5 s : suivant = 12 s");
        assert_eq!(s.suivant(m, 20_000), None);
        assert_eq!(s.precedent(m, 12_200), Some(5_000), "on est sur 12 s : précédent = 5 s");
        assert_eq!(s.precedent(m, 5_000), None);
        assert_eq!(s.precedent(m, 30_000), Some(20_000));
    }

    #[test]
    fn la_suppression_retire_le_bon_signet() {
        let mut s = Signets::en_memoire();
        let m = Path::new("/film.mkv");
        s.ajouter(m, 5_000);
        s.ajouter(m, 12_000);
        assert_eq!(s.index_proche(m, 5_300), Some(0));
        assert_eq!(s.index_proche(m, 8_000), None, "aucun signet à proximité");
        assert!(s.supprimer(m, 0));
        assert_eq!(s.liste(m).iter().map(|x| x.ms).collect::<Vec<_>>(), [12_000]);
        assert!(!s.supprimer(m, 5), "index hors liste");
    }
}

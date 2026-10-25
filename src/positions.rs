//! Mémorisation des positions de lecture — « reprendre là où on s'est arrêté ».
//!
//! Fichier JSON simple (`~/.cache/rmplayer/positions.json`) : chemin du média →
//! position en millisecondes. Écrit à la fermeture d'un média ou à la sortie,
//! relu au chargement suivant. Écrasé une fois pour toutes en fin de lecture
//! (position trop proche de la fin), pour ne pas reprendre sur le générique.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Position en deçà de laquelle on ne mémorise rien (début de film)
const MIN_SAVE_MS: u64 = 5_000;
/// Marge avant la fin : au-delà, l'entrée est oubliée
const END_MARGIN_MS: u64 = 10_000;

pub struct Positions {
    path: Option<PathBuf>,
    entries: HashMap<String, u64>,
}

impl Positions {
    /// Charge le fichier de positions (silencieusement vide s'il n'existe pas).
    pub fn load() -> Self {
        let path = dirs::cache_dir().map(|d| d.join("rmplayer").join("positions.json"));
        let entries = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| serde_json::from_str::<HashMap<String, u64>>(&text).ok())
            .unwrap_or_default();
        Self { path, entries }
    }

    fn key(path: &Path) -> String {
        path.to_string_lossy().to_string()
    }

    /// Position mémorisée pour ce média, si elle est pertinente vis-à-vis de la durée.
    pub fn get(&self, path: &Path, duration_ms: u64) -> Option<u64> {
        let value = *self.entries.get(&Self::key(path))?;
        if value < MIN_SAVE_MS {
            return None;
        }
        if duration_ms > 0 && value >= duration_ms.saturating_sub(END_MARGIN_MS) {
            return None;
        }
        Some(value)
    }

    /// Mémorise la position (ou efface l'entrée si trop proche du début / de la fin).
    pub fn remember(&mut self, path: &Path, position_ms: u64, duration_ms: u64) {
        let key = Self::key(path);
        let near_end = duration_ms > 0 && position_ms >= duration_ms.saturating_sub(END_MARGIN_MS);
        if position_ms < MIN_SAVE_MS || near_end {
            if self.entries.remove(&key).is_some() {
                let _ = self.save();
            }
            return;
        }
        self.entries.insert(key, position_ms);
        let _ = self.save();
    }

    fn save(&self) -> Result<(), String> {
        let Some(path) = self.path.as_ref() else {
            return Err("Aucun répertoire de cache".to_string());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(&self.entries).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }
}

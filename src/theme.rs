//! Thèmes externes : des fichiers JSON dans `~/.config/kplayer/themes/` qui remplacent les
//! couleurs de l'interface. Le thème intégré (clair / sombre d'egui) reste le défaut ; un thème
//! illisible est ignoré avec un message, jamais une interface cassée.
//!
//! Format (toutes les couleurs en RRGGBB, tous les champs facultatifs sauf `base`) :
//! ```json
//! { "base": "dark", "fond": "1B1D2A", "zone": "11121B", "texte": "E6E6F0", "accent": "7AA2F7" }
//! ```

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Theme {
    /// « dark » ou « light » : les valeurs d'egui dont on part.
    pub base: String,
    /// Fond des barres, panneaux et fenêtres.
    pub fond: Option<String>,
    /// Fond des zones de saisie et des curseurs.
    pub zone: Option<String>,
    /// Couleur du texte.
    pub texte: Option<String>,
    /// Couleur de sélection et des liens.
    pub accent: Option<String>,
}

/// `$XDG_CONFIG_HOME/kplayer/themes`.
pub fn dossier() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("kplayer").join("themes"))
}

/// Noms (sans `.json`) des thèmes présents, triés.
pub fn lister() -> Vec<String> {
    let Some(dossier) = dossier() else { return Vec::new() };
    let mut noms: Vec<String> = std::fs::read_dir(dossier)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let chemin = e.path();
            if chemin.extension()? != "json" {
                return None;
            }
            Some(chemin.file_stem()?.to_string_lossy().to_string())
        })
        .collect();
    noms.sort();
    noms
}

fn couleur(texte: &str) -> Result<egui::Color32, String> {
    let t = texte.trim().trim_start_matches('#');
    if t.len() != 6 {
        return Err(format!("couleur « {texte} » : six chiffres hexadécimaux attendus (RRGGBB)"));
    }
    let v = u32::from_str_radix(t, 16).map_err(|_| format!("couleur « {texte} » illisible"))?;
    Ok(egui::Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

impl Theme {
    pub fn lire(texte: &str) -> Result<Theme, String> {
        let theme: Theme = serde_json::from_str(texte).map_err(|e| format!("thème illisible : {e}"))?;
        if theme.base != "dark" && theme.base != "light" {
            return Err(format!("« base » doit valoir dark ou light (reçu « {} »)", theme.base));
        }
        // Valide les couleurs d'emblée : un thème à moitié bon serait pire qu'un thème refusé.
        for c in [&theme.fond, &theme.zone, &theme.texte, &theme.accent].into_iter().flatten() {
            couleur(c)?;
        }
        Ok(theme)
    }

    pub fn charger(nom: &str) -> Result<Theme, String> {
        let chemin = dossier().ok_or("dossier de configuration introuvable")?.join(format!("{nom}.json"));
        let texte = std::fs::read_to_string(&chemin)
            .map_err(|e| format!("thème « {nom} » : lecture de {} impossible ({e})", chemin.display()))?;
        Self::lire(&texte).map_err(|e| format!("thème « {nom} » : {e}"))
    }

    pub fn visuals(&self) -> egui::Visuals {
        let mut v = if self.base == "light" { egui::Visuals::light() } else { egui::Visuals::dark() };
        // Les couleurs ont été validées par `lire` : `unwrap_or` ne sert que de garde-fou.
        let lire = |c: &Option<String>| c.as_deref().and_then(|t| couleur(t).ok());
        if let Some(fond) = lire(&self.fond) {
            v.panel_fill = fond;
            v.window_fill = fond;
            v.widgets.noninteractive.bg_fill = fond;
        }
        if let Some(zone) = lire(&self.zone) {
            v.extreme_bg_color = zone;
            v.faint_bg_color = zone;
        }
        if let Some(texte) = lire(&self.texte) {
            v.override_text_color = Some(texte);
        }
        if let Some(accent) = lire(&self.accent) {
            v.selection.bg_fill = accent;
            v.hyperlink_color = accent;
        }
        v
    }

    /// Exemple écrit par le bouton « Créer un exemple » des préférences.
    pub const EXEMPLE: &'static str = "{\n  \"base\": \"dark\",\n  \"fond\": \"1B1D2A\",\n  \"zone\": \"11121B\",\n  \"texte\": \"E6E6F0\",\n  \"accent\": \"7AA2F7\"\n}\n";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_theme_valide_se_lit_et_colore_les_visuels() {
        let t = Theme::lire(Theme::EXEMPLE).unwrap();
        let v = t.visuals();
        assert_eq!(v.panel_fill, egui::Color32::from_rgb(0x1B, 0x1D, 0x2A));
        assert_eq!(v.override_text_color, Some(egui::Color32::from_rgb(0xE6, 0xE6, 0xF0)));
        assert_eq!(v.selection.bg_fill, egui::Color32::from_rgb(0x7A, 0xA2, 0xF7));
        // Sans couleur : les valeurs d'egui restent celles de la base.
        let nu = Theme::lire(r#"{"base": "light"}"#).unwrap().visuals();
        assert_eq!(nu.panel_fill, egui::Visuals::light().panel_fill);
    }

    #[test]
    fn un_theme_invalide_est_refuse_avec_la_raison() {
        assert!(Theme::lire("pas du json").is_err());
        assert!(Theme::lire(r#"{"base": "rose"}"#).unwrap_err().contains("dark ou light"));
        assert!(Theme::lire(r#"{"base": "dark", "fond": "12345"}"#).unwrap_err().contains("RRGGBB"));
        assert!(Theme::lire(r#"{"base": "dark", "texte": "ZZZZZZ"}"#).is_err());
        assert!(Theme::lire(r#"{"fond": "112233"}"#).is_err(), "« base » est obligatoire");
    }
}

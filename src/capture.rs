//! Captures d'écran : format (PNG, JPEG, WebP), qualité JPEG et nom de fichier.
//!
//! Les encodeurs sont en pur Rust (`png`, et `image` limité à ses encodeurs JPEG et WebP) : aucune
//! bibliothèque système de plus. **WebP est toujours sans perte** — l'encodeur disponible hors ligne
//! n'écrit que cela ; un WebP avec perte demanderait `libwebp`, que ce FFmpeg n'a pas non plus.

use crate::config::CaptureFormat;
use chrono::{DateTime, Local, Timelike};
use image::{ExtendedColorType, ImageEncoder};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

/// Modèle de nom par défaut (celui d'avant les réglages).
pub const MODELE_DEFAUT: &str = "rmplayer-{date}-{heure}";

/// Extension (sans point) d'un format.
pub fn extension(format: CaptureFormat) -> &'static str {
    match format {
        CaptureFormat::Png => "png",
        CaptureFormat::Jpeg => "jpg",
        CaptureFormat::Webp => "webp",
    }
}

/// Qualité JPEG bornée à 1–100 (0 ou 255 ne doivent pas faire échouer une capture).
pub fn qualite_jpeg(demandee: u8) -> u8 {
    demandee.clamp(1, 100)
}

/// Écrit l'image RGBA (`largeur × hauteur × 4` octets) dans le format demandé.
pub fn ecrire(
    chemin: &Path,
    format: CaptureFormat,
    qualite: u8,
    largeur: u32,
    hauteur: u32,
    rgba: &[u8],
) -> Result<(), String> {
    let attendu = largeur as usize * hauteur as usize * 4;
    if largeur == 0 || hauteur == 0 || rgba.len() < attendu {
        return Err("image incomplète".to_string());
    }
    let rgba = &rgba[..attendu];
    let fichier = std::fs::File::create(chemin).map_err(|e| format!("création du fichier: {e}"))?;
    let mut sortie = BufWriter::new(fichier);
    match format {
        CaptureFormat::Png => {
            let mut encodeur = png::Encoder::new(&mut sortie, largeur, hauteur);
            encodeur.set_color(png::ColorType::Rgba);
            encodeur.set_depth(png::BitDepth::Eight);
            let mut ecrivain = encodeur.write_header().map_err(|e| format!("en-tête PNG: {e}"))?;
            ecrivain.write_image_data(rgba).map_err(|e| format!("écriture PNG: {e}"))?;
        }
        CaptureFormat::Jpeg => {
            // Le JPEG n'a pas de transparence : on retire le canal alpha.
            let mut rvb = Vec::with_capacity(largeur as usize * hauteur as usize * 3);
            for px in rgba.chunks_exact(4) {
                rvb.extend_from_slice(&px[..3]);
            }
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut sortie, qualite_jpeg(qualite))
                .write_image(&rvb, largeur, hauteur, ExtendedColorType::Rgb8)
                .map_err(|e| format!("écriture JPEG: {e}"))?;
        }
        CaptureFormat::Webp => {
            image::codecs::webp::WebPEncoder::new_lossless(&mut sortie)
                .write_image(rgba, largeur, hauteur, ExtendedColorType::Rgba8)
                .map_err(|e| format!("écriture WebP: {e}"))?;
        }
    }
    std::io::Write::flush(&mut sortie).map_err(|e| format!("écriture: {e}"))?;
    Ok(())
}

/// Position en `HH-MM-SS` (les deux-points sont interdits dans certains systèmes de fichiers).
fn position_texte(ms: u64) -> String {
    let s = ms / 1000;
    format!("{:02}-{:02}-{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// Remplace ce qu'un nom de fichier ne peut pas contenir (séparateurs de dossier compris : un `/`
/// dans le modèle ne doit pas créer de sous-dossier) ; borne la longueur.
fn nettoyer(nom: &str) -> String {
    let propre: String = nom
        .chars()
        .map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    propre.trim().trim_matches('.').chars().take(150).collect::<String>().trim().to_string()
}

/// Nom de fichier (sans extension) tiré du modèle : `{media}` (titre du film sans extension),
/// `{position}` (`HH-MM-SS`), `{date}` (`AAAAMMJJ`), `{heure}` (`HHMMSS`). Un modèle vide, ou qui
/// ne donne rien après nettoyage, retombe sur le modèle par défaut.
pub fn nom_depuis_modele(
    modele: &str,
    media: Option<&str>,
    position_ms: u64,
    maintenant: DateTime<Local>,
) -> String {
    let remplir = |m: &str| {
        m.replace("{media}", media.unwrap_or("flux"))
            .replace("{position}", &position_texte(position_ms))
            .replace("{date}", &maintenant.format("%Y%m%d").to_string())
            .replace(
                "{heure}",
                &format!("{:02}{:02}{:02}", maintenant.hour(), maintenant.minute(), maintenant.second()),
            )
    };
    let nom = nettoyer(&remplir(modele));
    if nom.is_empty() {
        nettoyer(&remplir(MODELE_DEFAUT))
    } else {
        nom
    }
}

/// `<dossier>/<nom>.<ext>`, ou `<nom>-2.<ext>`, `-3`… si le fichier existe : deux captures qui
/// portent le même nom (un modèle sans heure) ne s'écrasent pas.
pub fn chemin_libre(dossier: &Path, nom: &str, ext: &str) -> PathBuf {
    let premier = dossier.join(format!("{nom}.{ext}"));
    if !premier.exists() {
        return premier;
    }
    (2..10_000)
        .map(|n| dossier.join(format!("{nom}-{n}.{ext}")))
        .find(|p| !p.exists())
        .unwrap_or(premier)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn instant() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, 25, 18, 5, 9).unwrap()
    }

    #[test]
    fn le_modele_remplace_media_position_date_et_heure() {
        let n = nom_depuis_modele("{media}_{position}_{date}-{heure}", Some("Mon film"), 3_725_000, instant());
        assert_eq!(n, "Mon film_01-02-05_20261025-180509");
    }

    #[test]
    fn un_modele_vide_ou_sans_resultat_retombe_sur_le_defaut() {
        assert_eq!(nom_depuis_modele("", None, 0, instant()), "rmplayer-20261025-180509");
        assert_eq!(nom_depuis_modele("  ", None, 0, instant()), "rmplayer-20261025-180509");
        assert_eq!(nom_depuis_modele("///", None, 0, instant()), "___", "des « / » donnent « ___ » : conservé, pas vide");
    }

    #[test]
    fn les_separateurs_et_caracteres_interdits_deviennent_des_soulignes() {
        assert_eq!(nom_depuis_modele("a/b", None, 0, instant()), "a_b");
        assert_eq!(nom_depuis_modele("a\\\\b:c*d?e", None, 0, instant()), "a__b_c_d_e");
        assert_eq!(nom_depuis_modele("..", None, 0, instant()), "rmplayer-20261025-180509", "un nom fait de points est refusé");
        let long = "x".repeat(400);
        assert_eq!(nom_depuis_modele(&long, None, 0, instant()).len(), 150);
    }

    #[test]
    fn un_flux_sans_titre_s_appelle_flux() {
        assert_eq!(nom_depuis_modele("{media}", None, 0, instant()), "flux");
    }

    #[test]
    fn la_qualite_jpeg_est_bornee() {
        assert_eq!(qualite_jpeg(0), 1);
        assert_eq!(qualite_jpeg(255), 100);
        assert_eq!(qualite_jpeg(92), 92);
    }

    #[test]
    fn chemin_libre_ne_remplace_jamais_un_fichier_existant() {
        let d = std::env::temp_dir().join(format!("rmplayer-test-capture-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let p1 = chemin_libre(&d, "cap", "png");
        assert_eq!(p1, d.join("cap.png"));
        std::fs::write(&p1, b"x").unwrap();
        assert_eq!(chemin_libre(&d, "cap", "png"), d.join("cap-2.png"));
        std::fs::write(d.join("cap-2.png"), b"x").unwrap();
        assert_eq!(chemin_libre(&d, "cap", "png"), d.join("cap-3.png"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn chaque_format_ecrit_un_fichier_a_la_bonne_signature() {
        let d = std::env::temp_dir().join(format!("rmplayer-test-formats-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let rgba: Vec<u8> = (0..16 * 16).flat_map(|i| [(i % 256) as u8, 100, 200, 255]).collect();
        for (format, debut) in [
            (CaptureFormat::Png, &b"\x89PNG\r\n\x1a\n"[..]),
            (CaptureFormat::Jpeg, &b"\xff\xd8\xff"[..]),
            (CaptureFormat::Webp, &b"RIFF"[..]),
        ] {
            let p = d.join(format!("t.{}", extension(format)));
            ecrire(&p, format, 80, 16, 16, &rgba).unwrap();
            let octets = std::fs::read(&p).unwrap();
            assert!(octets.starts_with(debut), "{format:?} : signature");
            if format == CaptureFormat::Webp {
                assert_eq!(&octets[8..12], b"WEBP");
            }
        }
        assert!(ecrire(&d.join("x.png"), CaptureFormat::Png, 80, 16, 16, &rgba[..10]).is_err(), "image incomplète refusée");
        let _ = std::fs::remove_dir_all(&d);
    }
}

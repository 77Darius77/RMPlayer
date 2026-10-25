//! Shaders de pixels : des fichiers GLSL au format « hook » de mpv, exécutés sur le GPU (Vulkan) par le
//! filtre `libplacebo` de FFmpeg (`custom_shader_path`).
//!
//! **Pourquoi concaténer.** `libplacebo` n'accepte qu'UN chemin de shader. Les fichiers actifs sont
//! donc assemblés, dans l'ordre de la liste, en un seul fichier du cache (`shaders-actifs.glsl`) :
//! chaque shader mpv commence par ses propres lignes `//!HOOK`, la concaténation est valide, et
//! l'ordre de la liste est l'ordre d'exécution.
//!
//! **Ce qui est refusé, sans jamais empêcher la lecture** : un fichier absent, trop gros (> 256 Kio),
//! illisible, ou qui ne contient pas de ligne `//!HOOK` (ce n'est pas un shader mpv) est ignoré avec
//! un message. Un shader qui ne compile pas est refusé plus tard, par `libplacebo` : le moteur retire
//! alors l'étage et le dit (`apply_video_filters`).

use std::path::{Path, PathBuf};

/// Taille maximale d'un shader.
const TAILLE_MAX: u64 = 256 * 1024;

/// Vérifie qu'un fichier est un shader utilisable ; renvoie son texte.
pub fn lire(chemin: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(chemin).map_err(|e| format!("illisible ({e})"))?;
    if !meta.is_file() {
        return Err("ce n'est pas un fichier".into());
    }
    if meta.len() > TAILLE_MAX {
        return Err(format!("trop gros ({} Ko, maximum {} Ko)", meta.len() / 1024, TAILLE_MAX / 1024));
    }
    let texte = std::fs::read_to_string(chemin).map_err(|e| format!("illisible ({e})"))?;
    if !texte.lines().any(|l| l.trim_start().starts_with("//!HOOK")) {
        return Err("aucune ligne « //!HOOK » : ce n'est pas un shader au format mpv".into());
    }
    Ok(texte)
}

/// Chemin du fichier assemblé.
fn chemin_assemble() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("rmplayer").join("shaders-actifs.glsl"))
}

/// Un chemin sûr pour une description de filtre FFmpeg : les caractères qui ont un sens dans une
/// chaîne de filtres (`:`, `,`, `'`, `\\`, `[`, `]`, `;`, espace) y demanderaient un échappement à deux
/// niveaux, source d'erreurs. On refuse plutôt que d'échapper de travers.
fn chemin_sans_danger(chemin: &Path) -> bool {
    !chemin.to_string_lossy().chars().any(|c| ":,'\\[];= \n".contains(c))
}

/// Assemble les shaders valides de `chemins` (dans l'ordre) en un fichier et renvoie son chemin, avec
/// la liste de ceux qui ont été ignorés et pourquoi. `None` si aucun shader n'est utilisable.
pub fn assembler(chemins: &[String]) -> (Option<PathBuf>, Vec<String>) {
    let mut ignores = Vec::new();
    let mut texte = String::new();
    let mut retenus = 0;
    for brut in chemins {
        let chemin = crate::config::Config::expand_path(brut);
        match lire(&chemin) {
            Ok(t) => {
                texte.push_str(&format!("// ---- {} ----\n", chemin.display()));
                texte.push_str(&t);
                if !t.ends_with('\n') {
                    texte.push('\n');
                }
                retenus += 1;
            }
            Err(raison) => ignores.push(format!("{} : {}", chemin.display(), raison)),
        }
    }
    if retenus == 0 {
        return (None, ignores);
    }
    let Some(sortie) = chemin_assemble() else {
        ignores.push("dossier de cache introuvable".into());
        return (None, ignores);
    };
    if !chemin_sans_danger(&sortie) {
        ignores.push(format!("{} : chemin du cache inutilisable dans une chaîne de filtres", sortie.display()));
        return (None, ignores);
    }
    let ecrire = || -> std::io::Result<()> {
        if let Some(dossier) = sortie.parent() {
            std::fs::create_dir_all(dossier)?;
        }
        // Réécrit seulement si le contenu change (le graphe n'a pas à être reconstruit pour rien).
        if std::fs::read_to_string(&sortie).ok().as_deref() != Some(texte.as_str()) {
            std::fs::write(&sortie, &texte)?;
        }
        Ok(())
    };
    match ecrire() {
        Ok(()) => (Some(sortie), ignores),
        Err(e) => {
            ignores.push(format!("{} : écriture impossible ({e})", sortie.display()));
            (None, ignores)
        }
    }
}

/// Shaders d'exemple, écrits dans `~/.config/rmplayer/shaders/` à la demande.
pub fn exemples() -> [(&'static str, &'static str); 4] {
    [
        (
            "gris.glsl",
            "//!HOOK MAIN\n//!BIND HOOKED\n//!DESC niveaux de gris (luminance)\nvec4 hook() {\n    vec4 c = HOOKED_texOff(0);\n    float y = dot(c.rgb, vec3(0.2126, 0.7152, 0.0722));\n    return vec4(y, y, y, c.a);\n}\n",
        ),
        (
            "sepia.glsl",
            "//!HOOK MAIN\n//!BIND HOOKED\n//!DESC sépia\nvec4 hook() {\n    vec4 c = HOOKED_texOff(0);\n    float y = dot(c.rgb, vec3(0.2126, 0.7152, 0.0722));\n    return vec4(y * vec3(1.07, 0.74, 0.43), c.a);\n}\n",
        ),
        (
            "nettete.glsl",
            "//!HOOK MAIN\n//!BIND HOOKED\n//!DESC netteté (masque flou léger)\nvec4 hook() {\n    vec4 c = HOOKED_texOff(vec2(0.0));\n    vec4 flou = (HOOKED_texOff(vec2(-1.0, 0.0)) + HOOKED_texOff(vec2(1.0, 0.0))\n               + HOOKED_texOff(vec2(0.0, -1.0)) + HOOKED_texOff(vec2(0.0, 1.0))) * 0.25;\n    return vec4(clamp(c.rgb + 0.6 * (c.rgb - flou.rgb), 0.0, 1.0), c.a);\n}\n",
        ),
        (
            "inverse.glsl",
            "//!HOOK MAIN\n//!BIND HOOKED\n//!DESC négatif\nvec4 hook() {\n    vec4 c = HOOKED_texOff(0);\n    return vec4(1.0 - c.rgb, c.a);\n}\n",
        ),
    ]
}

/// Écrit les shaders d'exemple dans `dossier` (sans écraser un fichier existant) ; renvoie les chemins
/// des fichiers présents.
pub fn installer_exemples(dossier: &Path) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dossier).map_err(|e| format!("création du dossier : {e}"))?;
    let mut chemins = Vec::new();
    for (nom, contenu) in exemples() {
        let chemin = dossier.join(nom);
        if !chemin.exists() {
            std::fs::write(&chemin, contenu).map_err(|e| format!("écriture de {nom} : {e}"))?;
        }
        chemins.push(chemin);
    }
    Ok(chemins)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rmplayer-test-shaders-{}-{nom}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn les_exemples_sont_des_shaders_mpv_valides() {
        let d = dossier("exemples");
        let chemins = installer_exemples(&d).unwrap();
        assert_eq!(chemins.len(), 4);
        for c in &chemins {
            let t = lire(c).unwrap_or_else(|e| panic!("{} : {e}", c.display()));
            assert!(t.contains("//!BIND HOOKED") && t.contains("vec4 hook()"), "{}", c.display());
        }
        // un fichier modifié par l'utilisateur n'est jamais écrasé
        std::fs::write(d.join("gris.glsl"), "//!HOOK MAIN\n// à moi\n").unwrap();
        installer_exemples(&d).unwrap();
        assert!(std::fs::read_to_string(d.join("gris.glsl")).unwrap().contains("à moi"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn un_fichier_sans_hook_absent_ou_trop_gros_est_refuse_avec_la_raison() {
        let d = dossier("refus");
        std::fs::write(d.join("texte.txt"), "bonjour").unwrap();
        assert!(lire(&d.join("texte.txt")).unwrap_err().contains("HOOK"));
        assert!(lire(&d.join("absent.glsl")).unwrap_err().contains("illisible"));
        assert!(lire(&d).unwrap_err().contains("pas un fichier"));
        std::fs::write(d.join("gros.glsl"), format!("//!HOOK MAIN\n{}", "x".repeat(300 * 1024))).unwrap();
        assert!(lire(&d.join("gros.glsl")).unwrap_err().contains("trop gros"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn le_chemin_du_cache_ne_doit_pas_contenir_de_caractere_de_filtre() {
        assert!(chemin_sans_danger(Path::new("/home/x/.cache/rmplayer/shaders-actifs.glsl")));
        for mauvais in ["/tmp/a:b/x.glsl", "/tmp/a,b/x.glsl", "/tmp/a b/x.glsl", "/tmp/a'b/x.glsl", "/tmp/a[b]/x.glsl"] {
            assert!(!chemin_sans_danger(Path::new(mauvais)), "{mauvais}");
        }
    }
}

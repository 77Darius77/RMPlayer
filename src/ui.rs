//! Interface utilisateur — egui 0.33
//! 
//! Interface optimisée Linux :
//!   - Contrôles en bas (barre de progression, boutons, volume)
//!   - OSD contextuel (infos média, statistiques)
//!   - Raccourcis **tous personnalisables** (`raccourcis.rs`) :
//!     la table effective est le seul endroit où une touche est décidée, et l'aide `F1`
//!     en est construite — elle ne peut donc pas mentir
//!   - Menu contextuel au clic droit sur l'image (session 25)
//!   - Mode plein écran avec masquage des panneaux
//!
//! Raccourcis par défaut : Espace, flèches (±1/5/30 s), ↑↓ volume, M muet, V sous-titres,
//! F/Entrée plein écran, S capture PNG, O OSD, I statistiques, L playlist, F5 préférences,
//! [ ] vitesse (\ revient à 1,0), N/P navigation, H décodage matériel/logiciel, Q quitter
//! (la liste qui fait foi est celle de `raccourcis::defauts`).
//!
//! Limite connue : **pas de réordonnancement de la playlist par glisser-déposer**
//! (voir `docs/INVENTAIRE-FONCTIONNALITES.md`).

use eframe::egui;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::player::{PlayerState, VideoPlayer};
use crate::raccourcis::{Action, Tableau as TableauRaccourcis};
use crate::renderer::VideoRenderer;
use crate::subtitles::SubtitleManager;

/// Délai de stabilité avant d'enregistrer la géométrie de la fenêtre.
///
/// Pendant qu'on tire un bord, la taille change à chaque image : écrire tout de
/// suite produirait des dizaines d'écritures du fichier de configuration par
/// seconde, pour une valeur intermédiaire qui n'est pas celle voulue. 600 ms est
/// plus court qu'un clic manqué et plus long qu'une rafale d'événements X11.
const TEMPORISATION_GEOMETRIE: std::time::Duration = std::time::Duration::from_millis(600);

/// Application principale egui
/// Pas d'un appui sur les raccourcis « glisser l'image », en pour cent de la taille affichée.
const PAS_GLISSEMENT: f32 = 2.0;

/// État de la boîte « Renommer le signet ».
struct RenommageSignet {
    media: PathBuf,
    index: usize,
    ms: u64,
    saisie: String,
    /// Le champ a déjà reçu le focus (une seule fois, à l'ouverture).
    a_le_focus: bool,
}

/// Touches qui restent des raccourcis pendant la saisie d'un texte : `Échap` (fermer) et les touches
/// de fonction. Les autres appartiennent au champ de saisie.
fn touche_active_pendant_saisie(touche: egui::Key) -> bool {
    use egui::Key::*;
    matches!(touche, Escape | F1 | F2 | F3 | F4 | F5 | F6 | F7 | F8 | F9 | F10 | F11 | F12)
}

pub struct App {
    pub player: VideoPlayer,
    renderer: VideoRenderer,
    pub config: Config,
    pub(crate) show_settings: bool,
    /// Profils de réglages enregistrés (voir `config::Profil`).
    pub(crate) profiles: Vec<crate::config::Profil>,
    /// Nom en cours de saisie pour enregistrer un profil.
    pub(crate) profil_nom: String,
    /// Nom du dernier profil appliqué (affiché avec une pastille).
    pub(crate) profil_actif: Option<String>,
    /// Brouillon de règle « profil automatique » (préférences) : champ comparé.
    pub(crate) regle_champ: crate::config::ChampRegle,
    /// Brouillon de règle : motif (dossier, extension ou résolution).
    pub(crate) regle_motif: String,
    /// Brouillon de règle : profil visé.
    pub(crate) regle_profil: String,
    /// Fenêtre « Ouvrir une adresse » (flux réseau).
    show_url_dialog: bool,
    /// Adresse en cours de saisie.
    url_input: String,
    show_playlist: bool,
    show_osd_info: bool,
    show_statistics: bool,
    show_about: bool,
    osd_timer: f32,
    pub(crate) error_message: Option<String>,
    /// Dernière information affichée à l'utilisateur (capture, réglage appliqué…)
    status_message: Option<String>,
    /// Périphériques de sortie audio (liste chargée à la demande, au premier
    /// affichage des préférences : l'énumération cpal ouvre chaque périphérique et
    /// produit du bruit ALSA/Jack au démarrage).
    pub(crate) audio_devices: Option<Vec<(String, u32, u16)>>,
    /// Sorties audio PipeWire (les vraies : enceinte, casque, écran) et l'instant de leur lecture.
    /// Relue au plus toutes les 3 s tant qu'un menu la réclame : `pw-dump` coûte une quarantaine de ms.
    pub(crate) sorties_audio: Vec<crate::sortie_audio::Sortie>,
    sorties_audio_le: Option<std::time::Instant>,
    /// Temps écoulé depuis le dernier enregistrement de la position de lecture
    position_tick: f32,
    /// Position choisie sur la barre de progression pendant un glissement
    seek_preview: Option<f32>,
    /// Rectangle de la barre de progression (pour traiter les clics très brefs que
    /// la détection de glissement d'egui ne voit pas)
    progress_rect: Option<egui::Rect>,
    /// Position du dernier appui du bouton principal (souris/tactile)
    press_pos: Option<egui::Pos2>,
    /// Instant du dernier seek déclenché par le glissement (limitation de fréquence)
    last_seek_at: Option<std::time::Instant>,
    /// Interface masquée (touche Tab : présentation plein écran sans panneaux)
    ui_hidden: bool,
    /// Fenêtre toujours au-dessus des autres
    always_on_top: bool,
    /// Fenêtre d'aide des raccourcis
    show_shortcuts: bool,
    /// Durée restante d'affichage du message d'état
    status_timer: f32,
    /// PTS de la dernière frame affichée (pour la cadence)
    last_frame_pts_ms: u64,
    frame_counter: u64,
    /// Texture GPU de la vidéo (réutilisée au lieu d'être recréée chaque frame)
    video_texture: Option<egui::TextureHandle>,
    /// Table des raccourcis effective (défauts + personnalisation).
    pub(crate) raccourcis: TableauRaccourcis,
    /// Piste de sous-titres choisie à la main, par média (si `subtitles.remember_track`).
    pistes_choisies: crate::pistes::PistesChoisies,
    /// Marque-pages par média (`signets.json`, dossier de configuration).
    signets: crate::signets::Signets,
    /// Boîte « Renommer le signet » ouverte (média, index, position, saisie).
    signet_renommage: Option<RenommageSignet>,
    /// Un champ de TEXTE éditable avait le focus à la fin de l'image précédente (egui publie alors
    /// `output.ime`). Voir la garde des raccourcis dans `update`.
    saisie_texte_active: bool,
    /// Mode d'aspect à rétablir quand on quitte « rogner » par son raccourci.
    aspect_avant_rogner: crate::config::AspectMode,
    /// Action en attente de capture (réglages : « appuyez sur une touche »).
    pub(crate) capture_raccourci: Option<Action>,
    /// Mode **réglage du recadrage** : l'image d'origine est affichée (recadrage
    /// suspendu) et le repère est dessiné. « Valider » en sort et applique le
    /// recadrage. État de séance, non persisté : au démarrage, un recadrage
    /// enregistré est appliqué directement.
    pub(crate) crop_reglage: bool,
    /// Accumulateur de molette (un cran = un pas, pas un pixel).
    molette_accumulee: f32,
    /// Textures des sous-titres image en cours : (début de l'événement, rang du
    /// rectangle, texture). Une composition PGS n'est téléversée qu'une fois.
    bitmap_textures: Vec<(u64, usize, egui::TextureHandle)>,
    dbg_police: bool,
    /// Gestionnaire de sous-titres (fichiers externes ASS/SSA/SRT)
    subtitles: SubtitleManager,
    /// Mode plein écran courant (masque l'interface)
    fullscreen: bool,
    /// Demande de plein écran à envoyer au gestionnaire de fenêtres au tour suivant
    /// (`Some(true)` = entrer, `Some(false)` = sortir)
    pending_fullscreen: Option<bool>,
    /// Commandes de fenêtre demandées par un raccourci, mises en attente.
    ///
    /// POURQUOI cette file d'attente : dans egui 0.33, `Context::input()` prend le
    /// verrou **exclusif** du contexte (`Context::input` → `Context::write`).
    /// Appeler une méthode `ctx.*` depuis sa fermeture est donc mortel : par
    /// exemple `send_viewport_cmd()` repasse par `Context::viewport_id()`, qui
    /// demande le verrou **partagé** — le même fil le détient déjà en exclusif et
    /// `parking_lot::RwLock` n'est pas réentrant : l'appel ne rend jamais la main,
    /// l'application se fige définitivement. C'est ce qui arrivait avec la touche
    /// `Q` (constaté le 2026-10-16 : fenêtre gelée, KWin la force à quitter par
    /// SIGABRT, ce qui ressemble à un plantage alors que c'est un interblocage).
    /// Les commandes sont désormais vidées au tour suivant, hors de toute
    /// fermeture de saisie.
    pending_viewport_cmds: Vec<egui::ViewportCommand>,
    /// Génération du lecteur vue au tour précédent (détecte load/seek)
    last_generation: u64,
    /// Vrai s'il faut afficher une frame dès que possible (après load/seek)
    pending_immediate: bool,
    /// Instant de création de l'application : sert à laisser au gestionnaire de
    /// fenêtres le temps de placer la fenêtre avant de vérifier sa géométrie.
    demarrage: std::time::Instant,
    /// Dernière géométrie observée — `(position, taille, agrandie)` de la zone
    /// cliente, la position restant `None` sous Wayland — et instant du dernier
    /// changement. Sert à n'écrire la configuration qu'une fois la fenêtre
    /// immobile (voir `memoriser_geometrie`).
    geometrie_observee: Option<(Option<[f32; 2]>, [f32; 2], bool)>,
    geometrie_changee_le: Option<std::time::Instant>,
    /// La géométrie enregistrée a-t-elle déjà été confrontée à l'écran réel ?
    /// Une seule fois : ensuite, un déplacement volontaire ne doit jamais être
    /// annulé par le lecteur.
    geometrie_verifiee: bool,
    /// Session **Wayland** : aucune position de fenêtre n'est ni connue ni
    /// imposable par le client (c'est le compositeur qui décide). Lu une fois au
    /// démarrage (voir `config::session_est_wayland`).
    wayland: bool,
    /// La fenêtre avait-elle le focus au tour précédent ? Sert à ne réagir qu'aux
    /// **changements** (perte ou retour), jamais à l'état lui-même.
    focus_precedent: Option<bool>,
    /// La fenêtre était-elle réduite au tour précédent ?
    reduite_precedente: bool,
    /// Une pause a-t-elle été **décidée par la perte de focus** ? Si oui, le retour
    /// au premier plan peut reprendre la lecture ; sinon il ne touche à rien (une
    /// pause volontaire de l'utilisateur ne doit pas être annulée par un clic).
    pause_par_focus: bool,
    /// Serveur MPRIS (touches média du bureau). `None` si le bus est absent :
    /// le lecteur vit alors normalement, simplement sans intégration au bureau.
    mpris: Option<std::sync::Arc<crate::mpris::Serveur>>,
    /// Le démarrage de MPRIS a-t-il déjà été tenté ? (Un échec ne doit pas être
    /// retenté à chaque image : la connexion D-Bus coûte quelques millisecondes.)
    mpris_demande: bool,
    /// Icône de la zone de notification (si `interface.tray_icon`).
    pub(crate) tray: Option<crate::tray::Tray>,
    /// Thème externe chargé : (nom, thème). Rechargé quand le réglage change.
    theme_externe: Option<(String, crate::theme::Theme)>,
    /// La fenêtre est-elle affichée ? (bascule depuis l'icône de notification)
    fenetre_visible: bool,
    /// Vignettes de la barre de progression : générateur (fil de travail) et
    /// texture de la vignette affichée pour la tranche courante.
    ///
    /// `None` tant que rien n'a été survolé : le fil n'est pas lancé au démarrage
    /// (il ouvrirait le fichier une seconde fois pour rien).
    vignettes: Option<crate::vignettes::Generateur>,
    vignette_affichee: Option<(u64, egui::TextureHandle)>,
    /// Position visée par le survol, en millisecondes (0 = aucune).
    vignette_cible: Option<u64>,
    /// Instance unique : écoute des fichiers envoyés par un autre processus
    /// (« ouvrir avec » alors que le lecteur tourne déjà). `None` si la mécanique
    /// est désactivée — voir `instance::Demarrage`.
    instance: Option<crate::instance::Instance>,
}

/// Installe une police système en secours pour les caractères absents d'egui.
///
/// egui n'embarque ni les flèches (`← ↑ → ↓`), ni `●`, `✕`, `⏯` : ces caractères
/// apparaissaient en carrés vides dans les menus et dans l'aide des raccourcis
/// (constaté à l'écran). On cherche une police système qui les contienne et on
/// l'ajoute **en dernier recours**, sans toucher aux polices d'egui.
pub fn installer_police_de_secours(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
        "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf",
        "/usr/share/fonts/truetype/freefont/FreeSans.ttf",
    ];
    let mut polices = egui::FontDefinitions::default();
    let mut chargee = None;
    for chemin in CANDIDATES {
        if let Ok(donnees) = std::fs::read(chemin) {
            polices
                .font_data
                .insert("secours".to_owned(), std::sync::Arc::new(egui::FontData::from_owned(donnees)));
            chargee = Some(chemin);
            break;
        }
    }
    let Some(chemin) = chargee else {
        tracing::warn!(
            "Aucune police de secours trouvée : les flèches et symboles rares \
             seront remplacés par leurs équivalents ASCII"
        );
        return;
    };
    // En dernier : les polices d'egui restent prioritaires pour le texte courant.
    for famille in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        polices
            .families
            .entry(famille)
            .or_default()
            .push("secours".to_owned());
    }
    ctx.set_fonts(polices);
    tracing::info!("Police de secours chargée : {}", chemin);

}

/// Nom de famille egui de la police des sous-titres (distinct de « secours »).
pub const FAMILLE_SOUS_TITRE: &str = "soustitre";

/// Résout un nom de famille de police en chemin de fichier, via fontconfig.
///
/// **Pourquoi `fc-match` et pas une dépendance** : egui ne sait pas lire
/// fontconfig ; ajouter `font-kit` ou `fontdb` pour une seule chaîne serait
/// disproportionné. `fc-match` est présent sur tout bureau Linux (c'est ce qui
/// sert à afficher les polices dans les réglages du système) et sa sortie est
/// stable : un chemin.
///
/// Un nom inconnu ne renvoie pas d'erreur : `fc-match` retombe sur la police par
/// défaut, et l'on ne peut donc pas distinguer « nom exact » de « nom approché ».
/// C'est accepté : l'utilisateur voit le résultat à l'écran, et le journal dit
/// quel fichier a servi.
pub fn resoudre_police(famille: &str) -> Option<String> {
    let nom = famille.trim();
    if nom.is_empty() {
        return None;
    }
    let sortie = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", nom])
        .output()
        .ok()?;
    if !sortie.status.success() {
        tracing::warn!("fc-match a échoué pour « {} »", nom);
        return None;
    }
    let chemin = String::from_utf8_lossy(&sortie.stdout).trim().to_string();
    if chemin.is_empty() || !std::path::Path::new(&chemin).exists() {
        tracing::warn!("Police « {} » introuvable (fc-match → « {} »)", nom, chemin);
        return None;
    }
    Some(chemin)
}

/// Installe la **police des sous-titres** dans egui, à côté de la police de secours.
///
/// `font_family` vide = rien à faire (le texte garde la police par défaut). Le
/// chargement est refait à chaque changement de réglage : c'est une opération de
/// quelques millisecondes (lecture d'un fichier TTF), faite une fois par
/// changement et non par image.
pub fn installer_police_sous_titre(ctx: &egui::Context, font_family: &str) {
    let mut polices = egui::FontDefinitions::default();
    // On réinstalle aussi la police de secours : `set_fonts` remplace l'ensemble.
    for chemin in [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    ] {
        if let Ok(donnees) = std::fs::read(chemin) {
            polices.font_data.insert(
                "secours".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(donnees)),
            );
            for famille in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                polices
                    .families
                    .entry(famille)
                    .or_default()
                    .push("secours".to_owned());
            }
            break;
        }
    }

    let nom = font_family.trim();
    if nom.is_empty() {
        ctx.set_fonts(polices);
        tracing::info!("Police des sous-titres : police par défaut");
        return;
    }
    match resoudre_police(nom) {
        Some(fichier) => match std::fs::read(&fichier) {
            Ok(donnees) => {
                polices.font_data.insert(
                    FAMILLE_SOUS_TITRE.to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(donnees)),
                );
                // Famille dédiée : seul le sous-titre l'utilise.
                polices.families.insert(
                    egui::FontFamily::Name(FAMILLE_SOUS_TITRE.into()),
                    vec![FAMILLE_SOUS_TITRE.to_owned()],
                );
                ctx.set_fonts(polices);
                tracing::info!("Police des sous-titres : « {} » ({})", nom, fichier);
            }
            Err(e) => {
                tracing::warn!("Lecture de la police « {} » impossible : {}", fichier, e);
                ctx.set_fonts(polices);
            }
        },
        None => {
            tracing::warn!(
                "Police « {} » non résolue : les sous-titres gardent la police par défaut",
                nom
            );
            ctx.set_fonts(polices);
        }
    }
}

/// `FontId` du texte des sous-titres : la famille choisie, ou la police par défaut.
/// Combinaison correspondant à un geste de souris brut (molette, clic du milieu), avec les
/// modificateurs portés par l'événement — sert à la capture d'un raccourci dans les préférences.
fn combinaison_souris(evt: &egui::Event) -> Option<crate::raccourcis::Combinaison> {
    use crate::raccourcis::{Combinaison, Souris, Touche};
    match evt {
        egui::Event::MouseWheel { delta, modifiers, .. } => {
            // Avec Maj, la molette verticale arrive sur l'axe horizontal.
            let d = delta.y + if modifiers.shift { delta.x } else { 0.0 };
            let geste = if d > 0.0 {
                Souris::MoletteHaut
            } else if d < 0.0 {
                Souris::MoletteBas
            } else {
                return None;
            };
            Some(Combinaison { ctrl: modifiers.ctrl, maj: modifiers.shift, alt: modifiers.alt, touche: Touche::Souris(geste) })
        }
        egui::Event::PointerButton { button: egui::PointerButton::Middle, pressed: true, modifiers, .. } => Some(Combinaison {
            ctrl: modifiers.ctrl,
            maj: modifiers.shift,
            alt: modifiers.alt,
            touche: Touche::Souris(Souris::ClicMilieu),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests_souris {
    use super::*;

    #[test]
    fn la_capture_reconnait_molette_et_clic_milieu_avec_leurs_modificateurs() {
        let ctrl = egui::Modifiers { ctrl: true, ..Default::default() };
        let molette = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, 1.0),
            modifiers: ctrl,
        };
        let c = combinaison_souris(&molette).unwrap();
        assert_eq!(c.texte(), "Ctrl+MoletteHaut");
        let bas = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, -1.0),
            modifiers: egui::Modifiers::NONE,
        };
        assert_eq!(combinaison_souris(&bas).unwrap().texte(), "MoletteBas");
        let maj = egui::Modifiers { shift: true, ..Default::default() };
        let horizontal = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(1.0, 0.0),
            modifiers: maj,
        };
        assert_eq!(combinaison_souris(&horizontal).unwrap().texte(), "Maj+MoletteHaut");
        let milieu = egui::Event::PointerButton {
            pos: egui::pos2(0.0, 0.0),
            button: egui::PointerButton::Middle,
            pressed: true,
            modifiers: egui::Modifiers { alt: true, ..Default::default() },
        };
        assert_eq!(combinaison_souris(&milieu).unwrap().texte(), "Alt+ClicMilieu");
        let relache = egui::Event::PointerButton {
            pos: egui::pos2(0.0, 0.0),
            button: egui::PointerButton::Middle,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert!(combinaison_souris(&relache).is_none());
    }
}

/// 16 directions unitaires pour dessiner un contour par copies décalées du texte.
const DIRECTIONS_CONTOUR: [(f32, f32); 16] = [
    (1.0, 0.0), (0.924, 0.383), (0.707, 0.707), (0.383, 0.924),
    (0.0, 1.0), (-0.383, 0.924), (-0.707, 0.707), (-0.924, 0.383),
    (-1.0, 0.0), (-0.924, -0.383), (-0.707, -0.707), (-0.383, -0.924),
    (0.0, -1.0), (0.383, -0.924), (0.707, -0.707), (0.924, -0.383),
];

/// Rayons (en pixels) auxquels le texte est recopié pour former un contour plein de
/// l'épaisseur demandée : un pas d'un pixel, jusqu'au rayon voulu.
fn rayons_contour(epaisseur: f32) -> Vec<f32> {
    if epaisseur <= 0.0 {
        return Vec::new();
    }
    let mut rayons: Vec<f32> = (1..=epaisseur.floor() as u32).map(|r| r as f32).collect();
    if rayons.last().copied().unwrap_or(0.0) < epaisseur {
        rayons.push(epaisseur);
    }
    rayons
}

/// Applique une opacité (0–1) à une couleur de sous-titre.
///
/// Un contour ou une ombre sont faits de nombreuses copies superposées du texte : une
/// opacité partielle se **cumulerait** (1 − (1 − a)^n) et le fondu ne se verrait pas sur
/// elles. Pour ces copies (`copies = true`) on prend l'opacité qui, cumulée sur ≈ 6 copies
/// qui se recouvrent, redonne l'opacité visée.
fn avec_alpha(couleur: egui::Color32, opacite: f32, copies: bool) -> egui::Color32 {
    let a = opacite.clamp(0.0, 1.0);
    let par_copie = if copies && a < 1.0 { 1.0 - (1.0 - a).powf(1.0 / 6.0) } else { a };
    couleur.gamma_multiply(par_copie)
}

/// Dessine un texte déjà mis en forme, à l'échelle `(sx, sy)` depuis son coin haut-gauche.
///
/// À l'échelle 1 c'est `Painter::galley`. Sinon les maillages des lignes sont recopiés avec
/// leurs sommets étirés : egui ne sait pas étirer un texte d'un facteur différent en X et en Y.
fn peindre_galley(
    ctx: &egui::Context,
    painter: &egui::Painter,
    pos: egui::Pos2,
    galley: &std::sync::Arc<egui::Galley>,
    couleur: egui::Color32,
    sx: f32,
    sy: f32,
) {
    if (sx - 1.0).abs() < 0.005 && (sy - 1.0).abs() < 0.005 {
        painter.galley(pos, galley.clone(), couleur);
        return;
    }
    let [w, h] = ctx.fonts(|f| f.font_image_size());
    let normalisation = egui::vec2(1.0 / w as f32, 1.0 / h as f32);
    let mut maillage = egui::epaint::Mesh::default();
    for ligne in &galley.rows {
        let decalage = maillage.vertices.len() as u32;
        maillage
            .indices
            .extend(ligne.row.visuals.mesh.indices.iter().map(|i| i + decalage));
        for sommet in &ligne.row.visuals.mesh.vertices {
            let local = ligne.pos.to_vec2() + sommet.pos.to_vec2();
            maillage.vertices.push(egui::epaint::Vertex {
                pos: pos + egui::vec2(local.x * sx, local.y * sy),
                uv: (sommet.uv.to_vec2() * normalisation).to_pos2(),
                color: if sommet.color == egui::Color32::PLACEHOLDER { couleur } else { sommet.color },
            });
        }
    }
    painter.add(egui::Shape::mesh(maillage));
}

fn police_sous_titre(taille: f32, famille: &str) -> egui::FontId {
    if famille.trim().is_empty() {
        egui::FontId::proportional(taille)
    } else {
        egui::FontId::new(
            taille,
            egui::FontFamily::Name(FAMILLE_SOUS_TITRE.into()),
        )
    }
}

/// Journalise la géométrie d'un titre de menu (target `kinterface`).
fn journaliser_menu(reponse: egui::InnerResponse<Option<()>>) -> egui::InnerResponse<Option<()>> {
    tracing::debug!(
        target: "kinterface",
        "menu-titre rect {:?}",
        reponse.response.rect
    );
    reponse
}

/// Journalise la géométrie d'un bouton de menu (target `kinterface`).
///
/// Pourquoi : l'audit des menus doit cliquer **sans deviner les pixels** (règle du
/// projet). Les entrées d'un menu n'existent à l'écran que lorsque le menu est
/// ouvert : leur rectangle est donc journalisé à ce moment-là, et l'audit relit
/// le journal pour cliquer juste.
fn journaliser_entree(libelle: &str, rect: egui::Rect, actif: bool) {
    tracing::debug!(
        target: "kinterface",
        "menu-item « {} » rect {:?} actif={}",
        libelle,
        rect,
        actif
    );
}

/// Bouton de menu dont la géométrie est journalisée.
fn bouton_menu(ui: &mut egui::Ui, libelle: impl Into<egui::WidgetText>) -> egui::Response {
    let texte: egui::WidgetText = libelle.into();
    // Le libellé peut être dynamique (« Pause » ↔ « Lecture ») : c'est celui qui
    // est réellement dessiné qui est journalisé, donc celui que l'audit cliquera.
    let nom = texte.text().to_string();
    let actif = ui.is_enabled();
    let reponse = ui.button(texte);
    journaliser_entree(&nom, reponse.rect, actif);
    reponse
}

/// Bouton de menu dont l'état (activé/désactivé) et la géométrie sont journalisés.
///
/// Utilisé par le menu contextuel **et** par les préférences : mêmes libellés, même
/// effet, donc vérifiables de la même manière par `tools/verif-personnalisation.sh`.
pub(crate) fn bouton_etat(
    ui: &mut egui::Ui,
    actif: bool,
    libelle: impl Into<egui::WidgetText>,
) -> egui::Response {
    let texte: egui::WidgetText = libelle.into();
    let nom = texte.text().to_string();
    let reponse = ui.add_enabled(actif, egui::Button::new(texte));
    journaliser_entree(&nom, reponse.rect, actif);
    reponse
}

/// Entrée sélectionnable de menu (piste audio, piste de sous-titres, chapitre) :
/// même journalisation que les boutons, pour que l'audit puisse la cliquer sans
/// deviner les pixels.
fn ligne_menu(ui: &mut egui::Ui, actif: bool, libelle: impl Into<egui::WidgetText>) -> egui::Response {
    let texte: egui::WidgetText = libelle.into();
    let nom = texte.text().to_string();
    let reponse = ui.selectable_label(actif, texte);
    journaliser_entree(&nom, reponse.rect, actif);
    reponse
}

/// Case à cocher de menu dont la géométrie est journalisée.
fn case_menu(ui: &mut egui::Ui, valeur: &mut bool, libelle: &str) -> egui::Response {
    let actif = ui.is_enabled();
    let reponse = ui.checkbox(valeur, libelle);
    journaliser_entree(libelle, reponse.rect, actif);
    reponse
}

/// Remplace les caractères qu'egui ne sait pas dessiner par un équivalent lisible.
///
/// Filet de sécurité indépendant de la police système : si aucune police de secours
/// n'a pu être chargée, les libellés restent lisibles au lieu d'afficher des carrés.
/// `←`/`→` deviennent `<`/`>`, `●` devient `*`, etc.
pub fn texte_sur(libelle: &str) -> String {
    let mut sortie = String::with_capacity(libelle.len());
    for c in libelle.chars() {
        match c {
            '←' => sortie.push_str("<"),
            '→' => sortie.push_str(">"),
            '↑' => sortie.push_str("^"),
            '↓' => sortie.push_str("v"),
            '●' => sortie.push_str("*"),
            _ => sortie.push(c),
        }
    }
    sortie
}

impl App {
    pub fn new(config: Config) -> Self {
        tracing::info!("RMPlayer UI initialisée");
        // Table construite AVANT le déplacement de `config` dans l'application.
        let raccourcis = TableauRaccourcis::construire(&config.raccourcis);

        let mut player = VideoPlayer::new(config.clone());
        // Lecture aléatoire : réglage mémorisé, appliqué dès le départ (un film
        // lancé depuis le gestionnaire de fichiers doit déjà suivre le mode choisi
        // à la session précédente). L'ordre est mélangé sur la playlist restaurée
        // juste après, donc on le refait une fois la liste en place.
        player.set_shuffle(config.playback.shuffle);
        player.set_repeat(config.playback.repeat);
        player.set_continuous(config.playback.continuous);
        player.set_boucle_ab(config.playback.ab_repetitions, config.playback.ab_delay_ms);
        // Restaure la playlist enregistrée : elle était sauvegardée mais jamais
        // relue, si bien que la liste était vide à chaque démarrage.
        for path in &config.playlist {
            if path.exists() && !player.playlist.contains(path) {
                player.add_to_playlist(path.clone());
            }
        }
        if !player.playlist.is_empty() {
            tracing::info!("Playlist restaurée: {} élément(s)", player.playlist.len());
        }

        Self {
            player,
            renderer: VideoRenderer::new(),
            config,
            show_settings: false,
            profiles: crate::config::load_profiles(),
            profil_nom: String::new(),
            profil_actif: None,
            regle_champ: crate::config::ChampRegle::Chemin,
            regle_motif: String::new(),
            regle_profil: String::new(),
            show_url_dialog: false,
            url_input: String::new(),
            show_playlist: false,
            show_osd_info: false,
            show_statistics: false,
            show_about: false,
            osd_timer: 0.0,
            error_message: None,
            status_message: None,
            audio_devices: None,
            sorties_audio: Vec::new(),
            sorties_audio_le: None,
            position_tick: 0.0,
            seek_preview: None,
            progress_rect: None,
            press_pos: None,
            last_seek_at: None,
            ui_hidden: false,
            always_on_top: false,
            show_shortcuts: false,
            status_timer: 0.0,
            last_frame_pts_ms: 0,
            frame_counter: 0,
            video_texture: None,
            raccourcis,
            pistes_choisies: crate::pistes::PistesChoisies::load(),
            signets: crate::signets::Signets::load(),
            signet_renommage: None,
            saisie_texte_active: false,
            aspect_avant_rogner: crate::config::AspectMode::Auto,
            capture_raccourci: None,
            crop_reglage: false,
            molette_accumulee: 0.0,
            bitmap_textures: Vec::new(),
            dbg_police: false,
            subtitles: SubtitleManager::new(),
            fullscreen: false,
            pending_fullscreen: None,
            pending_viewport_cmds: Vec::new(),
            last_generation: 0,
            pending_immediate: false,
            demarrage: std::time::Instant::now(),
            geometrie_observee: None,
            geometrie_changee_le: None,
            geometrie_verifiee: false,
            wayland: crate::config::session_est_wayland(),
            instance: None,
            focus_precedent: None,
            reduite_precedente: false,
            pause_par_focus: false,
            mpris: None,
            mpris_demande: false,
            tray: None,
            theme_externe: None,
            fenetre_visible: true,
            vignettes: None,
            vignette_affichee: None,
            vignette_cible: None,
        }
    }
    
    pub fn load_media(&mut self, path: &Path) -> bool {
        // Les réglages d'image (dont les miroirs et le recadrage) doivent être
        // réappliqués à chaque média : la chaîne de filtres repart de zéro.
        match self.player.load_file(path.to_path_buf()) {
            Ok(info) => {
                tracing::info!("Média chargé: {}", info.filename);
                // La règle de profil se décide **après** le chargement (la
                // résolution n'est connue qu'à ce moment) et **avant** la
                // construction de la chaîne, qui doit partir des bons réglages.
                self.appliquer_regles(path, Some(info.resolution));
                self.appliquer_filtres();
                self.after_media_loaded(path);
                true
            }
            Err(e) => {
                self.error_message = Some(format!("Erreur: {}", e));
                false
            }
        }
    }

    /// Applique la règle de profil qui correspond au média ouvert (préférences :
    /// « Profil appliqué automatiquement »). Retourne le nom du profil appliqué.
    ///
    /// Appelée à **chaque** ouverture — fichier choisi, entrée de playlist,
    /// élément suivant —, c'est tout l'intérêt : ne plus avoir à y penser.
    pub(crate) fn appliquer_regles(
        &mut self,
        chemin: &Path,
        resolution: Option<(u32, u32)>,
    ) -> Option<String> {
        if self.config.regles.is_empty() || self.profiles.is_empty() {
            return None;
        }
        let regle =
            crate::config::appliquer_regle(&mut self.config, &self.profiles, chemin, resolution)?;
        let nom = regle.profil.clone();
        // La partie audio du profil doit atteindre le moteur : `refresh_audio_chain`
        // ne relit PAS la configuration, il recalcule à partir des réglages que le
        // lecteur détient déjà. Sans `set_audio_filters`, l'égaliseur du profil
        // restait sans effet jusqu'au prochain réglage audio.
        self.player.set_audio_filters(&self.config.audio);
        self.profil_actif = Some(nom.clone());
        let _ = self.config.save();
        tracing::info!(
            target: "kinterface",
            "règle de profil appliquée : {} → « {} »",
            regle.resume(),
            nom
        );
        self.annonce(&format!("Profil « {} » — règle : {}", nom, regle.resume()));
        Some(nom)
    }

    /// Rappelle la piste de sous-titres que l'utilisateur avait choisie pour ce média
    /// (réglage « mémoriser la piste »). Sans mémoire pour ce média, rien ne change.
    fn rappeler_piste_choisie(&mut self, media: &Path) {
        if !self.config.subtitles.remember_track {
            return;
        }
        match self.pistes_choisies.get(media) {
            Some(crate::pistes::Choix::Externe(fichier)) => {
                if self.subtitles.select_path(&fichier) {
                    tracing::info!("Piste de sous-titres rappelée : {}", fichier.display());
                }
            }
            Some(crate::pistes::Choix::Integree(index)) => {
                if self.player.select_subtitle_stream(index).is_ok() {
                    tracing::info!("Piste intégrée rappelée : {}", index);
                }
            }
            None => {}
        }
    }

    /// Passe à la piste audio suivante (boucle sur les pistes du fichier) et la mémorise si le
    /// réglage est actif. C'est le chemin du raccourci ; le menu Audio fait la même chose.
    fn piste_audio_suivante(&mut self) {
        let pistes = self
            .player
            .media_info()
            .map(|i| i.audio_tracks.clone())
            .unwrap_or_default();
        if pistes.len() < 2 {
            self.annonce("Une seule piste audio");
            return;
        }
        let actuelle = self.player.current_audio_stream();
        let rang = pistes.iter().position(|p| Some(p.index) == actuelle).unwrap_or(0);
        let suivante = &pistes[(rang + 1) % pistes.len()];
        match self.player.select_audio_stream(suivante.index) {
            Ok(()) => {
                self.player.memoriser_piste_audio(suivante.index);
                tracing::info!(target: "kinterface", "piste audio suivante : flux {} — {}", suivante.index, suivante.label());
                self.annonce(&format!("Audio : {}", suivante.label()));
            }
            Err(e) => self.annonce(&format!("Piste audio indisponible : {e}")),
        }
    }

    /// Note la piste que l'utilisateur vient de choisir, pour le média ouvert.
    fn memoriser_piste_choisie(&mut self, choix: crate::pistes::Choix) {
        if !self.config.subtitles.remember_track {
            return;
        }
        if let Some(media) = self.player.current_path().cloned() {
            self.pistes_choisies.set(&media, &choix);
        }
    }

    /// Travaux d'interface après l'ouverture d'un média (rendu + sous-titres).
    fn after_media_loaded(&mut self, path: &Path) {
        // Tous les chemins d'ouverture (fichier choisi, playlist, suivant, glisser-déposer,
        // ligne de commande) passent ici : un seul endroit pour les fichiers récents.
        if self.config.playback.noter_recent(path) {
            let _ = self.config.save();
        }
        self.renderer.update_frame(vec![], 0, 0);
        if self.config.subtitles.auto_load {
            self.subtitles = SubtitleManager::new();
            self.subtitles.set_encodage(&self.config.subtitles.encoding);
            self.subtitles.auto_load(path, &self.config.subtitles.langues_preferees());
            if self.subtitles.count() > 0 {
                self.subtitles.set_visible(true);
            }
        }
        self.rappeler_piste_choisie(path);
    }

    /// Ouvre l'élément `index` de la playlist en mettant à jour l'index courant
    /// (bug corrigé : sans cela « suivant » repartait d'un rang erroné).
    fn play_playlist_index(&mut self, index: usize) -> bool {
        let path = match self.player.playlist.get(index).cloned() {
            Some(p) => p,
            None => return false,
        };
        match self.player.play_index(index) {
            Ok(info) => {
                tracing::info!("Playlist: lecture de {} ({})", index, info.filename);
                // Même chemin que `load_media` : une règle de profil s'applique
                // aussi quand la lecture vient de la playlist (sinon le réglage
                // dépendrait de la façon dont le fichier a été ouvert).
                self.appliquer_regles(&path, Some(info.resolution));
                self.appliquer_filtres();
                self.after_media_loaded(&path);
                true
            }
            Err(e) => {
                self.error_message = Some(format!("Erreur: {}", e));
                false
            }
        }
    }

    /// Remplace toute la configuration par celle d'un fichier validé (préférences →
    /// « Importer une configuration »). Le moteur est ensuite resynchronisé comme pour
    /// l'application d'un profil : filtres vidéo, chaîne audio, table des raccourcis.
    pub(crate) fn importer_configuration(&mut self, fichier: &Path) {
        let Some(destination) = crate::config::config_path() else {
            self.error_message = Some("Import : répertoire de configuration introuvable".into());
            return;
        };
        match crate::config::importer_config(fichier, &destination) {
            Ok(mut config) => {
                config.audio.sanitize();
                // Les fichiers récents et la playlist décrivent cette machine, pas les
                // réglages : ils suivent le fichier importé (c'est une sauvegarde complète).
                self.config = config;
                self.raccourcis = TableauRaccourcis::construire(&self.config.raccourcis);
                self.player.set_audio_filters(&self.config.audio);
                self.player.set_boucle_ab(
                    self.config.playback.ab_repetitions,
                    self.config.playback.ab_delay_ms,
                );
                self.appliquer_filtres();
                self.profil_actif = None;
                tracing::info!(target: "kinterface", "configuration importée : {:?}", fichier);
                self.annonce("Configuration importée");
            }
            Err(e) => {
                tracing::warn!("Import de configuration refusé : {}", e);
                self.error_message = Some(format!("Import refusé : {}", e));
            }
        }
    }

    /// Enregistre l'image courante en PNG dans le dossier de captures.
    fn take_screenshot(&mut self) {
        let position = self.player.current_position_ms();
        let media = self.player.current_path().cloned();

        // Taille d'origine : l'image du film décodée à part (décodeur autonome des vignettes), SANS
        // les réglages d'image ni le HDR convertis ; sinon l'image telle qu'elle est affichée.
        let natif = if self.config.interface.capture_native {
            match media.as_ref().map(|m| crate::vignettes::extraire(m, position, 16_384)) {
                Some(Ok(v)) => Some((v.largeur, v.hauteur, v.rgba)),
                Some(Err(e)) => {
                    tracing::warn!("Capture à la taille native impossible ({e}) — image affichée");
                    None
                }
                None => None,
            }
        } else {
            None
        };
        let (w, h, pixels) = match natif {
            Some(image) => image,
            None => {
                let (width, height) = self.renderer.get_dimensions();
                let frame = self.renderer.get_frame();
                let (Some(frame), w, h) = (frame, width, height) else {
                    self.status_message = Some("Capture impossible : aucune image affichée".to_string());
                    return;
                };
                let expected = (w as usize) * (h as usize) * 4;
                if w == 0 || h == 0 || frame.len() < expected {
                    self.status_message = Some("Capture impossible : image incomplète".to_string());
                    return;
                }
                (w, h, frame[..expected].to_vec())
            }
        };

        let titre = media
            .as_ref()
            .and_then(|m| m.file_stem())
            .map(|s| s.to_string_lossy().to_string());
        let nom = crate::capture::nom_depuis_modele(
            &self.config.interface.capture_name,
            titre.as_deref(),
            position,
            chrono::Local::now(),
        );
        let format = self.config.interface.capture_format;
        let dir = self.config.screenshots_dir();
        let path = crate::capture::chemin_libre(&dir, &nom, crate::capture::extension(format));

        match crate::capture::ecrire(&path, format, self.config.interface.capture_jpeg_quality, w, h, &pixels) {
            Ok(()) => {
                tracing::info!("Capture enregistrée: {:?} ({}x{}, {:?})", path, w, h, format);
                self.status_message = Some(format!("📸 Capture : {}", path.display()));
            }
            Err(e) => {
                tracing::error!("Échec de la capture: {}", e);
                self.error_message = Some(format!("Capture d'écran: {}", e));
            }
        }
    }

    /// Active/désactive le décodage matériel et recharge le média à la position
    /// courante (le mode de décodage se choisit à l'ouverture du flux).
    pub(crate) fn apply_hardware_decoding(&mut self, enabled: bool) {
        self.player.set_hardware_decoding(enabled);
        self.config.video.hardware_decoding = enabled;
        let _ = self.config.save();
        if self.player.current_path().is_some() {
            if let Err(e) = self.player.reload_current_media() {
                tracing::warn!("Rechargement après bascule de décodage: {}", e);
            }
        }
    }

    /// Charge la liste des périphériques audio si nécessaire (première ouverture
    /// des préférences, ou demande explicite de rafraîchissement).
    pub(crate) fn refresh_audio_devices(&mut self) {
        if self.show_settings && self.audio_devices.is_none() {
            self.audio_devices = Some(crate::audio::available_output_devices());
        }
    }

    /// La fenêtre flottante `id` est-elle ouverte comme fenêtre système (voir `fenetre::afficher`) ?
    pub(crate) fn est_detachee(&self, id: &str) -> bool {
        self.config.interface.detached_windows.iter().any(|d| d == id)
    }

    /// Détache ou rattache la fenêtre `id` et mémorise le choix.
    pub(crate) fn basculer_detachee(&mut self, id: &str) {
        let liste = &mut self.config.interface.detached_windows;
        if let Some(rang) = liste.iter().position(|d| d == id) {
            liste.remove(rang);
        } else {
            liste.push(id.to_string());
        }
        let _ = self.config.save();
        tracing::info!(target: "kinterface", "fenêtre « {} » : {}", id, if self.est_detachee(id) { "détachée" } else { "rattachée" });
    }

    /// Relit les sorties PipeWire si la liste est ancienne (ou si `force`).
    pub(crate) fn actualiser_sorties_audio(&mut self, force: bool) {
        let perimee = self
            .sorties_audio_le
            .map_or(true, |t| t.elapsed() > std::time::Duration::from_secs(3));
        if force || perimee {
            self.sorties_audio = crate::sortie_audio::lister();
            self.sorties_audio_le = Some(std::time::Instant::now());
        }
    }

    /// Nom lisible d'un réglage de sortie : la description PipeWire, « Périphérique par défaut »,
    /// ou le nom brut tel qu'enregistré (ancien nom ALSA, sortie débranchée).
    pub(crate) fn libelle_sortie_audio(&self, id: &str) -> String {
        if id.is_empty() {
            return "Périphérique par défaut".to_string();
        }
        match crate::sortie_audio::nom_pipewire(id) {
            Some(nom) => match self.sorties_audio.iter().find(|s| s.nom == nom) {
                Some(s) => s.description.clone(),
                None => format!("{nom} (absente)"),
            },
            None => id.to_string(),
        }
    }

    /// Choisit la sortie audio : **un seul chemin** pour le menu Audio, les préférences et le
    /// raccourci. Le moteur redirige le flux sans coupure ; seule une sortie ALSA explicite
    /// recrée la sortie et fait recharger le film (la position est conservée).
    pub(crate) fn choisir_sortie_audio(&mut self, id: &str) {
        if self.config.audio.device == id {
            return;
        }
        self.config.audio.device = id.to_string();
        let recharger = self.player.set_audio_device(id);
        let _ = self.config.save();
        if recharger && self.player.current_path().is_some() {
            if let Err(e) = self.player.reload_current_media() {
                tracing::warn!("Rechargement après changement de sortie audio: {}", e);
            }
        }
        let libelle = self.libelle_sortie_audio(id);
        tracing::info!(target: "kinterface", "sortie audio choisie : {}", libelle);
        self.annonce(&format!("Sortie audio : {libelle}"));
    }

    /// Fixe la disposition des haut-parleurs : un seul chemin pour le menu, les préférences et le
    /// raccourci. Le nombre de canaux de la sortie change : le film est rechargé (position conservée).
    pub(crate) fn definir_enceintes(&mut self, layout: crate::config::SpeakerLayout) {
        if self.config.audio.speakers == layout {
            return;
        }
        self.config.audio.speakers = layout;
        let recharger = self.player.set_speakers(layout);
        let _ = self.config.save();
        if recharger && self.player.current_path().is_some() {
            if let Err(e) = self.player.reload_current_media() {
                tracing::warn!("Rechargement après changement d'enceintes: {}", e);
            }
        }
        tracing::info!(target: "kinterface", "enceintes : {}", layout.libelle());
        self.annonce(&format!("Enceintes : {}", layout.libelle()));
    }

    /// Passe à la sortie audio suivante (raccourci) ; la liste est relue à chaque appui.
    fn sortie_audio_suivante(&mut self) {
        self.actualiser_sorties_audio(true);
        if self.sorties_audio.is_empty() {
            self.annonce("Aucune autre sortie audio (PipeWire introuvable)");
            return;
        }
        let suivante = crate::sortie_audio::suivante(&self.sorties_audio, &self.config.audio.device);
        self.choisir_sortie_audio(&suivante);
    }

    /// Ferme la fenêtre la plus « en avant » parmi celles qui sont ouvertes.
    /// Renvoie `true` si une fenêtre a été fermée.
    fn close_top_window(&mut self) -> bool {
        if self.signet_renommage.take().is_some() {
            self.annonce("Renommage annulé");
            return true;
        }
        let cible = if self.show_about {
            Some("À propos")
        } else if self.show_shortcuts {
            Some("Aide des raccourcis")
        } else if self.show_settings {
            Some("Préférences")
        } else if self.show_statistics {
            Some("Statistiques")
        } else {
            None
        };
        match cible {
            Some("À propos") => { self.show_about = false; }
            Some("Aide des raccourcis") => { self.show_shortcuts = false; }
            Some("Préférences") => { self.show_settings = false; }
            Some("Statistiques") => { self.show_statistics = false; }
            _ => return false,
        }
        if let Some(nom) = cible {
            tracing::info!("Fenêtre fermée: {}", nom);
            self.annonce(&format!("Fenêtre « {} » fermée", nom));
        }
        true
    }

    /// Menu contextuel du **clic droit** : les réglages qui servent pendant la
    /// lecture, sans passer par les menus de la barre ni par les préférences.
    ///
    /// Il est ouvert par egui lui-même (`Response::context_menu`), donc il se ferme
    /// par un clic à côté ou par Échap, et il n'interfère pas avec le menu du
    /// gestionnaire de fenêtres (celui-ci est sur la barre de titre, pas dans la
    /// zone client).
    fn menu_contextuel(&mut self, response: &egui::Response) {
        response.context_menu(|ui| {
            ui.set_min_width(280.0);
            ui.label(egui::RichText::new("Affichage").small().strong());

            // --- Rapport d'aspect ---
            ui.horizontal(|ui| {
                ui.label("Aspect");
                let modes = [
                    ("Auto", crate::config::AspectMode::Auto),
                    ("16:9", crate::config::AspectMode::Ratio16x9),
                    ("4:3", crate::config::AspectMode::Ratio4x3),
                    ("1.85", crate::config::AspectMode::Ratio185),
                    ("2.35", crate::config::AspectMode::Ratio235),
                    ("Remplir", crate::config::AspectMode::Remplir),
                    ("Rogner", crate::config::AspectMode::Rogner),
                ];
                for (nom, mode) in modes {
                    let actif = self.config.video.aspect == mode;
                    if ligne_menu(ui, actif, nom).clicked() && !actif {
                        self.config.video.aspect = mode;
                        let _ = self.config.save();
                        tracing::info!(target: "kinterface", "menu contextuel : aspect {}", nom);
                    }
                }
            });

            // --- Miroirs (horizontal ET vertical) ---
            let mut miroir_h = self.config.video.flip_h;
            if case_menu(ui, &mut miroir_h, "Miroir horizontal").changed() {
                self.config.video.flip_h = miroir_h;
                let _ = self.config.save();
                self.appliquer_filtres();
                tracing::info!(target: "kinterface", "menu contextuel : miroir horizontal {}", miroir_h);
            }
            let mut miroir_v = self.config.video.flip;
            if case_menu(ui, &mut miroir_v, "Miroir vertical").changed() {
                self.config.video.flip = miroir_v;
                let _ = self.config.save();
                self.appliquer_filtres();
                tracing::info!(target: "kinterface", "menu contextuel : miroir vertical {}", miroir_v);
            }
            if bouton_menu(ui, "Rotation 90°").clicked() {
                self.config.video.rotate = (self.config.video.rotate + 90) % 360;
                let _ = self.config.save();
                self.appliquer_filtres();
                tracing::info!(target: "kinterface", "menu contextuel : rotation {}", self.config.video.rotate);
            }

            ui.separator();
            ui.label(egui::RichText::new("Recadrage").small().strong());
            // Les quatre bords restent accessibles ici : c'est pendant la lecture
            // qu'on voit le mieux ce qu'il faut retirer.
            let mut change = false;
            {
                let v = &mut self.config.video;
                let mut haut = v.crop_top;
                let mut bas = v.crop_bottom;
                let mut gauche = v.crop_left;
                let mut droite = v.crop_right;
                change |= ui
                    .add(egui::Slider::new(&mut haut, 0..=500).text("haut"))
                    .changed();
                change |= ui
                    .add(egui::Slider::new(&mut bas, 0..=500).text("bas"))
                    .changed();
                change |= ui
                    .add(egui::Slider::new(&mut gauche, 0..=500).text("gauche"))
                    .changed();
                change |= ui
                    .add(egui::Slider::new(&mut droite, 0..=500).text("droite"))
                    .changed();
                v.crop_top = haut;
                v.crop_bottom = bas;
                v.crop_left = gauche;
                v.crop_right = droite;
            }
            if change {
                // Bouger un curseur ENTRE en mode réglage : on veut voir l'image
                // d'origine et le repère, pas le résultat déjà rogné.
                self.crop_reglage = true;
                let _ = self.config.save();
                self.appliquer_filtres();
                // Repère indispensable pendant le réglage : on le force.
                self.config.video.crop_overlay = true;
            }
            let mut zone = self.config.video.crop_overlay;
            if case_menu(ui, &mut zone, "Afficher le repère de recadrage")
                .on_hover_text("Affiche le repère tant qu'on règle")
                .changed()
            {
                self.config.video.crop_overlay = zone;
                let _ = self.config.save();
            }
            // --- Validation : c'est ce qui manquait ---
            // Sans ces deux boutons, le repère restait affiché et le recadrage
            // n'était jamais appliqué : impossible de terminer le réglage.
            ui.horizontal(|ui| {
                let peut_valider = self.config.video.crop_active() && self.crop_reglage;
                if bouton_etat(ui, peut_valider, "✔ Valider le recadrage")
                    .on_hover_text("Applique le recadrage et retire le repère (Entrée)")
                    .clicked()
                {
                    self.valider_recadrage();
                }
                if bouton_etat(
                    ui,
                    self.config.video.crop_active() && !self.crop_reglage,
                    "↺ Régler à nouveau",
                )
                .on_hover_text("Réaffiche l'image d'origine pour ajuster")
                .clicked()
                {
                    self.entrer_reglage_recadrage();
                }
            });
            if self.crop_reglage {
                ui.label(
                    egui::RichText::new("Réglage en cours — l'image d'origine est affichée")
                        .small()
                        .color(egui::Color32::from_rgb(120, 200, 255)),
                );
            }
            if ui
                .add_enabled(
                    self.config.video.crop_active(),
                    egui::Button::new("Retirer le recadrage"),
                )
                .clicked()
            {
                self.config.video.crop_top = 0;
                self.config.video.crop_bottom = 0;
                self.config.video.crop_left = 0;
                self.config.video.crop_right = 0;
                let _ = self.config.save();
                self.appliquer_filtres();
                tracing::info!(target: "kinterface", "menu contextuel : recadrage retiré");
            }

            ui.separator();
            ui.label(egui::RichText::new("Lecture et son").small().strong());
            ui.horizontal(|ui| {
                if bouton_menu(ui, "Vitesse −").clicked() {
                    let v = self.player.get_speed() - 0.1;
                    self.player.set_speed(v);
                }
                ui.label(format!("{:.2}×", self.player.get_speed()));
                if bouton_menu(ui, "Vitesse +").clicked() {
                    let v = self.player.get_speed() + 0.1;
                    self.player.set_speed(v);
                }
                if bouton_menu(ui, "1,0×").clicked() {
                    self.player.set_speed(1.0);
                }
            });
            let mut visible = self.subtitles.is_visible();
            if case_menu(ui, &mut visible, "Sous-titres affichés").changed() {
                self.subtitles.set_visible(visible);
                self.player.subtitles_visible = visible;
                tracing::info!(target: "kinterface", "menu contextuel : sous-titres {}", visible);
            }

            ui.separator();
            if bouton_menu(ui, "Préférences… (F5)").clicked() {
                self.show_settings = true;
            }
            if bouton_menu(ui, "Aide des raccourcis (F1)").clicked() {
                self.show_shortcuts = true;
            }
            if bouton_menu(ui, "Plein écran (F)").clicked() {
                self.set_fullscreen(!self.fullscreen);
            }
        });
    }

    /// Entre en **mode réglage du recadrage** : image d'origine + repère.
    pub(crate) fn entrer_reglage_recadrage(&mut self) {
        self.crop_reglage = true;
        self.config.video.crop_overlay = true;
        self.appliquer_filtres();
        tracing::info!(target: "kinterface", "recadrage : mode réglage");
        self.annonce("Recadrage : réglage (Entrée pour valider)");
    }

    /// **Valide** le recadrage : le repère disparaît et le recadrage est appliqué.
    ///
    /// C'est l'étape qui manquait : sans elle, on réglait sans jamais pouvoir
    /// terminer, et le recadrage n'était jamais visible à l'écran.
    pub(crate) fn valider_recadrage(&mut self) {
        if !self.config.video.crop_active() {
            // Rien à valider : on sort simplement du mode réglage.
            self.crop_reglage = false;
            self.appliquer_filtres();
            return;
        }
        self.crop_reglage = false;
        let _ = self.config.save();
        self.appliquer_filtres();
        let v = &self.config.video;
        tracing::info!(
            target: "kinterface",
            "recadrage validé : haut={} bas={} gauche={} droite={}",
            v.crop_top, v.crop_bottom, v.crop_left, v.crop_right
        );
        let (l, h) = self
            .player
            .media_info()
            .map(|i| {
                (
                    i.resolution
                        .0
                        .saturating_sub(v.crop_left + v.crop_right),
                    i.resolution
                        .1
                        .saturating_sub(v.crop_top + v.crop_bottom),
                )
            })
            .unwrap_or((0, 0));
        self.annonce(&format!("Recadrage appliqué : {} × {} px", l, h));
    }

    /// Repère visuel du **recadrage manuel** : assombrit la partie retirée et
    /// encadre la partie conservée, en coordonnées du média ramenées à l'écran.
    ///
    /// Sans ce repère, régler le recadrage revenait à taper des nombres en
    /// aveugle : le résultat n'était visible qu'après coup, et sur une scène
    /// sombre, impossible à juger.
    fn draw_crop_overlay(&self, ui: &egui::Ui, image: &egui::Response, disp: egui::Vec2) {
        let v = &self.config.video;
        if !v.crop_active() || !v.crop_overlay || !self.crop_reglage {
            return;
        }
        // Pendant une capture ou en plein écran sans interface, le repère ne doit
        // pas apparaître sur l'image : il est utile pour RÉGLER, pas pour regarder.
        let rect = Self::image_rect(image, disp).intersect(ui.clip_rect());
        if rect.width() < 40.0 || rect.height() < 40.0 {
            return;
        }
        let native = self
            .player
            .media_info()
            .map(|i| i.resolution)
            .filter(|(w, h)| *w > 0 && *h > 0)
            .unwrap_or((rect.width() as u32, rect.height() as u32));

        // Conversion : pixels du média → fraction de l'image affichée.
        let fx = |px: u32| (px as f32 / native.0 as f32).clamp(0.0, 1.0) * rect.width();
        let fy = |px: u32| (px as f32 / native.1 as f32).clamp(0.0, 1.0) * rect.height();
        let gauche = rect.left() + fx(v.crop_left);
        let droite = rect.right() - fx(v.crop_right);
        let haut = rect.top() + fy(v.crop_top);
        let bas = rect.bottom() - fy(v.crop_bottom);
        if droite - gauche < 4.0 || bas - haut < 4.0 {
            return;
        }
        let conserve = egui::Rect::from_min_max(
            egui::pos2(gauche, haut),
            egui::pos2(droite, bas),
        );

        let painter = ui.painter();
        // Zones retirées : voile sombre (45 %), assez visible sur une image claire
        // comme sur une image sombre.
        let voile = egui::Color32::from_black_alpha(115);
        for bande in [
            egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), conserve.top())),
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), conserve.bottom()),
                rect.max,
            ),
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), conserve.top()),
                egui::pos2(conserve.left(), conserve.bottom()),
            ),
            egui::Rect::from_min_max(
                egui::pos2(conserve.right(), conserve.top()),
                egui::pos2(rect.right(), conserve.bottom()),
            ),
        ] {
            if bande.width() > 0.0 && bande.height() > 0.0 {
                painter.rect_filled(bande, 0.0, voile);
            }
        }
        // Cadre de la zone conservée : deux traits (noir puis blanc) pour rester
        // visible quelle que soit l'image.
        painter.rect_stroke(
            conserve,
            0.0,
            egui::Stroke::new(3.0_f32, egui::Color32::from_black_alpha(180)),
            egui::StrokeKind::Outside,
        );
        painter.rect_stroke(
            conserve,
            0.0,
            egui::Stroke::new(1.5_f32, egui::Color32::WHITE),
            egui::StrokeKind::Inside,
        );
        // Dimensions conservées, en pixels du média : c'est l'unité des réglages.
        let largeur = native.0.saturating_sub(v.crop_left + v.crop_right);
        let hauteur = native.1.saturating_sub(v.crop_top + v.crop_bottom);
        let etiquette = format!("{} × {} px — réglage (Entrée pour valider)", largeur, hauteur);
        let position = egui::pos2(conserve.center().x - 60.0, conserve.top() - 22.0);
        let galerie = painter.layout_no_wrap(
            etiquette,
            egui::FontId::proportional(14.0),
            egui::Color32::WHITE,
        );
        let fond = egui::Rect::from_min_size(
            egui::pos2(position.x - 6.0, position.y - 3.0),
            egui::vec2(galerie.size().x + 12.0, galerie.size().y + 6.0),
        );
        painter.rect_filled(fond, 4.0, egui::Color32::from_black_alpha(190));
        painter.galley(position, galerie, egui::Color32::WHITE);
    }

    /// Raccourcis souris déclenchés pendant cette image (molette, clic du milieu,
    /// double-clic), d'après la table personnalisable.
    ///
    /// La molette est accumulée par egui sur plusieurs événements : on ne réagit
    /// qu'au franchissement d'un cran pour ne pas régler le volume au centième.
    fn souris_utilisee(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        if !response.hovered() {
            return actions;
        }
        // Modificateurs tenus pendant le geste : ils choisissent l'action (Ctrl+Molette ≠ Molette).
        let (ctrl, maj, alt) = ui.input(|i| (i.modifiers.ctrl, i.modifiers.shift, i.modifiers.alt));
        if response.double_clicked_by(egui::PointerButton::Primary) {
            if let Some(a) = self.raccourcis.pour_souris(crate::raccourcis::Souris::DoubleClic, ctrl, maj, alt) {
                actions.push(a);
            }
        }
        if response.clicked_by(egui::PointerButton::Middle) {
            if let Some(a) = self.raccourcis.pour_souris(crate::raccourcis::Souris::ClicMilieu, ctrl, maj, alt) {
                actions.push(a);
            }
        }
        // Molette : un cran = un pas. `raw_scroll_delta` donne des pixels.
        // Avec Maj, egui (et la plupart des systèmes) convertit la molette verticale en
        // défilement HORIZONTAL : on lit donc les deux axes quand Maj est tenue.
        let delta = ui.input(|i| {
            i.raw_scroll_delta.y + if i.modifiers.shift { i.raw_scroll_delta.x } else { 0.0 }
        });
        if delta.abs() > 1.0 {
            self.molette_accumulee += delta;
            let cran = 35.0;
            while self.molette_accumulee >= cran {
                self.molette_accumulee -= cran;
                if let Some(a) = self.raccourcis.pour_souris(crate::raccourcis::Souris::MoletteHaut, ctrl, maj, alt) {
                    actions.push(a);
                }
            }
            while self.molette_accumulee <= -cran {
                self.molette_accumulee += cran;
                if let Some(a) = self.raccourcis.pour_souris(crate::raccourcis::Souris::MoletteBas, ctrl, maj, alt) {
                    actions.push(a);
                }
            }
        }
        actions
    }

    /// Exécute une action déclenchée par la souris.
    ///
    /// Seules quelques actions ont un sens à la souris en dehors du clavier ; les
    /// autres sont ignorées plutôt que d'inventer un comportement.
    fn executer_action_souris(&mut self, action: Action) {
        match action {
            Action::LecturePause => {
                let _ = self.player.toggle_pause();
            }
            Action::VolumePlus => self.changer_volume(0.05),
            Action::VolumeMoins => self.changer_volume(-0.05),
            Action::Muet => {
                self.player.toggle_mute();
            }
            Action::PleinEcran => self.set_fullscreen(!self.fullscreen),
            Action::SousTitresVisibles => {
                let v = self.subtitles.toggle_visible();
                self.player.subtitles_visible = v;
                self.annonce(if v { "Sous-titres : affichés" } else { "Sous-titres : masqués" });
            }
            Action::Statistiques => {
                self.show_statistics = !self.show_statistics;
                self.osd_timer = 5.0;
            }
            Action::Osd => {
                self.show_osd_info = !self.show_osd_info;
                self.osd_timer = if self.show_osd_info { self.osd_duration_secs() } else { 0.0 };
            }
            Action::PanneauPlaylist => self.basculer_panneau_playlist(),
            Action::Capture => self.take_screenshot(),
            Action::Aspect => self.aspect_suivant(),
            Action::AspectRogner => self.basculer_rogner(),
            Action::ImageGlisserGauche => self.glisser_image(-PAS_GLISSEMENT, 0.0),
            Action::ImageGlisserDroite => self.glisser_image(PAS_GLISSEMENT, 0.0),
            Action::ImageGlisserHaut => self.glisser_image(0.0, -PAS_GLISSEMENT),
            Action::ImageGlisserBas => self.glisser_image(0.0, PAS_GLISSEMENT),
            Action::ImageCentrer => self.centrer_image(),
            Action::Reculer5s => self.decaler(-5000),
            Action::Avancer5s => self.decaler(5000),
            Action::Reculer1s => self.decaler(-1000),
            Action::Avancer1s => self.decaler(1000),
            Action::Reculer30s => self.decaler(-30000),
            Action::Avancer30s => self.decaler(30000),
            Action::VitesseMoins | Action::VitessePlus | Action::VitesseNormale => {
                let vitesse = match action {
                    Action::VitesseMoins => self.player.get_speed() - 0.1,
                    Action::VitessePlus => self.player.get_speed() + 0.1,
                    _ => 1.0,
                };
                self.player.set_speed(vitesse);
                self.annonce(&format!("Vitesse : {:.1}×", self.player.get_speed()));
            }
            Action::SignetAjouter => self.signet_ajouter(),
            Action::SignetSuivant => self.signet_aller(false),
            Action::SignetPrecedent => self.signet_aller(true),
            Action::SignetRenommer => self.signet_renommer_ici(),
            // **Passer par `play_playlist_index`, pas par `player.play_next()`** :
            // le chemin direct sautait `appliquer_regles` et `after_media_loaded`
            // (sous-titres, chapitres, OSD) — donc une règle de profil automatique
            // ne s'appliquait qu'aux fichiers ouverts autrement qu'avec « suivant ».
            // Et `let _ =` avalait l'erreur : un fichier cassé ne disait rien.
            Action::ElementSuivant => self.element_suivant(),
            Action::ElementPrecedent => self.element_precedent(),
            Action::Echap => {
                if !self.close_top_window() && self.fullscreen {
                    self.set_fullscreen(false);
                }
            }
            Action::Preferences => self.show_settings = true,
            Action::Fermer => self.pending_viewport_cmds.push(egui::ViewportCommand::Close),
            // Les autres actions sont propres au clavier (vitesse fine, captures de
            // touches, chapitres…) : rien à faire ici.
            _ => {}
        }
        // Journal : toute action déclenchée laisse une trace vérifiable.
        tracing::info!(target: "kinterface", "action souris : {}", action.id());
    }

    /// Déplace la lecture de `delta_ms` (négatif pour reculer), sans dépasser
    /// les bornes du média.
    /// Icône de la zone de notification : démarrage / arrêt selon le réglage, puis exécution
    /// des commandes reçues. Les commandes passent par les MÊMES chemins que le clavier
    /// (`element_suivant`…), jamais par le moteur en direct.
    fn traiter_tray(&mut self, ctx: &egui::Context) {
        use crate::tray::CommandeTray;
        if self.config.interface.tray_icon && self.tray.is_none() {
            match crate::tray::Tray::demarrer(ctx.clone()) {
                Ok(t) => {
                    tracing::info!("Icône de notification publiée");
                    self.tray = Some(t);
                }
                Err(e) => {
                    tracing::warn!("Icône de notification indisponible : {}", e);
                    // Ne pas réessayer à chaque image : le réglage est remis à faux.
                    self.config.interface.tray_icon = false;
                    let _ = self.config.save();
                    self.error_message = Some(format!("Icône de notification indisponible : {e}"));
                }
            }
        } else if !self.config.interface.tray_icon {
            if let Some(t) = self.tray.take() {
                t.arreter();
                // Une icône retirée ne doit pas laisser la fenêtre cachée sans moyen de la rouvrir.
                if !self.fenetre_visible {
                    self.fenetre_visible = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                }
            }
        }
        let Some(tray) = self.tray.as_mut() else { return };
        let titre = self.player.current_path().map(|p| {
            p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
        }).unwrap_or_default();
        tray.mettre_a_jour(&titre, self.player.state() == PlayerState::Playing);
        for commande in tray.commandes() {
            match commande {
                CommandeTray::LecturePause => {
                    let _ = self.player.toggle_pause();
                }
                CommandeTray::Suivant => self.element_suivant(),
                CommandeTray::Precedent => self.element_precedent(),
                CommandeTray::AfficherMasquer => {
                    self.fenetre_visible = !self.fenetre_visible;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(self.fenetre_visible));
                    if self.fenetre_visible {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    tracing::info!("Fenêtre {} depuis l'icône", if self.fenetre_visible { "affichée" } else { "masquée" });
                }
                CommandeTray::Quitter => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            }
        }
    }

    /// (Re)charge le thème externe quand le nom choisi change. Un thème illisible est refusé avec
    /// un message et le réglage est remis à « intégré » : pas de relecture du fichier à chaque image.
    fn actualiser_theme_externe(&mut self) {
        let voulu = self.config.interface.theme.trim().to_string();
        let actuel = self.theme_externe.as_ref().map(|(n, _)| n.clone()).unwrap_or_default();
        if voulu == actuel {
            return;
        }
        if voulu.is_empty() {
            self.theme_externe = None;
            return;
        }
        match crate::theme::Theme::charger(&voulu) {
            Ok(theme) => {
                tracing::info!("Thème externe chargé : {}", voulu);
                self.theme_externe = Some((voulu, theme));
            }
            Err(e) => {
                tracing::warn!("{}", e);
                self.error_message = Some(e);
                self.theme_externe = None;
                self.config.interface.theme.clear();
                let _ = self.config.save();
            }
        }
    }

    /// Pose un signet à la position courante du média ouvert.
    fn signet_ajouter(&mut self) {
        let Some(media) = self.player.current_path().cloned() else {
            self.annonce("Aucun média ouvert");
            return;
        };
        let ms = self.player.current_position_ms();
        if self.signets.ajouter(&media, ms) {
            tracing::info!(target: "kinterface", "signet ajouté à {} ms", ms);
            self.annonce(&format!("🔖 Signet ajouté à {}", crate::utils::format_time(ms)));
        } else {
            self.annonce("Il y a déjà un signet ici");
        }
    }

    /// Saute au signet suivant (`avant = false`) ou précédent (`avant = true`).
    fn signet_aller(&mut self, avant: bool) {
        let Some(media) = self.player.current_path().cloned() else { return };
        let pos = self.player.current_position_ms();
        let cible = if avant { self.signets.precedent(&media, pos) } else { self.signets.suivant(&media, pos) };
        match cible {
            Some(ms) => {
                let _ = self.player.seek(ms);
                self.annonce(&format!("🔖 Signet à {}", crate::utils::format_time(ms)));
            }
            None => self.annonce(if avant { "Aucun signet précédent" } else { "Aucun signet suivant" }),
        }
    }

    /// Rapport d'aspect suivant, sur le cycle COMPLET (automatique → 16:9 → 4:3 → 1,85 → 2,35 →
    /// remplir → rogner → personnalisé). Un seul chemin pour le clavier et la souris : le clavier
    /// sautait auparavant les trois derniers modes (piège 6 : deux chemins pour une action).
    fn aspect_suivant(&mut self) {
        use crate::config::AspectMode::*;
        let suivant = match self.config.video.aspect {
            Auto => Ratio16x9,
            Ratio16x9 => Ratio4x3,
            Ratio4x3 => Ratio185,
            Ratio185 => Ratio235,
            Ratio235 => Remplir,
            Remplir => Rogner,
            Rogner => Personnalise,
            Personnalise => Auto,
        };
        self.definir_aspect(suivant);
    }

    /// Fixe le rapport d'aspect, l'enregistre (APRÈS le changement : l'ancienne version de « K »
    /// écrivait la configuration avant de poser la nouvelle valeur) et l'annonce.
    fn definir_aspect(&mut self, mode: crate::config::AspectMode) {
        tracing::info!(target: "kinterface", "rapport d'aspect : {:?}", mode);
        self.config.video.aspect = mode;
        let _ = self.config.save();
        self.annonce(&format!("Rapport d'aspect : {}", mode.label()));
    }

    /// Active ou coupe les shaders de pixels (la liste est conservée).
    pub(crate) fn basculer_shaders(&mut self) {
        if self.config.video.shaders.is_empty() {
            self.annonce("Aucun shader dans la liste (Préférences ▸ Image ▸ Shaders de pixels)");
            return;
        }
        let actif = !self.config.video.shaders_enabled;
        self.config.video.shaders_enabled = actif;
        let _ = self.config.save();
        self.appliquer_filtres();
        self.annonce(if actif { "Shaders de pixels : activés" } else { "Shaders de pixels : coupés" });
    }

    /// Bascule « remplir en rognant » ; quitter ce mode rétablit celui d'avant.
    fn basculer_rogner(&mut self) {
        use crate::config::AspectMode::Rogner;
        let courant = self.config.video.aspect;
        if courant == Rogner {
            let avant = self.aspect_avant_rogner;
            self.definir_aspect(avant);
        } else {
            self.aspect_avant_rogner = courant;
            self.definir_aspect(Rogner);
        }
    }

    /// Décale l'image de `dx` / `dy` pour cent (bornes ±50 %), comme les curseurs des préférences.
    fn glisser_image(&mut self, dx: f32, dy: f32) {
        let v = &mut self.config.video;
        v.pan_x_percent = (v.pan_x_percent + dx).clamp(-50.0, 50.0);
        v.pan_y_percent = (v.pan_y_percent + dy).clamp(-50.0, 50.0);
        let (x, y) = (v.pan_x_percent, v.pan_y_percent);
        let _ = self.config.save();
        self.annonce(&format!("Image décalée : horizontal {x:+.0} %, vertical {y:+.0} %"));
    }

    /// Remet l'image au centre.
    fn centrer_image(&mut self) {
        self.config.video.pan_x_percent = 0.0;
        self.config.video.pan_y_percent = 0.0;
        let _ = self.config.save();
        self.annonce("Image recentrée");
    }

    /// Ouvre la boîte de renommage du signet de la position courante (à 1,5 s près).
    fn signet_renommer_ici(&mut self) {
        let Some(media) = self.player.current_path().cloned() else { return };
        let pos = self.player.current_position_ms();
        match self.signets.index_proche(&media, pos) {
            Some(i) => self.ouvrir_renommage(media, i),
            None => self.annonce("Aucun signet à cette position"),
        }
    }

    /// Ouvre la boîte de renommage du signet `index` du média : un seul chemin pour le raccourci
    /// et le menu.
    fn ouvrir_renommage(&mut self, media: PathBuf, index: usize) {
        let Some(signet) = self.signets.liste(&media).get(index).cloned() else { return };
        self.signet_renommage = Some(RenommageSignet {
            media,
            index,
            ms: signet.ms,
            saisie: signet.nom,
            a_le_focus: false,
        });
    }

    /// Applique le nom saisi et ferme la boîte.
    fn valider_renommage(&mut self) {
        let Some(r) = self.signet_renommage.take() else { return };
        if self.signets.renommer(&r.media, r.index, &r.saisie) {
            tracing::info!(target: "kinterface", "signet à {} ms renommé « {} »", r.ms, r.saisie.trim());
            self.annonce("Signet renommé");
        } else {
            self.annonce("Nom vide : le signet garde son nom");
        }
    }

    /// Supprime le signet de la position courante (à 1,5 s près).
    fn signet_supprimer_ici(&mut self) {
        let Some(media) = self.player.current_path().cloned() else { return };
        let pos = self.player.current_position_ms();
        match self.signets.index_proche(&media, pos) {
            Some(i) if self.signets.supprimer(&media, i) => self.annonce("Signet supprimé"),
            _ => self.annonce("Aucun signet à cette position"),
        }
    }

    fn decaler(&mut self, delta_ms: i64) {
        let pos = self.player.current_position_ms() as i64;
        let cible = (pos + delta_ms).max(0) as u64;
        let total = self.player.total_duration_ms();
        let cible = if total > 0 { cible.min(total) } else { cible };
        let _ = self.player.seek(cible);
    }

    /// Ajuste le volume de `delta` (borné par le volume maximal configuré, que le
    /// moteur applique déjà).
    fn changer_volume(&mut self, delta: f32) {
        let vol = (self.player.get_volume() + delta).clamp(0.0, 1.0);
        self.player.set_volume(vol);
        self.annonce(&format!("Volume : {:.0} %", vol * 100.0));
    }

    /// Applique la chaîne de filtres vidéo au réglage courant (miroirs, recadrage,
    /// couleurs…) et enregistre la configuration.
    pub(crate) fn appliquer_filtres(&mut self) {
        let video = self.config.video.clone();
        // L'aperçu du recadrage doit être posé AVANT la chaîne : c'est une entrée
        // de sa construction, pas un effet appliqué ensuite.
        // Aperçu ⇔ mode réglage. La préférence `crop_overlay` ne dit que si le
        // repère est souhaité ; elle ne doit pas suspendre le recadrage (c'était
        // le défaut : le recadrage n'était alors JAMAIS appliqué à l'écran).
        self.player
            .set_crop_apercu(self.crop_reglage && video.crop_active());
        self.player.set_video_filters(&video);
    }

    /// Affiche un message d'état éphémère (retour d'une bascule de fonction).
    pub(crate) fn annonce(&mut self, message: &str) {
        self.status_message = Some(message.to_string());
        self.status_timer = self.osd_duration_secs();
    }

    /// Durée d'affichage de l'OSD, en secondes (issue de la configuration).
    fn osd_duration_secs(&self) -> f32 {
        (self.config.interface.osd_duration.clamp(500, 20_000) as f32) / 1000.0
    }

    /// Bascule ou fixe le plein écran : état interne (masquage de l'interface),
    /// demande au gestionnaire de fenêtres, et journal explicite.
    fn set_fullscreen(&mut self, enabled: bool) {
        if self.fullscreen == enabled {
            return;
        }
        self.fullscreen = enabled;
        self.pending_fullscreen = Some(enabled);
        tracing::info!("Plein écran: {}", enabled);
    }

    /// Mémorise la taille et la position de la fenêtre, pour la retrouver au
    /// lancement suivant.
    ///
    /// **Deux sessions, deux possibilités.** Sous X11 (et sous XWayland), la
    /// position et la taille sont connues et imposables : la fenêtre se rouvre
    /// exactement là où on l'avait laissée. Sous **Wayland**, le protocole ne donne
    /// pas la position d'une fenêtre à son client — `inner_rect` et `outer_rect`
    /// restent `None` en permanence (mesuré, Plasma 6.8 / winit d'egui 0.33) — et
    /// aucun client ne peut se placer lui-même : c'est le compositeur qui décide.
    /// On y enregistre donc la **taille** (lue dans `screen_rect`, la surface
    /// allouée à egui) et l'état agrandi, jamais une position inventée ; la
    /// position relève d'une règle de fenêtre du compositeur
    /// (`tools/regle-fenetre-kwin.sh` pour KWin).
    ///
    /// **Pourquoi une temporisation** : pendant qu'on tire un bord, la taille
    /// change à chaque image. Écrire la configuration à chaque changement
    /// réécrirait le fichier des dizaines de fois par seconde ; on attend donc
    /// `TEMPORISATION_GEOMETRIE` de stabilité et on écrit une seule fois.
    ///
    /// **Ce qui n'est jamais enregistré** : le plein écran (la géométrie observée
    /// serait celle de l'écran entier), la fenêtre réduite (taille incohérente),
    /// et la géométrie d'une fenêtre **agrandie** — dans ce cas seul le drapeau
    /// est retenu, la position et la taille enregistrées restant celles de l'état
    /// normal : c'est ce qui permet de « restaurer » une fenêtre raisonnable.
    ///
    /// **Piège 2** : tout est lu dans une seule fermeture `ctx.input(...)` ; la
    /// correction de position passe par `pending_viewport_cmds`, jamais par un
    /// appel `ctx.*` fait depuis la fermeture (interblocage : KWin tue la fenêtre).
    fn memoriser_geometrie(&mut self, ctx: &egui::Context) {
        if self.fullscreen {
            return;
        }
        // Lecture seule, en une seule fermeture.
        let (interieur, exterieur, ecran, agrandie, reduite, surface) = ctx.input(|i| {
            let vp = i.viewport();
            (
                vp.inner_rect,
                vp.outer_rect,
                vp.monitor_size,
                vp.maximized == Some(true),
                vp.minimized == Some(true),
                i.content_rect(),
            )
        });
        if reduite {
            return;
        }
        // Taille : la zone cliente quand le serveur d'affichage la rapporte ; sous
        // Wayland, la surface allouée à egui (`screen_rect`) est la seule mesure
        // disponible — et c'est bien la taille cliente.
        let taille = interieur.map(|r| [r.width(), r.height()]);
        let taille = taille.unwrap_or([surface.width(), surface.height()]);
        // Position : connue sous X11 seulement. `outer_rect` donne le cadre, ce
        // qu'attend le gestionnaire de fenêtres ; à défaut, l'intérieur.
        let position = interieur.map(|r| exterieur.map_or(r.min, |e| e.min));

        // Une seule vérification contre l'écran réel, après un court délai : au
        // tout premier tour, le gestionnaire de fenêtres n'a pas encore placé la
        // fenêtre et la géométrie rapportée n'est pas encore la bonne. Sous
        // Wayland, il n'y a rien à corriger (aucune position connue) : on le dit
        // une fois, et on n'essaie pas d'envoyer une commande sans effet.
        if !self.geometrie_verifiee && self.demarrage.elapsed().as_millis() >= 700 {
            self.geometrie_verifiee = true;
            if self.wayland {
                tracing::info!(
                    "Session Wayland : la position de la fenêtre est décidée par le \
                     compositeur (taille et état agrandi sont restitués au lancement). \
                     Pour une position fixe, voir tools/regle-fenetre-kwin.sh"
                );
            } else if let (Some(ecran), Some(position)) = (ecran, position) {
                let (pos, voulue) =
                    crate::config::geometrie_visible([position.x, position.y], taille, [ecran.x, ecran.y]);
                let bouge = (pos[0] - position.x).abs() > 0.5 || (pos[1] - position.y).abs() > 0.5;
                let redimensionnee = (voulue[0] - taille[0]).abs() > 0.5
                    || (voulue[1] - taille[1]).abs() > 0.5;
                if bouge || redimensionnee {
                    tracing::warn!(
                        "Fenêtre enregistrée hors écran ou trop grande : replacée \
                         ({:.0}×{:.0} en {:.0},{:.0} sur un écran {:.0}×{:.0})",
                        voulue[0],
                        voulue[1],
                        pos[0],
                        pos[1],
                        ecran.x,
                        ecran.y
                    );
                    if redimensionnee {
                        self.pending_viewport_cmds
                            .push(egui::ViewportCommand::InnerSize(egui::vec2(voulue[0], voulue[1])));
                    }
                    self.pending_viewport_cmds.push(egui::ViewportCommand::OuterPosition(
                        egui::pos2(pos[0], pos[1]),
                    ));
                }
            }
        }

        // Sous la taille minimale, ce n'est pas une géométrie : fenêtre en cours
        // d'initialisation, ou état transitoire de réduction/restauration.
        if taille[0] < crate::config::TAILLE_MIN_FENETRE[0]
            || taille[1] < crate::config::TAILLE_MIN_FENETRE[1]
        {
            return;
        }

        // Le changement est-il déjà vu ? On repart alors le chronomètre.
        let courante = (position.map(|p| [p.x, p.y]), taille, agrandie);
        let identique = self.geometrie_observee.is_some_and(|(p, t, a)| {
            let meme_position = match (p, courante.0) {
                (None, None) => true,
                (Some(a), Some(b)) => (a[0] - b[0]).abs() < 0.5 && (a[1] - b[1]).abs() < 0.5,
                _ => false,
            };
            a == courante.2
                && meme_position
                && (t[0] - courante.1[0]).abs() < 0.5
                && (t[1] - courante.1[1]).abs() < 0.5
        });
        if !identique {
            self.geometrie_observee = Some(courante);
            self.geometrie_changee_le = Some(std::time::Instant::now());
            return;
        }

        // Stable depuis assez longtemps ?
        let Some(depuis) = self.geometrie_changee_le else {
            return;
        };
        if depuis.elapsed() < TEMPORISATION_GEOMETRIE {
            return;
        }

        // Déjà enregistré ? (Au demi-point près : la géométrie rapportée par le
        // gestionnaire de fenêtres n'est pas toujours au pixel.)
        let iface = &self.config.interface;
        let position_egale = match (iface.window_pos, position) {
            (Some(a), Some(p)) => (a[0] - p.x).abs() < 0.5 && (a[1] - p.y).abs() < 0.5,
            // Sans position connue (Wayland), la valeur enregistrée reste celle de
            // la dernière session X11 : elle ne doit pas être effacée ici.
            (_, None) => true,
            _ => false,
        };
        let deja = if agrandie {
            iface.window_maximized && iface.window_size.is_some()
        } else {
            !iface.window_maximized
                && position_egale
                && iface
                    .window_size
                    .is_some_and(|t| (t[0] - taille[0]).abs() < 0.5 && (t[1] - taille[1]).abs() < 0.5)
        };
        if deja {
            self.geometrie_changee_le = None;
            return;
        }

        if agrandie && iface.window_size.is_some() {
            // Fenêtre agrandie : on retient l'état, pas la géométrie de l'écran.
            self.config.interface.window_maximized = true;
        } else {
            self.config.interface.window_size = Some(taille);
            if let Some(p) = position {
                self.config.interface.window_pos = Some([p.x, p.y]);
            }
            self.config.interface.window_maximized = agrandie;
        }
        self.geometrie_changee_le = None;
        let memorisee = (
            self.config.interface.window_pos,
            self.config.interface.window_size.unwrap_or(taille),
            self.config.interface.window_maximized,
        );
        match self.config.save() {
            Ok(()) => {
                let ou = match memorisee.0 {
                    Some(p) => format!(" en {:.0},{:.0}", p[0], p[1]),
                    None => String::new(),
                };
                tracing::info!(
                    "Géométrie enregistrée : {:.0}×{:.0}{}{}",
                    memorisee.1[0],
                    memorisee.1[1],
                    ou,
                    if memorisee.2 { " (agrandie)" } else { "" }
                )
            }
            Err(e) => tracing::warn!("Géométrie non enregistrée : {}", e),
        }
    }

    /// Met en pause (ou reprend) selon le focus de la fenêtre.
    ///
    /// **Pourquoi une méthode dédiée** : trois cas se ressemblent et ne doivent pas
    /// être confondus — la fenêtre **réduite** (`minimized`, Wayland et X11), la
    /// perte de **focus clavier** (autre application au premier plan), et le retour
    /// au premier plan. Chacun a son réglage, et la reprise ne concerne **que** les
    /// pauses décidées ici (`pause_par_focus`) : une pause volontaire de
    /// l'utilisateur ne doit jamais être annulée par un simple clic ailleurs.
    ///
    /// Aucune commande de fenêtre n'est envoyée depuis la fermeture `ctx.input(...)`
    /// (piège 2) : tout se fait après lecture.
    fn traiter_focus(&mut self, ctx: &egui::Context) {
        let (focus, reduite) = ctx.input(|i| {
            let vp = i.viewport();
            (vp.focused.unwrap_or(true), vp.minimized == Some(true))
        });
        let mode = self.config.interface.pause_sur_perte_focus;
        let perdu_maintenant = !focus || reduite;
        let avant = self.focus_precedent;
        self.focus_precedente_maj(focus, reduite);

        // Pas de réglage, ou rien n'a changé : rien à faire.
        if mode == 0 || avant.is_none() {
            return;
        }
        let perdu_avant = avant.map(|f| !f).unwrap_or(false)
            || self.reduite_precedente;
        if !perdu_avant && perdu_maintenant {
            // Faut-il réagir à cet événement précis ?
            let concerne = mode == 1 || (mode == 2 && reduite);
            if concerne && self.player.state() == PlayerState::Playing {
                let _ = self.player.toggle_pause();
                self.pause_par_focus = true;
                tracing::info!(
                    "Focus perdu ({}) : lecture mise en pause (réglage {})",
                    if reduite { "fenêtre réduite" } else { "autre fenêtre" },
                    mode
                );
            }
        } else if perdu_avant && !perdu_maintenant {
            if self.pause_par_focus && self.config.interface.reprendre_sur_focus {
                if self.player.state() == PlayerState::Paused {
                    let _ = self.player.toggle_pause();
                    tracing::info!("Retour au premier plan : lecture reprise");
                }
            }
            self.pause_par_focus = false;
        }
    }

    /// Mémorise l'état de focus de ce tour (appelé par `traiter_focus`).
    fn focus_precedente_maj(&mut self, focus: bool, reduite: bool) {
        self.focus_precedent = Some(focus);
        self.reduite_precedente = reduite;
    }

    /// Démarre le serveur MPRIS (une fois, au premier tour).
    ///
    /// Séparé de `App::new` : la connexion D-Bus prend quelques millisecondes et
    /// peut échouer (pas de bus, nom déjà pris) — un échec doit laisser le lecteur
    /// intact, ce que fait `mpris::Demarrage::Indisponible`.
    fn demarrer_mpris(&mut self, ctx: &egui::Context) {
        match crate::mpris::demarrer("RMPlayer") {
            crate::mpris::Demarrage::Actif(serveur) => {
                // Réveil de l'interface : sans lui, une commande reçue pendant une
                // pause n'était exécutée qu'au prochain événement (touche, souris).
                serveur.reveiller_avec(ctx.clone());
                tracing::info!("MPRIS actif : {}", serveur.nom());
                self.mpris = Some(serveur);
            }
            crate::mpris::Demarrage::Indisponible(raison) => {
                tracing::info!("MPRIS indisponible : {}", raison);
            }
        }
    }

    /// Publie l'état de lecture vers le bureau, et exécute ses demandes.
    ///
    /// **Tout passe par les chemins de l'interface** (`element_suivant`,
    /// `toggle_pause`, `seek`…) : une touche média doit se comporter exactement
    /// comme le bouton équivalent — règles de profil, sous-titres, OSD compris
    /// (§4 piège 86). C'est la raison d'être de la file de commandes.
    fn traiter_mpris(&mut self) {
        let Some(serveur) = self.mpris.clone() else {
            return;
        };
        // --- Publication de l'état ---
        let (titre, artiste) = self
            .player
            .media_info()
            .map(|i| {
                (
                    i.filename.clone(),
                    i.audio_tracks
                        .first()
                        .map(|t| t.language.clone())
                        .unwrap_or_default(),
                )
            })
            .unwrap_or_default();
        let total = self.player.playlist.len();
        let rang = self.player.playlist_index();
        let etat = crate::mpris::Etat {
            lecture: match self.player.state() {
                PlayerState::Playing => crate::mpris::Lecture::Lecture,
                PlayerState::Paused => crate::mpris::Lecture::Pause,
                _ => crate::mpris::Lecture::Arrete,
            },
            position_ms: self.player.current_position_ms(),
            duree_ms: self.player.total_duration_ms(),
            titre,
            artiste,
            rang,
            // MPRIS attend un volume de 0 à 1 ; RMPlayer autorise jusqu'à 1,5.
            volume: (self.player.get_volume() as f64).clamp(0.0, 1.0),
            aleatoire: self.player.shuffle(),
            repetition: self.player.repeat(),
            vitesse: self.player.get_speed() as f64,
            peut_precedent: total > 1,
            peut_suivant: total > 1,
        };
        serveur.publier(etat);

        // --- Exécution des demandes ---
        for commande in serveur.commandes() {
            tracing::info!("MPRIS : commande reçue {:?}", commande);
            match commande {
                crate::mpris::Commande::Lecture => {
                    if self.player.state() != PlayerState::Playing {
                        let _ = self.player.toggle_pause();
                    }
                }
                crate::mpris::Commande::Pause => {
                    if self.player.state() == PlayerState::Playing {
                        let _ = self.player.toggle_pause();
                    }
                }
                crate::mpris::Commande::LecturePause => {
                    let _ = self.player.toggle_pause();
                }
                crate::mpris::Commande::Arreter => {
                    let _ = self.player.stop();
                }
                crate::mpris::Commande::Suivant => self.element_suivant(),
                crate::mpris::Commande::Precedent => self.element_precedent(),
                crate::mpris::Commande::SeDeplacer(us) => {
                    let delta = crate::mpris::millis(us);
                    let actuel = self.player.current_position_ms();
                    let cible = if us >= 0 {
                        actuel.saturating_add(delta)
                    } else {
                        actuel.saturating_sub(delta)
                    };
                    let total = self.player.total_duration_ms();
                    let cible = if total > 0 { cible.min(total) } else { cible };
                    let _ = self.player.seek(cible);
                }
                crate::mpris::Commande::Position(us) => {
                    let cible = crate::mpris::millis(us);
                    let total = self.player.total_duration_ms();
                    let cible = if total > 0 { cible.min(total) } else { cible };
                    let _ = self.player.seek(cible);
                }
                crate::mpris::Commande::Volume(v) => {
                    self.player.set_volume(v.clamp(0.0, 1.0) as f32);
                }
                crate::mpris::Commande::Vitesse(v) => {
                    // `set_speed` borne déjà aux limites du moteur ; on arrondit au
                    // dixième, comme les raccourcis `[` et `]` (un client peut
                    // demander 1,037 : la barre afficherait un nombre illisible).
                    let arrondi = (v * 10.0).round() / 10.0;
                    self.player.set_speed(arrondi as f32);
                }
                crate::mpris::Commande::Aleatoire(actif) => {
                    if self.player.shuffle() != actif {
                        self.basculer_aleatoire();
                    }
                }
                crate::mpris::Commande::Repetition(mode) => self.definir_repetition(mode),
                crate::mpris::Commande::Quitter => {
                    self.pending_viewport_cmds
                        .push(egui::ViewportCommand::Close);
                }
                crate::mpris::Commande::Avant => {
                    self.pending_viewport_cmds.push(egui::ViewportCommand::Focus);
                }
            }
        }
    }

    /// Traite les vignettes de la barre de progression.
    ///
    /// Appelée une fois par tour : elle dépose la demande de survol, récupère un
    /// résultat prêt (jamais bloquant) et tient la texture à jour.
    ///
    /// **Ce qui est évité** : lancer le fil de travail au démarrage (il ouvrirait le
    /// fichier pour rien), demander une vignette quand la souris n'est pas sur la
    /// barre, et demander deux fois la même tranche (le générateur le fait déjà).
    fn traiter_vignettes(&mut self, ctx: &egui::Context) {
        let Some(chemin) = self.player.current_path().map(|p| p.to_path_buf()) else {
            return;
        };
        // Un flux réseau n'a pas de vignettes : le décodage serait à refaire à chaque
        // position, sur un média qui n'est pas encore là.
        if crate::utils::is_stream_url(&chemin.to_string_lossy()) {
            return;
        }
        let Some(cible) = self.vignette_cible else {
            return;
        };
        // **Déjà en main ?** Sans ce test, la boucle d'affichage redemandait la même
        // tranche à chaque image : mesuré, 53 demandes pour 48 reçues alors que la
        // souris était immobile — le décodeur travaillait sans arrêt pour rien.
        let tranche_visee = cible / crate::vignettes::TRANCHE_MS;
        if let Some((deja, _)) = &self.vignette_affichee {
            if *deja == tranche_visee {
                return;
            }
        }
        // Et si elle est déjà en cours de calcul, le générateur le sait aussi : on
        // ne refait rien, mais on laisse un tour de plus pour recevoir le résultat.
        if self.vignettes.is_none() {
            self.vignettes = Some(crate::vignettes::Generateur::demarrer());
        }
        if let Some(generateur) = self.vignettes.as_mut() {
            generateur.demander(
                &chemin,
                cible,
                crate::vignettes::LARGEUR_DEFAUT,
                crate::vignettes::TRANCHE_MS,
            );
            // Un résultat (éventuellement d'une demande précédente) est récupéré
            // dans le même tour : c'est ce qui rend l'affichage fluide au survol.
            if let Some((tranche, resultat)) = generateur.recuperer() {
                match resultat {
                    Ok(vignette) => {
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [vignette.largeur as usize, vignette.hauteur as usize],
                            &vignette.rgba,
                        );
                        let texture = ctx.load_texture(
                            "vignette-progression",
                            image,
                            egui::TextureOptions::LINEAR,
                        );
                        tracing::debug!(
                            target: "kinterface",
                            "vignette prête : tranche {} ({}x{}, {} ms)",
                            tranche,
                            vignette.largeur,
                            vignette.hauteur,
                            vignette.position_ms
                        );
                        self.vignette_affichee = Some((tranche, texture));
                    }
                    Err(e) => {
                        tracing::debug!("vignette indisponible : {}", e);
                        self.vignette_affichee = None;
                    }
                }
            }
            // Une vignette en cours de calcul a besoin d'un tour de plus : sans cela,
            // egui resterait passif et le résultat n'arriverait jamais à l'écran.
            if generateur.en_cours() {
                ctx.request_repaint_after(std::time::Duration::from_millis(60));
            }
        }
    }

    /// Affiche ou masque le panneau de playlist — **toujours en le journalisant**.
    ///
    /// Trois chemins y mènent (raccourci `L`, deux entrées de menu) : ils passaient
    /// chacun par `self.show_playlist = !self.show_playlist`, donc une bascule
    /// silencieuse. Un banc ne peut pas vérifier ce qui n'écrit rien (règle du
    /// projet : un état doit être observable), et l'utilisateur non plus quand le
    /// panneau semble ne pas répondre.
    pub(crate) fn basculer_panneau_playlist(&mut self) {
        self.show_playlist = !self.show_playlist;
        tracing::info!(
            "Panneau playlist : {}",
            if self.show_playlist { "affiché" } else { "masqué" }
        );
    }

    /// Élément **suivant** de la playlist — un seul chemin pour les quatre
    /// commandes qui le demandent (raccourci `N`, menu, bouton ⏭, enchaînement).
    ///
    /// **Pourquoi une méthode et pas quatre appels au moteur** : les boutons et le
    /// menu appelaient `player.play_next()` en direct, donc sans `appliquer_regles`
    /// ni `after_media_loaded` — une règle de profil ne s'appliquait pas au fichier
    /// atteint (défaut trouvé par `verif-playlist.sh`). Le journal, lui, est ce que
    /// l'audit de l'interface lit pour savoir qu'un bouton a agi : il est donc écrit
    /// ici, une fois, avec le nom du fichier.
    fn element_suivant(&mut self) {
        match self.player.prochain_index() {
            Some(i) => {
                let nom = self
                    .player
                    .playlist
                    .get(i)
                    .map(|p| {
                        p.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();
                tracing::info!("Playlist : élément suivant → {} ({})", i, nom);
                self.play_playlist_index(i);
            }
            None => tracing::info!("Playlist : élément suivant demandé, playlist vide"),
        }
    }

    /// Élément **précédent** — même logique que `element_suivant`.
    fn element_precedent(&mut self) {
        match self.player.precedent_index() {
            Some(i) => {
                let nom = self
                    .player
                    .playlist
                    .get(i)
                    .map(|p| {
                        p.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();
                tracing::info!("Playlist : élément précédent → {} ({})", i, nom);
                self.play_playlist_index(i);
            }
            None => tracing::info!("Playlist : élément précédent demandé, playlist vide"),
        }
    }

    /// Fixe la répétition (moteur + configuration) : un seul chemin pour le menu, le panneau de
    /// playlist, les préférences, les raccourcis et MPRIS.
    pub(crate) fn definir_repetition(&mut self, mode: crate::config::RepeatMode) {
        self.player.set_repeat(mode);
        self.config.playback.repeat = mode;
        let _ = self.config.save();
        self.annonce(&format!("Répétition : {}", mode.libelle().to_lowercase()));
    }

    /// Aucune → fichier → playlist → aucune.
    pub(crate) fn repetition_suivante(&mut self) {
        let suivant = self.player.repeat().suivant();
        self.definir_repetition(suivant);
    }

    /// `Ctrl+L` : relire le fichier courant, ou arrêter la répétition (quel que soit le mode).
    pub(crate) fn basculer_boucle(&mut self) {
        let mode = if self.player.repeat() == crate::config::RepeatMode::Fichier {
            crate::config::RepeatMode::Aucune
        } else {
            crate::config::RepeatMode::Fichier
        };
        self.definir_repetition(mode);
    }

    /// Active ou coupe la lecture continue et retient le choix.
    pub(crate) fn basculer_continue(&mut self) {
        let actif = !self.player.continuous();
        self.player.set_continuous(actif);
        self.config.playback.continuous = actif;
        let _ = self.config.save();
        self.annonce(if actif {
            "Lecture continue : activée (les fichiers s'enchaînent)"
        } else {
            "Lecture continue : désactivée (arrêt après chaque fichier)"
        });
    }

    /// Active ou coupe la lecture aléatoire, et **retient le choix** dans la
    /// configuration (on ne veut pas le réactiver à chaque film).
    pub(crate) fn basculer_aleatoire(&mut self) {
        let actif = !self.player.shuffle();
        self.player.set_shuffle(actif);
        self.config.playback.shuffle = actif;
        let _ = self.config.save();
        self.annonce(if actif {
            "Lecture aléatoire : activée (chaque élément passe une fois)"
        } else {
            "Lecture aléatoire : désactivée (ordre de la liste)"
        });
    }

    /// Branche l'écoute de l'instance unique (voir `instance.rs`).
    pub fn brancher_instance(&mut self, instance: Option<crate::instance::Instance>) {
        self.instance = instance;
    }

    /// Traite les fichiers transmis par un autre processus.
    ///
    /// Comportement attendu par l'utilisateur qui double-clique une vidéo alors que
    /// le lecteur tourne : **le fichier se lit**. Le média courant est donc remplacé
    /// (et non simplement ajouté à la playlist), les fichiers suivants prennent place
    /// à la suite dans la liste, et la fenêtre est ramenée en avant — ce dernier
    /// point n'est qu'une *demande* sous Wayland (`ViewportCommand::Focus`), le
    /// compositeur pouvant l'ignorer.
    fn traiter_fichiers_recus(&mut self, ctx: &egui::Context) {
        let Some(instance) = &self.instance else {
            return;
        };
        let recus = instance.recuperer();
        if recus.is_empty() {
            return;
        }
        // Une seule salve par tour suffit : les suivantes attendent la frame
        // suivante, ce qui évite d'ouvrir dix médias d'un coup si le gestionnaire
        // de fichiers en a envoyé plusieurs.
        let mut iterateur = recus.into_iter();
        let premiers = iterateur.next().unwrap_or_default();
        let mut tous: Vec<std::path::PathBuf> = premiers.clone();
        for salve in iterateur {
            tous.extend(salve);
        }
        // Un chemin disparu entre le clic et la réception (fichier déplacé, clé USB
        // retirée) ne doit pas empêcher de lire les autres : il est écarté, et le
        // journal dit pourquoi.
        let avant = tous.len();
        tous.retain(|c| {
            let garde = crate::instance::est_media(c);
            if !garde {
                tracing::warn!("chemin reçu ignoré (absent) : {}", c.display());
            }
            garde
        });
        if tous.len() < avant {
            self.annonce(&format!(
                "{} fichier(s) reçu(s) introuvable(s) — ignoré(s)",
                avant - tous.len()
            ));
        }
        let Some(premier) = tous.first().cloned() else {
            // Aucun fichier exploitable : simple demande d'activation.
            self.pending_viewport_cmds.push(egui::ViewportCommand::Focus);
            return;
        };
        tracing::info!(
            "ouverture demandée par un autre processus : {} fichier(s), premier = {}",
            tous.len(),
            premier.display()
        );
        for fichier in &tous {
            if !self.player.playlist.contains(fichier) {
                self.player.add_to_playlist(fichier.clone());
                if !self.config.playlist.contains(fichier) {
                    self.config.playlist.push(fichier.clone());
                }
            }
        }
        let _ = self.config.save();
        self.load_media(&premier);
        // `load_media` laisse le lecteur en pause : une vidéo double-cliquée doit
        // démarrer, comme dans n'importe quel lecteur de bureau.
        if self.player.state() != PlayerState::Playing {
            let _ = self.player.toggle_pause();
        }
        self.annonce(&format!(
            "Ouvert depuis le gestionnaire de fichiers : {}",
            premier
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| premier.display().to_string())
        ));
        self.pending_viewport_cmds.push(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }

    /// Active le mode plein écran (interface masquée + demande au gestionnaire
    /// de fenêtres). Utilisé par l'option `--fullscreen`.
    pub fn set_startup_fullscreen(&mut self) {
        self.fullscreen = true;
        self.pending_fullscreen = Some(true);
    }

    /// Dessine les sous-titres actifs par-dessus l'image vidéo.
    /// Dessine les événements ASS actifs avec leur mise en page d'origine.
    ///
    /// Retourne `true` si quelque chose a été dessiné — auquel cas l'appelant ne
    /// dessine pas le rendu uniforme (sinon le texte apparaîtrait en double).
    fn draw_ass(
        &self,
        ui: &egui::Ui,
        image: &egui::Response,
        disp: egui::Vec2,
        position: u64,
        script: &crate::ass::AssScript,
    ) -> bool {
        // La zone du sous-titre est celle de l'image affichée : la zone du widget
        // n'est pas la zone de l'image (letterbox).
        let rect = Self::image_rect(image, disp).intersect(ui.clip_rect());
        if rect.height() < 20.0 || rect.width() < 20.0 {
            return false;
        }
        let painter = ui.painter();
        // Mesureur réel : la largeur d'un texte pour une taille et une graisse données.
        // On passe par la galerie d'egui pour que le placement corresponde exactement
        // à ce qui sera dessiné (une estimation ferait déborder ou décentrer).
        let mut dessine = false;
        for evenement in script.events_at(position) {
            let opacite = script.opacite(evenement, position);
            if opacite <= 0.01 {
                continue;
            }
            let mesurer = |texte: &str, police: f32, gras: bool| -> f32 {
                let galerie = painter.layout_no_wrap(
                    texte.to_string(),
                    egui::FontId::proportional(police),
                    egui::Color32::WHITE,
                );
                let w = galerie.size().x;
                if gras {
                    w + police * 0.06
                } else {
                    w
                }
            };
            // Le temps est RELATIF au début de l'événement : c'est la base des
            // animations `\move` / `\t` et du karaoké.
            let temps_relatif = position.saturating_sub(evenement.start_ms) as u32;
            let places = crate::ass::placer(
                evenement,
                rect.width(),
                rect.height(),
                script.play_res,
                opacite,
                temps_relatif,
                &mesurer,
            );
            for p in places {
                if p.text.trim().is_empty() {
                    continue;
                }
                // --- Découpage `\clip` / `\iclip` ------------------------------
                // On dessine avec une liste de peintres : un seul pour un découpage
                // ordinaire, quatre (les bandes autour de la zone retirée) pour un
                // `\iclip`. Repeindre une zone déjà peinte est sans effet visible :
                // les recouvrements entre bandes ne changent rien à l'image.
                for peintre in Self::peintres_decoupes(painter, rect, p.clip.as_ref()) {
                    if dessiner_fragment_ass(&peintre, rect, &p) {
                        dessine = true;
                    }
                }
            }
        }
        dessine
    }

    /// Peintres à utiliser pour un fragment, compte tenu de son découpage.
    ///
    /// - pas de découpage : le peintre d'origine ;
    /// - `\clip` : le peintre restreint au rectangle gardé ;
    /// - `\iclip` : quatre bandes qui entourent la zone retirée (egui ne sait pas
    ///   découper « à l'extérieur » d'un rectangle).
    fn peintres_decoupes(
        painter: &egui::Painter,
        zone: egui::Rect,
        clip: Option<&crate::ass::ClipPlace>,
    ) -> Vec<egui::Painter> {
        let Some(clip) = clip else {
            return vec![painter.clone()];
        };
        let mut base = painter.clone();
        if let Some((x1, y1, x2, y2)) = clip.garde {
            let garde = egui::Rect::from_min_max(
                egui::pos2(zone.left() + x1, zone.top() + y1),
                egui::pos2(zone.left() + x2, zone.top() + y2),
            );
            if garde.width() <= 0.5 || garde.height() <= 0.5 {
                return Vec::new();
            }
            base = base.with_clip_rect(garde);
        }
        if clip.exclusions.is_empty() {
            return vec![base];
        }
        // Découpage inversé : bandes haut, bas, gauche, droite autour de la zone.
        let clip_actuel = base.clip_rect();
        let mut sortie = Vec::new();
        for (x1, y1, x2, y2) in &clip.exclusions {
            let z = egui::Rect::from_min_max(
                egui::pos2(zone.left() + x1, zone.top() + y1),
                egui::pos2(zone.left() + x2, zone.top() + y2),
            );
            let bandes = [
                // au-dessus
                egui::Rect::from_min_max(clip_actuel.min, egui::pos2(clip_actuel.max.x, z.min.y)),
                // en dessous
                egui::Rect::from_min_max(egui::pos2(clip_actuel.min.x, z.max.y), clip_actuel.max),
                // à gauche
                egui::Rect::from_min_max(
                    egui::pos2(clip_actuel.min.x, z.min.y.max(clip_actuel.min.y)),
                    egui::pos2(z.min.x, z.max.y.min(clip_actuel.max.y)),
                ),
                // à droite
                egui::Rect::from_min_max(
                    egui::pos2(z.max.x, z.min.y.max(clip_actuel.min.y)),
                    egui::pos2(clip_actuel.max.x, z.max.y.min(clip_actuel.max.y)),
                ),
            ];
            for b in bandes {
                if b.width() > 0.5 && b.height() > 0.5 {
                    sortie.push(base.with_clip_rect(base.clip_rect().intersect(b)));
                }
            }
        }
        sortie
    }

    /// Dépose une composition de sous-titre image (PGS, DVD, DVB) sur la vidéo.
    ///
    /// Les coordonnées du fichier sont exprimées dans le repère **du média**
    /// (1920×1080 pour un Blu-ray). On les reporte sur la zone réellement affichée,
    /// avec deux facteurs d'échelle distincts : si un rapport d'aspect est imposé,
    /// l'image est étirée et le sous-titre doit l'être autant.
    fn draw_bitmap_subtitle(
        &mut self,
        ui: &egui::Ui,
        image: &egui::Response,
        disp: egui::Vec2,
        cue: &crate::player::BitmapCue,
    ) {
        let rect = Self::image_rect(image, disp).intersect(ui.clip_rect());
        if rect.width() < 20.0 || rect.height() < 20.0 {
            return;
        }
        let native = self
            .player
            .media_info()
            .map(|i| i.resolution)
            .filter(|(w, h)| *w > 0 && *h > 0)
            .unwrap_or((rect.width() as u32, rect.height() as u32));
        let sx = rect.width() / native.0 as f32;
        let sy = rect.height() / native.1 as f32;

        // Textures de l'événement courant seulement : une nouvelle composition
        // remplace l'ancienne, inutile de garder son téléversement.
        let debut = cue.start_ms;
        let mut textures: Vec<Option<egui::TextureHandle>> = Vec::with_capacity(cue.rects.len());
        for (i, r) in cue.rects.iter().enumerate() {
            if r.width == 0 || r.height == 0 {
                textures.push(None);
                continue;
            }
            let connu = self
                .bitmap_textures
                .iter()
                .find(|(d, idx, _)| *d == debut && *idx == i)
                .map(|(_, _, h)| h.clone());
            let handle = match connu {
                Some(h) => h,
                None => {
                    let img = egui::ColorImage::from_rgba_unmultiplied(
                        [r.width as usize, r.height as usize],
                        &r.rgba,
                    );
                    let h = ui.ctx().load_texture(
                        format!("sous-titre-image-{}-{}", debut, i),
                        img,
                        egui::TextureOptions::LINEAR,
                    );
                    self.bitmap_textures.push((debut, i, h.clone()));
                    h
                }
            };
            textures.push(Some(handle));
        }
        // Oublier les textures des compositions précédentes.
        self.bitmap_textures.retain(|(d, _, _)| *d == debut);

        let painter = ui.painter();
        for (r, tex) in cue.rects.iter().zip(textures.iter()) {
            let Some(handle) = tex else { continue };
            let min = egui::pos2(
                rect.left() + r.x as f32 * sx,
                rect.top() + r.y as f32 * sy,
            );
            let taille = egui::vec2(r.width as f32 * sx, r.height as f32 * sy);
            let dest = egui::Rect::from_min_size(min, taille).intersect(rect);
            // Rognage éventuel : on ajuste les coordonnées de texture en
            // conséquence pour ne pas déformer l'image.
            let uv = egui::Rect::from_min_max(
                egui::pos2(
                    (dest.left() - min.x) / taille.x,
                    (dest.top() - min.y) / taille.y,
                ),
                egui::pos2(
                    (dest.right() - min.x) / taille.x,
                    (dest.bottom() - min.y) / taille.y,
                ),
            );
            painter.image(
                handle.id(),
                dest,
                uv,
                egui::Color32::WHITE,
            );
        }
    }

    /// Zone RÉELLEMENT occupée par l'image, dans la fenêtre.
    ///
    /// `image.rect` est la zone **allouée** au widget, et sous `centered_and_justified`
    /// elle vaut tout l'espace disponible : plus grande que l'image dès que le
    /// rapport d'aspect du média diffère de celui de la fenêtre. S'en servir pour
    /// positionner les sous-titres les **étirait** (mesuré : 264 px au lieu de
    /// 247 pour un rectangle de 400 px). On recalcule donc la zone à partir de la
    /// taille d'affichage demandée, centrée comme l'image l'est.
    fn image_rect(image: &egui::Response, disp: egui::Vec2) -> egui::Rect {
        egui::Rect::from_center_size(image.rect.center(), disp)
    }

    fn draw_subtitles(
        &mut self,
        ui: &egui::Ui,
        image: &egui::Response,
        disp: egui::Vec2,
    ) {
        if !self.subtitles.is_visible() {
            return;
        }
        // Retard/avance demandé par l'utilisateur (corrige un fichier mal minuté).
        let decalage = self.config.subtitles.delay_ms;
        let position = if decalage >= 0 {
            self.player.current_position_ms().saturating_sub(decalage as u64)
        } else {
            self.player.current_position_ms() + (-decalage) as u64
        };
        // 1) Un fichier ASS/SSA actif porte sa propre mise en page : position, couleurs,
        //    tailles du fichier. On le dessine tel quel — sinon on perdrait tout
        //    l'intérêt du format (c'est ce qui distingue un ASS d'un SRT).
        if let Some(script) = self.subtitles.active_ass() {
            if self.draw_ass(ui, image, disp, position, script) {
                return;
            }
        }

        // 2) Sinon, rendu uniforme (SRT, ou ASS sans événement affichable).
        // Priorité au fichier externe (choix explicite de l'utilisateur : il a
        // chargé un SRT, c'est celui-là qu'il veut voir). À défaut, la piste
        // embarquée du conteneur.
        let (duree_min, duree_max) = (
            self.config.subtitles.min_duration_ms as u64,
            self.config.subtitles.max_duration_ms as u64,
        );
        self.subtitles.set_durees(duree_min, duree_max);
        let externe = self.subtitles.render_active(position);
        // Fenêtre d'affichage du sous-titre courant : sert au fondu.
        let mut bornes: Option<(u64, u64)> = None;
        let text = if !externe.trim().is_empty() {
            bornes = self.subtitles.active_bounds(position);
            externe
        } else if self.config.subtitles.use_embedded {
            // Piste **image** (PGS des Blu-ray, sous-image DVD) : les pixels de la
            // composition sont déposés tels quels, à l'échelle de l'image. Il n'y a
            // pas de texte à composer, et les réglages de police n'ont aucun sens
            // ici (la mise en forme vient du disque).
            if let Some(cue) = self.player.embedded_bitmap_at(position) {
                if !cue.rects.is_empty() {
                    let cue = cue.clone();
                    self.draw_bitmap_subtitle(ui, image, disp, &cue);
                    return;
                }
            }
            bornes = self.player.embedded_bounds_at(position, duree_min, duree_max);
            self.player
                .embedded_text_at(position, duree_min, duree_max)
                .unwrap_or_default()
        } else {
            String::new()
        };
        if text.trim().is_empty() {
            return;
        }
        let opacite = match bornes {
            Some((debut, fin)) => crate::subtitles::opacite_fondu(
                position,
                debut,
                fin,
                self.config.subtitles.fade_ms.min(5_000),
            ),
            None => 1.0,
        };

        // On se limite à la zone RÉELLEMENT visible (l'image peut déborder et
        // être rognée par le panneau).
        let rect = Self::image_rect(image, disp).intersect(ui.clip_rect());
        if rect.height() < 20.0 || rect.width() < 20.0 {
            return;
        }
        let painter = ui.painter();

        // --- Apparence : tous les paramètres viennent des préférences ---
        // Aucun n'est décoratif : chacun modifie un pixel du rendu, et les bancs d'essai
        // (tools/verif-soustitres.sh, verif-soustitres-style.sh) les vérifient un par un.
        let st = &self.config.subtitles;
        let font_size = st.font_size.clamp(8.0, 160.0);
        let couleur_texte = parse_couleur(&st.color).unwrap_or(egui::Color32::WHITE);
        let couleur_contour = parse_couleur(&st.outline_color).unwrap_or(egui::Color32::BLACK);
        let epaisseur = st.outline_width.clamp(0.0, 6.0);
        let epaisseur2 = st.outline2_width.clamp(0.0, 6.0);
        let couleur_contour2 = parse_couleur(&st.outline2_color).unwrap_or(egui::Color32::BLACK);
        let decalage_ombre = st.shadow_offset.clamp(0.0, 12.0);
        let couleur_ombre = parse_couleur(&st.shadow_color).unwrap_or(egui::Color32::BLACK);
        let alpha_ombre = st.shadow_alpha as f32 / 255.0;
        let alpha_fond = (st.background_alpha as f32 * opacite) as u8;
        let position_v = st.position_percent.clamp(0.0, 100.0) / 100.0;
        let alignement = st.align.min(2);
        // Position horizontale libre : 50 % = centre (comportement historique).
        // `align` dit où est ancré le bloc par rapport à ce point.
        let position_h = st.position_h_percent.clamp(0.0, 100.0) / 100.0;
        let famille = st.font_family.clone();
        // Interlettre et marges latérales (session 34). Les marges **bornent** la
        // zone : sans elles, une ligne longue débordait du cadre quand l'ancre était
        // près d'un bord. Elles restent plus petites que la largeur (une zone nulle
        // n'afficherait rien).
        let interlettre = st.letter_spacing.clamp(-5.0, 20.0);
        let marge_g = (st.margin_left_percent.clamp(0.0, 45.0) / 100.0) * rect.width();
        let marge_d = (st.margin_right_percent.clamp(0.0, 45.0) / 100.0) * rect.width();
        let zone = egui::Rect::from_min_max(
            egui::pos2(rect.left() + marge_g, rect.top()),
            egui::pos2((rect.right() - marge_d).max(rect.left() + marge_g + 20.0), rect.bottom()),
        );
        // Marges haute et basse (session 37) : elles bornent la zone verticale où
        // `position_percent` place le bloc, comme les marges latérales bornent l'horizontale.
        let marge_haut = (st.margin_top_percent.clamp(0.0, 45.0) / 100.0) * rect.height();
        let marge_bas = (st.margin_bottom_percent.clamp(0.0, 45.0) / 100.0) * rect.height();
        let zone_haut = rect.top() + marge_haut;
        let zone_bas = (rect.bottom() - marge_bas).max(zone_haut + 20.0);
        let interligne = st.line_spacing.clamp(0.8, 3.0);
        let echelle_x = st.scale_x.clamp(0.5, 2.0);
        let echelle_y = st.scale_y.clamp(0.5, 2.0);

        // Le gras s'obtient en épaississant le rendu : egui n'a pas de graisse
        // « bold » pour la police par défaut, on redessine donc le texte avec un
        // léger décalage horizontal, comme le fait un faux gras.
        let graisse = if st.bold { 0.7 } else { 0.0 };

        let lines: Vec<&str> = text.lines().collect();
        let line_height = font_size * interligne * echelle_y;
        let text_h = line_height * lines.len() as f32 + 12.0;
        // Position verticale : `position_v` = 0 en haut, 1 en bas de la zone.
        let haut_bande = zone_haut + (zone_bas - zone_haut - text_h) * position_v;
        let band = egui::Rect::from_min_max(
            egui::pos2(zone.left(), haut_bande),
            egui::pos2(zone.right(), haut_bande + text_h),
        );
        if alpha_fond > 0 {
            painter.rect_filled(
                band,
                egui::CornerRadius::same(4),
                egui::Color32::from_black_alpha(alpha_fond),
            );
        }
        let ctx = ui.ctx().clone();

        let mut y = band.top() + 6.0;
        for line in lines {
            // Les galleys sont posés avec la couleur « à remplir » (PLACEHOLDER) : c'est la
            // couleur donnée à `peindre_galley` qui s'applique. Avec une couleur figée dans le
            // galley, le contour était redessiné dans la couleur du TEXTE (défaut trouvé en
            // session 37 : le contour ne changeait jamais de couleur).
            // Interlettre : `layout_no_wrap` ne l'expose pas, on passe donc par un
            // `LayoutJob` d'un seul bloc de texte. À zéro (défaut), le rendu est
            // identique à avant.
            let galley = if interlettre.abs() > 0.01 {
                let mut job = egui::text::LayoutJob::default();
                job.append(
                    line,
                    0.0,
                    egui::TextFormat {
                        font_id: police_sous_titre(font_size, &famille),
                        color: egui::Color32::PLACEHOLDER,
                        extra_letter_spacing: interlettre,
                        ..Default::default()
                    },
                );
                painter.layout_job(job)
            } else {
                painter.layout_no_wrap(
                    line.to_string(),
                    police_sous_titre(font_size, &famille),
                    egui::Color32::PLACEHOLDER,
                )
            };
            let largeur = galley.size().x * echelle_x;
            // Ancre horizontale : le pourcentage choisi, puis le bloc est posé
            // selon `align` (début / centre / fin sur cette ancre). Avec la valeur
            // par défaut (50 %, centre) on retrouve exactement l'ancien calcul.
            // L'ancre est prise **dans la zone bornée par les marges**.
            let ancre = zone.left() + zone.width() * position_h;
            let x = match alignement {
                0 => ancre.clamp(zone.left(), (zone.right() - largeur).max(zone.left())),
                2 => (ancre - largeur).clamp(zone.left(), (zone.right() - largeur).max(zone.left())),
                _ => (ancre - largeur / 2.0)
                    .clamp(zone.left(), (zone.right() - largeur).max(zone.left())),
            };
            let pos = egui::pos2(x, y);
            let rayon_ext = epaisseur + epaisseur2;
            let peindre = |p: egui::Pos2, couleur: egui::Color32| {
                peindre_galley(&ctx, painter, p, &galley, couleur, echelle_x, echelle_y);
            };
            // Ombre : la silhouette (texte + contours) décalée vers le bas-droite.
            if decalage_ombre > 0.0 && alpha_ombre > 0.0 {
                let decale = pos + egui::vec2(decalage_ombre, decalage_ombre);
                let couleur = avec_alpha(couleur_ombre, alpha_ombre * opacite, true);
                for rayon in rayons_contour(rayon_ext) {
                    for (dx, dy) in DIRECTIONS_CONTOUR {
                        peindre(decale + egui::vec2(dx * rayon, dy * rayon), couleur);
                    }
                }
                peindre(decale, couleur);
            }
            // Contours, du plus large au plus proche du texte : second contour d'abord.
            if epaisseur2 > 0.0 {
                let couleur = avec_alpha(couleur_contour2, opacite, true);
                for rayon in rayons_contour(rayon_ext) {
                    for (dx, dy) in DIRECTIONS_CONTOUR {
                        peindre(pos + egui::vec2(dx * rayon, dy * rayon), couleur);
                    }
                }
            }
            if epaisseur > 0.0 {
                let couleur = avec_alpha(couleur_contour, opacite, true);
                for rayon in rayons_contour(epaisseur) {
                    for (dx, dy) in DIRECTIONS_CONTOUR {
                        peindre(pos + egui::vec2(dx * rayon, dy * rayon), couleur);
                    }
                }
            }
            let couleur = avec_alpha(couleur_texte, opacite, false);
            peindre(pos, couleur);
            if graisse > 0.0 {
                // Faux gras : second passage décalé (même technique que le contour).
                peindre(pos + egui::vec2(graisse, 0.0), couleur);
            }
            y += line_height;
        }
    }

    fn format_time(ms: u64) -> String { crate::utils::format_time(ms) }
    fn format_size(bytes: u64) -> String { crate::utils::format_size(bytes) }
    fn format_bitrate(bps: u64) -> String { crate::utils::format_bitrate(bps) }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Demande de plein écran différée : `--fullscreen` arrive trop tôt pour
        // pouvoir envoyer une commande au viewport, et la sortie du plein écran doit
        // être confirmée au gestionnaire de fenêtres (elle ne l'était pas : Échap
        // masquait l'interface mais laissait la fenêtre en plein écran).
        if let Some(enabled) = self.pending_fullscreen.take() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(enabled));
        }

        // Commandes de fenêtre demandées par un raccourci au tour précédent
        // (`pending_viewport_cmds`) : envoyées ici, hors de la fermeture
        // `ctx.input(...)` qui détient le verrou exclusif du contexte.
        if !self.pending_viewport_cmds.is_empty() {
            for cmd in std::mem::take(&mut self.pending_viewport_cmds) {
                ctx.send_viewport_cmd(cmd);
            }
        }

        // Avis HDR subi (Vulkan refusé : conversion par le CPU, ou image non convertie) :
        // montré une fois à l'écran. Sans cela, un film qui devient délavé après une mise
        // à jour du système n'a d'explication que dans le journal. Aucun `ctx.*` ici.
        if let Some(avis) = self.player.take_hdr_avis() {
            self.annonce(&avis);
        }

        // Pause de fin de boucle A-B : levée ici, car la lecture est en pause et le tour
        // d'interface normal ne s'exécute pas. Le réveil est demandé à l'échéance.
        self.player.tick_boucle_ab();
        if let Some(fin) = self.player.ab_attente() {
            ctx.request_repaint_after(fin.saturating_duration_since(std::time::Instant::now()));
        }

        // Taille et position de la fenêtre : mémorisées pour le prochain lancement.
        self.memoriser_geometrie(ctx);

        // Fichiers envoyés par un autre processus (« ouvrir avec » alors que le
        // lecteur tourne déjà) : ils se lisent ici, pas dans une seconde fenêtre.
        self.traiter_fichiers_recus(ctx);

        // Pause / reprise selon le focus de la fenêtre (réglage, désactivé par défaut).
        self.traiter_focus(ctx);

        // MPRIS : démarrage au premier tour (connexion D-Bus), puis publication de
        // l'état et exécution des demandes du bureau.
        if self.mpris.is_none() && !self.mpris_demande {
            self.mpris_demande = true;
            self.demarrer_mpris(ctx);
        }
        self.traiter_mpris();
        self.traiter_tray(ctx);

        // Vignettes de la barre de progression (module `vignettes`, fil de travail).
        self.traiter_vignettes(ctx);

        // En plein écran, on masque les panneaux pour n'avoir que la vidéo.

        // Enchaînement automatique : si le média est terminé et qu'un élément
        // suivant existe dans la playlist, on l'enchaîne.
        if self.player.is_finished() {
            // `index_apres_fin` suit tous les modes : lecture continue (coupée, rien ne suit),
            // aléatoire (l'ordre mélangé, un tour), répétition de la playlist (retour au début) ;
            // sans répétition, la fin de la liste arrête la lecture. Il passe par
            // `play_playlist_index`, le chemin de « suivant » (règles de profil, sous-titres, OSD).
            if let Some(idx) = self.player.index_apres_fin() {
                self.play_playlist_index(idx);
            }
        }

        // --- Cadence ---
        // L'application redessine en continu à la fréquence de l'écran pendant la
        // lecture ; on n'avance d'une frame que lorsque l'horloge média l'autorise.
        // Cela évite toute dérive cumulative (contrairement à request_repaint_after).
        let gen = self.player.generation();
        if gen != self.last_generation {
            self.last_generation = gen;
            self.pending_immediate = true;
            self.last_frame_pts_ms = 0;
        }

        // En lecture : une image à chaque échéance. **En pause : une seule image, et seulement
        // après un saut** (`pending_immediate` : barre de progression, ±5 s, signet, chapitre,
        // MPRIS, pas à pas). Sans cela le saut changeait la position mais l'écran gardait l'ancienne
        // image, et les répliques intégrées n'étaient pas relues (piège 122).
        let etat = self.player.state();
        let en_lecture = etat == PlayerState::Playing;
        if en_lecture || (etat == PlayerState::Paused && self.pending_immediate) {
            // Alimente le tampon audio et surveille l'horloge (carte son maître)
            // AVANT de décider si une image est due : sans cela, l'horloge audio
            // pourrait s'arrêter faute de données et figerait l'image avec elle.
            if en_lecture {
                self.player.tick_audio();
            }
            let due = self.pending_immediate
                || (en_lecture && self.player.next_frame_delay(self.last_frame_pts_ms).is_none());
            if due {
                let was_immediate = self.pending_immediate;
                if let Some(f) = self.player.pump_video_frame() {
                    let dims = (f.width, f.height);
                    let pts = f.pts_ms;
                    // Après un load/seek, on recale l'horloge sur la 1re frame réelle
                    // (supprime le décalage constant dû au délai de démarrage). Pas en pause :
                    // la reprise réancre elle-même l'horloge.
                    if was_immediate && en_lecture {
                        self.player.sync_clock_to(pts);
                    }
                    self.renderer.update_frame(f.data, dims.0, dims.1);
                    self.last_frame_pts_ms = pts;
                    self.frame_counter += 1;
                    if self.frame_counter % 60 == 0 {
                        tracing::debug!(
                            "frame #{} — PTS {} ms — dérive {} ms — horloge audio {} — audio buffer {} ms",
                            self.frame_counter,
                            pts,
                            self.player.drift_ms(pts),
                            self.player
                                .audio_clock_ms()
                                .map(|v| format!("{:.0} ms", v))
                                .unwrap_or_else(|| "—".into()),
                            self.player.audio_buffered_ms()
                        );
                    }
                }
                self.pending_immediate = false;
            }
        }

        if !self.dbg_police {
            self.dbg_police = true;
            ctx.fonts_mut(|f| {
                tracing::info!("DBG familles : {:?}", f.families());
                let id_mono = egui::FontId::monospace(14.0);
                let id_prop = egui::FontId::proportional(14.0);
                tracing::info!(
                    "DBG flèches monospace={:?} proportionnel={:?}",
                    f.has_glyphs(&id_mono, "←→↑↓●"),
                    f.has_glyphs(&id_prop, "←→↑↓●")
                );
            });
        }

        // Thème : externe s'il est choisi et lisible, sinon sombre ou clair.
        self.actualiser_theme_externe();
        match &self.theme_externe {
            Some((_, theme)) => ctx.set_visuals(theme.visuals()),
            None if self.config.interface.dark_theme => ctx.set_visuals(egui::Visuals::dark()),
            None => ctx.set_visuals(egui::Visuals::light()),
        }
        
        // Drag & drop de fichiers — egui 0.33: files are in raw.dropped_files, not an Event variant
        let dropped_paths: Vec<PathBuf> = ctx.input(|i| {
            i.raw.dropped_files.iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        
        if !dropped_paths.is_empty() {
            for file in dropped_paths {
                if self.load_media(&file) {
                    if !self.config.playlist.contains(&file) {
                        self.config.playlist.push(file.clone());
                        self.player.add_to_playlist(file);
                    }
                }
            }
            ctx.request_repaint();
        }
        
        // Enregistrement périodique de la position (reprise de lecture robuste)
        if self.player.state() == PlayerState::Playing {
            self.position_tick += ctx.input(|i| i.stable_dt);
            if self.position_tick >= 10.0 {
                self.position_tick = 0.0;
                self.player.persist_position();
            }
        } else {
            self.position_tick = 0.0;
        }

        // Timer OSD : n'éteint QUE le bandeau d'informations. Il fermait aussi la
        // fenêtre Statistiques, si bien qu'elle disparaissait toute seule après
        // quelques secondes alors que l'utilisateur venait de l'ouvrir.
        if self.osd_timer > 0.0 {
            self.osd_timer -= ctx.input(|i| i.stable_dt);
            if self.osd_timer <= 0.0 {
                self.show_osd_info = false;
            }
        }
        
        // Clic sur la barre de progression : on lit les événements bruts du pointeur.
        // egui ne signale un curseur que si le bouton est maintenu au moins une image ;
        // un clic très bref (pavé tactile, souris rapide) était donc ignoré.
        let (press, release) = ctx.input(|i| {
            let mut press = None;
            let mut release = None;
            for evt in &i.raw.events {
                if let egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    ..
                } = evt
                {
                    if *pressed {
                        press = Some(*pos);
                    } else {
                        release = Some(*pos);
                    }
                }
            }
            (press, release)
        });
        if let Some(p) = press {
            self.press_pos = Some(p);
        }
        if let Some(r) = release {
            let depart = self.press_pos.take();
            if let (Some(depart), Some(rect)) = (depart, self.progress_rect) {
                // Appui ET relâchement dans la barre : clic volontaire (pas un
                // relâchement de glissement, déjà traité par le curseur).
                if rect.contains(depart) && rect.contains(r) {
                    let ratio = ((r.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    let total = self.player.total_duration_ms();
                    if total > 0 {
                        let target = (ratio as f64 * total as f64) as u64;
                        let _ = self.player.seek(target);
                        tracing::info!("Progression: clic à {:.0} % → {} ms", ratio * 100.0, target);
                    }
                }
            }
        }

        // Raccourcis : la table (`raccourcis::Tableau`) décide QUELLE action est
        // demandée — y compris pour les raccourcis personnalisés — et le `match`
        // ci-dessous se contente de l'exécuter. Aucune touche n'est plus écrite en
        // dur ici : c'est ce qui rend les raccourcis modifiables et vérifiables.
        // --- Valider un réglage de recadrage en cours (Entrée) ---
        // Priorité sur le raccourci « plein écran » (Entrée en fait partie) :
        // pendant un réglage, l'utilisateur veut terminer, pas changer d'affichage.
        // On ne « return » pas : cela sauterait le rendu de l'image de cette image.
        // La touche est consommée en la retirant des actions ci-dessous.
        let entree_valide_recadrage = self.crop_reglage
            && ctx.input(|i| {
                i.events.iter().any(|e| {
                    matches!(
                        e,
                        egui::Event::Key {
                            key: egui::Key::Enter,
                            pressed: true,
                            ..
                        }
                    )
                })
            });
        if entree_valide_recadrage {
            self.valider_recadrage();
        }

        // --- Capture d'un raccourci (fenêtre des réglages) ---
        // Elle passe AVANT tout le reste : une touche pressée pendant la capture
        // ne doit déclencher aucune action, sinon on changerait de piste ou de
        // vitesse en voulant simplement choisir une touche.
        if let Some(action) = self.capture_raccourci {
            let capture = ctx.input(|i| {
                for evt in &i.raw.events {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } = evt
                    {
                        let combo = crate::raccourcis::Combinaison {
                            ctrl: modifiers.ctrl,
                            maj: modifiers.shift,
                            alt: modifiers.alt,
                            touche: crate::raccourcis::Touche::Clavier(*key),
                        };
                        return Some(combo);
                    }
                    // Gestes de souris : molette, clic du milieu (avec les modificateurs tenus).
                    if let Some(combo) = combinaison_souris(evt) {
                        return Some(combo);
                    }
                }
                // Le double-clic n'est pas un événement brut : egui le détecte.
                if i.pointer.button_double_clicked(egui::PointerButton::Primary) {
                    return Some(crate::raccourcis::Combinaison {
                        ctrl: i.modifiers.ctrl,
                        maj: i.modifiers.shift,
                        alt: i.modifiers.alt,
                        touche: crate::raccourcis::Touche::Souris(crate::raccourcis::Souris::DoubleClic),
                    });
                }
                None
            });
            if let Some(combo) = capture {
                self.capture_raccourci = None;
                if combo.touche == crate::raccourcis::Touche::Clavier(egui::Key::Escape) {
                    tracing::info!(target: "kinterface", "capture de raccourci annulée");
                } else if combo.touche == crate::raccourcis::Touche::Clavier(egui::Key::Backspace)
                    && !combo.ctrl
                    && !combo.maj
                    && !combo.alt
                {
                    crate::settings::definir_raccourci(self, action, "");
                } else {
                    crate::settings::definir_raccourci(self, action, &combo.texte());
                }
                // La touche capturée ne doit pas aussi déclencher son action.
                return;
            }
        }

        // Pendant la saisie d'un texte (adresse, nom de signet, langues…), les touches vont au champ :
        // `P` ne doit pas passer à l'élément précédent ni `F` basculer le plein écran. Seuls `Échap` et
        // les touches de fonction restent actifs.
        // **Pas `ctx.wants_keyboard_input()`** : il est vrai dès qu'un widget quelconque a le focus (un
        // bouton après un clic ou un `Tab`) et bloquerait alors tous les raccourcis — l'audit de
        // l'interface s'est figé là-dessus. On lit l'état relevé à la fin de l'image précédente.
        let saisie_texte = self.saisie_texte_active;
        let actions: Vec<crate::raccourcis::Action> = ctx.input(|i| {
            let mut v = Vec::new();
            for evt in &i.raw.events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = evt
                {
                    if saisie_texte && !touche_active_pendant_saisie(*key) {
                        continue;
                    }
                    // Journal des touches au niveau debug : indispensable pour
                    // diagnostiquer une touche qui n'arrive pas (raccourci global du
                    // bureau, fenêtre sans focus…).
                    tracing::debug!(target: "kinterface", "touche {:?}", key);
                    // Les modificateurs sont lus SUR L'ÉVÉNEMENT : l'état global
                    // (`i.modifiers`) peut déjà avoir changé si l'appui et le
                    // relâchement de Ctrl sont traités dans la même image, ce qui
                    // faisait perdre un raccourci sur deux (Ctrl+L, Ctrl+R).
                    // La touche qui vient de valider un recadrage est consommée :
                    // sans cela, Entrée déclencherait aussi le plein écran.
                    if entree_valide_recadrage && *key == egui::Key::Enter {
                        continue;
                    }
                    if let Some(action) = self.raccourcis.pour_clavier(
                        *key,
                        modifiers.ctrl,
                        modifiers.shift,
                        modifiers.alt,
                    ) {
                        v.push(action);
                    }
                }
            }
            v
        });
        for action in actions {
            {
                    match action {
                        Action::LecturePause => { let _ = self.player.toggle_pause(); },
                        // Différé (JAMAIS `ctx.*` ici : `ctx.input` détient le verrou
                        // exclusif du contexte — voir `pending_viewport_cmds`).
                        Action::Fermer => self.pending_viewport_cmds.push(egui::ViewportCommand::Close),
                        
                        // Trois actions distinctes (5 s, 1 s, 30 s) : elles portent
                        // trois raccourcis différents et sont donc personnalisables
                        // séparément. Auparavant, un seul `match` interne choisissait
                        // selon les modificateurs — impossible à remapper.
                        Action::Reculer5s => self.decaler(-5000),
                        Action::Reculer1s => self.decaler(-1000),
                        Action::Reculer30s => self.decaler(-30000),
                        Action::Avancer5s => self.decaler(5000),
                        Action::Avancer1s => self.decaler(1000),
                        Action::Avancer30s => self.decaler(30000),

                        Action::VolumePlus => self.changer_volume(0.05),
                        Action::VolumeMoins => self.changer_volume(-0.05),
                        Action::Muet => { self.player.toggle_mute(); },
                        
                        Action::PleinEcran => {
                            // Bascule plein écran : on pilote notre propre état (pour
                            // masquer l'interface) ET on demande au gestionnaire de
                            // fenêtres de passer en plein écran. F11 en plus de F/Entrée,
                            // car c'est le raccourci attendu des lecteurs et navigateurs.
                            self.set_fullscreen(!self.fullscreen);
                        }
                        Action::Echap => {
                            // Échap ferme d'abord la fenêtre ouverte (de la plus
                            // « en avant » à la plus discrète), puis sort du plein
                            // écran. Sans cela, « À propos » ou « Préférences »
                            // restaient affichées sans moyen de les fermer.
                            if !self.close_top_window() && self.fullscreen {
                                self.set_fullscreen(false);
                            }
                        }
                        
                        Action::Capture => { self.take_screenshot(); },
                        Action::SousTitresVisibles => {
                            let v = self.subtitles.toggle_visible();
                            self.player.subtitles_visible = v;
                            tracing::info!("Sous-titres visibles: {}", v);
                            self.annonce(if v {
                                "Sous-titres : affichés"
                            } else {
                                "Sous-titres : masqués"
                            });
                        },
                        Action::ElementSuivant => self.element_suivant(),
                        Action::ElementPrecedent => self.element_precedent(),

                        // Vitesse de lecture (comme mpv : [ ] et \ pour 1,0x).
                        // L'audio est rééchantillonné en même temps que la vidéo.
                        Action::VitesseMoins => {
                            let speed = self.player.get_speed() - 0.1;
                            self.player.set_speed(speed);
                            self.show_osd_info = true;
                            self.osd_timer = self.osd_duration_secs();
                        }
                        Action::VitessePlus => {
                            let speed = self.player.get_speed() + 0.1;
                            self.player.set_speed(speed);
                            self.show_osd_info = true;
                            self.osd_timer = self.osd_duration_secs();
                        }
                        Action::VitesseNormale => {
                            self.player.set_speed(1.0);
                            self.show_osd_info = true;
                            self.osd_timer = self.osd_duration_secs();
                        }
                        
                        // OSD : bascule (le bandeau est réellement dessiné, voir plus bas)
                        Action::Osd => {
                            self.show_osd_info = !self.show_osd_info;
                            self.osd_timer = if self.show_osd_info {
                                self.osd_duration_secs()
                            } else {
                                0.0
                            };
                        }
                        Action::Statistiques => {
                            self.show_statistics = !self.show_statistics;
                            self.osd_timer = 5.0;
                        },
                        // Lecture en boucle (Ctrl+L) — placé AVANT le motif `L` simple,
                        // sinon ce dernier capterait aussi Ctrl+L (premier motif gagnant).
                        Action::Boucle => self.basculer_boucle(),
                        Action::Repetition => self.repetition_suivante(),
                        Action::LectureContinue => self.basculer_continue(),
                        // Lecture aléatoire (Ctrl+A)
                        Action::Aleatoire => self.basculer_aleatoire(),
                        // Reprise à la position quittée (Ctrl+R)
                        Action::Reprise => {
                            self.config.playback.resume = !self.config.playback.resume;
                            self.player.set_resume(self.config.playback.resume);
                            let _ = self.config.save();
                            let actif = self.config.playback.resume;
                            self.annonce(if actif {
                                "Reprise de lecture : activée"
                            } else {
                                "Reprise de lecture : désactivée"
                            });
                        }
                        // Panneau playlist (auparavant inatteignable)
                        Action::PanneauPlaylist => self.basculer_panneau_playlist(),

                        // --- Bascule de fonctions ---
                        // Fenêtre toujours au-dessus des autres (via X11 : la
                        // commande d'eframe panique sur X11 en cas d'échec)
                        Action::ToujoursAuDessus => {
                            let cible = !self.always_on_top;
                            match crate::x11::set_always_on_top(cible) {
                                Ok(()) => {
                                    self.always_on_top = cible;
                                    self.annonce(if cible {
                                        "Fenêtre toujours au-dessus : activé"
                                    } else {
                                        "Fenêtre toujours au-dessus : désactivé"
                                    });
                                }
                                Err(e) => {
                                    tracing::warn!("Toujours au-dessus indisponible: {}", e);
                                    self.annonce(
                                        "Fenêtre toujours au-dessus : indisponible sur cet affichage",
                                    );
                                }
                            }
                        }
                        // Masquer / afficher toute l'interface
                        Action::MasquerInterface => {
                            self.ui_hidden = !self.ui_hidden;
                            tracing::info!("Interface masquée: {}", self.ui_hidden);
                            self.annonce(if self.ui_hidden {
                                "Interface masquée (Tab pour la revoir)"
                            } else {
                                "Interface affichée"
                            });
                        }
                        // Aide des raccourcis
                        Action::AideRaccourcis => {
                            self.show_shortcuts = !self.show_shortcuts;
                        }
                        // Fenêtre « À propos » (également accessible par le menu Préférences).
                        // F2 et non F12 : F12 est réservé par le bureau (Yakuake sous KDE),
                        // la touche n'arrive jamais jusqu'à l'application.
                        Action::APropos => {
                            self.show_about = !self.show_about;
                        }
                        // Répétition d'un passage (A-B)
                        Action::PointA => {
                            let message = self.player.toggle_loop_point_a();
                            self.annonce(&message);
                        }
                        Action::PointB => {
                            let message = self.player.toggle_loop_point_b();
                            self.annonce(&message);
                        }
                        Action::SignetAjouter => self.signet_ajouter(),
                        Action::SignetSuivant => self.signet_aller(false),
                        Action::SignetPrecedent => self.signet_aller(true),
                        Action::SignetSupprimer => self.signet_supprimer_ici(),
                        Action::SignetRenommer => self.signet_renommer_ici(),
                        // Piste de sous-titres suivante (fichiers externes)
                        Action::SousTitreSuivant => {
                            if self.subtitles.count() == 0 {
                                tracing::info!("Piste de sous-titres: aucune piste externe chargée");
                                self.annonce("Aucune piste de sous-titres externe");
                            } else {
                                self.subtitles.next_subtitle();
                                if let Some(f) = self.subtitles.active_path() {
                                    self.memoriser_piste_choisie(crate::pistes::Choix::Externe(f));
                                }
                                let nom = self.subtitles.active_name().unwrap_or_default();
                                tracing::info!("Piste de sous-titres: {}", nom);
                                self.annonce(&format!("Piste de sous-titres : {}", nom));
                            }
                        }
                        // Piste audio suivante : même chemin que le menu Audio.
                        Action::PisteAudioSuivante => self.piste_audio_suivante(),
                        Action::SortieAudioSuivante => self.sortie_audio_suivante(),
                        Action::Enceintes => self.definir_enceintes(self.player.speakers().suivante()),
                        // Chapitres (Page↑ / Page↓).
                        Action::ChapitrePrecedent => {
                            if self.player.previous_chapter().is_none() {
                                self.annonce("Aucun chapitre précédent");
                            }
                        }
                        Action::ChapitreSuivant => {
                            if self.player.next_chapter().is_none() {
                                self.annonce("Aucun chapitre suivant");
                            }
                        }
                        // Avance / recul d'une image, en pause (comme la touche « . » de
                        // VLC et les lecteurs de montage). On avance d'une durée de
                        // trame à la fois, puis on force l'affichage.
                        Action::ImageSuivante => {
                            let pas = self.player.frame_interval().as_millis() as u64;
                            let cible = self.player.current_position_ms() + pas.max(1);
                            let _ = self.player.seek(cible);
                            self.pending_immediate = true;
                            self.last_frame_pts_ms = 0;
                            self.annonce(&format!("Image suivante ({} ms)", cible));
                        }
                        Action::ImagePrecedente => {
                            let pas = self.player.frame_interval().as_millis() as u64;
                            let cible = self.player.current_position_ms().saturating_sub(pas.max(1));
                            let _ = self.player.seek(cible);
                            self.pending_immediate = true;
                            self.last_frame_pts_ms = 0;
                            self.annonce(&format!("Image précédente ({} ms)", cible));
                        }
                        // Rapport d'aspect : même fonction que la souris (cycle complet).
                        Action::Aspect => self.aspect_suivant(),
                        Action::Shaders => self.basculer_shaders(),
                        Action::AspectRogner => self.basculer_rogner(),
                        Action::ImageGlisserGauche => self.glisser_image(-PAS_GLISSEMENT, 0.0),
                        Action::ImageGlisserDroite => self.glisser_image(PAS_GLISSEMENT, 0.0),
                        Action::ImageGlisserHaut => self.glisser_image(0.0, -PAS_GLISSEMENT),
                        Action::ImageGlisserBas => self.glisser_image(0.0, PAS_GLISSEMENT),
                        Action::ImageCentrer => self.centrer_image(),

                        // Préférences (F5)
                        Action::Preferences => {
                            self.show_settings = !self.show_settings;
                            self.refresh_audio_devices();
                        },

                        // Bascule décodage matériel ↔ logiciel (comme « h » chez mpv).
                        // Le média courant est rechargé à la position courante, car le
                        // mode de décodage ne peut changer qu'à l'ouverture du flux.
                        Action::MiroirHorizontal => {
                            let etat = !self.config.video.flip_h;
                            self.config.video.flip_h = etat;
                            let _ = self.config.save();
                            self.appliquer_filtres();
                            self.annonce(if etat {
                                "Miroir horizontal : activé"
                            } else {
                                "Miroir horizontal : désactivé"
                            });
                        }
                        Action::MiroirVertical => {
                            let etat = !self.config.video.flip;
                            self.config.video.flip = etat;
                            let _ = self.config.save();
                            self.appliquer_filtres();
                            self.annonce(if etat {
                                "Miroir vertical : activé"
                            } else {
                                "Miroir vertical : désactivé"
                            });
                        }
                        // Une seule touche pour tout le cycle : entrer en réglage,
                        // puis valider. C'est ce qui manquait — on ne pouvait pas
                        // TERMINER un réglage au clavier.
                        Action::ZoneRecadrage => {
                            if self.crop_reglage {
                                self.valider_recadrage();
                            } else if self.config.video.crop_active() {
                                self.entrer_reglage_recadrage();
                            } else {
                                self.config.video.crop_overlay =
                                    !self.config.video.crop_overlay;
                                let _ = self.config.save();
                                self.annonce(if self.config.video.crop_overlay {
                                    "Repère de recadrage : affiché"
                                } else {
                                    "Repère de recadrage : masqué"
                                });
                            }
                        }
                        Action::DecodageMateriel => {
                            let enabled = !self.player.hardware_decoding_enabled();
                            self.apply_hardware_decoding(enabled);
                            self.show_osd_info = true;
                            self.osd_timer = self.osd_duration_secs();
                        },
                    }
            }
        }
        
        // Menu principal (barre de menu en haut) — masquée en plein écran ou si
        // l'interface a été masquée par l'utilisateur (Tab)
        if !self.fullscreen && !self.ui_hidden {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                journaliser_menu(ui.menu_button("📁 Fichier", |ui| {
                    if bouton_menu(ui, "📂 Ouvrir...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Vidéos", &["mp4", "mkv", "avi", "mov", "wmv", "flv", "webm"])
                            .add_filter("Tous les fichiers", &["*"])
                            .pick_file()
                        {
                            self.load_media(&path);
                            if !self.config.playlist.contains(&path) {
                                self.config.playlist.push(path.clone());
                                self.player.add_to_playlist(path);
                            }
                        }
                    }
                    
                    if bouton_menu(ui, "🌐 Ouvrir une adresse…").clicked() {
                        // Le champ est édité dans une petite fenêtre : `rfd` ne fait pas
                        // de saisie de texte, et une boîte de dialogue native serait
                        // disproportionnée pour une ligne.
                        self.show_url_dialog = true;
                    }
                    if bouton_menu(ui, "📁 Ouvrir dossier…").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            // Le dossier devient la playlist et la lecture démarre sur
                            // le premier fichier (auparavant : tous les fichiers étaient
                            // ouverts l'un après l'autre, seul le dernier restait).
                            let files = crate::utils::get_video_files_in_dir(&path);
                            if files.is_empty() {
                                self.error_message =
                                    Some(format!("Aucune vidéo dans {}", path.display()));
                            } else {
                                self.player.playlist = files.clone();
                                self.config.playlist = files.clone();
                                let _ = self.config.save();
                                self.show_playlist = true;
                                self.play_playlist_index(0);
                            }
                        }
                    }
                    
                    // Fichiers récents : le plus récent d'abord ; la liste est écrite par
                    // `after_media_loaded`. Le sous-menu est grisé tant qu'elle est vide.
                    let recents = self.config.playback.recent.clone();
                    let mut a_rouvrir: Option<PathBuf> = None;
                    let mut effacer_recents = false;
                    ui.add_enabled_ui(!recents.is_empty(), |ui| {
                        ui.menu_button("🕘 Fichiers récents", |ui| {
                            for chemin in &recents {
                                let nom = chemin
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| chemin.display().to_string());
                                if bouton_menu(ui, nom)
                                    .on_hover_text(chemin.display().to_string())
                                    .clicked()
                                {
                                    a_rouvrir = Some(chemin.clone());
                                }
                            }
                            ui.separator();
                            if bouton_menu(ui, "🗑 Effacer la liste").clicked() {
                                effacer_recents = true;
                            }
                        });
                    });
                    if let Some(chemin) = a_rouvrir {
                        self.load_media(&chemin);
                    }
                    if effacer_recents {
                        self.config.playback.recent.clear();
                        let _ = self.config.save();
                    }

                    ui.separator();
                    if bouton_menu(ui, "✕ Fermer").clicked() {
                        // Fermer, c'est fermer le lecteur : l'ancienne action ne
                        // libérait que le média et laissait la fenêtre ouverte,
                        // vide, sans rien annoncer.
                        self.pending_viewport_cmds.push(egui::ViewportCommand::Close);
                    }
                }));
                
                journaliser_menu(ui.menu_button("▶ Lecture", |ui| {
                    // `⏯` n'existe dans aucune des polices disponibles (vérifié dans
                    // les tables de glyphes) : on prend le symbole de l'action à venir,
                    // qui est dessiné dans toutes.
                    let (icone, action) = if self.player.state() == PlayerState::Playing {
                        ("⏸", "Pause")
                    } else {
                        ("▶", "Lecture")
                    };
                    if bouton_menu(ui, format!("{} {}", icone, action)).clicked() {
                        let _ = self.player.toggle_pause();
                        tracing::info!("Menu Lecture : {}", action);
                    }
                    if bouton_menu(ui, "⏹ Arrêter").clicked() {
                        let _ = self.player.stop();
                    }
                    ui.separator();
                    // Ordre : d'abord « où je vais » (élément, chapitre), puis les
                    // réglages de lecture (vitesse, boucle).
                    //
                    // Chapitres : navigation éditoriale du film.
                    let chapitres = self
                        .player
                        .media_info()
                        .map(|i| i.chapters.clone())
                        .unwrap_or_default();
                    if chapitres.is_empty() {
                        ui.label(egui::RichText::new("aucun chapitre").weak());
                    } else {
                        let courant = self.player.chapter_at(self.player.current_position_ms());
                        ui.label(
                            egui::RichText::new(format!(
                                "Chapitres ({}/{})",
                                courant.map(|i| i + 1).unwrap_or(0),
                                chapitres.len()
                            ))
                            .small()
                            .weak(),
                        );
                        if bouton_menu(ui, "⏮ Chapitre précédent (Page↑)").clicked() {
                            self.player.previous_chapter();
                        }
                        if bouton_menu(ui, "⏭ Chapitre suivant (Page↓)").clicked() {
                            self.player.next_chapter();
                        }
                        let mut cible: Option<u64> = None;
                        egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                            for (i, ch) in chapitres.iter().enumerate() {
                                let actif = courant == Some(i);
                                let titre = if ch.title.is_empty() {
                                    format!("Chapitre {}", i + 1)
                                } else {
                                    format!("{} {}", i + 1, ch.title)
                                };
                                let libelle = format!(
                                    "{} {}  {}",
                                    if actif { "●" } else { " " },
                                    Self::format_time(ch.start_ms),
                                    titre
                                );
                                if ligne_menu(ui, actif, libelle).clicked() && !actif {
                                    cible = Some(ch.start_ms);
                                }
                            }
                        });
                        if let Some(ms) = cible {
                            let _ = self.player.seek(ms);
                        }
                    }

                    ui.separator();
                    let media_signets = self.player.current_path().cloned();
                    let liste_signets: Vec<crate::signets::Signet> = media_signets
                        .as_ref()
                        .map(|m| self.signets.liste(m).to_vec())
                        .unwrap_or_default();
                    let mut aller_a: Option<u64> = None;
                    let mut supprimer: Option<usize> = None;
                    let mut renommer: Option<usize> = None;
                    let mut ajouter = false;
                    ui.menu_button("🔖 Signets", |ui| {
                        if bouton_menu(ui, "➕ Ajouter à la position (Ctrl+B)").clicked() {
                            ajouter = true;
                        }
                        ui.separator();
                        if liste_signets.is_empty() {
                            ui.label(egui::RichText::new("aucun signet pour ce film").weak());
                        }
                        for (i, signet) in liste_signets.iter().enumerate() {
                            ui.horizontal(|ui| {
                                if bouton_menu(ui, format!("{}  {}", crate::utils::format_time(signet.ms), signet.nom)).clicked() {
                                    aller_a = Some(signet.ms);
                                }
                                if bouton_menu(ui, "✎").clicked() {
                                    renommer = Some(i);
                                }
                                if bouton_menu(ui, "🗑").clicked() {
                                    supprimer = Some(i);
                                }
                            });
                        }
                    });
                    if ajouter {
                        self.signet_ajouter();
                    }
                    if let Some(ms) = aller_a {
                        let _ = self.player.seek(ms);
                    }
                    if let (Some(i), Some(m)) = (supprimer, media_signets.as_ref()) {
                        self.signets.supprimer(m, i);
                    }
                    if let (Some(i), Some(m)) = (renommer, media_signets.as_ref()) {
                        self.ouvrir_renommage(m.clone(), i);
                    }
                    if bouton_menu(ui, "⏭ Suivant (N)").clicked() {
                        self.element_suivant();
                    }
                    if bouton_menu(ui, "⏮ Précédent (P)").clicked() {
                        self.element_precedent();
                    }
                    
                    ui.separator();
                    // Le libellé de gauche portait « Vitesse: » et le slider un texte
                    // « 1.00x » déjà affiché par le nombre intégré : trois fois la même
                    // information. Un seul libellé, et des bornes identiques au reste.
                    ui.label(egui::RichText::new("Vitesse").small().weak());
                    let speed = self.player.get_speed();
                    let mut new_speed = speed;
                    if ui
                        .add(
                            egui::Slider::new(&mut new_speed, crate::player::VITESSE_MIN..=crate::player::VITESSE_MAX)
                                .suffix("x")
                                .fixed_decimals(2),
                        )
                        .changed()
                    {
                        self.player.set_speed(new_speed);
                    }
                    
                    ui.separator();
                    // Que faire à la fin d'un fichier : répétition, lecture continue, aléatoire.
                    ui.label(egui::RichText::new("À la fin d'un fichier").small().weak());
                    let courante = self.player.repeat();
                    for mode in crate::config::RepeatMode::TOUS {
                        let actif = courante == mode;
                        let libelle = format!("{}{}", if actif { "● " } else { "   " }, mode.libelle());
                        if ligne_menu(ui, actif, libelle).clicked() && !actif {
                            self.definir_repetition(mode);
                        }
                    }
                    let mut continue_ = self.player.continuous();
                    if case_menu(ui, &mut continue_, "Lecture continue (enchaîner)").clicked() {
                        self.basculer_continue();
                    }
                    let mut melange = self.player.shuffle();
                    if case_menu(ui, &mut melange, "🔀 Lecture aléatoire").clicked() {
                        self.basculer_aleatoire();
                    }
                }));
                
                journaliser_menu(ui.menu_button("🔊 Audio", |ui| {
                    // La borne haute est le volume MAXIMAL configuré, pas 150 % en
                    // dur : le réglage des préférences ne pouvait donc pas s'appliquer
                    // tant qu'on n'avait pas dépassé 150 %.
                    let volume = self.player.get_volume();
                    let max = self.config.audio.volume_max.clamp(100, 300) as f32 / 100.0;
                    let mut new_volume = volume;
                    if ui
                        .add(
                            egui::Slider::new(&mut new_volume, 0.0..=max)
                                .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
                        )
                        .changed()
                    {
                        self.player.set_volume(new_volume);
                    }

                    if bouton_menu(ui, if self.player.is_muted() {
                            "🔊 Rétablir le son"
                        } else {
                            "🔇 Couper le son"
                        })
                        .clicked()
                    {
                        self.player.toggle_mute();
                    }

                    // --- Sortie audio ---
                    // Les vraies sorties du système (enceinte, casque, écran), pas les noms ALSA
                    // bruts. Même chemin que les préférences et Ctrl+Maj+J.
                    ui.separator();
                    self.actualiser_sorties_audio(false);
                    ui.label(egui::RichText::new("Sortie audio").small().weak());
                    let actuelle = self.config.audio.device.clone();
                    let mut entrees: Vec<(String, String)> =
                        vec![(String::new(), "Périphérique par défaut".to_string())];
                    for s in &self.sorties_audio {
                        let libelle = if s.par_defaut {
                            format!("{} (défaut du système)", s.description)
                        } else {
                            s.description.clone()
                        };
                        entrees.push((s.identifiant(), libelle));
                    }
                    if !entrees.iter().any(|(id, _)| *id == actuelle) {
                        // Réglage hors liste (ancien nom ALSA, sortie éteinte) : on le montre.
                        entrees.push((actuelle.clone(), self.libelle_sortie_audio(&actuelle)));
                    }
                    let mut sortie_choisie: Option<String> = None;
                    for (id, libelle) in entrees {
                        let actif = id == actuelle;
                        let libelle = format!("{}{}", if actif { "● " } else { "   " }, libelle);
                        if ligne_menu(ui, actif, libelle).clicked() && !actif {
                            sortie_choisie = Some(id);
                        }
                    }
                    if let Some(id) = sortie_choisie {
                        self.choisir_sortie_audio(&id);
                    }

                    // --- Enceintes (nombre de canaux de la sortie) ---
                    let enceintes = self.config.audio.speakers;
                    let mut choix_enceintes = None;
                    ui.menu_button(format!("🔈 Enceintes : {}", enceintes.libelle()), |ui| {
                        for l in crate::config::SpeakerLayout::TOUS {
                            let actif = l == enceintes;
                            let libelle = format!("{}{}", if actif { "● " } else { "   " }, l.libelle());
                            if ligne_menu(ui, actif, libelle).clicked() && !actif {
                                choix_enceintes = Some(l);
                                ui.close();
                            }
                        }
                    });
                    if let Some(l) = choix_enceintes {
                        self.definir_enceintes(l);
                    }

                    // --- Pistes du conteneur ---
                    // Un MKV courant contient plusieurs langues : les exposer évite
                    // de devoir remuxer le fichier pour changer de version audio.
                    ui.separator();
                    let pistes: Vec<_> = self
                        .player
                        .media_info()
                        .map(|i| i.audio_tracks.clone())
                        .unwrap_or_default();
                    if pistes.is_empty() {
                        ui.label(egui::RichText::new("aucune piste audio").weak());
                    } else {
                        let courante = self.player.current_audio_stream();
                        let mut choisie = None;
                        ui.label(egui::RichText::new("Piste audio").small().weak());
                        for (i, piste) in pistes.iter().enumerate() {
                            let actif = courante == Some(piste.index);
                            let libelle = format!(
                                "{}{}",
                                if actif { "● " } else { "   " },
                                piste.label()
                            );
                            let rep = ligne_menu(ui, actif, libelle);
                            // Géométrie journalisée (debug) : l'audit clique la piste
                            // sans deviner les pixels (même règle que pour les
                            // fenêtres de réglages).
                            tracing::debug!(
                                target: "kinterface",
                                "menu audio piste {} rect {:?} actif={}",
                                i,
                                rep.rect,
                                actif
                            );
                            if rep.clicked() && !actif {
                                choisie = Some((i, piste.index));
                            }
                        }
                        if let Some((rang, index)) = choisie {
                            match self.player.select_audio_stream(index) {
                                Ok(()) => {
                                    self.player.memoriser_piste_audio(index);
                                    tracing::info!(
                                        target: "kinterface",
                                        "piste audio choisie : flux {} — {}",
                                        index,
                                        pistes[rang].label()
                                    );
                                    self.annonce(&format!("Audio : {}", pistes[rang].label()));
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        target: "kinterface",
                                        "changement de piste audio refusé : {}",
                                        e
                                    );
                                    self.annonce(&format!("Piste audio indisponible : {}", e));
                                }
                            }
                        }
                    }

                }));
                
                journaliser_menu(ui.menu_button("📝 Sous-titres", |ui| {
                    // Ordre : ce qui s'affiche MAINTENANT (visibilité, piste), puis ce
                    // qui vient du fichier, puis les fichiers externes, puis le
                    // chargement. L'ancien ordre commençait par une option de
                    // comportement (« Chargement automatique »), au-dessus de
                    // l'affichage dont on a besoin à l'instant.
                    let mut visible = self.subtitles.is_visible();
                    if case_menu(ui, &mut visible, "Afficher les sous-titres").clicked() {
                        self.subtitles.set_visible(visible);
                        self.player.subtitles_visible = visible;
                        tracing::info!("Sous-titres : affichage {}", if visible { "activé" } else { "désactivé" });
                        self.annonce(if visible {
                            "Sous-titres affichés"
                        } else {
                            "Sous-titres masqués"
                        });
                    }

                    if case_menu(ui, &mut self.config.subtitles.use_embedded, "Utiliser les sous-titres du fichier")
                        .on_hover_text("Pistes MKV/MP4 ; un fichier externe reste prioritaire")
                        .changed()
                    {
                        let _ = self.config.save();
                        self.player.set_track_preferences(
                            &self.config.audio.langues_preferees(),
                            &self.config.subtitles.langues_preferees(),
                            self.config.subtitles.use_embedded,
                        );
                        if !self.config.subtitles.use_embedded {
                            let _ = self.player.select_subtitle_stream(-1);
                        }
                    }

                    ui.separator();
                    let embarquees: Vec<_> = self
                        .player
                        .media_info()
                        .map(|i| i.subtitle_tracks.clone())
                        .unwrap_or_default();
                    if embarquees.is_empty() {
                        ui.label(egui::RichText::new("aucune piste dans le fichier").weak());
                    } else {
                        let courante = self.player.current_subtitle_stream();
                        let mut choisie: Option<i32> = None;
                        ui.label(egui::RichText::new("Pistes du fichier").small().weak());
                        if ui
                            .selectable_label(courante.is_none(), "   désactivées")
                            .clicked()
                        {
                            choisie = Some(-1);
                        }
                        for piste in embarquees.iter() {
                            let actif = courante == Some(piste.index);
                            let mut libelle = format!(
                                "{}{}",
                                if actif { "● " } else { "   " },
                                piste.label()
                            );
                            if piste.bitmap {
                                // Rendu depuis la session 23 : le libellé disait
                                // encore « non affichable ».
                                libelle.push_str("  (image)");
                            }
                            if ligne_menu(ui, actif, libelle).clicked() && !actif {
                                choisie = Some(piste.index);
                            }
                        }
                        if let Some(index) = choisie {
                            match self.player.select_subtitle_stream(index) {
                                Ok(()) => {
                                    if index >= 0 {
                                        self.memoriser_piste_choisie(crate::pistes::Choix::Integree(index));
                                    }
                                    self.subtitles.set_visible(index >= 0);
                                    self.player.subtitles_visible = index >= 0;
                                    self.annonce(if index >= 0 {
                                        "Sous-titres du fichier activés"
                                    } else {
                                        "Sous-titres du fichier désactivés"
                                    });
                                }
                                Err(e) => {
                                    tracing::warn!("Piste de sous-titres impossible : {}", e);
                                    self.annonce(&format!("Piste indisponible : {}", e));
                                }
                            }
                        }
                    }

                    ui.separator();
                    // Chargement automatique : un comportement, donc en fin de menu.
                    let mut auto = self.config.subtitles.auto_load;
                    if case_menu(ui, &mut auto, "Charger les sous-titres au démarrage")
                        .on_hover_text("Fichier homonyme ou suffixé de la langue (.fr., .eng.)")
                        .changed()
                    {
                        self.config.subtitles.auto_load = auto;
                        let _ = self.config.save();
                        tracing::info!(
                            "Sous-titres : chargement automatique {}",
                            if auto { "activé" } else { "désactivé" }
                        );
                    }

                    ui.separator();
                    let n = self.subtitles.count();
                    if n == 0 {
                        ui.label(egui::RichText::new("aucun fichier externe chargé").weak());
                    } else {
                        ui.label(format!("{} piste(s) chargée(s)", n));
                        if let Some(name) = self.subtitles.active_name() {
                            let langue = self.subtitles.active_language().unwrap_or_default();
                            if langue.is_empty() || langue == "und" {
                                ui.label(format!("Active : {}", name));
                            } else {
                                ui.label(format!("Active : {} [{}]", name, langue));
                            }
                        }
                        if bouton_menu(ui, "🔍 Relire les sous-titres du dossier").clicked() {
                            if let Some(path) = self.player.current_path().cloned() {
                                self.player.subtitles_visible = false;
                                self.subtitles = SubtitleManager::new();
                                self.subtitles.set_encodage(&self.config.subtitles.encoding);
                                self.subtitles.auto_load(&path, &self.config.subtitles.langues_preferees());
                                self.subtitles.set_visible(true);
                                self.player.subtitles_visible = true;
                            }
                        }
                        if bouton_menu(ui, "⏭ Piste suivante").clicked() {
                            self.subtitles.next_subtitle();
                            if let Some(f) = self.subtitles.active_path() {
                                self.memoriser_piste_choisie(crate::pistes::Choix::Externe(f));
                            }
                        }
                    }
                    if bouton_menu(ui, "📂 Charger un fichier…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Sous-titres", &["srt", "ass", "ssa", "sub"])
                            .pick_file()
                        {
                            self.subtitles.set_encodage(&self.config.subtitles.encoding);
                            match self.subtitles.load_subtitle(path) {
                                Ok(()) => self.subtitles.set_visible(true),
                                Err(e) => tracing::warn!("Sous-titre: {}", e),
                            }
                        }
                    }
                }));
                
                journaliser_menu(ui.menu_button("📋 Playlist", |ui| {
                    let n = self.player.playlist.len();
                    ui.label(
                        egui::RichText::new(if n == 0 {
                            "playlist vide".to_string()
                        } else {
                            format!(
                                "{} élément(s) — {}/{}",
                                n,
                                self.player.playlist_index().min(n - 1) + 1,
                                n
                            )
                        })
                        .small()
                        .weak(),
                    );
                    if bouton_menu(ui, "Afficher / masquer le panneau (L)").clicked() {
                        self.basculer_panneau_playlist();
                    }
                    if bouton_menu(ui, "Vider").clicked() {
                        self.player.playlist.clear();
                        self.config.playlist.clear();
                        let _ = self.config.save();
                    }
                }));

                journaliser_menu(ui.menu_button("⚙ Préférences", |ui| {
                    if bouton_menu(ui, "🎨 Réglages… (F5)").clicked() {
                        self.show_settings = true;
                        tracing::info!("Fenêtre Préférences ouverte (menu)");
                    }
                    if bouton_menu(ui, if self.show_statistics {
                            "📊 Masquer les statistiques"
                        } else {
                            "📊 Statistiques (I)"
                        })
                        .clicked()
                    {
                        self.show_statistics = !self.show_statistics;
                        self.osd_timer = 5.0;
                        tracing::info!(
                            "Fenêtre Statistiques {} (menu)",
                            if self.show_statistics { "ouverte" } else { "fermée" }
                        );
                    }
                    if bouton_menu(ui, "ℹ À propos (F2)").clicked() {
                        self.show_about = true;
                        tracing::info!("Fenêtre À propos ouverte (menu)");
                    }
                }));
                
                // Spacer pour centrer le titre
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.config.interface.show_title {
                        if let Some(info) = self.player.media_info() {
                            // Le nom du média, et le chapitre courant s'il y en a :
                            // c'est l'information la plus utile quand on saute de
                            // chapitre en chapitre, et elle n'était affichée nulle part
                            // en permanence (seulement dans l'OSD temporaire).
                            let mut titre = format!("🎬 {}", info.filename);
                            let chapitres = info.chapters.len();
                            if chapitres > 0 {
                                let courant = self
                                    .player
                                    .chapter_at(self.player.current_position_ms())
                                    .map(|i| i + 1)
                                    .unwrap_or(0);
                                titre.push_str(&format!(
                                    "  ·  chapitre {}/{}",
                                    courant, chapitres
                                ));
                            }
                            ui.label(titre);
                        } else {
                            ui.label("🎬 RMPlayer");
                        }
                    }
                });
            });
        });
        } // fin du bloc masqué en plein écran

        // Zone centrale (rendu vidéo)
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.player.state() == PlayerState::Idle {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("🎬 RMPlayer").heading());
                        ui.label("Lecteur vidéo professionnel Linux");
                        ui.label("");
                        ui.label("💡 Glissez-déposez un fichier vidéo ici");
                        ui.label("ou utilisez le menu Fichier → Ouvrir");
                        
                        if ui.button("📂 Ouvrir un fichier").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Vidéos", &["mp4", "mkv", "avi", "mov", "wmv", "flv", "webm"])
                                .add_filter("Tous les fichiers", &["*"])
                                .pick_file()
                            {
                                self.load_media(&path);
                                if !self.config.playlist.contains(&path) {
                                    self.config.playlist.push(path.clone());
                                    self.player.add_to_playlist(path);
                                }
                            }
                        }
                    });
                });
            } else {
                if let Some(frame_data) = self.renderer.get_frame() {
                    let (width, height) = self.renderer.get_dimensions();
                    if width > 0 && height > 0 {
                        let expected = (width as usize) * (height as usize) * 4;
                        if frame_data.len() == expected {
                            let image = egui::ColorImage::from_rgba_unmultiplied(
                                [width as usize, height as usize],
                                &frame_data,
                            );

                            // Réutilise la texture GPU si les dimensions n'ont pas changé
                            let tex_size = [width as usize, height as usize];
                            let handle = match &mut self.video_texture {
                                Some(h) if h.size() == tex_size => {
                                    h.set(image, egui::TextureOptions::default());
                                    h.clone()
                                }
                                _ => {
                                    let h = ctx.load_texture(
                                        "video_frame",
                                        image,
                                        egui::TextureOptions::default(),
                                    );
                                    self.video_texture = Some(h.clone());
                                    h
                                }
                            };

                            // Mise à l'échelle pour occuper l'espace disponible en
                            // conservant le rapport d'aspect (comme un lecteur vidéo).
                            // La référence est la résolution NATIVE du média : le
                            // tampon de frame peut être déjà réduit à la taille
                            // d'affichage (voir `scale_frame_to_rgba`), et partir de
                            // ses dimensions déformerait l'image.
                            let native = self
                                .player
                                .media_info()
                                .map(|i| i.resolution)
                                .filter(|(w, h)| *w > 0 && *h > 0)
                                .unwrap_or((width, height));
                            // Rapport d'aspect imposé : il Remplace celui du média pour
                            // le calcul des dimensions affichées. C'est une déformation
                            // VOLONTAIRE (corriger un encodage mal étiqueté, remplir
                            // l'écran) — le réglage par défaut ne déforme rien.
                            let native = match self.config.video.aspect {
                                crate::config::AspectMode::Auto => native,
                                crate::config::AspectMode::Ratio4x3 => (4, 3),
                                crate::config::AspectMode::Ratio16x9 => (16, 9),
                                crate::config::AspectMode::Ratio185 => (185, 100),
                                crate::config::AspectMode::Ratio235 => (235, 100),
                                crate::config::AspectMode::Personnalise => {
                                    let r = self.config.video.custom_aspect.clamp(0.25, 4.0);
                                    ((r * 1000.0) as u32, 1000)
                                }
                                // « Remplir » : on prend la forme de la zone disponible,
                                // calculée juste en dessous.
                                crate::config::AspectMode::Remplir => (0, 0),
                                // « Rogner » : rapport du média, mais agrandi jusqu'à couvrir
                                // la zone (`disp` dépasse alors la zone et le panneau rogne).
                                crate::config::AspectMode::Rogner => native,
                            };
                            let rogner = self.config.video.aspect == crate::config::AspectMode::Rogner;
                            ui.centered_and_justified(|ui| {
                                let avail = ui.available_size();
                                let disp = if native.0 == 0 || native.1 == 0 {
                                    // Mode « remplir » : toute la zone, sans rapport.
                                    avail
                                } else {
                                    let (sx, sy) = (avail.x / native.0 as f32, avail.y / native.1 as f32);
                                    let scale = if rogner { sx.max(sy) } else { sx.min(sy) }.max(0.01);
                                    egui::vec2(native.0 as f32 * scale, native.1 as f32 * scale)
                                };

                                // La conversion couleur se fera directement à la
                                // taille affichée (voir `scale_frame_to_rgba`) :
                                // inutile de fabriquer une image 4K que le GPU
                                // devrait réduire ensuite. En pixels physiques.
                                let ppp = ui.ctx().pixels_per_point();
                                self.player.set_display_target(
                                    (disp.x * ppp) as u32,
                                    (disp.y * ppp) as u32,
                                );

                                let sized_texture =
                                    egui::load::SizedTexture::new(handle.id(), disp);
                                // `Sense::click()` est INDISPENSABLE : sans lui, l'image
                                // n'est qu'un décor survolé, et ni le clic droit (menu
                                // contextuel), ni le double-clic, ni le clic du milieu ne
                                // parviennent à l'application (défaut constaté : le menu
                                // contextuel ne s'ouvrait jamais).
                                // Glisser : l'image est décalée d'un pourcentage de sa propre
                                // taille ; la fenêtre rogne ce qui dépasse. Les sous-titres et le
                                // repère de recadrage suivent (ils se calculent sur `response.rect`).
                                let decalage = egui::vec2(
                                    disp.x * self.config.video.pan_x_percent.clamp(-50.0, 50.0) / 100.0,
                                    disp.y * self.config.video.pan_y_percent.clamp(-50.0, 50.0) / 100.0,
                                );
                                let zone_image = egui::Rect::from_center_size(
                                    ui.max_rect().center() + decalage,
                                    disp,
                                );
                                let response = ui.put(
                                    zone_image,
                                    egui::Image::new(sized_texture).sense(egui::Sense::click()),
                                );

                                // --- Souris : raccourcis personnalisables ---
                                // Les actions passent par la MÊME table que le
                                // clavier : une molette ou un double-clic peuvent
                                // déclencher n'importe quelle action.
                                let souris = self.souris_utilisee(ui, &response);
                                for action in souris {
                                    self.executer_action_souris(action);
                                }

                                // --- Clic droit : menu contextuel des réglages ---
                                // (le clic droit est réservé à ce menu ; il n'est donc
                                // pas proposé comme raccourci dans les réglages)
                                self.menu_contextuel(&response);

                                // --- Repère visuel du recadrage ---
                                self.draw_crop_overlay(ui, &response, disp);

                                // --- Surimpression des sous-titres ---
                                self.draw_subtitles(ui, &response, disp);
                            });
                        }
                    }
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label("🎬 Rendu vidéo — en cours...");
                    });
                }
            }
        });
        
        // Barre de contrôles en bas
        if self.config.interface.show_progress_bar
            && !self.ui_hidden
            && (!self.fullscreen || self.config.interface.controls_always_visible)
        {
            egui::TopBottomPanel::bottom("controls")
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal_centered(|ui| {
                        let state = self.player.state();
                        if ui.button(if state == PlayerState::Playing { "⏸" } else { "▶" }).clicked() {
                            let _ = self.player.toggle_pause();
                        }
                        
                        if ui.button("⏹").clicked() {
                            let _ = self.player.stop();
                        }
                        
                        if ui.button("⏮").clicked() {
                            self.element_precedent();
                        }
                        if ui.button("⏭").clicked() {
                            self.element_suivant();
                        }
                        
                        ui.separator();
                        
                        // Barre de progression
                        let current_ms = self.player.current_position_ms();
                        let total_ms = self.player.total_duration_ms();
                        
                        // Pendant un glissement, la valeur affichée est celle choisie par
                        // l'utilisateur (et non la position de lecture, qui la ramènerait
                        // en arrière à chaque image).
                        let mut percentage = match self.seek_preview {
                            Some(preview) => preview,
                            None if total_ms > 0 => {
                                (current_ms as f32 / total_ms as f32).clamp(0.0, 1.0)
                            }
                            None => 0.0,
                        };

                        let response = ui.add(
                            egui::Slider::new(&mut percentage, 0.0..=1.0)
                                .show_value(false)
                                .trailing_fill(true),
                        );
                        self.progress_rect = Some(response.rect);
                        if response.hover_pos().is_none() {
                            self.vignette_cible = None;
                        }
                        // Géométrie exposée : le banc survole **la barre**, sans
                        // deviner sa position (leçon des sessions 11 et 34).
                        tracing::debug!(
                            target: "kinterface",
                            "progression rect {:?} duree {}",
                            response.rect,
                            total_ms
                        );

                        // Vignette au survol : on note la position visée, le tour
                        // suivant la demande au fil de travail (jamais de décodage
                        // dans ce tour-ci).
                        if total_ms > 0 {
                            // Diagnostic : le banc vérifie que le survol a bien
                            // demandé une vignette et qu'une vignette est affichée
                            // (sans cela, « ça ne marche pas » ne se distingue pas de
                            // « je n'ai rien demandé »).
                            if let Some(g) = self.vignettes.as_ref() {
                                let (envoyees, recues) = g.compteurs();
                                tracing::debug!(
                                    target: "kinterface",
                                    "vignettes : {} demande(s), {} reçue(s), affichée={}",
                                    envoyees,
                                    recues,
                                    self.vignette_affichee.is_some()
                                );
                            }
                            if let Some(pointeur) = response.hover_pos() {
                                let part = ((pointeur.x - response.rect.left())
                                    / response.rect.width().max(1.0))
                                    .clamp(0.0, 1.0);
                                let vise = (part as f64 * total_ms as f64) as u64;
                                // On ne change la cible que si la tranche change : cela
                                // évite de « réveiller » le générateur à chaque pixel
                                // parcouru pendant que la souris glisse sur la barre.
                                if vise / crate::vignettes::TRANCHE_MS
                                    != self.vignette_cible.unwrap_or(u64::MAX)
                                        / crate::vignettes::TRANCHE_MS
                                {
                                    self.vignette_cible = Some(vise);
                                }
                                // Délai affiché : la position sous le curseur, pour
                                // savoir où l'on va même avant que l'image arrive.
                                let curseur = format!(
                                    "{}",
                                    Self::format_time(self.vignette_cible.unwrap_or(0))
                                );
                                // `Tooltip::always_open` (l'API non dépréciée) : la
                                // bulle suit le pointeur et reste ouverte tant qu'il
                                // survole la barre — c'est exactement un aperçu, pas une
                                // info-bulle d'aide.
                                egui::Tooltip::always_open(
                                    ctx.clone(),
                                    ui.layer_id(),
                                    egui::Id::new("vignette-progression-bulle"),
                                    egui::PopupAnchor::Pointer,
                                )
                                .show(|ui| {
                                        if let Some((tranche, texture)) = &self.vignette_affichee
                                        {
                                            let visee = self.vignette_cible.unwrap_or(0)
                                                / crate::vignettes::TRANCHE_MS;
                                            let fraiche = *tranche == visee;
                                            ui.add(egui::Image::new((texture.id(), texture.size_vec2()))
                                                .max_width(320.0));
                                            if !fraiche {
                                                ui.label(
                                                    egui::RichText::new("…")
                                                        .small()
                                                        .weak(),
                                                );
                                            }
                                        }
                                        ui.label(egui::RichText::new(curseur).small());
                                });
                            }
                        }

                        // La valeur du curseur était auparavant purement décorative :
                        // rien ne la lisait. Elle déclenche maintenant une vraie
                        // recherche, en direct pendant le glissement.
                        // Interaction en cours : bouton maintenu ou curseur déplacé.
                        // egui ne signale pas toujours `clicked()`/`drag_stopped()` pour
                        // un simple clic sur la piste : c'est l'état du pointeur qui fait
                        // foi, sinon l'aperçu restait bloqué et la barre semblait inerte.
                        let interaction = response.dragged() || response.is_pointer_button_down_on();

                        if response.changed() && total_ms > 0 {
                            self.seek_preview = Some(percentage);
                            let target = (percentage as f64 * total_ms as f64) as u64;
                            // Un seek est coûteux (flush + repositionnement des décodeurs) :
                            // on le limite à environ 6 par seconde pendant le glissement.
                            let assez_recent = self
                                .last_seek_at
                                .map(|t| t.elapsed().as_millis() >= 150)
                                .unwrap_or(true);
                            if assez_recent {
                                let _ = self.player.seek(target);
                                self.last_seek_at = Some(std::time::Instant::now());
                            }
                        }

                        if !interaction {
                            // L'utilisateur a lâché : on applique la position choisie et on
                            // rend la barre à la position réelle de lecture.
                            if let Some(preview) = self.seek_preview.take() {
                                if total_ms > 0 {
                                    let target = (preview as f64 * total_ms as f64) as u64;
                                    let _ = self.player.seek(target);
                                    tracing::info!(
                                        "Progression: seek à {} ms ({:.0} %)",
                                        target,
                                        preview * 100.0
                                    );
                                }
                            }
                            self.last_seek_at = None;
                        }
                        
                        // Temps actuel / durée totale
                        ui.label(format!(
                            "{} / {}",
                            Self::format_time(current_ms),
                            Self::format_time(total_ms)
                        ));
                        
                        ui.separator();
                        
                        // Volume
                        let volume = self.player.get_volume();
                        let mut new_volume = volume;
                        ui.add(egui::Slider::new(&mut new_volume, 0.0..=1.5)
                            .text(&format!("{:.0}%", volume * 100.0)));
                        if new_volume != volume {
                            self.player.set_volume(new_volume);
                        }
                        
                        if ui.button("🔊").clicked() {
                            self.player.toggle_mute();
                        }
                        
                        ui.separator();
                        
                        // Vitesse
                        let speed = self.player.get_speed();
                        if ui.button(&format!("{:.2}x", speed)).clicked() {
                            self.player.set_speed(1.0);
                        }
                        
                        // Bouton plein écran (même chemin que F11 / F / Échap)
                        let is_fullscreen = ctx.viewport(|vp| vp.builder.fullscreen == Some(true)) || self.fullscreen;
                        if ui
                            .button("⛶")
                            .on_hover_text("Plein écran (F11)")
                            .clicked()
                        {
                            self.set_fullscreen(!is_fullscreen);
                        }
                    });
                });
        }
        
        // Playlist panel (latéral droit)
        if self.show_playlist && !self.ui_hidden {
            // **Largeur bornée.** Sans `max_width`, un `SidePanel` se dimensionne sur
            // son contenu : un nom de fichier long élargissait le panneau jusqu'à
            // occuper la moitié de la fenêtre (constaté à l'écran, session 34) — et
            // le nom n'était même pas tronqué, il débordait. La largeur par défaut
            // reste modeste et l'utilisateur peut la tirer dans des bornes utiles.
            let panneau = egui::SidePanel::right("playlist")
                .default_width(300.0)
                .min_width(200.0)
                .max_width(420.0)
                .resizable(true)
                .show(ctx, |ui| {
                    let total = self.player.playlist.len();
                    let current = self.player.playlist_index();
                    ui.horizontal(|ui| {
                        ui.heading("Playlist");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}/{}",
                                    if total == 0 { 0 } else { current.min(total - 1) + 1 },
                                    total
                                ))
                                .small()
                                .weak(),
                            );
                        });
                    });

                    let mut to_play: Option<usize> = None;
                    let mut to_remove: Option<usize> = None;

                    // Ligne de playlist : dessinée à la main, pour trois raisons
                    // mesurées à l'écran (session 34).
                    //
                    // 1. `ui.dnd_drag_source` enregistre son interaction APRÈS le
                    //    contenu : elle est donc au-dessus, et elle **avalait le
                    //    clic** — cliquer une ligne ne la lisait plus, et le ✕ ne
                    //    répondait plus. Constaté en cliquant : 0 lecture, 0 retrait.
                    // 2. `ui.dnd_drop_zone` peint le cadre du style inactif : chaque
                    //    ligne devenait une boîte grise, plus une barre « fin de la
                    //    liste » permanente. C'est ce que l'utilisateur a vu comme
                    //    « buggé ».
                    // 3. Un nom long n'était pas tronqué : il poussait le ✕ hors du
                    //    volet et élargissait le panneau.
                    //
                    // Ici : une seule interaction `click_and_drag` par ligne (clic =
                    // lire, glisser = déplacer), le texte est tronqué par le moteur de
                    // texte d'egui, et le ✕ est enregistré APRÈS la ligne — donc
                    // au-dessus d'elle dans sa petite zone.
                    let glisse = egui::DragAndDrop::has_payload_of_type::<usize>(ctx);
                    let relache = ctx.input(|i| i.pointer.any_released());
                    let mut cible: Option<usize> = None;
                    let mut cible_fin = false;
                    let mut rects: Vec<egui::Rect> = Vec::with_capacity(total);
                    let hauteur_ligne = ui.text_style_height(&egui::TextStyle::Body) + 6.0;

                    for i in 0..total {
                        let is_current = i == current;
                        let nom = self.player.playlist[i]
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "?".to_string());
                        let largeur = ui.available_width();
                        let (rect, reponse) = ui.allocate_exact_size(
                            egui::vec2(largeur, hauteur_ligne),
                            egui::Sense::click_and_drag(),
                        );
                        rects.push(rect);

                        // Fond : sélection, survol, sinon rien.
                        let fond = if is_current {
                            Some(ui.visuals().selection.bg_fill)
                        } else if reponse.hovered() {
                            Some(ui.visuals().widgets.hovered.weak_bg_fill)
                        } else {
                            None
                        };
                        if let Some(fond) = fond {
                            ui.painter().rect_filled(rect, 3.0, fond);
                        }

                        // Texte tronqué (une seule ligne, « … » à la coupure).
                        let couleur = if is_current {
                            ui.visuals().strong_text_color()
                        } else {
                            ui.visuals().text_color()
                        };
                        let mut tache = egui::text::LayoutJob::default();
                        tache.append(
                            &if is_current {
                                format!("▶ {}", nom)
                            } else {
                                format!("   {}", nom)
                            },
                            0.0,
                            egui::TextFormat {
                                font_id: egui::TextStyle::Body.resolve(ui.style()),
                                color: couleur,
                                ..Default::default()
                            },
                        );
                        tache.wrap = egui::text::TextWrapping {
                            max_width: (largeur - 26.0).max(24.0),
                            max_rows: 1,
                            break_anywhere: true,
                            overflow_character: Some('…'),
                        };
                        let galerie = ui.ctx().fonts_mut(|f| f.layout_job(tache));
                        let y = rect.center().y - galerie.size().y / 2.0;
                        ui.painter()
                            .galley(egui::pos2(rect.left() + 4.0, y), galerie, couleur);

                        // ✕ : enregistré après la ligne, donc prioritaire dans sa zone.
                        let zone_x = egui::Rect::from_center_size(
                            egui::pos2(rect.right() - 11.0, rect.center().y),
                            egui::vec2(18.0, hauteur_ligne - 4.0),
                        );
                        let croix = ui.interact(
                            zone_x,
                            egui::Id::new(("playlist-croix", i)),
                            egui::Sense::click(),
                        );
                        let couleur_croix = if croix.hovered() {
                            ui.visuals().strong_text_color()
                        } else {
                            ui.visuals().weak_text_color()
                        };
                        ui.painter().text(
                            croix.rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "✕",
                            egui::FontId::proportional(12.0),
                            couleur_croix,
                        );
                        if croix.on_hover_text("Retirer de la playlist").clicked() {
                            to_remove = Some(i);
                        }

                        // Clic sur la ligne (hors ✕, qui a déjà consommé le sien).
                        if reponse.clicked() && to_remove.is_none() {
                            to_play = Some(i);
                        }
                        // Glissement : la charge utile est posée au premier mouvement.
                        if reponse.drag_started() {
                            egui::DragAndDrop::set_payload(ctx, i);
                        }
                        if glisse && ui.rect_contains_pointer(rect) {
                            cible = Some(i);
                        }
                        tracing::debug!(
                            target: "kinterface",
                            "playlist ligne {} rect {:?}",
                            i,
                            rect
                        );
                    }

                    // Zone de fin de liste : elle n'existe **que** pendant un
                    // glissement (sinon c'était une boîte grise sans objet).
                    if glisse && total > 0 {
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 12.0),
                            egui::Sense::hover(),
                        );
                        if ui.rect_contains_pointer(rect) {
                            cible_fin = true;
                        }
                    }

                    // Repère d'insertion : une barre, seulement pendant le glissement.
                    if glisse {
                        let pinceau = egui::Stroke::new(
                            3.0_f32,
                            egui::Color32::from_rgb(90, 170, 255),
                        );
                        if let Some(i) = cible {
                            if let Some(rect) = rects.get(i) {
                                ui.painter().hline(rect.x_range(), rect.top() + 1.0, pinceau);
                            }
                        } else if cible_fin {
                            if let Some(rect) = rects.last() {
                                ui.painter().hline(rect.x_range(), rect.bottom() - 1.0, pinceau);
                            }
                        }
                    }

                    // Dépôt : la charge utile n'est retirée qu'au relâchement.
                    if glisse && relache && (cible.is_some() || cible_fin) && total > 0 {
                        if let Some(de) = egui::DragAndDrop::take_payload::<usize>(ctx) {
                            let de = *de;
                            let vers = if cible_fin { total - 1 } else { cible.unwrap_or(de) };
                            if de != vers {
                                if let Some(nouvel) = self.player.deplacer_dans_playlist(de, vers) {
                                    self.config.playlist = self.player.playlist.clone();
                                    let _ = self.config.save();
                                    tracing::info!(
                                        "Playlist : élément {} déplacé vers {} (courant = {})",
                                        de,
                                        vers,
                                        nouvel
                                    );
                                    self.annonce(&format!(
                                        "Playlist : élément déplacé ({} → {})",
                                        de + 1,
                                        vers + 1
                                    ));
                                }
                            }
                        }
                    }

                    if let Some(i) = to_play {
                        self.play_playlist_index(i);
                    }
                    if let Some(i) = to_remove {
                        let retire = self
                            .player
                            .playlist
                            .get(i)
                            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                            .unwrap_or_default();
                        self.player.remove_from_playlist(i);
                        self.config.playlist = self.player.playlist.clone();
                        let _ = self.config.save();
                        // Journalisé : une action qui ne laisse aucune trace est
                        // invérifiable (règle du projet), et le clic sur ✕ ne doit pas
                        // être confondu avec un clic de ligne.
                        tracing::info!("Playlist : élément {} retiré ({})", i, retire);
                        self.annonce(&format!("Playlist : retiré — {}", retire));
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        let melange = self.player.shuffle();
                        if ui
                            .selectable_label(melange, "🔀 Aléatoire")
                            .on_hover_text(
                                "Parcourir la liste au hasard, chaque élément une fois avant un nouveau cycle (Ctrl+A)",
                            )
                            .clicked()
                        {
                            self.basculer_aleatoire();
                        }
                        let repetition = self.player.repeat();
                        let (icone, aide_rep) = match repetition {
                            crate::config::RepeatMode::Aucune => ("🔁 Aucune", "Répétition : aucune — clic : relire le fichier (Ctrl+Maj+L)"),
                            crate::config::RepeatMode::Fichier => ("🔂 Fichier", "Répétition : le fichier — clic : relire la playlist (Ctrl+Maj+L)"),
                            crate::config::RepeatMode::Playlist => ("🔁 Playlist", "Répétition : la playlist — clic : aucune répétition (Ctrl+Maj+L)"),
                        };
                        if ui
                            .selectable_label(repetition != crate::config::RepeatMode::Aucune, icone)
                            .on_hover_text(aide_rep)
                            .clicked()
                        {
                            self.repetition_suivante();
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button("🗑")
                                .on_hover_text("Vider la playlist")
                                .clicked()
                            {
                                self.player.playlist.clear();
                                self.config.playlist.clear();
                                let _ = self.config.save();
                            }
                            if ui
                                .small_button("➕")
                                .on_hover_text("Ajouter des fichiers à la playlist")
                                .clicked()
                            {
                                if let Some(paths) = rfd::FileDialog::new()
                                    .add_filter("Vidéos", &["mp4", "mkv", "avi", "mov"])
                                    .pick_files()
                                {
                                    for path in paths {
                                        self.config.playlist.push(path.clone());
                                        self.player.add_to_playlist(path);
                                    }
                                    let _ = self.config.save();
                                }
                            }
                        });
                    });
                });
            // Largeur effectivement occupée : c'est le contrôle qui manquait quand
            // un nom de fichier long élargissait le volet jusqu'à la moitié de la
            // fenêtre (signalé en session 34 : « trop large et buggé »).
            tracing::debug!(
                target: "kinterface",
                "playlist panneau largeur {:.0}",
                panneau.response.rect.width()
            );
        }

        // Fenêtre de paramètres : architecture en sections dépliables (src/settings.rs).
        // Le corps défile et la hauteur est bornée à l'écran, donc la fenêtre ne peut
        // pas déborder ; ajouter un réglage = ajouter une section dans `SECTIONS`.
        if self.show_settings {
            let ouvert = crate::settings::window(ctx, self);
            // Fermeture par le ✕ : journalisée comme celle déclenchée par Échap.
            if self.show_settings && !ouvert {
                tracing::info!("Fenêtre fermée: Préférences");
                self.annonce("Fenêtre « Préférences » fermée");
            }
            self.show_settings = ouvert;
        }

        // Boîte « Renommer le signet »
        if self.signet_renommage.is_some() {
            let mut ouvert = true;
            let mut valider = false;
            let mut annuler = false;
            let reponse = egui::Window::new("🔖 Renommer le signet")
                .open(&mut ouvert)
                .collapsible(false)
                .resizable(false)
                .default_pos([120.0, 160.0])
                .default_width(380.0)
                .show(ctx, |ui| {
                    if let Some(r) = self.signet_renommage.as_mut() {
                        ui.label(format!("Signet à {}", crate::utils::format_time(r.ms)));
                        let rep = ui.add(egui::TextEdit::singleline(&mut r.saisie).desired_width(360.0));
                        if !r.a_le_focus {
                            rep.request_focus();
                            r.a_le_focus = true;
                        }
                        if rep.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            valider = true;
                        }
                        ui.horizontal(|ui| {
                            if ui.button("✔ Renommer").clicked() {
                                valider = true;
                            }
                            if ui.button("Annuler").clicked() {
                                annuler = true;
                            }
                        });
                    }
                });
            if let Some(r) = &reponse {
                tracing::debug!(target: "kinterface", "fenetre signet rect {:?}", r.response.rect);
            }
            if valider {
                self.valider_renommage();
            } else if annuler || !ouvert {
                self.signet_renommage = None;
            }
        }

        // Fenêtre « Ouvrir une adresse »
        if self.show_url_dialog {
            let mut ouvert = self.show_url_dialog;
            let mut lancer = false;
            let reponse = egui::Window::new("🌐 Ouvrir une adresse")
                .open(&mut ouvert)
                .collapsible(false)
                .resizable(false)
                .default_pos([120.0, 120.0])
                .default_width(520.0)
                .show(ctx, |ui| {
                    ui.label("Adresse du flux (http, https, HLS, RTSP, RTMP, SMB…) :");
                    let rep = ui.add(
                        egui::TextEdit::singleline(&mut self.url_input)
                            .desired_width(500.0)
                            .hint_text("https://exemple.test/flux.m3u8"),
                    );
                    ui.label(
                        egui::RichText::new(
                            "FFmpeg ouvre l'adresse directement. Les flux non seekables \
                             (direct) ne permettent ni reprise ni avance rapide.",
                        )
                        .small()
                        .weak(),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("▶ Ouvrir").clicked()
                            || (rep.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        {
                            lancer = true;
                        }
                        if ui.button("Annuler").clicked() {
                            self.show_url_dialog = false;
                        }
                    });
                });
            if let Some(r) = &reponse {
                tracing::debug!(target: "kinterface", "fenetre url rect {:?}", r.response.rect);
            }
            if lancer {
                let url = self.url_input.trim().to_string();
                if crate::utils::is_stream_url(&url) {
                    match self.player.load_url(&url) {
                        Ok(info) => {
                            tracing::info!(target: "kinterface", "flux ouvert : {}", url);
                            self.after_media_loaded(Path::new(&url));
                            self.annonce(&format!("Flux : {}", info.filename));
                            self.show_url_dialog = false;
                        }
                        Err(e) => {
                            tracing::warn!("Flux impossible : {}", e);
                            self.error_message = Some(format!("Flux impossible : {}", e));
                        }
                    }
                } else {
                    self.error_message = Some(
                        "Adresse non reconnue : elle doit commencer par http://, https://, \
                         rtsp://, rtmp://, smb:// ou ftp://"
                            .to_string(),
                    );
                }
            }
            if self.show_url_dialog && !ouvert {
                tracing::info!("Fenêtre fermée: Ouvrir une adresse");
                self.show_url_dialog = false;
            }
        }

        // Fenêtre statistiques
        if self.show_statistics {
            // `.open()` est indispensable : sans lui, egui ne dessine AUCUN
            // bouton de fermeture (✕) et la fenêtre ne peut plus être fermée.
            let mut ouvert = self.show_statistics;
            let detachee = self.est_detachee("statistiques");
            let retour = crate::fenetre::afficher(
                ctx, "statistiques", "Statistiques de lecture", detachee, &mut ouvert, [420.0, 440.0], true,
                egui::Window::new("📊 Statistiques de lecture").collapsible(true).resizable(false),
                |ui, _detachee| {
                    if let Some(info) = self.player.media_info() {
                        ui.label(format!("Fichier: {}", info.filename));
                        if let Some(path) = self.player.current_path() {
                            if let Ok(meta) = std::fs::metadata(path) {
                                ui.label(format!("Taille: {}", Self::format_size(meta.len())));
                            }
                        }
                        ui.label(format!("Résolution: {}x{}", info.resolution.0, info.resolution.1));
                        ui.label(format!("Codec vidéo: {}", info.video_codec));
                        ui.label(format!(
                            "Décodage: {}",
                            if self.player.is_hardware_decoding() {
                                "VA-API (matériel)"
                            } else {
                                "logiciel (CPU)"
                            }
                        ));
                        // HDR : ce que le flux annonce, et ce que l'affichage permet.
                        // Dans la fenêtre « Statistiques » parce que c'est là qu'on
                        // cherche à comprendre une image qui ne ressemble pas à ce
                        // qu'on attend.
                        if let Some(hdr) = self.player.hdr_description() {
                            ui.label(format!("HDR: {}", hdr));
                        }
                        ui.label(format!("Affichage: {}", self.player.hdr_contexte().resume()));
                        ui.label(format!("Codec audio: {}", info.audio_codec));
                        ui.label(format!("FPS: {:.2}", info.fps));
                        ui.label(format!("Débit: {}", Self::format_bitrate(info.bitrate)));
                        ui.label(format!(
                            "Audio: {}",
                            if self.player.has_audio() {
                                "présent"
                            } else {
                                "absent"
                            }
                        ));
                        
                        // Chaîne de filtres réellement installée (les compteurs
                        // doivent rester égaux : chaque filtre est 1 image → 1 image).
                        let chaine = self.player.filters_description();
                        if chaine.is_empty() {
                            ui.label("Filtres vidéo: aucun");
                        } else {
                            ui.label(format!("Filtres vidéo: {}", chaine));
                            if let Some((entrees, sorties)) = self.player.filter_frame_counts() {
                                ui.label(format!(
                                    "Filtres — images traitées: {} (entrées {} / sorties {})",
                                    sorties, entrees, sorties
                                ));
                            }
                        }

                        ui.separator();
                        ui.label(format!("Position: {} / {}", 
                            Self::format_time(self.player.current_position_ms()),
                            Self::format_time(self.player.total_duration_ms())));
                        ui.label(format!("Vitesse: {:.2}x", self.player.get_speed()));
                        ui.label(format!("Volume: {:.0}%", self.player.get_volume() * 100.0));
                    } else {
                        ui.label("Aucun média chargé");
                    }
                },
            );
            if retour.bascule {
                self.basculer_detachee("statistiques");
            }
            // Géométrie de la fenêtre journalisée (niveau debug) : elle sert à l'audit
            // de l'interface pour cliquer précisément sur le bouton de fermeture, sans
            // heuristique sur les pixels.
            if let Some(r) = retour.rect {
                tracing::debug!(target: "kinterface", "fenetre statistiques rect {:?}", r);
            }
            // Fermeture par le ✕ de la fenêtre : on la journalise comme celle
            // déclenchée par Échap, sinon l'action est invisible en diagnostic.
            if self.show_statistics && !ouvert {
                tracing::info!("Fenêtre fermée: Statistiques");
                self.annonce("Fenêtre « Statistiques » fermée");
            }
            self.show_statistics = ouvert;
        }
        
        // Fenêtre d'aide des raccourcis (F1)
        if self.show_shortcuts {
            // `.open()` est indispensable : sans lui, egui ne dessine AUCUN
            // bouton de fermeture (✕) et la fenêtre ne peut plus être fermée.
            let mut ouvert = self.show_shortcuts;
            let detachee = self.est_detachee("aide");
            let retour = crate::fenetre::afficher(
                ctx, "aide", "Aide des raccourcis", detachee, &mut ouvert, [500.0, 560.0], true,
                egui::Window::new("⌨ Aide des raccourcis").collapsible(true).resizable(false).default_width(460.0),
                |ui, _detachee| {
                    ui.label(
                        egui::RichText::new(
                            "Tous les raccourcis sont personnalisables (Préférences). \
                             F1 ferme cette fenêtre.",
                        )
                        .weak(),
                    );
                    // L'aide est construite à partir de la table des raccourcis :
                    // elle affiche donc ce qui MARCHE réellement, y compris après
                    // personnalisation. Une liste écrite en dur aurait menti dès le
                    // premier raccourci modifié.
                    let mut categorie_courante = "";
                    for action in crate::raccourcis::Action::TOUTES {
                        if action.categorie() != categorie_courante {
                            categorie_courante = action.categorie();
                            ui.separator();
                            ui.label(egui::RichText::new(categorie_courante).strong());
                        }
                        let touches = self.raccourcis.affichage(*action);
                        egui::Grid::new(format!("aide_{}", action.id()))
                            .num_columns(2)
                            .min_col_width(150.0)
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(texte_sur(&touches)).monospace());
                                ui.label(texte_sur(action.libelle()));
                                ui.end_row();
                            });
                    }
                    ui.separator();
                    ui.label(
                        egui::RichText::new(
                            "Réglages ▸ Raccourcis clavier et souris pour tout modifier.",
                        )
                        .small()
                        .weak(),
                    );
                },
            );
            if retour.bascule {
                self.basculer_detachee("aide");
            }
            // Géométrie de la fenêtre journalisée (niveau debug) : elle sert à l'audit
            // de l'interface pour cliquer précisément sur le bouton de fermeture, sans
            // heuristique sur les pixels.
            if let Some(r) = retour.rect {
                tracing::debug!(target: "kinterface", "fenetre aide rect {:?}", r);
            }
            // Fermeture par le ✕ de la fenêtre : on la journalise comme celle
            // déclenchée par Échap, sinon l'action est invisible en diagnostic.
            if self.show_shortcuts && !ouvert {
                tracing::info!("Fenêtre fermée: Aide des raccourcis");
                self.annonce("Fenêtre « Aide des raccourcis » fermée");
            }
            self.show_shortcuts = ouvert;
        }

        // Fenêtre À propos
        if self.show_about {
            // `.open()` est indispensable : sans lui, egui ne dessine AUCUN
            // bouton de fermeture (✕) et la fenêtre ne peut plus être fermée.
            let mut ouvert = self.show_about;
            let detachee = self.est_detachee("apropos");
            let retour = crate::fenetre::afficher(
                ctx, "apropos", "À propos de RMPlayer", detachee, &mut ouvert, [380.0, 380.0], true,
                egui::Window::new("ℹ À propos de RMPlayer").collapsible(false).resizable(false),
                |ui, _detachee| {
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("🎬 RMPlayer").heading());
                        ui.label("Lecteur vidéo professionnel Linux");
                        ui.label("Version 0.1.0");
                        ui.label("");
                        ui.label("Architecture:");
                        ui.indent("stack", |ui| {
                            ui.label("• FFmpeg (libav) — décodage vidéo/audio");
                            ui.label("• egui — interface utilisateur");
                            ui.label("• cpal — audio natif PipeWire/PulseAudio");
                        });
                        ui.label("");
                        ui.label("© 2026 RMPlayer — MIT License");
                    });
                },
            );
            if retour.bascule {
                self.basculer_detachee("apropos");
            }
            // Géométrie de la fenêtre journalisée (niveau debug) : elle sert à l'audit
            // de l'interface pour cliquer précisément sur le bouton de fermeture, sans
            // heuristique sur les pixels.
            if let Some(r) = retour.rect {
                tracing::debug!(target: "kinterface", "fenetre a-propos rect {:?}", r);
            }
            // Fermeture par le ✕ de la fenêtre : on la journalise comme celle
            // déclenchée par Échap, sinon l'action est invisible en diagnostic.
            if self.show_about && !ouvert {
                tracing::info!("Fenêtre fermée: À propos");
                self.annonce("Fenêtre « À propos » fermée");
            }
            self.show_about = ouvert;
        }
        
        // Une erreur survenue DANS le moteur (abandon d'une reconnexion réseau,
        // par exemple) doit apparaître à l'écran, pas seulement dans le journal :
        // sinon l'image reste figée sur la dernière frame sans aucune explication.
        if let PlayerState::Error(message) = self.player.state() {
            if self.error_message.is_none() {
                tracing::error!("Erreur du moteur affichée à l'écran : {}", message);
                self.error_message = Some(message);
            }
        }

        // Messages d'erreur temporaires
        {
            let error = self.error_message.clone();
            if error.is_some() {
                egui::TopBottomPanel::top("error_panel")
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.horizontal_centered(|ui| {
                            if let Some(ref err) = error {
                                ui.label(egui::RichText::new(format!("⚠ {}", err)).color(egui::Color32::RED));
                            }
                            if ui.small_button("✕").clicked() {
                                self.error_message = None;
                            }
                        });
                    });
            }
        }
        
        // Message d'état (capture enregistrée, bascule de fonction…) — éphémère
        if self.status_timer > 0.0 {
            self.status_timer -= ctx.input(|i| i.stable_dt);
            if self.status_timer <= 0.0 {
                self.status_message = None;
            }
        }
        if let Some(message) = self.status_message.clone() {
            egui::TopBottomPanel::top("status_message")
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.label(message);
                        if ui.small_button("✕").clicked() {
                            self.status_message = None;
                            self.status_timer = 0.0;
                        }
                    });
                });
        }

        // --- Reconnexion d'un flux réseau : bandeau persistant tant qu'elle dure ---
        // Ce n'est pas un message éphémère : il reste affiché pendant toute la
        // coupure, sinon l'utilisateur croirait à un gel du lecteur (l'image
        // reste figée sur la dernière frame reçue, c'est voulu).
        if self.player.reconnexion_en_cours() {
            egui::TopBottomPanel::top("bandeau_reconnexion")
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.label("⏳ Flux interrompu — reconnexion en cours…");
                        if ui.small_button("Arrêter").clicked() {
                            let _ = self.player.stop();
                        }
                    });
                });
        }

        // --- OSD : bandeau d'informations temporaire, réellement dessiné ---
        if self.show_osd_info && self.config.interface.show_osd && !self.show_statistics {
            let mut lines: Vec<String> = Vec::new();
            if let Some(info) = self.player.media_info() {
                lines.push(format!("🎬 {}", info.filename));
                lines.push(format!(
                    "{} / {}   ·   {}x{} @ {:.2} i/s",
                    Self::format_time(self.player.current_position_ms()),
                    Self::format_time(self.player.total_duration_ms()),
                    info.resolution.0,
                    info.resolution.1,
                    info.fps
                ));
                lines.push(format!(
                    "Vidéo {} · {}",
                    info.video_codec,
                    if self.player.is_hardware_decoding() {
                        "VA-API"
                    } else {
                        "logiciel"
                    }
                ));
                if self.player.has_audio() {
                    lines.push(format!(
                        "Audio {} · tampon {} ms",
                        info.audio_codec,
                        self.player.audio_buffered_ms()
                    ));
                    // Quelle horloge cadence l'image, et à quelle distance elle est
                    // de l'image affichée : c'est la synchronisation image/son.
                    lines.push(match self.player.audio_clock_ms() {
                        Some(_) => format!(
                            "Horloge : carte son ({:.2} s consommées) · écart image {:+} ms",
                            self.player.audio_played_seconds(),
                            self.player.drift_ms(self.last_frame_pts_ms)
                        ),
                        None => "Horloge : temps mural".to_string(),
                    });
                } else {
                    lines.push("Sans audio".to_string());
                }
                lines.push(format!(
                    "Volume {:.0} %   ·   Vitesse {:.2}x",
                    self.player.get_volume() * 100.0,
                    self.player.get_speed()
                ));
                if self.player.reconnexion_en_cours() {
                    lines.push("⏳ Flux interrompu — reconnexion en cours".to_string());
                }
            } else {
                lines.push("Aucun média chargé".to_string());
            }

            // État de la répétition A-B, s'il y a lieu
            if let (Some(a), Some(b)) = self.player.loop_points() {
                lines.push(format!(
                    "🔁 Répétition A-B : {} → {}",
                    Self::format_time(a),
                    Self::format_time(b)
                ));
            }

            egui::Area::new(egui::Id::new("osd_overlay"))
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(24.0, 48.0))
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_black_alpha(200))
                        .corner_radius(6.0)
                        .inner_margin(10.0)
                        .show(ui, |ui| {
                            for line in lines {
                                ui.label(egui::RichText::new(line).color(egui::Color32::WHITE));
                            }
                        });
                });
        }

        // Repaint continu pendant la lecture (la décision d'avancer la frame est
        // prise en début de `update` selon l'horloge média). Hors lecture, egui
        // reste passif : l'image affichée reste figée.
        if self.player.state() == PlayerState::Playing {
            ctx.request_repaint();
        }

        // Une commande de fenêtre différée a besoin d'un tour de plus pour être
        // envoyée (même en pause, où egui reste sinon passif).
        if !self.pending_viewport_cmds.is_empty() {
            ctx.request_repaint();
        }

        // Géométrie en attente d'enregistrement : hors lecture, egui redevient
        // passif dès le dernier événement — sans ce réveil, une fenêtre
        // redimensionnée puis laissée tranquille ne serait jamais mémorisée.
        if self.geometrie_changee_le.is_some() {
            ctx.request_repaint_after(TEMPORISATION_GEOMETRIE);
        }

        // Un champ de texte éditable a-t-il le focus ? (lu ICI, après le dessin de toutes les fenêtres,
        // pour la garde des raccourcis de l'image suivante)
        self.saisie_texte_active = ctx.output(|o| o.ime.is_some());
    }
}

/// Convertit une couleur « RRGGBB » (ou « #RRGGBB ») en `Color32`.
///
/// Renvoie `None` si la chaîne n'est pas exploitable : l'appelant retombe alors sur
/// la couleur par défaut plutôt que d'échouer (une préférence mal saisie ne doit pas
/// empêcher l'affichage des sous-titres).
fn parse_couleur(texte: &str) -> Option<egui::Color32> {
    let t = texte.trim().trim_start_matches('#');
    if t.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&t[0..2], 16).ok()?;
    let v = u8::from_str_radix(&t[2..4], 16).ok()?;
    let b = u8::from_str_radix(&t[4..6], 16).ok()?;
    Some(egui::Color32::from_rgb(r, v, b))
}

/// Écrit un PNG RGBA 8 bits. Encodeur pur Rust (`png`), aucune dépendance système.
pub(crate) fn save_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    crate::capture::ecrire(path, crate::config::CaptureFormat::Png, 100, width, height, rgba)
}

// La table statique des raccourcis a été remplacée par `raccourcis::Tableau` :
// l'aide (F1) et les réglages lisent désormais les combinaisons RÉELLEMENT
// actives, y compris personnalisées. Une liste en dur aurait menti dès le
// premier raccourci modifié.

/// Dessine un fragment de sous-titre ASS (cadre, ombre, contour, karaoké, rotation).
///
/// Séparée de `App::draw_ass` pour une raison précise : un même fragment peut devoir
/// être peint plusieurs fois avec des découpages différents (`\iclip`), et recopier ce
/// bloc aurait fait diverger les copies au premier changement.
fn dessiner_fragment_ass(
    painter: &egui::Painter,
    zone: egui::Rect,
    p: &crate::ass::RunPlace,
) -> bool {
    // `Color32::PLACEHOLDER` est indispensable : egui **fige la couleur dans la
    // galerie** lors de la mise en page, et l'argument de `painter.galley` n'est
    // qu'un repli pour les parties sans couleur. Une galerie blanche dessinait donc
    // tout en blanc, quelles que soient les couleurs du fichier ASS (mesuré : rouge
    // et jaune à 0 pixel).
    let galerie = painter.layout_no_wrap(
        p.text.clone(),
        egui::FontId::proportional(p.font_px),
        egui::Color32::PLACEHOLDER,
    );
    let pos = egui::pos2(zone.left() + p.x, zone.top() + p.y);

    // Décalage tourné : l'ombre et le contour sont des copies décalées du texte ;
    // quand le texte tourne, leurs décalages doivent tourner avec lui, sinon l'ombre
    // part du mauvais côté (visible immédiatement sur un titre incliné).
    let decaler = |dx: f32, dy: f32| -> egui::Vec2 {
        if p.angle == 0.0 {
            return egui::vec2(dx, dy);
        }
        let (s, c) = p.angle.sin_cos();
        egui::vec2(dx * c - dy * s, dx * s + dy * c)
    };
    let couleur_de = |c: crate::ass::Rgba| {
        egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
    };
    // Pose d'un texte coloré. Sans rotation, on garde le chemin d'origine
    // (`galley_with_override_text_color`) : c'est celui qui a été validé par les bancs.
    let poser = |peintre: &egui::Painter, decalage: egui::Vec2, couleur: egui::Color32| {
        let p_pos = pos + decalage;
        if p.angle == 0.0 {
            peintre.galley_with_override_text_color(p_pos, galerie.clone(), couleur);
        } else {
            peintre.add(egui::epaint::TextShape {
                pos: p_pos,
                galley: galerie.clone(),
                underline: egui::Stroke::NONE,
                fallback_color: couleur,
                override_text_color: Some(couleur),
                opacity_factor: 1.0,
                angle: p.angle,
            });
        }
    };

    // Cadre opaque (BorderStyle=3) : rectangle plein derrière le texte. On se sert de
    // la largeur calculée au placement (et non de la galerie) pour que le cadre épouse
    // exactement le texte mesuré.
    if let Some(fond) = p.box_colour {
        let marge = p.font_px * 0.15;
        let cadre = egui::Rect::from_min_size(pos, egui::vec2(p.width, p.height))
            .expand2(egui::vec2(marge, marge * 0.3));
        let couleur = couleur_de(fond);
        if p.angle == 0.0 {
            painter.rect_filled(cadre, egui::CornerRadius::same(0), couleur);
        } else {
            // Cadre tourné : quatre coins tournés autour de l'origine du texte.
            let coins = [cadre.left_top(), cadre.right_top(), cadre.right_bottom(), cadre.left_bottom()]
                .iter()
                .map(|c| pos + decaler(c.x - pos.x, c.y - pos.y))
                .collect::<Vec<_>>();
            painter.add(egui::epaint::PathShape {
                points: coins,
                closed: true,
                fill: couleur,
                stroke: egui::Stroke::NONE.into(),
            });
        }
    }

    // Ombre portée : une copie décalée, dessinée AVANT le texte.
    if p.shadow_px > 0.0 {
        let d = decaler(p.shadow_px, p.shadow_px);
        poser(painter, d, couleur_de(p.shadow_colour));
    }

    let couleur = couleur_de(p.colour);
    // Contour : 8 directions dès que l'épaisseur dépasse 1 px (voir session 18).
    if p.outline_px > 0.0 {
        let contour = couleur_de(p.outline_colour);
        let directions: &[(f32, f32)] = if p.outline_px > 1.0 {
            &[
                (-1.0, 0.0),
                (1.0, 0.0),
                (0.0, -1.0),
                (0.0, 1.0),
                (-0.7, -0.7),
                (0.7, -0.7),
                (-0.7, 0.7),
                (0.7, 0.7),
            ]
        } else {
            &[(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)]
        };
        for (dx, dy) in directions {
            let d = decaler(dx * p.outline_px, dy * p.outline_px);
            poser(painter, d, contour);
        }
    }

    // --- Karaoké -------------------------------------------------
    // `\kf` / `\K` : la syllabe est d'abord entièrement dans la couleur secondaire,
    // puis recouverte de la couleur principale **de gauche à droite**, à la vitesse de
    // la durée de la syllabe. On obtient ce balayage en dessinant deux fois : le texte
    // complet en secondaire, puis le même texte découpé à la largeur parcourue.
    if let Some((secondaire, avancement)) = p.karaoke {
        if avancement < 1.0 {
            poser(painter, egui::Vec2::ZERO, couleur_de(secondaire));
            let largeur_parcourue = p.width * avancement.clamp(0.0, 1.0);
            if largeur_parcourue > 0.5 {
                let bande = egui::Rect::from_min_size(pos, egui::vec2(largeur_parcourue, p.height));
                // Avec rotation, `clip_rect` d'egui reste aligné sur les axes : on
                // découpe sur les bornes tournées de la bande (découpage approché,
                // combinaison `\frz` + `\kf` rare, signalée en debug).
                let zone_balayage = if p.angle == 0.0 {
                    bande
                } else {
                    let coins = [
                        bande.left_top(),
                        bande.right_top(),
                        bande.right_bottom(),
                        bande.left_bottom(),
                    ]
                    .iter()
                    .map(|c| pos + decaler(c.x - pos.x, c.y - pos.y))
                    .collect::<Vec<_>>();
                    let min = egui::pos2(
                        coins.iter().fold(f32::INFINITY, |a, c| a.min(c.x)),
                        coins.iter().fold(f32::INFINITY, |a, c| a.min(c.y)),
                    );
                    let max = egui::pos2(
                        coins.iter().fold(f32::NEG_INFINITY, |a, c| a.max(c.x)),
                        coins.iter().fold(f32::NEG_INFINITY, |a, c| a.max(c.y)),
                    );
                    egui::Rect::from_min_max(min, max)
                };
                let peintre_decoupe = painter.with_clip_rect(zone_balayage);
                poser(&peintre_decoupe, egui::Vec2::ZERO, couleur);
            }
            return true;
        }
    }

    poser(painter, egui::Vec2::ZERO, couleur);
    if p.bold {
        // Faux gras, comme pour le rendu uniforme.
        poser(painter, decaler(p.font_px * 0.03, 0.0), couleur);
    }
    true
}

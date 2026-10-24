//! Configuration utilisateur — sérialisation JSON
//!
//! **Règle du projet (session 8) :** tout champ présent ici est réellement lu par le
//! moteur. Un réglage stocké mais ignoré est un mensonge pour l'utilisateur : les
//! champs décoratifs de la première version (deband, HDR, interpolation, égaliseur,
//! profil, raccourcis…) ont été retirés. Pour en ajouter un, il faut d'abord le
//! brancher dans le code.
//!
//! Les noms de variants gardent des `alias` pour rester lisible avec les fichiers de
//! configuration antérieurs (`EwaLanczosSharp`, `Mitchell`, `Sigmoid`).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Méthode de mise à l'échelle — réellement appliquée par libswscale lors de la
/// conversion en RGBA (voir `player::scale_frame_to_rgba`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScalingMethod {
    /// Plus proche voisin : le plus rapide, aspect « pixelisé » (utile pour l'animation)
    NearestNeighbor,
    /// Bilinéaire : compromis rapide
    Bilinear,
    /// Bicubique : net, bon rendu général
    Bicubic,
    /// Lanczos : le plus net en réduction, léger surcoût CPU (défaut)
    #[serde(alias = "EwaLanczosSharp")]
    Lanczos,
    /// Moyenne de zone : idéal pour réduire fortement (4K → petite fenêtre)
    #[serde(alias = "Mitchell")]
    Area,
}

/// Matrice de couleur de la conversion YUV → RVB.
///
/// « Automatique » suit ce que le flux déclare ; à défaut, la définition décide (HD → BT.709,
/// SD → BT.601). Les autres valeurs l'imposent — utile pour un film mal étiqueté, dont les
/// couleurs saturées sont légèrement fausses.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum ColorMatrix {
    #[default]
    Auto,
    Bt601,
    Bt709,
    Bt2020,
}

impl ColorMatrix {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Auto => "Automatique",
            Self::Bt601 => "BT.601 (SD)",
            Self::Bt709 => "BT.709 (HD)",
            Self::Bt2020 => "BT.2020 (UHD)",
        }
    }
}

/// Plage des niveaux du signal YUV : limitée (16–235, la norme vidéo) ou complète (0–255).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum ColorRange {
    #[default]
    Auto,
    Limited,
    Full,
}

impl ColorRange {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Auto => "Automatique",
            Self::Limited => "Limitée (16–235)",
            Self::Full => "Complète (0–255)",
        }
    }
}

/// Rapport d'aspect imposé à l'affichage.
///
/// « Automatique » utilise la résolution native du média (comportement d'origine).
/// Les autres valeurs **déforment** volontairement l'image : c'est ce qu'on veut pour
/// corriger un encodage étiqueté 16:9 alors qu'il est en 4:3, ou pour remplir l'écran.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AspectMode {
    #[serde(alias = "auto")]
    Auto,
    #[serde(alias = "4:3", alias = "4/3")]
    Ratio4x3,
    #[serde(alias = "16:9", alias = "16/9")]
    Ratio16x9,
    #[serde(alias = "1.85")]
    Ratio185,
    #[serde(alias = "2.35")]
    Ratio235,
    /// Remplit la zone disponible sans conserver aucun rapport (étirement).
    Remplir,
    /// Remplit la zone en **gardant** le rapport de l'image et en **rognant** ce qui dépasse
    /// (pas de bandes noires, mais les bords de l'image sont perdus).
    Rogner,
    /// Rapport personnalisé, en largeur pour une hauteur de 1 (`custom_aspect`).
    Personnalise,
}

impl Default for AspectMode {
    fn default() -> Self {
        Self::Auto
    }
}

impl AspectMode {
    /// Libellé affiché à l'utilisateur.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Auto => "Automatique (celle du média)",
            Self::Ratio4x3 => "4:3",
            Self::Ratio16x9 => "16:9",
            Self::Ratio185 => "1,85:1 (cinéma)",
            Self::Ratio235 => "2,35:1 (cinémascope)",
            Self::Remplir => "Remplir la fenêtre (étirer)",
            Self::Rogner => "Remplir la fenêtre (rogner)",
            Self::Personnalise => "Personnalisé",
        }
    }
}

/// Parité des trames pour le désentrelacement (`yadif`).
///
/// `Auto` laisse FFmpeg déduire le champ dominant de l'image ; `Tff` (haut
/// d'abord) et `Bff` (bas d'abord) sont utiles quand la détection se trompe —
/// un mauvais choix ne supprime pas le peigne, il le déplace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DeinterlaceParity {
    /// Détection automatique (défaut)
    #[serde(alias = "auto", alias = "Auto")]
    Auto,
    /// Trame du haut d'abord (`tff`)
    #[serde(alias = "tff")]
    TopFirst,
    /// Trame du bas d'abord (`bff`)
    #[serde(alias = "bff")]
    BottomFirst,
}

impl Default for DeinterlaceParity {
    fn default() -> Self {
        Self::Auto
    }
}

/// Filtre de désentrelacement.
///
/// **Pourquoi le choix existe** : il a été **mesuré**, pas deviné
/// (`tools/bench-desentrelacement.sh`, à refaire si la question se repose).
/// Sur une image ordinaire **immobile** — la majorité d'un film —, `bwdif` restitue
/// 45,8 dB contre 43,4 dB pour `yadif` ; sur une mire en lignes de 1 px (le pire cas
/// du désentrelacement), 20,6 dB contre 9,2 dB, parce que `yadif` efface purement et
/// simplement le détail d'une ligne sur deux alors que `bwdif` le conserve dans les
/// zones où il ne détecte pas de mouvement. En **mouvement**, les deux rendus ne
/// diffèrent que de 0,25 niveau en moyenne : c'est bien la détection de mouvement de
/// `bwdif` qui fait la différence sur les plans calmes. Et `bwdif` est **moins cher**
/// (11,2 contre 13,2 ms par image 1080p). D'où le défaut.
///
/// `yadif` reste offert : c'est le filtre historique, et une source au télécinéma
/// mal détecté peut se comporter autrement — au moins le choix est explicite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeinterlaceFilter {
    /// `yadif` : interpolation spatiale/temporelle, filtre historique.
    #[serde(alias = "Yadif", alias = "yadif=1")]
    Yadif,
    /// `bwdif` : détection de mouvement, conserve le détail des zones fixes (défaut).
    #[serde(alias = "Bwdif", alias = "bwdif=1")]
    Bwdif,
}

impl Default for DeinterlaceFilter {
    fn default() -> Self {
        Self::Bwdif
    }
}

impl DeinterlaceFilter {
    /// Nom du filtre FFmpeg (et libellé d'interface).
    pub fn nom(&self) -> &'static str {
        match self {
            Self::Yadif => "yadif",
            Self::Bwdif => "bwdif",
        }
    }

    /// Choix proposés dans l'interface.
    pub const CHOIX: [DeinterlaceFilter; 2] = [Self::Bwdif, Self::Yadif];
}

/// Lecture tolérante de la méthode de mise à l'échelle : une valeur inconnue (ou un
/// ancien nom non reconnu) retombe sur Lanczos au lieu de faire échouer toute la
/// configuration.
fn tolerant_scaling<'de, D>(deserializer: D) -> Result<ScalingMethod, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or(ScalingMethod::Lanczos))
}

/// Paramètres vidéo
///
/// `#[serde(default)]` : un champ absent prend sa valeur par défaut au lieu de faire
/// échouer la lecture du fichier entier. Sans cela, un profil écrit à la main avec
/// trois champs seulement était rejeté **en bloc** (mesuré : profil « ignoré »).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoSettings {
    /// Décodage matériel (VA-API) — repli logiciel automatique si le pilote refuse.
    /// `default` assure la compatibilité avec les fichiers de configuration existants.
    #[serde(default = "default_hardware_decoding")]
    pub hardware_decoding: bool,
    /// Faire la **réduction d'échelle dans le GPU** quand le décodage est matériel.
    ///
    /// Mesuré (1080p affiché en 720p) : 10,9 ms/image → **7,1 ms/image** (cf.
    /// `docs/OPTIMISATION-COMPARATIF.md`). Ne s'applique qu'aux cas où aucun filtre
    /// CPU n'est demandé, et retombe silencieusement sur l'ancien chemin si le
    /// pilote refuse l'échelle matérielle. Laisser à `true` : c'est un repli sûr.
    #[serde(default = "default_vrai")]
    pub gpu_scaling: bool,
    /// Méthode de réduction/agrandissement côté CPU
    #[serde(default = "default_scaling_method", deserialize_with = "tolerant_scaling")]
    pub scaling_method: ScalingMethod,

    // ---- Chaîne de filtres (libavfilter), appliquée après décodage ----
    // Chaque champ est traduit en filtre FFmpeg par `player::filter_description`
    // et n'existe que si un filtre réel l'applique (règle du projet).
    /// Détramage (filtre `deband`) : atténue les bandes visibles dans les dégradés
    /// (ciels, fondus au noir) sans lisser les contours.
    #[serde(default)]
    pub deband: bool,
    /// Débruitage temporel et spatial (filtre `hqdn3d`), force 0 à 3. 0 = désactivé.
    /// Traite surtout le bruit de compression des sources sombres ou de bas débit.
    #[serde(default)]
    pub denoise: u8,
    /// Désentrelacement (`bwdif` par défaut, `yadif` au choix), appliqué
    /// **uniquement aux trames marquées entrelacées** (`deint=interlaced`) : une
    /// source progressive n'est pas dégradée.
    #[serde(default)]
    pub deinterlace: bool,
    /// Luminosité (filtre `eq`), -1,0 à +1,0 — 0,0 = neutre.
    #[serde(default)]
    pub brightness: f32,
    /// Contraste (filtre `eq`), 0,0 à 3,0 — 1,0 = neutre.
    #[serde(default = "default_un")]
    pub contrast: f32,
    /// Saturation (filtre `eq`), 0,0 à 3,0 — 1,0 = neutre, 0,0 = noir et blanc.
    #[serde(default = "default_un")]
    pub saturation: f32,
    /// Gamma (filtre `eq`), 0,1 à 10,0 — 1,0 = neutre.
    #[serde(default = "default_un")]
    pub gamma: f32,
    /// Teinte (filtre `hue`), -180 à +180 degrés — 0 = neutre.
    ///
    /// Tourne les couleurs autour du cercle chromatique : utile pour corriger une
    /// balance des blancs franchement fausse, ou pour un rendu voulu.
    #[serde(default)]
    pub hue: f32,
    /// Niveaux d'entrée (filtre `colorlevels`) : noir et blanc à recadrer.
    ///
    /// Un vieux transfert vidéo a souvent des noirs « lavés » (16 → 40) et des
    /// blancs écrasés ; ramener l'entrée à 0,06 / 0,94 rend le contraste sans
    /// toucher au gamma. 0 = aucune correction (neutre).
    #[serde(default)]
    pub niveaux_noir: f32,
    #[serde(default = "default_un")]
    pub niveaux_blanc: f32,
    /// HDR : conversion tonale, 10 bits, tramage et débanding GPU (session 35).
    #[serde(default)]
    pub hdr: HdrSettings,
    /// Adoucissement (filtre `gblur`), 0 = désactivé, 1 à 3 = rayon croissant.
    ///
    /// Sert à masquer un bruit de compression tenace ou une mise à l'échelle dure.
    #[serde(default)]
    pub adoucir: u8,

    // ---- Paramètres avancés des filtres (section « Avancé » des préférences) ----
    /// Rayon de recherche du détramage (`deband=range`), 4 à 64 pixels. Un rayon
    /// plus grand traite des dégradés plus étendus, au prix du temps de calcul.
    #[serde(default = "default_deband_range")]
    pub deband_range: u32,
    /// Seuil de détection des bandes (`deband=1thr/2thr/3thr`), 0,001 à 0,2.
    /// Plus le seuil est haut, plus le filtre agit (et plus il peut lisser du détail).
    #[serde(default = "default_deband_threshold")]
    pub deband_threshold: f32,
    /// Parité des trames pour le désentrelacement (`yadif` comme `bwdif`).
    #[serde(default)]
    pub deinterlace_parity: DeinterlaceParity,
    /// Filtre de désentrelacement : `bwdif` (défaut, détail préservé) ou `yadif`.
    #[serde(default)]
    pub deinterlace_filter: DeinterlaceFilter,
    /// Netteté (filtre `unsharp`), 0 = arrêté, 1 à 3 = croissant.
    /// Compense le flou d'une source surcompressée ; au-delà de 3, les contours
    /// deviennent cernés (halos) — d'où la borne.
    #[serde(default)]
    pub sharpen: u8,
    /// Rotation de l'image : 0, 90, 180 ou 270 degrés (filtre `transpose`).
    #[serde(default)]
    pub rotate: u16,
    /// Miroir **vertical** (filtre `vflip`), pour une source filmée à l'envers.
    ///
    /// Le nom historique reste `flip` : le renommer ferait perdre le réglage des
    /// configurations existantes.
    #[serde(default)]
    pub flip: bool,
    /// Miroir **horizontal** (filtre `hflip`) : image inversée gauche-droite
    /// (webcam, miroir de salle, certains encodages).
    #[serde(default)]
    pub flip_h: bool,
    /// Afficher la zone de recadrage par-dessus l'image (repère visuel).
    #[serde(default = "default_vrai")]
    pub crop_overlay: bool,
    /// Matrice de couleur et plage de la conversion YUV → RVB (voir `ColorMatrix`).
    #[serde(default)]
    pub color_matrix: ColorMatrix,
    #[serde(default)]
    pub color_range: ColorRange,
    /// Glisser : décalage de l'image dans la fenêtre, en pourcentage de sa largeur / hauteur
    /// affichées (positif = vers la droite / le bas). Ce qui sort de la fenêtre est rogné.
    #[serde(default)]
    pub pan_x_percent: f32,
    #[serde(default)]
    pub pan_y_percent: f32,
    /// Rapport d'aspect imposé à l'affichage (déformation volontaire si ≠ natif).
    #[serde(default)]
    pub aspect: AspectMode,
    /// Rapport largeur/hauteur utilisé quand `aspect` vaut `Personnalise` (ex. 2.0).
    #[serde(default = "default_custom_aspect")]
    pub custom_aspect: f32,
    /// Recadrage manuel des bandes noires, en pixels de la résolution native
    /// (filtre `crop`). Les quatre valeurs sont appliquées ensemble, jamais seules :
    /// un recadrage asymétrique volontaire reste possible.
    #[serde(default)]
    pub crop_top: u32,
    #[serde(default)]
    pub crop_bottom: u32,
    #[serde(default)]
    pub crop_left: u32,
    #[serde(default)]
    pub crop_right: u32,
    /// Zoom numérique, 1,0 = image entière (filtre `crop` + `scale`).
    /// 1,5 affiche la moitié centrale agrandie — utile pour un plan large.
    #[serde(default = "default_zoom")]
    pub zoom: f32,
}

fn default_deband_range() -> u32 {
    16
}

fn default_deband_threshold() -> f32 {
    0.02
}

fn default_zoom() -> f32 {
    1.0
}

fn default_custom_aspect() -> f32 {
    16.0 / 9.0
}

fn default_hardware_decoding() -> bool {
    true
}

fn default_scaling_method() -> ScalingMethod {
    ScalingMethod::Lanczos
}

/// Valeur neutre des réglages d'image `eq` (contraste, saturation, gamma).
fn default_un() -> f32 {
    1.0
}

impl VideoSettings {
    /// Vrai si au moins un filtre de la chaîne est actif.
    pub fn filters_active(&self) -> bool {
        self.deband
            || self.denoise > 0
            || self.deinterlace
            || (self.brightness.abs() > 1e-3)
            || (self.contrast - 1.0).abs() > 1e-3
            || (self.saturation - 1.0).abs() > 1e-3
            || (self.gamma - 1.0).abs() > 1e-3
            || self.sharpen > 0
            || self.rotate != 0
            || self.flip
            || self.flip_h
            || (self.zoom - 1.0).abs() > 1e-3
            || self.crop_active()
    }

    /// Copie sans recadrage : sert à l'**aperçu** du repère de recadrage.
    ///
    /// Une fois le recadrage appliqué, l'image affichée ne contient plus les bandes
    /// retirées : dessiner le repère dessus assombrirait du contenu *conservé*.
    /// Pour régler, il faut voir l'image d'origine.
    pub fn sans_recadrage(&self) -> Self {
        let mut copie = self.clone();
        copie.crop_top = 0;
        copie.crop_bottom = 0;
        copie.crop_left = 0;
        copie.crop_right = 0;
        copie
    }

    /// Vrai si un recadrage (bandes noires) est demandé.
    pub fn crop_active(&self) -> bool {
        self.crop_top + self.crop_bottom + self.crop_left + self.crop_right > 0
    }

    /// Vrai si l'image est recadrée ou déformée : le rapport d'aspect affiché ne
    /// correspond alors plus à la résolution native, et il faut en tenir compte.
    pub fn aspect_impose(&self) -> bool {
        self.aspect != AspectMode::Auto
    }

    /// Décalage vertical du sous-titre, à rappeler dans l'aide quand le recadrage est
    /// actif : le texte est positionné en pourcentage de l'image VISIBLE, donc il
    /// remonte avec les bandes noires retirées.

    /// Vrai si la géométrie de l'image est modifiée (rotation, miroir, zoom).
    ///
    /// Ces filtres changent les dimensions : la mise à l'échelle et le calcul du
    /// rapport d'aspect doivent en tenir compte, contrairement aux filtres de
    /// couleur.
    pub fn geometry_changed(&self) -> bool {
        self.rotate != 0
            || self.flip
            || self.flip_h
            || (self.zoom - 1.0).abs() > 1e-3
            || self.crop_active()
    }

    /// Remet la chaîne de filtres à l'état neutre (aucune transformation).
    pub fn reset_filters(&mut self) {
        self.deband = false;
        self.denoise = 0;
        self.deinterlace = false;
        self.brightness = 0.0;
        self.contrast = 1.0;
        self.saturation = 1.0;
        self.gamma = 1.0;
        // Nouveaux réglages (session 34) : sans ces lignes, « Revenir aux valeurs
        // neutres » les laisserait actifs — un bouton qui ne remet pas tout à zéro
        // est un piège pour l'utilisateur (et pour le banc qui le vérifie).
        self.hue = 0.0;
        self.niveaux_noir = 0.0;
        self.niveaux_blanc = 1.0;
        self.adoucir = 0;
        self.deband_range = default_deband_range();
        self.deband_threshold = default_deband_threshold();
        self.deinterlace_parity = DeinterlaceParity::default();
        self.sharpen = 0;
        self.rotate = 0;
        self.flip = false;
        self.flip_h = false;
        self.zoom = default_zoom();
        self.aspect = AspectMode::default();
        self.color_matrix = ColorMatrix::default();
        self.color_range = ColorRange::default();
        self.pan_x_percent = 0.0;
        self.pan_y_percent = 0.0;
        self.crop_top = 0;
        self.crop_bottom = 0;
        self.crop_left = 0;
        self.crop_right = 0;
    }
}

/// Nombre de bandes de l'égaliseur (`superequalizer` de FFmpeg en expose 18).
pub const EQ_BANDS: usize = 18;

/// Fréquences centrales des 18 bandes du filtre `superequalizer` (Hz).
/// Elles sont **imposées par FFmpeg** : ce ne sont pas des valeurs choisies ici.
pub const EQ_FREQUENCIES: [u32; EQ_BANDS] = [
    65, 92, 131, 185, 262, 370, 523, 740, 1047, 1480, 2093, 2960, 4186, 5920, 8372, 11840,
    16744, 20000,
];

/// Paramètres audio (`#[serde(default)]` : voir `VideoSettings`)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    /// Nom exact du périphérique de sortie (cpal) ; vide = périphérique par défaut.
    /// La liste est proposée dans les préférences (voir `audio::available_output_devices`).
    #[serde(default)]
    pub device: String,
    /// Volume maximal autorisé, en pourcentage — réellement appliqué (`set_volume_max`).
    #[serde(default = "default_volume_max")]
    pub volume_max: u16,

    /// Égaliseur actif (filtre `superequalizer`).
    #[serde(default)]
    pub eq_enabled: bool,
    // ---- Traitement audio (équivalents des réglages PotPlayer) ----
    /// Langue audio préférée (métadonnée `language` du conteneur : `fre`, `eng`…).
    /// Vide = prendre la première piste (comportement historique).
    #[serde(default)]
    pub default_language: String,
    /// Normaliseur de volume (filtre `dynaudnorm`) : égalise le niveau perçu entre
    /// les scènes calmes et les passages forts. C'est le « Normalizer » de PotPlayer.
    #[serde(default)]
    pub normalizer: bool,
    /// Mode nuit / compression de dynamique (`acompressor` + `alimiter`) : réduit
    /// l'écart entre les murmures et les explosions, sans jamais écrêter.
    #[serde(default)]
    pub night_mode: bool,
    /// Conserver la hauteur du son quand la vitesse change (`atempo`).
    ///
    /// Activé par défaut : sans lui, la vitesse modifie la hauteur (effet bande
    /// magnétique). Mesuré : à 2×, un son de 440 Hz reste à 440 Hz.
    #[serde(default = "default_vrai")]
    pub pitch_preserved: bool,
    /// Réduction de bruit (`afftdn`) : 0 = arrêté, 1 à 3 = croissant.
    #[serde(default)]
    pub noise_reduction: u8,
    /// Élargissement de l'image stéréo (`extrastereo`) : 0 = arrêté, 1 à 3.
    #[serde(default)]
    pub stereo_widen: u8,
    /// Filtre d'écoute au casque (`crossfeed`) : adoucit la séparation gauche/droite.
    #[serde(default)]
    pub crossfeed: bool,
    /// Suppression de la voix (`pan`, mode karaoké) : retire le canal central.
    #[serde(default)]
    pub karaoke: bool,
    /// Réverbération (`aecho`) : 0 = arrêtée, 1 à 3.
    #[serde(default)]
    pub reverb: u8,
    /// Équilibre son direct / réverbération (0 = presque sec, 1 = très humide).
    ///
    /// `aecho` sépare le gain d'entrée (le son direct) du gain de sortie (les
    /// réflexions) : ce réglage expose ce second gain, qui était figé. Utile pour
    /// doser une ambiance de salle sans changer de preset.
    #[serde(default = "default_reverb_mix")]
    pub reverb_mix: f32,
    /// Décalage audio en millisecondes (`adelay`), −2000 à +2000 (synchronisation).
    #[serde(default)]
    pub delay_ms: i32,

    /// Gain de chaque bande, en multiplicateur linéaire (1,0 = neutre).
    ///
    /// C'est exactement ce qu'attend `superequalizer` (`1b=…`, `2b=…`) : la
    /// traduction en décibels est celle de FFmpeg (gain 4 = +12 dB, 0,1 = −20 dB,
    /// mesuré dans `tools/verif-audio.sh`). Le vecteur est normalisé à `EQ_BANDS`
    /// valeurs au chargement, pour qu'un fichier de configuration incomplet ou
    /// modifié à la main ne produise jamais une chaîne invalide.
    #[serde(default = "default_eq_gains")]
    pub eq_gains: Vec<f32>,
}

fn default_volume_max() -> u16 {
    150
}

/// Équilibre direct/réverbération par défaut : 60 % de réflexions.
fn default_reverb_mix() -> f32 {
    0.6
}

fn default_eq_gains() -> Vec<f32> {
    vec![1.0; EQ_BANDS]
}

/// Valeur par défaut `true` (les champs `bool` seraient sinon `false`).
fn default_vrai() -> bool {
    true
}

impl AudioSettings {
    /// Vrai si au moins une bande s'écarte du neutre.
    pub fn eq_is_neutral(&self) -> bool {
        self.eq_gains.iter().all(|g| (g - 1.0).abs() < 1e-3)
    }

    /// Vrai si l'égaliseur doit réellement être inséré dans la chaîne.
    pub fn eq_active(&self) -> bool {
        self.eq_enabled && !self.eq_is_neutral()
    }

    /// Vrai si au moins un traitement audio est demandé (hors égaliseur et vitesse).
    ///
    /// Sert à décider s'il faut insérer un limiteur de sécurité et à afficher un
    /// résumé honnête dans l'interface.
    pub fn processing_active(&self) -> bool {
        self.normalizer
            || self.night_mode
            || self.noise_reduction > 0
            || self.stereo_widen > 0
            || self.crossfeed
            || self.karaoke
            || self.reverb > 0
            || self.delay_ms != 0
    }

    /// Remet tous les traitements à l'état neutre (l'égaliseur et le périphérique
    /// ne sont pas touchés).
    pub fn reset_processing(&mut self) {
        self.normalizer = false;
        self.night_mode = false;
        self.noise_reduction = 0;
        self.stereo_widen = 0;
        self.crossfeed = false;
        self.karaoke = false;
        self.reverb = 0;
        self.delay_ms = 0;
    }

    /// Ramène le gain à la plage acceptée par FFmpeg (0 = silence, 20 = +26 dB).
    pub fn gain(&self, bande: usize) -> f32 {
        self.eq_gains
            .get(bande)
            .copied()
            .unwrap_or(1.0)
            .clamp(0.0, 20.0)
    }

    /// Applique un jeu de gains (presets de l'interface) et active l'égaliseur.
    pub fn set_eq_preset(&mut self, preset: EqPreset) {
        self.eq_gains = preset.gains();
        self.eq_enabled = !self.eq_is_neutral();
    }

    /// Normalise le vecteur à `EQ_BANDS` valeurs bornées.
    pub fn sanitize(&mut self) {
        self.eq_gains.resize(EQ_BANDS, 1.0);
        for g in &mut self.eq_gains {
            *g = g.clamp(0.0, 20.0);
        }
        // Bornes des nouveaux traitements : un fichier édité à la main ne doit pas
        // produire une chaîne de filtres invalide.
        self.noise_reduction = self.noise_reduction.min(3);
        self.stereo_widen = self.stereo_widen.min(3);
        self.reverb = self.reverb.min(3);
        self.reverb_mix = if self.reverb_mix.is_finite() {
            self.reverb_mix.clamp(0.0, 1.0)
        } else {
            default_reverb_mix()
        };
        self.delay_ms = self.delay_ms.clamp(-2000, 2000);
    }
}

/// Presets d'égalisation proposés dans l'interface.
///
/// Les gains sont des multiplicateurs linéaires appliqués aux 18 bandes fixes de
/// `superequalizer` : `+6 dB = ×2`, `−6 dB = ×0,5`. Les courbes restent volontairement
/// douces (±6 dB) : au-delà, la correction d'un mauvais enregistrement devient une
/// coloration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EqPreset {
    Flat,
    Bass,
    Treble,
    Voice,
    Loudness,
    /// Cinéma : graves tenus et présence des dialogues, aigus adoucis.
    Cinema,
    /// Podcast / parole parlée : coupe-bas franc et médiums en avant.
    Podcast,
}

impl EqPreset {
    /// Presets proposés dans l'interface, dans l'ordre d'affichage. **Source unique** :
    /// la liste de l'interface en était une copie, qui a divergé dès qu'un preset a
    /// été ajouté.
    pub const CHOIX: [EqPreset; 7] = [
        EqPreset::Flat,
        EqPreset::Bass,
        EqPreset::Voice,
        EqPreset::Treble,
        EqPreset::Loudness,
        EqPreset::Cinema,
        EqPreset::Podcast,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EqPreset::Flat => "Neutre",
            EqPreset::Bass => "Basses renforcées",
            EqPreset::Treble => "Aigus renforcés",
            EqPreset::Voice => "Voix (dialogue)",
            EqPreset::Loudness => "Isolation / écoute tardive",
            EqPreset::Cinema => "Cinéma (graves + dialogues)",
            EqPreset::Podcast => "Podcast / parole",
        }
    }

    /// Gains des 18 bandes (65 Hz → 20 kHz).
    pub fn gains(self) -> Vec<f32> {
        use EqPreset::*;
        // Index des bandes : graves 0-4, médiums 5-11, aigus 12-17.
        let courbe: [f32; EQ_BANDS] = match self {
            Flat => [1.0; EQ_BANDS],
            Bass => [
                2.0, 2.0, 1.8, 1.5, 1.3, 1.1, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
                1.0, 1.0, 1.0,
            ],
            Treble => [
                1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.1, 1.3, 1.5, 1.7, 1.9,
                2.0, 2.0, 2.0,
            ],
            // Présence de la parole (~300 Hz – 3 kHz), graves et extrêmes aigus retirés.
            Voice => [
                0.6, 0.6, 0.8, 1.0, 1.2, 1.5, 1.8, 1.8, 1.6, 1.3, 1.1, 1.0, 0.8, 0.6, 0.5,
                0.4, 0.3, 0.3,
            ],
            // Écoute à faible volume : on compense la perte de sensibilité des
            // extrêmes (courbes isosoniques), sans toucher aux médiums.
            Loudness => [
                1.6, 1.6, 1.4, 1.2, 1.1, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.1, 1.2, 1.3,
                1.4, 1.4, 1.4,
            ],
            // Cinéma : sous-grave et grave soutenus (+5 dB environ), présence à
            // 1–4 kHz pour les dialogues, aigus adoucis (une piste de film est
            // souvent dure au-dessus de 10 kHz).
            Cinema => [
                1.8, 1.7, 1.5, 1.3, 1.2, 1.2, 1.3, 1.4, 1.4, 1.3, 1.2, 1.1, 1.0, 0.95, 0.9,
                0.85, 0.8, 0.8,
            ],
            // Podcast : coupe-bas net (bruit de pièce et plosives sous 150 Hz),
            // médiums de la parole en avant, aigus légèrement adoucis.
            Podcast => [
                0.35, 0.4, 0.5, 0.8, 1.2, 1.6, 1.7, 1.6, 1.4, 1.2, 1.1, 1.0, 0.95, 0.9, 0.85,
                0.8, 0.8, 0.8,
            ],
        };
        courbe.to_vec()
    }
}

/// Quand appliquer la conversion tonale HDR → SDR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HdrMode {
    /// Convertit si le flux est HDR (PQ, HLG ou BT.2020). Le cas normal.
    Auto,
    /// Convertit même un flux SDR (comparaison, ou flux mal étiqueté).
    Toujours,
    /// Ne convertit jamais : l'image est affichée telle quelle. Sur un écran SDR
    /// elle paraîtra lavée — c'est le réglage pour **comparer**, ou pour une sortie
    /// déjà capable de HDR.
    Jamais,
}

impl HdrMode {
    pub fn libelle(self) -> &'static str {
        match self {
            HdrMode::Auto => "Automatique (si le flux est HDR)",
            HdrMode::Toujours => "Toujours convertir",
            HdrMode::Jamais => "Jamais (image telle quelle)",
        }
    }
    pub const CHOIX: [HdrMode; 3] = [HdrMode::Auto, HdrMode::Toujours, HdrMode::Jamais];
}

/// Opérateur de conversion tonale (HDR → SDR).
///
/// Les noms diffèrent entre les deux moteurs : `libplacebo` (GPU) et `tonemap` de
/// FFmpeg (CPU, repli) n'offrent pas la même liste. Les deux tables sont figées ici,
/// et un test vérifie qu'aucun nom n'est vide — un nom invalide fait échouer **toute**
/// la chaîne de filtres, donc l'image entière.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tonemap {
    /// ITU-R BT.2390 (EETF) : référence des lecteurs modernes.
    Bt2390,
    /// Filmic de Hable : contraste plus marqué, un peu plus « cinéma ».
    Hable,
    /// Mobius : doux, conserve la teinte des hautes lumières.
    Mobius,
    /// Reinhard : simple, tendance à aplatir les hautes lumières.
    Reinhard,
    /// Aucune conversion, seulement l'écrêtage (« clip ») : le plus proche de
    /// l'image brute, utile pour comparer ou alimenter une sortie HDR.
    Clip,
}

impl Tonemap {
    pub fn libelle(self) -> &'static str {
        match self {
            Tonemap::Bt2390 => "BT.2390 (recommandé)",
            Tonemap::Hable => "Hable (contrasté)",
            Tonemap::Mobius => "Mobius (doux)",
            Tonemap::Reinhard => "Reinhard (simple)",
            Tonemap::Clip => "Aucune (écrêtage)",
        }
    }
    /// Nom attendu par `libplacebo=tonemapping=…`.
    pub fn nom_libplacebo(self) -> &'static str {
        match self {
            Tonemap::Bt2390 => "bt.2390",
            Tonemap::Hable => "hable",
            Tonemap::Mobius => "mobius",
            Tonemap::Reinhard => "reinhard",
            Tonemap::Clip => "clip",
        }
    }
    /// Nom attendu par `tonemap=tonemap=…` (repli CPU). FFmpeg n'a pas de BT.2390 :
    /// on prend `hable`, le plus proche en rendu, plutôt qu'un nom invalide.
    pub fn nom_ffmpeg(self) -> &'static str {
        match self {
            Tonemap::Bt2390 | Tonemap::Hable => "hable",
            Tonemap::Mobius => "mobius",
            Tonemap::Reinhard => "reinhard",
            Tonemap::Clip => "clip",
        }
    }
    pub const CHOIX: [Tonemap; 5] = [
        Tonemap::Bt2390,
        Tonemap::Hable,
        Tonemap::Mobius,
        Tonemap::Reinhard,
        Tonemap::Clip,
    ];
}

/// Traitement des couleurs hors du gamut BT.709 après conversion tonale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Gamut {
    /// Perceptuel : compresse les couleurs vives sans les écrêter (défaut).
    Perceptuel,
    /// Relatif : écrête au gamut, préserve la luminance.
    Relatif,
    /// Désature vers le blanc : le plus fidèle en teinte perçue.
    Desaturer,
}

impl Gamut {
    pub fn libelle(self) -> &'static str {
        match self {
            Gamut::Perceptuel => "Perceptuel (recommandé)",
            Gamut::Relatif => "Relatif (écrêtage)",
            Gamut::Desaturer => "Désaturer vers le blanc",
        }
    }
    pub fn nom_libplacebo(self) -> &'static str {
        match self {
            Gamut::Perceptuel => "perceptual",
            Gamut::Relatif => "relative",
            Gamut::Desaturer => "desaturate",
        }
    }
    pub const CHOIX: [Gamut; 3] = [Gamut::Perceptuel, Gamut::Relatif, Gamut::Desaturer];
}

/// Tramage lors de la réduction à 8 bits.
///
/// Une source 10 bits affichée en 8 bits sans tramage montre des **marches** dans les
/// dégradés (ciel, pénombre) : c'est le défaut le plus visible d'un rendu 8 bits.
///
/// **Les quatre méthodes sont celles que `libplacebo` propose réellement** (`none`,
/// `blue`, `ordered`, `white`). La « diffusion d'erreur » a été envisagée puis
/// retirée : elle n'existe pas dans ce filtre (`Invalid argument` à la construction
/// de la chaîne, mesuré) — c'est une option d'autres implémentations, pas de
/// libplacebo. N'exposer que ce qui fonctionne.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dither {
    Aucun,
    /// Bruit bleu : grain fin, invisible à distance (défaut).
    BruitBleu,
    /// Ordonné (matrice de Bayer) : rapide, motif régulier.
    Ordonne,
    /// Bruit blanc : plus visible, parfois préféré pour les aplats.
    Blanc,
}

impl Dither {
    pub fn libelle(self) -> &'static str {
        match self {
            Dither::Aucun => "Aucun",
            Dither::BruitBleu => "Bruit bleu (recommandé)",
            Dither::Ordonne => "Ordonné",
            Dither::Blanc => "Bruit blanc",
        }
    }
    pub fn nom_libplacebo(self) -> &'static str {
        match self {
            Dither::Aucun => "none",
            Dither::BruitBleu => "blue",
            Dither::Ordonne => "ordered",
            Dither::Blanc => "white",
        }
    }
    pub const CHOIX: [Dither; 4] = [
        Dither::BruitBleu,
        Dither::Ordonne,
        Dither::Blanc,
        Dither::Aucun,
    ];
}

/// Réglages HDR et 10 bits.
///
/// **Pourquoi ces champs et pas d'autres** : chacun modifie réellement la chaîne de
/// filtres produite (vérifié par `tools/verif-hdr.sh`). Un réglage qui ne change
/// aucun pixel est un mensonge à l'utilisateur — c'est la règle du projet depuis la
/// session 11.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HdrSettings {
    pub mode: HdrMode,
    pub tonemap: Tonemap,
    pub gamut: Gamut,
    /// Restitution du contraste perdu dans les hautes lumières (`contrast_recovery`),
    /// 0 = aucune, 1 = forte. Au-delà de 0,6, les détails des nuages et des lampes
    /// remontent mais le bruit aussi.
    pub contrast_recovery: f32,
    /// Détection de crête dynamique : adapte la conversion image par image (le
    /// contraire — une crête fixe — écrase les scènes sombres).
    pub peak_detect: bool,
    /// Pic lumineux de la **source**, en nits (1000 = HDR10 courant, 4000 = masters
    /// de cinéma). Utilisé par le repli CPU ; le moteur GPU mesure le pic réel image
    /// par image et n'en a pas besoin.
    pub pic_nits: f32,
    pub dither: Dither,
    /// Débanding fait par le GPU (libplacebo) au lieu du filtre CPU `deband` :
    /// une passe de moins sur le CPU, et il s'applique sur les 10 bits d'origine.
    pub gpu_deband: bool,
    pub gpu_deband_radius: f32,
    /// Seuil de détection des bandes, 0 à 1 (0,04 par défaut = 4 sur 1024 dans
    /// libplacebo).
    pub gpu_deband_threshold: f32,
    /// Grain ajouté après débanding : sans lui, les aplats deviennent trop lisses.
    pub gpu_deband_grain: f32,
}

impl Default for HdrSettings {
    fn default() -> Self {
        Self {
            mode: HdrMode::Auto,
            tonemap: Tonemap::Bt2390,
            gamut: Gamut::Perceptuel,
            contrast_recovery: 0.3,
            peak_detect: true,
            pic_nits: 1000.0,
            dither: Dither::BruitBleu,
            gpu_deband: false,
            gpu_deband_radius: 32.0,
            gpu_deband_threshold: 0.04,
            gpu_deband_grain: 12.0,
        }
    }
}

impl HdrSettings {
    /// Remet les valeurs par défaut (bouton de la section).
    pub fn reinitialiser(&mut self) {
        *self = Self::default();
    }
}

/// Paramètres sous-titres (`#[serde(default)]` : voir `VideoSettings`)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SubtitleSettings {
    /// Chargement automatique des fichiers de sous-titres homonymes
    pub auto_load: bool,
    /// Taille de la police de rendu (points)
    pub font_size: f32,
    /// Langue préférée : les fichiers suffixés `.fr.srt`, `.fr.ass`… sont choisis
    /// d'abord (voir `subtitles::auto_load`)
    pub default_language: String,
    /// Utiliser les sous-titres **présents dans le conteneur** (MKV/MP4) quand
    /// aucun fichier externe n'est chargé.
    #[serde(default = "default_vrai")]
    pub use_embedded: bool,

    // ---- Apparence du texte (comme les réglages de style de PotPlayer/VLC) ----
    /// Couleur du texte, en hexadécimal RRGGBB.
    #[serde(default = "default_sub_color")]
    pub color: String,
    /// Interlettre, en points ajoutés entre deux caractères (filtre typographique,
    /// comme « Interlettre » de `Dialog(383)` de PotPlayer).
    ///
    /// Négatif = resserré, positif = aéré. Utile pour une police large sur un
    /// écran étroit, ou pour un sous-titre de karaoké qu'on veut plus lisible.
    #[serde(default)]
    pub letter_spacing: f32,
    /// Marges gauche et droite du bloc, en pourcentage de la largeur de l'image.
    ///
    /// Elles **bornent la zone de texte** : un sous-titre long ne peut plus
    /// déborder sur les bords ni sortir de l'image (mesuré auparavant : un texte
    /// long commençait hors du cadre avec une ancre à 0 %).
    #[serde(default)]
    pub margin_left_percent: f32,
    #[serde(default)]
    pub margin_right_percent: f32,
    /// Couleur du contour (ou du fond), RRGGBB.
    #[serde(default = "default_sub_outline")]
    pub outline_color: String,
    /// Épaisseur du contour, en pixels (0 = sans contour).
    #[serde(default = "default_sub_outline_width")]
    pub outline_width: f32,
    /// Opacité du bandeau de fond derrière le texte, 0–255 (0 = pas de bandeau).
    #[serde(default = "default_sub_background")]
    pub background_alpha: u8,
    /// Position verticale, en pourcentage de la hauteur de l'image (0 = haut, 100 = bas).
    #[serde(default = "default_sub_position")]
    pub position_percent: f32,
    /// Alignement horizontal : 0 = gauche, 1 = centre, 2 = droite.
    ///
    /// L'alignement dit **où est ancré** le bloc de texte par rapport à
    /// `position_h_percent` ; sans position explicite (50 %), il donne le
    /// comportement historique (texte collé au bord, ou centré).
    #[serde(default = "default_sub_align")]
    pub align: u8,
    /// **Position horizontale du bloc**, en pourcentage de la largeur de l'image.
    ///
    /// 50 % = centre (valeur par défaut, comportement d'avant) ; 20 % place le
    /// texte au cinquième gauche, 80 % au cinquième droit. Combinée à `align`,
    /// elle permet de poser un sous-titre là où l'image laisse de la place — le
    /// réglage que PotPlayer expose sous « Position hori. (%) ».
    #[serde(default = "default_sub_position_h")]
    pub position_h_percent: f32,
    /// **Famille de police** des sous-titres (nom tel que le connaît fontconfig,
    /// par exemple « DejaVu Serif » ou « Liberation Mono »).
    ///
    /// Vide = police par défaut du lecteur. La famille est résolue par `fc-match`
    /// puis chargée dans egui : un nom inexistant retombe sur la police par
    /// défaut, avec une trace au journal (jamais un écran vide).
    #[serde(default)]
    pub font_family: String,
    /// Texte en gras.
    #[serde(default)]
    pub bold: bool,
    /// Texte en italique.
    #[serde(default)]
    pub italic: bool,
    /// Décalage temporel des sous-titres, en millisecondes (positif = plus tard).
    /// Corrige un fichier dont les temps sont décalés de quelques dixièmes.
    #[serde(default)]
    pub delay_ms: i32,

    // ---- Mise en page et effets (lot 2, session 37) ----
    /// Marges haute et basse, en pourcentage de la hauteur de l'image : elles bornent la
    /// zone verticale où `position_percent` place le bloc (comme les marges latérales).
    #[serde(default)]
    pub margin_top_percent: f32,
    #[serde(default)]
    pub margin_bottom_percent: f32,
    /// Interligne, en multiple de la taille de police (1,25 = comportement historique).
    #[serde(default = "default_sub_line_spacing")]
    pub line_spacing: f32,
    /// Ombre portée : décalage en pixels vers le bas-droite (0 = pas d'ombre).
    #[serde(default)]
    pub shadow_offset: f32,
    /// Couleur de l'ombre, RRGGBB.
    #[serde(default = "default_sub_outline")]
    pub shadow_color: String,
    /// Opacité de l'ombre, 0–255.
    #[serde(default = "default_sub_shadow_alpha")]
    pub shadow_alpha: u8,
    /// Second contour, **autour** du premier : épaisseur en pixels (0 = aucun) et couleur.
    #[serde(default)]
    pub outline2_width: f32,
    #[serde(default = "default_sub_outline")]
    pub outline2_color: String,
    /// Échelle horizontale et verticale du texte (1,0 = taille normale).
    #[serde(default = "default_sub_scale")]
    pub scale_x: f32,
    #[serde(default = "default_sub_scale")]
    pub scale_y: f32,
    /// Durée du fondu d'apparition et de disparition, en millisecondes (0 = coupe franche).
    #[serde(default)]
    pub fade_ms: u32,

    // ---- Langues, durées, mémoire de piste (lot 2, session 37) ----
    /// Langues préférées, **par ordre de priorité** (`["fre", "eng"]`). Vide = on n'utilise
    /// que `default_language` (comportement historique).
    #[serde(default)]
    pub preferred_languages: Vec<String>,
    /// Durée d'affichage minimale / maximale d'un sous-titre, en ms (0 = pas de borne).
    #[serde(default)]
    pub min_duration_ms: u32,
    #[serde(default)]
    pub max_duration_ms: u32,
    /// Mémoriser, par média, la piste choisie à la main (fichier externe ou piste intégrée).
    #[serde(default)]
    pub remember_track: bool,
}

impl SubtitleSettings {
    /// Langues à essayer, dans l'ordre : `preferred_languages` (nettoyées, sans doublon),
    /// sinon `default_language` seule.
    pub fn langues_preferees(&self) -> Vec<String> {
        let mut liste: Vec<String> = Vec::new();
        for l in &self.preferred_languages {
            let l = l.trim().to_lowercase();
            if !l.is_empty() && !liste.contains(&l) {
                liste.push(l);
            }
        }
        if liste.is_empty() {
            let d = self.default_language.trim().to_lowercase();
            if !d.is_empty() {
                liste.push(d);
            }
        }
        liste
    }
}

/// Paramètres interface
#[derive(Debug, Clone, Serialize, Deserialize)]
// `serde(default)` : un champ ajouté dans une version ultérieure ne doit pas faire
// rejeter TOUTE la configuration (tout retombait alors aux valeurs par défaut, et
// l'utilisateur perdait ses réglages en silence).
#[serde(default)]
pub struct InterfaceSettings {
    pub dark_theme: bool,
    /// Afficher l'OSD (bandeau d'informations temporaire)
    pub show_osd: bool,
    /// Durée d'affichage de l'OSD, en millisecondes
    pub osd_duration: u32,
    pub show_progress_bar: bool,
    /// Garder la barre de contrôles visible même en plein écran
    pub controls_always_visible: bool,
    /// Mettre en pause quand la fenêtre perd le focus (0 = jamais, 1 = perte de
    /// focus, 2 = fenêtre réduite seulement).
    ///
    /// **Pourquoi un choix à trois états** : les deux comportements rendent service
    /// dans des situations différentes — « pause dès que je change de fenêtre »
    /// convient à la vidéo, « seulement quand je réduis » à l'écoute en fond. Un
    /// simple booléen obligerait à choisir pour tout le monde.
    #[serde(default)]
    pub pause_sur_perte_focus: u8,
    /// Reprendre la lecture quand la fenêtre revient au premier plan.
    ///
    /// Séparé de la pause : certains veulent la pause automatique **sans** la
    /// reprise (ils reviennent exprès pour relancer).
    #[serde(default = "default_vrai")]
    pub reprendre_sur_focus: bool,
    pub show_title: bool,
    /// Dossier d'enregistrement des captures d'écran (`~` accepté)
    #[serde(default = "default_screenshots_dir")]
    pub screenshots_dir: String,
    /// Dernière géométrie de la fenêtre : coin supérieur gauche du **cadre**
    /// (décorations comprises — c'est la coordonnée qu'attend le gestionnaire de
    /// fenêtres), en points logiques. Ce n'est pas une préférence mais une
    /// **mémoire d'usage** : sans elle, la fenêtre revient à 1280×720 à chaque
    /// lancement, et il faut la replacer à chaque film.
    ///
    /// `None` = jamais enregistrée (premier lancement, ou configuration écrite
    /// avant la session 32) : on garde alors la taille par défaut et le
    /// placement du gestionnaire de fenêtres.
    #[serde(default)]
    pub window_pos: Option<[f32; 2]>,
    /// Taille de la zone cliente (largeur, hauteur), en points logiques.
    #[serde(default)]
    pub window_size: Option<[f32; 2]>,
    /// Fenêtre agrandie (maximisée) au dernier usage.
    ///
    /// `window_pos`/`window_size` gardent alors la géométrie de l'état
    /// **normal** : c'est ce qui permet de « restaurer » une fenêtre raisonnable
    /// au lieu de retrouver la taille de l'écran entier.
    #[serde(default)]
    pub window_maximized: bool,
}

/// Taille minimale de la fenêtre — la même que `with_min_inner_size` au démarrage.
/// Une géométrie enregistrée plus petite (fenêtre réduite par le gestionnaire,
/// ou fichier modifié à la main) est refusée plutôt qu'appliquée.
pub const TAILLE_MIN_FENETRE: [f32; 2] = [320.0, 200.0];

/// Part minimale de la fenêtre qui doit rester visible sur l'écran enregistré.
///
/// **Pourquoi un seuil** : une fenêtre déplacée sur un second écran puis
/// relancée après débranchement de cet écran serait invisible et inaccessible —
/// l'application semblerait ne pas démarrer. En dessous de ce quart visible, on
/// replace la fenêtre au centre de l'écran principal.
const PART_VISIBLE_MINIMALE: f32 = 0.25;

impl InterfaceSettings {
    /// Géométrie à demander au gestionnaire de fenêtres au démarrage.
    ///
    /// Renvoie `(taille, position)`, chacun facultatif : une valeur absente,
    /// non finie ou plus petite que `TAILLE_MIN_FENETRE` est ignorée (on retombe
    /// sur le comportement d'avant). Les valeurs enregistrées ne sont pas
    /// « corrigées » ici : la vérification contre l'écran réel se fait dès que sa
    /// taille est connue (`geometrie_visible`).
    pub fn geometrie_de_demarrage(&self) -> (Option<[f32; 2]>, Option<[f32; 2]>) {
        let taille = self
            .window_size
            .filter(|t| t.iter().all(|v| v.is_finite()))
            .filter(|t| t[0] >= TAILLE_MIN_FENETRE[0] && t[1] >= TAILLE_MIN_FENETRE[1]);
        let position = self
            .window_pos
            .filter(|p| p.iter().all(|v| v.is_finite()))
            // Une position absurdement lointaine (fichier modifié à la main,
            // écriture interrompue) n'est pas appliquée : 100 000 pt hors écran,
            // ce n'est pas un second moniteur, c'est une valeur cassée.
            .filter(|p| p[0].abs() < 100_000.0 && p[1].abs() < 100_000.0);
        (taille, position)
    }
}

/// La session graphique est-elle **Wayland** ?
///
/// **Pourquoi cela change quelque chose** : sous Wayland, un client ne connaît pas
/// la position de sa fenêtre (le protocole ne la lui transmet pas) et ne peut pas
/// la fixer — c'est le compositeur qui place. Restituer une taille et un état
/// agrandi reste possible ; restituer une position, non. Il faut donc le savoir
/// pour ne pas promettre ce qu'on ne peut pas tenir.
///
/// Détection : `XDG_SESSION_TYPE` d'abord (c'est ce que déclare le gestionnaire de
/// session), puis la présence de `WAYLAND_DISPLAY` quand le type est absent — une
/// session X11 lancée dans une session Wayland a bien les deux, et c'est
/// `DISPLAY`/`XDG_SESSION_TYPE` qui tranchent.
pub fn session_est_wayland() -> bool {
    session_wayland(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
        std::env::var("DISPLAY").ok().as_deref(),
    )
}

/// Version pure de `session_est_wayland`, pour être vérifiable sans session.
fn session_wayland(session: Option<&str>, wayland: Option<&str>, x11: Option<&str>) -> bool {
    match session.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("wayland") => true,
        Some("x11") => false,
        // Type absent (lancement depuis un script, service, test) : Wayland
        // seulement si un compositeur Wayland est joignable ET qu'aucun X11 ne
        // l'est — sinon on suppose X11, où la mémoire de position fonctionne.
        _ => wayland.is_some_and(|w| !w.is_empty()) && x11.map_or(true, |d| d.is_empty()),
    }
}

/// Ramène une géométrie enregistrée dans les limites de l'écran.
///
/// Trois corrections, dans cet ordre : une taille plus grande que l'écran est
/// réduite (sinon les bords et donc les poignées de redimensionnement sortent de
/// l'écran) ; une fenêtre dont il ne resterait pas `PART_VISIBLE_MINIMALE` de
/// visible est recentrée ; sinon la position est rendue telle quelle.
///
/// Fonction pure, pour être vérifiable sans fenêtre (voir les tests).
pub fn geometrie_visible(
    position: [f32; 2],
    taille: [f32; 2],
    ecran: [f32; 2],
) -> ([f32; 2], [f32; 2]) {
    if !ecran.iter().all(|v| v.is_finite() && *v > 0.0) {
        return (position, taille);
    }
    let mut taille = taille;
    if !taille.iter().all(|v| v.is_finite() && *v > 0.0) {
        taille = [1280.0_f32.min(ecran[0]), 720.0_f32.min(ecran[1])];
    }
    taille = [taille[0].min(ecran[0]), taille[1].min(ecran[1])];

    let centre = [
        ((ecran[0] - taille[0]) / 2.0).max(0.0),
        ((ecran[1] - taille[1]) / 2.0).max(0.0),
    ];
    if !position.iter().all(|v| v.is_finite()) {
        return (centre, taille);
    }

    // Surface de recouvrement entre la fenêtre et l'écran.
    let largeur = (position[0] + taille[0]).min(ecran[0]) - position[0].max(0.0);
    let hauteur = (position[1] + taille[1]).min(ecran[1]) - position[1].max(0.0);
    let visible = largeur.max(0.0) * hauteur.max(0.0);
    if visible < PART_VISIBLE_MINIMALE * taille[0] * taille[1] {
        return (centre, taille);
    }
    (position, taille)
}

fn default_sub_color() -> String {
    "FFFFFF".to_string()
}
fn default_sub_outline() -> String {
    "000000".to_string()
}
fn default_sub_line_spacing() -> f32 {
    1.25
}
fn default_sub_shadow_alpha() -> u8 {
    160
}
fn default_sub_scale() -> f32 {
    1.0
}
fn default_sub_outline_width() -> f32 {
    2.0
}
fn default_sub_background() -> u8 {
    140
}
fn default_sub_position_h() -> f32 {
    50.0
}

fn default_sub_position() -> f32 {
    88.0
}
fn default_sub_align() -> u8 {
    1
}
fn default_screenshots_dir() -> String {
    "~/Images/Captures-video".to_string()
}

/// Paramètres de lecture
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaybackSettings {
    /// Reprendre la lecture à la position quittée (`~/.cache/kplayer/positions.json`)
    pub resume: bool,
    /// **Pré-cache des flux réseau**, en millisecondes (0 = désactivé).
    ///
    /// Un fichier local n'en a pas besoin : le disque ne prend pas de retard. Un
    /// flux, si : sans réserve, chaque segment est demandé au moment exact où il
    /// devient nécessaire (mesuré : intervalles de requêtes égaux à la durée des
    /// segments, aucune avance) et la moindre gigue se voit. Avec une réserve, le
    /// lecteur lit en avance et traverse une coupure de courte durée sans s'arrêter.
    ///
    /// Le prix est de la mémoire : ≈ 350 Ko par seconde à 44,1 kHz stéréo.
    /// Borné à 30 s (voir `audio::CAPACITE_MAX_MS`).
    #[serde(default = "default_precache_ms")]
    pub precache_ms: u64,
    /// Lecture aléatoire au démarrage (mémorisée d'une session à l'autre : on ne
    /// veut pas la réactiver à chaque film).
    #[serde(default)]
    pub shuffle: bool,
    /// **Fichiers récents**, le plus récent d'abord, sans doublon, bornés à
    /// `RECENTS_MAX`. Écrits à chaque ouverture (`noter_recent`), proposés par le menu
    /// Fichier → Récents.
    #[serde(default)]
    pub recent: Vec<PathBuf>,
    /// Répétition A-B : nombre de **retours** au point A avant que la boucle soit levée et
    /// que le film continue au-delà de B. `0` = sans fin (le comportement historique).
    #[serde(default)]
    pub ab_repetitions: u32,
    /// Répétition A-B : pause, en millisecondes, entre l'arrivée en B et le retour en A.
    #[serde(default)]
    pub ab_delay_ms: u64,
}

/// Plafonds des deux réglages de la répétition A-B (un fichier édité à la main ne doit pas
/// pouvoir demander une pause de plusieurs heures).
pub const AB_REPETITIONS_MAX: u32 = 99;
pub const AB_DELAI_MAX_MS: u64 = 30_000;

/// Nombre maximal de fichiers récents conservés.
pub const RECENTS_MAX: usize = 10;

impl PlaybackSettings {
    /// Note l'ouverture d'un média : il passe en tête, son ancienne place est libérée,
    /// la liste est ramenée à `RECENTS_MAX`. Retourne `true` si la liste a changé.
    pub fn noter_recent(&mut self, chemin: &Path) -> bool {
        if self.recent.first().map(|p| p.as_path()) == Some(chemin) {
            return false;
        }
        self.recent.retain(|p| p.as_path() != chemin);
        self.recent.insert(0, chemin.to_path_buf());
        self.recent.truncate(RECENTS_MAX);
        true
    }
}

/// Profondeur de pré-cache par défaut d'un flux réseau : 6 s.
///
/// Choisi pour absorber une coupure « normale » (changement d'antenne, hoquet
/// Wi-Fi) sans retarder le démarrage : la référence est le pré-remplissage audio
/// (120 ms) plus la file vidéo, et non les 6 s (le pré-cache se **remplit en
/// tâche de fond**, il ne bloque pas l'ouverture).
fn default_precache_ms() -> u64 {
    6_000
}

/// Configuration complète de l'application
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub video: VideoSettings,
    /// Raccourcis personnalisés : `id d'action` → combinaisons séparées par des
    /// virgules (`"Ctrl+Maj+H, MoletteHaut"`). Une valeur vide désactive l'action.
    /// Seuls les **écarts** par rapport aux défauts sont écrits ici.
    #[serde(default)]
    pub raccourcis: std::collections::BTreeMap<String, String>,
    pub audio: AudioSettings,
    pub subtitles: SubtitleSettings,
    pub interface: InterfaceSettings,
    pub playback: PlaybackSettings,
    pub playlist: Vec<PathBuf>,
    /// Profils appliqués automatiquement selon le média (chemin, extension,
    /// résolution). Vide = aucune automatisation : un profil ne s'applique que
    /// sur demande. Voir `Regle`.
    #[serde(default)]
    pub regles: Vec<Regle>,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            hardware_decoding: true,
            gpu_scaling: true,
            scaling_method: ScalingMethod::Lanczos,
            deband: false,
            denoise: 0,
            deinterlace: false,
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            gamma: 1.0,
            hue: 0.0,
            niveaux_noir: 0.0,
            niveaux_blanc: 1.0,
            adoucir: 0,
            hdr: HdrSettings::default(),
            deband_range: default_deband_range(),
            deband_threshold: default_deband_threshold(),
            deinterlace_parity: DeinterlaceParity::default(),
            deinterlace_filter: DeinterlaceFilter::default(),
            sharpen: 0,
            rotate: 0,
            flip: false,
            flip_h: false,
            crop_overlay: true,
            zoom: default_zoom(),
            color_matrix: ColorMatrix::default(),
            color_range: ColorRange::default(),
            pan_x_percent: 0.0,
            pan_y_percent: 0.0,
            aspect: AspectMode::default(),
            custom_aspect: default_custom_aspect(),
            crop_top: 0,
            crop_bottom: 0,
            crop_left: 0,
            crop_right: 0,
        }
    }
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            device: String::new(),
            volume_max: 150,
            default_language: String::new(),
            eq_enabled: false,
            eq_gains: default_eq_gains(),
            normalizer: false,
            night_mode: false,
            pitch_preserved: true,
            noise_reduction: 0,
            stereo_widen: 0,
            crossfeed: false,
            karaoke: false,
            reverb: 0,
            reverb_mix: default_reverb_mix(),
            delay_ms: 0,
        }
    }
}

impl Default for SubtitleSettings {
    fn default() -> Self {
        Self {
            auto_load: true,
            font_size: 24.0,
            default_language: "fr".to_string(),
            use_embedded: true,
            color: default_sub_color(),
            letter_spacing: 0.0,
            margin_left_percent: 0.0,
            margin_right_percent: 0.0,
            outline_color: default_sub_outline(),
            outline_width: default_sub_outline_width(),
            background_alpha: default_sub_background(),
            position_h_percent: default_sub_position_h(),
            font_family: String::new(),
            position_percent: default_sub_position(),
            align: default_sub_align(),
            bold: false,
            italic: false,
            delay_ms: 0,
            margin_top_percent: 0.0,
            margin_bottom_percent: 0.0,
            line_spacing: default_sub_line_spacing(),
            shadow_offset: 0.0,
            shadow_color: default_sub_outline(),
            shadow_alpha: default_sub_shadow_alpha(),
            outline2_width: 0.0,
            outline2_color: default_sub_outline(),
            scale_x: 1.0,
            scale_y: 1.0,
            fade_ms: 0,
            preferred_languages: Vec::new(),
            min_duration_ms: 0,
            max_duration_ms: 0,
            remember_track: false,
        }
    }
}

impl Default for InterfaceSettings {
    fn default() -> Self {
        Self {
            dark_theme: true,
            show_osd: true,
            osd_duration: 3000,
            show_progress_bar: true,
            controls_always_visible: false,
            pause_sur_perte_focus: 0,
            reprendre_sur_focus: true,
            show_title: true,
            screenshots_dir: default_screenshots_dir(),
            // Aucune géométrie mémorisée : première taille connue, placement du
            // gestionnaire de fenêtres (comportement d'avant la session 32).
            window_pos: None,
            window_size: None,
            window_maximized: false,
        }
    }
}

impl Default for PlaybackSettings {
    fn default() -> Self {
        Self {
            resume: true,
            precache_ms: default_precache_ms(),
            shuffle: false,
            recent: Vec::new(),
            ab_repetitions: 0,
            ab_delay_ms: 0,
        }
    }
}

/// Valeurs par défaut, composées des structures : **une seule source de vérité**
/// (les mêmes valeurs servent à un fichier absent, à un champ manquant dans un
/// profil écrit à la main, et à la remise à zéro d'une section).
impl Default for Config {
    fn default() -> Self {
        Self {
            video: VideoSettings::default(),
            raccourcis: std::collections::BTreeMap::new(),
            audio: AudioSettings::default(),
            subtitles: SubtitleSettings::default(),
            interface: InterfaceSettings::default(),
            playback: PlaybackSettings::default(),
            playlist: Vec::new(),
            regles: Vec::new(),
        }
    }
}

/// Un profil de réglages nommé, réutilisable d'un média à l'autre.
///
/// **Pourquoi un profil séparé de la configuration** : la configuration dit « comment
/// je veux que le lecteur se comporte » ; un profil dit « comment je veux voir CE
/// genre de contenu » (un film sombre, de l'anime, un vieux DVD entrelacé). Sans
/// profils, il faut refaire dix réglages à chaque film — c'est le reproche qu'on fait
/// à tout lecteur qui accumule des options.
///
/// Le profil ne contient **que** les réglages de rendu : vidéo (filtres, géométrie,
/// cadrage, aspect), audio (égaliseur et traitements), sous-titres (style, langue).
/// Ni l'interface, ni la reprise de lecture, ni le **périphérique de sortie audio**
/// (propre à la machine : un profil partagé ne doit pas couper le son chez autrui).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profil {
    pub name: String,
    /// Description libre (à quoi sert ce profil).
    #[serde(default)]
    pub description: String,
    pub video: VideoSettings,
    pub audio: AudioSettings,
    pub subtitles: SubtitleSettings,
}

impl Profil {
    /// Capture l'état de rendu courant dans un profil nommé.
    pub fn capture(nom: &str, config: &Config) -> Self {
        Self {
            name: nom.to_string(),
            description: String::new(),
            video: config.video.clone(),
            audio: config.audio.clone(),
            subtitles: config.subtitles.clone(),
        }
    }

    /// Applique le profil à une configuration, en préservant ce qui est propre à la
    /// machine (périphérique de sortie).
    pub fn appliquer(&self, config: &mut Config) {
        let peripherique = config.audio.device.clone();
        config.video = self.video.clone();
        config.audio = self.audio.clone();
        config.audio.device = peripherique;
        config.subtitles = self.subtitles.clone();
        // Le nombre de bandes de l'égaliseur peut venir d'un fichier édité à la main.
        config.audio.sanitize();
    }

    /// Résumé lisible : ce qui distingue réellement ce profil des valeurs par défaut.
    /// Sert à vérifier qu'un profil n'est pas vide avant de l'enregistrer.
    pub fn resume(&self) -> String {
        let mut parties: Vec<String> = Vec::new();
        if self.video.deband {
            parties.push("détramage".into());
        }
        if self.video.denoise > 0 {
            parties.push(format!("débruitage {}", self.video.denoise));
        }
        if self.video.deinterlace {
            parties.push("désentrelacement".into());
        }
        if (self.video.brightness).abs() > 0.01
            || (self.video.contrast - 1.0).abs() > 0.01
            || (self.video.saturation - 1.0).abs() > 0.01
            || (self.video.gamma - 1.0).abs() > 0.01
        {
            parties.push("image".into());
        }
        if self.video.sharpen > 0 {
            parties.push(format!("netteté {}", self.video.sharpen));
        }
        if self.video.crop_active() {
            parties.push("recadrage".into());
        }
        if self.video.aspect != AspectMode::Auto {
            parties.push(format!("aspect {}", self.video.aspect.label()));
        }
        if self.audio.eq_enabled {
            parties.push("égaliseur".into());
        }
        for (actif, nom) in [
            (self.audio.normalizer, "normaliseur"),
            (self.audio.night_mode, "mode nuit"),
            (self.audio.noise_reduction > 0, "bruit"),
            (self.audio.stereo_widen > 0, "stéréo"),
            (self.audio.crossfeed, "crossfeed"),
            (self.audio.karaoke, "karaoké"),
            (self.audio.reverb > 0, "réverbération"),
        ] {
            if actif {
                parties.push(nom.into());
            }
        }
        if parties.is_empty() {
            "réglages par défaut".to_string()
        } else {
            parties.join(", ")
        }
    }
}

// ---------------------------------------------------------------------------
// Règles : un profil choisi automatiquement selon le média
// ---------------------------------------------------------------------------

/// Sur quoi porte une règle de profil automatique.
///
/// `Inconnu` existe pour la **tolérance de lecture** : une valeur écrite à la main
/// (`"champ": "dossier"`) ne doit pas faire perdre toute la configuration. Une
/// règle de champ inconnu ne correspond simplement jamais.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChampRegle {
    /// Sous-chaîne du chemin du média (`anime`, `~/Films/4K`).
    #[serde(alias = "Chemin", alias = "dossier", alias = "Dossier", alias = "path")]
    Chemin,
    /// Extension du fichier (`mkv`, `avi` ; plusieurs séparées par des virgules).
    #[serde(alias = "Extension", alias = "ext")]
    Extension,
    /// Résolution : seuil de largeur (`1920`, `≥1920`, `<1280`) ou dimensions
    /// exactes (`1280x720`).
    #[serde(alias = "Resolution", alias = "résolution", alias = "resolution")]
    Resolution,
    /// Champ non reconnu (fichier édité à la main) : règle jamais applicable.
    #[serde(other)]
    Inconnu,
}

impl ChampRegle {
    /// Libellé affiché dans les préférences et dans `--list-regles`.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Chemin => "chemin",
            Self::Extension => "extension",
            Self::Resolution => "résolution",
            Self::Inconnu => "champ inconnu",
        }
    }

    /// Champs proposés dans l'interface (l'ordre est celui du menu).
    pub const CHOIX: [ChampRegle; 3] = [Self::Chemin, Self::Extension, Self::Resolution];
}

/// Une règle « ce genre de média → ce profil ».
///
/// **Pourquoi** : les profils se choisissaient à la main. Or on sait à l'avance ce
/// qu'on va lire (« ce dossier = anime », « les fichiers de plus de 1920 pixels =
/// film sombre »). Une règle évite le clic et surtout l'oubli.
///
/// **Ordre** : les règles sont essayées dans l'ordre de la liste et la **première**
/// qui correspond gagne. C'est prévisible, et corrigeable par l'utilisateur — une
/// « spécificité » calculée (dossier > extension > résolution) devrait se deviner,
/// donc elle serait incompréhensible le jour où elle surprend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Regle {
    /// Champ comparé.
    pub champ: ChampRegle,
    /// Motif, dont le sens dépend du champ (voir `ChampRegle`).
    pub motif: String,
    /// Nom du profil à appliquer (dans `profiles.json`).
    pub profil: String,
}

/// Comparaison demandée par une règle de résolution.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ComparaisonResolution {
    /// Largeur au moins égale (`1920`, `>=1920`, `>1919`).
    LargeurAuMoins(u32),
    /// Largeur au plus égale (`<1280`, `<=1280`).
    LargeurAuPlus(u32),
    /// Largeur exacte (`=1920`).
    LargeurExacte(u32),
    /// Dimensions exactes (`1280x720`).
    Dimensions(u32, u32),
}

/// Analyse tolérante d'un motif de résolution.
///
/// Accepte `1920`, `≥1920`, `>=1920`, `> 1920`, `=1920`, `<1280`, `<=1280`,
/// `1280x720` (et `1280×720`). Un motif incompréhensible donne `None` : la règle
/// ne correspond jamais, elle ne correspond pas « au hasard ».
fn analyser_resolution(motif: &str) -> Option<ComparaisonResolution> {
    let texte = motif
        .trim()
        .to_lowercase()
        .replace('×', "x")
        .replace('≥', ">=")
        .replace('≤', "<=");
    let texte = texte.trim();

    if let Some(reste) = texte.strip_prefix(">=") {
        let v = reste.trim().parse::<u32>().ok()?;
        return (v > 0).then_some(ComparaisonResolution::LargeurAuMoins(v));
    }
    if let Some(reste) = texte.strip_prefix('>') {
        // Entiers : « > 1280 » équivaut à « au moins 1281 ».
        let v = reste.trim().parse::<u32>().ok()?;
        return Some(ComparaisonResolution::LargeurAuMoins(v.saturating_add(1)));
    }
    if let Some(reste) = texte.strip_prefix("<=") {
        let v = reste.trim().parse::<u32>().ok()?;
        return (v > 0).then_some(ComparaisonResolution::LargeurAuPlus(v));
    }
    if let Some(reste) = texte.strip_prefix('<') {
        let v = reste.trim().parse::<u32>().ok()?;
        return Some(ComparaisonResolution::LargeurAuPlus(v.saturating_sub(1)));
    }
    if let Some(reste) = texte.strip_prefix('=') {
        let v = reste.trim().parse::<u32>().ok()?;
        return (v > 0).then_some(ComparaisonResolution::LargeurExacte(v));
    }
    if let Some((largeur, hauteur)) = texte.split_once('x') {
        let l = largeur.trim().parse::<u32>().ok()?;
        let h = hauteur.trim().parse::<u32>().ok()?;
        return (l > 0 && h > 0).then_some(ComparaisonResolution::Dimensions(l, h));
    }
    // Un nombre seul se lit « au moins cette largeur » : c'est ce qu'annonce un
    // écran (« 1920 » = Full HD et au-delà), et la lecture la moins surprenante.
    let v = texte.parse::<u32>().ok()?;
    (v > 0).then_some(ComparaisonResolution::LargeurAuMoins(v))
}

impl Regle {
    /// Vrai si la règle désigne le média décrit par `chemin` et `resolution`.
    ///
    /// `resolution` vaut `None` tant que le média n'est pas ouvert (une règle de
    /// résolution ne peut alors pas décider — elle ne correspond pas).
    pub fn correspond(&self, chemin: &Path, resolution: Option<(u32, u32)>) -> bool {
        match self.champ {
            ChampRegle::Inconnu => false,
            ChampRegle::Chemin => {
                let motif = self.motif.trim().to_lowercase();
                if motif.is_empty() {
                    return false;
                }
                // `~` est développé : l'utilisateur écrit un chemin familier, le
                // média arrive avec un chemin absolu.
                let motif = match motif.strip_prefix("~/") {
                    Some(reste) => dirs::home_dir()
                        .map(|d| {
                            d.join(reste)
                                .to_string_lossy()
                                .to_lowercase()
                        })
                        .unwrap_or(motif),
                    None => motif,
                };
                chemin
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(motif.as_str())
            }
            ChampRegle::Extension => {
                let Some(ext) = chemin.extension().and_then(|e| e.to_str()) else {
                    return false;
                };
                let ext = ext.to_lowercase();
                self.motif
                    .split([',', ';', ' ', '\t'])
                    .map(|m| m.trim().trim_start_matches('.').to_lowercase())
                    .any(|m| !m.is_empty() && m == ext)
            }
            ChampRegle::Resolution => {
                let Some((largeur, hauteur)) = resolution else {
                    return false;
                };
                match analyser_resolution(&self.motif) {
                    Some(ComparaisonResolution::LargeurAuMoins(v)) => largeur >= v,
                    Some(ComparaisonResolution::LargeurAuPlus(v)) => largeur <= v,
                    Some(ComparaisonResolution::LargeurExacte(v)) => largeur == v,
                    Some(ComparaisonResolution::Dimensions(l, h)) => largeur == l && hauteur == h,
                    None => false,
                }
            }
        }
    }

    /// Résumé lisible, tel qu'il apparaît dans les préférences et le journal.
    pub fn resume(&self) -> String {
        let motif = self.motif.trim();
        match self.champ {
            ChampRegle::Chemin => format!("chemin contient « {} »", motif),
            ChampRegle::Extension => format!("extension « {} »", motif),
            ChampRegle::Resolution => format!("résolution « {} »", motif),
            ChampRegle::Inconnu => format!("champ inconnu (« {} »)", motif),
        }
    }
}

/// Première règle applicable : elle correspond au média **et** son profil existe.
///
/// Une règle qui désigne un profil supprimé est **ignorée** (la suivante est
/// essayée) : une règle orpheline ne doit pas bloquer la lecture ni laisser croire
/// qu'un profil a été appliqué.
pub fn regle_correspondante<'a>(
    regles: &'a [Regle],
    profils: &[Profil],
    chemin: &Path,
    resolution: Option<(u32, u32)>,
) -> Option<&'a Regle> {
    regles.iter().find(|r| {
        if !profils.iter().any(|p| p.name == r.profil) {
            tracing::debug!(
                "Règle ignorée (profil « {} » absent) : {}",
                r.profil,
                r.resume()
            );
            return false;
        }
        r.correspond(chemin, resolution)
    })
}

/// Applique au média qui vient d'être ouvert la règle de profil qui le concerne.
///
/// Retourne la règle appliquée (pour que l'appelant l'annonce et rafraîchisse le
/// moteur), ou `None` si aucune règle ne correspond. La configuration **reste**
/// modifiée : c'est le sens d'un profil, et l'appelant la persiste.
pub fn appliquer_regle(
    config: &mut Config,
    profils: &[Profil],
    chemin: &Path,
    resolution: Option<(u32, u32)>,
) -> Option<Regle> {
    let regle = regle_correspondante(&config.regles, profils, chemin, resolution)?.clone();
    let profil = profils.iter().find(|p| p.name == regle.profil)?.clone();
    profil.appliquer(config);
    tracing::info!(
        "Règle de profil : {} → « {} » ({})",
        regle.resume(),
        profil.name,
        profil.resume()
    );
    Some(regle)
}

/// Fichier des profils (à côté de `config.json`).
pub fn profiles_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("kplayer").join("profiles.json"))
}

/// Charge les profils enregistrés. Un fichier absent ou illisible donne une liste
/// vide : les profils sont un confort, ils ne doivent jamais empêcher de démarrer.
pub fn load_profiles() -> Vec<Profil> {
    let Some(chemin) = profiles_path() else {
        return Vec::new();
    };
    let Ok(texte) = std::fs::read_to_string(&chemin) else {
        return Vec::new();
    };
    match serde_json::from_str::<Vec<Profil>>(&texte) {
        Ok(mut liste) => {
            tracing::info!("{} profil(s) chargé(s) depuis {:?}", liste.len(), chemin);
            // Deux profils de même nom rendraient « Appliquer » ambigu.
            let mut vus: Vec<String> = Vec::new();
            liste.retain(|p| {
                if vus.contains(&p.name) {
                    tracing::warn!("Profil en double ignoré : {}", p.name);
                    false
                } else {
                    vus.push(p.name.clone());
                    true
                }
            });
            liste
        }
        Err(e) => {
            tracing::warn!("Profils illisibles ({:?}) : {}", chemin, e);
            Vec::new()
        }
    }
}

/// Enregistre la liste des profils.
pub fn save_profiles(liste: &[Profil]) -> Result<(), String> {
    let chemin = profiles_path().ok_or("répertoire de configuration introuvable")?;
    if let Some(parent) = chemin.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("création du dossier de configuration : {}", e))?;
    }
    let contenu =
        serde_json::to_string_pretty(liste).map_err(|e| format!("sérialisation : {}", e))?;
    std::fs::write(&chemin, contenu).map_err(|e| format!("écriture de {:?} : {}", chemin, e))?;
    tracing::info!("{} profil(s) enregistré(s) dans {:?}", liste.len(), chemin);
    Ok(())
}

/// Chemin du fichier de configuration (`$XDG_CONFIG_HOME/kplayer/config.json`).
///
/// Sert à nommer le socket de l'instance unique (`instance.rs`) : deux sessions
/// isolées — les bancs d'essai posent `XDG_CONFIG_HOME` — ne doivent pas se
/// partager l'instance.
pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("kplayer").join("config.json"))
}

/// Sections reconnues au premier niveau d'un fichier de configuration.
const SECTIONS_CONFIG: &[&str] = &[
    "video", "raccourcis", "audio", "subtitles", "interface", "playback", "playlist", "regles",
];

/// Vérifie qu'un texte est bien une configuration de KPlayer, sans rien écrire.
///
/// Un `{}` ou un JSON étranger se relirait « avec succès » (tous les champs ont une valeur
/// par défaut) et **remettrait tous les réglages à zéro sans rien dire** : on exige donc au
/// moins une section reconnue, et un contenu qui se relit comme `Config`.
pub fn valider_config(texte: &str) -> Result<Config, String> {
    let valeur: serde_json::Value =
        serde_json::from_str(texte).map_err(|e| format!("JSON invalide : {e}"))?;
    let Some(objet) = valeur.as_object() else {
        return Err("ce n'est pas une configuration (objet JSON attendu)".to_string());
    };
    if !SECTIONS_CONFIG.iter().any(|s| objet.contains_key(*s)) {
        return Err(format!(
            "aucune section de KPlayer reconnue (attendu : {})",
            SECTIONS_CONFIG.join(", ")
        ));
    }
    serde_json::from_value::<Config>(valeur).map_err(|e| format!("réglage invalide : {e}"))
}

/// Écrit `config` dans `vers`, au format exact de `config.json` (relisible par `importer_config`).
pub fn exporter_config(config: &Config, vers: &Path) -> Result<(), String> {
    let contenu = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Erreur sérialisation config: {e}"))?;
    std::fs::write(vers, contenu).map_err(|e| format!("Écriture de {} impossible : {e}", vers.display()))
}

/// Importe `depuis` vers `destination` (le `config.json` de l'utilisateur).
///
/// **Le fichier est validé d'abord ; s'il est refusé, `destination` n'est pas touchée.**
/// Une fois accepté, ses octets sont copiés tels quels (écriture dans un fichier voisin puis
/// renommage : une coupure ne laisse jamais une configuration à moitié écrite) — ce qu'on a
/// importé est donc, octet pour octet, ce que le lecteur relira.
pub fn importer_config(depuis: &Path, destination: &Path) -> Result<Config, String> {
    let texte = std::fs::read_to_string(depuis)
        .map_err(|e| format!("Lecture de {} impossible : {e}", depuis.display()))?;
    let config = valider_config(&texte)?;
    if let Some(dossier) = destination.parent() {
        std::fs::create_dir_all(dossier)
            .map_err(|e| format!("Erreur création répertoire config: {e}"))?;
    }
    let temporaire = destination.with_extension("json.import");
    std::fs::write(&temporaire, texte.as_bytes())
        .map_err(|e| format!("Écriture impossible : {e}"))?;
    std::fs::rename(&temporaire, destination).map_err(|e| {
        let _ = std::fs::remove_file(&temporaire);
        format!("Remplacement de la configuration impossible : {e}")
    })?;
    Ok(config)
}

impl Config {
    /// Charger la configuration depuis le fichier utilisateur
    pub fn load() -> Result<Self, String> {
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("kplayer").join("config.json");
            if config_path.exists() {
                match std::fs::read_to_string(&config_path) {
                    Ok(content) => {
                        match serde_json::from_str::<Config>(&content) {
                            Ok(mut config) => {
                                tracing::info!("Configuration chargée depuis: {:?}", config_path);
                                // Un fichier édité à la main peut contenir un nombre de
                                // bandes incorrect ou des gains hors plage : on le
                                // normalise plutôt que de refuser le démarrage.
                                config.audio.sanitize();
                                // Règles de profil : un motif vide ne peut pas
                                // correspondre ; le garder ferait croire à une règle
                                // active dans les préférences.
                                let avant = config.regles.len();
                                config.regles.retain(|r| {
                                    let garde = !r.motif.trim().is_empty()
                                        && !r.profil.trim().is_empty()
                                        && r.champ != ChampRegle::Inconnu;
                                    if !garde {
                                        tracing::warn!(
                                            "Règle de profil écartée (champ/motif/profil inutilisable) : {} → « {} »",
                                            r.resume(),
                                            r.profil
                                        );
                                    }
                                    garde
                                });
                                if config.regles.len() != avant {
                                    tracing::info!(
                                        "{} règle(s) de profil retenue(s) sur {}",
                                        config.regles.len(),
                                        avant
                                    );
                                }
                                return Ok(config);
                            }
                            Err(e) => {
                                tracing::warn!("Erreur parsing config: {}, utilisation des valeurs par défaut", e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Erreur lecture config: {}", e);
                    }
                }
            }
        }

        let default_config = Config::default();
        if let Err(e) = default_config.save() {
            tracing::warn!("Impossible de sauvegarder la config par défaut: {}", e);
        }
        Ok(default_config)
    }

    /// Sauvegarder la configuration dans le fichier utilisateur
    pub fn save(&self) -> Result<(), String> {
        if let Some(config_dir) = dirs::config_dir() {
            let kplayer_dir = config_dir.join("kplayer");
            std::fs::create_dir_all(&kplayer_dir)
                .map_err(|e| format!("Erreur création répertoire config: {}", e))?;

            let config_path = kplayer_dir.join("config.json");
            let content = serde_json::to_string_pretty(self)
                .map_err(|e| format!("Erreur sérialisation config: {}", e))?;

            std::fs::write(&config_path, content)
                .map_err(|e| format!("Erreur écriture config: {}", e))?;

            tracing::info!("Configuration sauvegardée dans: {:?}", config_path);
            Ok(())
        } else {
            Err("Impossible de trouver le répertoire de configuration".to_string())
        }
    }

    /// Développe un chemin contenant `~` (dossier de captures d'écran).
    pub fn expand_path(value: &str) -> PathBuf {
        if let Some(rest) = value.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(rest);
            }
        }
        PathBuf::from(value)
    }

    /// Dossier des captures d'écran, créé si nécessaire.
    pub fn screenshots_dir(&self) -> PathBuf {
        let dir = Self::expand_path(&self.interface.screenshots_dir);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!("Dossier de captures indisponible ({:?}): {}", dir, e);
        }
        dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profil(nom: &str, luminosite: f32) -> Profil {
        let mut video = VideoSettings::default();
        video.brightness = luminosite;
        Profil {
            name: nom.to_string(),
            description: String::new(),
            video,
            audio: AudioSettings::default(),
            subtitles: SubtitleSettings::default(),
        }
    }

    fn regle(champ: ChampRegle, motif: &str, profil: &str) -> Regle {
        Regle {
            champ,
            motif: motif.to_string(),
            profil: profil.to_string(),
        }
    }

    #[test]
    fn une_regle_de_chemin_est_une_sous_chaine_sans_casse() {
        let r = regle(ChampRegle::Chemin, "ANIME", "P");
        assert!(r.correspond(Path::new("/media/films/anime/Serie/x.mkv"), None));
        assert!(!r.correspond(Path::new("/media/films/policier/x.mkv"), None));
        // Un motif vide ne doit jamais correspondre (sinon il capte tout).
        assert!(!regle(ChampRegle::Chemin, "   ", "P").correspond(Path::new("/a/b.mkv"), None));
    }

    #[test]
    fn une_regle_de_chemin_developpe_le_tilde() {
        let Some(home) = dirs::home_dir() else { return };
        let fichier = home.join("Films/essai.mkv");
        let r = regle(ChampRegle::Chemin, "~/Films", "P");
        assert!(r.correspond(&fichier, None), "le ~ doit être développé");
    }

    #[test]
    fn une_regle_d_extension_accepte_listes_et_points() {
        let r = regle(ChampRegle::Extension, ".MKV, avi", "P");
        assert!(r.correspond(Path::new("/films/a.mkv"), None));
        assert!(r.correspond(Path::new("/films/b.AVI"), None));
        assert!(!r.correspond(Path::new("/films/c.mp4"), None));
        // Un fichier sans extension ne correspond à aucune extension.
        assert!(!r.correspond(Path::new("/films/sans_extension"), None));
    }

    #[test]
    fn une_resolution_se_lit_comme_un_seuil_de_largeur() {
        // Un nombre seul = « au moins cette largeur » (lecture d'un écran).
        assert!(regle(ChampRegle::Resolution, "1920", "P")
            .correspond(Path::new("/a.mkv"), Some((3840, 2160))));
        assert!(!regle(ChampRegle::Resolution, "1920", "P")
            .correspond(Path::new("/a.mkv"), Some((1280, 720))));
        assert!(regle(ChampRegle::Resolution, ">=1920", "P")
            .correspond(Path::new("/a.mkv"), Some((1920, 1080))));
        assert!(regle(ChampRegle::Resolution, "> 1280", "P")
            .correspond(Path::new("/a.mkv"), Some((1281, 720))));
        assert!(!regle(ChampRegle::Resolution, ">1280", "P")
            .correspond(Path::new("/a.mkv"), Some((1280, 720))));
        assert!(regle(ChampRegle::Resolution, "<1280", "P")
            .correspond(Path::new("/a.mkv"), Some((720, 576))));
        assert!(!regle(ChampRegle::Resolution, "≤1280", "P")
            .correspond(Path::new("/a.mkv"), Some((1281, 720))));
        assert!(regle(ChampRegle::Resolution, "=1920", "P")
            .correspond(Path::new("/a.mkv"), Some((1920, 800))));
        assert!(regle(ChampRegle::Resolution, "720×576", "P")
            .correspond(Path::new("/a.mkv"), Some((720, 576))));
        assert!(!regle(ChampRegle::Resolution, "720x576", "P")
            .correspond(Path::new("/a.mkv"), Some((720, 480))));
    }

    #[test]
    fn une_resolution_illisible_ne_correspond_jamais() {
        for motif in ["", "beaucoup", "0", "x", "1920x"] {
            assert!(
                !regle(ChampRegle::Resolution, motif, "P")
                    .correspond(Path::new("/a.mkv"), Some((3840, 2160))),
                "motif « {} » ne devrait jamais correspondre",
                motif
            );
        }
        // Sans média ouvert, la résolution est inconnue : aucune décision.
        assert!(!regle(ChampRegle::Resolution, "1920", "P").correspond(Path::new("/a.mkv"), None));
    }

    #[test]
    fn la_premiere_regle_qui_correspond_gagne() {
        let profils = vec![profil("Sombre", 0.3), profil("Net", 0.1)];
        let regles = vec![
            regle(ChampRegle::Chemin, "anime", "Sombre"),
            regle(ChampRegle::Extension, "mkv", "Net"),
        ];
        let choix = regle_correspondante(
            &regles,
            &profils,
            Path::new("/films/anime/ep1.mkv"),
            Some((1920, 1080)),
        )
        .expect("une règle doit correspondre");
        assert_eq!(choix.profil, "Sombre", "l'ordre de la liste doit décider");
    }

    #[test]
    fn une_regle_vers_un_profil_absent_est_ignoree_au_profit_de_la_suivante() {
        let profils = vec![profil("Net", 0.1)];
        let regles = vec![
            regle(ChampRegle::Chemin, "anime", "Supprimé"),
            regle(ChampRegle::Chemin, "anime", "Net"),
        ];
        let choix = regle_correspondante(&regles, &profils, Path::new("/anime/a.mkv"), None)
            .expect("la seconde règle doit prendre le relais");
        assert_eq!(choix.profil, "Net");
    }

    #[test]
    fn un_champ_inconnu_ne_correspond_jamais() {
        let r = regle(ChampRegle::Inconnu, "anime", "P");
        assert!(!r.correspond(Path::new("/anime/a.mkv"), Some((1920, 1080))));
        assert!(r.resume().contains("inconnu"));
    }

    #[test]
    fn appliquer_une_regle_change_reellement_la_configuration() {
        let mut config = Config::default();
        config.video.brightness = 0.0;
        config.regles = vec![regle(ChampRegle::Extension, "mkv", "Sombre")];
        let profils = vec![profil("Sombre", 0.35)];

        let appliquee = appliquer_regle(
            &mut config,
            &profils,
            Path::new("/films/a.mkv"),
            Some((1920, 1080)),
        )
        .expect("la règle doit s'appliquer");
        assert_eq!(appliquee.profil, "Sombre");
        assert!((config.video.brightness - 0.35).abs() < 1e-6);

        // Un média qui ne correspond pas laisse la configuration intacte.
        let mut config2 = Config::default();
        config2.regles = vec![regle(ChampRegle::Extension, "mkv", "Sombre")];
        assert!(appliquer_regle(&mut config2, &profils, Path::new("/a.mp4"), None).is_none());
        assert!((config2.video.brightness - 0.0).abs() < 1e-6);
    }

    #[test]
    fn une_regle_se_relit_depuis_le_json_et_tolere_les_variantes() {
        let json = r#"[
            {"champ": "Chemin", "motif": "anime", "profil": "A"},
            {"champ": "extension", "motif": "mkv", "profil": "B"},
            {"champ": "résolution", "motif": "1920", "profil": "C"},
            {"champ": "dossier-surnom", "motif": "x", "profil": "D"}
        ]"#;
        let regles: Vec<Regle> = serde_json::from_str(json).unwrap();
        assert_eq!(regles[0].champ, ChampRegle::Chemin);
        assert_eq!(regles[1].champ, ChampRegle::Extension);
        assert_eq!(regles[2].champ, ChampRegle::Resolution);
        assert_eq!(regles[3].champ, ChampRegle::Inconnu);
    }

    #[test]
    fn les_reglages_de_sous_titre_ont_des_defauts_surs() {
        // Une configuration écrite avant ces champs doit rester lisible, et les
        // valeurs par défaut doivent reproduire l'ancien aspect (texte centré).
        let s: SubtitleSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.position_h_percent, 50.0, "centre par défaut");
        assert!(s.font_family.is_empty(), "police du lecteur par défaut");
        // Bornes : la position est un pourcentage, elle ne doit pas sortir de [0,100]
        // (le rendu la borne aussi, mais la valeur écrite reste lisible).
        let p: SubtitleSettings =
            serde_json::from_str(r#"{"position_h_percent": 20.0}"#).unwrap();
        assert_eq!(p.position_h_percent, 20.0);
    }

    #[test]
    fn le_precache_reseau_a_une_valeur_par_defaut_et_se_relit() {
        // Défaut : 6 s — une coupure courte passe, sans retarder le démarrage (la
        // réserve se remplit en tâche de fond).
        let defaut: PlaybackSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(defaut.precache_ms, 6_000);
        // Réglage explicite, y compris à zéro (désactivé).
        let zero: PlaybackSettings = serde_json::from_str(r#"{"precache_ms": 0}"#).unwrap();
        assert_eq!(zero.precache_ms, 0);
        // Une ancienne configuration (sans le champ) garde son autre réglage.
        let ancienne: PlaybackSettings =
            serde_json::from_str(r#"{"resume": false}"#).unwrap();
        assert!(!ancienne.resume);
        assert_eq!(ancienne.precache_ms, 6_000);
    }

    #[test]
    fn la_geometrie_de_fenetre_absente_ne_change_rien() {
        // Configuration d'avant la session 32 : aucun champ de géométrie.
        let interface: InterfaceSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(interface.geometrie_de_demarrage(), (None, None));
        assert!(!interface.window_maximized);
        // Et elle se relit quand elle existe.
        let ecrit: InterfaceSettings = serde_json::from_str(
            r#"{"window_pos": [120.0, 80.0], "window_size": [900.0, 500.0],
                "window_maximized": true}"#,
        )
        .unwrap();
        assert_eq!(
            ecrit.geometrie_de_demarrage(),
            (Some([900.0, 500.0]), Some([120.0, 80.0]))
        );
        assert!(ecrit.window_maximized);
    }

    #[test]
    fn une_geometrie_aberrante_est_ignoree_au_demarrage() {
        // Une géométrie aberrante doit retomber sur le comportement par défaut,
        // jamais produire une fenêtre impossible à retrouver à l'écran.
        for json in [
            r#"{"window_pos": [10.0, 10.0], "window_size": [10.0, 10.0]}"#,
            r#"{"window_pos": [10.0, 10.0], "window_size": [900.0, 0.0]}"#,
        ] {
            let interface: InterfaceSettings = serde_json::from_str(json).unwrap();
            assert!(
                interface.geometrie_de_demarrage().0.is_none(),
                "taille acceptée à tort : {}",
                json
            );
        }
        // Position refusée : absurdement loin (fichier modifié à la main).
        let loin: InterfaceSettings =
            serde_json::from_str(r#"{"window_pos": [999999.0, 10.0]}"#).unwrap();
        assert_eq!(loin.geometrie_de_demarrage().1, None);
        // La position seule reste utile (l'utilisateur peut avoir fourni la taille
        // d'une version précédente) : on la garde.
        let partielle: InterfaceSettings =
            serde_json::from_str(r#"{"window_pos": [120.0, 80.0]}"#).unwrap();
        assert_eq!(partielle.geometrie_de_demarrage().1, Some([120.0, 80.0]));
    }

    #[test]
    fn la_session_wayland_est_detectee_sans_se_tromper() {
        // Le type déclaré par le gestionnaire de session tranche.
        assert!(session_wayland(Some("wayland"), Some("wayland-0"), None));
        assert!(!session_wayland(Some("x11"), Some("wayland-0"), Some(":0")));
        // Type absent : Wayland seulement si un compositeur répond ET que X11 est
        // absent (une session X11 renseigne toujours DISPLAY).
        assert!(session_wayland(None, Some("wayland-1"), None));
        assert!(!session_wayland(None, Some("wayland-1"), Some(":0")));
        assert!(!session_wayland(None, None, Some(":0")));
        assert!(!session_wayland(None, Some(""), None));
        // Casse et espaces d'un fichier écrit à la main.
        assert!(session_wayland(Some(" Wayland "), None, None));
    }

    #[test]
    fn une_fenetre_hors_ecran_est_recentree() {
        let ecran = [1920.0, 1080.0];
        // Cas normal : rien ne bouge.
        assert_eq!(
            geometrie_visible([100.0, 50.0], [900.0, 500.0], ecran),
            ([100.0, 50.0], [900.0, 500.0])
        );
        // Second écran débranché : 95 % de la fenêtre hors champ → recentrée.
        let (pos, taille) = geometrie_visible([2500.0, 100.0], [800.0, 450.0], ecran);
        assert_eq!(taille, [800.0, 450.0]);
        assert_eq!(pos, [560.0, 315.0]);
        // Fenêtre plus grande que l'écran : réduite, et position ramenée dedans.
        let (pos, taille) = geometrie_visible([0.0, 0.0], [3000.0, 2000.0], ecran);
        assert_eq!(taille, [1920.0, 1080.0]);
        assert_eq!(pos, [0.0, 0.0]);
        // Sans taille d'écran connue (Wayland, premier tour), on ne touche à rien.
        assert_eq!(
            geometrie_visible([2500.0, 100.0], [800.0, 450.0], [0.0, 0.0]),
            ([2500.0, 100.0], [800.0, 450.0])
        );
    }

    fn dossier_d_essai(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kp-test-{}-{}", nom, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn une_configuration_exportee_se_reimporte_octet_pour_octet() {
        let d = dossier_d_essai("export");
        let mut config = Config::default();
        config.video.brightness = 0.25;
        config.playback.recent.push(PathBuf::from("/x/film.mkv"));
        let export = d.join("export.json");
        exporter_config(&config, &export).unwrap();
        let destination = d.join("conf").join("config.json");
        let relue = importer_config(&export, &destination).unwrap();
        assert!((relue.video.brightness - 0.25).abs() < 1e-6);
        assert_eq!(relue.playback.recent, config.playback.recent);
        assert_eq!(
            std::fs::read(&export).unwrap(),
            std::fs::read(&destination).unwrap(),
            "le fichier importé doit être identique à l'export, octet pour octet"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn un_fichier_refuse_n_ecrase_pas_la_configuration() {
        let d = dossier_d_essai("refus");
        let destination = d.join("config.json");
        std::fs::write(&destination, "ORIGINAL").unwrap();
        for (nom, contenu) in [
            ("vide.json", ""),
            ("texte.json", "pas du json"),
            ("tableau.json", "[1, 2]"),
            ("etranger.json", r#"{"foo": 1}"#),
            ("objet-vide.json", "{}"),
            ("type-faux.json", r#"{"video": {"brightness": "beaucoup"}}"#),
        ] {
            let source = d.join(nom);
            std::fs::write(&source, contenu).unwrap();
            assert!(importer_config(&source, &destination).is_err(), "{nom} aurait dû être refusé");
            assert_eq!(
                std::fs::read_to_string(&destination).unwrap(),
                "ORIGINAL",
                "{nom} : la configuration existante a été touchée"
            );
        }
        assert!(importer_config(&d.join("absent.json"), &destination).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn la_boucle_a_b_se_relit_et_un_ancien_fichier_garde_la_boucle_historique() {
        let l: PlaybackSettings =
            serde_json::from_str(r#"{"ab_repetitions": 3, "ab_delay_ms": 1500}"#).unwrap();
        assert_eq!((l.ab_repetitions, l.ab_delay_ms), (3, 1500));
        let ancienne: PlaybackSettings = serde_json::from_str("{}").unwrap();
        assert_eq!((ancienne.ab_repetitions, ancienne.ab_delay_ms), (0, 0), "0 = sans fin, sans pause");
    }

    #[test]
    fn le_style_des_sous_titres_se_relit_et_un_ancien_fichier_garde_l_aspect() {
        let st: SubtitleSettings = serde_json::from_str(
            r#"{"margin_top_percent": 10.0, "line_spacing": 2.0, "shadow_offset": 4.0,
                "outline2_width": 3.0, "scale_x": 1.5, "fade_ms": 500}"#,
        )
        .unwrap();
        assert_eq!(st.margin_top_percent, 10.0);
        assert_eq!(st.line_spacing, 2.0);
        assert_eq!((st.shadow_offset, st.outline2_width), (4.0, 3.0));
        assert_eq!((st.scale_x, st.scale_y, st.fade_ms), (1.5, 1.0, 500));
        // Un fichier d'avant le lot : l'aspect est exactement l'ancien.
        let ancien: SubtitleSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(ancien.line_spacing, 1.25, "interligne historique");
        assert_eq!((ancien.margin_top_percent, ancien.margin_bottom_percent), (0.0, 0.0));
        assert_eq!((ancien.shadow_offset, ancien.outline2_width), (0.0, 0.0));
        assert_eq!((ancien.scale_x, ancien.scale_y, ancien.fade_ms), (1.0, 1.0, 0));
    }

    #[test]
    fn les_langues_preferees_gardent_leur_priorite_et_retombent_sur_la_langue_par_defaut() {
        let mut st = SubtitleSettings::default();
        st.default_language = "fre".into();
        assert_eq!(st.langues_preferees(), ["fre"], "liste vide : la langue par défaut seule");
        st.preferred_languages = vec![" ENG ".into(), "fre".into(), "eng".into(), "".into()];
        assert_eq!(st.langues_preferees(), ["eng", "fre"], "ordre conservé, doublons et vides écartés");
        let relu: SubtitleSettings = serde_json::from_str(
            r#"{"preferred_languages": ["ger"], "min_duration_ms": 1500, "max_duration_ms": 6000, "remember_track": true}"#,
        )
        .unwrap();
        assert_eq!(relu.langues_preferees(), ["ger"]);
        assert_eq!((relu.min_duration_ms, relu.max_duration_ms, relu.remember_track), (1500, 6000, true));
        let ancien: SubtitleSettings = serde_json::from_str("{}").unwrap();
        assert!(ancien.preferred_languages.is_empty() && !ancien.remember_track);
        assert_eq!((ancien.min_duration_ms, ancien.max_duration_ms), (0, 0));
    }

    #[test]
    fn matrice_plage_et_glisser_se_relisent_et_se_remettent_a_zero() {
        let mut video: VideoSettings = serde_json::from_str(
            r#"{"color_matrix": "Bt709", "color_range": "Full", "pan_x_percent": 12.5, "aspect": "Rogner"}"#,
        )
        .unwrap();
        assert_eq!((video.color_matrix, video.color_range), (ColorMatrix::Bt709, ColorRange::Full));
        assert_eq!(video.pan_x_percent, 12.5);
        assert_eq!(video.aspect, AspectMode::Rogner);
        video.reset_filters();
        assert_eq!((video.color_matrix, video.color_range), (ColorMatrix::Auto, ColorRange::Auto));
        assert_eq!((video.pan_x_percent, video.pan_y_percent), (0.0, 0.0));
        assert_eq!(video.aspect, AspectMode::Auto);
        let ancienne: VideoSettings = serde_json::from_str("{}").unwrap();
        assert_eq!((ancienne.color_matrix, ancienne.color_range), (ColorMatrix::Auto, ColorRange::Auto));
    }

    #[test]
    fn les_recents_sont_ordonnes_sans_doublon_et_bornes() {
        let mut lecture = PlaybackSettings::default();
        assert!(lecture.recent.is_empty());
        for nom in ["a", "b", "c"] {
            assert!(lecture.noter_recent(Path::new(nom)));
        }
        let noms = |l: &PlaybackSettings| -> Vec<String> {
            l.recent.iter().map(|p| p.to_string_lossy().to_string()).collect()
        };
        assert_eq!(noms(&lecture), ["c", "b", "a"], "le plus récent d'abord");
        // Rouvrir « a » : il remonte, rien n'est dupliqué.
        assert!(lecture.noter_recent(Path::new("a")));
        assert_eq!(noms(&lecture), ["a", "c", "b"]);
        // Rouvrir la tête : rien à écrire.
        assert!(!lecture.noter_recent(Path::new("a")));
        // Au-delà de la borne, les plus anciens sortent.
        for i in 0..(RECENTS_MAX + 5) {
            lecture.noter_recent(Path::new(&format!("film-{i}")));
        }
        assert_eq!(lecture.recent.len(), RECENTS_MAX);
        assert_eq!(lecture.recent[0], PathBuf::from(format!("film-{}", RECENTS_MAX + 4)));
    }

    #[test]
    fn les_recents_se_relisent_et_un_ancien_fichier_reste_valide() {
        let lecture: PlaybackSettings =
            serde_json::from_str(r#"{"recent": ["/x/un.mkv", "/x/deux.mkv"]}"#).unwrap();
        assert_eq!(lecture.recent.len(), 2);
        assert_eq!(lecture.recent[0], PathBuf::from("/x/un.mkv"));
        // Configuration antérieure (sans le champ) : liste vide, et le reste garde ses défauts.
        let ancienne: PlaybackSettings = serde_json::from_str(r#"{"resume": false}"#).unwrap();
        assert!(ancienne.recent.is_empty());
        assert!(!ancienne.resume);
        assert_eq!(ancienne.precache_ms, default_precache_ms());
    }

    #[test]
    fn les_nouveaux_reglages_d_image_se_relisent_et_se_remettent_a_zero() {
        // Teinte, niveaux et adoucissement (session 34) : lus depuis un fichier
        // écrit à la main, et remis au neutre par « Revenir aux valeurs neutres » —
        // un bouton qui laisse un filtre actif serait un piège.
        let mut video: VideoSettings = serde_json::from_str(
            r#"{"hue": 45.0, "niveaux_noir": 0.12, "niveaux_blanc": 0.9, "adoucir": 2}"#,
        )
        .unwrap();
        assert!((video.hue - 45.0).abs() < 1e-6);
        assert!((video.niveaux_noir - 0.12).abs() < 1e-6);
        assert!((video.niveaux_blanc - 0.9).abs() < 1e-6);
        assert_eq!(video.adoucir, 2);
        video.reset_filters();
        assert_eq!(video.hue, 0.0);
        assert_eq!(video.niveaux_noir, 0.0);
        assert_eq!(video.niveaux_blanc, 1.0);
        assert_eq!(video.adoucir, 0);
        // Une configuration antérieure (sans ces champs) reste neutre.
        let ancienne: VideoSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(ancienne.hue, 0.0);
        assert_eq!(ancienne.niveaux_noir, 0.0);
        assert_eq!(ancienne.niveaux_blanc, 1.0);
        assert_eq!(ancienne.adoucir, 0);
    }

    #[test]
    fn la_pause_sur_focus_et_le_mix_de_reverbation_se_relisent() {
        // Réglages de la session 34 : deux champs facultatifs, avec des défauts sûrs
        // (aucune pause automatique, 60 % de réflexions).
        let interface: InterfaceSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(interface.pause_sur_perte_focus, 0, "pas de pause par défaut");
        assert!(interface.reprendre_sur_focus);
        let ecrite: InterfaceSettings =
            serde_json::from_str(r#"{"pause_sur_perte_focus": 2, "reprendre_sur_focus": false}"#)
                .unwrap();
        assert_eq!(ecrite.pause_sur_perte_focus, 2);
        assert!(!ecrite.reprendre_sur_focus);

        let audio: AudioSettings = serde_json::from_str("{}").unwrap();
        assert!((audio.reverb_mix - 0.6).abs() < 1e-6);
        // Une valeur hors bornes est ramenée dans [0,1] par `sanitize` (fichier
        // édité à la main, ou écriture interrompue).
        let mut hors: AudioSettings =
            serde_json::from_str(r#"{"reverb_mix": 5.0}"#).unwrap();
        hors.sanitize();
        assert!((hors.reverb_mix - 1.0).abs() < 1e-6);
        let mut absurde: AudioSettings = serde_json::from_str(r#"{"reverb_mix": -3}"#).unwrap();
        absurde.sanitize();
        assert_eq!(absurde.reverb_mix, 0.0);
    }

    #[test]
    fn les_presets_d_egaliseur_sont_tous_dans_la_liste_de_l_interface() {
        // `EqPreset::CHOIX` est la source unique : un preset ajouté au moteur mais
        // absent de la liste resterait invisible dans l'interface (défaut évité en
        // session 34, le compilateur le signalait).
        assert_eq!(EqPreset::CHOIX.len(), 7);
        for p in EqPreset::CHOIX {
            assert_eq!(p.gains().len(), EQ_BANDS, "courbe incomplète pour {:?}", p);
            assert!(!p.label().is_empty());
            // Les gains restent dans les bornes du moteur ([0, 20], cf. `sanitize`).
            for g in p.gains() {
                assert!(g > 0.0 && g <= 20.0);
            }
        }
        // Deux presets ne doivent pas être identiques : sinon l'un ne sert à rien.
        let mut vues: Vec<Vec<u32>> = Vec::new();
        for p in EqPreset::CHOIX {
            let signature: Vec<u32> = p.gains().iter().map(|g| (g * 100.0) as u32).collect();
            assert!(!vues.contains(&signature), "preset en double : {:?}", p);
            vues.push(signature);
        }
    }

    #[test]
    fn une_configuration_sans_champ_regles_se_lit_quand_meme() {
        // Compatibilité : les fichiers écrits avant l'ajout des règles n'ont pas
        // ce champ et ne doivent pas perdre le reste de leurs réglages.
        let config: Config = serde_json::from_str(r#"{"video": {"brightness": 0.2}}"#).unwrap();
        assert!(config.regles.is_empty());
        assert!((config.video.brightness - 0.2).abs() < 1e-6);
    }
}

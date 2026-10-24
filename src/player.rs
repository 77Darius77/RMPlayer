//! Moteur de lecture vidéo — FFmpeg 7.x (via ffmpeg-sys-next)
//!
//! Architecture :
//!   - ffmpeg-sys-next pour les bindings FFI FFmpeg 7.x (structs réelles)
//!   - Décodage matériel VA-API (libva/Mesa) avec repli logiciel automatique
//!   - Gestion des streams vidéo / audio / sous-titres
//!   - Conversion couleur via libswscale → RGBA pour egui

use std::collections::VecDeque;
use std::path::PathBuf;
use std::ffi::CString;
use std::os::raw::c_int;
use std::time::Duration;

use ffmpeg_sys_next::*;

/// Une frame vidéo décodée, prête à l'affichage (RGBA, 4 octets/pixel).
pub struct VideoFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub pts_ms: u64,
}

/// Une piste audio ou de sous-titres présente dans le conteneur.
///
/// Les pistes embarquées étaient ignorées : le moteur prenait le **premier** flux
/// audio et aucun sous-titre du conteneur. Or un MKV courant contient plusieurs
/// langues audio et plusieurs pistes de sous-titres ; ne pas les exposer oblige
/// l'utilisateur à remuxer le fichier.
#[derive(Debug, Clone)]
pub struct TrackInfo {
    /// Index du flux dans le conteneur (identifiant stable pour la sélection).
    pub index: i32,
    /// Nom lisible du codec (« AAC », « SubRip »…).
    pub codec: String,
    /// Métadonnée `language` du conteneur (`fre`, `eng`, `und`…).
    pub language: String,
    /// Métadonnée `title` (« VF », « Commentaire du réalisateur »…).
    pub title: String,
    /// Audio : nombre de canaux. Sous-titres : 0.
    pub channels: i32,
    /// Audio : taux d'échantillonnage. Sous-titres : 0.
    pub sample_rate: i32,
    /// Sous-titres **image** (PGS, DVD) : il n'y a pas de texte à extraire, la piste
    /// ne peut pas être affichée par le lecteur — l'interface le dit explicitement
    /// au lieu d'afficher un silence inexpliqué.
    pub bitmap: bool,
}

impl TrackInfo {
    /// Libellé pour l'interface : « fre — VF (AAC, 6 canaux) ».
    pub fn label(&self) -> String {
        let mut parties: Vec<String> = Vec::new();
        if !self.language.is_empty() && self.language != "und" {
            parties.push(self.language.clone());
        }
        if !self.title.is_empty() {
            parties.push(self.title.clone());
        }
        let details = if self.bitmap {
            format!("{}, image", self.codec)
        } else if self.channels > 0 {
            if self.sample_rate > 0 {
                format!(
                    "{}, {} canaux, {:.1} kHz",
                    self.codec,
                    self.channels,
                    self.sample_rate as f32 / 1000.0
                )
            } else {
                format!("{}, {} canaux", self.codec, self.channels)
            }
        } else {
            self.codec.clone()
        };
        parties.push(details);
        parties.join(" — ")
    }
}

/// Un chapitre du conteneur (MKV, MP4, DVD…).
///
/// Les chapitres sont la seule navigation « éditoriale » d'un film : sans eux, il faut
/// chercher à l'aveugle dans la barre de progression. Ils sont lus une fois au
/// chargement (ils ne changent pas en cours de lecture).
#[derive(Debug, Clone)]
pub struct Chapter {
    pub start_ms: u64,
    pub end_ms: u64,
    /// Titre éditorial (« Chapitre 3 », « Générique »…). Vide si absent.
    pub title: String,
}

/// Un sous-titre extrait du conteneur, prêt à l'affichage.
#[derive(Debug, Clone)]
pub struct EmbeddedCue {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

/// Une image de sous-titre prête à composer : pixels RGBA non prémultipliés.
///
/// Les pistes **image** (PGS des Blu-ray, sous-images DVD, DVB) n'ont pas de
/// texte : le décodeur rend un ou plusieurs rectangles d'indices de palette. On
/// les convertit ici en RGBA, une fois par événement, pour que l'affichage n'ait
/// qu'à déposer une texture.
#[derive(Clone)]
pub struct BitmapRect {
    /// Coin supérieur gauche dans le repère de la vidéo (pixels du média).
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` octets, ligne du haut d'abord.
    pub rgba: Vec<u8>,
}

/// Budget mémoire des sous-titres image gardés en mémoire.
///
/// Une composition plein écran en 1080p pèse **8 Mo** (1920 × 1080 × 4). Borner
/// par un *nombre* d'événements ne suffit donc pas : 600 événements vaudraient
/// plusieurs gigaoctets. On borne par le volume, en oubliant les plus anciens —
/// qui ne peuvent plus s'afficher, l'extinction étant arrivée.
const MAX_BITMAP_BYTES: usize = 64 * 1024 * 1024;

/// Nombre maximal d'événements image conservés (garde-fou secondaire : beaucoup
/// de petites sous-images DVD tiennent dans le budget d'octets).
const MAX_BITMAP_CUES: usize = 300;

/// Taille d'un événement, en octets de pixels.
fn bitmap_cue_bytes(c: &BitmapCue) -> usize {
    c.rects.iter().map(|r| r.rgba.len()).sum()
}

/// Un événement de sous-titre image (une composition PGS, une sous-image DVD).
#[derive(Clone)]
pub struct BitmapCue {
    pub start_ms: u64,
    /// Fin d'affichage ; **0 = jusqu'au prochain événement** (cas d'un direct PGS,
    /// où l'extinction est un événement à part entière).
    pub end_ms: u64,
    pub rects: Vec<BitmapRect>,
}

/// Convertit un rectangle d'indices de palette en RGBA.
///
/// La palette livrée par FFmpeg est en **0xAARRGGBB** — mesuré sur ce build pour
/// les trois formats : PGS (`fffe0000` pour du rouge), sous-image DVD et DVB.
/// FFmpeg a déjà fait la conversion YCbCr → RGB du format d'origine ; il ne reste
/// donc qu'à distribuer les octets.
///
/// `linesize` peut dépasser `width` (lignes alignées) : on lit ligne par ligne.
fn bitmap_vers_rgba(
    indices: &[u8],
    linesize: usize,
    width: u32,
    height: u32,
    palette: &[u32],
) -> Option<Vec<u8>> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 {
        return None;
    }
    if indices.len() < linesize.saturating_mul(h.saturating_sub(1)) + w {
        return None;
    }
    let mut rgba = vec![0u8; w * h * 4];
    for y in 0..h {
        let ligne = y * linesize;
        for x in 0..w {
            let index = indices[ligne + x] as usize;
            let argb = match palette.get(index) {
                // Sans palette (rare) : blanc opaque plutôt que rien — un
                // sous-titre invisible serait impossible à diagnostiquer.
                None => 0xFFFF_FFFFu32,
                Some(v) => *v,
            };
            let a = (argb >> 24) as u8;
            // Un pixel entièrement transparent est laissé à zéro : cela évite de
            // composer une couleur parasite sur la vidéo.
            if a == 0 {
                continue;
            }
            let o = (y * w + x) * 4;
            rgba[o] = (argb >> 16) as u8;
            rgba[o + 1] = (argb >> 8) as u8;
            rgba[o + 2] = argb as u8;
            rgba[o + 3] = a;
        }
    }
    Some(rgba)
}

/// Informations sur le média (exposé à l'UI)
#[derive(Debug, Clone)]
pub struct MediaInfo {
    pub filename: String,
    pub resolution: (u32, u32),
    pub fps: f32,
    pub bitrate: u64,
    pub video_codec: String,
    pub audio_codec: String,
    /// Toutes les pistes audio du conteneur (au moins une si le média en a).
    pub audio_tracks: Vec<TrackInfo>,
    /// Toutes les pistes de sous-titres du conteneur.
    pub subtitle_tracks: Vec<TrackInfo>,
    /// Chapitres, dans l'ordre chronologique.
    pub chapters: Vec<Chapter>,
}

/// Bornes de vitesse de lecture, **source unique** : le moteur, le menu et les
/// préférences les partagent (aucune constante ne le garantissait auparavant).
pub const VITESSE_MIN: f32 = 0.1;
pub const VITESSE_MAX: f32 = 8.0;

/// État du lecteur
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerState {
    Idle,
    Loading,
    Playing,
    Paused,
    Stopped,
    Error(String),
}

struct VideoStream {
    index: i32,
    codec_context: *mut AVCodecContext,
    scale_context: *mut SwsContext,
    /// Format d'entrée du contexte swscale courant (recalculé si le flux change)
    scale_format: AVPixelFormat,
    /// Méthode d'interpolation utilisée pour le contexte courant
    scale_method: crate::config::ScalingMethod,
    /// Dimensions source et destination du contexte swscale courant
    scale_src_width: i32,
    scale_src_height: i32,
    scale_dst_width: i32,
    scale_dst_height: i32,
    /// Décodage matériel DEMANDÉ pour ce flux (décodeur ouvert avec le périphérique
    /// VA-API) — ce n'est pas encore une preuve d'usage.
    hw_requested: bool,
    /// Décodage matériel CONFIRMÉ : au moins une frame VA-API a été produite. C'est
    /// cette valeur qui est rapportée à l'interface, car libavcodec peut retomber
    /// silencieusement sur le logiciel (profil refusé par le pilote).
    hw_active: bool,
    /// Vrai après avoir signalé une fois le repli effectif en logiciel
    hw_fallback_logged: bool,
    /// Vrai si au moins une image a été réduite par le GPU (`scale_vaapi`) :
    /// preuve que l'optimisation est réellement active, journalisée une fois.
    echelle_gpu_active: bool,
    /// Réglage `video.gpu_scaling` : permet de mesurer l'écart et de revenir en
    /// arrière sur un pilote qui se comporterait mal.
    echelle_gpu_voulue: bool,
    /// Vrai si ce codec sait utiliser l'accélération VA-API (message d'erreur utile)
    codec_hw_capable: bool,
    /// Nombre d'échecs de rapatriement d'une surface matérielle (diagnostic)
    hw_failures: u32,
    /// Chaîne de filtres libavfilter courante (créée au premier usage)
    filters: Option<Box<VideoFilterChain>>,
    /// Chaîne de filtres **GPU** persistante (réduction et/ou désentrelacement).
    gpu_chain: Option<Box<GpuChain>>,
    /// Signature d'une chaîne qui a ÉCHOUÉ : on ne réessaie pas à chaque image
    /// (une description invalide ferait sinon 25 tentatives de construction par
    /// seconde, et noierait le journal).
    filters_failed_signature: Option<String>,
    time_base: AVRational,
    /// Métadonnées de couleur du flux (transfert, primaires, profondeur).
    hdr_info: Option<crate::hdr::Info>,
    /// Étage HDR décidé pour ce flux (vide = aucun). Conservé pour le diagnostic et
    /// l'affichage : c'est ce que l'interface annonce.
    hdr_etage: String,
    /// L'étage HDR a-t-il dû être abandonné (Vulkan absent, chaîne refusée) ?
    hdr_indisponible: bool,
    /// Étage CPU (`zscale` + `tonemap`) du même plan, prêt à remplacer l'étage GPU si
    /// Vulkan est refusé. Vide quand le plan nominal n'est pas GPU.
    hdr_etage_cpu: String,
    /// Vrai quand l'étage GPU a été refusé et que l'étage CPU l'a remplacé.
    hdr_repli_cpu: bool,
    /// Pourquoi le repli CPU a été refusé (source trop lourde) ; vide sinon.
    hdr_repli_motif: String,
    /// Cadence annoncée par le flux (i/s) : sert au budget du repli CPU.
    fps_flux: f32,
    /// Avis à afficher une fois à l'écran (changement d'état HDR subi par l'utilisateur).
    hdr_avis: Option<String>,
}

/// Décodeur d'une piste de sous-titres du conteneur.
///
/// Les sous-titres ne sont pas « lus » comme la vidéo ou l'audio : chaque paquet
/// contient un ou plusieurs événements avec des durées **relatives** au paquet.
/// Le décodeur les convertit en texte (ou en image pour le PGS).
struct SubtitleStream {
    index: i32,
    codec_context: *mut AVCodecContext,
    time_base: AVRational,
    /// Vrai si la piste est en image (PGS/DVD) : aucun texte à afficher.
    bitmap: bool,
}

struct AudioStream {
    index: i32,
    codec_context: *mut AVCodecContext,
    /// Convertisseur vers f32 entrelacé (format de sortie audio)
    swr: *mut SwrContext,
    /// Disposition de canaux de sortie (utilisée par swr)
    out_layout: AVChannelLayout,
    out_sample_rate: i32,
    /// Base de temps du stream audio (conversion PTS → ms)
    time_base: AVRational,
}

/// Chaîne de filtres FFmpeg **audio** (`abuffer → superequalizer → abuffersink`).
///
/// Placée après la conversion vers le format du périphérique (f32 entrelacé), donc
/// sur le chemin réel de l'écoute : ce que le filtre produit est exactement ce qui
/// part vers la carte son. Format d'entrée = format de sortie (f32, même taux, mêmes
/// canaux), ce qui évite toute conversion supplémentaire.
struct AudioFilterChain {
    graph: *mut AVFilterGraph,
    src: *mut AVFilterContext,
    sink: *mut AVFilterContext,
    /// Description + taux + canaux : toute différence impose de reconstruire.
    signature: String,
    sample_rate: i32,
    channels: i32,
    /// Échantillons entrés / sortis : contrôle d'intégrité (le filtre ne doit ni
    /// perdre ni inventer d'échantillons).
    samples_in: u64,
    samples_out: u64,
}

impl Drop for AudioFilterChain {
    fn drop(&mut self) {
        unsafe {
            if !self.graph.is_null() {
                let mut g = self.graph;
                avfilter_graph_free(&mut g);
                self.graph = std::ptr::null_mut();
            }
        }
    }
}

/// Chaîne de filtres FFmpeg (`libavfilter`) appliquée entre le décodage et la
/// conversion RGBA : détramage, débruitage, désentrelacement, réglages d'image.
///
/// **Pourquoi un graphe de filtres et pas libswscale** : ces traitements (gradient
/// adaptatif, moyennage temporel, interpolation de champ, courbe de transfert)
/// n'existent pas dans libswscale. Ils sont fournis par les filtres `deband`,
/// `hqdn3d`, `yadif` et `eq` de libavfilter.
///
/// **Choix important : uniquement des filtres 1 frame → 1 frame.** Le moteur avance
/// d'une image par tour d'affichage ; un filtre qui change la cadence (par exemple
/// `yadif` en mode `send_field`, qui double le nombre d'images) casserait la
/// synchronisation. `yadif` est donc utilisé en mode `send_frame` avec
/// `deint=1` (seules les trames marquées entrelacées sont traitées).
struct VideoFilterChain {
    graph: *mut AVFilterGraph,
    src: *mut AVFilterContext,
    sink: *mut AVFilterContext,
    /// Description + géométrie + format + base de temps du graphe courant : toute
    /// différence (réglage modifié, format qui change en cours de flux) impose
    /// de reconstruire le graphe.
    signature: String,
    /// Nombre de frames ressorties par le graphe (doit rester égal au nombre entré).
    frames_in: u64,
    frames_out: u64,
}

impl Drop for VideoFilterChain {
    fn drop(&mut self) {
        unsafe {
            if !self.graph.is_null() {
                let mut g = self.graph;
                avfilter_graph_free(&mut g);
                self.graph = std::ptr::null_mut();
            }
        }
    }
}

/// Fin de flux annoncée par FFmpeg (`AVERROR_EOF`, fourcc « EOF » ; macro côté C).
/// Ne pas la confondre avec une coupure : l'une signifie « le média est fini »,
/// l'autre « la source ne répond plus ».
const AVERROR_EOF: i32 = -541_478_725;

/// Attente maximale d'une opération réseau, en microsecondes (3 s).
///
/// C'est aussi la **borne du blocage de l'interface** : la lecture d'un paquet se
/// fait dans le fil d'interface (le démultiplexeur n'est pas déporté dans un
/// thread), donc une source muette fige la boucle d'événements au plus ce délai,
/// une fois, avant que la coupure ne soit constatée et la reconnexion lancée.
/// 10 s (valeur initiale) donnait un gel perceptible ; 3 s laisse largement le
/// temps à un serveur correct de répondre, même sur un réseau lent.
const RESEAU_TIMEOUT_US: &str = "3000000";

/// Attente maximale de la sonde d'accessibilité (4 s), en microsecondes.
const SONDE_TIMEOUT_US: &str = "4000000";

/// Marge (ms) en deçà de la fin d'un média en dessous de laquelle un EOF de flux
/// réseau est considéré comme une fin normale plutôt que comme une coupure.
const FIN_DE_FLUX_MARGE_MS: u64 = 3_000;

/// Nombre de tentatives de reconnexion consécutives avant d'abandonner.
const RESEAU_TENTATIVES_MAX: u32 = 8;

/// Délai avant la première tentative, puis doublement à chaque échec
/// (0,3 s → 0,6 → 1,2 → 2,4 → 4,8 → 8 → 8 → 8 s).
fn delai_reconnexion(tentative: u32) -> Duration {
    let ms = 300.0 * 2f64.powi(tentative.saturating_sub(1) as i32);
    Duration::from_millis(ms.min(8000.0) as u64)
}

/// Enveloppe minimale pour confier à un thread un pointeur FFmpeg.
///
/// Sûreté : la sonde ouvre son **propre** contexte, l'utilise seule, puis le
/// ferme. Aucun autre thread n'y touche — le pointeur ne sort jamais du thread
/// qui l'a créé, il ne sert qu'à satisfaire le compilateur.
struct PointeurFfmpeg(*mut AVFormatContext);
unsafe impl Send for PointeurFfmpeg {}

/// État d'un flux réseau, qui permet de le rouvrir après une coupure.
struct NetworkStream {
    /// Adresse d'origine (rouverte telle quelle).
    url: String,
    /// Playlist HLS (`.m3u8`) : le conteneur peut être relu depuis le début, et
    /// un direct n'a évidemment pas de position de reprise.
    hls: bool,
    /// Tentatives consécutives déjà effectuées.
    tentatives: u32,
    /// Instant de la prochaine tentative (aucune tant qu'il n'y a pas de panne).
    prochaine_a: Option<std::time::Instant>,
    /// En panne : la lecture est suspendue jusqu'à la prochaine tentative. Évite
    /// de relire en boucle une source qui ne répond pas.
    en_panne: bool,
    /// Dernier code d'erreur observé, pour le message final.
    derniere_erreur: Option<i32>,
    /// Sonde d'accessibilité en cours : ouvre l'URL dans un **thread**, sinon un
    /// serveur muet (connexion acceptée, aucune réponse) figerait l'interface
    /// pendant tout le délai d'attente, à chaque tentative.
    sonde: Option<std::thread::JoinHandle<bool>>,
}

impl NetworkStream {
    fn nouveau(url: &str) -> Self {
        let minuscule = url.to_lowercase();
        let chemin = minuscule.split(['?', '#']).next().unwrap_or("");
        Self {
            url: url.to_string(),
            hls: chemin.ends_with(".m3u8") || chemin.ends_with(".m3u"),
            tentatives: 0,
            prochaine_a: None,
            en_panne: false,
            derniere_erreur: None,
            sonde: None,
        }
    }
}

/// Ce que `read_packet_into_decoder` doit faire d'un paquet vidéo.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReadMode {
    /// Décoder tout de suite en image affichable (chemin de l'affichage).
    Frame,
    /// Ne pas décoder : garder le paquet pour plus tard. Utilisé quand on lit des
    /// paquets uniquement pour alimenter le tampon audio — décoder en avance
    /// empilerait des images de plusieurs mégaoctets et bloquerait l'alimentation.
    AudioOnly,
}

/// Nombre maximal de paquets vidéo mis en attente (garde-fou mémoire ; un
/// paquet pèse quelques dizaines de kilo-octets).
const MAX_QUEUED_PACKETS: usize = 240;

/// Plafond mémoire des paquets vidéo en attente, en octets.
///
/// Le nombre de paquets ne suffit pas comme garde-fou : à 4K, un paquet peut peser
/// plusieurs centaines de kilo-octets. 48 Mo couvrent largement 6 s de 1080p et
/// bornent le cas 4K — c'est la contrepartie assumée du pré-cache.
const MAX_QUEUE_BYTES: usize = 48 * 1024 * 1024;

/// Bornes **de sécurité** de la lecture anticipée pendant l'alimentation du
/// tampon audio. Elles ne doivent jamais gêner le fonctionnement normal : dans
/// un conteneur entrelacé, remplir 120 ms de son demande 3 ou 4 images d'avance.
/// Elles protègent seulement d'un conteneur qui stockerait tout le son très loin
/// de l'image (chaque image en attente occupe un cadre RGBA à la taille
/// d'affichage : 8 Mo en 1080p).
const MAX_PENDING_VIDEO: usize = 24;
/// Avance média maximale (ms) de l'image décodée sur l'image **affichée**.
/// Mesurée sur la position affichée et non sur l'horloge : quand l'horloge audio
/// se bloque, c'est précisément le moment où il faut continuer à alimenter le
/// tampon — s'y référer bloquerait l'alimentation et figerait tout.
const MAX_AVANCE_VIDEO_MS: u64 = 300;

/// Délai (ms) sans progression de l'horloge audio au-delà duquel on considère que
/// la carte son ne consomme plus (périphérique suspendu, piste terminée, tampon
/// qui ne se remplit plus) et où l'on revient au temps mural pour ne pas figer
/// l'image. Très supérieur à la période des blocs audio (une vingtaine de ms),
/// pour ne jamais se déclencher sur un simple à-coup.
const SILENCE_HORLOGE_MS: u128 = 250;

pub struct VideoPlayer {
    format_context: Option<*mut AVFormatContext>,
    video_stream: Option<VideoStream>,
    audio_stream: Option<AudioStream>,
    /// Sortie audio matérielle (cpal) + tampon circulaire (créée une seule fois)
    audio_output: Option<crate::audio::AudioOutput>,
    /// Vrai si aucun périphérique audio n'est disponible
    audio_disabled: bool,
    /// Le tampon a-t-il été rempli avant de laisser la carte consommer ?
    /// Remis à `false` à chaque média et à chaque seek (le tampon est vidé).
    audio_prefill_fait: bool,
    /// Depuis quand attend-on ce préremplissage (borne de sécurité : il ne doit
    /// jamais retarder la lecture au-delà de quelques centaines de millisecondes).
    audio_prefill_depuis: Option<std::time::Instant>,
    /// Profondeur visée du pré-cache des flux réseau (ms) ; 0 = désactivé.
    /// Un fichier local ne se pré-cache pas : le disque ne prend pas de retard.
    precache_ms: u64,
    /// PTS du dernier paquet vidéo mis en attente : avec la position courante, il
    /// donne l'avance réelle du pré-cache (c'est la mesure qui compte, pas le
    /// nombre de paquets : un GOP long et un GOP court n'ont pas le même poids).
    queue_pts_max_ms: u64,
    /// Octets de paquets en attente : second garde-fou, en mémoire cette fois.
    queue_octets: usize,
    /// Le préchargement a rencontré la fin du flux (ou une erreur) alors que des
    /// paquets attendent encore d'être joués : la décision « fin » ou « coupure » est
    /// **reportée** au moment où la lecture y arrive réellement. Sans cela, lire en
    /// avance faisait conclure à une fin de média (ou à une coupure) plusieurs
    /// secondes trop tôt, alors qu'il restait du contenu en réserve.
    prefetch_fini: bool,
    /// Vrai pendant un tour de `precharger_reseau` : les erreurs d'E/S y sont
    /// mémorisées, pas traitées.
    prefetch_en_cours: bool,
    subtitle_stream: Option<i32>,
    /// Contexte de périphérique VA-API (créé une fois, partagé par les médias)
    hw_device: Option<*mut AVBufferRef>,
    /// Vrai si le décodage matériel est autorisé par la configuration
    hwdec_enabled: bool,
    /// Réglage `video.gpu_scaling`, appliqué au média courant et aux suivants.
    gpu_scaling_enabled: bool,
    /// Copie des réglages vidéo : sert à décider si la chaîne de filtres demandée a
    /// un équivalent matériel (`filtre_gpu_equivalent`).
    config_video: crate::config::VideoSettings,
    /// Méthode de mise à l'échelle appliquée par libswscale (réglage réel)
    scaling_method: crate::config::ScalingMethod,
    /// Volume maximal autorisé (pourcentage) — appliqué au volume courant
    volume_max: u16,
    /// Nom du périphérique audio demandé ("" = défaut)
    audio_device: String,
    /// Nombre consécutif de paquets vidéo décodés sans qu'une image soit produite
    video_decode_failures: u32,
    /// Point A de la répétition A-B (ms)
    loop_a_ms: Option<u64>,
    /// Point B de la répétition A-B (ms)
    loop_b_ms: Option<u64>,
    /// Répétition A-B : retours autorisés avant de lever la boucle (0 = sans fin).
    ab_repetitions: u32,
    /// Répétition A-B : pause (ms) entre l'arrivée en B et le retour en A.
    ab_delay_ms: u64,
    /// Retours déjà faits sur la boucle courante.
    ab_retours: u32,
    /// Instant où la pause de fin de boucle prend fin (le retour en A a déjà eu lieu).
    ab_reprise_a: Option<std::time::Instant>,
    /// Reprise de lecture à la position quittée
    resume_enabled: bool,
    /// Positions mémorisées par média
    positions: crate::positions::Positions,
    /// Taille d'affichage souhaitée (pixels) pour la conversion des frames ;
    /// (0, 0) = taille native du média. Renseignée par l'interface.
    display_target: (u32, u32),
    /// Taille imposée à `dump_frame_at` (0,0 = résolution native).
    dump_taille: (u32, u32),
    state: PlayerState,
    current_position_ms: u64,
    total_duration_ms: u64,
    last_frame_dims: (u32, u32),
    fps: f32,
    /// Frames vidéo décodées en attente d'affichage
    pending_video: VecDeque<VideoFrame>,
    /// Paquets vidéo lus pendant l'alimentation du tampon audio, pas encore
    /// envoyés au décodeur (l'ordre du fichier est préservé).
    queued_video_packets: VecDeque<*mut AVPacket>,
    /// Flux réseau en cours (avec son état de reconnexion) ; `None` pour un fichier.
    network: Option<NetworkStream>,
    /// Garde-temps des E/S du flux courant. Détenu ici pour que le pointeur
    /// confié à FFmpeg reste valide tant que le contexte de format existe.
    delai_reseau: Option<std::sync::Arc<DelaiReseau>>,
    /// Fin de flux atteinte
    eof: bool,
    /// Horloge de lecture murale (repli quand aucune horloge audio n'est
    /// disponible) : instant de départ et PTS de référence.
    clock_start: Option<std::time::Instant>,
    /// PTS de la première frame affichée (référence de l'horloge murale)
    clock_base_pts_ms: u64,
    /// Dernière valeur connue de l'horloge audio (ms) et instant où elle a
    /// avancé : servent au garde-fou « l'horloge audio ne bouge plus ».
    audio_clock_seen_ms: Option<f64>,
    audio_clock_seen_at: Option<std::time::Instant>,
    /// L'horloge audio est muette : on est revenu au temps mural pour la cadence.
    audio_clock_stalled: bool,
    /// Profondeur visée du tampon audio (ms) — réglable par `--audio-buffer`
    /// pour vérifier que la profondeur n'influe pas sur la synchronisation.
    audio_buffer_ms: u64,
    /// Fenêtre du bilan d'horloge (une ligne de journal par seconde).
    clock_report_at: Option<std::time::Instant>,
    buffered_min_ms: u64,
    buffered_max_ms: u64,
    /// Incrémenté à chaque changement de média / seek / reprise, pour que l'UI
    /// sache qu'il faut afficher une frame immédiatement.
    generation: u64,
    /// Cible d'un seek en cours : les frames (et l'audio) antérieures sont
    /// décodées puis jetées pour arriver précisément à la position demandée.
    seek_target_ms: Option<u64>,
    speed: f32,
    volume: f32,
    muted: bool,
    pub subtitles_visible: bool,
    pub loop_enabled: bool,
    /// Lecture aléatoire : l'ordre de passage est mélangé une fois, puis suivi.
    ///
    /// **Pourquoi un ordre mélangé plutôt qu'un tirage à chaque fois** : un tirage
    /// uniforme indépendant rejoue le même élément plusieurs fois de suite et peut
    /// ignorer la moitié de la liste (c'est le hasard ; il en a le droit, mais cela
    /// ressemble à un défaut). Ici chaque élément passe **une fois** avant qu'un
    /// cycle recommence — comportement attendu d'un lecteur de bureau.
    shuffle: bool,
    /// Permutation des indices de la playlist, et rang courant dedans.
    ordre_aleatoire: Vec<usize>,
    rang_aleatoire: usize,
    pub playlist: Vec<PathBuf>,
    playlist_index: usize,
    /// Chemin du média réellement ouvert (indépendant de la playlist : un fichier
    /// peut être lu sans y figurer).
    current_path: Option<PathBuf>,
    media_info: Option<MediaInfo>,
    /// Décodeur et événements de la piste de sous-titres embarquée sélectionnée.
    subtitle_decoder: Option<SubtitleStream>,
    /// Événements extraits (ordre chronologique), bornés pour ne pas croître
    /// indéfiniment sur un film de deux heures.
    embedded_cues: VecDeque<EmbeddedCue>,
    /// Événements de sous-titres **image** (PGS/DVD/DVB), même ordre.
    bitmap_cues: VecDeque<BitmapCue>,
    /// Langue audio demandée ("" = première piste, comportement historique).
    audio_language: String,
    /// Utiliser les sous-titres du conteneur quand il n'y a pas de fichier externe.
    prefer_embedded_subs: bool,
    /// Langue de sous-titres demandée ("" = première piste textuelle).
    subtitle_language: String,
    /// Chaîne de filtres audio (égaliseur). Créée à la première frame audio
    /// lorsque l'égaliseur est actif.
    audio_filters: Option<Box<AudioFilterChain>>,
    /// Signature d'une chaîne audio refusée : on ne réessaie pas à chaque paquet.
    audio_filters_failed: Option<String>,
    /// Description de la chaîne de filtres audio active ("" = aucune).
    audio_filters_description: String,
    /// Copie des réglages audio : la chaîne doit pouvoir être recalculée à tout
    /// moment (vitesse, format du périphérique) sans dépendre de l'interface.
    audio_settings: crate::config::AudioSettings,
    /// Vrai si la vitesse doit préserver la hauteur du son.
    audio_pitch_preserved: bool,
    /// Description de la chaîne de filtres vidéo active ("" = aucune).
    /// Construite à partir des réglages par `filter_description` : c'est le seul
    /// endroit où les réglages deviennent une suite de filtres FFmpeg, ce qui rend
    /// la traduction vérifiable en une ligne de journal.
    filters_description: String,
    /// Copie des réglages vidéo courants (voir `set_video_filters`).
    video_settings: crate::config::VideoSettings,
    /// Aperçu du recadrage : les filtres sont calculés SANS le recadrage, pour que
    /// le repère dessiné par l'interface montre ce qui sera réellement conservé.
    crop_apercu: bool,
}

impl VideoPlayer {
    pub fn new(config: crate::config::Config) -> Self {
        tracing::info!("FFmpeg player initialisé (bindings ffmpeg-sys-next)");

        let hwdec_enabled = config.video.hardware_decoding;
        let gpu_scaling_enabled = config.video.gpu_scaling;
        let config_video = config.video.clone();
        let scaling_method = config.video.scaling_method.clone();
        let volume_max = config.audio.volume_max.clamp(100, 300);
        let audio_device = config.audio.device.clone();
        let resume_enabled = config.playback.resume;
        let audio_settings = config.audio.clone();
        let audio_pitch_preserved = config.audio.pitch_preserved;
        // Vitesse initiale = 1,0 (la vitesse n'est pas persistée : c'est un réglage
        // de séance). Le format du périphérique est inconnu ici ; on prend 2 canaux
        // et la chaîne est reconstruite dès la première trame audio de toute façon.
        let audio_filters_description =
            audio_filter_description(&config.audio, 1.0, audio_pitch_preserved, 2);
        let filters_description = filter_description(&config.video);
        if !filters_description.is_empty() {
            tracing::info!("Chaîne de filtres vidéo: {}", filters_description);
        }
        let hw_device = if hwdec_enabled {
            unsafe { create_vaapi_device() }
        } else {
            tracing::info!("Décodage matériel désactivé par la configuration");
            None
        };

        unsafe {
            avformat_network_init();
            // Les messages d'FFmpeg partent par défaut sur la sortie d'erreur, où ils
            // noient les journaux de l'application (un flux AV1 refusé en produit des
            // centaines). On les redirige vers `tracing`, au niveau « debug » : ils
            // restent accessibles avec `RUST_LOG=kplayer=debug` (ou `ffmpeg=debug`).
            av_log_set_callback(Some(ffmpeg_log_callback));
        }

        Self {
            format_context: None,
            video_stream: None,
            audio_stream: None,
            audio_output: None,
            audio_disabled: false,
            audio_prefill_fait: false,
            audio_prefill_depuis: None,
            precache_ms: config.playback.precache_ms,
            queue_pts_max_ms: 0,
            queue_octets: 0,
            prefetch_fini: false,
            prefetch_en_cours: false,
            subtitle_stream: None,
            hw_device,
            hwdec_enabled,
            gpu_scaling_enabled,
            config_video,
            scaling_method,
            volume_max,
            audio_device,
            video_decode_failures: 0,
            loop_a_ms: None,
            loop_b_ms: None,
            ab_repetitions: config.playback.ab_repetitions.min(crate::config::AB_REPETITIONS_MAX),
            ab_delay_ms: config.playback.ab_delay_ms.min(crate::config::AB_DELAI_MAX_MS),
            ab_retours: 0,
            ab_reprise_a: None,
            resume_enabled,
            positions: crate::positions::Positions::load(),
            display_target: (0, 0),
            dump_taille: (0, 0),
            state: PlayerState::Idle,
            current_position_ms: 0,
            total_duration_ms: 0,
            last_frame_dims: (0, 0),
            fps: 30.0,
            pending_video: VecDeque::new(),
            queued_video_packets: VecDeque::new(),
            network: None,
            delai_reseau: None,
            eof: false,
            clock_start: None,
            clock_base_pts_ms: 0,
            audio_clock_seen_ms: None,
            audio_clock_seen_at: None,
            audio_clock_stalled: false,
            audio_buffer_ms: crate::audio::CIBLE_TAMPON_MS,
            clock_report_at: None,
            buffered_min_ms: u64::MAX,
            buffered_max_ms: 0,
            generation: 0,
            seek_target_ms: None,
            speed: 1.0,
            volume: 1.0,
            muted: false,
            subtitles_visible: false,
            loop_enabled: false,
            shuffle: false,
            ordre_aleatoire: Vec::new(),
            rang_aleatoire: 0,
            playlist: Vec::new(),
            playlist_index: 0,
            current_path: None,
            media_info: None,
            subtitle_decoder: None,
            embedded_cues: VecDeque::new(),
            bitmap_cues: VecDeque::new(),
            audio_language: config.audio.default_language.clone(),
            prefer_embedded_subs: config.subtitles.use_embedded,
            subtitle_language: config.subtitles.default_language.clone(),
            audio_filters: None,
            audio_filters_failed: None,
            audio_filters_description,
            audio_settings,
            audio_pitch_preserved,
            filters_description,
            video_settings: config.video.clone(),
            crop_apercu: false,
        }
    }

    /// Active ou désactive le décodage matériel VA-API.
    /// Le changement s'applique au prochain chargement de média (un flux déjà
    /// ouvert conserve son décodeur). Le périphérique VA-API est créé à la
    /// demande s'il ne l'était pas encore.
    pub fn set_hardware_decoding(&mut self, enabled: bool) {
        self.hwdec_enabled = enabled;
        if enabled && self.hw_device.is_none() {
            self.hw_device = unsafe { create_vaapi_device() };
        }
        tracing::info!(
            "Décodage matériel {} (effectif au prochain chargement)",
            if enabled { "activé" } else { "désactivé" }
        );
    }

    /// Active/désactive la réduction d'échelle par le GPU, **immédiatement** pour
    /// le flux en cours (aucun rechargement nécessaire : le drapeau est lu à chaque
    /// image). C'est ce que fait la case à cocher des préférences.
    pub fn set_gpu_scaling(&mut self, enabled: bool) {
        self.gpu_scaling_enabled = enabled;
        if let Some(vs) = self.video_stream.as_mut() {
            vs.echelle_gpu_voulue = enabled;
            // Réinitialise la trace pour que le journal dise de nouveau quel chemin
            // est utilisé (sinon un changement resterait invisible).
            vs.echelle_gpu_active = false;
        }
        tracing::info!(
            "Réduction par le GPU {}",
            if enabled { "activée" } else { "désactivée" }
        );
    }

    /// Vrai si le flux vidéo courant est décodé par le matériel.
    pub fn is_hardware_decoding(&self) -> bool {
        self.video_stream.as_ref().map(|v| v.hw_active).unwrap_or(false)
    }

    /// Applique une nouvelle chaîne de filtres vidéo (libavfilter).
    ///
    /// Le changement prend effet **sur l'image suivante**, sans recharger le média :
    /// la chaîne courante est invalidée et reconstruite à la première frame. La
    /// chaîne réellement installée est journalisée, ce qui rend le réglage
    /// vérifiable (un sélecteur ne prouve rien, une ligne de journal si).
    /// Active l'aperçu du recadrage (l'image affichée reste l'originale).
    ///
    /// À appeler **avant** `set_video_filters` : les deux participent à la même
    /// chaîne, et l'aperçu doit être pris en compte dans sa construction.
    pub fn set_crop_apercu(&mut self, actif: bool) {
        if self.crop_apercu == actif {
            return;
        }
        self.crop_apercu = actif;
        if let Some(vs) = self.video_stream.as_mut() {
            vs.filters = None;
            vs.filters_failed_signature = None;
        }
        tracing::info!(
            "Aperçu du recadrage : {}",
            if actif {
                "activé (image d'origine affichée, recadrage suspendu)"
            } else {
                "désactivé (recadrage appliqué)"
            }
        );
    }

    /// Décide et mémorise l'étage HDR du flux courant, à partir des réglages.
    ///
    /// Appelé à l'ouverture d'un média **et** à chaque changement de réglage : c'est
    /// ce qui garantit que l'étage décrit correspond toujours à la configuration
    /// affichée dans les préférences (un étage calculé une fois pour toutes
    /// ignorerait les changements, défaut classique).
    fn recalculer_etage_hdr(&mut self, video: &crate::config::VideoSettings) -> crate::hdr::Plan {
        // Le contexte (écran + sortie) est relu à chaque recalcul : brancher un autre
        // écran pendant la lecture doit changer la décision, pas rester figé au
        // démarrage. Le coût est une lecture de fichier sysfs, négligeable.
        let contexte = crate::hdr::contexte();
        let plan = crate::hdr::plan_avec(
            self.video_stream.as_ref().and_then(|v| v.hdr_info.as_ref()),
            &video.hdr,
            &contexte,
        );
        // L'étage CPU du même plan, gardé en réserve : si Vulkan est refusé à
        // l'exécution (`apply_video_filters`), il remplace l'étage GPU sans refaire de
        // calcul ni relire les réglages. Vide si le plan n'est pas GPU.
        let etage_cpu = if plan.moteur == crate::hdr::Moteur::Gpu {
            crate::hdr::plan_force(
                self.video_stream.as_ref().and_then(|v| v.hdr_info.as_ref()),
                &video.hdr,
                &contexte,
                Some("cpu"),
            )
            .etage
        } else {
            String::new()
        };
        if let Some(vs) = self.video_stream.as_mut() {
            vs.hdr_etage = plan.etage.clone();
            vs.hdr_indisponible = false;
            vs.hdr_etage_cpu = etage_cpu;
            vs.hdr_repli_cpu = false;
            vs.hdr_repli_motif.clear();
        }
        if !plan.etage.is_empty() {
            tracing::info!("HDR: {} — étage: {}", plan.explication, plan.etage);
        } else {
            tracing::info!("HDR: {}", plan.explication);
        }
        plan
    }

    /// Ce que le lecteur applique réellement au flux en matière de HDR — pour
    /// l'interface et le banc. `None` si aucun média n'est ouvert.
    pub fn hdr_description(&self) -> Option<String> {
        let vs = self.video_stream.as_ref()?;
        let info = vs.hdr_info.as_ref()?;
        let mut texte = info.resume();
        if vs.hdr_repli_cpu {
            texte.push_str(
                " — Vulkan indisponible : conversion par le processeur (qualité réduite : \
                 opérateur Hable, crête fixe)",
            );
        } else if vs.hdr_indisponible {
            texte.push_str(" — étage HDR refusé par le système (Vulkan ?), image non convertie");
            if !vs.hdr_repli_motif.is_empty() {
                texte.push_str(&format!(" ; repli CPU refusé : {}", vs.hdr_repli_motif));
            }
        } else if !vs.hdr_etage.is_empty() {
            texte.push_str(" — étage appliqué");
        }
        Some(texte)
    }

    /// Avis HDR à montrer **une fois** à l'utilisateur (Vulkan refusé : conversion par le
    /// CPU, ou image non convertie). Retiré à la lecture : l'interface l'affiche et il ne
    /// revient pas à l'image suivante.
    pub fn take_hdr_avis(&mut self) -> Option<String> {
        self.video_stream.as_mut()?.hdr_avis.take()
    }

    /// Ce que l'environnement permet : écran (EDID) et capacité de la sortie.
    ///
    /// Exposé à l'interface pour que l'utilisateur voie **pourquoi** un film HDR est
    /// converti : l'écran, ou la fenêtre. Sans cette ligne, « mon écran est HDR et
    /// rien ne change » reste incompréhensible.
    pub fn hdr_contexte(&self) -> crate::hdr::Contexte {
        crate::hdr::contexte()
    }

    pub fn set_video_filters(&mut self, video: &crate::config::VideoSettings) {
        // On garde une copie : à l'ouverture d'un média, l'étage HDR doit être
        // recalculé avec les réglages courants sans que l'appelant ait à les repasser.
        self.video_settings = video.clone();
        // L'étage HDR est recalculé **avant** de composer la description : il dépend
        // des mêmes réglages et doit figurer en tête de chaîne.
        self.recalculer_etage_hdr(video);
        let base = if self.crop_apercu {
            // En aperçu, on montre l'image d'ORIGINE : le repère dessiné par
            // l'interface désigne alors exactement la partie qui sera conservée.
            filter_description(&video.sans_recadrage())
        } else {
            filter_description(video)
        };
        // L'étage HDR passe **en tête**, sur les bits d'origine : après le
        // recadrage et la mise à l'échelle, les hautes lumières auraient déjà perdu
        // leur précision. Une seule description (`filters_description`) contient
        // donc les deux, ce qui rend la reconstruction du graphe — et sa
        // journalisation — exactes.
        let etage = self
            .video_stream
            .as_ref()
            .map(|v| v.hdr_etage.clone())
            .unwrap_or_default();
        let description = match (etage.is_empty(), base.is_empty()) {
            (true, _) => base,
            (false, true) => etage,
            (false, false) => format!("{},{}", etage, base),
        };
        if description == self.filters_description {
            return;
        }
        self.filters_description = description.clone();
        if let Some(vs) = self.video_stream.as_mut() {
            vs.filters = None;
            vs.filters_failed_signature = None;
        }
        tracing::info!(
            "Filtres vidéo: {}",
            if description.is_empty() {
                "aucun (image d'origine)".to_string()
            } else {
                description
            }
        );
        // Force un tour d'affichage : le changement doit être visible même en pause.
        self.generation += 1;
    }

    /// Ouvre (ou réouvre) le décodeur audio d'une piste du conteneur.
    ///
    /// Utilisé au chargement **et** au changement de piste : le décodeur, le
    /// convertisseur et la chaîne de filtres dépendent du format de la piste
    /// (taux, canaux), il faut donc tout reconstruire. Le tampon circulaire est
    /// vidé : sinon on entendrait un fragment de l'ancienne langue.
    fn open_audio_stream(&mut self, index: i32) -> Result<(), String> {
        let Some(format_context) = self.format_context else {
            return Err("aucun média chargé".to_string());
        };
        // Fermer l'ancien décodeur avant d'en ouvrir un autre.
        if let Some(mut ancien) = self.audio_stream.take() {
            unsafe {
                if !ancien.swr.is_null() {
                    swr_free(&mut ancien.swr);
                }
                if !ancien.codec_context.is_null() {
                    let mut p = ancien.codec_context;
                    avcodec_free_context(&mut p);
                }
            }
        }
        self.audio_filters = None;
        self.audio_filters_failed = None;

        unsafe {
            let streams = (*format_context).streams;
            let nb = (*format_context).nb_streams as usize;
            if index < 0 || index as usize >= nb {
                return Err(format!("piste audio {} hors du conteneur", index));
            }
            let stream = &**streams.add(index as usize);
            let codecpar = &*stream.codecpar;
            if codecpar.codec_type != AVMediaType::AVMEDIA_TYPE_AUDIO {
                return Err(format!("le flux {} n'est pas audio", index));
            }

            let out_cfg = self.ensure_audio_output();
            let codec = avcodec_find_decoder(codecpar.codec_id);
            if codec.is_null() {
                return Err("aucun décodeur pour cette piste audio".to_string());
            }
            let codec_context = avcodec_alloc_context3(codec);
            if codec_context.is_null() {
                return Err("allocation du décodeur audio impossible".to_string());
            }
            if avcodec_parameters_to_context(codec_context, stream.codecpar) < 0
                || avcodec_open2(codec_context, codec, std::ptr::null_mut()) < 0
            {
                let mut p = codec_context;
                avcodec_free_context(&mut p);
                return Err("ouverture du décodeur audio refusée".to_string());
            }

            // Le format de sortie est celui du PÉRIPHÉRIQUE (et non celui du
            // fichier) : sans cela un fichier mono joué sur une sortie stéréo
            // défilerait à la mauvaise vitesse.
            let ctx = &*codec_context;
            let out_sample_rate = out_cfg
                .map(|(r, _)| r as i32)
                .unwrap_or_else(|| ctx.sample_rate.max(48000));
            let channels = out_cfg.map(|(_, c)| c as i32).unwrap_or(2);

            let mut out_layout: AVChannelLayout = std::mem::zeroed();
            av_channel_layout_default(&mut out_layout, channels.max(1));
            let mut swr: *mut SwrContext = std::ptr::null_mut();
            let ret = swr_alloc_set_opts2(
                &mut swr,
                &out_layout,
                AVSampleFormat::AV_SAMPLE_FMT_FLT,
                out_sample_rate,
                &ctx.ch_layout,
                ctx.sample_fmt,
                ctx.sample_rate,
                0,
                std::ptr::null_mut(),
            );
            if ret < 0 || swr.is_null() || swr_init(swr) < 0 {
                if !swr.is_null() {
                    swr_free(&mut swr);
                }
                let mut p = codec_context;
                avcodec_free_context(&mut p);
                return Err("convertisseur audio (swresample) indisponible".to_string());
            }

            self.audio_stream = Some(AudioStream {
                index,
                codec_context,
                swr,
                out_layout,
                out_sample_rate,
                time_base: stream.time_base,
            });
        }

        // L'ancien son encore dans le tampon appartient à l'autre piste.
        if let Some(out) = self.audio_output.as_ref() {
            out.clear();
        }
        self.rebuild_audio_resampler();
        self.refresh_audio_chain();
        Ok(())
    }

    /// Ouvre le décodeur d'une piste de sous-titres du conteneur (`index` < 0 : aucune).
    ///
    /// Les sous-titres ne passent pas par la même pompe que la vidéo : chaque paquet
    /// contient des événements avec des durées **relatives** au paquet. Le décodeur
    /// les convertit en texte et `collect_subtitle_packet` les met en file.
    pub fn select_subtitle_stream(&mut self, index: i32) -> Result<(), String> {
        // Fermer la piste précédente et oublier ses événements.
        if let Some(ancien) = self.subtitle_decoder.take() {
            unsafe {
                if !ancien.codec_context.is_null() {
                    let mut p = ancien.codec_context;
                    avcodec_free_context(&mut p);
                }
            }
        }
        self.embedded_cues.clear();
        self.bitmap_cues.clear();
        if index < 0 {
            tracing::info!("Sous-titres embarqués : désactivés");
            return Ok(());
        }

        let Some(format_context) = self.format_context else {
            return Err("aucun média chargé".to_string());
        };
        unsafe {
            let streams = (*format_context).streams;
            let nb = (*format_context).nb_streams as usize;
            if index as usize >= nb {
                return Err(format!("piste de sous-titres {} hors du conteneur", index));
            }
            let stream = &**streams.add(index as usize);
            let codecpar = &*stream.codecpar;
            if codecpar.codec_type != AVMediaType::AVMEDIA_TYPE_SUBTITLE {
                return Err(format!("le flux {} n'est pas un sous-titre", index));
            }
            let codec = avcodec_find_decoder(codecpar.codec_id);
            if codec.is_null() {
                return Err("aucun décodeur pour cette piste de sous-titres".to_string());
            }
            let codec_context = avcodec_alloc_context3(codec);
            if codec_context.is_null() {
                return Err("allocation du décodeur de sous-titres impossible".to_string());
            }
            if avcodec_parameters_to_context(codec_context, stream.codecpar) < 0
                || avcodec_open2(codec_context, codec, std::ptr::null_mut()) < 0
            {
                let mut p = codec_context;
                avcodec_free_context(&mut p);
                return Err("ouverture du décodeur de sous-titres refusée".to_string());
            }
            let bitmap = matches!(
                codecpar.codec_id,
                AVCodecID::AV_CODEC_ID_HDMV_PGS_SUBTITLE
                    | AVCodecID::AV_CODEC_ID_DVD_SUBTITLE
                    | AVCodecID::AV_CODEC_ID_DVB_SUBTITLE
            );
            self.subtitle_decoder = Some(SubtitleStream {
                index,
                codec_context,
                time_base: stream.time_base,
                bitmap,
            });
            if bitmap {
                tracing::info!(
                    "Piste de sous-titres {} en image (PGS/DVD) : les images sont \
                     composées sur la vidéo (pas de texte, réglages de police sans effet)",
                    index
                );
            } else {
                tracing::info!("Sous-titres embarqués : piste {} activée", index);
            }
        }
        Ok(())
    }

    /// Met à jour les préférences de piste (langues, usage des sous-titres embarqués).
    ///
    /// Ne change pas la piste courante : cela n'a d'effet qu'au prochain chargement,
    /// sauf pour `prefer_embedded_subs` qui peut activer une piste immédiatement si
    /// aucune n'est choisie.
    pub fn set_track_preferences(
        &mut self,
        audio_language: &str,
        subtitle_language: &str,
        prefer_embedded: bool,
    ) {
        self.audio_language = audio_language.to_string();
        self.subtitle_language = subtitle_language.to_string();
        self.prefer_embedded_subs = prefer_embedded;
        if prefer_embedded && self.subtitle_decoder.is_none() {
            if let Some(info) = self.media_info.as_ref() {
                if let Some(piste) = choisir_piste(&info.subtitle_tracks, subtitle_language) {
                    let index = piste.index;
                    if let Err(e) = self.select_subtitle_stream(index) {
                        tracing::warn!("Sous-titres embarqués indisponibles : {}", e);
                    }
                }
            }
        }
    }

    /// Piste de sous-titres embarquée active (index dans le conteneur).
    pub fn current_subtitle_stream(&self) -> Option<i32> {
        self.subtitle_decoder.as_ref().map(|s| s.index)
    }

    /// Texte du sous-titre embarqué à la position donnée.
    pub fn embedded_text_at(&self, position_ms: u64) -> Option<String> {
        if self.subtitle_decoder.as_ref().map(|s| s.bitmap).unwrap_or(true) {
            return None;
        }
        let mut textes: Vec<&str> = Vec::new();
        for cue in &self.embedded_cues {
            if position_ms >= cue.start_ms && position_ms < cue.end_ms {
                textes.push(&cue.text);
            }
        }
        if textes.is_empty() {
            None
        } else {
            Some(textes.join("\n"))
        }
    }

    /// Sous-titre **image** actif à la position donnée (PGS, DVD, DVB).
    ///
    /// Le dernier événement commencé fait foi : c'est ainsi que fonctionne une
    /// composition PGS (elle remplace la précédente jusqu'au prochain display set).
    pub fn embedded_bitmap_at(&self, position_ms: u64) -> Option<&BitmapCue> {
        let cue = self
            .bitmap_cues
            .iter()
            .rev()
            .find(|c| c.start_ms <= position_ms)?;
        if cue.end_ms != 0 && position_ms >= cue.end_ms {
            return None;
        }
        Some(cue)
    }

    /// Index du chapitre contenant la position donnée (`None` si aucun chapitre).
    pub fn chapter_at(&self, position_ms: u64) -> Option<usize> {
        let chapitres = self.media_info.as_ref()?.chapters.as_slice();
        chapitres
            .iter()
            .position(|c| position_ms >= c.start_ms && position_ms < c.end_ms)
    }

    /// Va au début du chapitre suivant (`None` si c'était le dernier).
    ///
    /// Sauter « au chapitre suivant » tel qu'on l'attend : si on est au milieu du
    /// chapitre 3, on va au début du 4 ; si on est au tout début du chapitre 3, on
    /// va aussi au 4 (comportement des lecteurs de salon et de PotPlayer). Le
    /// chapitre 1 n'est atteignable que par le début du média ou le menu.
    pub fn next_chapter(&mut self) -> Option<usize> {
        let position = self.current_position_ms;
        let chapitres = self.media_info.as_ref()?.chapters.clone();
        let suivant = chapitres
            .iter()
            .position(|c| c.start_ms > position + 500)?;
        let cible = chapitres[suivant].start_ms;
        tracing::info!("Chapitre suivant : {} à {} ms", suivant + 1, cible);
        let _ = self.seek(cible);
        Some(suivant)
    }

    /// Revient au début du chapitre courant, ou au précédent si on est déjà au début.
    pub fn previous_chapter(&mut self) -> Option<usize> {
        let position = self.current_position_ms;
        let chapitres = self.media_info.as_ref()?.chapters.clone();
        // Le dernier chapitre commençant nettement avant la position courante.
        let precedent = chapitres
            .iter()
            .enumerate()
            .filter(|(_, c)| c.start_ms + 1500 < position)
            .map(|(i, _)| i)
            .last()?;
        let cible = chapitres[precedent].start_ms;
        tracing::info!("Chapitre précédent : {} à {} ms", precedent + 1, cible);
        let _ = self.seek(cible);
        Some(precedent)
    }

    /// Change de piste audio à chaud (sans recharger le média).
    pub fn select_audio_stream(&mut self, index: i32) -> Result<(), String> {
        self.open_audio_stream(index)?;
        // On force la lecture d'une frame pour que le changement s'entende tout de
        // suite, même en pause.
        self.generation += 1;
        Ok(())
    }

    /// Piste audio actuellement ouverte (index dans le conteneur).
    pub fn current_audio_stream(&self) -> Option<i32> {
        self.audio_stream.as_ref().map(|a| a.index)
    }

    /// Applique de nouveaux réglages d'égalisation (effet immédiat).
    ///
    /// Le graphe est reconstruit à la première frame audio suivante ; la chaîne
    /// réellement installée est journalisée, ce qui rend le réglage vérifiable.
    pub fn set_audio_filters(&mut self, audio: &crate::config::AudioSettings) {
        self.audio_settings = audio.clone();
        self.audio_pitch_preserved = audio.pitch_preserved;
        let description = self.audio_description_courante();
        if description == self.audio_filters_description {
            // Trace utile au diagnostic : « rien ne change » doit être observable,
            // sans quoi on ne distingue pas « non appliqué » de « déjà appliqué ».
            tracing::debug!(
                "Traitement audio inchangé : demandé « {} », actif « {} »",
                description,
                self.audio_filters_description
            );
            return;
        }
        self.audio_filters_description = description.clone();
        self.audio_filters = None;
        self.audio_filters_failed = None;
        // Rééchantillonnage cohérent avec le mode choisi (hauteur préservée ou non).
        self.rebuild_audio_resampler();
        tracing::info!(
            "Traitement audio: {}",
            if description.is_empty() {
                "aucun (signal d'origine)".to_string()
            } else {
                // La chaîne elle-même, et non « mise à jour » : c'est la seule trace
                // qui prouve qu'un profil a réellement atteint le moteur.
                format!("chaîne mise à jour ({})", description)
            }
        );
    }

    /// Description de la chaîne audio active ("" = aucune).
    pub fn audio_filters_description(&self) -> &str {
        &self.audio_filters_description
    }

    /// Bilan des échantillons entrés/sortis du graphe audio.
    pub fn audio_filter_counts(&self) -> Option<(u64, u64)> {
        self.audio_filters
            .as_ref()
            .map(|c| (c.samples_in, c.samples_out))
    }

    /// Description de la chaîne de filtres active ("" = aucune).
    pub fn filters_description(&self) -> &str {
        &self.filters_description
    }

    /// Bilan des frames entrées/sorties du graphe de filtres : la chaîne est
    /// construite avec des filtres 1:1, donc les deux compteurs doivent rester
    /// égaux. Un écart signalerait un filtre qui change la cadence.
    pub fn filter_frame_counts(&self) -> Option<(u64, u64)> {
        self.video_stream
            .as_ref()
            .and_then(|v| v.filters.as_ref())
            .map(|c| (c.frames_in, c.frames_out))
    }

    /// Chemin du média actuellement ouvert.
    pub fn current_path(&self) -> Option<&PathBuf> {
        self.current_path.as_ref()
    }

    /// Enregistre la position courante (appelé périodiquement pendant la lecture :
    /// si le programme se termine anormalement, la reprise n'est pas perdue).
    pub fn persist_position(&mut self) {
        if !self.resume_enabled {
            return;
        }
        let (Some(path), position) = (self.current_path.clone(), self.current_position_ms) else {
            return;
        };
        if position == 0 {
            return;
        }
        self.positions.remember(&path, position, self.total_duration_ms);
    }

    /// Active/désactive la reprise de lecture (mémorisation des positions).
    pub fn set_resume(&mut self, enabled: bool) {
        self.resume_enabled = enabled;
        tracing::info!("Reprise de lecture: {}", enabled);
    }

    /// Vrai si le décodage matériel est autorisé (état voulu, avant chargement).
    pub fn hardware_decoding_enabled(&self) -> bool {
        self.hwdec_enabled
    }

    /// Change la méthode de mise à l'échelle (effective dès la frame suivante : le
    /// contexte libswscale est reconstruit quand la méthode diffère).
    pub fn set_scaling_method(&mut self, method: crate::config::ScalingMethod) {
        if self.scaling_method != method {
            tracing::info!("Mise à l'échelle: {:?}", method);
            self.scaling_method = method;
        }
    }

    /// Modifie le volume maximal autorisé et ramène le volume courant dans la limite.
    /// Profondeur visée du tampon audio, en millisecondes (borne : 40 … 1000).
    pub fn set_audio_buffer_ms(&mut self, ms: u64) {
        self.audio_buffer_ms = ms.clamp(40, 1000);
    }

    /// Règle la profondeur du **pré-cache des flux réseau** (0 = désactivé).
    ///
    /// Effet immédiat : la capacité du tampon audio suit (c'est lui qui garde le
    /// média en avance), et le prochain tour de boucle remplit la réserve. Un
    /// fichier local n'est jamais pré-caché : la capacité revient alors à celle
    /// d'origine, sans quoi on garderait en mémoire du média déjà sur le disque.
    pub fn set_precache_ms(&mut self, ms: u64) {
        self.precache_ms = ms.min(crate::audio::CAPACITE_MAX_MS);
        self.appliquer_capacite_tampon();
    }

    /// Aligne la capacité du tampon audio sur le mode de lecture :
    /// pré-cache pour un flux, profondeur courte pour un fichier local.
    fn appliquer_capacite_tampon(&self) {
        let Some(out) = self.audio_output.as_ref() else {
            return;
        };
        if self.network.is_some() && self.precache_ms > 0 {
            // Le tampon doit pouvoir contenir la réserve **et** la profondeur de
            // travail : sinon il jetterait l'audio le plus ancien (le pré-cache se
            // détruirait lui-même).
            out.set_capacite_ms(self.precache_ms + self.audio_buffer_ms + 500);
        } else {
            out.set_capacite_ms(crate::audio::CIBLE_TAMPON_MS * 8);
        }
    }

    /// Remplit la réserve réseau : lit des paquets **en avance** jusqu'à ce que le
    /// dernier paquet vidéo en attente se trouve `precache_ms` devant l'image
    /// affichée.
    ///
    /// Pourquoi c'est utile, mesuré (session 30) : sans réserve, le démultiplexeur
    /// HLS demande un segment **au moment exact** où il en a besoin (intervalles de
    /// requêtes égaux à la durée des segments, zéro avance) ; la moindre gigue se
    /// traduit par une attente, que seule la reconnexion rattrape. Avec réserve, les
    /// segments sont demandés en avance et une coupure courte passe inaperçue.
    ///
    /// Les paquets vidéo sont rangés **non décodés** (quelques dizaines de Ko au lieu
    /// de plusieurs Mo par image RGBA) ; l'audio va dans le tampon, dont la capacité a
    /// été portée par `appliquer_capacite_tampon`.
    fn precharger_reseau(&mut self) {
        if self.precache_ms == 0
            || self.network.is_none()
            || self.audio_stream.is_none()
            || self.eof
            || self.flux_en_panne()
        {
            return;
        }
        if self.prefetch_fini {
            return;
        }
        // Deux plafonds : les octets réellement en mémoire, et le nombre de paquets.
        //
        // Et un **budget par tour** volontairement modeste (8 paquets). Remplir 6 s
        // d'un coup (512 paquets, avec décodage et égalisation de l'audio) bloquait
        // la boucle d'interface assez longtemps pour que l'image prenne du retard :
        // mesuré −154 ms d'écart image/son après une reconnexion. Réparti sur les
        // tours d'affichage (60 i/s × 8 = 480 paquets/s), le remplissage reste
        // quasi instantané à l'échelle de la réserve.
        self.prefetch_en_cours = true;
        let mut lus = 0u32;
        while lus < 8 {
            let avance = self.queue_pts_max_ms.saturating_sub(self.current_position_ms);
            if avance >= self.precache_ms {
                break;
            }
            if self.queue_octets >= MAX_QUEUE_BYTES
                || self.queued_video_packets.len() >= MAX_QUEUED_PACKETS
            {
                break;
            }
            // Ne jamais dépasser la capacité du tampon audio : au-delà, `push`
            // jetterait les échantillons les plus anciens — c'est-à-dire le début
            // du pré-cache, donc la réserve qu'on vient de constituer.
            if self.audio_buffered_ms() + 200 >= self.capacite_tampon_ms() {
                break;
            }
            lus += 1;
            if !self.read_packet_into_decoder(ReadMode::AudioOnly) {
                break;
            }
        }
        self.prefetch_en_cours = false;
    }

    /// Reste-t-il de quoi jouer sans le réseau ? (paquets en attente ou audio
    /// au-delà de la profondeur de travail.)
    fn reserve_disponible(&self) -> bool {
        self.queue_octets > 0 || self.audio_buffered_ms() > self.audio_buffer_ms + 500
    }

    /// Capacité du tampon audio, en millisecondes (0 si aucune sortie).
    fn capacite_tampon_ms(&self) -> u64 {
        self.audio_output.as_ref().map_or(0, |o| o.capacite_ms())
    }

    /// Avance actuelle du pré-cache, en millisecondes (diagnostic et mesure).
    pub fn precache_avance_ms(&self) -> u64 {
        self.queue_pts_max_ms
            .saturating_sub(self.current_position_ms)
    }


    pub fn set_volume_max(&mut self, max_percent: u16) {
        self.volume_max = max_percent.clamp(100, 300);
        let limit = self.volume_max as f32 / 100.0;
        if self.volume > limit {
            self.set_volume(limit);
        }
    }


    /// Change le périphérique de sortie audio. La sortie est recréée au prochain
    /// chargement de média (l'appelant enchaîne en général sur `reload_current_media`,
    /// car le format du nouveau périphérique peut différer).
    pub fn set_audio_device(&mut self, name: &str) {
        if self.audio_device == name {
            return;
        }
        tracing::info!(
            "Périphérique audio: {}",
            if name.is_empty() { "par défaut" } else { name }
        );
        self.audio_device = name.to_string();
        // La sortie sera reconstruite : on relâche l'ancienne (la destruction d'un
        // flux cpal peut attendre le thread audio ; c'est le prix d'un changement).
        self.audio_output = None;
        self.audio_disabled = false;
        self.audio_prefill_fait = false;
        self.audio_prefill_depuis = None;
    }

    /// Taille d'affichage attendue pour les frames (pixels). La conversion
    /// couleur se fait directement à cette taille, ce qui évite de fabriquer une
    /// image pleine résolution que le GPU réduirait ensuite. `(0, 0)` = natif ;
    /// l'agrandissement reste toujours à la charge du GPU.
    /// Impose une taille à `dump_frame_at` (comparaison A/B de deux chemins).
    pub fn set_dump_taille(&mut self, width: u32, height: u32) {
        self.dump_taille = (width, height);
    }

    pub fn set_display_target(&mut self, width: u32, height: u32) {
        // Hystérésis : on ignore les écarts de quelques pixels, sinon chaque
        // frame reconstruirait le contexte swscale (arrondis du calcul
        // d'échelle côté interface).
        let (cw, ch) = self.display_target;
        let dx = (width as i64 - cw as i64).abs();
        let dy = (height as i64 - ch as i64).abs();
        if dx > 16 || dy > 16 {
            self.display_target = (width, height);
        }
    }

    /// Recharge le média courant en conservant la position et l'état lecture/pause.
    /// Sert à appliquer un changement de mode de décodage, qui n'agit qu'à
    /// l'ouverture d'un flux.
    pub fn reload_current_media(&mut self) -> Result<(), String> {
        // Le média ouvert peut ne pas figurer dans la playlist (fichier chargé
        // seul, playlist non renseignée) : on se fie d'abord au flux réel.
        let path = match self.current_path.clone() {
            Some(p) => p,
            None => match self.playlist.get(self.playlist_index) {
                Some(p) => p.clone(),
                None => return Ok(()), // rien à recharger
            },
        };
        let position = self.current_position_ms;
        let was_paused = self.state == PlayerState::Paused;

        self.load_file(path)?;

        if position > 0 {
            let _ = self.seek(position);
        }
        if was_paused {
            let _ = self.toggle_pause();
        }
        Ok(())
    }

    /// Ouvre un fichier média et prépare les décodeurs.
    /// Ouvre un flux réseau (http, HLS, RTSP…).
    ///
    /// FFmpeg ouvre l'URL comme un fichier : `avformat_open_input` accepte les deux,
    /// `avformat_network_init()` est déjà appelé au démarrage. `current_path` reste
    /// vide : la reprise de position et les sous-titres externes n'ont pas de sens
    /// sur un flux non seekable.
    pub fn load_url(&mut self, url: &str) -> Result<MediaInfo, String> {
        tracing::info!("Ouverture du flux: {}", url);
        self.load_file(PathBuf::from(url))
    }

    pub fn load_file(&mut self, path: PathBuf) -> Result<MediaInfo, String> {
        tracing::info!("Chargement du média: {}", path.display());
        self.state = PlayerState::Loading;

        // Flux réseau : on retient l'adresse pour pouvoir le rouvrir après une
        // coupure (voir `tenter_reconnexion`). Un fichier local n'en a pas besoin.
        let adresse = path.to_string_lossy().to_string();
        let est_reseau = crate::utils::is_stream_url(&adresse);
        self.network = if est_reseau {
            Some(NetworkStream::nouveau(&adresse))
        } else {
            None
        };

        // Fermer un éventuel média déjà ouvert
        self.close_media();

        let filename = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|e| format!("Erreur de nom de fichier: {}", e))?;

        unsafe {
            let mut format_context: *mut AVFormatContext = std::ptr::null_mut();

            // Options d'ouverture réseau : sans elles, une source injoignable fait
            // attendre l'ouverture indéfiniment (l'interface semble figée) et une
            // connexion coupée en cours de lecture remonte comme une erreur sèche.
            let mut options: *mut AVDictionary = std::ptr::null_mut();
            if est_reseau {
                let cles = [
                    ("rw_timeout", RESEAU_TIMEOUT_US),  // 10 s par opération d'E/S
                    ("timeout", RESEAU_TIMEOUT_US),     // rtsp / rtmp
                    ("reconnect", "1"),                 // reconnexion HTTP interne
                    ("reconnect_streamed", "1"),        // y compris pour un flux continu
                    ("reconnect_delay_max", "2"),       // plafond de 2 s entre essais
                    ("user_agent", "KPlayer/0.1"),
                ];
                for (cle, valeur) in cles {
                    let ck = CString::new(cle).unwrap();
                    let cv = CString::new(valeur).unwrap();
                    av_dict_set(&mut options, ck.as_ptr(), cv.as_ptr(), 0);
                }
            }

            let ret = avformat_open_input(
                &mut format_context,
                filename.as_ptr(),
                std::ptr::null(),
                &mut options,
            );
            if !options.is_null() {
                av_dict_free(&mut options);
            }
            if ret < 0 || format_context.is_null() {
                let message = format!(
                    "Impossible d'ouvrir {} ({} — code {}){}",
                    if est_reseau { "le flux" } else { "le fichier" },
                    av_error_text(ret),
                    ret,
                    if est_reseau {
                        ""
                    } else {
                        " - Format non supporté"
                    }
                );
                self.state = PlayerState::Error(message.clone());
                return Err(message);
            }

            // Garde-temps des E/S : borne chaque attente réseau, y compris pour
            // les segments HLS ouverts par le démultiplexeur lui-même.
            if est_reseau {
                let delai = std::sync::Arc::new(DelaiReseau::nouveau());
                installer_delai_reseau(format_context, &delai);
                self.delai_reseau = Some(delai);
            }

            self.armer_delai_reseau(DELAI_OUVERTURE_MS);
            let ret = avformat_find_stream_info(format_context, std::ptr::null_mut());
            self.desarmer_delai_reseau();
            if ret < 0 {
                avformat_close_input(&mut format_context);
                self.state = PlayerState::Error(format!("Erreur stream info: code {}", ret));
                return Err(format!(
                    "Impossible de lire les informations du fichier (code {})",
                    ret
                ));
            }

            let mut video_index: i32 = -1;
            let mut audio_index: i32 = -1;
            let mut subtitle_index: i32 = -1;
            let mut fps: f32 = 30.0;
            let mut resolution = (0u32, 0u32);
            let mut video_codec_name = String::from("unknown");
            let mut audio_codec_name = String::from("unknown");
            let mut video_hdr: Option<crate::hdr::Info> = None;

            let nb_streams = (*format_context).nb_streams as usize;
            let streams = (*format_context).streams;

            for i in 0..nb_streams {
                let stream_ptr = *streams.add(i);
                if stream_ptr.is_null() {
                    continue;
                }
                let stream = &*stream_ptr;
                let codecpar = &*stream.codecpar;

                match codecpar.codec_type {
                    AVMediaType::AVMEDIA_TYPE_VIDEO => {
                        if video_index < 0 {
                            video_index = i as i32;
                            let codec = avcodec_find_decoder(codecpar.codec_id);
                            if !codec.is_null() && !(*codec).long_name.is_null() {
                                video_codec_name = std::ffi::CStr::from_ptr((*codec).long_name)
                                    .to_string_lossy()
                                    .to_string();
                            }
                            // Métadonnées de couleur : transfert (PQ/HLG), primaires,
                            // profondeur. Lues **avant** l'ouverture du décodeur, pour
                            // que la chaîne de filtres soit construite avec elles dès
                            // la première image.
                            let format_sonde = codecpar.format;
                            let nom_format = av_get_pix_fmt_name(
                                std::mem::transmute::<c_int, AVPixelFormat>(format_sonde),
                            );
                            let nom_format = if nom_format.is_null() {
                                String::new()
                            } else {
                                std::ffi::CStr::from_ptr(nom_format)
                                    .to_string_lossy()
                                    .to_string()
                            };
                            let bits = crate::hdr::bits_de_format(&nom_format);
                            video_hdr = Some(crate::hdr::detecter(
                                codecpar.color_trc as i32,
                                codecpar.color_primaries as i32,
                                bits,
                            ));
                        }
                    }
                    AVMediaType::AVMEDIA_TYPE_AUDIO => {
                        if audio_index < 0 {
                            audio_index = i as i32;
                            let codec = avcodec_find_decoder(codecpar.codec_id);
                            if !codec.is_null() && !(*codec).long_name.is_null() {
                                audio_codec_name = std::ffi::CStr::from_ptr((*codec).long_name)
                                    .to_string_lossy()
                                    .to_string();
                            }
                        }
                    }
                    AVMediaType::AVMEDIA_TYPE_SUBTITLE => {
                        if subtitle_index < 0 {
                            subtitle_index = i as i32;
                        }
                    }
                    _ => {}
                }
            }

            // --- Inventaire des pistes audio et de sous-titres ---
            //
            // Fait AVANT d'ouvrir les décodeurs : l'interface peut ainsi proposer la
            // liste complète, et la langue préférée oriente le choix de la piste.
            let mut audio_tracks: Vec<TrackInfo> = Vec::new();
            let mut subtitle_tracks: Vec<TrackInfo> = Vec::new();
            for i in 0..nb_streams {
                let stream_ptr = *streams.add(i);
                if stream_ptr.is_null() {
                    continue;
                }
                let stream = &*stream_ptr;
                let codecpar = &*stream.codecpar;
                let genre = match codecpar.codec_type {
                    AVMediaType::AVMEDIA_TYPE_AUDIO => "audio",
                    AVMediaType::AVMEDIA_TYPE_SUBTITLE => "sous-titres",
                    _ => continue,
                };
                let codec = avcodec_find_decoder(codecpar.codec_id);
                let codec_name = if !codec.is_null() && !(*codec).long_name.is_null() {
                    std::ffi::CStr::from_ptr((*codec).long_name)
                        .to_string_lossy()
                        .to_string()
                } else {
                    format!("codec {}", codecpar.codec_id as i32)
                };
                let langue = stream_metadata(stream.metadata, "language");
                let titre = stream_metadata(stream.metadata, "title");
                // Sous-titres image : le décodeur existe, mais il ne produit pas de
                // texte (PGS, DVD). On le signale au lieu de laisser croire qu'une
                // piste vide est un bug.
                let bitmap = matches!(
                    codecpar.codec_id,
                    AVCodecID::AV_CODEC_ID_HDMV_PGS_SUBTITLE
                        | AVCodecID::AV_CODEC_ID_DVD_SUBTITLE
                        | AVCodecID::AV_CODEC_ID_DVB_SUBTITLE
                );
                let info = TrackInfo {
                    index: i as i32,
                    codec: codec_name,
                    language: langue.clone(),
                    title: titre.clone(),
                    channels: if genre == "audio" { codecpar.ch_layout.nb_channels } else { 0 },
                    sample_rate: if genre == "audio" { codecpar.sample_rate } else { 0 },
                    bitmap,
                };
                if genre == "audio" {
                    tracing::info!("Piste audio {} : {}", audio_tracks.len(), info.label());
                    audio_tracks.push(info);
                } else {
                    tracing::info!("Piste de sous-titres {} : {}", subtitle_tracks.len(), info.label());
                    subtitle_tracks.push(info);
                }
            }

            // Choix de la piste audio : langue préférée si elle existe, sinon la
            // première (comportement historique, jamais de régression).
            if let Some(choisie) = choisir_piste(&audio_tracks, &self.audio_language) {
                if audio_index != choisie.index {
                    tracing::info!(
                        "Langue audio « {} » demandée → piste {}",
                        self.audio_language,
                        choisie.label()
                    );
                }
                audio_index = choisie.index;
            }

            // Durée (en µs AV_TIME_BASE)
            let duration_ms = if (*format_context).duration > 0 {
                ((*format_context).duration / 1000) as u64
            } else {
                0
            };

            let bitrate = if (*format_context).bit_rate > 0 {
                (*format_context).bit_rate as u64
            } else {
                0
            };

            // --- Préparer le décodeur vidéo ---
            if video_index >= 0 {
                let stream = &**streams.add(video_index as usize);
                let codecpar = &*stream.codecpar;

                let codec = avcodec_find_decoder(codecpar.codec_id);
                if !codec.is_null() {
                    // Première tentative en décodage matériel (VA-API), seconde en
                    // logiciel : le pilote peut refuser une variante précise du
                    // profil (typiquement le H.264 pleine échelle « yuvj420p » des
                    // vidéos de téléphone), et avcodec_open2() échoue alors.
                    let codec_hw_capable = codec_supports_vaapi(codec);
                    let hw_usable = self.hwdec_enabled && self.hw_device.is_some();
                    let want_hw = hw_usable && codec_hw_capable;
                    let attempts: &[bool] = if want_hw { &[true, false] } else { &[false] };

                    let mut codec_context: *mut AVCodecContext = std::ptr::null_mut();
                    let mut opened = false;
                    let mut hw_requested = false;


                    for &use_hw in attempts {
                        if !codec_context.is_null() {
                            avcodec_free_context(&mut codec_context);
                        }
                        codec_context = avcodec_alloc_context3(codec);
                        if codec_context.is_null() {
                            break;
                        }
                        if avcodec_parameters_to_context(codec_context, stream.codecpar) < 0 {
                            break;
                        }
                        if use_hw {
                            // libavcodec prend possession de cette référence
                            (*codec_context).hw_device_ctx =
                                av_buffer_ref(self.hw_device.unwrap());
                            (*codec_context).get_format = Some(select_hw_format);
                        }
                        if avcodec_open2(codec_context, codec, std::ptr::null_mut()) >= 0 {
                            opened = true;
                            hw_requested = use_hw;
                            break;
                        }
                        if use_hw {
                            tracing::warn!(
                                "VA-API refusé pour ce flux (profil non pris en charge) — repli logiciel"
                            );
                        }
                    }

                    if !opened {
                        if !codec_context.is_null() {
                            avcodec_free_context(&mut codec_context);
                        }
                        avformat_close_input(&mut format_context);

                        self.state = PlayerState::Error("Erreur ouverture codec vidéo".into());
                        return Err("Impossible d'ouvrir le décodeur vidéo".into());
                    }

                    let width = codecpar.width;
                    let height = codecpar.height;
                    resolution = (width.max(0) as u32, height.max(0) as u32);

                    let avg_num = stream.avg_frame_rate.num;
                    let avg_den = stream.avg_frame_rate.den;
                    if avg_num != 0 && avg_den != 0 {
                        fps = avg_num as f32 / avg_den as f32;
                    }

                    self.video_stream = Some(VideoStream {
                        index: video_index,
                        codec_context,
                        scale_context: std::ptr::null_mut(),
                        scale_format: AVPixelFormat::AV_PIX_FMT_NONE,
                        scale_method: crate::config::ScalingMethod::Bilinear, // force la 1re création
                        scale_src_width: 0,
                        scale_src_height: 0,
                        scale_dst_width: 0,
                        scale_dst_height: 0,
                        hw_requested,
                        hw_active: false,
                        hw_fallback_logged: false,
                        echelle_gpu_active: false,
                        gpu_chain: None,
                        echelle_gpu_voulue: self.gpu_scaling_enabled,
                        codec_hw_capable,
                        hw_failures: 0,
                        filters: None,
                        filters_failed_signature: None,
                        // Métadonnées relevées juste avant (transfert PQ/HLG,
                        // primaires, profondeur) : elles décident de l'étage HDR.
                        hdr_info: video_hdr.clone(),
                        hdr_etage: String::new(),
                        hdr_indisponible: false,
                        hdr_etage_cpu: String::new(),
                        hdr_repli_cpu: false,
                        hdr_repli_motif: String::new(),
                        fps_flux: fps,
                        hdr_avis: None,
                        time_base: stream.time_base,
                    });
                    if hw_requested {
                        tracing::info!("Décodeur ouvert en mode VA-API (usage confirmé à la 1re frame)");
                    }
                    // L'étage HDR dépend du **flux** (transfert, primaires,
                    // profondeur) autant que des réglages : il est recalculé à chaque
                    // ouverture de média, et la chaîne est refaite en conséquence.
                    // (`set_video_filters` ne fait rien si la description est
                    // inchangée : on la vide d'abord pour forcer la recomposition —
                    // sinon un média HDR ouvert après un média SDR garderait la
                    // chaîne du précédent.)
                    let reglages = self.video_settings.clone();
                    self.filters_description = String::new();
                    self.set_video_filters(&reglages);
                }
            }

            self.format_context = Some(format_context);
            // Le décodeur audio est ouvert par la même méthode que lors d'un
            // changement de piste : un seul chemin de code, donc un seul jeu de bugs.
            if audio_index >= 0 {
                if let Err(e) = self.open_audio_stream(audio_index) {
                    tracing::warn!("Piste audio {} inutilisable : {}", audio_index, e);
                }
            }
            // Sous-titres embarqués : choisir la piste une fois pour toutes.
            // L'interface peut ensuite en imposer une autre (menu Sous-titres) ; un
            // fichier externe chargé prendra le dessus à l'affichage.
            if self.prefer_embedded_subs && !subtitle_tracks.is_empty() {
                // Toutes les pistes sont candidates : les pistes **image**
                // (PGS des Blu-ray, sous-images DVD, DVB) sont rendues comme
                // des bitmaps composés sur la vidéo depuis la session 23.
                match choisir_piste(&subtitle_tracks, &self.subtitle_language) {
                    Some(piste) => {
                        tracing::info!("Sous-titres embarqués retenus : {}", piste.label());
                        let index = piste.index;
                        if let Err(e) = self.select_subtitle_stream(index) {
                            tracing::warn!("Sous-titres embarqués indisponibles : {}", e);
                        }
                    }
                    None => {
                        tracing::info!("Aucune piste de sous-titres utilisable");
                    }
                }
            }

            self.total_duration_ms = duration_ms;
            self.subtitle_stream = if subtitle_index >= 0 {
                Some(subtitle_index)
            } else {
                None
            };

            // --- Chapitres ---
            let mut chapters: Vec<Chapter> = Vec::new();
            let nb_chapitres = (*format_context).nb_chapters as usize;
            if nb_chapitres > 0 {
                for i in 0..nb_chapitres {
                    let ch_ptr = *(*format_context).chapters.add(i);
                    if ch_ptr.is_null() {
                        continue;
                    }
                    let ch = &*ch_ptr;
                    let tb = ch.time_base;
                    if tb.den == 0 {
                        continue;
                    }
                    let ms = |v: i64| -> u64 {
                        ((v as i128 * tb.num as i128 * 1000 / tb.den as i128).max(0)) as u64
                    };
                    let titre = stream_metadata(ch.metadata, "title");
                    chapters.push(Chapter {
                        start_ms: ms(ch.start),
                        end_ms: ms(ch.end),
                        title: titre,
                    });
                }
                chapters.sort_by_key(|c| c.start_ms);
                tracing::info!("{} chapitre(s) dans le conteneur", chapters.len());
            }

            let media_info = MediaInfo {
                filename: path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                resolution,
                fps,
                bitrate,
                video_codec: video_codec_name,
                audio_codec: audio_codec_name,
                audio_tracks,
                subtitle_tracks,
                chapters,
            };

            self.media_info = Some(media_info.clone());
            self.current_path = Some(path.clone());
            self.current_position_ms = 0;
            self.fps = fps;
            self.pending_video.clear();
            self.flush_video_packets();
            self.seek_target_ms = None;
            self.reset_clock_watch();
            self.eof = false;
            self.state = PlayerState::Playing;
            self.sync_audio_state();
            // (Re)démarre l'horloge de lecture
            self.clock_start = Some(std::time::Instant::now());
            self.clock_base_pts_ms = 0;
            self.generation = self.generation.wrapping_add(1);

            tracing::info!(
                "Média prêt: {} — {}x{} @ {:.2} fps, durée {} ms",
                media_info.video_codec,
                resolution.0,
                resolution.1,
                fps,
                duration_ms
            );

            // Reprise de lecture : on se replace à la position mémorisée.
            // Sans objet sur un flux réseau (direct ou playlist changeante).
            if self.resume_enabled && !est_reseau {
                if let Some(position) = self.positions.get(&path, duration_ms) {
                    tracing::info!("Reprise de lecture à {} ms", position);
                    if let Err(e) = self.seek(position) {
                        tracing::warn!("Reprise impossible: {}", e);
                    }
                }
            }

            Ok(media_info)
        }
    }

    /// Lit un paquet et le route vers le décodeur adéquat.
    /// Renvoie `false` en fin de flux ou en cas d'erreur de lecture.
    fn read_packet_into_decoder(&mut self, mode: ReadMode) -> bool {
        let format_context = match self.format_context {
            Some(fc) => fc,
            None => return false,
        };

        // Descriptions lues AVANT les emprunts mutables (une copie, sinon l'emprunt
        // de `self` durerait jusqu'à la fin du bloc).
        let audio_filters = self.audio_filters_description.clone();
        let audio_format = self
            .audio_output
            .as_ref()
            .map(|o| (o.sample_rate() as i32, o.channels() as i32));
        // Position courante lue AVANT les emprunts : sert de repli quand une
        // trame audio n'annonce pas de PTS exploitable.
        let position_ms = self.current_position_ms;

        // Pointeur brut vers le décodeur vidéo : évite un emprunt longue durée de
        // `self.video_stream` qui bloquerait l'accès aux autres champs.
        let video_ptr: *mut VideoStream = match self.video_stream.as_mut() {
            Some(v) => v as *mut VideoStream,
            None => std::ptr::null_mut(),
        };

        unsafe {
            let pkt = av_packet_alloc();
            if pkt.is_null() {
                return false;
            }

            // Durée de la lecture : sur un flux, un blocage de plusieurs secondes
            // signale une source muette (le fil d'interface attend ce retour).
            let debut_lecture = std::time::Instant::now();
            self.armer_delai_reseau(DELAI_LECTURE_MS);
            let ret = av_read_frame(format_context, pkt);
            self.desarmer_delai_reseau();
            let duree_lecture = debut_lecture.elapsed();
            if duree_lecture > Duration::from_millis(1000) {
                tracing::warn!(
                    "Lecture réseau bloquée {} ms (source lente ou muette)",
                    duree_lecture.as_millis()
                );
            }
            if ret < 0 {
                if self.prefetch_en_cours {
                    // On lit en avance : une erreur ici ne dit rien de ce que
                    // l'utilisateur est en train de voir. On la mémorise et on
                    // arrête le préchargement ; la lecture la rencontrera au bon
                    // moment (c'est elle qui décide « fin » ou « coupure »).
                    let mut p = pkt;
                    av_packet_free(&mut p);
                    self.prefetch_fini = true;
                    tracing::debug!(
                        "Préchargement arrêté sur erreur de lecture ({} — la lecture tranchera)",
                        av_error_text(ret)
                    );
                    return false;
                }
                // Deux causes très différentes, longtemps confondues :
                //   - fin de flux (AVERROR_EOF, ou indicateur EOF du contexte
                //     d'E/S) : le média est terminé, ou la playlist HLS aussi ;
                //   - coupure (délai dépassé, connexion réinitialisée, segment
                //     absent) : la source peut revenir, il faut la rouvrir.
                // Les confondre faisait passer toute coupure réseau pour une fin
                // de lecture : le flux s'arrêtait définitivement.
                let io_eof = {
                    let pb = (*format_context).pb;
                    !pb.is_null() && avio_feof(pb) != 0
                };
                let mut p = pkt;
                av_packet_free(&mut p);
                if ret == AVERROR_EOF || io_eof {
                    self.clore_ou_reconnecte();
                } else if ret == AVERROR_EXIT {
                    // Interruption demandée par notre garde-temps : la source
                    // n'a pas répondu dans le délai imparti.
                    self.note_stream_error(
                        ret,
                        &format!(
                            "source muette (aucune donnée en {} s)",
                            DELAI_LECTURE_MS / 1000
                        ),
                    );
                } else {
                    self.note_stream_error(ret, &av_error_text(ret));
                }
                return false;
            }

            // Lecture réussie : une éventuelle panne est derrière nous.
            self.note_stream_ok();

            let stream_index = (*pkt).stream_index;
            let mut handled = false;

            if !video_ptr.is_null() && stream_index == (*video_ptr).index {
                handled = true;
                if mode == ReadMode::AudioOnly {
                    // Lecture anticipée destinée au tampon audio : on garde le
                    // PAQUET (quelques dizaines de Ko) au lieu de le décoder en
                    // image RGBA (plusieurs Mo). Le décodeur ne prendra ce paquet
                    // qu'au moment de l'affichage (voir `pump_video_frame`).
                    self.queue_video_packet(pkt);
                } else {
                    self.decode_video_packet(pkt);
                }
            }

            // --- Stream audio : décodage + conversion + poussée dans le tampon ---
            if !handled {
                if let (Some(aus), Some(out)) =
                    (self.audio_stream.as_mut(), self.audio_output.as_ref())
                {
                    if stream_index == aus.index {
                        // Pendant un seek, on jette l'audio antérieur à la cible
                        // (sinon le tampon se remplit avec du son déjà dépassé).
                        let drop_for_seek = match self.seek_target_ms {
                            Some(target) if aus.time_base.den != 0 && (*pkt).pts >= 0 => {
                                let pts_ms = ((*pkt).pts as i128
                                    * aus.time_base.num as i128
                                    * 1000
                                    / aus.time_base.den as i128)
                                    .max(0) as u64;
                                pts_ms + 1 < target
                            }
                            _ => false,
                        };
                        if !drop_for_seek {
                        let send = avcodec_send_packet(aus.codec_context, pkt);
                        if send >= 0 {
                            let mut samples: Vec<f32> = Vec::new();
                            // Position média du premier échantillon du lot : elle
                            // sert d'origine à l'horloge audio (carte son maître).
                            let mut pts_audio: Option<u64> = None;
                            loop {
                                let frame = av_frame_alloc();
                                if frame.is_null() {
                                    break;
                                }
                                let recv = avcodec_receive_frame(aus.codec_context, frame);
                                if recv == 0 {
                                    if pts_audio.is_none() {
                                        pts_audio = Some(if (*frame).pts >= 0
                                            && aus.time_base.den != 0
                                        {
                                            ((*frame).pts as i128
                                                * aus.time_base.num as i128
                                                * 1000
                                                / aus.time_base.den as i128)
                                                .max(0) as u64
                                        } else {
                                            position_ms
                                        });
                                    }
                                    decode_audio_frame_to_f32(aus, frame, &mut samples);
                                }
                                let mut f = frame;
                                av_frame_free(&mut f);
                                if recv != 0 {
                                    break;
                                }
                            }
                            if !samples.is_empty() {
                                // Égaliseur : appliqué au format du périphérique,
                                // donc exactement sur le signal écouté.
                                if let Some((taux, canaux)) = audio_format {
                                    apply_audio_filters(
                                        &mut self.audio_filters,
                                        &mut self.audio_filters_failed,
                                        &audio_filters,
                                        taux,
                                        canaux,
                                        &mut samples,
                                    );
                                }
                                out.push(&samples, pts_audio.unwrap_or(position_ms));
                            }
                        }
                        }
                    }
                }
            }

            // --- Piste de sous-titres embarquée : décodage des événements ---
            // Traitée après l'audio, sur le même paquet lu : les sous-titres d'un
            // MKV arrivent mêlés aux autres flux, il ne faut pas rater leurs paquets.
            if !handled {
                let index_subs = self.subtitle_decoder.as_ref().map(|s| s.index);
                if let (Some(cible), Some(subs)) = (index_subs, self.subtitle_decoder.as_ref()) {
                    if stream_index == cible {
                        collect_subtitle_packet(
                            subs,
                            pkt,
                            &mut self.embedded_cues,
                            &mut self.bitmap_cues,
                        );
                    }
                }
            }

            av_packet_unref(pkt);
            let mut p = pkt;
            av_packet_free(&mut p);
            true
        }
    }

    /// Renvoie la prochaine frame vidéo décodée (en la tirant de la file ou en
    /// décodant de nouveaux paquets si nécessaire).
    pub fn pump_video_frame(&mut self) -> Option<VideoFrame> {
        if self.state != PlayerState::Playing && self.state != PlayerState::Paused {
            return None;
        }
        // Aucun média ouvert (coupure réseau en cours de reconnexion) : rien à
        // pomper. Sans ce garde-fou, la boucle de décodage tournerait à vide.
        if self.format_context.is_none() || self.flux_en_panne() {
            return None;
        }

        // Vider d'abord la file des frames déjà décodées
        while let Some(frame) = self.pending_video.pop_front() {
            if self.accept_pts(frame.pts_ms) && self.accept_for_display(frame.pts_ms) {
                self.current_position_ms = frame.pts_ms;
                return Some(frame);
            }
        }

        // Sinon, décoder jusqu'à obtenir une frame (borne pour éviter une boucle infinie)
        let mut guard = 0;
        while guard < 512 {
            guard += 1;

            // Paquets vidéo lus d'avance pour alimenter le tampon audio : ils
            // passent au décodeur AVANT toute nouvelle lecture (ordre du fichier).
            if let Some(pkt) = self.queued_video_packets.pop_front() {
                unsafe {
                    self.queue_octets = self.queue_octets.saturating_sub((*pkt).size.max(0) as usize);
                }
                self.decode_video_packet(pkt);
                let mut p = pkt;
                unsafe { av_packet_free(&mut p) };
                continue;
            }

            if self.eof {
                // Purger le décodeur en fin de flux
                if let Some(f) = self.drain_video_decoder() {
                    self.current_position_ms = f.pts_ms;
                    return Some(f);
                }
                if self.loop_enabled && !self.playlist.is_empty() {
                    let path = self.playlist[self.playlist_index % self.playlist.len()].clone();
                    let _ = self.load_file(path);
                    return self.pump_video_frame();
                }
                tracing::info!("Fin du média ({} ms)", self.current_position_ms);
                self.state = PlayerState::Stopped;
                self.sync_audio_state();
                return None;
            }

            if !self.read_packet_into_decoder(ReadMode::Frame) {
                if self.eof {
                    continue; // fin de flux : le tour suivant gérera la purge
                }
                // Coupure réseau : on rend la main plutôt que de boucler sur une
                // source muette ; la reconnexion est tentée par l'interface
                // (`service_network`), qui reste ainsi maîtresse du rythme.
                return None;
            }

            while let Some(frame) = self.pending_video.pop_front() {
                if self.accept_pts(frame.pts_ms) && self.accept_for_display(frame.pts_ms) {
                    self.current_position_ms = frame.pts_ms;
                    return Some(frame);
                }
            }
        }

        None
    }

    /// Range un paquet vidéo en attente de décodage (copie par comptage de
    /// références : le paquet d'origine est libéré par l'appelant).
    fn queue_video_packet(&mut self, pkt: *mut AVPacket) {
        // Avance réelle du pré-cache : PTS du dernier paquet rangé. Un paquet sans
        // PTS exploitable (`pts < 0`) ne la fait pas avancer — c'est le cas des
        // flux sans horodatage, où l'avance ne peut de toute façon pas être
        // mesurée et où le plafond de paquets reste seul juge.
        if let Some(vs) = self.video_stream.as_ref() {
            unsafe {
                let pts = (*pkt).pts;
                if pts >= 0 && vs.time_base.den != 0 {
                    let pts_ms = (pts as i128 * vs.time_base.num as i128 * 1000
                        / vs.time_base.den as i128) as u64;
                    self.queue_pts_max_ms = self.queue_pts_max_ms.max(pts_ms);
                }
                self.queue_octets = self.queue_octets.saturating_add((*pkt).size.max(0) as usize);
            }
        }
        if self.queued_video_packets.len() >= MAX_QUEUED_PACKETS {
            // File saturée (conteneur anormal) : décoder tout de suite plutôt que
            // perdre l'image ; le paquet reste libéré par l'appelant.
            self.decode_video_packet(pkt);
            return;
        }
        unsafe {
            let mut clone = av_packet_alloc();
            if clone.is_null() {
                return;
            }
            if av_packet_ref(clone, pkt) < 0 {
                av_packet_free(&mut clone);
                return;
            }
            self.queued_video_packets.push_back(clone);
        }
    }

    /// Envoie un paquet vidéo au décodeur et récupère les images disponibles.
    ///
    /// **Ne libère pas** le paquet : c'est l'appelant qui en est propriétaire
    /// (le décodeur n'en garde qu'une référence).
    fn decode_video_packet(&mut self, pkt: *mut AVPacket) {
        let video_ptr: *mut VideoStream = match self.video_stream.as_mut() {
            Some(v) => v as *mut VideoStream,
            None => return,
        };
        if video_ptr.is_null() || pkt.is_null() {
            return;
        }

        // Copies lues avant les emprunts (sinon l'emprunt de `self` durerait).
        let filters = self.filters_description.clone();
        // Équivalent matériel de la chaîne (désentrelacement seul), s'il existe.
        let chaine_gpu = filtre_gpu_equivalent(&self.config_video, &filters);
        let position_ms = self.current_position_ms;

        unsafe {
            let vs = &mut *video_ptr;
            let send_ret = avcodec_send_packet(vs.codec_context, pkt);

            if send_ret >= 0 {
                let mut decoded: Vec<VideoFrame> = Vec::new();
                loop {
                    let frame = av_frame_alloc();
                    if frame.is_null() {
                        break;
                    }
                    let recv = avcodec_receive_frame(vs.codec_context, frame);
                    if recv == 0 {
                        let pts_ms = if (*frame).pts >= 0 && vs.time_base.den != 0 {
                            ((*frame).pts as i128 * vs.time_base.num as i128 * 1000
                                / vs.time_base.den as i128)
                                .max(0) as u64
                        } else {
                            position_ms
                        };
                        let mut dims = self.last_frame_dims;
                        let target = self.display_target;
                        let method = self.scaling_method.clone();
                        if let Some(data) =
                            scale_frame_to_rgba(
                                vs, frame, target, &method, &filters, chaine_gpu, &mut dims,
                            )
                        {
                            self.last_frame_dims = dims;
                            decoded.push(VideoFrame {
                                data,
                                width: dims.0,
                                height: dims.1,
                                pts_ms,
                            });
                        }
                    }
                    let mut f = frame;
                    av_frame_free(&mut f);
                    if recv != 0 {
                        break; // EAGAIN ou erreur : plus de frame disponible
                    }
                }
                let produced = !decoded.is_empty();
                for f in decoded {
                    self.pending_video.push_back(f);
                }
                self.note_video_decode_result(produced);
            }
        }
    }

    /// Libère les paquets vidéo mis en attente (seek, changement de média, arrêt).
    fn flush_video_packets(&mut self) {
        while let Some(mut p) = self.queued_video_packets.pop_front() {
            unsafe { av_packet_free(&mut p) };
        }
        // L'avance du pré-cache repart de zéro avec la file : sinon elle resterait
        // au PTS d'un média précédent et le préchargeur croirait avoir de l'avance.
        self.queue_pts_max_ms = 0;
        self.queue_octets = 0;
        self.prefetch_fini = false;
        self.prefetch_en_cours = false;
    }

    /// Force le décodeur vidéo à rendre les frames restantes (fin de flux).
    fn drain_video_decoder(&mut self) -> Option<VideoFrame> {
        let filters = self.filters_description.clone();
        let chaine_gpu = filtre_gpu_equivalent(&self.config_video, &filters);
        let video_ptr: *mut VideoStream = self.video_stream.as_mut().map(|v| v as *mut VideoStream)?;

        unsafe {
            let vs = &mut *video_ptr;
            // Envoyer un paquet vide signale la fin au décodeur
            avcodec_send_packet(vs.codec_context, std::ptr::null());

            loop {
                let frame = av_frame_alloc();
                if frame.is_null() {
                    return None;
                }
                let recv = avcodec_receive_frame(vs.codec_context, frame);
                let mut out = None;
                if recv == 0 {
                    let pts_ms = if (*frame).pts >= 0 && vs.time_base.den != 0 {
                        ((*frame).pts as i128 * vs.time_base.num as i128 * 1000
                            / vs.time_base.den as i128)
                            .max(0) as u64
                    } else {
                        self.current_position_ms
                    };
                    let mut dims = self.last_frame_dims;
                    let target = self.display_target;
                    let method = self.scaling_method.clone();
                    if let Some(data) =
                        scale_frame_to_rgba(
                            vs, frame, target, &method, &filters, chaine_gpu, &mut dims,
                        )
                    {
                        self.last_frame_dims = dims;
                        out = Some(VideoFrame {
                            data,
                            width: dims.0,
                            height: dims.1,
                            pts_ms,
                        });
                    }
                }
                let mut f = frame;
                av_frame_free(&mut f);
                if let Some(vf) = out {
                    return Some(vf);
                }
                if recv != 0 {
                    return None;
                }
            }
        }
    }
    /// Intervalle entre deux images en **temps média** (indépendant de la
    /// vitesse de lecture) : le délai mural vaut cet intervalle divisé par la
    /// vitesse, et l'avance image par image doit l'utiliser tel quel.
    pub fn frame_interval(&self) -> Duration {
        let fps = if self.fps > 1.0 { self.fps } else { 30.0 };
        let secs = 1.0 / fps as f64;
        Duration::from_secs_f64(secs.clamp(0.001, 1.0))
    }

    /// Délai à attendre avant d'afficher la frame suivante.
    ///
    /// Définit (ou annule) le point A de la répétition A-B, à la position courante.
    pub fn toggle_loop_point_a(&mut self) -> String {
        if self.loop_a_ms.is_some() {
            self.loop_a_ms = None;
            self.loop_b_ms = None;
            tracing::info!("Répétition A-B: annulée");
            return "Répétition A-B : annulée".to_string();
        }
        self.loop_a_ms = Some(self.current_position_ms);
        tracing::info!("Répétition A-B: point A à {} ms", self.current_position_ms);
        format!("Répétition A-B : point A à {}", Self::format_ms(self.current_position_ms))
    }

    /// Définit le point B (et active la répétition) ou annule la répétition.
    pub fn toggle_loop_point_b(&mut self) -> String {
        let Some(a) = self.loop_a_ms else {
            tracing::warn!("Répétition A-B: point B demandé sans point A");
            return "Répétition A-B : définissez d'abord le point A (touche A)".to_string();
        };
        if self.loop_b_ms.is_some() {
            self.loop_a_ms = None;
            self.loop_b_ms = None;
            tracing::info!("Répétition A-B: annulée");
            return "Répétition A-B : annulée".to_string();
        }
        let b = self.current_position_ms;
        if b <= a {
            tracing::warn!(
                "Répétition A-B: point B ({}) invalide, il doit suivre le point A ({})",
                b,
                a
            );
            return format!(
                "Répétition A-B : le point B ({}) doit suivre le point A ({})",
                Self::format_ms(b),
                Self::format_ms(a)
            );
        }
        self.loop_b_ms = Some(b);
        self.ab_retours = 0;
        tracing::info!("Répétition A-B: boucle de {} à {} ms", a, b);
        // On se replace au point A pour que la répétition démarre immédiatement
        let _ = self.seek(a);
        format!(
            "Répétition A-B : {} → {} (lecture en boucle)",
            Self::format_ms(a),
            Self::format_ms(b)
        )
    }

    /// État de la répétition A-B : (point A, point B).
    pub fn loop_points(&self) -> (Option<u64>, Option<u64>) {
        (self.loop_a_ms, self.loop_b_ms)
    }

    /// Vrai si la frame peut être affichée ; gère la répétition A-B.
    ///
    /// **À appeler pour CHAQUE frame rendue** : les frames proviennent de deux
    /// boucles différentes (file d'attente, puis décodage à la demande). Le contrôle
    /// n'existait que dans la première, si bien que la répétition ne se déclenchait
    /// jamais en lecture normale.
    fn accept_for_display(&mut self, pts_ms: u64) -> bool {
        if let (Some(a), Some(b)) = (self.loop_a_ms, self.loop_b_ms) {
            if pts_ms >= b {
                if self.ab_repetitions > 0 && self.ab_retours >= self.ab_repetitions {
                    // Les passages demandés sont faits : la boucle est levée et le film
                    // continue au-delà de B (cette image est affichée normalement).
                    tracing::info!(
                        "Répétition A-B: {} retour(s) effectués, boucle levée",
                        self.ab_retours
                    );
                    self.loop_a_ms = None;
                    self.loop_b_ms = None;
                    self.ab_retours = 0;
                    return true;
                }
                self.ab_retours += 1;
                tracing::info!(
                    "Répétition A-B: retour au point A ({} ms), retour {}",
                    a,
                    self.ab_retours
                );
                let _ = self.seek(a);
                if self.ab_delay_ms > 0 {
                    // Pause réelle (image et son) ; `tick_boucle_ab` la lèvera.
                    let _ = self.toggle_pause();
                    self.ab_reprise_a =
                        Some(std::time::Instant::now() + Duration::from_millis(self.ab_delay_ms));
                }
                return false;
            }
        }
        true
    }

    /// Règle la répétition A-B : nombre de retours (0 = sans fin) et pause entre deux passages.
    pub fn set_boucle_ab(&mut self, retours: u32, delai_ms: u64) {
        self.ab_repetitions = retours.min(crate::config::AB_REPETITIONS_MAX);
        self.ab_delay_ms = delai_ms.min(crate::config::AB_DELAI_MAX_MS);
        tracing::info!(
            "Répétition A-B: {} retour(s), pause de {} ms",
            self.ab_repetitions,
            self.ab_delay_ms
        );
    }

    /// Pose directement les deux points (ligne de commande `--boucle-ab`).
    pub fn set_loop_points(&mut self, a_ms: u64, b_ms: u64) -> bool {
        if b_ms <= a_ms {
            return false;
        }
        self.loop_a_ms = Some(a_ms);
        self.loop_b_ms = Some(b_ms);
        self.ab_retours = 0;
        true
    }

    /// Instant de fin de la pause de boucle, s'il y en a une en cours.
    pub fn ab_attente(&self) -> Option<std::time::Instant> {
        self.ab_reprise_a
    }

    /// À appeler à chaque tour de l'interface, **y compris en pause** : lève la pause de fin
    /// de boucle quand le délai est écoulé. Si l'utilisateur a repris ou arrêté la lecture
    /// entre-temps, l'attente est simplement oubliée.
    pub fn tick_boucle_ab(&mut self) {
        let Some(fin) = self.ab_reprise_a else { return };
        if self.state != PlayerState::Paused {
            self.ab_reprise_a = None;
            return;
        }
        if std::time::Instant::now() >= fin {
            self.ab_reprise_a = None;
            let _ = self.toggle_pause();
        }
    }

    /// Formatage court d'une position (mm:ss) pour les messages d'état.
    fn format_ms(ms: u64) -> String {
        let total = ms / 1000;
        format!("{:02}:{:02}", total / 60, total % 60)
    }

    /// Suit les paquets vidéo décodés sans image et arrête la lecture au bout d'une
    /// rafale : un flux illisible ne doit pas saturer le processeur indéfiniment.
    ///
    /// C'est aussi ce contrôle qui couvre le cas d'un codec sans décodeur logiciel
    /// (AV1 ici) : contrairement à ce que laisserait croire `AV_CODEC_CAP_HARDWARE`,
    /// le décodeur AV1 de ce FFmpeg **ne pose pas ce drapeau** (il n'annonce que
    /// `AV_CODEC_CAP_DR1`) — vérifié dans `libavcodec/av1dec.c`. On ne peut donc pas le
    /// détecter à l'ouverture ; on le constate à l'usage, en ~2 s de lecture.
    fn note_video_decode_result(&mut self, produced_frame: bool) {
        const MAX_CONSECUTIVE_FAILURES: u32 = 120;
        if produced_frame {
            self.video_decode_failures = 0;
            return;
        }
        self.video_decode_failures = self.video_decode_failures.saturating_add(1);
        if self.video_decode_failures != MAX_CONSECUTIVE_FAILURES {
            return;
        }

        let codec = self
            .media_info
            .as_ref()
            .map(|i| i.video_codec.clone())
            .unwrap_or_else(|| "inconnu".into());
        // Si ce codec sait utiliser VA-API, la cause la plus probable est qu'on est en
        // logiciel alors qu'il n'existe pas de décodeur logiciel (cas de l'AV1 ici).
        let hw_possible = self
            .video_stream
            .as_ref()
            .map(|v| v.codec_hw_capable)
            .unwrap_or(false);
        let message = if hw_possible && !self.is_hardware_decoding() {
            format!(
                "Aucune image décodée (codec {}) : ce flux exige le décodage matériel. \
                 Activez-le avec la touche H.",
                codec
            )
        } else {
            format!(
                "Aucune image décodée après {} paquets : flux illisible (codec {}).",
                MAX_CONSECUTIVE_FAILURES, codec
            )
        };
        tracing::error!("{}", message);
        self.state = PlayerState::Error(message);
    }

    /// Position média (ms) donnée par la **carte son** : les échantillons qu'elle
    /// a réellement consommés depuis l'origine posée au dernier seek/pause.
    ///
    /// `None` s'il n'y a pas de flux audio, pas de sortie audio, ou si aucun
    /// échantillon n'a encore été poussé depuis la dernière remise à zéro.
    pub fn audio_clock_ms(&self) -> Option<f64> {
        if self.audio_clock_stalled {
            return None;
        }
        if self.audio_stream.is_none() {
            return None;
        }
        self.audio_output.as_ref()?.media_ms(self.speed)
    }

    /// Position média (ms) de référence pour cadencer la vidéo.
    ///
    /// **La carte son est maître** : la cadence suit les échantillons qu'elle a
    /// consommés, donc l'image est toujours alignée sur le son réellement
    /// restitué — y compris quand le périphérique avance à son propre rythme
    /// (Bluetooth, HDMI, carte dont l'horloge dérive par rapport au temps
    /// système) et quand la profondeur du tampon varie.
    ///
    /// Le **temps mural** ne sert que de repli : pas de piste audio, pas de
    /// sortie audio, ou horloge audio muette (voir `tick_audio`).
    pub fn media_clock_ms(&self) -> Option<f64> {
        if let Some(audio) = self.audio_clock_ms() {
            return Some(audio);
        }
        let start = self.clock_start?;
        Some(
            self.clock_base_pts_ms as f64
                + start.elapsed().as_secs_f64() * 1000.0 * self.speed.max(0.1) as f64,
        )
    }

    /// Délai à attendre avant d'afficher la frame suivante.
    ///
    /// Auto-correcteur : on vise la position média exacte de la frame suivante
    /// (`pts + intervalle`), donc un réveil tardif réduit le délai suivant et
    /// l'erreur ne s'accumule pas. `None` = afficher tout de suite (en retard).
    ///
    /// L'intervalle visé est un intervalle **média** (`frame_interval()`), divisé
    /// ici par la vitesse pour obtenir le délai mural.
    pub fn next_frame_delay(&self, last_frame_pts_ms: u64) -> Option<Duration> {
        let now_ms = self.media_clock_ms()?;
        let speed = self.speed.max(0.1) as f64;
        let interval_media_ms = self.frame_interval().as_secs_f64() * 1000.0;

        let target_media_ms = last_frame_pts_ms as f64 + interval_media_ms;
        let remaining = (target_media_ms - now_ms) / 1000.0 / speed;
        if remaining > 0.0005 {
            Some(Duration::from_secs_f64(remaining.min(1.0)))
        } else {
            None
        }
    }

    /// Entretient l'horloge de lecture : garde-fou de l'horloge audio, puis
    /// alimentation du tampon. À appeler à chaque tour de boucle pendant la
    /// lecture, **avant** `next_frame_delay`.
    ///
    /// C'est ce qui rompt le cercle « on ne lit un paquet que pour afficher une
    /// frame, et on n'affiche une frame que lorsque la carte son a consommé du
    /// son » : le tampon est alimenté indépendamment de l'image.
    pub fn tick_audio(&mut self) {
        if self.state != PlayerState::Playing {
            return;
        }
        // Coupure réseau : la reconnexion est tentée ici, à chaque tour de boucle.
        self.service_network();
        self.guard_audio_clock();
        // **Remplir AVANT de laisser le périphérique consommer.** L'ordre inverse
        // (démarrer la carte, puis alimenter) faisait sortir un ou deux blocs de
        // silence au tout début de chaque lecture : le premier tour d'interface est
        // le plus lourd (police, textures, première image), la carte réclamait déjà
        // du son que le décodeur n'avait pas encore produit. Mesuré : 2 blocs à
        // l'ouverture, comptés comme sous-alimentations — un défaut intermittent du
        // banc d'horloge, mais surtout un début de lecture qui grinçait.
        if !self.audio_prefill_fait && self.audio_buffered_ms() < self.audio_buffer_ms {
            let debut = *self
                .audio_prefill_depuis
                .get_or_insert_with(std::time::Instant::now);
            self.feed_audio();
            let assez = self.audio_buffered_ms() >= self.audio_buffer_ms;
            // **Le préremplissage ne doit JAMAIS bloquer la lecture.** Après un seek
            // près de la fin de la piste audio, ou sur un média sans son exploitable,
            // le tampon n'atteint jamais la cible : attendre indéfiniment laissait
            // l'image figée (mesuré : « lecture figée à 6 003 ms »). Au-delà d'un
            // délai bref, on démarre avec ce qu'on a — c'est au pire un bloc de
            // silence, jamais un gel.
            let delai_depasse = debut.elapsed() > Duration::from_millis(200);
            if assez || delai_depasse {
                self.audio_prefill_fait = true;
                self.audio_prefill_depuis = None;
                tracing::info!(
                    "Tampon audio prérempli ({} ms{}) — la carte démarre maintenant",
                    self.audio_buffered_ms(),
                    if assez { "" } else { ", délai atteint" }
                );
                // `sync_audio_state` n'est pas appelé à chaque image : sans cet appel
                // explicite, le drapeau changeait sans que la carte en soit informée
                // et la lecture restait muette (mesuré : tampon monté jusqu'au
                // plafond de 1 088 ms, horloge audio nulle).
                self.sync_audio_state();
            }
            self.report_clock();
            return;
        }
        self.feed_audio();
        // Réserve réseau : remplie après l'alimentation normale, avec ses propres
        // plafonds (mémoire et capacité de tampon).
        self.precharger_reseau();
        self.report_clock();
    }

    /// Détecte une horloge audio muette (tampon qui ne se remplit plus,
    /// périphérique suspendu, fin de la piste audio) : on repasse alors au temps
    /// mural, sinon l'image resterait figée sur une horloge arrêtée.
    fn guard_audio_clock(&mut self) {
        let now_ms = match (self.audio_stream.is_some(), self.audio_output.as_ref()) {
            (true, Some(out)) => out.media_ms(self.speed),
            _ => None,
        };
        let Some(now_ms) = now_ms else {
            self.audio_clock_seen_ms = None;
            self.audio_clock_seen_at = None;
            self.audio_clock_stalled = false;
            return;
        };

        let avance = match self.audio_clock_seen_ms {
            Some(avant) => now_ms > avant + 0.5,
            None => true,
        };
        if avance {
            self.audio_clock_seen_ms = Some(now_ms);
            self.audio_clock_seen_at = Some(std::time::Instant::now());
            if self.audio_clock_stalled {
                self.audio_clock_stalled = false;
                tracing::info!("Horloge audio: de nouveau vivante, cadence reprise sur la carte son");
            }
            return;
        }
        let muette_depuis = self.audio_clock_seen_at.map(|t| t.elapsed().as_millis());
        if !self.audio_clock_stalled && muette_depuis.unwrap_or(0) > SILENCE_HORLOGE_MS {
            self.audio_clock_stalled = true;
            tracing::warn!(
                "Horloge audio muette depuis {} ms (tampon {} ms) : cadence rendue au temps mural",
                muette_depuis.unwrap_or(0),
                self.audio_buffered_ms()
            );
        }
    }

    /// Maintient le tampon audio à sa profondeur cible en lisant des paquets
    /// **sans attendre qu'une image soit due**. Les images décodées au passage
    /// s'empilent dans `pending_video` (affichées à leur heure) ; la pile est
    /// bornée pour ne pas laisser croître la mémoire.
    fn feed_audio(&mut self) {
        if self.audio_stream.is_none()
            || self.audio_output.is_none()
            || self.audio_disabled
            || self.eof
            || self.format_context.is_none()
            || self.flux_en_panne()
        {
            return;
        }
        let mut lus = 0;
        while self.audio_buffered_ms() < self.audio_buffer_ms && lus < 64 {
            // Garde-fous mémoire : ils ne doivent pas empêcher le tampon de se
            // remplir dans un conteneur normal (voir les constantes).
            if self.pending_video.len() >= MAX_PENDING_VIDEO {
                break;
            }
            if let Some(derniere) = self.pending_video.back() {
                if derniere.pts_ms > self.current_position_ms + MAX_AVANCE_VIDEO_MS {
                    break;
                }
            }
            lus += 1;
            if !self.read_packet_into_decoder(ReadMode::AudioOnly) {
                break; // fin de flux : `eof` est positionné
            }
        }
    }

    /// Une ligne de journal par seconde : les deux horloges, l'écart, la
    /// profondeur du tampon et les sous-alimentations. Sert de mesure
    /// reproductible (`tools/verif-horloge.sh`).
    fn report_clock(&mut self) {
        let tampon = self.audio_buffered_ms();
        if tampon > 0 || self.buffered_max_ms > 0 {
            self.buffered_min_ms = self.buffered_min_ms.min(tampon);
            self.buffered_max_ms = self.buffered_max_ms.max(tampon);
        }
        let Some(debut) = self.clock_report_at else {
            self.clock_report_at = Some(std::time::Instant::now());
            self.buffered_min_ms = u64::MAX;
            self.buffered_max_ms = 0;
            return;
        };
        if debut.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.clock_report_at = Some(std::time::Instant::now());
        let audio = self.audio_clock_ms();
        let murale = self.clock_start.map(|s| {
            self.clock_base_pts_ms as f64 + s.elapsed().as_secs_f64() * 1000.0 * self.speed as f64
        });
        let (sous_alim, contentions, retard) = self
            .audio_output
            .as_ref()
            .map(|o| (o.underruns(), o.contentions(), o.device_latency_ms()))
            .unwrap_or((0, 0, 0.0));
        let avance = self.precache_avance_ms();
        let octets = self.queue_octets / 1024;
        tracing::debug!(
            "Horloge: audio {} — murale {} — écart {} ms — tampon {}/{} ms — retard périphérique {:.0} ms — sous-alimentations {} (contentions {}){}",
            audio.map(|v| format!("{:.0} ms", v)).unwrap_or_else(|| "—".into()),
            murale.map(|v| format!("{:.0} ms", v)).unwrap_or_else(|| "—".into()),
            match (audio, murale) {
                (Some(a), Some(m)) => format!("{:.0}", m - a),
                _ => "—".into(),
            },
            if self.buffered_min_ms == u64::MAX { 0 } else { self.buffered_min_ms },
            self.buffered_max_ms,
            retard,
            sous_alim,
            contentions,
            if self.network.is_some() {
                format!(" — pré-cache {} ms / {} Ko", avance, octets)
            } else {
                String::new()
            }
        );
        self.buffered_min_ms = u64::MAX;
        self.buffered_max_ms = 0;
    }

    /// Décode l'image située à la position demandée et la renvoie **à la
    /// résolution native** du média, filtres appliqués — sans fenêtre, sans GPU.
    ///
    /// Sert à `--dump-frame`, donc à la vérification chiffrée des filtres : une
    /// capture d'écran dépend de la taille de la fenêtre et du rééchantillonnage,
    /// ce qui rend toute mesure de netteté ou de bruit discutable. Ici l'image
    /// produite est exactement celle que le moteur fabrique avant l'affichage.
    pub fn dump_frame_at(&mut self, position_ms: u64) -> Option<VideoFrame> {
        // Résolution native par défaut : aucune mise à l'échelle dans le chemin
        // mesuré. `--bench-taille` peut l'imposer (comparaison de pixels entre
        // l'échelle GPU et l'échelle CPU).
        if self.dump_taille.0 >= 16 && self.dump_taille.1 >= 16 {
            self.display_target = self.dump_taille;
        } else {
            self.display_target = (0, 0);
        }
        self.state = PlayerState::Paused;
        if position_ms > 0 {
            let _ = self.seek(position_ms);
        }
        // Pompe quelques images : après un seek, les premières peuvent précéder la
        // cible (elles sont jetées par `accept_pts`).
        let mut derniere = None;
        for _ in 0..64 {
            match self.pump_video_frame() {
                Some(frame) => {
                    let atteinte = frame.pts_ms + 1 >= position_ms;
                    derniere = Some(frame);
                    if atteinte {
                        break;
                    }
                }
                None => break,
            }
        }
        derniere
    }

    /// Détecte les bandes noires (letterbox / pillarbox) du média courant.
    ///
    /// **Pourquoi par analyse d'image plutôt que par le filtre `cropdetect`** :
    /// `cropdetect` ne communique son résultat que par un message de journal, qu'il
    /// faudrait ensuite analyser — fragile et impossible à mesurer proprement. Ici on
    /// décode quelques images réparties dans le film et on mesure directement les
    /// lignes et colonnes sombres. Le résultat est reproductible et vérifiable
    /// (`tools/verif-cadrage.sh`).
    ///
    /// La marge retenue est le **maximum** observé sur les échantillons : mieux vaut
    /// ne pas recadrer une image claire que rogner un plan dont les bords sont noirs
    /// pour des raisons artistiques. Les valeurs sont arrondies à des multiples de 2
    /// (exigence de nombreux formats de pixels).
    pub fn detect_black_bars(&mut self, echantillons: usize) -> (u32, u32, u32, u32) {
        let duree = self.total_duration_ms.max(1000);
        // Mesures par image : on garde la MÉDIANE, pas le maximum. Une bande de
        // letterbox est présente sur TOUTES les images ; une scène sombre isolée ne
        // doit pas faire rogner tout le film (le maximum, essayé d'abord, recadrait
        // sur un seul plan noir).
        let mut mesures: Vec<(u32, u32, u32, u32)> = Vec::new();

        // On n'altère pas l'état de lecture : sauvegarde puis restauration.
        let (etat, position, generation) = (self.state.clone(), self.current_position_ms, self.generation);
        let filtre_avant = self.filters_description.clone();
        self.filters_description = String::new(); // mesurer l'image d'origine

        let n = echantillons.clamp(1, 12);
        for i in 0..n {
            // Éviter le tout début et la toute fin (génériques noirs).
            let fraction = (i as f64 + 0.5) / n as f64;
            let t = (duree as f64 * fraction) as u64;
            let Some(frame) = self.dump_frame_at(t) else {
                continue;
            };
            if frame.width < 8 || frame.height < 8 {
                continue;
            }
            // Seuil de « noir » : 18/255 (les bandes d'un film sont presque toujours
            // à 16 ou 0, alors qu'un ciel nocturne monte plus haut).
            mesures.push(bordures_sombres(&frame.data, frame.width, frame.height, 18));
        }

        // Restauration de l'état.
        self.filters_description = filtre_avant;
        self.audio_filters = None;
        self.state = etat;
        self.current_position_ms = position;
        self.generation = generation + 1;

        // Médiane par bord, arrondie à des valeurs paires. Une bande plus petite que
        // 1 % de la dimension est ignorée : ce n'est pas une bande mais du contenu
        // sombre en bord d'image, et la rogner ferait perdre de l'image utile.
        let median = |valeurs: Vec<u32>| -> u32 {
            if valeurs.is_empty() {
                return 0;
            }
            let mut v = valeurs;
            v.sort_unstable();
            v[v.len() / 2]
        };
        let (h, b, g, d) = if mesures.is_empty() {
            (0, 0, 0, 0)
        } else {
            (
                median(mesures.iter().map(|m| m.0).collect()),
                median(mesures.iter().map(|m| m.1).collect()),
                median(mesures.iter().map(|m| m.2).collect()),
                median(mesures.iter().map(|m| m.3).collect()),
            )
        };
        let arrondi = |valeur: u32, dimension: u32| {
            if (valeur as f32) < dimension as f32 * 0.01 {
                0
            } else {
                (valeur / 2) * 2
            }
        };
        let dimension = self.last_frame_dims;
        (
            arrondi(h, dimension.1.max(2)),
            arrondi(b, dimension.1.max(2)),
            arrondi(g, dimension.0.max(2)),
            arrondi(d, dimension.0.max(2)),
        )
    }

    /// Extrait tous les sous-titres de la piste embarquée sélectionnée.
    ///
    /// Sert à `--dump-subs`, donc à la vérification : on peut alors comparer le texte
    /// et les minutages à ce que contient réellement le conteneur, sans dépendre de
    /// l'affichage (et sans écran).
    /// Décode toute la piste de sous-titres **image** et renvoie ses événements,
    /// dans l'ordre chronologique (mesure sans écran : `--dump-images`).
    pub fn dump_bitmap_cues(&mut self) -> Vec<BitmapCue> {
        let Some(format_context) = self.format_context else {
            return Vec::new();
        };
        let Some(index) = self.subtitle_decoder.as_ref().map(|s| s.index) else {
            return Vec::new();
        };
        let mut textes: VecDeque<EmbeddedCue> = VecDeque::new();
        let mut images: VecDeque<BitmapCue> = VecDeque::new();
        unsafe {
            av_seek_frame(format_context, index, 0, AVSEEK_FLAG_BACKWARD);
        }
        let paquet = unsafe { av_packet_alloc() };
        if paquet.is_null() {
            return Vec::new();
        }
        let mut garde = 0u32;
        loop {
            garde += 1;
            if garde > 200_000 {
                break;
            }
            let ret = unsafe { av_read_frame(format_context, paquet) };
            if ret < 0 {
                break;
            }
            if unsafe { (*paquet).stream_index } == index {
                if let Some(subs) = self.subtitle_decoder.as_ref() {
                    unsafe {
                        collect_subtitle_packet(subs, paquet, &mut textes, &mut images);
                    }
                }
            }
            unsafe { av_packet_unref(paquet) };
        }
        let mut p = paquet;
        unsafe { av_packet_free(&mut p) };

        let mut liste: Vec<BitmapCue> = images.into_iter().collect();
        liste.sort_by_key(|c| c.start_ms);
        liste
    }

    pub fn dump_subtitles(&mut self) -> Vec<EmbeddedCue> {
        let Some(format_context) = self.format_context else {
            return Vec::new();
        };
        let Some(index) = self.subtitle_decoder.as_ref().map(|s| s.index) else {
            return Vec::new();
        };
        let mut cues: VecDeque<EmbeddedCue> = VecDeque::new();

        unsafe {
            av_seek_frame(format_context, index, 0, AVSEEK_FLAG_BACKWARD);
        }
        let paquet = unsafe { av_packet_alloc() };
        if paquet.is_null() {
            return Vec::new();
        }
        let mut garde = 0u32;
        loop {
            garde += 1;
            if garde > 200_000 {
                break;
            }
            let ret = unsafe { av_read_frame(format_context, paquet) };
            if ret < 0 {
                break;
            }
            if unsafe { (*paquet).stream_index } == index {
                if let Some(subs) = self.subtitle_decoder.as_ref() {
                    let mut images = VecDeque::new();
                    unsafe {
                        collect_subtitle_packet(subs, paquet, &mut cues, &mut images);
                    }
                }
            }
            unsafe { av_packet_unref(paquet) };
        }
        let mut p = paquet;
        unsafe { av_packet_free(&mut p) };

        let mut liste: Vec<EmbeddedCue> = cues.into_iter().collect();
        liste.sort_by_key(|c| c.start_ms);
        liste
    }

    /// Décode tout l'audio du média courant en f32 entrelacé **égaliseur compris**,
    /// sans passer par la carte son.
    ///
    /// Sert à `--dump-audio`, donc à la vérification chiffrée de l'égaliseur : une
    /// écoute ne prouve pas un gain de +12 dB, une transformée de Fourier si. Le
    /// format est celui du média (taux et canaux d'origine), ce qui rend la mesure
    /// indépendante du périphérique de l'utilisateur.
    pub fn dump_audio(&mut self) -> Option<(Vec<f32>, u32, u16)> {
        let format_context = self.format_context?;
        let audio_index = self.audio_stream.as_ref()?.index;

        // Format de référence : celui du flux (pas du périphérique), pour que la
        // mesure soit reproductible sur n'importe quelle machine.
        // Le taux à déclarer est celui que produisent les échantillons, c'est-à-dire
        // celui du convertisseur (format du périphérique, éventuellement divisé par
        // la vitesse en mode « hauteur modifiée »). Déclarer le taux du FICHIER
        // donnait un WAV dont l'en-tête mentait — et une mesure faussée d'autant.
        let canaux_fichier = unsafe {
            let ctx = (*format_context).streams.add(audio_index as usize);
            let st = *ctx;
            (*st).codecpar.as_ref().map(|p| p.ch_layout.nb_channels).unwrap_or(2)
        };
        let (taux, canaux) = {
            let aus = self.audio_stream.as_ref()?;
            let taux = if aus.out_sample_rate > 0 {
                aus.out_sample_rate as u32
            } else {
                unsafe {
                    let ctx = (*format_context).streams.add(audio_index as usize);
                    let st = *ctx;
                    (*st).codecpar.as_ref().map(|p| p.sample_rate).unwrap_or(48000) as u32
                }
            };
            (taux, canaux_fichier.max(1) as u16)
        };

        // Ordre de lecture depuis le début.
        unsafe {
            av_seek_frame(format_context, audio_index, 0, AVSEEK_FLAG_BACKWARD);
        }
        self.seek_target_ms = None;

        let mut sortie: Vec<f32> = Vec::new();
        let paquet = unsafe { av_packet_alloc() };
        if paquet.is_null() {
            return None;
        }
        let mut garde = 0u32;
        loop {
            garde += 1;
            if garde > 200_000 {
                break;
            }
            let ret = unsafe { av_read_frame(format_context, paquet) };
            if ret < 0 {
                break; // fin de flux
            }
            if unsafe { (*paquet).stream_index } == audio_index {
                let Some(aus) = self.audio_stream.as_mut() else { break };
                unsafe {
                    if avcodec_send_packet(aus.codec_context, paquet) >= 0 {
                        let mut bloc: Vec<f32> = Vec::new();
                        loop {
                            let frame = av_frame_alloc();
                            if frame.is_null() {
                                break;
                            }
                            let recv = avcodec_receive_frame(aus.codec_context, frame);
                            if recv == 0 {
                                decode_audio_frame_to_f32(aus, frame, &mut bloc);
                            }
                            let mut f = frame;
                            av_frame_free(&mut f);
                            if recv != 0 {
                                break;
                            }
                        }
                        // Même chemin que la lecture réelle : l'égaliseur s'applique
                        // au format du périphérique. Ici on force ce format à celui
                        // du flux pour que la mesure soit stable, puis on rééchantillonne
                        // si le périphérique diffère — c'est ce que fait déjà `swr`.
                        if !bloc.is_empty() {
                            let desc = self.audio_description_courante();
                            apply_audio_filters(
                                &mut self.audio_filters,
                                &mut self.audio_filters_failed,
                                &desc,
                                taux as i32,
                                canaux as i32,
                                &mut bloc,
                            );
                            sortie.extend_from_slice(&bloc);
                        }
                    }
                }
            }
            unsafe {
                av_packet_unref(paquet);
            }
        }
        let mut p = paquet;
        unsafe { av_packet_free(&mut p) };

        // Récupérer ce qui reste dans la ligne de retard de l'égaliseur.
        let canaux_filtre = canaux as i32;
        unsafe {
            flush_audio_filters(&mut self.audio_filters, canaux_filtre, &mut sortie);
        }

        if sortie.is_empty() {
            return None;
        }
        Some((sortie, taux as u32, canaux))
    }

    /// Décode `frames` images consécutives (filtres compris) sans affichage et
    /// renvoie (durée en secondes, nombre d'images réellement décodées).
    ///
    /// Sert à `--bench` : c'est le seul moyen d'attribuer un coût aux filtres.
    /// Mesurer le processus entier pendant la lecture ne dit rien — le rendu
    /// logiciel de l'écran virtuel (llvmpipe) consomme des cœurs entiers et noie
    /// la mesure (constaté : 888 % d'un cœur sans filtre, à l'arrêt du média).
    /// Mesure le décodage **pour une taille d'affichage donnée**.
    ///
    /// Sans cela, `--bench` mesure toujours la conversion à la résolution native et
    /// ne peut pas montrer l'effet d'une réduction : or c'est précisément le cas
    /// où l'échelle matérielle change tout (cf. `docs/OPTIMISATION-COMPARATIF.md`).
    /// `(0, 0)` = résolution native.
    pub fn bench_decode_taille(&mut self, frames: usize, taille: (u32, u32)) -> (f64, u64) {
        self.display_target = taille;
        self.state = PlayerState::Paused;
        let debut = std::time::Instant::now();
        let mut compte = 0u64;
        while compte < frames as u64 {
            match self.pump_video_frame() {
                Some(_) => compte += 1,
                None => break,
            }
        }
        (debut.elapsed().as_secs_f64(), compte)
    }

    /// Écart (ms) entre l'image affichée et l'horloge de référence : c'est la
    /// synchronisation image/son quand la carte son est maître. > 0 = l'image est
    /// en avance sur le son.
    pub fn drift_ms(&self, last_frame_pts_ms: u64) -> i64 {
        match self.media_clock_ms() {
            Some(now_ms) => last_frame_pts_ms as i64 - now_ms as i64,
            None => 0,
        }
    }

    /// Décide si une frame doit être affichée ou jetée (phase de seek).
    fn accept_pts(&mut self, pts_ms: u64) -> bool {
        match self.seek_target_ms {
            Some(target) => {
                if pts_ms + 1 < target {
                    false // encore avant la cible : on jette
                } else {
                    self.seek_target_ms = None; // cible atteinte
                    true
                }
            }
            None => true,
        }
    }

    /// Crée la sortie audio si nécessaire (une seule fois pour tout le
    /// programme) et renvoie son format : (taux, canaux).
    fn ensure_audio_output(&mut self) -> Option<(u32, u16)> {
        if self.audio_output.is_none() {
            let wanted = if self.audio_device.is_empty() {
                None
            } else {
                Some(self.audio_device.clone())
            };
            match crate::audio::AudioOutput::new(wanted.as_deref()) {
                Ok(out) => {
                    tracing::info!(
                        "Sortie audio prête: {} Hz, {} canaux",
                        out.sample_rate(),
                        out.channels()
                    );
                    self.audio_output = Some(out);
                    // Capacité adaptée au mode dès l'ouverture : pré-cache pour un
                    // flux, profondeur courte pour un fichier local.
                    self.appliquer_capacite_tampon();
                    tracing::info!(
                        "Tampon audio : capacité {} ms (pré-cache réseau {} ms)",
                        self.capacite_tampon_ms(),
                        if self.network.is_some() { self.precache_ms } else { 0 }
                    );
                }
                Err(e) => {
                    tracing::warn!("Audio indisponible (lecture vidéo seule): {}", e);
                    self.audio_disabled = true;
                }
            }
        }
        self.audio_output
            .as_ref()
            .map(|o| (o.sample_rate(), o.channels()))
    }

    /// Synchronise l'état de la sortie audio avec l'état du lecteur.
    ///
    /// **La carte ne démarre qu'une fois le tampon prérempli** (`audio_prefill_fait`).
    /// Démarrage inverse = deux blocs de silence en tête de lecture : le premier tour
    /// d'interface est le plus lourd (police de secours, première texture) et la carte
    /// réclamait des blocs de 93 ms qu'aucune alimentation ne pouvait encore fournir
    /// (mesuré : sous-alimentations à 70 et 96 ms, 2 266 et 2 352 échantillons
    /// manquants). Tant que la carte n'est pas lancée, elle sort du silence sans que
    /// ce soit un manque de données : le compteur ne bouge pas.
    fn sync_audio_state(&self) {
        if let Some(out) = self.audio_output.as_ref() {
            out.set_playing(self.state == PlayerState::Playing && self.audio_prefill_fait);
            out.set_volume(if self.muted { 0.0 } else { self.volume });
        }
    }

    /// Nombre de millisecondes d'audio en attente dans le tampon.
    pub fn audio_buffered_ms(&self) -> u64 {
        self.audio_output.as_ref().map_or(0, |o| o.buffered_ms())
    }

    pub fn has_audio(&self) -> bool {
        self.audio_output.is_some()
    }

    /// Secondes de média réellement consommées par la carte son depuis la
    /// dernière remise à zéro du tampon (seek, pause, changement de piste).
    pub fn audio_played_seconds(&self) -> f64 {
        self.audio_output
            .as_ref()
            .map(|o| o.played_frames() as f64 / o.sample_rate().max(1) as f64)
            .unwrap_or(0.0)
    }

    /// Autorise `ms` millisecondes pour l'opération réseau qui commence.
    fn armer_delai_reseau(&self, ms: u64) {
        if let Some(delai) = self.delai_reseau.as_ref() {
            delai.armer(ms);
        }
    }

    /// L'opération réseau est terminée : plus de limite.
    fn desarmer_delai_reseau(&self) {
        if let Some(delai) = self.delai_reseau.as_ref() {
            delai.desarmer();
        }
    }

    /// Enregistre une coupure de lecture.
    ///
    /// Pour un flux réseau, la lecture est **suspendue** et une reconnexion est
    /// programmée : la source peut très bien revenir (serveur redémarré, réseau
    /// rétabli, playlist rechargée). Pour un fichier local, rien à rouvrir : le
    /// comportement historique est conservé (fin de lecture).
    fn note_stream_error(&mut self, code: i32, texte: &str) {
        // **Réserve encore disponible** : l'erreur porte sur des données dont la
        // lecture n'a pas besoin tout de suite. Rouvrir le flux maintenant
        // interromprait la lecture pour rien (mesuré : serveur absent 4 s avec 6 s
        // de réserve → une reconnexion inutile, alors que l'image continuait).
        // On note, on arrête le préchargement, et on laissera la reconnexion se
        // déclencher quand la réserve sera réellement épuisée.
        if self.network.is_some() && self.reserve_disponible() {
            self.prefetch_fini = true;
            tracing::debug!(
                "Lecture réseau en échec ({}) — ignoré : {} ms de réserve devant la lecture",
                texte,
                self.audio_buffered_ms()
            );
            return;
        }
        match self.network.as_mut() {
            Some(net) => {
                if net.en_panne {
                    return; // coupure déjà signalée : on ne compte pas deux fois
                }
                net.en_panne = true;
                net.derniere_erreur = Some(code);
                net.tentatives = 0;
                let delai = delai_reconnexion(1);
                net.prochaine_a = Some(std::time::Instant::now() + delai);
                tracing::warn!(
                    "Flux interrompu ({} — code {}) : reconnexion dans {:.1} s{}",
                    texte,
                    code,
                    delai.as_secs_f32(),
                    if net.hls { " (playlist HLS)" } else { "" }
                );
            }
            None => {
                self.eof = true;
                tracing::warn!("Lecture interrompue ({} — code {})", texte, code);
            }
        }
    }

    /// Un EOF est-il la vraie fin du média, ou une coupure déguisée ?
    ///
    /// Le décodeur HLS de FFmpeg **escamote** les erreurs de segment : quand tous
    /// les segments d'une playlist échouent (serveur tombé, réseau coupé), il les
    /// saute un par un en quelques millisecondes puis renvoie `AVERROR_EOF` —
    /// exactement le code d'une fin de playlist légitime, et `avio_feof()` est
    /// vrai dans les deux cas. Le code de retour ne permet donc pas de trancher
    /// (constaté : `ret=-541478725 io_eof=true` sur un flux coupé à 9 s d'une
    /// playlist de 60 s).
    ///
    /// Le critère retenu est la **position** : un flux réseau qui se termine
    /// alors qu'il reste du média connu (durée annoncée non atteinte), ou dont la
    /// durée est inconnue (direct), est traité comme une coupure et reconnecté.
    /// Un flux qui se termine au bout de sa durée est une fin normale.
    fn clore_ou_reconnecte(&mut self) {
        if self.network.is_none() {
            // Fichier local : un EOF est un EOF.
            self.eof = true;
            return;
        }
        let duree = self.total_duration_ms;
        let position = self.current_position_ms;
        if fin_legitime(position, duree) {
            self.eof = true;
            tracing::info!("Fin du flux ({} / {} ms)", position, duree);
            return;
        }
        self.note_stream_error(
            AVERROR_EOF,
            "fin de playlist sans fin de média (segments manquants)",
        );
    }

    /// La lecture a repris : efface l'état de panne et journalise le rétablissement.
    fn note_stream_ok(&mut self) {
        let Some(net) = self.network.as_mut() else { return };
        if !net.en_panne && net.derniere_erreur.is_none() {
            return;
        }
        if net.tentatives > 0 || net.derniere_erreur.is_some() {
            tracing::info!(
                "Flux rétabli ({} tentative(s) de reconnexion) — lecture reprise",
                net.tentatives
            );
        }
        net.en_panne = false;
        net.tentatives = 0;
        net.derniere_erreur = None;
        net.prochaine_a = None;
    }

    /// Vrai si la lecture est suspendue par une coupure réseau en attente d'essai.
    fn flux_en_panne(&self) -> bool {
        self.network.as_ref().map(|n| n.en_panne).unwrap_or(false)
    }

    /// Tente de rouvrir le flux et de reprendre à la position affichée.
    /// Renvoie `true` si la lecture peut continuer.
    pub fn tenter_reconnexion(&mut self) -> bool {
        let Some(mut net) = self.network.take() else {
            return false;
        };
        let url = net.url.clone();
        // Reprise à l'endroit où l'on en est **réellement** : la position vient de
        // l'horloge (carte son maître), pas du dernier paquet vidéo décodé. Avec un
        // pré-cache, les paquets lus sont très en avance sur l'image affichée ; en
        // prenant `current_position_ms` on reprenait plusieurs secondes en arrière
        // (mesuré : reprise à 9 421 ms alors que le son en était à 13 389 ms, soit
        // 4 s de vidéo rejouée).
        // On prend l'**horloge audio** (et non le temps mural de repli) : après une
        // coupure, le repli mural a continué d'avancer pendant le gel et ferait
        // sauter plusieurs secondes de film. Si le son s'est tu, on revient à la
        // dernière image affichée — c'est-à-dire à ce que l'utilisateur a vu.
        let reprise = self
            .audio_clock_ms()
            .map(|ms| ms.max(0.0) as u64)
            .unwrap_or(self.current_position_ms)
            .max(self.current_position_ms);
        // Le compteur est incrémenté par la sonde (c'est elle qui atteste l'échec
        // ou la joignabilité) ; ici on ne fait qu'ouvrir.
        let numero = net.tentatives.max(1);

        tracing::info!(
            "Reconnexion au flux ({}/{}) — reprise à {} ms",
            numero,
            RESEAU_TENTATIVES_MAX,
            reprise
        );

        let resultat = self.load_file(PathBuf::from(&url));

        // `load_file` a recréé son propre état réseau : c'est celui de la
        // reconnexion qui compte (compteur de tentatives, panne).
        if let Some(frais) = self.network.take() {
            net.hls = frais.hls;
        }
        match resultat {
            Ok(_) => {
                net.en_panne = false;
                net.tentatives = 0;
                net.derniere_erreur = None;
                net.prochaine_a = None;
                net.sonde = None;
                self.network = Some(net);
                self.state = PlayerState::Playing;
                // Reprise à l'image affichée au moment de la coupure. Sur un
                // direct, le seek échoue ou retombe au bord du direct : c'est le
                // comportement attendu, on ne saute pas dans le passé.
                if reprise > 0 {
                    match self.seek(reprise) {
                        Ok(()) => tracing::info!("Flux rouvert, reprise à {} ms", reprise),
                        Err(e) => tracing::info!(
                            "Flux rouvert, reprise à {} ms impossible ({}) : lecture au bord du direct",
                            reprise,
                            e
                        ),
                    }
                }
                self.generation = self.generation.wrapping_add(1);
                tracing::info!("Reconnexion réussie (tentative {})", numero);
                true
            }
            Err(e) => {
                // Le serveur répondait à la sonde mais l'ouverture complète a
                // échoué (playlist invalide, segment refusé…) : même traitement.
                tracing::warn!("Ouverture après sonde impossible : {}", e);
                self.network = Some(net);
                self.state = PlayerState::Playing;
                self.sonde_echouee();
                false
            }
        }
    }

    /// Rouvert par la boucle d'interface : effectue la tentative quand elle est due.
    ///
    /// La tentative se fait en deux temps :
    ///   1. une **sonde** en arrière-plan vérifie que le serveur répond (le fil
    ///      d'interface reste donc réactif, même face à un serveur muet) ;
    ///   2. une fois la sonde positive, la réouverture réelle a lieu ici et dure
    ///      quelques centaines de millisecondes, comme l'ouverture initiale.
    pub fn service_network(&mut self) {
        if self.state != PlayerState::Playing {
            return;
        }
        let Some(net) = self.network.as_mut() else { return };
        if !net.en_panne {
            // Une sonde traînante ne doit pas survivre à un rétablissement.
            net.sonde = None;
            return;
        }

        // Sonde déjà lancée : on attend son verdict.
        if let Some(sonde) = net.sonde.as_ref() {
            if !sonde.is_finished() {
                return;
            }
            let joignable = net
                .sonde
                .take()
                .map(|s| s.join().unwrap_or(false))
                .unwrap_or(false);
            if !joignable {
                self.sonde_echouee();
                return;
            }
            // Le serveur répond : réouverture réelle (rapide, elle aboutit).
            self.tenter_reconnexion();
            return;
        }

        // Nouvelle tentative due ? On lance la sonde, jamais l'ouverture bloquante.
        if let Some(quand) = net.prochaine_a {
            if std::time::Instant::now() < quand {
                return;
            }
        }
        let url = net.url.clone();
        net.sonde = Some(std::thread::spawn(move || sonde_url(&url)));
    }

    /// La sonde a échoué : compte une tentative et replanifie (ou abandonne).
    fn sonde_echouee(&mut self) {
        let Some(net) = self.network.as_mut() else { return };
        net.tentatives += 1;
        let numero = net.tentatives;
        if numero >= RESEAU_TENTATIVES_MAX {
            let message = format!(
                "Flux interrompu : {} tentative(s) de reconnexion sans succès",
                numero
            );
            tracing::error!("{}", message);
            net.en_panne = false;
            net.prochaine_a = None;
            net.sonde = None;
            self.state = PlayerState::Error(message);
            return;
        }
        let delai = delai_reconnexion(numero + 1);
        net.prochaine_a = Some(std::time::Instant::now() + delai);
        tracing::warn!(
            "Reconnexion {} : source injoignable — nouvel essai dans {:.1} s",
            numero,
            delai.as_secs_f32()
        );
    }

    /// Vrai si le média courant est un flux réseau (et non un fichier local).
    pub fn est_flux_reseau(&self) -> bool {
        self.network.is_some()
    }

    /// Vrai si un flux réseau est en cours de reconnexion (pour l'affichage).
    pub fn reconnexion_en_cours(&self) -> bool {
        self.network
            .as_ref()
            .map(|n| n.en_panne || n.prochaine_a.is_some())
            .unwrap_or(false)
    }

    /// Remet à zéro l'état de surveillance de l'horloge audio (seek, pause, arrêt,
    /// changement de média) : la surveillance repart de la nouvelle origine.
    fn reset_clock_watch(&mut self) {
        self.audio_clock_seen_ms = None;
        self.audio_clock_seen_at = None;
        self.audio_clock_stalled = false;
        self.clock_report_at = None;
        self.buffered_min_ms = u64::MAX;
        self.buffered_max_ms = 0;
    }

    /// Recale l'horloge de lecture sur le PTS d'une frame précise (après load/seek).
    pub fn sync_clock_to(&mut self, pts_ms: u64) {
        self.clock_start = Some(std::time::Instant::now());
        self.clock_base_pts_ms = pts_ms;
    }

    /// Numéro de génération : change à chaque load/seek/reprise.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Vrai si le média est terminé (fin de flux atteinte) : sert à l'enchaînement
    /// automatique de la playlist dans l'interface.
    pub fn is_finished(&self) -> bool {
        self.eof && self.state == PlayerState::Stopped
    }

    pub fn current_position_ms(&self) -> u64 {
        self.current_position_ms
    }
    pub fn total_duration_ms(&self) -> u64 {
        self.total_duration_ms
    }

    pub fn seek(&mut self, position_ms: u64) -> Result<(), String> {
        let format_context = match self.format_context {
            Some(fc) => fc,
            None => return Err("Aucun média chargé".to_string()),
        };
        // On cible le stream vidéo dans SA base de temps : plus précis que
        // l'échelle globale (stream_index = -1), qui atterrit souvent sur un
        // keyframe éloigné de la cible.
        let (stream_index, time_base) = match self.video_stream.as_ref() {
            Some(vs) => (vs.index, vs.time_base),
            None => (-1, AVRational { num: 1, den: AV_TIME_BASE }),
        };

        unsafe {
            let target_us = (position_ms as i64) * 1000; // µs
            let target_ts = if stream_index >= 0 && time_base.den > 0 {
                av_rescale_q(
                    target_us,
                    AVRational { num: 1, den: AV_TIME_BASE },
                    time_base,
                )
            } else {
                target_us
            };
            // On accepte les keyframes jusqu'à 500 ms après la cible pour
            // resserrer d'éventuelles imprécisions.
            let max_ts = target_ts + 500;
            let ret = avformat_seek_file(
                format_context,
                stream_index,
                i64::MIN,
                target_ts,
                max_ts,
                AVSEEK_FLAG_BACKWARD,
            );
            if ret >= 0 {
                // Vider les buffers du décodeur pour éviter les frames périmées
                if let Some(vs) = self.video_stream.as_ref() {
                    avcodec_flush_buffers(vs.codec_context);
                }
                self.pending_video.clear();
                self.flush_video_packets();
                // Les événements de sous-titres déjà décodés appartiennent au
                // voisinage de l'ancienne position. On les oublie : un événement
                // image sans extinction (end_ms == 0) resterait sinon affiché
                // indéfiniment après un saut dans le temps.
                self.embedded_cues.clear();
                self.bitmap_cues.clear();
                self.seek_target_ms = Some(position_ms);
                if let Some(out) = self.audio_output.as_ref() {
                    out.clear();
                }
                // Tampon vidé : le préremplissage doit avoir lieu de nouveau, sinon
                // la carte ressort du silence pendant le premier tour qui suit le saut.
                self.audio_prefill_fait = false;
                self.audio_prefill_depuis = None;
                if let Some(aus) = self.audio_stream.as_ref() {
                    avcodec_flush_buffers(aus.codec_context);
                }
                self.eof = false;
                self.current_position_ms = position_ms;
                // Recaler l'horloge sur la nouvelle position
                self.clock_start = Some(std::time::Instant::now());
                self.clock_base_pts_ms = position_ms;
                self.reset_clock_watch();
                self.generation = self.generation.wrapping_add(1);
                tracing::info!("Seek à {} ms", position_ms);
                Ok(())
            } else {
                Err(format!("Erreur seek: code {}", ret))
            }
        }
    }

    pub fn toggle_pause(&mut self) -> Result<(), String> {
        // Une pause décidée par l'utilisateur ne doit pas être levée par la boucle A-B.
        // (Le propre appel de la boucle repose `ab_reprise_a` juste après.)
        self.ab_reprise_a = None;
        match self.state {
            PlayerState::Playing => {
                self.state = PlayerState::Paused;
                // Le tampon audio N'EST PAS vidé : il conserve les ~120 ms déjà
                // décodées, et l'horloge audio (donc la position du son) reste où
                // elle est. À la reprise, image et son repartent du même instant.
                // Le vider obligerait à réancrer l'horloge sur la position du
                // déméruxeur — en avance sur l'image — et décalerait le son.
                self.reset_clock_watch();
                self.sync_audio_state();
                tracing::info!("Pause");
                Ok(())
            }
            PlayerState::Paused | PlayerState::Idle | PlayerState::Stopped => {
                // Reprendre l'horloge en repartant de la position courante
                self.clock_start = Some(std::time::Instant::now());
                self.clock_base_pts_ms = self.current_position_ms;
                self.reset_clock_watch();
                self.eof = false;
                self.generation = self.generation.wrapping_add(1);
                self.state = PlayerState::Playing;
                self.sync_audio_state();
                tracing::info!("Lecture");
                Ok(())
            }
            _ => Err("État invalide".to_string()),
        }
    }

    pub fn stop(&mut self) -> Result<(), String> {
        self.state = PlayerState::Stopped;
        // Arrêt demandé : on n'insiste pas sur un flux injoignable.
        if let Some(net) = self.network.as_mut() {
            net.en_panne = false;
            net.prochaine_a = None;
        }
        self.current_position_ms = 0;
        self.clock_start = None;
        if let Some(out) = self.audio_output.as_ref() {
            out.clear();
        }
        self.audio_prefill_fait = false;
        self.audio_prefill_depuis = None;
        self.reset_clock_watch();
        self.sync_audio_state();
        tracing::info!("Arrêt");
        Ok(())
    }

    /// Élément suivant, selon le mode courant (linéaire ou aléatoire).
    ///
    /// Renvoie l'**index** sans rien charger : l'interface doit passer par son
    /// propre chemin (`load_media`), qui applique les règles de profil — sinon un
    /// profil automatique ne s'appliquerait qu'aux fichiers ouverts autrement.
    pub fn prochain_index(&self) -> Option<usize> {
        if self.playlist.is_empty() {
            return None;
        }
        if self.shuffle && self.ordre_aleatoire.len() == self.playlist.len() {
            let rang = (self.rang_aleatoire + 1) % self.ordre_aleatoire.len();
            return self.ordre_aleatoire.get(rang).copied();
        }
        Some((self.playlist_index + 1) % self.playlist.len())
    }

    /// Élément précédent, même logique que `prochain_index`.
    pub fn precedent_index(&self) -> Option<usize> {
        if self.playlist.is_empty() {
            return None;
        }
        if self.shuffle && self.ordre_aleatoire.len() == self.playlist.len() {
            let n = self.ordre_aleatoire.len();
            let rang = (self.rang_aleatoire + n - 1) % n;
            return self.ordre_aleatoire.get(rang).copied();
        }
        Some(if self.playlist_index == 0 {
            self.playlist.len() - 1
        } else {
            self.playlist_index - 1
        })
    }

    /// Active ou coupe la lecture aléatoire.
    ///
    /// À l'activation, l'ordre est mélangé **en gardant l'élément courant en tête
    /// du parcours** : on continue sur la chanson qu'on écoute, puis on découvre le
    /// reste — couper la lecture pour « commencer au hasard » serait une surprise
    /// désagréable. À la coupure, on revient à l'ordre de la liste.
    pub fn set_shuffle(&mut self, actif: bool) {
        if self.shuffle == actif {
            return;
        }
        self.shuffle = actif;
        if actif {
            self.remelanger();
            tracing::info!(
                "Lecture aléatoire : activée ({} élément(s), départ sur {})",
                self.playlist.len(),
                self.playlist_index
            );
        } else {
            self.ordre_aleatoire.clear();
            self.rang_aleatoire = 0;
            tracing::info!("Lecture aléatoire : désactivée (ordre de la liste)");
        }
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    /// Reconstruit la permutation et place le rang sur l'élément courant.
    fn remelanger(&mut self) {
        let n = self.playlist.len();
        let mut ordre = melange(n, graine_aleatoire());
        // L'élément courant passe en tête du parcours.
        if let Some(pos) = ordre.iter().position(|i| *i == self.playlist_index) {
            ordre.swap(0, pos);
        }
        self.ordre_aleatoire = ordre;
        self.rang_aleatoire = 0;
    }

    pub fn add_to_playlist(&mut self, path: PathBuf) {
        self.playlist.push(path);
        self.invalider_ordre_aleatoire();
    }

    /// L'ordre mélangé ne survit pas à un changement de la liste : il contiendrait
    /// des indices qui ne désignent plus les mêmes fichiers (ou plus rien du tout).
    fn invalider_ordre_aleatoire(&mut self) {
        if self.shuffle {
            self.remelanger();
        } else {
            self.ordre_aleatoire.clear();
            self.rang_aleatoire = 0;
        }
    }

    /// Déplace un élément dans la playlist (glisser-déposer), en suivant
    /// l'élément **en cours de lecture** : s'il est déplacé, son index change, et
    /// l'index courant doit suivre — sinon la barre de playlist désignerait un
    /// autre fichier que celui qui joue.
    ///
    /// `vers` est la position d'insertion **après** retrait de l'élément (de 0 à
    /// len-1). Retourne le nouvel index courant.
    pub fn deplacer_dans_playlist(&mut self, de: usize, vers: usize) -> Option<usize> {
        let n = self.playlist.len();
        if de >= n || vers >= n || de == vers {
            return None;
        }
        let element = self.playlist.remove(de);
        self.playlist.insert(vers, element);

        // Suivi de l'index courant : trois cas, et il faut les trois.
        let courant = self.playlist_index;
        self.playlist_index = if courant == de {
            vers
        } else if de < courant && vers >= courant {
            courant - 1
        } else if de > courant && vers <= courant {
            courant + 1
        } else {
            courant
        };
        self.invalider_ordre_aleatoire();
        Some(self.playlist_index)
    }

    /// Index de l'élément courant dans la playlist.
    pub fn playlist_index(&self) -> usize {
        self.playlist_index
    }

    /// Charge l'élément `index` de la playlist.
    pub fn play_index(&mut self, index: usize) -> Result<MediaInfo, String> {
        let path = self
            .playlist
            .get(index)
            .cloned()
            .ok_or_else(|| "Index de playlist invalide".to_string())?;
        self.playlist_index = index;
        // Choix explicite : en lecture aléatoire, le parcours reprend **à cet
        // élément** (sinon le suivant tiré ignorerait le clic de l'utilisateur).
        if let Some(pos) = self.ordre_aleatoire.iter().position(|i| *i == index) {
            self.rang_aleatoire = pos;
        }
        tracing::info!("Playlist: lecture de l'élément {}", index);
        self.load_file(path)
    }

    /// Retire l'élément `index` de la playlist (ajuste l'index courant).
    pub fn remove_from_playlist(&mut self, index: usize) {
        if index >= self.playlist.len() {
            return;
        }
        self.playlist.remove(index);
        if self.playlist.is_empty() {
            self.playlist_index = 0;
        } else if index < self.playlist_index {
            self.playlist_index -= 1;
        } else if self.playlist_index >= self.playlist.len() {
            self.playlist_index = self.playlist.len() - 1;
        }
        self.invalider_ordre_aleatoire();
    }

    /// Avance l'index de playlist d'un cran (utilisé par l'enchaînement auto).
    ///
    /// En lecture aléatoire, « un cran » suit l'ordre mélangé : sans cela,
    /// l'enchaînement automatique rejouerait la liste dans l'ordre alors que le
    /// bouton ▶︎ suivant, lui, la parcourt au hasard — deux comportements pour la
    /// même intention.
    pub fn playlist_index_advance(&mut self) {
        if self.playlist.is_empty() {
            return;
        }
        if self.shuffle {
            if self.ordre_aleatoire.len() != self.playlist.len() {
                self.remelanger();
            }
            if !self.ordre_aleatoire.is_empty() {
                self.rang_aleatoire = (self.rang_aleatoire + 1) % self.ordre_aleatoire.len();
                self.playlist_index = self.ordre_aleatoire[self.rang_aleatoire];
            }
            return;
        }
        self.playlist_index = (self.playlist_index + 1) % self.playlist.len();
    }


    pub fn state(&self) -> PlayerState {
        self.state.clone()
    }
    pub fn media_info(&self) -> Option<&MediaInfo> {
        self.media_info.as_ref()
    }
    pub fn get_speed(&self) -> f32 {
        self.speed
    }

    pub fn set_speed(&mut self, speed: f32) {
        let speed = speed.clamp(VITESSE_MIN as f32, VITESSE_MAX as f32);
        if (speed - self.speed).abs() < 0.001 {
            return;
        }
        self.speed = speed;
        tracing::info!("Vitesse: {:.2}x", self.speed);
        // Sans cette reconversion, la vidéo accélérerait pendant que l'audio
        // continuerait à son rythme : décalage croissant garanti. Deux voies :
        //   - hauteur préservée (défaut) : `atempo` dans la chaîne audio, à taux
        //     d'échantillonnage constant ;
        //   - sinon : rééchantillonnage (la hauteur suit la vitesse).
        self.rebuild_audio_resampler();
        self.refresh_audio_chain();
    }

    /// Vrai si la vitesse doit préserver la hauteur du son.
    fn respecte_hauteur(&self) -> bool {
        self.audio_pitch_preserved && (self.speed - 1.0).abs() > 1e-3
    }

    /// Reconstruit la description de la chaîne audio à partir de l'état courant
    /// (réglages + vitesse + format du périphérique).
    ///
    /// La chaîne dépend de la vitesse (`atempo`) : elle doit donc être recalculée
    /// quand la vitesse change, et pas seulement quand les réglages changent.
    pub(crate) fn refresh_audio_chain(&mut self) {
        let description = self.audio_description_courante();
        if description == self.audio_filters_description {
            return;
        }
        self.audio_filters_description = description;
        // Le graphe est reconstruit à la première trame suivante.
        self.audio_filters = None;
        self.audio_filters_failed = None;
    }

    /// Description que devrait avoir la chaîne audio maintenant.
    fn audio_description_courante(&self) -> String {
        let canaux = self
            .audio_output
            .as_ref()
            .map(|o| o.channels() as i32)
            .unwrap_or(2);
        audio_filter_description(
            &self.audio_settings,
            self.speed,
            self.audio_pitch_preserved,
            canaux,
        )
    }

    /// Reconstruit le convertisseur audio à la vitesse courante.
    ///
    /// Le périphérique consomme toujours `rate_device` échantillons par seconde :
    /// pour que N secondes de média durent 1 s, il faut lui fournir
    /// `rate_device / N` échantillons par seconde de média.
    fn rebuild_audio_resampler(&mut self) {
        let Some(device_rate) = self.audio_output.as_ref().map(|o| o.sample_rate()) else {
            return;
        };
        // Lu AVANT l'emprunt mutable de `audio_stream`.
        let hauteur_preservee = self.respecte_hauteur();
        let Some(aus) = self.audio_stream.as_mut() else {
            return;
        };
        if aus.codec_context.is_null() {
            return;
        }

        // Avec `atempo` dans la chaîne (hauteur préservée), la vitesse ne doit PAS
        // être obtenue par rééchantillonnage : c'est le filtre qui comprime ou
        // étire la durée, à taux d'échantillonnage constant. Rééchantillonner en
        // plus changerait la hauteur deux fois.
        let facteur = if hauteur_preservee { 1.0 } else { self.speed.max(0.1) };
        let out_rate = ((device_rate as f32 / facteur).round() as i32).clamp(8_000, 384_000);
        let ctx = unsafe { &*aus.codec_context };

        let mut swr: *mut SwrContext = std::ptr::null_mut();
        let ret = unsafe {
            swr_alloc_set_opts2(
                &mut swr,
                &aus.out_layout,
                AVSampleFormat::AV_SAMPLE_FMT_FLT,
                out_rate,
                &ctx.ch_layout,
                ctx.sample_fmt,
                ctx.sample_rate,
                0,
                std::ptr::null_mut(),
            )
        };
        if ret < 0 || swr.is_null() || unsafe { swr_init(swr) } < 0 {
            tracing::warn!("Rééchantillonnage audio pour la vitesse impossible");
            unsafe {
                if !swr.is_null() {
                    swr_free(&mut swr);
                }
            }
            return;
        }

        unsafe {
            if !aus.swr.is_null() {
                swr_free(&mut aus.swr);
            }
        }
        aus.swr = swr;
        aus.out_sample_rate = out_rate;
        // Le tampon contient encore de l'audio à l'ancienne cadence
        if let Some(out) = self.audio_output.as_ref() {
            out.clear();
        }
        if hauteur_preservee {
            tracing::info!(
                "Vitesse {:.2}x : hauteur préservée (atempo, {} Hz inchangé)",
                self.speed,
                device_rate
            );
        } else {
            tracing::info!(
                "Audio rééchantillonné pour {:.2}x ({} Hz → {} Hz, hauteur modifiée)",
                self.speed,
                device_rate,
                out_rate
            );
        }
    }

    pub fn get_volume(&self) -> f32 {
        self.volume
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, self.volume_max as f32 / 100.0);
        self.sync_audio_state();
        tracing::info!("Volume: {:.0}%", self.volume * 100.0);
    }

    /// Vrai si le son est coupé (pour que le menu annonce l'action à venir).
    pub fn is_muted(&self) -> bool {
        self.muted
    }

    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        self.sync_audio_state();
        tracing::info!("Mute: {}", self.muted);
    }

    pub fn toggle_loop(&mut self) {
        self.loop_enabled = !self.loop_enabled;
        tracing::info!("Boucle: {}", self.loop_enabled);
    }

    /// Ferme le média courant et libère toutes les ressources FFmpeg associées.
    fn close_media(&mut self) {
        // Mémorise la position avant de tout libérer (sauf option désactivée).
        // Jamais pour un flux réseau : une position dans un direct n'a pas de sens,
        // et la mémoriser ferait « reprendre » un flux à un endroit disparu.
        if self.resume_enabled && !self.est_flux_reseau() {
            if let Some(path) = self.current_path.clone() {
                let position = self.current_position_ms;
                let duration = self.total_duration_ms;
                if position > 0 {
                    self.positions.remember(&path, position, duration);
                }
            }
        }
        // Contrôle d'intégrité de la chaîne de filtres : tous les filtres utilisés
        // sont 1 image → 1 image. Un écart signalerait un filtre qui change la
        // cadence (la synchronisation du moteur en dépend).
        if let Some(vs) = self.video_stream.as_ref() {
            if let Some(chain) = vs.filters.as_ref() {
                if chain.frames_in != chain.frames_out {
                    tracing::warn!(
                        "Chaîne de filtres déséquilibrée : {} image(s) entrée(s), {} sortie(s)",
                        chain.frames_in,
                        chain.frames_out
                    );
                } else {
                    tracing::debug!(
                        "Chaîne de filtres équilibrée : {} image(s) dans les deux sens",
                        chain.frames_in
                    );
                }
            }
        }

        unsafe {
            if let Some(vs) = self.video_stream.take() {
                if !vs.codec_context.is_null() {
                    let mut p = vs.codec_context;
                    avcodec_free_context(&mut p);
                }
                if !vs.scale_context.is_null() {
                    sws_freeContext(vs.scale_context);
                }
            }
            if let Some(mut aus) = self.audio_stream.take() {
                if !aus.swr.is_null() {
                    swr_free(&mut aus.swr);
                }
                if !aus.codec_context.is_null() {
                    let mut p = aus.codec_context;
                    avcodec_free_context(&mut p);
                }
            }
            // La sortie audio n'est PAS détruite : sa destruction attend la fin du
            // thread audio (ce qui peut bloquer). On la met simplement en pause.
            if let Some(out) = self.audio_output.as_ref() {
                out.set_playing(false);
                out.clear();
            }
            if let Some(mut fc) = self.format_context.take() {
                avformat_close_input(&mut fc);
            }
            // Le contexte de format est fermé : le pointeur confié à FFmpeg
            // (`interrupt_callback.opaque`) n'est plus utilisé, le garde-temps
            // peut être libéré.
            self.delai_reseau = None;
        }
        self.last_frame_dims = (0, 0);
        self.pending_video.clear();
        self.flush_video_packets();
        self.eof = false;
        self.clock_start = None;
        self.clock_base_pts_ms = 0;
        self.current_path = None;
    }

    pub fn quit(&mut self) -> Result<(), String> {
        self.close_media();
        // Le périphérique VA-API est partagé par tous les médias : il n'est
        // libéré qu'à la fermeture du lecteur.
        unsafe {
            if let Some(device) = self.hw_device.take() {
                let mut d = device;
                av_buffer_unref(&mut d);
            }
        }
        self.state = PlayerState::Idle;
        tracing::info!("KPlayer quitté");
        Ok(())
    }
}

/// Décode un paquet de sous-titres et ajoute les événements à la file.
///
/// **Pourquoi les durées sont relatives** : un paquet de sous-titres contient
/// `start_display_time` / `end_display_time` en millisecondes **par rapport à son
/// propre PTS**. Les ajouter donne l'intervalle absolu, seule forme utilisable pour
/// savoir quel texte afficher à un instant donné.
///
/// **Pourquoi le texte des événements ASS est découpé** : le décodeur renvoie
/// `ReadOrder,Layer,Style,Name,MarginL,MarginR,MarginV,Effect,Text` — le texte est
/// après le 8e champ. Il contient encore les balises de mise en forme (`{\pos…}`),
/// retirées ici : le lecteur affiche du texte simple.
unsafe fn collect_subtitle_packet(
    sub: &SubtitleStream,
    packet: *mut AVPacket,
    file: &mut VecDeque<EmbeddedCue>,
    images_file: &mut VecDeque<BitmapCue>,
) {
    if sub.codec_context.is_null() {
        return;
    }
    let mut av_sub: AVSubtitle = std::mem::zeroed();
    let mut recu: c_int = 0;
    let ret = avcodec_decode_subtitle2(sub.codec_context, &mut av_sub, &mut recu, packet);
    if ret < 0 || recu == 0 {
        return;
    }

    // Position du paquet en millisecondes (base de temps du flux de sous-titres).
    let pts_ms = if (*packet).pts >= 0 && sub.time_base.den != 0 {
        ((*packet).pts as i128 * sub.time_base.num as i128 * 1000 / sub.time_base.den as i128)
            .max(0) as u64
    } else if (*packet).dts >= 0 && sub.time_base.den != 0 {
        ((*packet).dts as i128 * sub.time_base.num as i128 * 1000 / sub.time_base.den as i128)
            .max(0) as u64
    } else {
        0
    };

    let debut = pts_ms + av_sub.start_display_time as u64;
    // Durée de l'événement, dans l'ordre des sources fiables :
    //  1. `end_display_time` du décodeur (format SRT « externe », ASS…) ;
    //  2. **`duration` du paquet**, en unités de la base de temps du flux — c'est
    //     là que Matroska range la durée d'un sous-titre, et le décodeur SubRip ne
    //     la reporte PAS (`end_display_time` valait 0 : les cues duraient 3 s au
    //     lieu de 2 s, mesuré sur un MKV de référence) ;
    //  3. à défaut, 3 s : mieux qu'un sous-titre jamais effacé.
    let duree = if av_sub.end_display_time > av_sub.start_display_time {
        (av_sub.end_display_time - av_sub.start_display_time) as u64
    } else if (*packet).duration > 0 && sub.time_base.den != 0 {
        ((*packet).duration as i128 * sub.time_base.num as i128 * 1000
            / sub.time_base.den as i128)
            .max(0) as u64
    } else {
        3000
    };
    let fin = debut + duree;

    let mut morceaux: Vec<String> = Vec::new();
    let mut images: Vec<BitmapRect> = Vec::new();
    for i in 0..av_sub.num_rects as usize {
        let rect = *av_sub.rects.add(i);
        if rect.is_null() {
            continue;
        }
        let r = &*rect;
        let texte = match r.type_ {
            AVSubtitleType::SUBTITLE_ASS => {
                if r.ass.is_null() {
                    None
                } else {
                    let brut = std::ffi::CStr::from_ptr(r.ass).to_string_lossy().to_string();
                    // 9 champs séparés par des virgules, le 9e est le texte.
                    let mut iter = brut.splitn(9, ',');
                    for _ in 0..8 {
                        iter.next();
                    }
                    iter.next().map(|t| nettoyer_balises_ass(t))
                }
            }
            AVSubtitleType::SUBTITLE_TEXT => {
                if r.text.is_null() {
                    None
                } else {
                    Some(
                        std::ffi::CStr::from_ptr(r.text)
                            .to_string_lossy()
                            .to_string(),
                    )
                }
            }
            // Image (PGS/DVD/DVB) : pas de texte, mais un ou plusieurs rectangles
            // d'indices de palette à convertir en RGBA pour la composition.
            AVSubtitleType::SUBTITLE_BITMAP => {
                if let Some(rect_image) = unsafe { rect_bitmap_vers_image(r) } {
                    images.push(rect_image);
                }
                None
            }
            _ => None,
        };
        if let Some(t) = texte {
            let t = t.replace("\r", "").trim().to_string();
            if !t.is_empty() {
                morceaux.push(t);
            }
        }
    }
    avsubtitle_free(&mut av_sub);

    if !images.is_empty() {
        images_file.push_back(BitmapCue {
            start_ms: debut,
            end_ms: fin,
            rects: images,
        });
        // Éviction par volume ET par nombre : le plus ancien part d'abord.
        let mut total: usize = images_file.iter().map(bitmap_cue_bytes).sum();
        while !images_file.is_empty()
            && (total > MAX_BITMAP_BYTES || images_file.len() > MAX_BITMAP_CUES)
        {
            if let Some(vieux) = images_file.pop_front() {
                total = total.saturating_sub(bitmap_cue_bytes(&vieux));
            }
        }
    } else if !images_file.is_empty()
        && images_file
            .back()
            .map(|c| c.end_ms == 0 || c.end_ms > debut)
            .unwrap_or(false)
    {
        // Composition **sans objet** : c'est ainsi qu'un flux PGS éteint le
        // sous-titre (un « display set » vide). On termine l'événement en cours
        // au lieu de le laisser affiché indéfiniment.
        if let Some(dernier) = images_file.back_mut() {
            dernier.end_ms = debut;
        }
    }

    if !morceaux.is_empty() {
        file.push_back(EmbeddedCue {
            start_ms: debut,
            end_ms: fin,
            text: morceaux.join("\n"),
        });
        // Borne : au-delà, on jette les événements les plus anciens. Un film de
        // deux heures très bavard reste ainsi sous quelques milliers d'entrées.
        const MAX_CUES: usize = 4000;
        while file.len() > MAX_CUES {
            file.pop_front();
        }
    }
}

/// Convertit un rectangle de sous-titre image en RGBA (voir `bitmap_vers_rgba`).
///
/// Sûreté : ne lit que ce que FFmpeg a rempli, en vérifiant les pointeurs et la
/// taille des lignes. Un rectangle incohérent est ignoré (mieux vaut pas de
/// sous-titre qu'un plantage sur un flux mal formé).
unsafe fn rect_bitmap_vers_image(r: &AVSubtitleRect) -> Option<BitmapRect> {
    if r.data[0].is_null() || r.w <= 0 || r.h <= 0 {
        return None;
    }
    let linesize = r.linesize[0];
    if linesize < r.w {
        return None;
    }
    let (w, h) = (r.w as u32, r.h as u32);
    let totale = (linesize as usize) * (h as usize);
    if totale > 64 * 1024 * 1024 {
        return None; // garde-fou : un rectangle de cette taille est incohérent
    }
    let indices = std::slice::from_raw_parts(r.data[0], totale);

    // La palette compte `nb_colors` entrées (4 pour le DVD, jusqu'à 256 en PGS).
    let nb = r.nb_colors.max(0) as usize;
    let palette: Vec<u32> = if r.data[1].is_null() || nb == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(r.data[1] as *const u32, nb).to_vec()
    };

    let rgba = bitmap_vers_rgba(indices, linesize as usize, w, h, &palette)?;
    Some(BitmapRect {
        x: r.x,
        y: r.y,
        width: w,
        height: h,
        rgba,
    })
}

/// Retire les balises de mise en forme ASS et convertit les sauts de ligne.
fn nettoyer_balises_ass(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    let mut dans_balise = false;
    for c in texte.chars() {
        match c {
            '{' => dans_balise = true,
            '}' => dans_balise = false,
            _ if dans_balise => {}
            _ => sortie.push(c),
        }
    }
    sortie.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")
}

/// Mesure les bandes sombres autour d'une image RGBA.
///
/// Une ligne (ou colonne) est considérée comme « bande noire » si sa luminance
/// moyenne est sous le seuil **et** si son écart-type est faible : une zone très
/// sombre mais texturée (ciel de nuit) n'est pas une bande.
fn bordures_sombres(rgba: &[u8], largeur: u32, hauteur: u32, seuil: u8) -> (u32, u32, u32, u32) {
    let w = largeur as usize;
    let h = hauteur as usize;
    if rgba.len() < w * h * 4 {
        return (0, 0, 0, 0);
    }
    let luma = |x: usize, y: usize| -> u8 {
        let i = (y * w + x) * 4;
        // Approximation entière de la luminance (0,299 R + 0,587 V + 0,114 B).
        ((rgba[i] as u32 * 77 + rgba[i + 1] as u32 * 150 + rgba[i + 2] as u32 * 29) >> 8) as u8
    };

    // Une bande est sombre ET uniforme : un ciel nocturne est sombre mais bruité.
    let ligne_bande = |y: usize| -> bool {
        let mut somme = 0u32;
        let mut somme2 = 0u32;
        for x in 0..w {
            let v = luma(x, y) as u32;
            somme += v;
            somme2 += v * v;
        }
        let moyenne = somme / w as u32;
        let variance = somme2 / w as u32 - moyenne * moyenne;
        moyenne <= seuil as u32 && variance < 64
    };
    let colonne_bande = |x: usize| -> bool {
        let mut somme = 0u32;
        let mut somme2 = 0u32;
        for y in 0..h {
            let v = luma(x, y) as u32;
            somme += v;
            somme2 += v * v;
        }
        let moyenne = somme / h as u32;
        let variance = somme2 / h as u32 - moyenne * moyenne;
        moyenne <= seuil as u32 && variance < 64
    };

    let mut haut = 0;
    while haut < h / 2 && ligne_bande(haut) {
        haut += 1;
    }
    let mut bas = 0;
    while bas < h / 2 && ligne_bande(h - 1 - bas) {
        bas += 1;
    }
    let mut gauche = 0;
    while gauche < w / 2 && colonne_bande(gauche) {
        gauche += 1;
    }
    let mut droite = 0;
    while droite < w / 2 && colonne_bande(w - 1 - droite) {
        droite += 1;
    }
    (haut as u32, bas as u32, gauche as u32, droite as u32)
}

/// Lit une métadonnée de flux (`language`, `title`…).
unsafe fn stream_metadata(dict: *mut AVDictionary, cle: &str) -> String {
    if dict.is_null() {
        return String::new();
    }
    let Ok(cle_c) = CString::new(cle) else {
        return String::new();
    };
    let entree = av_dict_get(dict, cle_c.as_ptr(), std::ptr::null(), 0);
    if entree.is_null() || (*entree).value.is_null() {
        return String::new();
    }
    std::ffi::CStr::from_ptr((*entree).value)
        .to_string_lossy()
        .to_string()
}

/// Choisit une piste selon une langue demandée.
///
/// Règles, dans l'ordre :
///  1. langue exacte (`fre` demandé, `fre` trouvé) ;
///  2. préfixe de deux lettres (`fr` demandé, `fre`/`fra` trouvé) — les conteneurs
///     utilisent indifféremment les codes ISO 639-1 et 639-2 ;
///  3. sinon la première piste : un fichier sans métadonnée de langue doit rester
///     lisible exactement comme avant.
fn choisir_piste<'a>(pistes: &'a [TrackInfo], langue: &str) -> Option<&'a TrackInfo> {
    if pistes.is_empty() {
        return None;
    }
    let demande = langue.trim().to_lowercase();
    if !demande.is_empty() {
        if let Some(t) = pistes
            .iter()
            .find(|t| t.language.to_lowercase() == demande)
        {
            return Some(t);
        }
        let court = &demande[..demande.len().min(2)];
        if let Some(t) = pistes
            .iter()
            .find(|t| t.language.to_lowercase().starts_with(court))
        {
            return Some(t);
        }
    }
    pistes.first()
}

/// Traduit les réglages en une chaîne de filtres FFmpeg (libavfilter).
///
/// C'est **le seul endroit** où un réglage de `Config` devient un filtre : la chaîne
/// produite est journalisée (`Chaîne de filtres vidéo: …`), donc vérifiable sans
/// lire le code. Vide = aucun filtre, aucun coût, aucune modification de l'image.
///
/// Les valeurs sont bornées ici (et non dans l'interface) pour qu'un fichier de
/// configuration modifié à la main ne puisse pas produire une chaîne invalide.
fn filter_description(video: &crate::config::VideoSettings) -> String {
    let mut filters: Vec<String> = Vec::new();

    // Désentrelacement : `deint=1` = seulement les trames marquées entrelacées,
    // `mode=send_frame` = une image en entrée, une image en sortie (la cadence du
    // moteur suppose 1:1 ; `send_field` la doublerait).
    if video.deinterlace {
        let parite = match video.deinterlace_parity {
            crate::config::DeinterlaceParity::Auto => "auto",
            crate::config::DeinterlaceParity::TopFirst => "tff",
            crate::config::DeinterlaceParity::BottomFirst => "bff",
        };
        // `bwdif` et `yadif` acceptent les mêmes options (`mode`, `parity`, `deint`) :
        // le choix se réduit donc au nom du filtre, et les deux sont vérifiés par
        // mesure (`tools/bench-desentrelacement.sh`) : `bwdif` l'emporte sur les
        // scènes calmes (49,0 contre 43,4 dB) à coût inférieur.
        filters.push(format!(
            "{}=mode=send_frame:parity={}:deint=interlaced",
            video.deinterlace_filter.nom(),
            parite
        ));
    }

    // Débruitage temporel + spatial (4 valeurs : luma spatial, luma temporel,
    // chroma spatial, chroma temporel). Les forces ne sont PAS une progression
    // arbitraire : elles ont été mesurées sur un bruit gaussien de test, où
    //   8:8:6:6      réduit l'énergie haute fréquence de 1 %   (imperceptible)
    //   25:25:20:20  la réduit de 17 %
    //   60:60:50:50  la réduit de 37 %
    // D'où les trois niveaux ci-dessous (1 = léger pour du bruit de compression,
    // 3 = fort, pour une source très bruitée). Voir tools/verif-filtres.sh.
    let forces: [&str; 4] = ["", "8:8:6:6", "25:25:20:20", "60:60:50:50"];
    if video.denoise > 0 {
        let niveau = video.denoise.min(3) as usize;
        filters.push(format!("hqdn3d={}", forces[niveau]));
    }

    // Détramage. Les valeurs sont écrites explicitement (au lieu de laisser les
    // défauts de FFmpeg) : la chaîne journalisée montre alors EXACTEMENT ce qui est
    // appliqué, y compris quand l'utilisateur a modifié le rayon ou le seuil.
    // Défauts FFmpeg : range=16, seuils=0,02 — ce sont aussi nos valeurs par défaut.
    if video.deband {
        let range = video.deband_range.clamp(4, 64);
        let seuil = video.deband_threshold.clamp(0.001, 0.2);
        filters.push(format!(
            "deband=range={}:1thr={:.4}:2thr={:.4}:3thr={:.4}",
            range, seuil, seuil, seuil
        ));
    }

    // Réglages d'image. `eq` n'est inséré que si un paramètre s'écarte du neutre :
    // un filtre identité coûterait un passage complet sur chaque image.
    let brightness = video.brightness.clamp(-1.0, 1.0);
    let contrast = video.contrast.clamp(0.0, 3.0);
    let saturation = video.saturation.clamp(0.0, 3.0);
    let gamma = video.gamma.clamp(0.1, 10.0);
    if brightness.abs() > 1e-3
        || (contrast - 1.0).abs() > 1e-3
        || (saturation - 1.0).abs() > 1e-3
        || (gamma - 1.0).abs() > 1e-3
    {
        filters.push(format!(
            "eq=brightness={:.3}:contrast={:.3}:saturation={:.3}:gamma={:.3}",
            brightness, contrast, saturation, gamma
        ));
    }

    // Teinte : `hue` tourne les couleurs autour du cercle chromatique (degrés).
    // Inséré seulement s'il s'écarte du neutre — un filtre identité coûterait un
    // passage complet sur chaque image, comme pour `eq`.
    let hue = video.hue.clamp(-180.0, 180.0);
    if hue.abs() > 0.5 {
        filters.push(format!("hue=h={:.2}", hue));
    }

    // Niveaux : `colorlevels` recadre l'intervalle d'entrée sur [0,1]. Uniquement
    // si l'utilisateur a resserré les bornes : sans cela le filtre serait neutre
    // mais coûteux.
    let noir = video.niveaux_noir.clamp(0.0, 0.9);
    let blanc = video.niveaux_blanc.clamp(0.1, 1.0);
    if noir > 0.001 || (blanc - 1.0).abs() > 0.001 {
        // `rimax`/`romax` à 1 : on ne compresse pas la sortie, on ne fait que
        // recadrer l'entrée (un recadrage de sortie écraserait les hautes lumières).
        filters.push(format!(
            "colorlevels=rimin={:.3}:gimin={:.3}:bimin={:.3}:rimax={:.3}:gimax={:.3}:bimax={:.3}",
            noir, noir, noir, blanc, blanc, blanc
        ));
    }

    // Adoucissement : `gblur` gaussien. Rayons croissants, bornés — au-delà, l'image
    // devient floue au point qu'on ne distingue plus les visages.
    let adoucir: [&str; 4] = ["", "gblur=sigma=0.6", "gblur=sigma=1.2", "gblur=sigma=2.0"];
    if video.adoucir > 0 {
        filters.push(adoucir[(video.adoucir.min(3)) as usize].to_string());
    }

    // Netteté. `unsharp` travaille par différence de flou gaussien ; les valeurs
    // sont croissantes et bornées (au-delà, halos autour des contours).
    let nettete: [&str; 4] = [
        "",
        "unsharp=5:5:0.8:5:5:0.0",
        "unsharp=5:5:1.2:5:5:0.0",
        "unsharp=7:7:1.6:7:7:0.0",
    ];
    if video.sharpen > 0 {
        filters.push(nettete[(video.sharpen.min(3)) as usize].to_string());
    }

    // Recadrage des bandes noires (letterbox/pillarbox). Placé avant le zoom pour que
    // celui-ci parte de l'image utile. Les valeurs sont bornées : un recadrage plus
    // grand que l'image elle-même produirait un graphe invalide (FFmpeg refuse), donc
    // on laisse toujours au moins 16 pixels dans chaque dimension.
    if video.crop_active() {
        let g = video.crop_left.clamp(0, 4096);
        let d = video.crop_right.clamp(0, 4096);
        let h = video.crop_top.clamp(0, 4096);
        let b = video.crop_bottom.clamp(0, 4096);
        filters.push(format!(
            "crop=w=max(16\\,iw-{}-{}):h=max(16\\,ih-{}-{}):x={}:y={}",
            g, d, h, b, g, h
        ));
    }

    // Géométrie : le zoom passe AVANT la rotation (on recadre dans l'image
    // d'origine, puis on la tourne). `crop` prend des dimensions paires (exigence
    // de nombreux formats de pixels) et `scale` ramène à la place libérée.
    //
    // Les dimensions exactes sont inconnues ici (elles n'arrivent qu'à la
    // construction du graphe) : on exprime donc le recadrage en fractions, avec
    // `iw`/`ih` — FFmpeg les évalue par image.
    let zoom = video.zoom.clamp(1.0, 4.0);
    if (zoom - 1.0).abs() > 1e-3 {
        // Largeur = iw / zoom (arrondie à une valeur paire via trunc+2*floor).
        filters.push(format!(
            "crop=w=trunc(iw/{z}/2)*2:h=trunc(ih/{z}/2)*2:x=trunc((iw-iw/{z})/4)*2:y=trunc((ih-ih/{z})/4)*2",
            z = zoom
        ));
        // Le recadrage réduit la taille : on rétablit la taille d'origine pour que
        // la conversion RGBA et le rapport d'aspect restent ceux du média.
        filters.push("scale=iw*".to_string() + &format!("{}", zoom) + ":ih*" + &format!("{}", zoom));
    }

    match video.rotate {
        90 => filters.push("transpose=1".to_string()),   // 90° horaire
        180 => filters.push("transpose=1,transpose=1".to_string()),
        270 => filters.push("transpose=2".to_string()),  // 90° anti-horaire
        _ => {}
    }
    if video.flip {
        filters.push("vflip".to_string());
    }
    // Miroir horizontal : `hflip`. Placé après la rotation, comme le miroir
    // vertical, pour que le repère de l'utilisateur reste l'image affichée.
    if video.flip_h {
        filters.push("hflip".to_string());
    }

    filters.join(",")
}

/// Équivalent **GPU** de la chaîne de filtres, quand il existe.
///
/// Seul le désentrelacement a un équivalent VA-API (`deinterlace_vaapi`). Mesuré sur
/// une source 1080i affichée en 720p : 7,00 ms/image avec `bwdif` sur le CPU contre
/// **3,30 ms/image** avec `deinterlace_vaapi` (−53 %), parce qu'il n'y a alors plus
/// de rapatriement avant le traitement.
///
/// La fonction refuse dès que la chaîne contient autre chose que le désentrelaceur :
/// `eq`, `deband`, `hqdn3d`, `crop`, `transpose`… n'ont pas d'équivalent dans ce
/// pilote (`procamp_vaapi` échoue ici : « filter caps: 20 – not implemented »), et
/// mélanger les deux chemins coûterait deux passages au lieu d'un gain.
fn filtre_gpu_equivalent(video: &crate::config::VideoSettings, chaine_cpu: &str) -> Option<&'static str> {
    if !video.deinterlace {
        return None;
    }
    let seulement_desentrelaceur = !chaine_cpu.contains(',')
        && (chaine_cpu.starts_with("bwdif=") || chaine_cpu.starts_with("yadif="));
    if seulement_desentrelaceur {
        Some("deinterlace_vaapi")
    } else {
        None
    }
}

/// Un filtre de la chaîne audio, avec la contrainte qu'il impose.
struct FiltreAudio {
    texte: String,
    /// Vrai si le filtre n'accepte QUE deux canaux (`pan`, `crossfeed`…).
    /// Sur un périphérique mono, il doit être écarté — sinon FFmpeg refuse le graphe.
    stereo_seulement: bool,
}

/// Décompose un facteur de vitesse en plusieurs `atempo` chaînés.
///
/// `atempo` n'accepte que 0,5 à 100 : en dessous de 0,5 il faut multiplier les
/// étages (0,1× = 0,5 × 0,5 × 0,4). Le produit est exact, donc la durée obtenue
/// reste celle attendue.
fn atempo_etages(vitesse: f32) -> Vec<f32> {
    let mut restant = vitesse.clamp(0.01, 100.0);
    let mut etages = Vec::new();
    while restant < 0.5 - 1e-6 {
        etages.push(0.5);
        restant /= 0.5;
    }
    if (restant - 1.0).abs() > 1e-3 {
        etages.push(restant);
    }
    etages
}

/// Chaîne de traitement audio demandée par les réglages, dans l'ordre où elle
/// s'applique.
///
/// **L'ordre est raisonné, pas arbitraire :**
///  1. `atempo` d'abord : la correction de durée s'applique avant tout traitement,
///     pour que les filtres à constante de temps (compresseur, réverbération)
///     travaillent sur la durée réelle d'écoute ;
///  2. réduction de bruit : nettoyer avant d'amplifier (sinon on amplifie le bruit) ;
///  3. égaliseur : corriger la réponse avant la dynamique ;
///  4. réverbération : effet créatif, après la courbe ;
///  5. stéréo (élargissement, crossfeed, karaoké) ;
///  6. compresseur de nuit, puis normaliseur : la compression précède l'égalisation
///     de niveau, sinon le normaliseur annulerait l'effet de la compression ;
///  7. limiteur de sécurité : **toujours en dernier** quand un traitement existe,
///     pour qu'aucune combinaison de gains ne puisse écrêter ;
///  8. décalage audio (synchronisation lèvres/son) en tout dernier.
fn filtres_audio(
    audio: &crate::config::AudioSettings,
    vitesse: f32,
    pitch_preserved: bool,
) -> Vec<FiltreAudio> {
    let mut filtres: Vec<FiltreAudio> = Vec::new();
    let simple = |t: String| FiltreAudio {
        texte: t,
        stereo_seulement: false,
    };
    let stereo = |t: String| FiltreAudio {
        texte: t,
        stereo_seulement: true,
    };

    // 1. Vitesse sans changement de hauteur.
    if pitch_preserved && (vitesse - 1.0).abs() > 1e-3 {
        for etage in atempo_etages(vitesse) {
            filtres.push(simple(format!("atempo={:.4}", etage)));
        }
    }

    // 2. Réduction de bruit. Les niveaux ne sont pas devinés : mesurés sur un son
    //    pur 440 Hz noyé dans du bruit blanc (rapport signal/bruit avant : 19,4 dB) —
    //      1 : nr=20/nf=-50  → 21,9 dB  (+2,5 dB de marge)
    //      2 : nr=40/nf=-45  → 28,0 dB  (+8,6 dB)
    //      3 : nr=60/nf=-40  → 42,7 dB  (+23,3 dB) sans atténuer le signal
    //    Voir tools/verif-audio.sh. Sur un bruit PUR (sans signal), ces filtres ne
    //    peuvent rien retirer : il n'y a pas de « signal » à préserver.
    if audio.noise_reduction > 0 {
        let txt = match audio.noise_reduction.min(3) {
            1 => "afftdn=nr=20:nf=-50",
            2 => "afftdn=nr=40:nf=-45:track_noise=1",
            _ => "afftdn=nr=60:nf=-40",
        };
        filtres.push(simple(txt.to_string()));
    }

    // 3. Égaliseur 18 bandes.
    if audio.eq_active() {
        let mut parties: Vec<String> = Vec::with_capacity(crate::config::EQ_BANDS);
        for bande in 0..crate::config::EQ_BANDS {
            parties.push(format!("{}b={:.3}", bande + 1, audio.gain(bande)));
        }
        filtres.push(simple(format!("superequalizer={}", parties.join(":"))));
    }

    // 4. Réverbération : trois tailles de pièce. Les retours sont volontairement
    //    discrets (décroissance 0,2 à 0,4) : au-delà, la parole devient inintelligible.
    if audio.reverb > 0 {
        // `aecho=in_gain:out_gain:delays:decays` : `in_gain` = part du son direct,
        // `out_gain` = part des réflexions. Les délais et décroissances donnent le
        // **caractère** de la salle (niveaux 1 à 3) ; l'équilibre direct/réverbéré
        // est réglable séparément (`reverb_mix`), ce qui évite de changer de preset
        // pour simplement doser l'ambiance.
        let (delais, decroissances) = match audio.reverb.min(3) {
            1 => ("25|40", "0.25|0.15"),
            2 => ("40|60", "0.3|0.2"),
            _ => ("60|90|120", "0.35|0.25|0.15"),
        };
        let mix = if audio.reverb_mix.is_finite() {
            audio.reverb_mix.clamp(0.0, 1.0)
        } else {
            crate::config::AudioSettings::default().reverb_mix
        };
        // 0 → 0,05 (presque sec, mais le filtre reste actif), 1 → 0,95 (très humide).
        let humide = 0.05 + 0.90 * mix;
        // Le direct baisse légèrement quand le mouillé monte : sinon le total
        // sature (mesuré à l'oreille comme un « pompage » sur les voix).
        let direct = 1.0 - 0.25 * mix;
        filtres.push(simple(format!(
            "aecho={:.3}:{:.3}:{}:{}",
            direct, humide, delais, decroissances
        )));
    }

    // 5. Outils stéréo. `extrastereo` augmente l'écart entre canaux (m > 1),
    //    `crossfeed` le réduit (écoute au casque), `pan` retire le canal central
    //    (karaoké). Les trois exigent deux canaux.
    if audio.stereo_widen > 0 {
        let m = [0.0, 1.3, 1.7, 2.2][(audio.stereo_widen.min(3)) as usize];
        filtres.push(stereo(format!("extrastereo=m={:.2}", m)));
    }
    if audio.crossfeed {
        filtres.push(stereo(
            "crossfeed=strength=0.35:range=0.6:slope=0.5".to_string(),
        ));
    }
    if audio.karaoke {
        // Différence des canaux : ce qui est identique à gauche et à droite (la voix,
        // presque toujours au centre) disparaît. Coefficient 0,5 pour éviter l'écrêtage.
        filtres.push(stereo(
            "pan=stereo|c0=0.5*c0-0.5*c1|c1=0.5*c1-0.5*c0".to_string(),
        ));
    }

    // 6. Dynamique : mode nuit puis normaliseur.
    if audio.night_mode {
        // Seuil bas (0,06 ≈ −24 dBFS), rapport 4:1 : les murmures remontent, les
        // explosions sont contenues. Le gain de rattrapage compense la perte.
        filtres.push(simple(
            "acompressor=threshold=0.06:ratio=4:attack=20:release=400:makeup=3".to_string(),
        ));
    }
    if audio.normalizer {
        // `dynaudnorm` adapte son gain progressivement : avec les valeurs par défaut
        // de FFmpeg (fenêtre 500 ms, lissage 31), il lui faut ~6 s pour atteindre son
        // gain — sur un fichier court, on croit qu'il ne fait rien. Mesuré : fenêtre
        // 200 ms + lissage 11 convergent en ~2,5 s, pour un résultat identique
        // (voir tools/verif-audio.sh). Crête visée 0,95, gain maximal 10× (20 dB) :
        // au-delà, le souffle d'une scène silencieuse deviendrait audible.
        filtres.push(simple(
            "dynaudnorm=framelen=200:gausssize=11:peak=0.95:maxgain=10".to_string(),
        ));
    }

    // 7. Limiteur de sécurité : dès qu'un traitement peut dépasser 0 dBFS.
    //
    // `level=false` est indispensable : l'option « auto level » (activée par défaut)
    // NORMALISE la sortie au niveau de limite, donc atténue tout le signal — un
    // égaliseur neutre sortait 0,45 dB plus bas, ce qui faussait la mesure (et
    // s'entendait). Avec `level=false`, le limiteur est transparent tant qu'aucune
    // crête ne dépasse `limit` : vérifié, 0,5 reste 0,5, et une crête à 1,4 est
    // ramenée à 0,98. Voir tools/verif-audio.sh.
    if !filtres.is_empty() {
        filtres.push(simple(
            "alimiter=level_in=1:level_out=1:limit=0.98:level=false:attack=5:release=50"
                .to_string(),
        ));
    }

    // 8. Décalage lèvres/son.
    if audio.delay_ms != 0 {
        let ms = audio.delay_ms.clamp(-2000, 2000);
        if ms > 0 {
            // Retarder l'audio : `adelay` sur tous les canaux.
            filtres.push(simple(format!("adelay=delays={}:all=1", ms)));
        } else {
            // Avancer l'audio n'existe pas : on coupe le début. `atrim` puis
            // `asetpts` pour que la sortie reparte de zéro.
            filtres.push(simple(format!(
                "atrim=start={:.3},asetpts=PTS-STARTPTS",
                (-ms) as f32 / 1000.0
            )));
        }
    }

    filtres
}

/// Description textuelle de la chaîne audio pour un nombre de canaux donné.
///
/// Les filtres stéréo sont retirés sur un périphérique mono : FFmpeg refuserait le
/// graphe entier, et l'utilisateur perdrait **tous** ses réglages à cause d'un seul.
fn audio_filter_description(
    audio: &crate::config::AudioSettings,
    vitesse: f32,
    pitch_preserved: bool,
    canaux: i32,
) -> String {
    filtres_audio(audio, vitesse, pitch_preserved)
        .into_iter()
        .filter(|f| canaux >= 2 || !f.stereo_seulement)
        .map(|f| f.texte)
        .collect::<Vec<_>>()
        .join(",")
}

/// Fait passer un bloc d'échantillons f32 entrelacés par l'égaliseur.
///
/// Le graphe est reconstruit si le taux, le nombre de canaux ou la description
/// changent (périphérique différent, égaliseur modifié). En cas de refus, la
/// signature est mémorisée : la lecture continue **sans égaliseur**, avec un
/// message, au lieu de réessayer vingt-cinq fois par seconde.
unsafe fn apply_audio_filters(
    chain_slot: &mut Option<Box<AudioFilterChain>>,
    failed_slot: &mut Option<String>,
    description: &str,
    sample_rate: i32,
    channels: i32,
    samples: &mut Vec<f32>,
) {
    if description.is_empty() || sample_rate <= 0 || channels <= 0 {
        return;
    }
    let signature = format!("{}|{}|{}", description, sample_rate, channels);

    let rebuild = match chain_slot.as_ref() {
        Some(c) => c.signature != signature,
        None => true,
    };
    if rebuild {
        *chain_slot = None;
        if failed_slot.as_deref() == Some(signature.as_str()) {
            return;
        }
        match build_audio_chain(description, sample_rate, channels, &signature) {
            Ok(chain) => {
                tracing::info!(
                    "Chaîne audio active : {} Hz, {} canaux ({} éch./s attendus)",
                    chain.sample_rate,
                    chain.channels,
                    chain.sample_rate * chain.channels
                );
                *chain_slot = Some(Box::new(chain));
                *failed_slot = None;
            }
            Err(e) => {
                tracing::warn!("Égaliseur inapplicable — lecture sans égalisation : {}", e);
                *failed_slot = Some(signature);
                return;
            }
        }
    }

    let Some(chain) = chain_slot.as_mut() else {
        return;
    };
    if samples.is_empty() {
        return;
    }
    let nb = samples.len() / channels as usize;
    if nb == 0 {
        return;
    }

    // Frame d'entrée : on remplit un tampon alloué par FFmpeg (l'API exige une
    // frame référencée ; passer nos propres pointeurs serait accepté sur certaines
    // versions et pas d'autres).
    let frame = av_frame_alloc();
    if frame.is_null() {
        return;
    }
    (*frame).nb_samples = nb as c_int;
    (*frame).format = AVSampleFormat::AV_SAMPLE_FMT_FLT as c_int;
    (*frame).sample_rate = sample_rate;
    av_channel_layout_default(&mut (*frame).ch_layout, channels);
    if av_frame_get_buffer(frame, 0) < 0 {
        let mut f = frame;
        av_frame_free(&mut f);
        return;
    }
    std::ptr::copy_nonoverlapping(
        samples.as_ptr() as *const u8,
        (*frame).data[0],
        nb * channels as usize * std::mem::size_of::<f32>(),
    );

    let ret = av_buffersrc_add_frame_flags(chain.src, frame, AV_BUFFERSRC_FLAG_KEEP_REF as c_int);
    let mut f = frame;
    av_frame_free(&mut f);
    if ret < 0 {
        tracing::warn!("Envoi d'un bloc audio au graphe refusé : {}", av_error_text(ret));
        return;
    }
    chain.samples_in += nb as u64;

    // L'égaliseur est un filtre à réponse impulsionnelle finie : il introduit un
    // retard. Un paquet donné ne ressort donc PAS forcément dans l'appel courant.
    // On collecte tout ce que le puits veut bien donner MAINTENANT, et on vide le
    // tampon d'entrée : ces échantillons sont désormais dans la ligne de retard du
    // filtre et ressortiront plus tard. Les conserver produisait un signal deux
    // fois trop long (mesuré : 248 640 échantillons pour 132 637 en entrée).
    let mut sortie: Vec<f32> = Vec::with_capacity(samples.len());
    loop {
        let out = av_frame_alloc();
        if out.is_null() {
            break;
        }
        let ret = av_buffersink_get_frame(chain.sink, out);
        if ret < 0 {
            let mut tmp = out;
            av_frame_free(&mut tmp);
            if ret != -(libc::EAGAIN) {
                tracing::warn!("Lecture du graphe audio : {}", av_error_text(ret));
            }
            break;
        }
        let produits = (*out).nb_samples as usize;
        if produits > 0 && !(*out).data[0].is_null() {
            // Ce contrôle aurait évité un WAV rempli de NaN : si le format n'est
            // pas celui attendu, on s'arrête au lieu de lire hors du tampon.
            if (*out).format != AVSampleFormat::AV_SAMPLE_FMT_FLT as c_int {
                tracing::warn!(
                    "Égaliseur : format de sortie inattendu ({}) — égalisation ignorée",
                    av_get_sample_fmt_name(std::mem::transmute::<c_int, AVSampleFormat>((*out).format))
                        .as_ref()
                        .map(|p| std::ffi::CStr::from_ptr(p).to_string_lossy().to_string())
                        .unwrap_or_else(|| "?".to_string())
                );
                let mut tmp = out;
                av_frame_free(&mut tmp);
                break;
            }
            let n = produits * channels as usize;
            let source = std::slice::from_raw_parts((*out).data[0] as *const f32, n);
            sortie.extend_from_slice(source);
            chain.samples_out += produits as u64;
        }
        let mut tmp = out;
        av_frame_free(&mut tmp);
    }
    samples.clear();
    samples.extend_from_slice(&sortie);
}

/// Vide la ligne de retard du graphe audio (fin de flux).
///
/// Sans cet appel, les derniers échantillons restent bloqués dans le filtre : le
/// contrôle d'intégrité (entrées = sorties) ne tomberait jamais juste et la fin du
/// fichier serait tronquée.
unsafe fn flush_audio_filters(
    chain_slot: &mut Option<Box<AudioFilterChain>>,
    channels: i32,
    sortie: &mut Vec<f32>,
) {
    let Some(chain) = chain_slot.as_mut() else {
        return;
    };
    if channels <= 0 {
        return;
    }
    // Un paquet vide signale la fin de flux au graphe.
    if av_buffersrc_add_frame_flags(chain.src, std::ptr::null_mut(), 0) < 0 {
        return;
    }
    loop {
        let out = av_frame_alloc();
        if out.is_null() {
            break;
        }
        let ret = av_buffersink_get_frame(chain.sink, out);
        if ret < 0 {
            let mut tmp = out;
            av_frame_free(&mut tmp);
            break;
        }
        let produits = (*out).nb_samples as usize;
        if produits > 0 && !(*out).data[0].is_null() {
            // Ce contrôle aurait évité un WAV rempli de NaN : si le format n'est
            // pas celui attendu, on s'arrête au lieu de lire hors du tampon.
            if (*out).format != AVSampleFormat::AV_SAMPLE_FMT_FLT as c_int {
                tracing::warn!(
                    "Égaliseur : format de sortie inattendu ({}) — égalisation ignorée",
                    av_get_sample_fmt_name(std::mem::transmute::<c_int, AVSampleFormat>((*out).format))
                        .as_ref()
                        .map(|p| std::ffi::CStr::from_ptr(p).to_string_lossy().to_string())
                        .unwrap_or_else(|| "?".to_string())
                );
                let mut tmp = out;
                av_frame_free(&mut tmp);
                break;
            }
            let n = produits * channels as usize;
            let source = std::slice::from_raw_parts((*out).data[0] as *const f32, n);
            sortie.extend_from_slice(source);
            chain.samples_out += produits as u64;
        }
        let mut tmp = out;
        av_frame_free(&mut tmp);
    }
}

/// Construit le graphe `abuffer → superequalizer → abuffersink`.
unsafe fn build_audio_chain(
    description: &str,
    sample_rate: i32,
    channels: i32,
    signature: &str,
) -> Result<AudioFilterChain, String> {
    let graph = avfilter_graph_alloc();
    if graph.is_null() {
        return Err("allocation du graphe audio impossible".to_string());
    }
    let mut chain = AudioFilterChain {
        graph,
        src: std::ptr::null_mut(),
        sink: std::ptr::null_mut(),
        signature: signature.to_string(),
        sample_rate,
        channels,
        samples_in: 0,
        samples_out: 0,
    };

    let args = format!(
        "sample_rate={}:sample_fmt=flt:channel_layout={}c",
        sample_rate, channels
    );
    let Ok(args_c) = CString::new(args) else {
        return Err("arguments de source audio invalides".to_string());
    };
    // `superequalizer` sort en format PLANAR (fltp) : chaque canal dans un plan
    // distinct. Lire `nb_samples * canaux` flottants depuis `data[0]` débordait donc
    // du tampon (mesuré : NaN et valeurs à 3,2e38 dans le WAV produit). On force la
    // négociation en flt entrelacé — ce que le reste du moteur attend.
    let description_complete = format!(
        "{},aformat=sample_fmts=flt:channel_layouts={}c",
        description, channels
    );
    let Ok(desc_c) = CString::new(description_complete) else {
        return Err("description de filtres audio invalide".to_string());
    };
    let abuffer = b"abuffer\0".as_ptr() as *const std::os::raw::c_char;
    let abuffersink = b"abuffersink\0".as_ptr() as *const std::os::raw::c_char;
    let in_name = b"in\0".as_ptr() as *const std::os::raw::c_char;
    let out_name = b"out\0".as_ptr() as *const std::os::raw::c_char;

    let ret = avfilter_graph_create_filter(
        &mut chain.src,
        avfilter_get_by_name(abuffer),
        in_name,
        args_c.as_ptr(),
        std::ptr::null_mut(),
        chain.graph,
    );
    if ret < 0 || chain.src.is_null() {
        return Err(format!("source audio du graphe refusée ({})", av_error_text(ret)));
    }
    let ret = avfilter_graph_create_filter(
        &mut chain.sink,
        avfilter_get_by_name(abuffersink),
        out_name,
        std::ptr::null(),
        std::ptr::null_mut(),
        chain.graph,
    );
    if ret < 0 || chain.sink.is_null() {
        return Err(format!("puits audio du graphe refusé ({})", av_error_text(ret)));
    }

    let mut outputs = avfilter_inout_alloc();
    let mut inputs = avfilter_inout_alloc();
    if outputs.is_null() || inputs.is_null() {
        avfilter_inout_free(&mut outputs);
        avfilter_inout_free(&mut inputs);
        return Err("allocation des liaisons audio impossible".to_string());
    }
    // `avfilter_inout_free` libère `name` avec `av_free` : allocation obligatoire.
    (*outputs).name = av_strdup(in_name);
    (*outputs).filter_ctx = chain.src;
    (*outputs).pad_idx = 0;
    (*outputs).next = std::ptr::null_mut();
    (*inputs).name = av_strdup(out_name);
    (*inputs).filter_ctx = chain.sink;
    (*inputs).pad_idx = 0;
    (*inputs).next = std::ptr::null_mut();
    if (*outputs).name.is_null() || (*inputs).name.is_null() {
        avfilter_inout_free(&mut outputs);
        avfilter_inout_free(&mut inputs);
        return Err("allocation des noms de liaison audio impossible".to_string());
    }

    let ret = avfilter_graph_parse_ptr(
        chain.graph,
        desc_c.as_ptr(),
        &mut inputs,
        &mut outputs,
        std::ptr::null_mut(),
    );
    if ret < 0 {
        return Err(format!("chaîne audio refusée ({})", av_error_text(ret)));
    }
    let ret = avfilter_graph_config(chain.graph, std::ptr::null_mut());
    if ret < 0 {
        return Err(format!(
            "configuration du graphe audio refusée ({})",
            av_error_text(ret)
        ));
    }
    Ok(chain)
}

/// Construit un graphe `buffer → filtres → buffersink` pour la description donnée.
///
/// `signature` n'est pas décorative : elle contient la géométrie, le format de
/// pixels et la base de temps du flux. Le format peut changer en cours de lecture
/// (rapatriement VA-API, changement de profil), et un graphe construit pour un
/// autre format échouerait à chaque image.
unsafe fn build_filter_chain(
    description: &str,
    width: i32,
    height: i32,
    format: AVPixelFormat,
    time_base: AVRational,
) -> Result<VideoFilterChain, String> {
    let graph = avfilter_graph_alloc();
    if graph.is_null() {
        return Err("allocation du graphe de filtres impossible".to_string());
    }

    // Toute erreur à partir d'ici doit libérer le graphe : on passe par une
    // fermeture interne pour ne pas oublier un `avfilter_graph_free`.
    let mut chain = VideoFilterChain {
        graph,
        src: std::ptr::null_mut(),
        sink: std::ptr::null_mut(),
        signature: String::new(),
        frames_in: 0,
        frames_out: 0,
    };

    let fmt_name = av_get_pix_fmt_name(format);
    let fmt_name = if fmt_name.is_null() {
        "yuv420p".to_string()
    } else {
        std::ffi::CStr::from_ptr(fmt_name).to_string_lossy().to_string()
    };
    let args = format!(
        "video_size={}x{}:pix_fmt={}:time_base={}/{}:pixel_aspect=1/1",
        width,
        height,
        fmt_name,
        if time_base.num > 0 { time_base.num } else { 1 },
        if time_base.den > 0 { time_base.den } else { 25 }
    );
    let Ok(args_c) = CString::new(args) else {
        return Err("arguments de source invalides".to_string());
    };
    let Ok(desc_c) = CString::new(description) else {
        return Err("description de filtres invalide".to_string());
    };
    let buffer_name = b"buffer\0".as_ptr() as *const std::os::raw::c_char;
    let in_name = b"in\0".as_ptr() as *const std::os::raw::c_char;

    let ret = avfilter_graph_create_filter(
        &mut chain.src,
        avfilter_get_by_name(buffer_name),
        in_name,
        args_c.as_ptr(),
        std::ptr::null_mut(),
        chain.graph,
    );
    if ret < 0 || chain.src.is_null() {
        return Err(format!("source du graphe refusée ({})", av_error_text(ret)));
    }

    let sink_name = b"buffersink\0".as_ptr() as *const std::os::raw::c_char;
    let out_name = b"out\0".as_ptr() as *const std::os::raw::c_char;
    let ret = avfilter_graph_create_filter(
        &mut chain.sink,
        avfilter_get_by_name(sink_name),
        out_name,
        std::ptr::null(),
        std::ptr::null_mut(),
        chain.graph,
    );
    if ret < 0 || chain.sink.is_null() {
        return Err(format!("puits du graphe refusé ({})", av_error_text(ret)));
    }

    // Listes d'entrées/sorties exigées par l'analyseur de description.
    let mut outputs = avfilter_inout_alloc();
    let mut inputs = avfilter_inout_alloc();
    if outputs.is_null() || inputs.is_null() {
        avfilter_inout_free(&mut outputs);
        avfilter_inout_free(&mut inputs);
        return Err("allocation des liaisons impossible".to_string());
    }
    // ATTENTION : `avfilter_inout_free` libère `name` avec `av_free`. Y mettre
    // l'adresse d'une chaîne statique (« in ») provoque `free(): invalid pointer`
    // au moment où FFmpeg nettoie les listes — plantage observé au premier essai.
    // Le nom doit donc être un bloc alloué par FFmpeg (`av_strdup`).
    (*outputs).name = av_strdup(in_name);
    (*outputs).filter_ctx = chain.src;
    (*outputs).pad_idx = 0;
    (*outputs).next = std::ptr::null_mut();
    (*inputs).name = av_strdup(out_name);
    (*inputs).filter_ctx = chain.sink;
    (*inputs).pad_idx = 0;
    (*inputs).next = std::ptr::null_mut();
    if (*outputs).name.is_null() || (*inputs).name.is_null() {
        avfilter_inout_free(&mut outputs);
        avfilter_inout_free(&mut inputs);
        return Err("allocation des noms de liaison impossible".to_string());
    }

    let ret = avfilter_graph_parse_ptr(
        chain.graph,
        desc_c.as_ptr(),
        &mut inputs,
        &mut outputs,
        std::ptr::null_mut(),
    );
    if ret < 0 {
        return Err(format!(
            "chaîne « {} » refusée ({})",
            description,
            av_error_text(ret)
        ));
    }

    let ret = avfilter_graph_config(chain.graph, std::ptr::null_mut());
    if ret < 0 {
        return Err(format!("configuration du graphe refusée ({})", av_error_text(ret)));
    }

    chain.signature = format!("{}|{}x{}|{}", description, width, height, fmt_name);
    Ok(chain)
}

/// Nom lisible d'un format de pixels (`AV_PIX_FMT_…` → « yuv420p »).
fn pixfmt_name(format: AVPixelFormat) -> String {
    unsafe {
        let p = av_get_pix_fmt_name(format);
        if p.is_null() {
            format!("fmt{}", format as c_int)
        } else {
            std::ffi::CStr::from_ptr(p).to_string_lossy().to_string()
        }
    }
}

/// Sépare un éventuel **étage HDR** de tête du reste de la chaîne.
///
/// Renvoie `(chaîne sans l'étage, reste)` si la description commence par un étage
/// produit par `crate::hdr::plan` (`libplacebo=…` ou `zscale=…`), `None` sinon.
///
/// L'étage HDR est toujours **en tête** : il doit travailler sur les bits d'origine,
/// avant le recadrage et la mise à l'échelle (sinon on convertirait une image déjà
/// réduite, en perdant la précision des hautes lumières).
pub(crate) fn decouper_etage_hdr(description: &str) -> Option<(String, String)> {
    let (tete, reste) = match description.split_once(',') {
        Some((t, r)) => (t, r.to_string()),
        None => (description, String::new()),
    };
    let commence_par_etage = tete.starts_with("libplacebo=") || tete.starts_with("zscale=");
    if !commence_par_etage {
        return None;
    }
    Some((reste.clone(), reste))
}

/// Phrase affichée une fois quand l'étage HDR est retiré : l'image n'est PAS convertie,
/// donc délavée sur un écran SDR. Si le repli CPU a été refusé, on en donne la raison.
fn avis_hdr_non_converti(motif_repli: &str) -> String {
    if motif_repli.is_empty() {
        "HDR : Vulkan indisponible, image non convertie (couleurs délavées)".to_string()
    } else {
        format!(
            "HDR : Vulkan indisponible et source trop lourde pour le processeur — image non \
             convertie ({})",
            motif_repli
        )
    }
}

/// Fait passer une frame décodée par la chaîne de filtres et renvoie la frame
/// transformée (`None` = pas de sortie : la frame d'origine est alors utilisée).
///
/// Le graphe est reconstruit si la description ou le format d'entrée change. En cas
/// de refus, on **ne réessaie pas** à chaque image (25 tentatives par seconde
/// noieraient le journal) : la signature refusée est mémorisée et la lecture
/// continue sans filtres, avec un message explicite.
unsafe fn apply_video_filters(
    vs: &mut VideoStream,
    description: &str,
    frame: *mut AVFrame,
) -> Option<*mut AVFrame> {
    let width = (*frame).width;
    let height = (*frame).height;
    let format: AVPixelFormat = std::mem::transmute::<c_int, AVPixelFormat>((*frame).format);
    let signature = format!("{}|{}x{}|{}", description, width, height, pixfmt_name(format));

    let rebuild = match vs.filters.as_ref() {
        Some(chain) => chain.signature != signature,
        None => true,
    };
    if rebuild {
        vs.filters = None; // libère l'ancien graphe (Drop)
        if vs.filters_failed_signature.as_deref() == Some(signature.as_str()) {
            return None;
        }
        match build_filter_chain(description, width, height, format, vs.time_base) {
            Ok(chain) => {
                tracing::info!(
                    "Filtres vidéo actifs: {} ({}x{}, {})",
                    description,
                    width,
                    height,
                    pixfmt_name(format)
                );
                vs.filters = Some(Box::new(chain));
                vs.filters_failed_signature = None;
            }
            Err(e) => {
                // **Repli ciblé de l'étage HDR.** Un étage `libplacebo` échoue si
                // Vulkan est absent (pilote manquant, session distante, mémoire GPU
                // insuffisante). Perdre **toute** la chaîne pour cela serait
                // disproportionné : on réessaie une fois sans l'étage, et on le dit.
                if let Some((sans_hdr, reste)) = decouper_etage_hdr(description) {
                    // **Repli CPU borné.** Si c'est l'étage GPU (`libplacebo`) qui est
                    // refusé, l'étage CPU du même plan (`zscale` + `tonemap`) peut le
                    // remplacer — mais seulement si la source tient dans le budget mesuré
                    // (`hdr::repli_cpu_possible` : 1080p30). Au-delà, une conversion
                    // saccadée serait pire qu'une image non convertie : on s'abstient et
                    // on le dit.
                    if description.starts_with("libplacebo=") && !vs.hdr_etage_cpu.is_empty() {
                        match crate::hdr::repli_cpu_possible(width, height, vs.fps_flux) {
                            Ok(()) => {
                                let cpu = if reste.is_empty() {
                                    vs.hdr_etage_cpu.clone()
                                } else {
                                    format!("{},{}", vs.hdr_etage_cpu, reste)
                                };
                                tracing::warn!("Étage HDR refusé ({}) — repli CPU : {}", e, cpu);
                                match build_filter_chain(&cpu, width, height, format, vs.time_base) {
                                    Ok(mut chain) => {
                                        // La signature DEMANDÉE, pas celle de la chaîne de
                                        // repli : sinon chaque image la jugerait périmée
                                        // et retenterait Vulkan, échouerait, reconstruirait.
                                        chain.signature = signature.clone();
                                        tracing::info!(
                                            "Filtres vidéo actifs (repli CPU): {} ({}x{}, {})",
                                            cpu,
                                            width,
                                            height,
                                            pixfmt_name(format)
                                        );
                                        vs.filters = Some(Box::new(chain));
                                        vs.filters_failed_signature = None;
                                        vs.hdr_repli_cpu = true;
                                        vs.hdr_avis = Some(
                                            "HDR : Vulkan indisponible, conversion par le \
                                             processeur (qualité réduite)"
                                                .to_string(),
                                        );
                                        return appliquer_au_graphe(vs, frame);
                                    }
                                    Err(e_cpu) => {
                                        tracing::warn!(
                                            "Étage HDR : repli CPU impossible ({}) — étage retiré",
                                            e_cpu
                                        );
                                    }
                                }
                            }
                            Err(raison) => {
                                tracing::warn!("Étage HDR : repli CPU refusé ({})", raison);
                                vs.hdr_repli_motif = raison;
                            }
                        }
                    }
                    // Sans étage, il ne reste parfois **aucun** filtre : il n'y a alors
                    // rien à construire (un graphe vide est refusé), la lecture
                    // continue simplement sans filtres.
                    if reste.is_empty() {
                        tracing::warn!(
                            "Étage HDR refusé ({}) — lecture sans filtres : aucun filtre ne reste",
                            e
                        );
                        vs.filters_failed_signature = Some(signature);
                        vs.hdr_indisponible = true;
                        vs.hdr_avis = Some(avis_hdr_non_converti(&vs.hdr_repli_motif));
                        return None;
                    }
                    tracing::warn!(
                        "Étage HDR refusé ({}) — nouvel essai sans lui : {}",
                        e,
                        if reste.is_empty() { "aucun filtre" } else { reste.as_str() }
                    );
                    match build_filter_chain(&sans_hdr, width, height, format, vs.time_base) {
                        Ok(mut chain) => {
                            // La signature DEMANDÉE (voir le repli CPU plus haut) : sans cela,
                            // l'échec de Vulkan était rejoué à chaque image dès qu'un autre
                            // filtre était actif (mesuré : 20 refus en 5 s à 5 i/s).
                            chain.signature = signature.clone();
                            tracing::info!(
                                "Filtres vidéo actifs (étage HDR retiré): {} ({}x{}, {})",
                                sans_hdr,
                                width,
                                height,
                                pixfmt_name(format)
                            );
                            vs.filters = Some(Box::new(chain));
                            vs.filters_failed_signature = None;
                            vs.hdr_indisponible = true;
                            vs.hdr_avis = Some(avis_hdr_non_converti(&vs.hdr_repli_motif));
                            return appliquer_au_graphe(vs, frame);
                        }
                        Err(e2) => {
                            tracing::warn!(
                                "Chaîne de filtres inapplicable même sans étage HDR — lecture sans filtres : {}",
                                e2
                            );
                        }
                    }
                } else {
                    tracing::warn!(
                        "Chaîne de filtres inapplicable — lecture sans filtres : {}",
                        e
                    );
                }
                vs.filters_failed_signature = Some(signature);
                return None;
            }
        }
    }

    appliquer_au_graphe(vs, frame)
}

/// Envoie une frame au graphe courant et récupère la sortie.
///
/// Séparé pour être réutilisé par le repli d'étage HDR (voir plus haut) : la
/// reconstruction d'un graphe ne doit pas dupliquer ce code.
/// Recopie les métadonnées de couleur **du flux** sur une frame décodée.
///
/// **Pourquoi c'est nécessaire** : `vf_libplacebo` lit le transfert, les primaires et
/// la matrice d'entrée dans les métadonnées de la frame. Un décodeur les propage
/// souvent, mais pas toujours (flux remuxé, en-tête incomplet, transfert matériel) —
/// et sans elles, libplacebo traite une image PQ comme du SDR : la chaîne est
/// acceptée et **rien n'est converti**, sans le moindre message. Les recopier depuis
/// ce que le conteneur annonce rend le comportement reproductible.
///
/// Seuls les champs de description sont touchés : aucune donnée d'image n'est
/// modifiée, donc l'image envoyée au graphe reste celle du décodeur.
unsafe fn poser_metadonnees_couleur(frame: *mut AVFrame, info: &crate::hdr::Info) {
    // Les constantes sont des **variantes d'énumération** dans ce crate (bindgen les
    // génère ainsi), pas des constantes libres : on les nomme par leur variante.
    use ffmpeg_sys_next::{
        AVColorPrimaries, AVColorSpace, AVColorTransferCharacteristic,
    };
    let trc = match info.transfert {
        crate::hdr::Transfert::Pq => AVColorTransferCharacteristic::AVCOL_TRC_SMPTE2084,
        crate::hdr::Transfert::Hlg => AVColorTransferCharacteristic::AVCOL_TRC_ARIB_STD_B67,
        _ => AVColorTransferCharacteristic::AVCOL_TRC_BT709,
    };
    let (primaires, matrice) = if info.primaires == crate::hdr::Primaires::Bt2020 {
        (
            AVColorPrimaries::AVCOL_PRI_BT2020,
            AVColorSpace::AVCOL_SPC_BT2020_NCL,
        )
    } else {
        (AVColorPrimaries::AVCOL_PRI_BT709, AVColorSpace::AVCOL_SPC_BT709)
    };
    (*frame).color_trc = trc;
    (*frame).color_primaries = primaires;
    (*frame).colorspace = matrice;
    // **La plage est indispensable** : un rapatriement matériel laisse souvent
    // `color_range` non précisée (0), et libplacebo ne convertit alors rien du tout —
    // chaîne acceptée, image inchangée, aucun message (mesuré : 61/98/190 au lieu de
    // 47/107/250 sur la même mire en HEVC 10 bits décodé par VA-API). La vidéo HDR10
    // est en plage limitée (16-235), sauf indication contraire du flux.
    if (*frame).color_range == ffmpeg_sys_next::AVColorRange::AVCOL_RANGE_UNSPECIFIED {
        (*frame).color_range = ffmpeg_sys_next::AVColorRange::AVCOL_RANGE_MPEG;
    }
}

/// Applique la chaîne de filtres vidéo (voir `apply_video_filters`)
unsafe fn appliquer_au_graphe(vs: &mut VideoStream, frame: *mut AVFrame) -> Option<*mut AVFrame> {
    // Les métadonnées de couleur doivent être posées **avant** l'envoi : c'est sur
    // elles que libplacebo décide qu'il y a quelque chose à convertir.
    if let Some(info) = vs.hdr_info.as_ref() {
        poser_metadonnees_couleur(frame, info);
    }
    // Filet de sécurité : si l'horodatage manque encore (chemin exotique), on en
    // fabrique un à partir du rang de la frame. Un `pts` invalide ne fait pas échouer
    // le graphe : il le rend **muet** (voir le commentaire du rapatriement matériel).
    if (*frame).pts == AV_NOPTS_VALUE {
        let pas = if vs.time_base.num > 0 && vs.time_base.den > 0 {
            av_rescale_q(1, vs.time_base, AVRational { num: 1, den: 1 }).max(1)
        } else {
            1
        };
        (*frame).pts = vs.filters.as_ref().map(|c| c.frames_in as i64).unwrap_or(0) * pas;
        if (*frame).duration <= 0 {
            (*frame).duration = pas;
        }
    }
    let chain = vs.filters.as_mut()?;
    let ret = av_buffersrc_add_frame_flags(
        chain.src,
        frame,
        AV_BUFFERSRC_FLAG_KEEP_REF as c_int,
    );
    if ret < 0 {
        tracing::warn!("Envoi d'une frame au graphe de filtres refusé : {}", av_error_text(ret));
        return None;
    }
    chain.frames_in += 1;

    let out = av_frame_alloc();
    if out.is_null() {
        return None;
    }
    let ret = av_buffersink_get_frame(chain.sink, out);
    if ret < 0 {
        // **Un graphe muet ne doit pas rester silencieux.** Si le puits répond
        // `EAGAIN` indéfiniment alors que des dizaines de frames sont entrées, c'est
        // le signe d'une entrée inexploitable (horodatage manquant, format refusé) :
        // l'image affichée serait celle du décodeur sans conversion, et rien ne le
        // dirait. On le signale une fois.
        if ret == -11 && chain.frames_in >= 30 && chain.frames_out == 0 {
            tracing::warn!(
                "Le graphe de filtres n'émet aucune image après {} frames \
                 (horodatage ou format d'entrée inexploitable) — image d'origine affichée",
                chain.frames_in
            );
        }
        let mut tmp = out;
        av_frame_free(&mut tmp);
        if ret != -(libc::EAGAIN) {
            tracing::warn!("Lecture de la sortie du graphe de filtres : {}", av_error_text(ret));
        }
        return None;
    }
    chain.frames_out += 1;
    Some(out)
}

/// Délai maximal accordé à la lecture d'un paquet sur un flux réseau.
///
/// C'est la borne du blocage de l'interface : le démultiplexeur travaille dans
/// le fil d'interface, donc une source muette fige la boucle d'événements au
/// plus ce délai, une fois, avant la constatation de la coupure.
const DELAI_LECTURE_MS: u64 = 3000;

/// Délai maximal accordé à l'ouverture d'un flux et à la lecture de ses
/// métadonnées (plus généreux : un serveur peut légitimement être lent).
const DELAI_OUVERTURE_MS: u64 = 10_000;

/// Code d'erreur FFmpeg signifiant « opération interrompue par l'appelant ».
const AVERROR_EXIT: i32 = -1_414_092_869;

/// Garde-temps des entrées/sorties réseau.
///
/// `rw_timeout` ne suffit pas : le démultiplexeur HLS ouvre lui-même les
/// segments, avec ses propres contextes d'E/S, et le délai passé à
/// `avformat_open_input` ne les atteint pas (constaté : une source muette
/// bloquait la lecture bien au-delà du délai demandé). FFmpeg consulte en
/// revanche `AVFormatContext.interrupt_callback` dans **toutes** ses boucles
/// d'E/S, y compris celles des segments : c'est le seul mécanisme qui borne
/// réellement l'attente.
struct DelaiReseau {
    /// Origine du comptage.
    debut: std::time::Instant,
    /// Instant (ms depuis `debut`) au-delà duquel on interrompt ; 0 = pas de limite.
    limite_ms: std::sync::atomic::AtomicU64,
}

impl DelaiReseau {
    fn nouveau() -> Self {
        Self {
            debut: std::time::Instant::now(),
            limite_ms: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Autorise `ms` millisecondes pour l'opération qui commence.
    fn armer(&self, ms: u64) {
        let limite = self.debut.elapsed().as_millis() as u64 + ms;
        self.limite_ms
            .store(limite, std::sync::atomic::Ordering::Release);
    }

    /// Plus de limite (opération terminée).
    fn desarmer(&self) {
        self.limite_ms
            .store(0, std::sync::atomic::Ordering::Release);
    }

    fn depasse(&self) -> bool {
        let limite = self.limite_ms.load(std::sync::atomic::Ordering::Acquire);
        limite != 0 && self.debut.elapsed().as_millis() as u64 >= limite
    }

}

/// Callback consulté par FFmpeg pendant les entrées/sorties bloquantes.
/// Renvoyer 1 interrompt l'opération (elle rend alors `AVERROR_EXIT`).
extern "C" fn cb_interruption(opaque: *mut libc::c_void) -> libc::c_int {
    if opaque.is_null() {
        return 0;
    }
    // Sûreté : `opaque` pointe vers le `DelaiReseau` détenu par le lecteur, qui
    // vit plus longtemps que le contexte de format (voir `close_media`).
    let delai = unsafe { &*(opaque as *const DelaiReseau) };
    if delai.depasse() {
        1
    } else {
        0
    }
}

/// Installe le garde-temps sur un contexte de format.
fn installer_delai_reseau(contexte: *mut AVFormatContext, delai: &std::sync::Arc<DelaiReseau>) {
    if contexte.is_null() {
        return;
    }
    unsafe {
        (*contexte).interrupt_callback = AVIOInterruptCB {
            callback: Some(cb_interruption),
            opaque: std::sync::Arc::as_ptr(delai) as *mut libc::c_void,
        };
    }
}

/// Vérifie qu'une URL répond, **sans bloquer l'interface**.
///
/// Ouvre le flux dans un thread séparé (contexte FFmpeg propre, refermé aussitôt)
/// et renvoie `true` si le serveur a bien répondu. Un serveur muet ferait
/// attendre `avformat_open_input` plusieurs secondes : dans le fil d'interface,
/// ce serait un gel à chaque tentative de reconnexion.
fn sonde_url(url: &str) -> bool {
    let Ok(nom) = CString::new(url) else {
        return false;
    };
    unsafe {
        let mut options: *mut AVDictionary = std::ptr::null_mut();
        // Délai volontairement court : au-delà, la source est considérée comme
        // absente et une autre tentative sera faite plus tard.
        let ck = CString::new("rw_timeout").unwrap();
        let cv = CString::new(SONDE_TIMEOUT_US).unwrap();
        av_dict_set(&mut options, ck.as_ptr(), cv.as_ptr(), 0);
        let ck2 = CString::new("timeout").unwrap();
        av_dict_set(&mut options, ck2.as_ptr(), cv.as_ptr(), 0);

        let ctx = PointeurFfmpeg(std::ptr::null_mut());
        let mut contexte = ctx.0;
        // Le délai `rw_timeout` ne couvre pas les E/S internes du démultiplexeur
        // HLS : on installe ici aussi le garde-temps consulté par FFmpeg.
        let ret = avformat_open_input(&mut contexte, nom.as_ptr(), std::ptr::null(), &mut options);
        if !options.is_null() {
            av_dict_free(&mut options);
        }
        if !contexte.is_null() {
            avformat_close_input(&mut contexte);
        }
        ret >= 0
    }
}

/// Un flux réseau qui se termine est-il arrivé au bout de son média ?
///
/// Distinction indispensable : le décodeur HLS renvoie `AVERROR_EOF` aussi bien
/// pour une fin de playlist que pour un serveur tombé (il saute alors les
/// segments manquants). Seule la position permet de trancher :
///   - durée connue et position presque au bout → c'est fini ;
///   - durée connue et position loin du bout → coupure ;
///   - durée inconnue (direct) → on considère que c'est une coupure : une
///     playlist de direct peut être rechargée plus tard, et insister coûte
///     quelques secondes alors qu'abandonner coûte le flux.
fn fin_legitime(position_ms: u64, duree_ms: u64) -> bool {
    duree_ms > 0 && position_ms + FIN_DE_FLUX_MARGE_MS >= duree_ms
}

/// Texte d'une erreur FFmpeg (`-22` → « Invalid argument »).
fn av_error_text(code: c_int) -> String {
    let mut buf = [0i8; 128];
    unsafe {
        if av_strerror(code, buf.as_mut_ptr(), buf.len()) == 0 {
            return std::ffi::CStr::from_ptr(buf.as_ptr())
                .to_string_lossy()
                .to_string();
        }
    }
    format!("code {}", code)
}

/// Traduit la méthode de mise à l'échelle choisie en options libswscale.
/// `SWS_ACCURATE_RND` évite les arrondis grossiers des rééchantillonnages forts.
fn scaling_flags(method: &crate::config::ScalingMethod) -> c_int {
    use crate::config::ScalingMethod::*;
    let flags = match method {
        NearestNeighbor => SWS_POINT,
        Bilinear => SWS_BILINEAR,
        Bicubic => SWS_BICUBIC,
        Lanczos => SWS_LANCZOS,
        Area => SWS_AREA,
    };
    flags | SWS_ACCURATE_RND
}

/// Redirige un message de journal FFmpeg vers `tracing`.
///
/// `va_list` n'est pas exploitable directement en Rust : on demande à FFmpeg de
/// formater la ligne lui-même (`av_log_format_line2`), puis on la transmet.
unsafe extern "C" fn ffmpeg_log_callback(
    ptr: *mut libc::c_void,
    level: c_int,
    fmt: *const libc::c_char,
    vl: *mut __va_list_tag,
) {
    const BUFFER_SIZE: usize = 512;
    let mut buffer = [0 as libc::c_char; BUFFER_SIZE];
    let mut print_prefix: c_int = 1;
    let written = av_log_format_line2(
        ptr,
        level,
        fmt,
        vl,
        buffer.as_mut_ptr(),
        BUFFER_SIZE as c_int,
        &mut print_prefix,
    );
    if written <= 0 {
        return;
    }
    let text = std::ffi::CStr::from_ptr(buffer.as_ptr())
        .to_string_lossy()
        .trim_end()
        .to_string();
    if text.is_empty() {
        return;
    }
    match level {
        l if l <= AV_LOG_ERROR => tracing::error!(target: "ffmpeg", "{}", text),
        l if l <= AV_LOG_WARNING => tracing::warn!(target: "ffmpeg", "{}", text),
        _ => tracing::debug!(target: "ffmpeg", "{}", text),
    }
}

/// Crée le contexte de périphérique de décodage VA-API (libva / Mesa / radeonsi).
/// Renvoie `None` si le pilote ou le matériel refuse : le lecteur reste alors en
/// décodage logiciel, sans erreur.
unsafe fn create_vaapi_device() -> Option<*mut AVBufferRef> {
    let mut device: *mut AVBufferRef = std::ptr::null_mut();
    let ret = av_hwdevice_ctx_create(
        &mut device,
        AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
        std::ptr::null(),
        std::ptr::null_mut(),
        0,
    );
    if ret < 0 || device.is_null() {
        tracing::warn!("VA-API indisponible (code {}) — décodage logiciel", ret);
        None
    } else {
        tracing::info!("VA-API : périphérique de décodage matériel prêt");
        Some(device)
    }
}

/// Callback `get_format` : choisit le format VA-API lorsqu'il est proposé.
/// Renvoyer autre chose qu'un format de la liste est interdit ; on se rabat donc
/// sur le premier format proposé (donc le logiciel) si VA-API n'y figure pas.
unsafe extern "C" fn select_hw_format(
    _ctx: *mut AVCodecContext,
    formats: *const AVPixelFormat,
) -> AVPixelFormat {
    if formats.is_null() {
        return AVPixelFormat::AV_PIX_FMT_NONE;
    }
    let mut p = formats;
    let mut first = AVPixelFormat::AV_PIX_FMT_NONE;
    while *p != AVPixelFormat::AV_PIX_FMT_NONE {
        if *p == AVPixelFormat::AV_PIX_FMT_VAAPI {
            return *p;
        }
        if first == AVPixelFormat::AV_PIX_FMT_NONE {
            first = *p;
        }
        p = p.add(1);
    }
    first
}

/// Vrai si ce décodeur sait produire des surfaces VA-API.
unsafe fn codec_supports_vaapi(codec: *const AVCodec) -> bool {
    let mut index = 0;
    loop {
        let config = avcodec_get_hw_config(codec, index);
        if config.is_null() {
            return false;
        }
        if (*config).pix_fmt == AVPixelFormat::AV_PIX_FMT_VAAPI {
            return true;
        }
        index += 1;
    }
}

/// Convertit une AVFrame décodée en buffer RGBA (Vec<u8>) via libswscale.
///
/// `target` est la taille d'affichage souhaitée en pixels (issue de la taille du
/// panneau vidéo, rapport d'aspect conservé). La conversion se fait directement à
/// cette taille : inutile de fabriquer une image 4K que le GPU devra réduire
/// ensuite — c'est le poste de dépense principal hors décodage (mesuré ×2,7 sur
/// une source 2160p, sans parler de l'upload de texture). `(0, 0)` = taille native,
/// et on n'agrandit jamais côté CPU : c'est le GPU qui s'en charge.
/// Chaîne de filtres **GPU** conservée entre les images.
///
/// Deux raisons de ne pas la reconstruire à chaque image :
///  1. construire un graphe libavfilter coûte cher (une allocation, une analyse de
///     description et une configuration à chaque image) ;
///  2. un désentrelaceur garde un **état** entre les images : reconstruit à chaque
///     fois, il ne produirait jamais de sortie (la première image ne sort qu'à la
///     deuxième). Constaté : `buffersink` renvoyait `EAGAIN` à chaque appel.
struct GpuChain {
    graph: *mut AVFilterGraph,
    src: *mut AVFilterContext,
    sink: *mut AVFilterContext,
    /// Description + géométrie + base de temps : toute différence impose de
    /// reconstruire (un graphe est lié à un format et à une taille).
    signature: String,
}

impl Drop for GpuChain {
    fn drop(&mut self) {
        unsafe {
            if !self.graph.is_null() {
                let mut g = self.graph;
                avfilter_graph_free(&mut g);
                self.graph = std::ptr::null_mut();
            }
        }
    }
}

/// Rapatrie une surface VA-API **après l'avoir réduite dans le GPU**.
///
/// **Pourquoi** (mesuré, `docs/OPTIMISATION-COMPARATIF.md`) : sans cela, chaque
/// image 1080p est recopiée telle quelle en mémoire système, puis réduite par
/// libswscale (CPU). Sur un flux 1080p affiché en 720p, la chaîne complète coûte
/// 9,95 ms/image ; en réduisant d'abord côté GPU (`scale_vaapi`) et en ne
/// rapatriant que le résultat, elle tombe à **4,60 ms/image** (−54 %), à qualité
/// comparable (même filtre bilinéaire matériel).
///
/// Le graphe est `buffer(surface) → scale_vaapi → hwdownload → format=nv12 →
/// buffersink` : il ne sort que des images **logicielles**, donc l'appelant continue
/// exactement comme avant (sws_scale ne fait plus qu'une conversion de couleur).
///
/// Renvoie `None` si le pilote refuse l'échelle matérielle : l'appelant retombe
/// alors sur le rapatriement simple. C'est le cas prévu, pas une erreur — certains
/// pilotes n'exposent pas `scale_vaapi` (comme `procamp_vaapi` ici).
unsafe fn chaine_gpu_rapatrier(
    vs: &mut VideoStream,
    frame: *mut AVFrame,
    dw: i32,
    dh: i32,
    prefixe: &str,
) -> Option<*mut AVFrame> {
    if (*frame).hw_frames_ctx.is_null() || dw < 16 || dh < 16 {
        tracing::debug!(
            "chaîne GPU : entrée inutilisable (hw_frames_ctx null={}, {}x{})",
            (*frame).hw_frames_ctx.is_null(),
            dw,
            dh
        );
        return None;
    }
    let signature = format!(
        "{}|{}x{}|{}x{}|{}|{}/{}",
        prefixe,
        (*frame).width,
        (*frame).height,
        dw,
        dh,
        (*frame).format,
        vs.time_base.num,
        vs.time_base.den
    );

    // Reconstruction : première image, ou changement de taille/format.
    let reconstruire = match vs.gpu_chain.as_ref() {
        Some(c) => c.signature != signature,
        None => true,
    };
    if reconstruire {
        vs.gpu_chain = None; // libère l'ancien graphe (Drop)
        match construire_chaine_gpu(frame, dw, dh, prefixe, vs.time_base, signature) {
            Ok(c) => vs.gpu_chain = Some(Box::new(c)),
            Err(raison) => {
                tracing::debug!("chaîne GPU indisponible : {}", raison);
                // On mémorise l'échec en gardant la signature : inutile de
                // réessayer à chaque image (25 tentatives par seconde).
                return None;
            }
        }
    }
    let chaine = vs.gpu_chain.as_mut()?;

    let ret = av_buffersrc_add_frame_flags(
        chaine.src,
        frame,
        AV_BUFFERSRC_FLAG_KEEP_REF as c_int,
    );
    if ret < 0 {
        tracing::debug!("chaîne GPU : frame refusée ({})", av_error_text(ret));
        vs.gpu_chain = None;
        return None;
    }

    // Un désentrelaceur peut ne rien sortir tout de suite (il lui faut l'image
    // suivante) : on vide ce qui est disponible et on garde la dernière image.
    let mut derniere: *mut AVFrame = std::ptr::null_mut();
    loop {
        let out = av_frame_alloc();
        if out.is_null() {
            break;
        }
        let ret = av_buffersink_get_frame(chaine.sink, out);
        if ret < 0 {
            let mut tmp = out;
            av_frame_free(&mut tmp);
            break;
        }
        if !derniere.is_null() {
            let mut tmp = derniere;
            av_frame_free(&mut tmp);
        }
        derniere = out;
    }
    if derniere.is_null() {
        tracing::debug!("chaîne GPU : aucune image en sortie (EAGAIN)");
        return None;
    }
    Some(derniere)
}

/// Construit le graphe GPU décrit par `prefixe` + `scale_vaapi` + `hwdownload`.
unsafe fn construire_chaine_gpu(
    frame: *mut AVFrame,
    dw: i32,
    dh: i32,
    prefixe: &str,
    time_base: AVRational,
    signature: String,
) -> Result<GpuChain, String> {
    let mut graph = avfilter_graph_alloc();
    if graph.is_null() {
        return Err("allocation du graphe".to_string());
    }
    let Ok(desc) = CString::new(format!(
        "{}{}scale_vaapi=w={}:h={}:format=nv12,hwdownload,format=nv12",
        prefixe,
        if prefixe.is_empty() { "" } else { "," },
        dw,
        dh
    )) else {
        avfilter_graph_free(&mut graph);
        return Err("description refusée".to_string());
    };

    // Source : un `buffer` dont on décrit la surface matérielle.
    // `avfilter_graph_create_filter` refuse un `buffer` sans arguments : il faut
    // allouer le filtre puis lui donner ses paramètres AVANT de l'initialiser
    // (`av_buffersrc_parameters_set` → `avfilter_init_str`). C'est le seul ordre
    // accepté pour transmettre `hw_frames_ctx`.
    let filtre_buffer = avfilter_get_by_name(b"buffer\0".as_ptr() as *const std::os::raw::c_char);
    if filtre_buffer.is_null() {
        avfilter_graph_free(&mut graph);
        return Err("filtre « buffer » absent".to_string());
    }
    let src = avfilter_graph_alloc_filter(
        graph,
        filtre_buffer,
        b"in\0".as_ptr() as *const std::os::raw::c_char,
    );
    if src.is_null() {
        avfilter_graph_free(&mut graph);
        return Err("allocation du filtre « buffer »".to_string());
    }
    let params = av_buffersrc_parameters_alloc();
    if params.is_null() {
        avfilter_graph_free(&mut graph);
        return Err("allocation des paramètres de source".to_string());
    }
    (*params).format = (*frame).format;
    (*params).width = (*frame).width;
    (*params).height = (*frame).height;
    // `frame->time_base` vaut souvent 0/1 (les décodeurs ne le remplissent pas) :
    // le passer tel quel fait échouer l'initialisation avec « Invalid argument ».
    (*params).time_base = if time_base.num > 0 && time_base.den > 0 {
        time_base
    } else {
        AVRational { num: 1, den: 25 }
    };
    (*params).sample_aspect_ratio = (*frame).sample_aspect_ratio;
    (*params).hw_frames_ctx = av_buffer_ref((*frame).hw_frames_ctx);
    let ret = av_buffersrc_parameters_set(src, params);
    av_free(params as *mut std::os::raw::c_void);
    if ret < 0 {
        avfilter_graph_free(&mut graph);
        return Err(format!("paramètres de surface refusés ({})", av_error_text(ret)));
    }
    let ret = avfilter_init_str(src, std::ptr::null());
    if ret < 0 {
        avfilter_graph_free(&mut graph);
        return Err(format!("initialisation de la source ({})", av_error_text(ret)));
    }

    let mut sink: *mut AVFilterContext = std::ptr::null_mut();
    let ret = avfilter_graph_create_filter(
        &mut sink,
        avfilter_get_by_name(b"buffersink\0".as_ptr() as *const std::os::raw::c_char),
        b"out\0".as_ptr() as *const std::os::raw::c_char,
        std::ptr::null(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 || sink.is_null() {
        avfilter_graph_free(&mut graph);
        return Err(format!("puits refusé ({})", av_error_text(ret)));
    }

    let mut outputs = avfilter_inout_alloc();
    let mut inputs = avfilter_inout_alloc();
    if outputs.is_null() || inputs.is_null() {
        avfilter_inout_free(&mut outputs);
        avfilter_inout_free(&mut inputs);
        avfilter_graph_free(&mut graph);
        return Err("allocation des liaisons".to_string());
    }
    (*outputs).name = av_strdup(b"in\0".as_ptr() as *const std::os::raw::c_char);
    (*outputs).filter_ctx = src;
    (*outputs).pad_idx = 0;
    (*outputs).next = std::ptr::null_mut();
    (*inputs).name = av_strdup(b"out\0".as_ptr() as *const std::os::raw::c_char);
    (*inputs).filter_ctx = sink;
    (*inputs).pad_idx = 0;
    (*inputs).next = std::ptr::null_mut();
    if (*outputs).name.is_null() || (*inputs).name.is_null() {
        avfilter_inout_free(&mut outputs);
        avfilter_inout_free(&mut inputs);
        avfilter_graph_free(&mut graph);
        return Err("allocation des noms de liaison".to_string());
    }
    let ret = avfilter_graph_parse_ptr(
        graph,
        desc.as_ptr(),
        &mut inputs,
        &mut outputs,
        std::ptr::null_mut(),
    );
    avfilter_inout_free(&mut inputs);
    avfilter_inout_free(&mut outputs);
    if ret < 0 {
        avfilter_graph_free(&mut graph);
        return Err(format!("description « {} » refusée ({})", prefixe, av_error_text(ret)));
    }
    let ret = avfilter_graph_config(graph, std::ptr::null_mut());
    if ret < 0 {
        avfilter_graph_free(&mut graph);
        return Err(format!("configuration refusée ({})", av_error_text(ret)));
    }
    Ok(GpuChain {
        graph,
        src,
        sink,
        signature,
    })
}

unsafe fn scale_frame_to_rgba(
    vs: &mut VideoStream,
    frame: *mut AVFrame,
    target: (u32, u32),
    method: &crate::config::ScalingMethod,
    filters_description: &str,
    gpu_filters: Option<&str>,
    dims_out: &mut (u32, u32),
) -> Option<Vec<u8>> {
    // Taille de réduction souhaitée, calculée AVANT le rapatriement : c'est elle
    // qui permet de faire la réduction dans le GPU plutôt qu'après la copie.
    let (cible_w, cible_h) = {
        let (fw, fh) = ((*frame).width, (*frame).height);
        if target.0 >= 16 && target.1 >= 16 && target.0 < fw as u32 && target.1 < fh as u32 {
            (
                ((target.0 / 2) * 2).max(16) as i32,
                ((target.1 / 2) * 2).max(16) as i32,
            )
        } else {
            (fw, fh)
        }
    };

    // Frame matérielle : la surface VA-API doit d'abord être rapatriée en
    // mémoire système (NV12 en général) avant la conversion RGBA.
    let mut transferred: *mut AVFrame = std::ptr::null_mut();
    let mut source = frame;
    // Vrai quand le GPU a déjà appliqué les filtres demandés (désentrelacement) :
    // la chaîne CPU doit alors être sautée, sinon ils seraient appliqués deux fois.
    let mut filtres_faits_par_gpu = false;
    if (*frame).format == AVPixelFormat::AV_PIX_FMT_VAAPI as c_int {
        // Preuve d'usage réel du matériel
        vs.hw_active = true;

        // --- Optimisation mesurée : réduire DANS le GPU ---
        // Sans filtre CPU à appliquer, la copie 1080p puis la réduction par swscale
        // sont remplacées par une réduction matérielle suivie d'une copie du seul
        // résultat (9,95 → 4,60 ms/image, cf. docs/OPTIMISATION-COMPARATIF.md).
        // Dès qu'un filtre CPU est demandé, on garde l'ordre actuel : les filtres
        // travaillent sur la résolution native (leurs seuils en dépendent).
        // Chemin GPU : soit la chaîne est vide (il ne reste que la réduction), soit
        // elle se réduit au désentrelacement, qui a un équivalent matériel.
        let prefixe_gpu = if filters_description.is_empty() {
            Some("")
        } else {
            gpu_filters
        };
        let reduction_utile = cible_w < (*frame).width || cible_h < (*frame).height;
        tracing::debug!(
            "décision GPU : voulue={} reduction={} prefixe={:?} chaine_cpu={:?}",
            vs.echelle_gpu_voulue, reduction_utile, gpu_filters, filters_description
        );
        if vs.echelle_gpu_voulue && (reduction_utile || gpu_filters.is_some()) {
            if let Some(prefixe) = prefixe_gpu {
                if let Some(software) = chaine_gpu_rapatrier(vs, frame, cible_w, cible_h, prefixe)
                {
                    if !vs.echelle_gpu_active {
                        vs.echelle_gpu_active = true;
                        // Transparence : le pilote utilise SON filtre haute qualité
                        // (bilinéaire polyphasé), pas la méthode choisie pour le CPU
                        // (Lanczos par défaut, plus net). Mesuré : 30,7 dB d'écart
                        // sur une mire à contours nets — d'où la case à décocher.
                        if !matches!(
                            method,
                            crate::config::ScalingMethod::Bilinear
                                | crate::config::ScalingMethod::NearestNeighbor
                        ) {
                            tracing::info!(
                                "Échelle matérielle : filtre HQ du pilote (la méthode {:?} \
                                 ne s'applique qu'au chemin CPU)",
                                method
                            );
                        }
                        tracing::info!(
                            "Traitement par le GPU actif : {}x{} → {}x{} ({}scale_vaapi)",
                            (*frame).width,
                            (*frame).height,
                            cible_w,
                            cible_h,
                            if prefixe.is_empty() { "" } else { "deinterlace_vaapi + " }
                        );
                    }
                    transferred = software;
                    source = software;
                    filtres_faits_par_gpu = !prefixe.is_empty();
                }
            }
        }

        if transferred.is_null() {
            let software = av_frame_alloc();
            if software.is_null() {
                return None;
            }
            // **Les horodatages ne suivent pas le rapatriement.** Mesuré : la frame
            // rendue par `av_hwframe_transfer_data` a `pts = AV_NOPTS_VALUE` et une
            // durée nulle. Le moteur s'en passe pour l'affichage (il suit le temps
            // ailleurs), mais pas un graphe de filtres : `libplacebo` refuse alors de
            // rendre quoi que ce soit — le puits répond `EAGAIN` à chaque image, la
            // chaîne est reconstruite inutilement et l'image affichée reste celle du
            // décodeur. On les recopie donc explicitement.
            let pts_source = (*frame).pts;
            let duree_source = (*frame).duration;
            let meilleur_source = (*frame).best_effort_timestamp;
            if av_hwframe_transfer_data(software, frame, 0) < 0 {
            vs.hw_failures += 1;
            if vs.hw_failures <= 3 {
                tracing::warn!(
                    "Rapatriement d'une surface VA-API impossible (échec n°{})",
                    vs.hw_failures
                );
            }
                let mut tmp = software;
                av_frame_free(&mut tmp);
                return None;
            }
            (*software).pts = pts_source;
            (*software).duration = duree_source;
            (*software).best_effort_timestamp = meilleur_source;
            transferred = software;
            source = software;
        }
    }

    // --- Chaîne de filtres (libavfilter) : détramage, débruitage, désentrelacement,
    // réglages d'image. Placée AVANT la mise à l'échelle : les filtres travaillent
    // sur la résolution native (les seuils de `deband` et la détection
    // d'entrelacement de `yadif` supposent l'image d'origine).
    let mut filtered: *mut AVFrame = std::ptr::null_mut();
    if !filters_description.is_empty() && !filtres_faits_par_gpu {
        if let Some(f) = apply_video_filters(vs, filters_description, source) {
            filtered = f;
            source = f;
        }
    }

    let width = (*source).width;
    let height = (*source).height;
    if width <= 0 || height <= 0 {
        if !transferred.is_null() {
            let mut tmp = transferred;
            av_frame_free(&mut tmp);
        }
        if !filtered.is_null() {
            let mut tmp = filtered;
            av_frame_free(&mut tmp);
        }
        return None;
    }

    // Dimensions de sortie : réduction éventuelle vers la taille d'affichage
    // (largeurs paires, et jamais plus grande que la source).
    let mut dw = width;
    let mut dh = height;
    if target.0 >= 16 && target.1 >= 16 && target.0 < width as u32 && target.1 < height as u32 {
        dw = ((target.0 / 2) * 2).max(16) as i32;
        dh = ((target.1 / 2) * 2).max(16) as i32;
    }

    // Frames logicielles alors que VA-API était demandé : le pilote a refusé le
    // profil et libavcodec a basculé en logiciel (sans erreur visible).
    if vs.hw_requested && !vs.hw_active && !vs.hw_fallback_logged {
        vs.hw_fallback_logged = true;
        tracing::info!("VA-API demandé mais le flux est décodé en logiciel (profil non pris en charge)");
    }

    let src_format: AVPixelFormat = std::mem::transmute::<c_int, AVPixelFormat>((*source).format);

    // Le format d'entrée peut changer en cours de flux (ou être NV12 après un
    // rapatriement matériel) et la taille d'affichage varie avec la fenêtre :
    // dans ce cas le contexte swscale est reconstruit.
    if !vs.scale_context.is_null()
        && (vs.scale_format != src_format
            || vs.scale_method != *method
            || vs.scale_src_width != width
            || vs.scale_src_height != height
            || vs.scale_dst_width != dw
            || vs.scale_dst_height != dh)
    {
        sws_freeContext(vs.scale_context);
        vs.scale_context = std::ptr::null_mut();
    }

    if vs.scale_context.is_null() {
        vs.scale_context = sws_getContext(
            width,
            height,
            src_format,
            dw,
            dh,
            AVPixelFormat::AV_PIX_FMT_RGBA,
            scaling_flags(method),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        vs.scale_format = src_format;
        vs.scale_method = method.clone();
        vs.scale_src_width = width;
        vs.scale_src_height = height;
        vs.scale_dst_width = dw;
        vs.scale_dst_height = dh;
    }
    if vs.scale_context.is_null() {
        if !transferred.is_null() {
            let mut tmp = transferred;
            av_frame_free(&mut tmp);
        }
        if !filtered.is_null() {
            let mut tmp = filtered;
            av_frame_free(&mut tmp);
        }
        return None;
    }

    let linesize = dw * 4;
    let mut buffer = vec![0u8; (linesize * dh) as usize];
    let dst_data: [*mut u8; 4] = [buffer.as_mut_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()];
    let dst_stride: [c_int; 4] = [linesize, 0, 0, 0];

    sws_scale(
        vs.scale_context,
        (*source).data.as_ptr() as *const *const u8,
        (*source).linesize.as_ptr(),
        0,
        height,
        dst_data.as_ptr(),
        dst_stride.as_ptr(),
    );

    if !transferred.is_null() {
        let mut tmp = transferred;
        av_frame_free(&mut tmp);
    }
    if !filtered.is_null() {
        let mut tmp = filtered;
        av_frame_free(&mut tmp);
    }

    dims_out.0 = dw as u32;
    dims_out.1 = dh as u32;
    Some(buffer)
}

/// Décode une AVFrame audio et ajoute les échantillons f32 entrelacés à `out`.
unsafe fn decode_audio_frame_to_f32(aus: &mut AudioStream, frame: *mut AVFrame, out: &mut Vec<f32>) {
    if aus.swr.is_null() {
        return;
    }
    let in_samples = (*frame).nb_samples;
    if in_samples <= 0 {
        return;
    }
    // extended_data couvre les formats à plus de 8 plans ; sinon data suffit
    let src_planes = if (*frame).extended_data.is_null() {
        (*frame).data.as_ptr()
    } else {
        (*frame).extended_data
    };

    // Nombre de samples de sortie (majoré) puis allocation du tampon
    let out_samples = swr_get_out_samples(aus.swr, in_samples);
    if out_samples <= 0 {
        return;
    }
    let channels = aus.out_layout.nb_channels.max(1) as usize;
    let total = out_samples as usize * channels;

    let mut buffer: Vec<f32> = vec![0.0; total];
    let mut out_ptr: [*mut u8; 1] = [buffer.as_mut_ptr() as *mut u8];

    let produced = swr_convert(
        aus.swr,
        out_ptr.as_mut_ptr(),
        out_samples,
        src_planes as *const *const u8,
        in_samples,
    );

    if produced > 0 {
        let n = produced as usize * channels;
        out.extend_from_slice(&buffer[..n.min(buffer.len())]);
    }
}

/// Graine du mélange : l'horloge système, faute de mieux.
///
/// Une graine fixe donnerait la **même** séquence de lecture à chaque lancement —
/// ce qui n'est pas « aléatoire ». La valeur n'a pas besoin d'être
/// cryptographique : elle sert à ne pas rejouer deux fois le même parcours.
fn graine_aleatoire() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e37_79b9_7f4a_7c15)
}

/// Mélange de `0..n` (Fisher-Yates), déterministe pour une graine donnée.
///
/// **Pourquoi cette fonction est séparée et publique** : elle est la seule partie
/// vérifiable sans écran ni fichier — un mélange doit être une **permutation**
/// (chaque élément une fois, aucun oubli, aucun doublon), et c'est exactement ce
/// que le banc et les tests contrôlent. Le générateur est un xorshift64* : trois
/// lignes, aucune dépendance, une distribution suffisante pour 10 000 titres.
pub fn melange(n: usize, graine: u64) -> Vec<usize> {
    let mut ordre: Vec<usize> = (0..n).collect();
    if n < 2 {
        return ordre;
    }
    let mut etat = graine | 1; // un état nul resterait bloqué à zéro
    let mut suivant = || {
        etat ^= etat >> 12;
        etat ^= etat << 25;
        etat ^= etat >> 27;
        etat.wrapping_mul(0x2545_f491_4f6c_dd1d)
    };
    for i in (1..n).rev() {
        let j = (suivant() % (i as u64 + 1)) as usize;
        ordre.swap(i, j);
    }
    ordre
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        let _ = self.quit();
    }
}

#[cfg(test)]
mod tests_filtres {
    use super::*;
    use crate::config::{DeinterlaceFilter, DeinterlaceParity, VideoSettings};

    #[test]
    fn le_desentrelacement_utilise_le_filtre_choisi() {
        // Le nom du filtre dans la chaîne est la seule preuve que le réglage atteint
        // libavfilter : on vérifie les deux valeurs, et pas seulement le défaut.
        let mut video = VideoSettings::default();
        video.deinterlace = true;
        video.deinterlace_filter = DeinterlaceFilter::Bwdif;
        let chaine = filter_description(&video);
        assert!(
            chaine.starts_with("bwdif=mode=send_frame:parity=auto:deint=interlaced"),
            "chaîne inattendue : {}",
            chaine
        );

        video.deinterlace_filter = DeinterlaceFilter::Yadif;
        let chaine = filter_description(&video);
        assert!(
            chaine.starts_with("yadif=mode=send_frame:parity=auto:deint=interlaced"),
            "chaîne inattendue : {}",
            chaine
        );
    }

    #[test]
    fn les_deux_filtres_recoivent_la_parite() {
        // `bwdif` et `yadif` acceptent les mêmes options : une parité choisie doit
        // apparaître pareillement, sinon `bwdif` serait moins réglable que `yadif`.
        let mut video = VideoSettings::default();
        video.deinterlace = true;
        for (parite, texte) in [
            (DeinterlaceParity::TopFirst, "parity=tff"),
            (DeinterlaceParity::BottomFirst, "parity=bff"),
            (DeinterlaceParity::Auto, "parity=auto"),
        ] {
            video.deinterlace_parity = parite;
            for filtre in [DeinterlaceFilter::Bwdif, DeinterlaceFilter::Yadif] {
                video.deinterlace_filter = filtre;
                let chaine = filter_description(&video);
                assert!(
                    chaine.contains(texte),
                    "{} sans « {} » : {}",
                    filtre.nom(),
                    texte,
                    chaine
                );
            }
        }
    }

    #[test]
    fn pas_de_desentrelacement_quand_il_est_desactive() {
        let mut video = VideoSettings::default();
        video.deinterlace = false;
        video.deinterlace_filter = DeinterlaceFilter::Bwdif;
        let chaine = filter_description(&video);
        assert!(!chaine.contains("bwdif"), "chaîne : {}", chaine);
        assert!(!chaine.contains("yadif"), "chaîne : {}", chaine);
    }

    #[test]
    fn le_filtre_se_relit_depuis_la_configuration() {
        // Défaut : bwdif (mesuré meilleur). Et un fichier qui écrit « yadif » — ou
        // une variante de casse — doit être compris, sans perdre le reste.
        let defaut: VideoSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(defaut.deinterlace_filter, DeinterlaceFilter::Bwdif);
        let y: VideoSettings =
            serde_json::from_str(r#"{"deinterlace_filter": "yadif"}"#).unwrap();
        assert_eq!(y.deinterlace_filter, DeinterlaceFilter::Yadif);
        let y2: VideoSettings =
            serde_json::from_str(r#"{"deinterlace_filter": "Yadif"}"#).unwrap();
        assert_eq!(y2.deinterlace_filter, DeinterlaceFilter::Yadif);
    }
}

#[cfg(test)]
mod tests_sousimages {
    use super::*;

    /// Palette telle que FFmpeg la livre : 0xAARRGGBB.
    fn palette() -> Vec<u32> {
        vec![0x0000_0000, 0xFFFF_FF00, 0xFFFF_0000, 0x8000_FF00]
    }

    #[test]
    fn un_indice_de_palette_devient_un_pixel_rgba() {
        // Une ligne de 2 pixels : index 1 (jaune opaque) puis 2 (rouge opaque).
        let indices = vec![1u8, 2];
        let rgba = bitmap_vers_rgba(&indices, 2, 2, 1, &palette()).unwrap();
        assert_eq!(rgba, vec![255, 255, 0, 255, 255, 0, 0, 255]);
    }

    #[test]
    fn la_transparence_est_conservee() {
        // Index 0 : alpha nul → pixel entièrement à zéro (aucune couleur parasite).
        let rgba = bitmap_vers_rgba(&[0u8, 1], 2, 2, 1, &palette()).unwrap();
        assert_eq!(&rgba[0..4], &[0, 0, 0, 0]);
        assert_eq!(&rgba[4..8], &[255, 255, 0, 255]);
    }

    #[test]
    fn l_alpha_partiel_est_recopie() {
        let rgba = bitmap_vers_rgba(&[3u8], 1, 1, 1, &palette()).unwrap();
        assert_eq!(rgba, vec![0, 255, 0, 0x80]);
    }

    #[test]
    fn un_index_hors_palette_devient_blanc_opaque() {
        // Mieux vaut un sous-titre blanc visible qu'un pixel invisible qu'on ne
        // saurait pas diagnostiquer.
        let rgba = bitmap_vers_rgba(&[7u8], 1, 1, 1, &palette()).unwrap();
        assert_eq!(rgba, vec![255, 255, 255, 255]);
    }

    #[test]
    fn sans_palette_du_tout_les_pixels_sont_blancs() {
        let rgba = bitmap_vers_rgba(&[1u8, 2], 2, 2, 1, &[]).unwrap();
        assert_eq!(rgba, vec![255, 255, 255, 255, 255, 255, 255, 255]);
    }

    #[test]
    fn les_lignes_alignees_sont_lues_avec_le_bon_decalage() {
        // linesize (6) > width (3) : le remplissage de fin de ligne est ignoré.
        let indices = vec![1u8, 1, 1, 9, 9, 9, 2, 2, 2, 9, 9, 9];
        let rgba = bitmap_vers_rgba(&indices, 6, 3, 2, &palette()).unwrap();
        // première ligne : jaune ; deuxième : rouge
        assert_eq!(&rgba[0..4], &[255, 255, 0, 255]);
        assert_eq!(&rgba[8..12], &[255, 255, 0, 255]);
        assert_eq!(&rgba[12..16], &[255, 0, 0, 255]);
    }

    #[test]
    fn un_rectangle_incoherent_est_refuse() {
        // Largeur nulle, hauteur nulle, ou tampon trop court : pas de rendu.
        assert!(bitmap_vers_rgba(&[], 0, 0, 0, &palette()).is_none());
        assert!(bitmap_vers_rgba(&[1, 1], 2, 2, 2, &palette()).is_none());
        // Dimension cohérente mais aucune donnée : refusé aussi.
        assert!(bitmap_vers_rgba(&[], 4, 4, 4, &palette()).is_none());
    }

    #[test]
    fn la_taille_du_rect_est_bien_quatre_octets_par_pixel() {
        let rgba = bitmap_vers_rgba(&[1u8; 12], 4, 4, 3, &palette()).unwrap();
        assert_eq!(rgba.len(), 4 * 3 * 4);
    }

    #[test]
    fn un_evenement_image_a_une_duree_et_une_extinction() {
        // Un flux PGS éteint par une composition vide : le cue porte une fin.
        let cue = BitmapCue {
            start_ms: 1000,
            end_ms: 4000,
            rects: vec![BitmapRect {
                x: 10,
                y: 20,
                width: 2,
                height: 2,
                rgba: vec![0; 16],
            }],
        };
        assert_eq!(bitmap_cue_bytes(&cue), 16);
    }

    #[test]
    fn le_budget_memoire_se_calcule_sur_les_pixels() {
        let grand = BitmapCue {
            start_ms: 0,
            end_ms: 1000,
            rects: vec![BitmapRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                rgba: vec![0; 1920 * 1080 * 4],
            }],
        };
        // Une composition plein écran 1080p pèse 8 Mo : le budget de 64 Mo
        // n'en garde donc que 8 — c'est bien un budget d'octets, pas de nombre.
        assert_eq!(bitmap_cue_bytes(&grand), 8_294_400);
        assert!(MAX_BITMAP_BYTES / bitmap_cue_bytes(&grand) == 8);
    }
}

#[cfg(test)]
mod tests_reseau {
    use super::*;

    #[test]
    fn une_fin_de_playlist_au_bout_du_media_est_une_vraie_fin() {
        // 60 s de playlist, coupure à 59,5 s : le média est fini.
        assert!(fin_legitime(59_500, 60_000));
        // Fin exactement à la durée annoncée.
        assert!(fin_legitime(60_000, 60_000));
        // Position au-delà de la durée (PTS arrondis) : toujours une fin.
        assert!(fin_legitime(61_333, 60_000));
    }

    #[test]
    fn une_fin_de_playlist_en_cours_de_media_est_une_coupure() {
        // Le cas mesuré : segments perdus, le décodeur rend EOF à 9,4 s d'une
        // playlist de 60 s. Ce n'est PAS une fin.
        assert!(!fin_legitime(9_466, 60_000));
        assert!(!fin_legitime(0, 60_000));
        // À 3 s de la fin, on est encore dans la marge.
        assert!(!fin_legitime(56_000, 60_000));
    }

    #[test]
    fn une_duree_inconnue_est_traitee_comme_une_coupure() {
        // Direct : durée NOPTS (0). Le flux peut reprendre, on réessaie.
        assert!(!fin_legitime(120_000, 0));
        assert!(!fin_legitime(0, 0));
    }

    #[test]
    fn le_delai_double_puis_plafonne() {
        // 0,3 s → 0,6 → 1,2 → 2,4 → 4,8 → 8 → 8 (plafond)
        let attendu = [300, 600, 1200, 2400, 4800, 8000, 8000, 8000];
        for (i, ms) in attendu.iter().enumerate() {
            let d = delai_reconnexion(i as u32 + 1);
            assert_eq!(d.as_millis() as u64, *ms, "tentative {}", i + 1);
        }
        // La somme reste bornée : l'utilisateur n'attend jamais indéfiniment.
        let total: u128 = (1..=RESEAU_TENTATIVES_MAX)
            .map(|n| delai_reconnexion(n).as_millis())
            .sum();
        assert!(total < 45_000, "attente cumulée {} ms", total);
    }

    #[test]
    fn un_flux_hls_est_reconnu_meme_avec_parametres() {
        assert!(NetworkStream::nouveau("https://exemple.test/live/index.m3u8").hls);
        assert!(NetworkStream::nouveau("https://exemple.test/live/index.m3u").hls);
        assert!(NetworkStream::nouveau("https://exemple.test/INDEX.M3U8").hls);
        // Paramètres et ancre ne doivent pas empêcher la reconnaissance.
        assert!(NetworkStream::nouveau("https://exemple.test/p.m3u8?token=abc#t=10").hls);
        // Un MP4 servi en HTTP n'est pas une playlist.
        assert!(!NetworkStream::nouveau("https://exemple.test/film.mp4").hls);
        // Une adresse dont un dossier s'appelle « .m3u8 » non plus.
        assert!(!NetworkStream::nouveau("https://exemple.test/x.m3u8/film.ts").hls);
    }

    #[test]
    fn le_garde_temps_n_interrompt_que_quand_il_est_arme() {
        let delai = DelaiReseau::nouveau();
        // Désarmé : jamais d'interruption, même après une longue attente.
        assert!(!delai.depasse());
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!delai.depasse());
    }

    #[test]
    fn le_garde_temps_interrompt_au_dela_du_delai() {
        let delai = DelaiReseau::nouveau();
        delai.armer(40);
        // Juste après l'armement, l'opération a le droit de se poursuivre.
        assert!(!delai.depasse());
        std::thread::sleep(std::time::Duration::from_millis(80));
        assert!(delai.depasse(), "le délai de 40 ms devait être dépassé");
    }

    #[test]
    fn desarmer_libere_l_operation_suivante() {
        let delai = DelaiReseau::nouveau();
        delai.armer(10);
        std::thread::sleep(std::time::Duration::from_millis(30));
        assert!(delai.depasse());
        delai.desarmer();
        assert!(!delai.depasse(), "une opération suivante ne doit pas être interrompue");
    }

    #[test]
    fn le_chemin_gpu_ne_prend_que_le_desentrelacement_seul() {
        // Le désentrelacement est le seul filtre qui a un équivalent VA-API. Tout
        // autre filtre (courbe, débruitage, recadrage, rotation) doit garder le
        // chemin CPU : mélanger les deux ferait deux passages au lieu d'un gain.
        let mut video = crate::config::VideoSettings::default();
        video.deinterlace = true;
        assert_eq!(
            filtre_gpu_equivalent(&video, "bwdif=mode=send_frame:parity=auto:deint=interlaced"),
            Some("deinterlace_vaapi")
        );
        assert_eq!(
            filtre_gpu_equivalent(&video, "yadif=mode=send_frame:parity=tff:deint=interlaced"),
            Some("deinterlace_vaapi")
        );
        // Désentrelacement + un autre filtre : pas d'équivalent GPU.
        assert_eq!(
            filtre_gpu_equivalent(
                &video,
                "bwdif=mode=send_frame:parity=auto:deint=interlaced,hqdn3d=8:8:6:6"
            ),
            None
        );
        // Un autre filtre seul : pas d'équivalent GPU non plus.
        assert_eq!(filtre_gpu_equivalent(&video, "eq=brightness=0.100"), None);
        // Désentrelacement désactivé : rien à déporter, même si la chaîne y ressemble.
        video.deinterlace = false;
        assert_eq!(
            filtre_gpu_equivalent(&video, "bwdif=mode=send_frame:parity=auto:deint=interlaced"),
            None
        );
    }

    #[test]
    fn un_flux_neuf_ne_est_pas_en_panne() {
        let net = NetworkStream::nouveau("http://127.0.0.1/flux.m3u8");
        assert!(!net.en_panne);
        assert_eq!(net.tentatives, 0);
        assert!(net.prochaine_a.is_none());
        assert!(net.derniere_erreur.is_none());
        assert_eq!(net.url, "http://127.0.0.1/flux.m3u8");
    }
}

#[cfg(test)]
mod tests_etage_hdr {
    use super::decouper_etage_hdr;

    #[test]
    fn l_etage_hdr_se_decoupe_en_tete_uniquement() {
        // Cas normal : étage GPU en tête, filtres d'image derrière.
        let (sans, reste) = decouper_etage_hdr("libplacebo=tonemapping=bt.2390,eq=contrast=1.2")
            .expect("étage reconnu");
        assert_eq!(sans, "eq=contrast=1.2");
        assert_eq!(reste, "eq=contrast=1.2");

        // Repli CPU : même découpe.
        let (sans, _) = decouper_etage_hdr("zscale=t=linear,tonemap=tonemap=hable,format=yuv420p")
            .expect("étage CPU reconnu");
        assert_eq!(sans, "tonemap=tonemap=hable,format=yuv420p");

        // Étage seul : il ne reste rien (l'appelant ne doit pas construire un graphe vide).
        let (sans, reste) = decouper_etage_hdr("libplacebo=tonemapping=clip").expect("reconnu");
        assert!(sans.is_empty());
        assert!(reste.is_empty());

        // Une chaîne qui ne commence pas par un étage HDR n'est pas touchée : sans
        // cela, un `format=` ou un `eq=` en tête serait pris pour l'étage.
        assert!(decouper_etage_hdr("eq=contrast=1.2,libplacebo=x").is_none());
        assert!(decouper_etage_hdr("unsharp=5:5:0.8").is_none());
        assert!(decouper_etage_hdr("").is_none());
    }
}

#[cfg(test)]
mod tests_aleatoire {
    use super::melange;

    #[test]
    fn le_melange_est_une_permutation() {
        // Le point qui compte : aucune perte, aucun doublon, quelle que soit la
        // graine. Une seule entrée oubliée et un élément de la playlist ne serait
        // jamais lu en mode aléatoire.
        for n in [0usize, 1, 2, 3, 7, 50, 999] {
            for graine in [1u64, 42, 0xdead_beef, u64::MAX] {
                let ordre = melange(n, graine);
                assert_eq!(ordre.len(), n, "taille fausse pour n={}", n);
                let mut vu = vec![false; n];
                for i in &ordre {
                    assert!(*i < n, "indice hors bornes pour n={}", n);
                    assert!(!vu[*i], "doublon {} pour n={}", i, n);
                    vu[*i] = true;
                }
                assert!(vu.iter().all(|v| *v), "élément oublié pour n={}", n);
            }
        }
    }

    #[test]
    fn le_melange_change_avec_la_graine_et_avec_le_temps() {
        // Deux graines différentes doivent donner deux parcours différents : c'est
        // toute la différence entre « aléatoire » et « ordre fixe rebaptisé ».
        let a = melange(20, 12345);
        let b = melange(20, 12346);
        assert_ne!(a, b, "deux graines voisines donnent le même parcours");
        assert_ne!(a, (0..20).collect::<Vec<_>>(), "le mélange n'a rien mélangé");
        // Et la graine réelle change d'un appel à l'autre (horloge).
        let s1 = super::graine_aleatoire();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let s2 = super::graine_aleatoire();
        assert_ne!(s1, s2, "la graine ne varie pas : parcours identique à chaque lancement");
    }
}

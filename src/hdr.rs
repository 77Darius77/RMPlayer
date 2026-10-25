//! HDR : détection, conversion tonale, 10 bits et tramage.
//!
//! **Le besoin.** Un film HDR10 (PQ/BT.2020) ou HLG lu tel quel sur un écran SDR
//! apparaît **lavé** : les basses lumières sont trop claires, les hautes écrasées,
//! et les couleurs désaturées. Ce module détecte les métadonnées du flux et décide
//! de l'étage à insérer dans la chaîne de filtres.
//!
//! **Deux moteurs, un choix mesuré** :
//!
//! - `libplacebo` (`libavfilter`, Vulkan) : conversion tonale de référence (BT.2390,
//!   ST.2094-40, Hable…), *gamut mapping* perceptuel, détection de crête dynamique,
//!   tramage par diffusion d'erreur, débanding GPU — le tout en une passe sur la
//!   carte graphique. Mesuré sur une mire PQ 1000 nits : bande de 5 nits 61 → 48,
//!   bande de 30 nits 98 → 109, crête 190 → 255.
//! - `zscale` + `tonemap` (CPU, libzimg) : **repli** quand Vulkan n'est pas
//!   disponible (pilote absent, session distante, mémoire GPU insuffisante). Moins
//!   bon sur le gamut et plus lent, mais il existe — et un repli qui existe vaut
//!   mieux qu'une image lavée.
//!
//! **Ce que le module ne fait pas, et pourquoi le dire** : la fenêtre est rendue par
//! egui en **RGBA 8 bits SDR**. Il n'y a donc pas de « passthrough HDR » possible
//! aujourd'hui : même sur un écran HDR, la sortie reste SDR. `Mode::Jamais` existe
//! pour comparer, et le jour où une surface 10 bits HDR existera, c'est ce chemin
//! qu'il faudra brancher — les détections et le module sont déjà là.
//!
//! Toutes les décisions sont des **fonctions pures** (`detecter`, `plan`), testables
//! sans GPU ni fichier : le banc `tools/verif-hdr.sh` mesure ensuite les pixels.

use crate::config::{Dither, HdrMode, HdrSettings, Tonemap};

/// Transfert (courbe électro-optique) du flux, tel que le conteneur l'annonce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transfert {
    /// BT.709 / BT.601 (SDR).
    Sdr,
    /// `smpte2084` : PQ, le transfert du HDR10 et du Dolby Vision.
    Pq,
    /// `arib-std-b67` : HLG, le transfert des diffusions (TNT, flux TV).
    Hlg,
    /// Transfert annoncé mais inconnu de ce module (ex. `log100`).
    Autre(String),
}

impl Transfert {
    pub fn libelle(&self) -> &str {
        match self {
            Transfert::Sdr => "SDR",
            Transfert::Pq => "PQ (HDR10)",
            Transfert::Hlg => "HLG",
            Transfert::Autre(nom) => nom,
        }
    }
    pub fn est_hdr(&self) -> bool {
        matches!(self, Transfert::Pq | Transfert::Hlg)
    }
}

/// Primaires de couleur du flux.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Primaires {
    Bt709,
    /// BT.2020 : gamut large du HDR ; lu comme SDR, il donne des couleurs ternes.
    Bt2020,
    Autre(String),
}

impl Primaires {
    pub fn libelle(&self) -> &str {
        match self {
            Primaires::Bt709 => "BT.709",
            Primaires::Bt2020 => "BT.2020",
            Primaires::Autre(nom) => nom,
        }
    }
}

/// Ce que le flux annonce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    pub transfert: Transfert,
    pub primaires: Primaires,
    /// Profondeur par composante, lue sur le format de pixel (8, 10, 12…).
    pub bits: u8,
}

impl Info {
    /// Le flux est-il HDR ? **PQ ou HLG, ou primaires BT.2020** : un encodeur qui
    /// oublie le transfert mais annonce BT.2020 produit quand même une image
    /// fausse en SDR, donc on le traite comme HDR.
    pub fn est_hdr(&self) -> bool {
        self.transfert.est_hdr() || self.primaires == Primaires::Bt2020
    }

    /// Résumé lisible, pour le journal, l'interface et le banc.
    ///
    /// En SDR, le transfert n'est pas répété (« SDR — SDR / … » n'apprend rien) :
    /// seules les primaires et la profondeur sont utiles à lire.
    pub fn resume(&self) -> String {
        if self.est_hdr() {
            format!(
                "HDR — {} / {} / {} bits",
                self.transfert.libelle(),
                self.primaires.libelle(),
                self.bits
            )
        } else {
            format!("SDR — {} / {} bits", self.primaires.libelle(), self.bits)
        }
    }
}

/// Noms FFmpeg des transferts (ceux qu'acceptent `zscale` et `libplacebo`).
pub fn nom_transfert(t: &Transfert) -> &str {
    match t {
        Transfert::Sdr => "bt709",
        Transfert::Pq => "smpte2084",
        Transfert::Hlg => "arib-std-b67",
        Transfert::Autre(_) => "bt709",
    }
}

pub fn nom_primaires(p: &Primaires) -> &str {
    match p {
        Primaires::Bt709 => "bt709",
        Primaires::Bt2020 => "bt2020",
        Primaires::Autre(_) => "bt709",
    }
}

/// Ce que la chaîne va réellement faire — décidé une fois, journalisé, et
/// vérifiable par le banc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Étage à insérer **avant** les filtres d'image (chaîne avfilter), vide s'il
    /// n'y a rien à faire.
    pub etage: String,
    /// Phrase d'explication (journal, interface) : ce qui est appliqué et pourquoi.
    pub explication: String,
    /// Moteur retenu, pour l'affichage et le diagnostic.
    pub moteur: Moteur,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moteur {
    /// Rien à faire : le flux est SDR et le tramage ne s'applique pas.
    Aucun,
    /// `libplacebo` (Vulkan).
    Gpu,
    /// `zscale` + `tonemap` (CPU).
    Cpu,
}

impl Moteur {
    pub fn libelle(&self) -> &'static str {
        match self {
            Moteur::Aucun => "aucun",
            Moteur::Gpu => "libplacebo (GPU, Vulkan)",
            Moteur::Cpu => "zscale + tonemap (CPU)",
        }
    }
}

/// Variable d'environnement qui force le moteur (bancs, diagnostic) :
/// `gpu`, `cpu` ou `aucun`.
pub const VAR_MOTEUR: &str = "RMPLAYER_HDR_MOTEUR";

/// Moteur forcé par l'environnement, s'il y en a un.
pub fn moteur_force() -> Option<&'static str> {
    match std::env::var(VAR_MOTEUR).ok().as_deref().map(str::trim) {
        Some("gpu") => Some("gpu"),
        Some("cpu") => Some("cpu"),
        Some("aucun") => Some("aucun"),
        _ => None,
    }
}

/// Débit maximal (en mégapixels par seconde) au-delà duquel le **repli CPU** de l'étage HDR
/// est refusé : 1920 × 1080 × 30.
///
/// **Mesuré** (`docs/HDR.md`, 2026-10-24, Ryzen 7 5800X) : `zscale` + `tonemap` tient
/// 114 i/s en 1080p (237 Mpx/s) et seulement 26 i/s en 4K (216 Mpx/s), en occupant 9 à
/// 10 cœurs sur 16. Le budget retenu est environ **26 %** de cette capacité : il laisse
/// le reste du processeur au décodage, à l'audio et à l'interface. Un 4K à 24 i/s
/// (199 Mpx/s) serait à 92 % de la capacité : lecture saccadée, pire qu'une image non
/// convertie. Accepté : 1080p30, 720p60, tout ce qui est plus petit. Refusé : 1080p60,
/// 1440p24, 4K.
pub const BUDGET_REPLI_CPU_MPX_S: f64 = 1920.0 * 1080.0 * 30.0 / 1_000_000.0;

/// Le moteur CPU peut-il remplacer le GPU pour cette source ? `Err` donne la raison, en
/// clair, pour le journal et l'interface.
///
/// La cadence inconnue (≤ 1 i/s, valeur absente) est prise à 30 i/s : on ne refuse pas
/// un 1080p parce qu'on ignore sa cadence, mais un 4K reste refusé.
pub fn repli_cpu_possible(largeur: i32, hauteur: i32, fps: f32) -> Result<(), String> {
    let fps = if fps > 1.0 { fps as f64 } else { 30.0 };
    let debit = largeur.max(0) as f64 * hauteur.max(0) as f64 * fps / 1_000_000.0;
    if debit <= BUDGET_REPLI_CPU_MPX_S {
        Ok(())
    } else {
        Err(format!(
            "source {}x{} à {:.0} i/s = {:.0} Mpx/s, au-delà des {:.0} Mpx/s que le CPU tient \
             (1080p30)",
            largeur, hauteur, fps, debit, BUDGET_REPLI_CPU_MPX_S
        ))
    }
}

/// Ce que l'environnement permet, indépendamment du flux.
///
/// **Trois capacités distinctes, souvent confondues** :
/// 1. **l'écran** est HDR (annoncé par son EDID, cf. `crate::edid`) ;
/// 2. **le compositeur** sait transporter des couleurs étendues (Wayland
///    `wp_color_manager_v1` ; KWin 6.8 l'expose) ;
/// 3. **notre fenêtre** sait en produire — et c'est le point bloquant aujourd'hui.
///
/// Un passthrough HDR exige les trois. La détection porte sur (1) et (3) ; (2) est
/// vrai sur ce poste mais inutilisable sans (3), et n'est donc pas interrogé.
#[derive(Debug, Clone, Default)]
pub struct Contexte {
    /// Capacité de **chaque écran allumé** (EDID). Vide si aucun n'est lisible.
    pub ecrans: Vec<(String, crate::edid::Capacite)>,
    /// Notre chemin de rendu peut-il produire du HDR (10 bits + PQ) ?
    pub sortie_hdr: bool,
    /// Pourquoi la sortie ne le peut pas, quand c'est le cas.
    pub raison_sortie: String,
}

impl Contexte {
    /// Au moins un écran est-il HDR ?
    pub fn au_moins_un_ecran_hdr(&self) -> bool {
        self.ecrans.iter().any(|(_, c)| c.hdr())
    }

    /// **Tous** les écrans allumés sont-ils HDR ?
    ///
    /// C'est la condition retenue pour un passthrough : le client Wayland ne sait pas
    /// sur quel écran sa fenêtre est affichée, donc tant qu'un seul écran SDR est
    /// branché, envoyer du HDR reviendrait à parier. Prudence assumée, et écrite ici.
    pub fn ecran_hdr(&self) -> bool {
        !self.ecrans.is_empty() && self.ecrans.iter().all(|(_, c)| c.hdr())
    }

    /// Résumé lisible pour le journal et l'interface.
    pub fn resume(&self) -> String {
        let ecran = if self.ecrans.is_empty() {
            "écran : capacité inconnue (EDID illisible)".to_string()
        } else {
            let details: Vec<String> = self
                .ecrans
                .iter()
                .map(|(nom, c)| format!("{} ({})", nom, c.resume()))
                .collect();
            format!(
                "écrans : {} → {}",
                details.join(", "),
                if self.ecran_hdr() {
                    "tous HDR".to_string()
                } else if self.au_moins_un_ecran_hdr() {
                    // Cas piégeux : un HDR et un SDR branchés. Le passthrough reste
                    // refusé (on ne sait pas où est la fenêtre), et le résumé le dit.
                    "au moins un HDR, mais pas tous".to_string()
                } else {
                    "SDR".to_string()
                }
            )
        };
        let sortie = if self.sortie_hdr {
            "sortie : HDR possible".to_string()
        } else if self.raison_sortie.is_empty() {
            "sortie : SDR".to_string()
        } else {
            format!("sortie : SDR ({})", self.raison_sortie)
        };
        format!("{} ; {}", ecran, sortie)
    }

    /// Le passthrough HDR est-il possible (écran **et** sortie) ?
    pub fn passthrough_possible(&self) -> bool {
        self.ecran_hdr() && self.sortie_hdr
    }
}

/// Détecte ce que l'environnement permet : écran (EDID) et capacité de la sortie.
///
/// `sortie_hdr` est **toujours faux aujourd'hui**, et la raison est écrite dans
/// `RAISON_SORTIE_SDR` : ce n'est pas un renoncement, c'est une limite mesurée de la
/// chaîne de rendu (voir la constante).
pub fn contexte() -> Contexte {
    let ecrans = crate::edid::detecter();
    Contexte {
        ecrans,
        sortie_hdr: false,
        raison_sortie: RAISON_SORTIE_SDR.to_string(),
    }
}

/// Pourquoi notre fenêtre ne peut pas produire de HDR aujourd'hui.
///
/// Vérifié dans les sources des dépendances (session 35) : **winit 0.30 n'expose
/// aucune API de gestion des couleurs** — ni choix d'espace colorimétrique, ni format
/// de surface 10 bits — et egui/eframe 0.33 n'a qu'un réglage de tramage. La chaîne
/// de rendu produit donc une texture RGBA 8 bits en sRGB, quel que soit l'écran.
///
/// Le compositeur, lui, est prêt : ce poste expose `wp_color_manager_v1` v3 et
/// `zwp_linux_dmabuf_v1` avec le format `ABGR2101010`. La limitation est donc dans
/// la bibliothèque de fenêtrage, pas dans le matériel ni dans le bureau.
pub const RAISON_SORTIE_SDR: &str = "la fenêtre est rendue en RGBA 8 bits (winit/egui \
sans API de couleurs) ; cf. docs/HDR.md";

/// Décide de l'étage à insérer.
///
/// `info` : métadonnées du flux (`None` si le média n'est pas encore ouvert).
/// `hdr` : réglages de l'utilisateur.
///
/// Trois questions, dans cet ordre :
/// 1. **Faut-il convertir ?** Oui si le mode l'impose (`Toujours`), ou si le flux
///    est HDR et que le mode ne l'interdit pas (`Auto`). `Jamais` ne convertit
///    rien — utile pour comparer, ou pour une sortie déjà HDR.
/// 2. **Faut-il seulement tramer ?** Sur une source 10 bits SDR, descendre en
///    8 bits sans tramage fait apparaître des marches dans les dégradés : c'est le
///    seul intérêt de l'étage dans ce cas, et il n'exige pas de conversion tonale.
/// 3. **Quel moteur ?** GPU par défaut pour une conversion (qualité du gamut et de
///    la détection de crête) ; CPU en repli. Pour un simple tramage, libplacebo
///    reste le meilleur (diffusion d'erreur) mais coûte un transfert GPU : le CPU
///    est préféré si la source est petite… non — mesuré, le transfert est négligeable
///    devant l'encodage/le décodage. On ne devine pas : GPU si disponible.
#[cfg(test)]
pub fn plan(info: Option<&Info>, hdr: &HdrSettings) -> Plan {
    // Façade de test : contexte neutre (écran inconnu, sortie SDR). Le chemin réel
    // passe par `plan_avec`, avec la détection d'écran.
    // Moteur non forcé : les tests ne lisent jamais l'environnement du processus (ils
    // tournent en parallèle ; poser `RMPLAYER_HDR_MOTEUR` dans l'un faisait échouer les autres).
    plan_force(info, hdr, &Contexte::default(), None)
}

/// Comme `plan`, en tenant compte de **l'écran et de la sortie**.
///
/// Là se trouve le branchement demandé : le mode `Auto` ne se contente pas de
/// regarder le flux, il regarde aussi ce que l'affichage peut recevoir.
pub fn plan_avec(info: Option<&Info>, hdr: &HdrSettings, ctx: &Contexte) -> Plan {
    plan_force(info, hdr, ctx, moteur_force())
}

/// Le plan, avec le moteur forcé donné explicitement (`gpu`, `cpu`, `aucun`) au lieu d'être
/// lu dans l'environnement : c'est ce qui rend les tests déterministes.
pub fn plan_force(
    info: Option<&Info>,
    hdr: &HdrSettings,
    ctx: &Contexte,
    force: Option<&str>,
) -> Plan {
    let Some(info) = info else {
        return Plan {
            etage: String::new(),
            explication: "aucun média ouvert".to_string(),
            moteur: Moteur::Aucun,
        };
    };

    // « Jamais » veut dire **rien du tout** : pas de conversion, pas de tramage, pas
    // de débanding. C'est le réglage de comparaison (« à quoi ressemble l'image
    // brute ? »), et un étage de tramage y ajouterait une modification alors que
    // l'utilisateur a demandé l'image telle quelle.
    if hdr.mode == HdrMode::Jamais {
        return Plan {
            etage: String::new(),
            explication: format!(
                "{} : aucun traitement HDR (mode « jamais ») — image telle quelle",
                info.resume()
            ),
            moteur: Moteur::Aucun,
        };
    }
    // **Le branchement par détection.** En mode automatique, trois situations :
    //
    // a. écran HDR **et** sortie capable → **passthrough** : on ne convertit rien, le
    //    HDR part tel quel vers l'affichage (c'est le cas que ce module prépare) ;
    // b. écran HDR mais sortie SDR → conversion tonale, et l'explication dit **pourquoi**
    //    (limite de la fenêtre, pas de l'écran) ;
    // c. écran SDR → conversion tonale si le flux est HDR (comportement historique).
    //
    // Aujourd'hui (b) est toujours le cas, parce qu'aucune sortie HDR n'existe dans la
    // chaîne de rendu. Le branchement est néanmoins écrit et testé : le jour où la
    // fenêtre saura produire du 10 bits, il n'y aura qu'à renseigner `sortie_hdr`.
    let passthrough = hdr.mode == HdrMode::Auto && info.est_hdr() && ctx.passthrough_possible();
    if passthrough {
        return Plan {
            etage: String::new(),
            explication: format!(
                "{} : passthrough HDR — écran {} et sortie capables, aucune conversion",
                info.resume(),
                ctx.ecrans
                    .first()
                    .map(|(n, _)| n.as_str())
                    .unwrap_or("HDR")
            ),
            moteur: Moteur::Aucun,
        };
    }
    let convertir = match hdr.mode {
        HdrMode::Jamais => false, // traité ci-dessus
        HdrMode::Toujours => true,
        HdrMode::Auto => info.est_hdr(),
    };
    let tramer = hdr.dither != Dither::Aucun && info.bits > 8;
    let debander = hdr.gpu_deband && (convertir || info.bits >= 8);

    if !convertir && !tramer && !debander {
        let explication = if info.est_hdr() {
            format!(
                "{} : conversion tonale désactivée (mode « jamais ») — image telle quelle",
                info.resume()
            )
        } else {
            format!("{} : aucun étage à insérer", info.resume())
        };
        return Plan {
            etage: String::new(),
            explication,
            moteur: Moteur::Aucun,
        };
    }

    if force == Some("aucun") {
        return Plan {
            etage: String::new(),
            explication: format!("{} : étage HDR désactivé par {}", info.resume(), VAR_MOTEUR),
            moteur: Moteur::Aucun,
        };
    }

    let moteur = if force == Some("cpu") {
        Moteur::Cpu
    } else {
        Moteur::Gpu
    };

    let etage = match moteur {
        Moteur::Gpu => filtre_libplacebo(hdr, convertir, tramer, debander),
        Moteur::Cpu => filtre_cpu(info, hdr, convertir, tramer),
        Moteur::Aucun => String::new(),
    };

    let mut quoi = Vec::new();
    if convertir {
        quoi.push(format!("conversion tonale {}", hdr.tonemap.libelle()));
    }
    if tramer {
        quoi.push(format!(
            "tramage {} ({} → 8 bits)",
            hdr.dither.libelle(),
            info.bits
        ));
    }
    if debander {
        quoi.push("débanding GPU".to_string());
    }
    let mut explication = format!("{} : {}", info.resume(), quoi.join(", "));
    // Quand la conversion a lieu **parce que l'affichage est limité**, le dire : c'est
    // la question la plus fréquente (« pourquoi mon écran HDR n'a-t-il pas de HDR ? »)
    // et la réponse n'est pas dans le réglage, elle est dans la chaîne de rendu.
    if convertir && info.est_hdr() {
        if ctx.ecran_hdr() {
            explication.push_str(&format!(
                " — écran HDR détecté, mais {}",
                if ctx.raison_sortie.is_empty() {
                    "la sortie est SDR".to_string()
                } else {
                    ctx.raison_sortie.clone()
                }
            ));
        } else if !ctx.ecrans.is_empty() {
            explication.push_str(" — écran SDR (EDID)");
        }
    }
    Plan {
        etage,
        explication,
        moteur,
    }
}

/// Étage `libplacebo`.
///
/// Les paramètres de **sortie** sont explicites (BT.709, TV range, 8 bits) : sans
/// eux, libplacebo conserve le transfert et la conversion tonale ne s'applique pas —
/// mesuré : une mire PQ 1000 nits restait à 190/255 au lieu de monter à 255.
fn filtre_libplacebo(
    hdr: &HdrSettings,
    convertir: bool,
    tramer: bool,
    debander: bool,
) -> String {
    let mut opts: Vec<String> = Vec::new();
    // **Piège capital** : dans `vf_libplacebo`, `colorspace`, `color_primaries`,
    // `color_trc` et `range` désignent la **SORTIE**, pas l'entrée. Les renseigner
    // avec les valeurs de la source revient à demander « convertis PQ → PQ » : la
    // chaîne est acceptée, aucune conversion n'a lieu, et l'image reste lavée —
    // mesuré (5 nits à 61,2 au lieu de 48, aucun message d'erreur).
    //
    // L'entrée, elle, vient des **métadonnées de la frame** : notre moteur les
    // recopie depuis le flux avant d'envoyer l'image au graphe (voir
    // `player::poser_metadonnees_couleur`), pour ne pas dépendre de ce que le
    // décodeur a bien voulu propager. C'est pourquoi cette fonction n'a pas besoin
    // des métadonnées du flux en argument.
    opts.push("color_trc=bt709".to_string());
    opts.push("color_primaries=bt709".to_string());
    opts.push("colorspace=bt709".to_string());
    opts.push("range=tv".to_string());
    opts.push(format!("format=yuv420p"));

    if convertir {
        opts.push(format!("tonemapping={}", hdr.tonemap.nom_libplacebo()));
        if hdr.tonemap != Tonemap::Clip {
            opts.push(format!("contrast_recovery={:.2}", hdr.contrast_recovery.clamp(0.0, 1.0)));
        }
        opts.push(format!("gamut_mode={}", hdr.gamut.nom_libplacebo()));
        opts.push(format!("peak_detect={}", if hdr.peak_detect { "true" } else { "false" }));
    } else {
        // Sans conversion tonale, on ne veut surtout pas que libplacebo en fasse une.
        opts.push("tonemapping=clip".to_string());
    }
    if tramer {
        opts.push(format!("dithering={}", hdr.dither.nom_libplacebo()));
    } else {
        opts.push("dithering=none".to_string());
    }
    if debander {
        opts.push("deband=true".to_string());
        opts.push(format!("deband_radius={:.0}", hdr.gpu_deband_radius.clamp(4.0, 64.0)));
        opts.push(format!(
            "deband_threshold={:.1}",
            (hdr.gpu_deband_threshold * 100.0).clamp(0.0, 100.0)
        ));
        opts.push(format!("deband_grain={:.0}", hdr.gpu_deband_grain.clamp(0.0, 64.0)));
    }
    format!("libplacebo={}", opts.join(":"))
}

/// Étage de repli, sur le CPU : `zscale` (linéarisation) puis `tonemap`.
///
/// **Trois pièges, tous trouvés en mesurant** (une chaîne acceptée peut être fausse) :
///
/// 1. `zscale` exige **toutes** les caractéristiques d'entrée (`tin`, `pin`, `min`,
///    `rin`) : sans elles il répond « no path between colorspaces ».
/// 2. `tonemap` travaille sur du **RGB flottant**, pas sur du YUV : il faut donc un
///    `format=gbrpf32le` entre les deux `zscale`, sinon la sortie est sombre et fausse
///    (mesuré : 17/36/163 au lieu de 44/90/244).
/// 3. Les deux `zscale` n'ont pas le même `npl` : l'entrée linéarise avec **1,0 =
///    blanc SDR (100 nits)** — c'est l'unité qu'attend `tonemap` — et `peak` annonce
///    alors le pic de la source en nits. Avec 1,0 = 1000 nits des deux côtés, le
///    résultat est écrasé (17/36/163).
///
/// Comparaison mesurée sur une mire PQ (5 / 30 / 1000 nits) : GPU 47/107/253,
/// repli CPU 44/90/244. Le repli est un peu plus terne (Hable contre BT.2390) mais
/// du même ordre — il fait son travail.
fn filtre_cpu(info: &Info, hdr: &HdrSettings, convertir: bool, tramer: bool) -> String {
    let _ = tramer; // le CPU ne trame pas : le plan l'annonce séparément
    let entree = nom_transfert(&info.transfert);
    let primaires = nom_primaires(&info.primaires);
    let matrice = if info.primaires == Primaires::Bt2020 {
        "bt2020nc"
    } else {
        "bt709"
    };
    if !convertir {
        // Sans conversion tonale : simple passage en 8 bits.
        return "format=yuv420p".to_string();
    }
    let mut morceaux: Vec<String> = Vec::new();
    // 1) Linéarisation, 1,0 = blanc SDR (100 nits) : l'unité de `tonemap`.
    morceaux.push(format!(
        "zscale=t=linear:tin={}:pin={}:min={}:rin=limited:npl=100",
        entree, primaires, matrice
    ));
    // 2) RGB flottant : `tonemap` refuse le YUV.
    morceaux.push("format=gbrpf32le".to_string());
    // 3) Conversion tonale. `peak` = pic de la source en nits (réglable).
    morceaux.push(format!(
        "tonemap=tonemap={}:desat={:.2}:peak={:.0}",
        hdr.tonemap.nom_ffmpeg(),
        if hdr.tonemap == Tonemap::Clip { 0.0 } else { 2.0 },
        hdr.pic_nits.clamp(100.0, 10000.0)
    ));
    // 4) Retour en YUV BT.709 8 bits, toujours avec 1,0 = 100 nits.
    morceaux.push(
        "zscale=t=bt709:p=bt709:m=bt709:tin=linear:min=gbr:npl=100:r=limited".to_string(),
    );
    morceaux.push("format=yuv420p".to_string());
    morceaux.join(",")
}

/// Lit les métadonnées de couleur d'un flux, telles que FFmpeg les expose.
///
/// Séparé de FFmpeg pour être **testable** : les valeurs sont celles des
/// énumérations `AVColorTransferCharacteristic` / `AVColorPrimaries`.
pub fn detecter(trc: i32, primaires: i32, bits: u8) -> Info {
    // AVCOL_TRC : 1 = bt709, 16 = smpte2084 (PQ), 18 = arib-std-b67 (HLG).
    let transfert = match trc {
        16 => Transfert::Pq,
        18 => Transfert::Hlg,
        0 | 1 | 2 | 4 | 5 | 6 | 7 | 13 | 14 | 15 | 17 => Transfert::Sdr,
        autre => Transfert::Autre(nom_transfert_inconnu(autre)),
    };
    // AVCOL_PRI : 2 = non précisé (le cas le plus fréquent sur un fichier SDR
    // ancien), 1 = bt709, 9 = bt2020. « Non précisé » **n'est pas** BT.709 : on le
    // dit tel quel plutôt que d'inventer, mais on ne le traite pas comme du HDR.
    let primaires = match primaires {
        9 => Primaires::Bt2020,
        2 => Primaires::Autre("non précisé".to_string()),
        0 | 1 | 4 | 5 | 6 | 7 | 8 => Primaires::Bt709,
        autre => Primaires::Autre(nom_primaires_inconnu(autre)),
    };
    Info {
        transfert,
        primaires,
        bits,
    }
}

fn nom_transfert_inconnu(code: i32) -> String {
    match code {
        8 => "linear".to_string(),
        11 => "iec61966-2-1".to_string(),
        12 => "bt2020-10".to_string(),
        17 => "smpte428".to_string(),
        _ => format!("trc#{}", code),
    }
}

fn nom_primaires_inconnu(code: i32) -> String {
    match code {
        10 => "smpte428".to_string(),
        11 => "smpte431".to_string(),
        12 => "smpte432".to_string(),
        _ => format!("prim#{}", code),
    }
}

/// Profondeur par composante d'un format de pixel FFmpeg (par son nom).
///
/// « yuv420p » → 8, « yuv420p10le » → 10, « p010le » → 10, « gbrpf32le » → 32,
/// « rgb24 » → 8. Sert à savoir si un tramage a un sens.
pub fn bits_de_format(nom: &str) -> u8 {
    let nom = nom.to_ascii_lowercase();
    if nom.contains("p010") || nom.contains("p016") {
        return 10;
    }
    // Les formats 10/12/14 bits portent leur profondeur juste avant `le`/`be`.
    for (motif, bits) in [("10le", 10u8), ("10be", 10), ("12le", 12), ("12be", 12),
                          ("14le", 14), ("14be", 14), ("9le", 9), ("9be", 9)] {
        if nom.contains(motif) {
            return bits;
        }
    }
    if nom.contains("f32") || nom.contains("f64") {
        return 32;
    }
    if nom.contains("16") {
        return 16;
    }
    8
}

/// Ouvre un fichier **sans le décoder** pour lire ses métadonnées de couleur.
///
/// Sert au chemin `--hdr-info` : c'est la partie du module qu'on peut vérifier sans
/// GPU et sans fenêtre. Le décodage, lui, se mesure avec `--dump-frame`.
pub fn inspecter_fichier(chemin: &std::path::Path) -> Result<Info, String> {
    use std::ffi::CString;
    let nom = CString::new(chemin.to_string_lossy().as_bytes())
        .map_err(|_| "chemin invalide".to_string())?;
    unsafe {
        let mut format: *mut ffmpeg_sys_next::AVFormatContext = std::ptr::null_mut();
        if ffmpeg_sys_next::avformat_open_input(
            &mut format,
            nom.as_ptr(),
            std::ptr::null(),
            std::ptr::null_mut(),
        ) < 0
        {
            return Err(format!("ouverture impossible : {}", chemin.display()));
        }
        if ffmpeg_sys_next::avformat_find_stream_info(format, std::ptr::null_mut()) < 0 {
            ffmpeg_sys_next::avformat_close_input(&mut format);
            return Err("métadonnées illisibles".to_string());
        }
        let index = ffmpeg_sys_next::av_find_best_stream(
            format,
            ffmpeg_sys_next::AVMediaType::AVMEDIA_TYPE_VIDEO,
            -1,
            -1,
            std::ptr::null_mut(),
            0,
        );
        if index < 0 {
            ffmpeg_sys_next::avformat_close_input(&mut format);
            return Err("aucun flux vidéo".to_string());
        }
        let flux = *(*format).streams.add(index as usize);
        let par = (*flux).codecpar;
        let format_pixel: ffmpeg_sys_next::AVPixelFormat =
            std::mem::transmute::<i32, ffmpeg_sys_next::AVPixelFormat>((*par).format);
        let nom_format = ffmpeg_sys_next::av_get_pix_fmt_name(format_pixel);
        let nom_format = if nom_format.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(nom_format).to_string_lossy().to_string()
        };
        let info = detecter(
            (*par).color_trc as i32,
            (*par).color_primaries as i32,
            bits_de_format(&nom_format),
        );
        ffmpeg_sys_next::avformat_close_input(&mut format);
        Ok(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reglages() -> HdrSettings {
        HdrSettings::default()
    }

    #[test]
    fn la_detection_reconnait_pq_hlg_et_sdr() {
        // PQ + BT.2020 : HDR10
        let pq = detecter(16, 9, 10);
        assert_eq!(pq.transfert, Transfert::Pq);
        assert_eq!(pq.primaires, Primaires::Bt2020);
        assert!(pq.est_hdr());
        assert!(pq.resume().contains("HDR10"));

        // HLG + BT.2020 : diffusion
        let hlg = detecter(18, 9, 10);
        assert_eq!(hlg.transfert, Transfert::Hlg);
        assert!(hlg.est_hdr());

        // BT.709 : SDR, même en 10 bits
        let sdr = detecter(1, 1, 10);
        assert!(!sdr.est_hdr());
        assert!(sdr.resume().contains("SDR"));

        // BT.2020 annoncé sans transfert HDR : traité comme HDR (les couleurs
        // seraient fausses en SDR).
        let douteux = detecter(0, 9, 10);
        assert!(douteux.est_hdr());

        // Un transfert inconnu n'est pas pris pour du HDR.
        let inconnu = detecter(99, 1, 8);
        assert!(!inconnu.est_hdr());
        assert!(matches!(inconnu.transfert, Transfert::Autre(_)));
    }

    #[test]
    fn la_profondeur_se_lit_dans_le_nom_du_format() {
        assert_eq!(bits_de_format("yuv420p"), 8);
        assert_eq!(bits_de_format("yuv420p10le"), 10);
        assert_eq!(bits_de_format("yuv422p10le"), 10);
        assert_eq!(bits_de_format("p010le"), 10);
        assert_eq!(bits_de_format("p016le"), 10);
        assert_eq!(bits_de_format("yuv420p12le"), 12);
        assert_eq!(bits_de_format("gbrpf32le"), 32);
        assert_eq!(bits_de_format("rgb24"), 8);
        // Un format inconnu ne doit pas être annoncé 10 bits par accident.
        assert_eq!(bits_de_format("nv12"), 8);
    }

    #[test]
    fn le_plan_suit_le_mode_et_la_source() {
        let pq = detecter(16, 9, 10);
        let sdr = detecter(1, 1, 8);

        // Auto + HDR → conversion tonale par le GPU
        let p = plan(Some(&pq), &reglages());
        assert_eq!(p.moteur, Moteur::Gpu);
        assert!(p.etage.starts_with("libplacebo="));
        assert!(p.etage.contains("tonemapping="));
        assert!(p.explication.contains("conversion tonale"));

        // Auto + SDR 8 bits → rien du tout
        let p = plan(Some(&sdr), &reglages());
        assert_eq!(p.moteur, Moteur::Aucun);
        assert!(p.etage.is_empty());

        // Jamais + HDR → rien, et l'explication dit pourquoi
        let mut r = reglages();
        r.mode = HdrMode::Jamais;
        let p = plan(Some(&pq), &r);
        assert_eq!(p.moteur, Moteur::Aucun);
        assert!(p.etage.is_empty(), "mode jamais : aucun étage, même en 10 bits");
        assert!(p.explication.contains("jamais"));

        // Toujours + SDR → conversion quand même (utile pour comparer)
        r.mode = HdrMode::Toujours;
        let p = plan(Some(&sdr), &r);
        assert_eq!(p.moteur, Moteur::Gpu);
        assert!(p.etage.contains("tonemapping="));
    }

    #[test]
    fn le_plan_demande_une_sortie_sdr_et_pas_une_sortie_pq() {
        // **Piège mesuré** : `color_trc` de `vf_libplacebo` est la **sortie**. Y
        // mettre `smpte2084` (le transfert de la source) demande « PQ → PQ » : la
        // chaîne passe, aucune conversion n'a lieu, l'image reste lavée. Le test fige
        // donc la sortie BT.709 et interdit la valeur de la source.
        let pq = detecter(16, 9, 10);
        let p = plan(Some(&pq), &reglages());
        assert!(p.etage.contains("format=yuv420p"), "sortie 8 bits annoncée");
        assert!(p.etage.contains("color_trc=bt709"), "sortie SDR demandée : {}", p.etage);
        assert!(p.etage.contains("color_primaries=bt709"), "primaires de sortie BT.709");
        assert!(
            !p.etage.contains("color_trc=smpte2084"),
            "le transfert de la source ne doit pas être demandé en sortie"
        );
    }

    #[test]
    fn le_repli_cpu_produit_une_chaine_zscale_et_tonemap() {
        let pq = detecter(16, 9, 10);
        let mut r = reglages();
        r.mode = HdrMode::Toujours;
        // Moteur forcé : c'est le chemin du banc (`RMPLAYER_HDR_MOTEUR=cpu`), passé en paramètre.
        let p = plan_force(Some(&pq), &r, &Contexte::default(), Some("cpu"));
        assert_eq!(p.moteur, Moteur::Cpu);
        assert!(p.etage.contains("zscale="), "zscale présent : {}", p.etage);
        assert!(p.etage.contains("tonemap="), "tonemap présent : {}", p.etage);
        // zscale exige l'annonce complète de l'entrée, sinon il refuse la chaîne.
        assert!(p.etage.contains("tin=smpte2084"), "transfert d'entrée annoncé");
        assert!(p.etage.contains("pin=bt2020"), "primaires d'entrée annoncées");
    }

    #[test]
    fn le_tramage_ne_s_active_que_sur_plus_de_8_bits() {
        let huit = detecter(1, 1, 8);
        let dix = detecter(1, 1, 10);
        let mut r = reglages();
        r.dither = Dither::BruitBleu;
        // 8 bits SDR : rien à tramer, rien à faire.
        assert_eq!(plan(Some(&huit), &r).moteur, Moteur::Aucun);
        // 10 bits SDR : un étage de tramage, sans conversion tonale.
        let p = plan(Some(&dix), &r);
        assert_eq!(p.moteur, Moteur::Gpu);
        assert!(p.etage.contains("dithering=blue"));
        assert!(!p.etage.contains("tonemapping=bt.2390"), "aucune conversion tonale");
        assert!(p.etage.contains("tonemapping=clip"), "conversion explicitement désactivée");
    }

    #[test]
    fn le_debanding_gpu_s_ajoute_et_se_regle() {
        let pq = detecter(16, 9, 10);
        let mut r = reglages();
        r.gpu_deband = true;
        r.gpu_deband_radius = 20.0;
        r.gpu_deband_threshold = 0.05;
        let p = plan(Some(&pq), &r);
        assert!(p.etage.contains("deband=true"), "{}", p.etage);
        assert!(p.etage.contains("deband_radius=20"), "{}", p.etage);
        assert!(p.etage.contains("deband_threshold=5"), "{}", p.etage);
        assert!(p.explication.contains("débanding"));
        // Le débanding GPU remplace le débanding CPU (un seul passage suffit).
        assert!(!p.etage.contains("deband=range="), "pas de débanding CPU en plus");
    }

    #[test]
    fn chaque_operateur_de_conversion_a_un_nom_valide_partout() {
        // Un nom qui n'existe pas dans libplacebo ou dans tonemap fait échouer toute
        // la chaîne : on fige donc les correspondances.
        for t in Tonemap::CHOIX {
            assert!(!t.nom_libplacebo().is_empty());
            assert!(!t.nom_ffmpeg().is_empty());
            assert!(!t.libelle().is_empty());
        }
        // Les deux moteurs ne partagent pas les mêmes noms : `tonemap` de FFmpeg
        // n'a pas de bt.2390, libplacebo n'a pas `clip` sous ce nom.
        assert_eq!(Tonemap::Bt2390.nom_libplacebo(), "bt.2390");
        assert_eq!(Tonemap::Bt2390.nom_ffmpeg(), "hable");
        assert_eq!(Tonemap::Clip.nom_libplacebo(), "clip");
        assert_eq!(Tonemap::Clip.nom_ffmpeg(), "clip");
    }

    #[test]
    fn le_passthrough_exige_un_ecran_hdr_et_une_sortie_capable() {
        let pq = detecter(16, 9, 10);
        let mut r = reglages();
        r.mode = HdrMode::Auto;

        // Écran HDR + sortie capable → aucun étage (passthrough).
        let ctx = Contexte {
            ecrans: vec![(
                "card0-DP-3".to_string(),
                crate::edid::Capacite {
                    pq: true,
                    bt2020: true,
                    ..Default::default()
                },
            )],
            sortie_hdr: true,
            raison_sortie: String::new(),
        };
        let p = plan_avec(Some(&pq), &r, &ctx);
        assert_eq!(p.moteur, Moteur::Aucun);
        assert!(p.etage.is_empty(), "passthrough : aucun filtre");
        assert!(p.explication.contains("passthrough"));

        // Écran HDR mais sortie SDR (notre cas) → conversion, et l'explication dit
        // que le blocage vient de la sortie, pas de l'écran.
        let ctx_sdr = Contexte {
            ecrans: ctx.ecrans.clone(),
            sortie_hdr: false,
            raison_sortie: "fenêtre 8 bits".to_string(),
        };
        let p = plan_avec(Some(&pq), &r, &ctx_sdr);
        assert_eq!(p.moteur, Moteur::Gpu);
        assert!(p.etage.contains("tonemapping="));
        assert!(p.explication.contains("écran HDR détecté"), "{}", p.explication);
        assert!(p.explication.contains("fenêtre 8 bits"), "{}", p.explication);

        // Écran SDR + sortie capable → on ne fait pas de passthrough pour autant :
        // un écran qui n'annonce pas HDR ne doit pas recevoir du PQ.
        let ctx_sdr_ecran = Contexte {
            ecrans: vec![("card0-DP-3".to_string(), crate::edid::Capacite::default())],
            sortie_hdr: true,
            raison_sortie: String::new(),
        };
        let p = plan_avec(Some(&pq), &r, &ctx_sdr_ecran);
        assert!(!p.etage.is_empty(), "conversion maintenue sur écran SDR");
        assert!(p.explication.contains("écran SDR"));
    }

    #[test]
    fn le_contexte_resume_les_trois_capacites() {
        let ctx = Contexte {
            ecrans: vec![("card0-DP-3".to_string(), crate::edid::Capacite::default())],
            sortie_hdr: false,
            raison_sortie: RAISON_SORTIE_SDR.to_string(),
        };
        let resume = ctx.resume();
        assert!(resume.contains("card0-DP-3"));
        assert!(resume.contains("SDR"));
        assert!(!ctx.ecran_hdr());
        assert!(!ctx.passthrough_possible());
        // Le contexte réel de la machine : écran lu ou inconnu, sortie toujours SDR.
        let reel = contexte();
        assert!(!reel.sortie_hdr, "aucune sortie HDR ne doit être revendiquée");
        assert!(!reel.raison_sortie.is_empty(), "la raison doit être écrite");
    }

    #[test]
    fn le_repli_cpu_suit_le_budget_mesure() {
        // Acceptés : 1080p30 (le budget lui-même), 720p60, 1080p24, petite mire.
        assert!(repli_cpu_possible(1920, 1080, 30.0).is_ok(), "1080p30 = le budget");
        assert!(repli_cpu_possible(1920, 1080, 24.0).is_ok());
        assert!(repli_cpu_possible(1280, 720, 60.0).is_ok());
        assert!(repli_cpu_possible(640, 360, 5.0).is_ok());
        // Refusés : 1080p60, 1440p24, 4K à 24 et 60 i/s.
        assert!(repli_cpu_possible(1920, 1080, 60.0).is_err());
        assert!(repli_cpu_possible(2560, 1440, 24.0).is_err());
        assert!(repli_cpu_possible(3840, 2160, 24.0).is_err());
        assert!(repli_cpu_possible(3840, 2160, 60.0).is_err());
        // Cadence inconnue : prise à 30 i/s, donc 1080p accepté, 4K refusé.
        assert!(repli_cpu_possible(1920, 1080, 0.0).is_ok());
        assert!(repli_cpu_possible(3840, 2160, 0.0).is_err());
        // Dimensions absurdes : jamais de panique, et un débit nul est accepté.
        assert!(repli_cpu_possible(-5, 0, 24.0).is_ok());
    }

    #[test]
    fn le_refus_du_repli_cpu_dit_pourquoi() {
        let raison = repli_cpu_possible(3840, 2160, 24.0).expect_err("4K refusé");
        assert!(raison.contains("3840x2160"), "la source est nommée : {raison}");
        assert!(raison.contains("Mpx/s"), "le débit est chiffré : {raison}");
        assert!(raison.contains("1080p30"), "le budget est nommé : {raison}");
    }

    #[test]
    fn le_plan_cpu_force_donne_toujours_un_etage_zscale() {
        // Le repli de `player.rs` s'appuie sur ce plan : il doit exister même quand le plan
        // nominal est GPU, et ne jamais être vide pour un flux HDR converti.
        let pq = detecter(16, 9, 10);
        let gpu = plan(Some(&pq), &reglages());
        assert_eq!(gpu.moteur, Moteur::Gpu);
        let cpu = plan_force(Some(&pq), &reglages(), &Contexte::default(), Some("cpu"));
        assert_eq!(cpu.moteur, Moteur::Cpu);
        assert!(cpu.etage.starts_with("zscale="), "tête reconnue par decouper_etage_hdr");
    }

    #[test]
    fn le_mode_force_par_l_environnement_est_respecte() {
        let pq = detecter(16, 9, 10);
        let p = plan_force(Some(&pq), &reglages(), &Contexte::default(), Some("aucun"));
        assert_eq!(p.moteur, Moteur::Aucun);
        assert!(p.explication.contains(VAR_MOTEUR));
    }
}

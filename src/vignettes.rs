//! Vignettes de prévisualisation — « voir où l'on va » avant de cliquer.
//!
//! **Le besoin.** Survoler la barre de progression doit montrer l'image de l'instant
//! pointé : c'est ce qui permet de se déplacer dans un film sans avancer à l'aveugle.
//!
//! **Pourquoi un décodeur séparé, et pas le lecteur existant.** Le lecteur a un état
//! précieux : position, horloge audio, files de paquets, chaîne de filtres GPU. Y
//! demander une image « ailleurs » obligerait à faire un `seek` puis à revenir — la
//! lecture sauterait, l'horloge audio décrocherait, et le coût serait payé à chaque
//! pixel de survol. Ici, un **contexte FFmpeg autonome** ouvre le même fichier et
//! décode l'image demandée ; il ne partage rien avec la lecture en cours.
//!
//! **Trois choix de conception mesurables** :
//!
//! 1. **Un fil de travail**, pas le fil d'interface. Ouvrir un fichier et décoder
//!    prend des dizaines de millisecondes : le faire dans `update()` figerait la
//!    fenêtre à chaque nouvelle vignette (le projet a déjà payé ce prix, §4 piège 2).
//! 2. **Le décodeur reste ouvert** entre deux demandes du même fichier, et se
//!    referme quand le média change. Rouvrir à chaque pixel multiplierait le coût
//!    par le nombre d'événements de souris.
//! 3. **Les demandes sont regroupées par « tranche »** (une vignette par tranche de
//!    temps, pas par position exacte) et une demande en cours n'est jamais doublée :
//!    survoler vite la barre produit des dizaines d'événements pour la même visée.
//!
//! La largeur est bornée (`largeur_max`) : une vignette de 1080p coûterait 8 Mo de
//! texture pour 200 px affichés.

use std::ffi::CString;
use std::os::raw::c_int;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use ffmpeg_sys_next::*;

/// Une vignette décodée : pixels RGBA non prémultipliés, dimensionnés pour
/// l'affichage (largeur ≤ `largeur_max`, rapport d'aspect conservé).
#[derive(Clone)]
pub struct Vignette {
    pub largeur: u32,
    pub hauteur: u32,
    /// Position réellement décodée (celle de l'image, pas celle demandée : après un
    /// `seek`, la première image peut précéder la cible).
    pub position_ms: u64,
    /// `largeur × hauteur × 4` octets.
    pub rgba: Vec<u8>,
}

/// Décodeur autonome pour un fichier.
struct Decodeur {
    chemin: PathBuf,
    format: *mut AVFormatContext,
    codec: *mut AVCodecContext,
    index: c_int,
    /// Base de temps du flux (celle qu'attend `av_rescale_q`).
    base_tb: AVRational,
    /// Contexte de mise à l'échelle réutilisé entre deux images de même géométrie.
    sws: *mut SwsContext,
    sws_cle: (c_int, c_int, c_int, c_int, c_int),
    /// Image réutilisée entre deux décodages (allocation coûteuse).
    trame: *mut AVFrame,
    paquet: *mut AVPacket,
}

// Les pointeurs FFmpeg ne sont pas `Send` par nature, mais ce décodeur vit
// exclusivement dans son fil de travail : il n'est jamais partagé. L'assertion est
// donc sûre **tant que** `Decodeur` ne sort pas de ce fil (aucune API publique ne
// le rend).
unsafe impl Send for Decodeur {}

impl Decodeur {
    /// Ouvre le fichier et prépare le premier flux vidéo.
    fn ouvrir(chemin: &Path) -> Result<Self, String> {
        let nom = CString::new(chemin.to_string_lossy().as_bytes())
            .map_err(|_| "chemin invalide (octet nul)".to_string())?;
        unsafe {
            let mut format: *mut AVFormatContext = std::ptr::null_mut();
            if avformat_open_input(
                &mut format,
                nom.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
            ) < 0
            {
                return Err(format!("ouverture impossible : {}", chemin.display()));
            }
            if avformat_find_stream_info(format, std::ptr::null_mut()) < 0 {
                avformat_close_input(&mut format);
                return Err("aucun information de flux lisible".to_string());
            }
            let index = av_find_best_stream(
                format,
                AVMediaType::AVMEDIA_TYPE_VIDEO,
                -1,
                -1,
                std::ptr::null_mut(),
                0,
            );
            if index < 0 {
                avformat_close_input(&mut format);
                return Err("aucun flux vidéo dans ce fichier".to_string());
            }
            let flux = *(*format).streams.add(index as usize);
            let codec_param = (*flux).codecpar;
            let decodeur = avcodec_find_decoder((*codec_param).codec_id);
            if decodeur.is_null() {
                avformat_close_input(&mut format);
                return Err("décodeur absent pour ce format".to_string());
            }
            let mut codec = avcodec_alloc_context3(decodeur);
            if codec.is_null() || avcodec_parameters_to_context(codec, codec_param) < 0 {
                avcodec_free_context(&mut codec);
                avformat_close_input(&mut format);
                return Err("préparation du décodeur impossible".to_string());
            }
            // Décodage **logiciel** volontairement : un décodeur matériel partagerait
            // le périphérique VA-API avec la lecture en cours, avec des files et des
            // surfaces limitées. Une vignette est petite, le logiciel suffit — et il
            // fonctionne pour tous les fichiers, y compris ceux que le pilote refuse.
            (*codec).thread_count = 1;
            if avcodec_open2(codec, decodeur, std::ptr::null_mut()) < 0 {
                avcodec_free_context(&mut codec);
                avformat_close_input(&mut format);
                return Err("ouverture du décodeur impossible".to_string());
            }
            let base_tb = (*flux).time_base;
            let trame = av_frame_alloc();
            let paquet = av_packet_alloc();
            if trame.is_null() || paquet.is_null() {
                av_frame_free(&mut { trame });
                av_packet_free(&mut { paquet });
                avcodec_free_context(&mut codec);
                avformat_close_input(&mut format);
                return Err("allocation FFmpeg impossible".to_string());
            }
            Ok(Self {
                chemin: chemin.to_path_buf(),
                format,
                codec,
                index,
                base_tb,
                sws: std::ptr::null_mut(),
                sws_cle: (0, 0, 0, 0, 0),
                trame,
                paquet,
            })
        }
    }

    /// Décode l'image la plus proche de `position_ms` et la réduit.
    ///
    /// Le `seek` est fait en **avant** (`AVSEEK_FLAG_BACKWARD`) : on tombe sur
    /// l'image-clé précédente, puis on avance jusqu'à dépasser la cible. Aller en
    /// arrière depuis une image-clé suivante serait plus lent et moins précis.
    fn image_a(&mut self, position_ms: u64, largeur_max: u32) -> Result<Vignette, String> {
        unsafe {
            // **Même conversion que le lecteur** (`player::seek`) : les
            // microsecondes sont ramenées dans la base de temps du flux par
            // `av_rescale_q`, et le déplacement passe par `avformat_seek_file` avec
            // une fenêtre (min, cible, max), qui accepte une image-clé un peu
            // au-delà de la cible. Écrire soi-même `position / base` en flottant
            // était faux : dans ce fichier la base annoncée par le flux (1/25) ne
            // correspond pas à celle des paquets, et le déplacement atterrissait
            // hors du média — `av_read_frame` renvoyait alors EOF après un paquet
            // (mesuré : vignette de 0 ms pour une demande à 5 s).
            let cible = av_rescale_q(
                (position_ms as i64) * 1000,
                AVRational { num: 1, den: AV_TIME_BASE },
                self.base_tb,
            );
            if position_ms > 0 {
                let max_ts = cible + av_rescale_q(
                    500_000,
                    AVRational { num: 1, den: AV_TIME_BASE },
                    self.base_tb,
                );
                if avformat_seek_file(
                    self.format,
                    self.index,
                    i64::MIN,
                    cible,
                    max_ts,
                    AVSEEK_FLAG_BACKWARD,
                ) < 0
                {
                    return Err(format!("déplacement à {} ms impossible", position_ms));
                }
            }
            avcodec_flush_buffers(self.codec);
            // 240 images : à 60 i/s, cela couvre 4 s après l'image-clé. Au-delà, on
            // renvoie la dernière image décodée plutôt que d'échouer — une vignette
            // approchée vaut mieux qu'un cadre vide.
            let mut derniere: Option<Vignette> = None;
            for _ in 0..240 {
                if av_read_frame(self.format, self.paquet) < 0 {
                    break;
                }
                if (*self.paquet).stream_index != self.index {
                    av_packet_unref(self.paquet);
                    continue;
                }
                let envoye = avcodec_send_packet(self.codec, self.paquet);
                av_packet_unref(self.paquet);
                if envoye < 0 {
                    continue;
                }
                loop {
                    let recu = avcodec_receive_frame(self.codec, self.trame);
                    if recu < 0 {
                        break;
                    }
                    let pts = (*self.trame).pts;
                    let ms = if pts >= 0 {
                        av_rescale_q(pts, self.base_tb, AVRational { num: 1, den: 1000 })
                            .max(0) as u64
                    } else {
                        0
                    };
                    if let Some(v) = self.redimensionner(ms, largeur_max)? {
                        derniere = Some(v);
                    }
                    // On s'arrête dès qu'on a dépassé la cible : inutile de décoder
                    // la suite pour une vignette.
                    if pts >= 0 && ms + 1 >= position_ms {
                        return derniere.ok_or_else(|| "aucune image décodée".to_string());
                    }
                }
            }
            derniere.ok_or_else(|| format!("aucune image à {} ms", position_ms))
        }
    }

    /// Convertit la trame courante en RGBA réduit.
    ///
    /// Renvoie `Ok(None)` si la trame n'est pas exploitable (format inconnu) : ce
    /// n'est pas une erreur, l'appelant passe à la suivante.
    unsafe fn redimensionner(
        &mut self,
        position_ms: u64,
        largeur_max: u32,
    ) -> Result<Option<Vignette>, String> {
        let largeur_src = (*self.trame).width;
        let hauteur_src = (*self.trame).height;
        if largeur_src <= 0 || hauteur_src <= 0 {
            return Ok(None);
        }
        let format_src: AVPixelFormat =
            std::mem::transmute::<c_int, AVPixelFormat>((*self.trame).format);
        if format_src == AVPixelFormat::AV_PIX_FMT_NONE {
            return Ok(None);
        }
        // Largeur bornée, rapport d'aspect conservé, hauteur au moins 1 px.
        let largeur_max = largeur_max.max(32);
        let (largeur, hauteur) = if (largeur_src as u32) <= largeur_max {
            (largeur_src as u32, hauteur_src as u32)
        } else {
            let h = ((hauteur_src as f64) * (largeur_max as f64) / (largeur_src as f64))
                .round()
                .max(1.0) as u32;
            (largeur_max, h)
        };
        let cle = (largeur_src, hauteur_src, format_src as c_int, largeur as c_int, hauteur as c_int);
        if self.sws.is_null() || self.sws_cle != cle {
            if !self.sws.is_null() {
                sws_freeContext(self.sws);
            }
            self.sws = sws_getContext(
                largeur_src,
                hauteur_src,
                format_src,
                largeur as c_int,
                hauteur as c_int,
                AVPixelFormat::AV_PIX_FMT_RGBA,
                SWS_BILINEAR,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            self.sws_cle = cle;
        }
        if self.sws.is_null() {
            return Err("conversion d'image impossible".to_string());
        }
        let stride = (largeur * 4) as c_int;
        // **Marge de sécurité conservée** : les routines SIMD de libswscale écrivent
        // par blocs et peuvent dépasser la dernière ligne de quelques octets quand la
        // largeur n'est pas un multiple du bloc (200 px, par exemple). La marge est
        // inutilisée et `stride` ne change pas : la disposition des lignes est
        // identique. (La vraie avarie de la session 34 était ailleurs — voir
        // `player::seek` et le commentaire de `image_a` — mais l'écriture par blocs
        // reste réelle, et la marge ne coûte qu'une ligne.)
        let mut rgba = vec![0u8; (largeur * hauteur * 4) as usize + stride as usize + 64];
        let dst: [*mut u8; 4] = [
            rgba.as_mut_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ];
        let dst_stride: [c_int; 4] = [stride, 0, 0, 0];
        let lignes = sws_scale(
            self.sws,
            (*self.trame).data.as_ptr() as *const *const u8,
            (*self.trame).linesize.as_ptr(),
            0,
            hauteur_src,
            dst.as_ptr(),
            dst_stride.as_ptr(),
        );
        if lignes <= 0 {
            return Ok(None);
        }
        // Le tampon est plus grand que nécessaire : on le ramène à la taille exacte
        // de l'image avant de le rendre (une texture egui exige w×h×4).
        rgba.truncate((largeur * hauteur * 4) as usize);
        Ok(Some(Vignette {
            largeur,
            hauteur,
            position_ms,
            rgba,
        }))
    }
}

impl Drop for Decodeur {
    fn drop(&mut self) {
        unsafe {
            if !self.sws.is_null() {
                sws_freeContext(self.sws);
            }
            av_frame_free(&mut self.trame);
            av_packet_free(&mut self.paquet);
            avcodec_free_context(&mut self.codec);
            avformat_close_input(&mut self.format);
        }
    }
}

/// Extrait une vignette, sans fil de travail — **le chemin vérifiable**.
///
/// Ouvre le fichier, décode, referme. C'est ce que la ligne de commande utilise
/// (`--vignette`) et ce que le banc mesure : lent (une ouverture par appel), mais
/// sans état caché, donc reproductible. L'interface passe par `Generateur`.
pub fn extraire(chemin: &Path, position_ms: u64, largeur_max: u32) -> Result<Vignette, String> {
    let mut decodeur = Decodeur::ouvrir(chemin)?;
    decodeur.image_a(position_ms, largeur_max)
}

/// Demande adressée au fil de travail.
struct Demande {
    chemin: PathBuf,
    position_ms: u64,
    largeur_max: u32,
    /// Tranche demandée : sert à ne pas redemander la même chose.
    tranche: u64,
}

/// Générateur asynchrone : l'interface dépose une demande, lit les résultats prêts.
///
/// **Aucune attente dans le fil d'interface** : `demander()` ne fait qu'envoyer un
/// message, et `recuperer()` ne prend que ce qui est déjà là.
pub struct Generateur {
    envoi: Option<Sender<Demande>>,
    recus: Receiver<(u64, Result<Vignette, String>)>,
    /// Dernière tranche demandée et pas encore reçue : évite d'empiler dix demandes
    /// identiques quand la souris balaie la barre.
    en_attente: Option<u64>,
    /// Nombre de demandes envoyées (diagnostic et banc).
    envoyees: u64,
    /// Résultats reçus (diagnostic et banc).
    recues: u64,
}

impl Generateur {
    pub fn demarrer() -> Self {
        let (tx_demande, rx_demande) = mpsc::channel::<Demande>();
        let (tx_resultat, rx_resultat) = mpsc::channel();
        std::thread::Builder::new()
            .name("kplayer-vignettes".into())
            .spawn(move || {
                // Le décodeur survit d'une demande à l'autre **pour le même fichier**.
                let mut courant: Option<Decodeur> = None;
                for demande in rx_demande {
                    let meme_fichier = courant
                        .as_ref()
                        .is_some_and(|d| d.chemin == demande.chemin);
                    if !meme_fichier {
                        courant = Decodeur::ouvrir(&demande.chemin).ok();
                    }
                    let resultat = match courant.as_mut() {
                        Some(decodeur) => decodeur.image_a(demande.position_ms, demande.largeur_max),
                        None => Err(format!(
                            "vignette impossible pour {}",
                            demande.chemin.display()
                        )),
                    };
                    if tx_resultat.send((demande.tranche, resultat)).is_err() {
                        break;
                    }
                }
            })
            .ok();
        Self {
            envoi: Some(tx_demande),
            recus: rx_resultat,
            en_attente: None,
            envoyees: 0,
            recues: 0,
        }
    }

    /// Demande une vignette pour `position_ms`. Sans effet si la même tranche est
    /// déjà en cours — c'est le cas normal quand la souris balaie la barre.
    pub fn demander(&mut self, chemin: &Path, position_ms: u64, largeur_max: u32, tranche_ms: u64) {
        let tranche = if tranche_ms == 0 {
            position_ms
        } else {
            position_ms / tranche_ms
        };
        if self.en_attente == Some(tranche) {
            return;
        }
        let Some(envoi) = &self.envoi else {
            return;
        };
        let demande = Demande {
            chemin: chemin.to_path_buf(),
            position_ms,
            largeur_max,
            tranche,
        };
        if envoi.send(demande).is_ok() {
            self.en_attente = Some(tranche);
            self.envoyees += 1;
        }
    }

    /// Résultat déjà disponible, s'il y en a un. Jamais bloquant.
    pub fn recuperer(&mut self) -> Option<(u64, Result<Vignette, String>)> {
        match self.recus.try_recv() {
            Ok((tranche, resultat)) => {
                self.en_attente = None;
                self.recues += 1;
                Some((tranche, resultat))
            }
            Err(_) => None,
        }
    }

    /// Une vignette est-elle en cours de calcul ?
    pub fn en_cours(&self) -> bool {
        self.en_attente.is_some()
    }

    pub fn compteurs(&self) -> (u64, u64) {
        (self.envoyees, self.recues)
    }
}

/// Tranche par défaut : une vignette par seconde de média.
///
/// Plus fine, on décoderait cinquante images pour traverser la barre d'un film de
/// deux heures ; plus grossière, la vignette ne correspondrait plus au point visé.
pub const TRANCHE_MS: u64 = 1000;

/// Largeur par défaut d'une vignette : 320 px.
///
/// À 320×180, une vignette pèse 230 Ko en RGBA — la mémoire d'une texture reste
/// raisonnable, et le détail suffit pour reconnaître un plan.
pub const LARGEUR_DEFAUT: u32 = 320;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_largeur_est_bornee_et_le_rapport_conserve() {
        // Le calcul est dans `redimensionner`, qui exige une trame FFmpeg : on
        // vérifie ici la règle arithmétique qui compte (rapport d'aspect), sur les
        // mêmes valeurs que le code.
        for (l_src, h_src, maxi) in [(1920u32, 1080u32, 320u32), (3840, 2160, 320), (640, 480, 320)] {
            let (l, h) = if l_src <= maxi {
                (l_src, h_src)
            } else {
                let h = ((h_src as f64) * (maxi as f64) / (l_src as f64)).round().max(1.0) as u32;
                (maxi, h)
            };
            assert_eq!(l, maxi);
            let attendu = (l_src as f64 / h_src as f64) / (l as f64 / h as f64);
            assert!(
                (attendu - 1.0).abs() < 0.02,
                "rapport d'aspect faussé pour {}x{} → {}x{}",
                l_src,
                h_src,
                l,
                h
            );
        }
    }

    #[test]
    fn la_tranche_regroupe_les_demandes_voisines() {
        // Deux positions de la même seconde doivent tomber dans la même tranche :
        // c'est ce qui évite de décoder pour chaque pixel de survol.
        let t = TRANCHE_MS;
        assert_eq!(10_000 / t, 10_500 / t);
        assert_ne!(10_000 / t, 11_000 / t);
    }

    #[test]
    fn un_fichier_absent_echoue_proprement() {
        let err = extraire(Path::new("/tmp/inexistant-kplayer-vignette.mkv"), 0, 320)
            .err()
            .expect("doit échouer");
        assert!(
            err.contains("ouverture impossible") || err.contains("aucun"),
            "message inattendu : {}",
            err
        );
    }
}

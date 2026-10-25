//! Sortie audio — cpal (PipeWire / PulseAudio / ALSA)
//!
//! Architecture :
//!   - RMPlayer décode l'audio via FFmpeg et convertit en f32 entrelacé
//!     (au format **du périphérique**, via libswresample, côté `player.rs`)
//!   - Les échantillons sont poussés dans un tampon circulaire (ring buffer)
//!   - Un callback cpal les consomme et les écrit vers la carte son
//!
//! Principes de robustesse (importants) :
//!   1. **Le callback audio ne bloque jamais** : il utilise `try_lock` et, si le
//!      tampon est momentanément occupé, écrit du silence. Ainsi le thread audio
//!      ne peut jamais rester bloqué sur le thread d'interface (ce qui gelait
//!      l'application et faisait tuer la fenêtre par le gestionnaire de fenêtres).
//!   2. **Aucun appel de contrôle bloquant** (`play`/`pause`) depuis l'interface :
//!      ces appels peuvent attendre le thread audio et donc geler l'UI. On se
//!      contente d'un drapeau atomique ; le callback produit du silence à l'arrêt.
//!   3. Le flux est créé **une seule fois** et vit aussi longtemps que
//!      l'application : plus de création/destruction de flux à chaque média
//!      (source de blocages et d'erreurs ALSA/PipeWire).
//!
//! ## L'horloge audio
//!
//! Le callback compte les échantillons du média qu'il **sort réellement** du
//! tampon (`played_samples`). Ce compteur est la seule mesure du temps qui soit
//! cadencée par la carte son elle-même : la position média s'en déduit
//! (`ancre + trames consommées / taux`, retard du périphérique retranché).
//! C'est ce que `player.rs` utilise pour cadencer la vidéo au lieu du temps
//! mural. L'arithmétique (`Ancrage`) est pure et testée unitairement.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Taille maximale du tampon **par défaut**, en échantillons (≈ 1 s à 48 kHz stéréo).
///
/// C'est un plafond de mémoire, pas une profondeur de travail : la profondeur visée
/// reste `CIBLE_TAMPON_MS`. Il est **relevable** pour un flux réseau
/// (`set_capacite_ms`), où le tampon fait alors office de pré-cache : c'est ce qui
/// permet de traverser une coupure de plusieurs secondes sans s'arrêter.
const MAX_SAMPLES: usize = 48_000 * 2;

/// Capacité maximale acceptée pour un flux réseau (mémoire : 30 s ≈ 10 Mo à 44,1 kHz
/// stéréo en f32). Au-delà, le pré-cache coûterait plus qu'il ne rapporte.
pub const CAPACITE_MAX_MS: u64 = 30_000;

/// Profondeur visée du tampon, en millisecondes.
///
/// Marge volontairement faible : elle n'a **aucun** effet sur la synchronisation
/// image/son (c'est l'horloge audio qui la fixe), seulement sur la résistance aux
/// à-coups de lecture disque et sur la latence d'un changement de position.
pub const CIBLE_TAMPON_MS: u64 = 120;

/// Origine et arithmétique de l'horloge audio — **pur**, sans périphérique.
///
/// L'horloge est « ancrée » sur un échantillon précis du média : le premier
/// poussé après une remise à zéro (seek, pause, changement de piste). À partir
/// de là, la position média ne dépend plus que du nombre de trames que la carte
/// son a consommées — donc de la carte son, et non du temps système.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ancrage {
    /// Un échantillon de référence a été poussé depuis la dernière remise à zéro.
    actif: bool,
    /// Position média (ms) de cet échantillon de référence.
    pts_ms: u64,
    /// Valeur du compteur de consommation à l'instant de l'ancrage.
    trames_a_l_ancrage: u64,
}

impl Ancrage {
    /// Ancre l'horloge sur l'échantillon de position `pts_ms`, alors que la carte
    /// a consommé `trames_consommees` trames depuis la remise à zéro.
    pub fn sur(&mut self, pts_ms: u64, trames_consommees: u64) {
        self.actif = true;
        self.pts_ms = pts_ms;
        self.trames_a_l_ancrage = trames_consommees;
    }

    /// Oublie l'origine : l'horloge n'existe plus jusqu'au prochain ancrage.
    pub fn oublier(&mut self) {
        *self = Self::default();
    }

    pub fn est_actif(&self) -> bool {
        self.actif
    }

    /// Position média (ms) du son que la carte restitue maintenant.
    ///
    /// - `trames_consommees` : **trames** (échantillons par canal) du média
    ///   sorties du tampon depuis la remise à zéro — le silence de
    ///   sous-alimentation n'est pas compté ;
    /// - `taux` : taux d'échantillonnage du périphérique ;
    /// - `vitesse` : secondes de média par seconde de sortie (l'audio est
    ///   rééchantillonné ou passé à `atempo` pour tenir le débit de la carte) ;
    /// - `retard_ms` : retard du périphérique (ce qui vient d'être remis à la
    ///   carte ne s'entend que plus tard).
    ///
    /// Le résultat ne remonte jamais avant l'ancre : au tout début, le retard du
    /// périphérique ne doit pas retarder l'image (il n'y a pas encore de son).
    pub fn media_ms(
        &self,
        trames_consommees: u64,
        taux: u32,
        vitesse: f32,
        retard_ms: f64,
    ) -> Option<f64> {
        if !self.actif {
            return None;
        }
        let trames = trames_consommees.saturating_sub(self.trames_a_l_ancrage) as f64;
        let secondes = trames / taux.max(1) as f64;
        let v = vitesse.max(0.1) as f64;
        let avance = secondes * 1000.0 * v - retard_ms * v;
        Some((self.pts_ms as f64 + avance).max(self.pts_ms as f64))
    }
}

/// Compteurs partagés entre le thread audio et l'interface (atomiques seulement :
/// le callback ne prend aucun verrou pour les écrire).
#[derive(Default)]
struct Compteurs {
    /// Échantillons du média sortis du tampon vers la carte. Le silence de
    /// sous-alimentation n'est pas compté : il n'appartient à aucun instant du
    /// média. Compté en échantillons (donc trames × canaux), converti à la lecture.
    played_samples: AtomicU64,
    /// Blocs sortis en **contention** : l'interface tenait le tampon, le callback a
    /// émis du silence plutôt que d'attendre. Ce n'est PAS un manque de données —
    /// le compter comme sous-alimentation faisait apparaître un défaut inexistant
    /// (un bloc au démarrage, à chaque lancement).
    contentions: AtomicU64,
    /// Blocs sortis en sous-alimentation (tampon vide) — mesure de confort.
    underruns: AtomicU64,
    /// Retard du périphérique en millisecondes (bits de f64), lissé.
    latency_ms: AtomicU64,
    /// Le tampon a-t-il déjà atteint sa profondeur cible au moins une fois depuis
    /// la dernière remise à zéro ? Avant cela, un tampon incomplet est **normal** :
    /// le premier bloc réclamé par la carte ne peut pas être plein (le décodeur
    /// vient de commencer à produire). Compter ce cas comme une sous-alimentation
    /// signalait un défaut inexistant, à chaque démarrage.
    a_atteint_cible: AtomicBool,
    /// Instant (ms depuis la création de la sortie) de la dernière poussée
    /// d'échantillons. Sert à ne compter une sous-alimentation que si le tampon
    /// était censé être alimenté : quand la piste audio est terminée, le silence
    /// n'est pas un défaut.
    last_push_ms: AtomicU64,
}

/// Sortie audio cpal avec tampon circulaire inter-thread.
pub struct AudioOutput {
    /// Le flux doit rester vivant : sa destruction attend le thread audio.
    _stream: cpal::Stream,
    ring: Arc<Mutex<VecDeque<f32>>>,
    /// Volume stocké en bits de f32 (partagé avec le callback).
    volume_bits: Arc<AtomicU32>,
    /// Lecture active (sinon le callback produit du silence).
    playing: Arc<AtomicBool>,
    compteurs: Arc<Compteurs>,
    /// Origine des horodatages internes (comptage des sous-alimentations).
    origin: std::time::Instant,
    /// Origine de l'horloge audio. N'est écrite et lue que par le thread appelant
    /// (`push` / `media_ms` / `clear`) : le verrou n'est jamais tenu par l'audio.
    ancrage: Mutex<Ancrage>,
    sample_rate: u32,
    channels: u16,
    /// Capacité du tampon, en millisecondes (voir `set_capacite_ms`).
    capacite_ms: Arc<AtomicU64>,
}

impl AudioOutput {
    /// Ouvre le périphérique de sortie par défaut (format f32).
    ///
    /// Le taux d'échantillonnage et le nombre de canaux exposés sont ceux du
    /// périphérique : c'est à l'appelant de convertir ses échantillons à ce
    /// format (sinon la lecture serait à la mauvaise vitesse).
    pub fn new(wanted: Option<&str>) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = match wanted.filter(|n| !n.is_empty()) {
            Some(name) => match find_output_device(&host, name) {
                Some(device) => {
                    tracing::info!("Périphérique audio demandé retenu: {}", name);
                    device
                }
                None => {
                    tracing::warn!(
                        "Périphérique audio « {} » introuvable — retour au périphérique par défaut",
                        name
                    );
                    host.default_output_device()
                        .ok_or_else(|| "Aucun périphérique de sortie audio".to_string())?
                }
            },
            None => host
                .default_output_device()
                .ok_or_else(|| "Aucun périphérique de sortie audio".to_string())?,
        };
        let supported = device
            .default_output_config()
            .map_err(|e| format!("Config audio par défaut indisponible: {}", e))?;
        let config: cpal::StreamConfig = supported.config();

        tracing::info!(
            "Sortie audio: {} — {} Hz, {} canaux ({:?})",
            device.name().unwrap_or_else(|_| "?".into()),
            config.sample_rate.0,
            config.channels,
            supported.sample_format()
        );

        let ring = Arc::new(Mutex::new(VecDeque::<f32>::with_capacity(MAX_SAMPLES)));
        let volume_bits = Arc::new(AtomicU32::new(1.0f32.to_bits()));
        let playing = Arc::new(AtomicBool::new(false));
        let compteurs = Arc::new(Compteurs::default());
        let origin = std::time::Instant::now();
        let orig_cb = origin;

        let ring_cb = Arc::clone(&ring);
        let vol_cb = Arc::clone(&volume_bits);
        let play_cb = Arc::clone(&playing);
        let compt_cb = Arc::clone(&compteurs);
        let err_fn = |e| tracing::error!("Erreur flux audio: {}", e);

        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => device
                .build_output_stream(
                    &config,
                    move |data: &mut [f32], info: &cpal::OutputCallbackInfo| {
                        fill_output(
                            data,
                            &ring_cb,
                            &vol_cb,
                            &play_cb,
                            &compt_cb,
                            retard_peripherique(info),
                            orig_cb,
                        );
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("Création flux audio (f32) impossible: {}", e))?,
            cpal::SampleFormat::I16 => device
                .build_output_stream(
                    &config,
                    move |data: &mut [i16], info: &cpal::OutputCallbackInfo| {
                        let retard = retard_peripherique(info);
                        let mut tmp = vec![0.0f32; data.len()];
                        fill_output(
                            &mut tmp,
                            &ring_cb,
                            &vol_cb,
                            &play_cb,
                            &compt_cb,
                            retard,
                            orig_cb,
                        );
                        for (o, s) in data.iter_mut().zip(tmp.iter()) {
                            *o = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                        }
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("Création flux audio (i16) impossible: {}", e))?,
            other => {
                return Err(format!("Format d'échantillon audio non géré: {:?}", other));
            }
        };

        stream
            .play()
            .map_err(|e| format!("Démarrage flux audio impossible: {}", e))?;

        // Capacité par défaut exprimée en millisecondes, à partir de la constante
        // d'échantillons : une seule source de vérité, et le même plafond mémoire.
        let capacite_defaut = (MAX_SAMPLES as u64 * 1000)
            / (config.sample_rate.0 as u64 * config.channels.max(1) as u64);

        Ok(Self {
            _stream: stream,
            ring,
            volume_bits,
            playing,
            compteurs,
            origin,
            ancrage: Mutex::new(Ancrage::default()),
            sample_rate: config.sample_rate.0,
            channels: config.channels,
            capacite_ms: Arc::new(AtomicU64::new(capacite_defaut)),
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Ajoute des échantillons f32 entrelacés au tampon (production, côté UI).
    ///
    /// `pts_ms` est la position média du **premier** échantillon du lot. Si
    /// l'horloge attend une origine (après un seek, une pause ou un changement de
    /// piste), c'est lui qui la fournit : c'est en effet le prochain échantillon
    /// que la carte son consommera.
    pub fn push(&self, samples: &[f32], pts_ms: u64) {
        if samples.is_empty() {
            return;
        }
        let mut ring = match self.ring.lock() {
            Ok(r) => r,
            Err(_) => return,
        };

        // Ancrage dans la même section critique que le tampon : le compteur de
        // consommation ne peut pas bouger entre la lecture et l'affectation.
        if let Ok(mut ancrage) = self.ancrage.lock() {
            if !ancrage.est_actif() {
                let trames = self.compteurs.played_samples.load(Ordering::Relaxed)
                    / self.channels.max(1) as u64;
                ancrage.sur(pts_ms, trames);
            }
        }

        // En cas de saturation, on jette les échantillons les plus anciens :
        // mieux vaut perdre un peu d'audio que prendre du retard croissant.
        // Par **trames entières** : un demi-échantillon décalerait les canaux.
        self.compteurs
            .last_push_ms
            .store(self.origin.elapsed().as_millis() as u64, Ordering::Relaxed);

        let canaux = self.channels.max(1) as usize;
        let capacite = self.capacite_echantillons();
        let overflow = (ring.len() + samples.len()).saturating_sub(capacite);
        let a_jeter = (overflow.div_ceil(canaux) * canaux).min(ring.len());
        if a_jeter > 0 {
            ring.drain(..a_jeter);
        }
        ring.extend(samples.iter().copied());

        // Profondeur atteinte ? (le test se fait sous le verrou du tampon)
        let trames_presentes = ring.len() / self.channels.max(1) as usize;
        let ms = (trames_presentes as u64 * 1000) / self.sample_rate.max(1) as u64;
        if ms >= CIBLE_TAMPON_MS {
            self.compteurs
                .a_atteint_cible
                .store(true, Ordering::Relaxed);
        }
    }

    /// Niveau de remplissage actuel, en millisecondes.
    pub fn buffered_ms(&self) -> u64 {
        let Ok(ring) = self.ring.lock() else {
            return 0;
        };
        let frames = ring.len() / self.channels.max(1) as usize;
        (frames as u64 * 1000) / self.sample_rate.max(1) as u64
    }

    /// Position média (ms) du son que la carte son restitue **maintenant**.
    ///
    /// `None` tant qu'aucun échantillon n'a été poussé depuis la dernière remise
    /// à zéro : il n'y a alors aucune horloge audio à interroger. `vitesse` est
    /// la vitesse de lecture (secondes de média par seconde de sortie).
    pub fn media_ms(&self, vitesse: f32) -> Option<f64> {
        let ancrage = self.ancrage.lock().ok()?;
        let trames =
            self.compteurs.played_samples.load(Ordering::Relaxed) / self.channels.max(1) as u64;
        let retard = f64::from_bits(self.compteurs.latency_ms.load(Ordering::Relaxed));
        ancrage.media_ms(trames, self.sample_rate, vitesse, retard)
    }

    /// Nombre de trames du média consommées par la carte depuis la remise à zéro.
    pub fn played_frames(&self) -> u64 {
        self.compteurs.played_samples.load(Ordering::Relaxed) / self.channels.max(1) as u64
    }

    /// Blocs sortis en sous-alimentation (tampon vide pendant la lecture).
    pub fn underruns(&self) -> u64 {
        self.compteurs.underruns.load(Ordering::Relaxed)
    }

    /// Blocs où l'interface tenait le tampon (silence émis sans attendre).
    /// Distinct d'une sous-alimentation : les données ne manquaient pas.
    pub fn contentions(&self) -> u64 {
        self.compteurs.contentions.load(Ordering::Relaxed)
    }

    /// Retard estimé du périphérique, en millisecondes.
    pub fn device_latency_ms(&self) -> f64 {
        f64::from_bits(self.compteurs.latency_ms.load(Ordering::Relaxed))
    }

    /// Vide le tampon (changement de média, seek, pause) et **oublie l'origine**
    /// de l'horloge : la prochaine poussée en définira une nouvelle.
    pub fn clear(&self) {
        if let Ok(mut ring) = self.ring.lock() {
            ring.clear();
            if let Ok(mut ancrage) = self.ancrage.lock() {
                ancrage.oublier();
            }
            // Le compteur repart de zéro *avec* l'origine : les deux sont remis à
            // zéro sous le même verrou, aucun échantillon d'une autre époque ne
            // peut être attribué à la nouvelle horloge.
            self.compteurs.played_samples.store(0, Ordering::Relaxed);
            self.compteurs
                .a_atteint_cible
                .store(false, Ordering::Relaxed);
        }
    }

    /// Capacité du tampon, en échantillons (dérivée de `capacite_ms`).
    fn capacite_echantillons(&self) -> usize {
        let ms = self.capacite_ms.load(Ordering::Relaxed);
        ((self.sample_rate as u64 * self.channels.max(1) as u64 * ms) / 1000) as usize
    }

    /// Capacité du tampon, en millisecondes.
    ///
    /// Un **fichier local** n'a besoin que d'environ une seconde : le disque ne
    /// prend pas de retard. Un **flux réseau** en revanche se pré-cache : la
    /// capacité est alors portée à la profondeur demandée (voir
    /// `player::set_precache_ms`), ce qui permet de traverser une coupure sans
    /// s'arrêter — c'est le seul endroit du lecteur où l'on garde du média en
    /// mémoire au-delà de la seconde.
    pub fn set_capacite_ms(&self, ms: u64) {
        self.capacite_ms
            .store(ms.clamp(200, CAPACITE_MAX_MS), Ordering::Relaxed);
    }

    /// Capacité actuelle du tampon, en millisecondes (diagnostic).
    pub fn capacite_ms(&self) -> u64 {
        self.capacite_ms.load(Ordering::Relaxed)
    }

    pub fn set_volume(&self, volume: f32) {
        self.volume_bits
            .store(volume.clamp(0.0, 1.5).to_bits(), Ordering::Relaxed);
    }

    /// Active/désactive la production de son.
    ///
    /// On ne touche **pas** au flux (pas de `play`/`pause`) : ces appels peuvent
    /// attendre le thread audio et donc geler l'interface. Le drapeau suffit, le
    /// callback émettant du silence lorsque la lecture est inactive.
    pub fn set_playing(&self, playing: bool) {
        self.playing.store(playing, Ordering::Relaxed);
    }
}

/// Retard entre l'instant du callback et celui où ses échantillons seront
/// réellement audibles, lu dans l'horodatage fourni par cpal.
///
/// Certains hôtes ne renseignent pas `playback` (ou donnent une valeur absurde) :
/// on renvoie alors `None` et on conserve l'estimation précédente.
fn retard_peripherique(info: &cpal::OutputCallbackInfo) -> Option<f64> {
    let ts = info.timestamp();
    let d = ts.playback.duration_since(&ts.callback)?;
    let ms = d.as_secs_f64() * 1000.0;
    if (0.0..=500.0).contains(&ms) {
        Some(ms)
    } else {
        None
    }
}

/// Remplit `data` depuis le tampon circulaire (consommateur, thread audio).
///
/// N'utilise QUE `try_lock` : ce thread ne doit jamais attendre. Compte les
/// échantillons du média réellement sortis (base de l'horloge audio) et les
/// sous-alimentations (diagnostic).
fn fill_output(
    data: &mut [f32],
    ring: &Arc<Mutex<VecDeque<f32>>>,
    volume: &Arc<AtomicU32>,
    playing: &Arc<AtomicBool>,
    compteurs: &Compteurs,
    latence_ms: Option<f64>,
    origin: std::time::Instant,
) {
    let vol = f32::from_bits(volume.load(Ordering::Relaxed));

    if !playing.load(Ordering::Relaxed) {
        data.fill(0.0);
        return;
    }

    // Retard du périphérique, lissé : il varie de quelques millisecondes d'un
    // bloc à l'autre, et une horloge qui saute ferait sauter l'image.
    if let Some(latence) = latence_ms {
        let ancien = f64::from_bits(compteurs.latency_ms.load(Ordering::Relaxed));
        let lisse = if ancien == 0.0 {
            latence
        } else {
            ancien * 0.8 + latence * 0.2
        };
        compteurs
            .latency_ms
            .store(lisse.to_bits(), Ordering::Relaxed);
    }

    // try_lock : si l'interface tient le tampon, on émet du silence plutôt que
    // d'attendre (évite tout risque de blocage du thread audio).
    let Ok(mut buf) = ring.try_lock() else {
        data.fill(0.0);
        compteurs.contentions.fetch_add(1, Ordering::Relaxed);
        return;
    };

    let mut sortis = 0u64;
    let mut manquants = 0usize;
    for s in data.iter_mut() {
        *s = match buf.pop_front() {
            Some(v) => {
                sortis += 1;
                v * vol
            }
            None => {
                manquants += 1;
                0.0 // sous-alimentation : silence
            }
        };
    }
    let deja_joue = compteurs.played_samples.load(Ordering::Relaxed) > 0;
    if sortis > 0 {
        compteurs
            .played_samples
            .fetch_add(sortis, Ordering::Relaxed);
    }
    // Une sous-alimentation n'est un défaut que si le tampon était censé être
    // alimenté : ni avant le premier échantillon (le périphérique réclame du son
    // avant que le décodeur ait rien produit), ni après la fin de la piste audio
    // (le silence qui suit n'est pas un manque).
    let maintenant = origin.elapsed().as_millis() as u64;
    let alimente_recemment =
        maintenant.saturating_sub(compteurs.last_push_ms.load(Ordering::Relaxed)) < 400;
    // Trois conditions, chacune pour écarter un faux positif mesuré :
    //  - `alimente_recemment` : après la fin de la piste, le silence n'est pas un manque ;
    //  - `deja_joue || sortis > 0` : avant le premier échantillon, le périphérique
    //    réclame du son que le décodeur n'a pas encore produit ;
    //  - `a_atteint_cible` : tant que le tampon n'a jamais atteint sa profondeur,
    //    il est normal qu'il se vide en fin de bloc (démarrage, reprise).
    let cible_atteinte = compteurs.a_atteint_cible.load(Ordering::Relaxed);
    if manquants > 0 && alimente_recemment && (deja_joue || sortis > 0) && cible_atteinte {
        let n = compteurs.underruns.fetch_add(1, Ordering::Relaxed);
        // Trace des premières occurrences seulement : au démarrage, une image
        // d'interface lourde (police, textures) peut vider le tampon ; savoir QUAND
        // permet de distinguer un incident de démarrage d'un défaut de régime.
        if n < 4 {
            tracing::debug!(
                "Sous-alimentation n°{} à {} ms de lecture (manquants {} éch.)",
                n + 1,
                maintenant,
                manquants
            );
        }
    }
}

/// Retrouve un périphérique de sortie par son nom exact (tel que listé dans les
/// préférences).
fn find_output_device(host: &cpal::Host, name: &str) -> Option<cpal::Device> {
    let devices = host.output_devices().ok()?;
    for device in devices {
        if device.name().map(|n| n == name).unwrap_or(false) {
            return Some(device);
        }
    }
    None
}

/// Un périphérique ALSA qui court-circuite le serveur de son (`hw:`, `plughw:`) : seul ce genre
/// d'entrée reste utile à proposer, en « avancé » — le reste (`dmix:`, `front:`, `surround51:`…)
/// n'est que du bruit quand PipeWire fait déjà le travail.
pub fn est_sortie_directe(nom: &str) -> bool {
    nom.starts_with("hw:") || nom.starts_with("plughw:")
}

/// Liste les périphériques de sortie disponibles (nom, taux, canaux) — proposée dans
/// les préférences pour que le réglage « sortie audio » ait un sens concret.
pub fn available_output_devices() -> Vec<(String, u32, u16)> {
    let host = cpal::default_host();
    let mut list = Vec::new();
    if let Ok(devices) = host.output_devices() {
        for device in devices {
            let name = device.name().unwrap_or_else(|_| "?".to_string());
            let (rate, channels) = device
                .default_output_config()
                .map(|c| (c.sample_rate().0, c.channels()))
                .unwrap_or((0, 0));
            list.push((name, rate, channels));
        }
    }
    list.sort_by(|a, b| a.0.cmp(&b.0));
    list
}

#[cfg(test)]
mod tests {
    use super::Ancrage;

    const TAUX: u32 = 48_000;

    #[test]
    fn horloge_absente_tant_qu_aucun_ancrage() {
        let a = Ancrage::default();
        assert_eq!(a.media_ms(0, TAUX, 1.0, 0.0), None);
        assert!(!a.est_actif());
    }

    #[test]
    fn horloge_suit_les_trames_consommees() {
        let mut a = Ancrage::default();
        a.sur(0, 0);
        // 48 000 trames consommées = 1 s de média à vitesse 1.
        let t = a.media_ms(48_000, TAUX, 1.0, 0.0).unwrap();
        assert!((t - 1000.0).abs() < 0.001, "obtenu {}", t);
    }

    #[test]
    fn horloge_demarre_a_la_position_de_l_ancrage() {
        let mut a = Ancrage::default();
        a.sur(90_000, 0);
        assert_eq!(a.media_ms(0, TAUX, 1.0, 0.0).unwrap(), 90_000.0);
    }

    #[test]
    fn horloge_suit_la_vitesse_de_lecture() {
        let mut a = Ancrage::default();
        a.sur(0, 0);
        // 2× : une seconde de sortie vaut deux secondes de média.
        let t = a.media_ms(48_000, TAUX, 2.0, 0.0).unwrap();
        assert!((t - 2000.0).abs() < 0.001, "obtenu {}", t);
    }

    #[test]
    fn horloge_retranche_le_retard_du_peripherique() {
        let mut a = Ancrage::default();
        a.sur(10_000, 0);
        let t = a.media_ms(48_000, TAUX, 1.0, 150.0).unwrap();
        assert!((t - 10_850.0).abs() < 0.001, "obtenu {}", t);
    }

    #[test]
    fn horloge_jamais_avant_l_ancre() {
        let mut a = Ancrage::default();
        a.sur(4_000, 0);
        // 300 ms de retard alors que rien n'est encore consommé.
        assert_eq!(a.media_ms(0, TAUX, 1.0, 300.0).unwrap(), 4_000.0);
    }

    #[test]
    fn horloge_relative_a_l_ancrage_et_non_au_depart_du_compteur() {
        let mut a = Ancrage::default();
        // Ancre posée après un seek alors que le compteur n'est pas à zéro.
        a.sur(30_000, 1_000);
        let t = a.media_ms(49_000, TAUX, 1.0, 0.0).unwrap();
        assert!((t - 31_000.0).abs() < 0.001, "obtenu {}", t);
    }

    #[test]
    fn oublier_puis_reancrer_repart_de_la_nouvelle_position() {
        let mut a = Ancrage::default();
        a.sur(0, 0);
        a.media_ms(48_000, TAUX, 1.0, 0.0);
        a.oublier();
        assert_eq!(a.media_ms(48_000, TAUX, 1.0, 0.0), None);
        a.sur(60_000, 0);
        let t = a.media_ms(24_000, TAUX, 1.0, 0.0).unwrap();
        assert!((t - 60_500.0).abs() < 0.001, "obtenu {}", t);
    }

    #[test]
    fn une_carte_plus_lente_que_le_temps_mural_ralentit_le_media() {
        // Cas visé : en 10 s de temps mural, la carte n'a consommé que 9,98 s
        // (dérive d'horloge de 0,2 %). Le média doit suivre la CARTE.
        let mut a = Ancrage::default();
        a.sur(0, 0);
        let trames = (TAUX as f64 * 0.998) as u64;
        let t = a.media_ms(trames, TAUX, 1.0, 0.0).unwrap();
        assert!((t - 998.0).abs() < 1.0, "obtenu {}", t);
        assert!(t < 1000.0, "le média doit suivre la carte, pas le temps mural");
    }

    #[test]
    fn une_carte_plus_rapide_avance_le_media() {
        let mut a = Ancrage::default();
        a.sur(0, 0);
        let trames = (TAUX as f64 * 1.002) as u64;
        let t = a.media_ms(trames, TAUX, 1.0, 0.0).unwrap();
        assert!((1000.0..=1002.0).contains(&t), "obtenu {}", t);
    }
}

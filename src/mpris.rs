//! MPRIS — les touches média du bureau (KDE, GNOME, claviers multimédias).
//!
//! **Le besoin.** Une touche « lecture/pause » ou « piste suivante » du clavier doit
//! agir sur le lecteur, et le widget média du bureau doit afficher ce qui joue.
//! C'est l'**intégration au bureau** :
//! sans MPRIS, les touches média ne font rien et Plasma affiche « aucun lecteur ».
//!
//! **Pourquoi D-Bus, et pas autre chose.** C'est le mécanisme que le bureau écoute :
//! `org.mpris.MediaPlayer2` (spécification freedesktop, version 2.2). Aucune autre
//! voie ne donne à la fois les touches média, la pochette et la position dans le
//! widget système. Coût : une dépendance (`zbus`), assumée pour cette raison.
//!
//! **Comment l'état circule.** L'interface graphique est propriétaire de l'état de
//! lecture ; MPRIS est un serveur qui répond à des appels venus de l'extérieur. Trois
//! pièces, sans verrou tenu longtemps :
//!
//! - `Etat` — ce que le bureau peut lire (statut, titre, position, volume…) ;
//! - `commandes` — ce que le bureau demande (lecture/pause, suivant, seek…) ; c'est
//!   l'interface graphique qui les exécute, au tour suivant, par ses propres chemins
//!   (donc avec les règles de profil, les sous-titres, etc. — §4 piège 86) ;
//! - la **connexion**, gardée pour émettre `PropertiesChanged` : le bureau ne
//!   redemande pas les propriétés à chaque image, il attend d'être prévenu.
//!
//! **Isolation.** Si le bus de session est absent (banc, session sans D-Bus), le
//! serveur ne démarre pas et le lecteur continue : `Demarrage::Indisponible(raison)`
//! est journalisé, jamais une erreur fatale.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zbus::blocking::Connection;
use zbus::zvariant::{OwnedValue, Value};

/// Chemin d'objet imposé par la spécification MPRIS.
pub const CHEMIN: &str = "/org/mpris/MediaPlayer2";
/// Nom bien connu : `org.mpris.MediaPlayer2.<identifiant>`.
pub const NOM: &str = "org.mpris.MediaPlayer2.rmplayer";
/// Préfixe des identifiants de piste (`mpris:trackid`).
pub const PREFIXE_PISTE: &str = "/org/rmplayer/track/";

pub const IFACE_LECTEUR: &str = "org.mpris.MediaPlayer2.Player";

/// État de lecture, tel que MPRIS le nomme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lecture {
    Arrete,
    Pause,
    Lecture,
}

/// Statut MPRIS correspondant (chaîne exacte de la spécification).
pub fn statut_mpris(etat: Lecture) -> &'static str {
    match etat {
        Lecture::Arrete => "Stopped",
        Lecture::Pause => "Paused",
        Lecture::Lecture => "Playing",
    }
}

/// Mode de boucle MPRIS : `None`, `Track` ou `Playlist`.
///
/// RMPlayer ne sait faire que « relire la playlist » (`loop_enabled`) : on annonce
/// donc `Playlist` ou `None`, jamais `Track` — annoncer un mode qu'on n'applique pas
/// ferait mentir le bureau, qui proposerait un choix sans effet.
pub fn boucle_mpris(actif: bool) -> &'static str {
    if actif {
        "Playlist"
    } else {
        "None"
    }
}

/// Identifiant de piste MPRIS pour un rang de playlist.
pub fn identifiant_piste(index: usize) -> String {
    format!("{}{}", PREFIXE_PISTE, index)
}

/// Rang de playlist extrait d'un identifiant de piste (`None` si étranger).
pub fn rang_de_piste(identifiant: &str) -> Option<usize> {
    identifiant.strip_prefix(PREFIXE_PISTE)?.parse().ok()
}

/// Millisecondes → microsecondes (MPRIS compte en µs, partout).
pub fn micros(ms: u64) -> i64 {
    (ms as i64).saturating_mul(1000)
}

/// Microsecondes → millisecondes, sans passer par un négatif.
pub fn millis(us: i64) -> u64 {
    (us.max(0) as u64) / 1000
}

/// Demande venue du bureau.
///
/// **Aucune n'est exécutée dans le fil D-Bus** : elles sont déposées ici et
/// appliquées par l'interface graphique, qui seule connaît les conséquences
/// (recharger un média, appliquer une règle de profil, mettre à jour l'affichage).
#[derive(Debug, Clone, PartialEq)]
pub enum Commande {
    Lecture,
    Pause,
    LecturePause,
    Arreter,
    Suivant,
    Precedent,
    /// Déplacement relatif, en microsecondes (positif = avant).
    SeDeplacer(i64),
    /// Déplacement absolu, en microsecondes.
    Position(i64),
    Volume(f64),
    /// Vitesse de lecture (propriété `Rate`, que la spécification déclare
    /// modifiable : un client peut vouloir ralentir la lecture).
    Vitesse(f64),
    Aleatoire(bool),
    Boucle(bool),
    Quitter,
    /// Ramener la fenêtre au premier plan.
    Avant,
}

/// Ce que le bureau peut lire. Écrit par l'interface graphique à chaque changement.
#[derive(Debug, Clone)]
pub struct Etat {
    pub lecture: Lecture,
    pub position_ms: u64,
    pub duree_ms: u64,
    pub titre: String,
    pub artiste: String,
    /// Rang de l'élément courant dans la playlist (pour `mpris:trackid`).
    pub rang: usize,
    /// Volume, de 0 à 1 (MPRIS ; RMPlayer autorise jusqu'à 1,5 en interne).
    pub volume: f64,
    pub aleatoire: bool,
    pub boucle: bool,
    pub vitesse: f64,
    pub peut_precedent: bool,
    pub peut_suivant: bool,
}

impl Default for Etat {
    fn default() -> Self {
        Self {
            lecture: Lecture::Arrete,
            position_ms: 0,
            duree_ms: 0,
            titre: String::new(),
            artiste: String::new(),
            rang: 0,
            volume: 1.0,
            aleatoire: false,
            boucle: false,
            vitesse: 1.0,
            peut_precedent: false,
            peut_suivant: false,
        }
    }
}

/// Partage entre l'interface, le fil D-Bus et les gestionnaires d'appels.
struct Partage {
    etat: Mutex<Etat>,
    commandes: Mutex<Vec<Commande>>,
    /// Connexion gardée pour émettre `PropertiesChanged`.
    connexion: Mutex<Option<Connection>>,
    /// Dernier état publié, pour ne signaler que les changements réels.
    publie: Mutex<Option<(String, String, u64, bool, bool)>>,
    /// Contexte egui, pour **réveiller l'interface** quand une commande arrive.
    ///
    /// **Pourquoi c'est indispensable** : hors lecture, egui reste passif — il ne
    /// redessine que sur événement. Une commande déposée par le bureau pendant une
    /// pause n'aurait donc jamais été exécutée : la touche média semblait morte
    /// (mesuré : `Next` et le réglage de volume sans effet dès que le lecteur était
    /// en pause, alors que `PlayPause` fonctionnait en lecture).
    reveil: Mutex<Option<egui::Context>>,
}

impl Partage {
    fn etat(&self) -> Etat {
        self.etat
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }
    fn pousser(&self, commande: Commande) {
        if let Ok(mut liste) = self.commandes.lock() {
            liste.push(commande);
        }
        if let Ok(g) = self.reveil.lock() {
            if let Some(ctx) = g.as_ref() {
                ctx.request_repaint();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Interfaces exposées sur le bus
// ---------------------------------------------------------------------------

struct Racine {
    partage: Arc<Partage>,
}

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Racine {
    #[zbus(property)]
    fn identity(&self) -> String {
        "RMPlayer".to_string()
    }

    /// RMPlayer peut se fermer depuis le bureau (`Quit` est implémenté).
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }

    /// Une fenêtre peut être ramenée au premier plan — **demande**, que le
    /// compositeur peut ignorer sous Wayland.
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }

    /// Pas de liste de pistes MPRIS (`org.mpris.MediaPlayer2.TrackList`) : la
    /// playlist de RMPlayer se pilote dans sa fenêtre, et implémenter cette interface
    /// à moitié serait pire que ne pas l'annoncer.
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        "rmplayer".to_string()
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec![
            "file".to_string(),
            "http".to_string(),
            "https".to_string(),
            "rtsp".to_string(),
            "rtmp".to_string(),
            "mms".to_string(),
            "smb".to_string(),
        ]
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        crate::utils::TYPES_MIME.iter().map(|t| t.to_string()).collect()
    }

    fn quit(&self) {
        tracing::info!("MPRIS : demande de fermeture");
        self.partage.pousser(Commande::Quitter);
    }

    fn raise(&self) {
        tracing::info!("MPRIS : demande de mise au premier plan");
        self.partage.pousser(Commande::Avant);
    }
}

struct Lecteur {
    partage: Arc<Partage>,
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Lecteur {
    #[zbus(property)]
    fn playback_status(&self) -> String {
        statut_mpris(self.partage.etat().lecture).to_string()
    }

    #[zbus(property)]
    fn loop_status(&self) -> String {
        boucle_mpris(self.partage.etat().boucle).to_string()
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        self.partage.etat().vitesse
    }

    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.partage.etat().aleatoire
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.partage.etat().volume
    }

    /// Position **en microsecondes** (spécification MPRIS).
    #[zbus(property)]
    fn position(&self) -> i64 {
        micros(self.partage.etat().position_ms)
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        crate::player::VITESSE_MIN as f64
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        crate::player::VITESSE_MAX as f64
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.partage.etat().peut_suivant
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.partage.etat().peut_precedent
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.partage.etat().duree_ms > 0
    }

    /// Métadonnées : le strict nécessaire pour qu'un widget média soit utile
    /// (identifiant de piste, durée, titre, artiste).
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let etat = self.partage.etat();
        let mut m: HashMap<String, OwnedValue> = HashMap::new();
        let piste = identifiant_piste(etat.rang);
        if let Ok(v) = OwnedValue::try_from(Value::from(piste)) {
            m.insert("mpris:trackid".to_string(), v);
        }
        if etat.duree_ms > 0 {
            if let Ok(v) = OwnedValue::try_from(Value::from(micros(etat.duree_ms))) {
                m.insert("mpris:length".to_string(), v);
            }
        }
        if let Ok(v) = OwnedValue::try_from(Value::from(etat.titre.clone())) {
            m.insert("xesam:title".to_string(), v);
        }
        if !etat.artiste.is_empty() {
            if let Ok(v) = OwnedValue::try_from(Value::from(vec![etat.artiste.clone()])) {
                m.insert("xesam:artist".to_string(), v);
            }
        }
        m
    }

    /// `Properties.Set Volume` : le curseur de volume du widget média.
    #[zbus(property)]
    fn set_volume(&self, volume: f64) {
        self.partage.pousser(Commande::Volume(volume.clamp(0.0, 1.0)));
    }

    /// `Properties.Set Rate` : borné aux limites du moteur (`VITESSE_MIN`/`MAX`).
    #[zbus(property)]
    fn set_rate(&self, vitesse: f64) {
        self.partage.pousser(Commande::Vitesse(vitesse));
    }

    /// `Properties.Set Shuffle`.
    #[zbus(property)]
    fn set_shuffle(&self, actif: bool) {
        self.partage.pousser(Commande::Aleatoire(actif));
    }

    /// `Properties.Set LoopStatus` : `Track` n'est pas géré par RMPlayer (il relit la
    /// playlist entière) — on le traite comme `Playlist` plutôt que de refuser, ce
    /// qui laisserait le widget dans un état incohérent.
    #[zbus(property)]
    fn set_loop_status(&self, mode: String) {
        self.partage.pousser(Commande::Boucle(mode != "None"));
    }

    fn play(&self) {
        self.partage.pousser(Commande::Lecture);
    }

    fn pause(&self) {
        self.partage.pousser(Commande::Pause);
    }

    fn play_pause(&self) {
        self.partage.pousser(Commande::LecturePause);
    }

    fn stop(&self) {
        self.partage.pousser(Commande::Arreter);
    }

    fn next(&self) {
        self.partage.pousser(Commande::Suivant);
    }

    fn previous(&self) {
        self.partage.pousser(Commande::Precedent);
    }

    /// `Offset` : déplacement **relatif**, en microsecondes.
    fn seek(&self, offset: i64) {
        self.partage.pousser(Commande::SeDeplacer(offset));
    }

    /// `SetPosition` : déplacement absolu, en microsecondes, avec l'identifiant de
    /// piste. Un identifiant étranger (autre piste) est ignoré, comme le veut la
    /// spécification — c'est ce qui évite de sauter au mauvais endroit quand le
    /// bureau a une vue en retard.
    fn set_position(&self, track_id: String, position: i64) {
        let etat = self.partage.etat();
        let courant = identifiant_piste(etat.rang);
        if rang_de_piste(&track_id) == Some(etat.rang) {
            self.partage.pousser(Commande::Position(position));
        } else {
            tracing::debug!(
                "MPRIS : SetPosition igno  ré (piste {} ≠ courante {})",
                track_id,
                courant
            );
        }
    }
}

/// Ce que le démarrage a donné.
pub enum Demarrage {
    /// Le nom est pris sur le bus : le lecteur est pilotable depuis le bureau.
    Actif(Arc<Serveur>),
    /// Pas de bus de session (banc, session minimale) : le lecteur vit sans.
    Indisponible(String),
}

/// Serveur MPRIS : garde la connexion et l'état partagé.
pub struct Serveur {
    partage: Arc<Partage>,
    nom: String,
}

impl Serveur {
    /// Publie l'état courant. N'émet `PropertiesChanged` que si quelque chose a
    /// réellement changé — sinon le bureau serait réveillé à chaque image.
    ///
    /// `position_ms` n'est **pas** dans la comparaison : la spécification demande de
    /// ne pas signaler la position (elle est interrogée à la demande, `Position`).
    pub fn publier(&self, etat: Etat) {
        let cle = (
            statut_mpris(etat.lecture).to_string(),
            format!("{}|{}|{}", etat.titre, etat.artiste, etat.rang),
            etat.duree_ms,
            etat.aleatoire,
            etat.boucle,
        );
        // **L'état est toujours mis à jour**, même quand rien ne change à signaler :
        // la position avance à chaque image et n'est pas dans la clé de changement
        // (la spécification déconseille de la signaler). Sortir plus tôt laissait
        // `Position` figée à sa première valeur — mesuré : 0 µs après 8 s de lecture.
        if let Ok(mut g) = self.partage.etat.lock() {
            *g = etat;
        }
        if let Ok(mut publie) = self.partage.publie.lock() {
            if publie.as_ref() == Some(&cle) {
                return;
            }
            *publie = Some(cle.clone());
        }
        // Flèche : émettre `PropertiesChanged` (le bureau ne redemande pas).
        let Some(connexion) = self
            .partage
            .connexion
            .lock()
            .ok()
            .and_then(|g| g.clone())
        else {
            return;
        };
        let statut = cle.0.clone();
        let mut changes: HashMap<&str, Value> = HashMap::new();
        changes.insert("PlaybackStatus", Value::from(statut.as_str()));
        changes.insert("Shuffle", Value::from(cle.3));
        changes.insert("LoopStatus", Value::from(boucle_mpris(cle.4)));
        changes.insert("Metadata", self.metadata_valeur());
        let _ = connexion.emit_signal(
            None::<&str>,
            CHEMIN,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &(IFACE_LECTEUR, changes, Vec::<String>::new()),
        );
    }

    fn metadata_valeur(&self) -> Value<'static> {
        let etat = self.partage.etat();
        let mut m: HashMap<String, OwnedValue> = HashMap::new();
        if let Ok(v) = OwnedValue::try_from(Value::from(identifiant_piste(etat.rang))) {
            m.insert("mpris:trackid".to_string(), v);
        }
        if etat.duree_ms > 0 {
            if let Ok(v) = OwnedValue::try_from(Value::from(micros(etat.duree_ms))) {
                m.insert("mpris:length".to_string(), v);
            }
        }
        if let Ok(v) = OwnedValue::try_from(Value::from(etat.titre.clone())) {
            m.insert("xesam:title".to_string(), v);
        }
        Value::from(m)
    }

    /// Commandes déposées par le bureau depuis le dernier appel.
    pub fn commandes(&self) -> Vec<Commande> {
        self.partage
            .commandes
            .lock()
            .map(|mut l| std::mem::take(&mut *l))
            .unwrap_or_default()
    }

    pub fn nom(&self) -> &str {
        &self.nom
    }

    /// Branche le réveil de l'interface : une commande du bureau provoque un tour
    /// d'affichage, même en pause (sinon elle attendrait un événement quelconque).
    pub fn reveiller_avec(&self, ctx: egui::Context) {
        if let Ok(mut g) = self.partage.reveil.lock() {
            *g = Some(ctx);
        }
    }
}

/// Démarre le serveur MPRIS sur le bus de session.
///
/// Ne bloque pas : la connexion et l'enregistrement des interfaces prennent quelques
/// millisecondes, une seule fois au démarrage.
pub fn demarrer(nom_joueur: &str) -> Demarrage {
    let partage = Arc::new(Partage {
        etat: Mutex::new(Etat::default()),
        commandes: Mutex::new(Vec::new()),
        connexion: Mutex::new(None),
        publie: Mutex::new(None),
        reveil: Mutex::new(None),
    });
    let racine = Racine {
        partage: partage.clone(),
    };
    let lecteur = Lecteur {
        partage: partage.clone(),
    };
    let resultat = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.name(NOM))
        .and_then(|b| b.serve_at(CHEMIN, racine))
        .and_then(|b| b.serve_at(CHEMIN, lecteur))
        .and_then(|b| b.build());
    match resultat {
        Ok(connexion) => {
            if let Ok(mut g) = partage.connexion.lock() {
                *g = Some(connexion);
            }
            tracing::info!(
                "MPRIS : {} exposé sur le bus de session (identité « {} »)",
                NOM,
                nom_joueur
            );
            Demarrage::Actif(Arc::new(Serveur {
                partage,
                nom: NOM.to_string(),
            }))
        }
        Err(e) => {
            tracing::info!("MPRIS indisponible : {}", e);
            Demarrage::Indisponible(e.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_statuts_suivent_la_specification() {
        // Chaînes exactes : un bureau compare ces valeurs, une majuscule change tout.
        assert_eq!(statut_mpris(Lecture::Arrete), "Stopped");
        assert_eq!(statut_mpris(Lecture::Pause), "Paused");
        assert_eq!(statut_mpris(Lecture::Lecture), "Playing");
        assert_eq!(boucle_mpris(true), "Playlist");
        assert_eq!(boucle_mpris(false), "None");
    }

    #[test]
    fn l_identifiant_de_piste_va_et_vient() {
        let id = identifiant_piste(7);
        assert_eq!(id, "/org/rmplayer/track/7");
        assert_eq!(rang_de_piste(&id), Some(7));
        // Un identifiant étranger (autre lecteur, ou valeur d'un bureau en retard)
        // ne doit pas être interprété.
        assert_eq!(rang_de_piste("/org/mpris/MediaPlayer2/TrackList/1"), None);
        assert_eq!(rang_de_piste(""), None);
    }

    #[test]
    fn les_conversions_de_temps_sont_exactes_et_sans_debordement() {
        assert_eq!(micros(0), 0);
        assert_eq!(micros(1), 1000);
        assert_eq!(micros(3_600_000), 3_600_000_000);
        assert_eq!(millis(3_600_000_000), 3_600_000);
        // Une valeur négative (bogue d'appelant) ne doit pas produire un temps
        // énorme par conversion non signée.
        assert_eq!(millis(-5_000), 0);
        // Pas de débordement sur une durée absurde.
        assert!(micros(u64::MAX / 2) > 0);
    }

    #[test]
    fn l_etat_par_defaut_ne_pretend_rien() {
        let e = Etat::default();
        assert_eq!(statut_mpris(e.lecture), "Stopped");
        assert_eq!(e.duree_ms, 0);
        assert!(e.titre.is_empty());
        // Sans playlist, on ne prétend pas pouvoir aller ailleurs.
        assert!(!e.peut_suivant && !e.peut_precedent);
    }
}

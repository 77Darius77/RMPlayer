//! Instance unique — « ouvrir avec » arrive dans la fenêtre déjà ouverte
//!
//! **Le problème.** Un fichier ouvert depuis le gestionnaire de fichiers doit être
//! lu : si KPlayer tourne déjà, lancer un second processus donnerait deux fenêtres,
//! deux lectures concurrentes du même fichier de position, et l'impression que le
//! clic n'a rien fait. Le comportement attendu est celui des lecteurs de bureau :
//! le fichier arrive dans la fenêtre existante.
//!
//! **Pourquoi pas X11.** Sous X11, un tel mécanisme se fait par propriété de
//! fenêtre ou par D-Bus. Or Plasma 6.8 n'a plus de session X11 (page mémoire
//! `kplayer-wayland-plasma68`) et une propriété de fenêtre X11 n'existe pas sous
//! Wayland. D-Bus demanderait une crate (`zbus`) et nommerait un service ; un
//! **socket Unix** fait le travail avec la bibliothèque standard seule, sans
//! dépendance et sans configuration de bus.
//!
//! **Le nom du socket dépend du dossier de configuration.** C'est ce qui empêche
//! deux sessions isolées (les bancs d'essai, qui posent `XDG_CONFIG_HOME`) de se
//! voler leurs fichiers : l'instance d'un banc ne parle qu'aux processus qui
//! partagent sa configuration.
//!
//! **Limite assumée** : Wayland interdit à un client de se mettre au premier plan
//! (`ViewportCommand::Focus` est une *demande* que le compositeur peut ignorer
//! sans jeton d'activation). Le fichier est bien chargé ; la fenêtre peut rester
//! derrière. KWin la met en avant dans la plupart des cas, mais ce n'est pas
//! garanti par le protocole — c'est dit ici plutôt que promis à l'utilisateur.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Variable d'environnement qui désactive la mécanique (utile aux bancs).
pub const DESACTIVATION: &str = "KPLAYER_INSTANCE_UNIQUE";

/// Ce que le démarrage a donné.
pub enum Demarrage {
    /// Aucune autre instance : celle-ci écoute.
    Unique(Instance),
    /// Une instance existante a reçu les fichiers ; ce processus doit sortir.
    Transfere { destinataire: PathBuf },
    /// Mécanique désactivée (option, variable d'environnement, pas de dossier).
    Desactive(&'static str),
}

/// Écoute de l'instance courante.
pub struct Instance {
    /// Chemins reçus d'autres processus, à consommer depuis l'interface.
    recus: Receiver<Vec<PathBuf>>,
    /// Fichier du socket, retiré à la fermeture.
    chemin: Option<PathBuf>,
    /// Arrêt demandé pour le fil d'écoute.
    arrete: Arc<AtomicBool>,
}

impl Instance {
    /// Démarre l'écoute, ou transmet `fichiers` à l'instance déjà présente.
    ///
    /// `fichiers` vide = simple demande d'activation (la fenêtre existante est
    /// ramenée au premier plan sans changer de média).
    pub fn demarrer(fichiers: &[PathBuf], desactive_par_option: bool) -> Demarrage {
        if desactive_par_option {
            return Demarrage::Desactive("option --nouvelle-instance");
        }
        match std::env::var(DESACTIVATION) {
            Ok(v) if matches!(v.trim(), "0" | "off" | "non" | "false" | "") => {
                return Demarrage::Desactive("variable KPLAYER_INSTANCE_UNIQUE")
            }
            _ => {}
        }
        let Some(chemin) = chemin_socket() else {
            return Demarrage::Desactive("aucun dossier pour le socket (XDG_RUNTIME_DIR absent)");
        };

        // Sockets laissés par des instances tuées (bancs, plantages) : on nettoie
        // avant de décider quoi que ce soit.
        if let Some(dossier) = chemin.parent() {
            nettoyer_sockets_orphelins(dossier, &chemin);
        }

        // Une instance répond-elle déjà ? On lui parle et on s'arrête là.
        if let Ok(mut flux) = UnixStream::connect(&chemin) {
            if transmettre(&mut flux, fichiers).is_ok() {
                return Demarrage::Transfere {
                    destinataire: chemin,
                };
            }
            tracing::warn!(
                "instance existante injoignable sur {} : reprise en main",
                chemin.display()
            );
        }

        // Socket d'une session morte (kill -9, plantage) : le fichier reste mais
        // personne n'écoute. On le retire — sans cela, la mécanique resterait
        // cassée jusqu'au prochain redémarrage de la machine.
        // Cas résiduel : un fichier très récemment créé, ou un dossier sans droit
        // de suppression, peut avoir échappé au balayage.
        if chemin.exists() {
            tracing::info!(
                "instance précédente disparue sans fermer son socket : reprise ({})",
                chemin.display()
            );
            if let Err(e) = std::fs::remove_file(&chemin) {
                tracing::warn!("socket périmé non retiré ({}): {}", chemin.display(), e);
            }
        }

        let ecoute = match UnixListener::bind(&chemin) {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("instance unique indisponible ({}): {}", chemin.display(), e);
                return Demarrage::Desactive("socket non créable");
            }
        };
        let (tx, recus) = mpsc::channel();
        let arrete = Arc::new(AtomicBool::new(false));
        let drapeau = arrete.clone();
        let chemin_fil = chemin.clone();
        std::thread::Builder::new()
            .name("kplayer-instance".into())
            .spawn(move || {
                for flux in ecoute.incoming() {
                    if drapeau.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(flux) = flux else { continue };
                    // **Un seul lecteur tamponné pour toute la connexion.** La
                    // première ligne peut être la sonde de présence ou le premier
                    // chemin ; un `BufReader` créé après coup perdrait ce qui a été
                    // lu en avance (défaut constaté : la sonde avalait le premier
                    // fichier, donc un envoi d'un seul fichier arrivait vide).
                    let _ = flux.set_read_timeout(Some(std::time::Duration::from_secs(3)));
                    let mut lecteur = BufReader::new(&flux);
                    let mut premiere = String::new();
                    let _ = lecteur.read_line(&mut premiere);
                    if premiere.trim() == PING {
                        let mut sortie = &flux;
                        let _ = writeln!(sortie, "{}", PONG);
                        let _ = sortie.flush();
                        continue;
                    }
                    let mut fichiers: Vec<PathBuf> = Vec::new();
                    if !premiere.trim().is_empty() {
                        fichiers.push(PathBuf::from(premiere.trim()));
                    }
                    for ligne in lecteur.lines().map_while(Result::ok) {
                        let ligne = ligne.trim();
                        if !ligne.is_empty() {
                            fichiers.push(PathBuf::from(ligne));
                        }
                    }
                    tracing::info!(
                        "instance unique : {} fichier(s) reçu(s) de {}",
                        fichiers.len(),
                        flux
                            .peer_addr()
                            .map(|a| a.as_pathname().map(|p| p.display().to_string()))
                            .ok()
                            .flatten()
                            .unwrap_or_else(|| "un autre processus".into())
                    );
                    if tx.send(fichiers).is_err() {
                        break;
                    }
                }
                let _ = chemin_fil;
            })
            .ok();

        Demarrage::Unique(Instance {
            recus,
            chemin: Some(chemin),
            arrete,
        })
    }

    /// Chemins arrivés depuis le dernier tour (jamais bloquant).
    pub fn recuperer(&self) -> Vec<Vec<PathBuf>> {
        let mut tous = Vec::new();
        while let Ok(fichiers) = self.recus.try_recv() {
            tous.push(fichiers);
        }
        tous
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.arrete.store(true, Ordering::Relaxed);
        if let Some(chemin) = &self.chemin {
            // Un flux en attente débloque `accept` : on s'y connecte une dernière
            // fois pour que le fil se termine proprement.
            let _ = UnixStream::connect(chemin);
            let _ = std::fs::remove_file(chemin);
        }
    }
}

/// Chemin du socket : `$XDG_RUNTIME_DIR/kplayer-<empreinte de la configuration>.sock`.
///
/// L'empreinte porte sur le **dossier de configuration** : deux sessions isolées
/// (les bancs posent `XDG_CONFIG_HOME`) ne se parlent jamais.
fn chemin_socket() -> Option<PathBuf> {
    let runtime = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|d| !d.trim().is_empty())
        // Repli : /run/user/<uid> si l'utilisateur est connecté, sinon un dossier
        // privé dans /tmp. L'uid vient des métadonnées de /proc/self (aucune
        // dépendance `libc` pour un seul nombre).
        .or_else(|| {
            let uid = std::fs::metadata("/proc/self").ok()?.uid();
            let run = PathBuf::from(format!("/run/user/{uid}"));
            Some(if run.is_dir() {
                run.display().to_string()
            } else {
                format!("/tmp/kplayer-{uid}")
            })
        })?;
    let dossier = PathBuf::from(runtime);
    if !dossier.is_dir() {
        // /tmp/kplayer-<uid> n'existe pas encore : on le crée, privé.
        if dossier.starts_with("/tmp") {
            std::fs::create_dir_all(&dossier).ok()?;
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dossier, std::fs::Permissions::from_mode(0o700));
        } else {
            return None;
        }
    }
    let config = crate::config::config_path()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    Some(dossier.join(format!("kplayer-{}.sock", empreinte(&config))))
}

/// Empreinte courte et stable d'une chaîne (sans dépendance de hachage externe).
fn empreinte(texte: &str) -> String {
    // FNV-1a 64 : stable entre sessions et entre compilations, contrairement à
    // `DefaultHasher` (dont l'algorithme peut changer).
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for octet in texte.as_bytes() {
        h ^= *octet as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:016x}", h)
}

/// Marqueur de « simple présence » : ce n'est pas un chemin (un chemin de la ligne
/// de commande est absolu, il commence par « / »), et une instance vivante répond
/// « pong » sans rien ouvrir.
const PING: &str = "#ping";
const PONG: &str = "pong";

/// Une instance répond-elle sur ce socket ? (500 ms au plus)
fn repond(chemin: &Path) -> bool {
    let Ok(mut flux) = UnixStream::connect(chemin) else {
        return false;
    };
    let court = Some(std::time::Duration::from_millis(500));
    let _ = flux.set_read_timeout(court);
    let _ = flux.set_write_timeout(court);
    if writeln!(flux, "{}", PING).is_err() || flux.flush().is_err() {
        return false;
    }
    let _ = flux.shutdown(std::net::Shutdown::Write);
    let mut reponse = String::new();
    BufReader::new(&flux).read_line(&mut reponse).is_ok()
        && reponse.trim() == PONG
}

/// Retire les sockets `kplayer-*.sock` du dossier d'exécution dont l'instance est
/// morte.
///
/// **Pourquoi ce balayage.** Un processus tué par `SIGTERM` ou `SIGKILL` n'exécute
/// aucun code de nettoyage : le fichier du socket reste. Sans balayage, chaque
/// session de bancs — qui utilise son propre dossier de configuration, donc son
/// propre socket — en laisse un derrière elle (26 accumulés en une seule suite,
/// constaté). On interroge chaque socket : celui qui ne répond pas est mort.
///
/// La sonde est un **ping** et non une connexion vide : une instance vivante ne
/// doit pas voir sa fenêtre passer au premier plan parce qu'un autre processus a
/// balayé les sockets.
fn nettoyer_sockets_orphelins(dossier: &Path, le_notre: &Path) {
    let Ok(entrees) = std::fs::read_dir(dossier) else {
        return;
    };
    let mut retires = 0;
    for entree in entrees.flatten() {
        let chemin = entree.path();
        let nom = entree.file_name().to_string_lossy().to_string();
        if !nom.starts_with("kplayer-") || !nom.ends_with(".sock") {
            continue;
        }
        if repond(&chemin) {
            continue;
        }
        if std::fs::remove_file(&chemin).is_ok() {
            retires += 1;
            // Le nôtre mérite une ligne visible : c'est la trace qu'une instance
            // précédente est morte sans se refermer (plantage, kill -9).
            if chemin == le_notre {
                tracing::info!(
                    "instance précédente disparue sans fermer son socket : reprise ({})",
                    chemin.display()
                );
            }
        }
    }
    if retires > 0 {
        tracing::debug!("{} socket(s) orphelin(s) retiré(s) de {}", retires, dossier.display());
    }
}

/// Envoie `fichiers` (un chemin par ligne) puis ferme l'écriture.
fn transmettre(flux: &mut UnixStream, fichiers: &[PathBuf]) -> std::io::Result<()> {
    // Délai : une instance figée ne doit pas bloquer le lancement pour toujours.
    flux.set_write_timeout(Some(std::time::Duration::from_secs(3)))?;
    for f in fichiers {
        writeln!(flux, "{}", f.display())?;
    }
    flux.flush()?;
    flux.shutdown(std::net::Shutdown::Write)
}

/// Le chemin est-il un média acceptable pour une transmission ?
///
/// Volontairement large : un fichier existant ou une adresse de flux. Le filtrage
/// par extension se fait à l'ouverture, pas ici — sinon un format vidéo inconnu
/// serait refusé au moment du clic, sans explication.
pub fn est_media(chemin: &Path) -> bool {
    let texte = chemin.to_string_lossy();
    crate::utils::is_stream_url(&texte) || chemin.exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_empreinte_est_stable_et_discriminante() {
        // Même entrée, même valeur : c'est ce qui permet au socket de garder le
        // même nom d'une session à l'autre.
        assert_eq!(empreinte("/home/x/.config/kplayer"), empreinte("/home/x/.config/kplayer"));
        assert_ne!(empreinte("/a"), empreinte("/b"));
        assert_eq!(empreinte("").len(), 16);
        // Valeur figée : si l'algorithme changeait, les sockets d'une version
        // précédente seraient orphelins sans que personne ne le voie.
        assert_eq!(empreinte("kplayer"), "b6dcc233c3cc71e3");
    }

    #[test]
    fn un_fichier_existant_ou_un_flux_sont_des_medias() {
        let ici = std::env::current_dir().unwrap().join("Cargo.toml");
        assert!(est_media(&ici));
        assert!(est_media(Path::new("https://exemple.test/flux.m3u8")));
        assert!(!est_media(Path::new("/chemin/absent/qui/n/existe/pas.mkv")));
    }

    #[test]
    fn le_socket_depend_du_dossier_de_configuration() {
        // Deux configurations isolées ne doivent pas partager de socket : sans
        // cela, un banc volerait les fichiers de l'utilisateur.
        let a = format!("kplayer-{}.sock", empreinte("/tmp/banc-a"));
        let b = format!("kplayer-{}.sock", empreinte("/tmp/banc-b"));
        assert_ne!(a, b);
    }
}

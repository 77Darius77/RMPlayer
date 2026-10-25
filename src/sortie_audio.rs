//! Sorties audio du système, vues par PipeWire : les lister sous leur nom lisible et diriger le
//! flux du lecteur vers l'une d'elles.
//!
//! **Pourquoi pas la liste de cpal.** Sous PipeWire, cpal passe par ALSA et énumère des noms
//! bruts (`dmix:CARD=HDMI,DEV=3`, `hw:CARD=Generic,DEV=0`…) : une trentaine d'entrées, aucune ne
//! désigne l'enceinte Bluetooth ou l'écran que la personne utilise, et les `hw:` contournent le
//! serveur de son. Les vraies sorties sont les nœuds `Audio/Sink` de PipeWire.
//!
//! **Comment.** Le flux est ouvert sur la sortie par défaut (le « périphérique » ALSA `default`),
//! puis redirigé en posant `target.object` dans les métadonnées PipeWire de ce flux — ce que fait
//! l'applet de volume de KDE. C'est utilisable en cours de lecture, sans rouvrir le film. On passe
//! par `pw-dump` et `pw-metadata` (installés avec PipeWire) : aucune bibliothèque de plus, et
//! l'absence des outils se traduit par une liste vide, pas par une panne.
//!
//! **La mémoire de WirePlumber.** Il retient la dernière destination d'une application et la lui
//! rend à la prochaine ouverture. Revenir à « périphérique par défaut » doit donc EFFACER la cible
//! (`diriger_flux(None)`), sinon le choix précédent reviendrait seul.

use serde_json::Value;
use std::process::Command;
use std::time::Duration;

/// Préfixe d'un identifiant de sortie PipeWire dans `audio.device`. Les anciens réglages (noms
/// ALSA bruts, ou vide) restent valables : seuls les identifiants préfixés passent par ici.
pub const PREFIXE: &str = "pw:";

/// Une sortie audio PipeWire.
#[derive(Debug, Clone, PartialEq)]
pub struct Sortie {
    /// `node.name`, stable d'une session à l'autre (c'est ce que WirePlumber mémorise aussi).
    pub nom: String,
    /// `node.description` : « JBL Flip 4 », « Navi 21/23 HDMI/DP Audio Controller… ».
    pub description: String,
    /// Sortie par défaut du système au moment de la lecture du graphe.
    pub par_defaut: bool,
}

impl Sortie {
    /// Identifiant à écrire dans `audio.device`.
    pub fn identifiant(&self) -> String {
        format!("{PREFIXE}{}", self.nom)
    }
}

/// Ce qu'on retient du graphe PipeWire.
#[derive(Debug, Default, PartialEq)]
pub struct Graphe {
    pub sorties: Vec<Sortie>,
    /// Nœud du flux de lecture du processus `pid`, s'il existe déjà.
    pub flux: Option<u32>,
}

#[derive(Debug, PartialEq)]
pub enum Erreur {
    /// `pw-dump` / `pw-metadata` absents ou en échec : pas de PipeWire exploitable.
    Outil(String),
    /// Le flux du lecteur n'existe pas (encore) dans le graphe.
    FluxAbsent,
    /// La sortie demandée n'existe pas (débranchée, éteinte).
    SortieAbsente(String),
}

impl std::fmt::Display for Erreur {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Erreur::Outil(e) => write!(f, "PipeWire indisponible : {e}"),
            Erreur::FluxAbsent => write!(f, "le flux audio du lecteur n'est pas encore dans PipeWire"),
            Erreur::SortieAbsente(n) => write!(f, "la sortie « {n} » n'existe pas"),
        }
    }
}

/// Le nom PipeWire d'un identifiant `pw:<nom>` ; `None` pour « par défaut » et pour les noms ALSA.
pub fn nom_pipewire(identifiant: &str) -> Option<&str> {
    identifiant.strip_prefix(PREFIXE).filter(|n| !n.is_empty())
}

/// Le nom ALSA à ouvrir avec cpal pour cet identifiant. « Par défaut » et les sorties PipeWire
/// ouvrent le périphérique par défaut (le flux est ensuite redirigé) ; un ancien nom ALSA est
/// ouvert tel quel.
pub fn nom_alsa(identifiant: &str) -> Option<String> {
    if identifiant.is_empty() || identifiant.starts_with(PREFIXE) {
        None
    } else {
        Some(identifiant.to_string())
    }
}

/// Lit le résultat de `pw-dump`. Pure (testable sans PipeWire).
pub fn analyser(json: &str, pid: u32) -> Option<Graphe> {
    let objets: Vec<Value> = serde_json::from_str(json).ok()?;
    let texte = |v: &Value, cle: &str| v.get(cle).and_then(Value::as_str).map(str::to_owned);
    let est = |o: &Value, suffixe: &str| {
        o.get("type").and_then(Value::as_str).is_some_and(|t| t.ends_with(suffixe))
    };
    let props = |o: &Value| o.get("info").and_then(|i| i.get("props")).cloned().unwrap_or(Value::Null);

    // Sortie par défaut : métadonnée `default.audio.sink` = {"name": "…"}.
    let mut defaut: Option<String> = None;
    for o in objets.iter().filter(|o| est(o, "Metadata")) {
        for m in o.get("metadata").and_then(Value::as_array).into_iter().flatten() {
            if m.get("key").and_then(Value::as_str) == Some("default.audio.sink") {
                defaut = m.get("value").and_then(|v| texte(v, "name"));
            }
        }
    }

    // Les clients PipeWire du processus : leur `application.process.id` est le seul lien fiable
    // entre un flux et nous (les noms d'application peuvent être partagés entre instances).
    let identifiant_pid = |p: &Value| match p.get("application.process.id") {
        Some(Value::Number(n)) => n.as_u64() == Some(u64::from(pid)),
        Some(Value::String(s)) => s.parse::<u64>().ok() == Some(u64::from(pid)),
        _ => false,
    };
    let clients: Vec<u64> = objets
        .iter()
        .filter(|o| est(o, "Client") && identifiant_pid(&props(o)))
        .filter_map(|o| o.get("id").and_then(Value::as_u64))
        .collect();

    let mut graphe = Graphe::default();
    for o in objets.iter().filter(|o| est(o, "Node")) {
        let p = props(o);
        match p.get("media.class").and_then(Value::as_str) {
            Some("Audio/Sink") => {
                let Some(nom) = texte(&p, "node.name") else { continue };
                graphe.sorties.push(Sortie {
                    description: texte(&p, "node.description").unwrap_or_else(|| nom.clone()),
                    par_defaut: defaut.as_deref() == Some(nom.as_str()),
                    nom,
                });
            }
            Some("Stream/Output/Audio") => {
                let client = p.get("client.id").and_then(Value::as_u64);
                if client.is_some_and(|c| clients.contains(&c)) {
                    graphe.flux = o.get("id").and_then(Value::as_u64).map(|i| i as u32);
                }
            }
            _ => {}
        }
    }
    graphe.sorties.sort_by(|a, b| a.nom.cmp(&b.nom));
    Some(graphe)
}

fn lire_graphe() -> Result<Graphe, Erreur> {
    let sortie = Command::new("pw-dump")
        .output()
        .map_err(|e| Erreur::Outil(format!("pw-dump : {e}")))?;
    if !sortie.status.success() {
        return Err(Erreur::Outil("pw-dump a échoué".into()));
    }
    analyser(&String::from_utf8_lossy(&sortie.stdout), std::process::id())
        .ok_or_else(|| Erreur::Outil("sortie de pw-dump illisible".into()))
}

/// Les sorties audio du système, triées par nom ; vide sans PipeWire.
pub fn lister() -> Vec<Sortie> {
    lire_graphe().map(|g| g.sorties).unwrap_or_default()
}

fn metadonnee(flux: u32, cle: &str, valeur: Option<&str>) -> Result<(), Erreur> {
    let mut cmd = Command::new("pw-metadata");
    cmd.args(["-n", "default", &flux.to_string(), cle]);
    match valeur {
        Some(v) => cmd.args([v, "Spa:String"]),
        None => cmd.arg("-d"),
    };
    let sortie = cmd.output().map_err(|e| Erreur::Outil(format!("pw-metadata : {e}")))?;
    if sortie.status.success() {
        Ok(())
    } else {
        Err(Erreur::Outil("pw-metadata a échoué".into()))
    }
}

/// Dirige le flux du lecteur vers la sortie `cible`, ou le remet sur la sortie par défaut du
/// système (`None` : la cible mémorisée est effacée).
pub fn diriger_flux(cible: Option<&str>) -> Result<(), Erreur> {
    let graphe = lire_graphe()?;
    let flux = graphe.flux.ok_or(Erreur::FluxAbsent)?;
    match cible {
        Some(nom) => {
            if !graphe.sorties.iter().any(|s| s.nom == nom) {
                return Err(Erreur::SortieAbsente(nom.to_string()));
            }
            metadonnee(flux, "target.object", Some(nom))
        }
        None => {
            metadonnee(flux, "target.object", None)?;
            metadonnee(flux, "target.node", None)
        }
    }
}

/// Comme `diriger_flux`, en attendant que le flux apparaisse dans le graphe (il n'existe qu'une
/// fois la sortie ouverte). À appeler hors du fil de l'interface.
pub fn diriger_flux_avec_attente(cible: Option<&str>) -> Result<(), Erreur> {
    let mut derniere = Erreur::FluxAbsent;
    for _ in 0..60 {
        match diriger_flux(cible) {
            Ok(()) => return Ok(()),
            Err(Erreur::FluxAbsent) => derniere = Erreur::FluxAbsent,
            Err(autre) => return Err(autre),
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(derniere)
}

/// Identifiants proposés quand on passe à la sortie suivante : « par défaut », puis chaque sortie
/// PipeWire dans l'ordre des noms.
pub fn cycle(sorties: &[Sortie]) -> Vec<String> {
    std::iter::once(String::new()).chain(sorties.iter().map(Sortie::identifiant)).collect()
}

/// L'identifiant qui suit `actuel` dans le cycle. Un réglage hors liste (ancien nom ALSA, sortie
/// débranchée) renvoie au début : « par défaut ».
pub fn suivante(sorties: &[Sortie], actuel: &str) -> String {
    let ids = cycle(sorties);
    match ids.iter().position(|i| i == actuel) {
        Some(rang) => ids[(rang + 1) % ids.len()].clone(),
        None => ids[0].clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extrait réduit d'un `pw-dump` : deux sorties, un client du processus 4242 avec son flux,
    /// un autre client (VLC) avec le sien, et la métadonnée de la sortie par défaut.
    const DUMP: &str = r#"[
      {"id":40,"type":"PipeWire:Interface:Metadata","metadata":[
        {"subject":0,"key":"default.audio.sink","type":"Spa:String:JSON","value":{"name":"bluez_output.JBL"}}]},
      {"id":107,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Sink",
        "node.name":"alsa_output.hdmi","node.description":"Écran HDMI"}}},
      {"id":119,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Sink",
        "node.name":"bluez_output.JBL","node.description":"JBL Flip 4"}}},
      {"id":70,"type":"PipeWire:Interface:Client","info":{"props":{"application.process.id":4242}}},
      {"id":71,"type":"PipeWire:Interface:Client","info":{"props":{"application.process.id":"999"}}},
      {"id":106,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Stream/Output/Audio",
        "client.id":70,"node.name":"alsa_playback.rmplayer"}}},
      {"id":86,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Stream/Output/Audio",
        "client.id":71,"node.name":"vlc"}}}
    ]"#;

    #[test]
    fn le_graphe_donne_les_sorties_leur_description_et_le_defaut() {
        let g = analyser(DUMP, 4242).unwrap();
        assert_eq!(g.sorties.len(), 2);
        assert_eq!(g.sorties[0].nom, "alsa_output.hdmi", "triées par nom");
        assert_eq!(g.sorties[0].description, "Écran HDMI");
        assert!(!g.sorties[0].par_defaut);
        assert!(g.sorties[1].par_defaut, "la JBL est la sortie par défaut");
        assert_eq!(g.sorties[1].identifiant(), "pw:bluez_output.JBL");
    }

    #[test]
    fn le_flux_est_celui_du_processus_et_pas_celui_d_un_autre() {
        assert_eq!(analyser(DUMP, 4242).unwrap().flux, Some(106));
        assert_eq!(analyser(DUMP, 999).unwrap().flux, Some(86), "pid donné en texte");
        assert_eq!(analyser(DUMP, 1).unwrap().flux, None, "aucun flux pour un autre processus");
    }

    #[test]
    fn un_dump_illisible_ou_vide_ne_plante_pas() {
        assert!(analyser("pas du json", 1).is_none());
        let g = analyser("[]", 1).unwrap();
        assert!(g.sorties.is_empty() && g.flux.is_none());
    }

    #[test]
    fn seuls_les_identifiants_prefixes_designent_pipewire() {
        assert_eq!(nom_pipewire("pw:bluez_output.JBL"), Some("bluez_output.JBL"));
        assert_eq!(nom_pipewire(""), None);
        assert_eq!(nom_pipewire("pw:"), None);
        assert_eq!(nom_pipewire("hw:CARD=Generic,DEV=0"), None, "un nom ALSA reste ALSA");
        assert_eq!(nom_pipewire("pipewire"), None);
    }

    #[test]
    fn seuls_les_noms_alsa_sont_ouverts_tels_quels() {
        assert_eq!(nom_alsa(""), None);
        assert_eq!(nom_alsa("pw:bluez_output.JBL"), None, "PipeWire : périphérique par défaut puis redirection");
        assert_eq!(nom_alsa("hw:CARD=Generic,DEV=0").as_deref(), Some("hw:CARD=Generic,DEV=0"));
        assert_eq!(nom_alsa("pipewire").as_deref(), Some("pipewire"));
    }

    fn sorties() -> Vec<Sortie> {
        ["a", "b", "c"]
            .iter()
            .map(|n| Sortie { nom: n.to_string(), description: n.to_string(), par_defaut: false })
            .collect()
    }

    #[test]
    fn le_cycle_commence_par_defaut_et_boucle() {
        let s = sorties();
        assert_eq!(cycle(&s), ["", "pw:a", "pw:b", "pw:c"]);
        assert_eq!(suivante(&s, ""), "pw:a");
        assert_eq!(suivante(&s, "pw:a"), "pw:b");
        assert_eq!(suivante(&s, "pw:c"), "", "après la dernière, retour à « par défaut »");
    }

    #[test]
    fn un_reglage_hors_liste_renvoie_a_par_defaut() {
        let s = sorties();
        assert_eq!(suivante(&s, "hw:CARD=Generic,DEV=0"), "", "ancien nom ALSA");
        assert_eq!(suivante(&s, "pw:debranchee"), "", "sortie disparue");
        assert_eq!(suivante(&[], "pw:a"), "", "sans PipeWire, il n'y a que « par défaut »");
    }
}

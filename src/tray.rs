//! Icône dans la zone de notification (protocole StatusNotifierItem, celui de KDE Plasma,
//! GNOME avec extension, XFCE…). Un clic affiche / masque la fenêtre ; le menu pilote la
//! lecture. Le service D-Bus tourne dans un fil à part (`ksni`) ; il ne touche jamais au lecteur :
//! il dépose des **commandes** que l'interface lit à chaque image.

use ksni::blocking::{Handle, TrayMethods};
use std::sync::mpsc::{channel, Receiver, Sender};

/// Ce que l'utilisateur a demandé depuis l'icône.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandeTray {
    LecturePause,
    Suivant,
    Precedent,
    AfficherMasquer,
    Quitter,
}

struct IconeRMPlayer {
    envoi: Sender<CommandeTray>,
    /// Pour réveiller l'interface : sans cela, une fenêtre au repos (lecture en pause) ne verrait
    /// la commande qu'au prochain événement.
    contexte: egui::Context,
    titre: String,
    en_lecture: bool,
}

impl IconeRMPlayer {
    fn demander(&self, c: CommandeTray) {
        let _ = self.envoi.send(c);
        self.contexte.request_repaint();
    }
}

impl ksni::Tray for IconeRMPlayer {
    fn id(&self) -> String {
        "rmplayer".into()
    }
    fn title(&self) -> String {
        "RMPlayer".into()
    }
    fn icon_name(&self) -> String {
        "rmplayer".into()
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "RMPlayer".into(),
            description: self.titre.clone(),
            ..Default::default()
        }
    }
    /// Clic principal : afficher ou masquer la fenêtre.
    fn activate(&mut self, _x: i32, _y: i32) {
        self.demander(CommandeTray::AfficherMasquer);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;
        let bouton = |libelle: &str, icone: &str, c: CommandeTray| -> ksni::MenuItem<Self> {
            StandardItem {
                label: libelle.into(),
                icon_name: icone.into(),
                activate: Box::new(move |t: &mut Self| t.demander(c)),
                ..Default::default()
            }
            .into()
        };
        vec![
            bouton(
                if self.en_lecture { "Pause" } else { "Lecture" },
                if self.en_lecture { "media-playback-pause" } else { "media-playback-start" },
                CommandeTray::LecturePause,
            ),
            bouton("Suivant", "media-skip-forward", CommandeTray::Suivant),
            bouton("Précédent", "media-skip-backward", CommandeTray::Precedent),
            MenuItem::Separator,
            bouton("Afficher / masquer la fenêtre", "window", CommandeTray::AfficherMasquer),
            bouton("Quitter", "application-exit", CommandeTray::Quitter),
        ]
    }
}

pub struct Tray {
    handle: Handle<IconeRMPlayer>,
    commandes: Receiver<CommandeTray>,
    dernier: (String, bool),
}

impl Tray {
    /// Publie l'icône sur le bus de session. Échec (pas de bus, pas de zone de notification) :
    /// l'erreur est rendue, le lecteur continue sans icône.
    pub fn demarrer(contexte: egui::Context) -> Result<Self, String> {
        let (envoi, commandes) = channel();
        let icone = IconeRMPlayer { envoi, contexte, titre: String::new(), en_lecture: false };
        let handle = icone.spawn().map_err(|e| e.to_string())?;
        Ok(Self { handle, commandes, dernier: (String::new(), false) })
    }

    /// Commandes reçues depuis le dernier appel.
    pub fn commandes(&self) -> Vec<CommandeTray> {
        self.commandes.try_iter().collect()
    }

    /// Met à jour l'infobulle et le libellé Lecture / Pause — seulement si quelque chose a changé.
    pub fn mettre_a_jour(&mut self, titre: &str, en_lecture: bool) {
        if self.dernier.0 == titre && self.dernier.1 == en_lecture {
            return;
        }
        self.dernier = (titre.to_string(), en_lecture);
        let titre = titre.to_string();
        self.handle.update(move |t: &mut IconeRMPlayer| {
            t.titre = titre;
            t.en_lecture = en_lecture;
        });
    }

    /// Retire l'icône du bus.
    pub fn arreter(self) {
        let _ = self.handle.shutdown();
    }
}

//! Raccourcis clavier et souris — **table unique**, personnalisable.
//!
//! Pourquoi un module : les raccourcis étaient écrits en dur dans un `match` de
//! 300 lignes, donc ni listables, ni vérifiables, ni modifiables. Ici chaque
//! **action** a un identifiant stable, un libellé et une ou plusieurs
//! combinaisons par défaut (clavier *ou* souris). La configuration ne contient
//! que les **écarts** par rapport aux défauts, ce qui rend l'ajout d'un raccourci
//! indolore : un réglage absent garde sa valeur par défaut.
//!
//! Tout est analysable sans interface (d'où les tests unitaires) : le banc
//! `tools/verif-raccourcis.sh` peut donc vérifier une personnalisation réelle.

use std::collections::BTreeMap;

/// Action déclenchable par un raccourci.
///
/// L'`id` est **persisté** dans la configuration : ne jamais le renommer sans
/// prévoir la reprise des réglages existants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    Fermer,
    LecturePause,
    ElementSuivant,
    ElementPrecedent,
    Reculer5s,
    Reculer1s,
    Reculer30s,
    Avancer5s,
    Avancer1s,
    Avancer30s,
    VolumePlus,
    VolumeMoins,
    Muet,
    Boucle,
    /// Lecture aléatoire (bascule).
    Aleatoire,
    Reprise,
    PointA,
    PointB,
    /// Marque-pages : poser, aller au suivant / précédent, supprimer celui de la position courante.
    SignetAjouter,
    SignetSuivant,
    SignetPrecedent,
    SignetSupprimer,
    VitesseMoins,
    VitessePlus,
    VitesseNormale,
    ChapitrePrecedent,
    ChapitreSuivant,
    ImageSuivante,
    ImagePrecedente,
    SousTitresVisibles,
    SousTitreSuivant,
    PleinEcran,
    Echap,
    Capture,
    Aspect,
    MiroirHorizontal,
    MiroirVertical,
    ZoneRecadrage,
    ToujoursAuDessus,
    MasquerInterface,
    Osd,
    Statistiques,
    PanneauPlaylist,
    AideRaccourcis,
    APropos,
    Preferences,
    DecodageMateriel,
}

impl Action {
    /// Toutes les actions, dans l'ordre d'affichage des réglages.
    pub const TOUTES: &'static [Action] = &[
        Action::LecturePause,
        Action::ElementSuivant,
        Action::ElementPrecedent,
        Action::Fermer,
        Action::Reculer5s,
        Action::Reculer1s,
        Action::Reculer30s,
        Action::Avancer5s,
        Action::Avancer1s,
        Action::Avancer30s,
        Action::Boucle,
        Action::Reprise,
        Action::PointA,
        Action::PointB,
        Action::SignetAjouter,
        Action::SignetSuivant,
        Action::SignetPrecedent,
        Action::SignetSupprimer,
        Action::VitesseMoins,
        Action::VitessePlus,
        Action::VitesseNormale,
        Action::ChapitrePrecedent,
        Action::ChapitreSuivant,
        Action::ImageSuivante,
        Action::ImagePrecedente,
        Action::VolumePlus,
        Action::VolumeMoins,
        Action::Muet,
        Action::SousTitresVisibles,
        Action::SousTitreSuivant,
        Action::PleinEcran,
        Action::Echap,
        Action::Capture,
        Action::Aspect,
        Action::MiroirHorizontal,
        Action::MiroirVertical,
        Action::ZoneRecadrage,
        Action::ToujoursAuDessus,
        Action::MasquerInterface,
        Action::Osd,
        Action::Statistiques,
        Action::PanneauPlaylist,
        Action::Aleatoire,
        Action::AideRaccourcis,
        Action::APropos,
        Action::Preferences,
        Action::DecodageMateriel,
    ];

    pub fn id(&self) -> &'static str {
        match self {
            Action::Fermer => "lecture.fermer",
            Action::LecturePause => "lecture.pause",
            Action::ElementSuivant => "lecture.suivant",
            Action::ElementPrecedent => "lecture.precedent",
            Action::Reculer5s => "nav.reculer5",
            Action::Reculer1s => "nav.reculer1",
            Action::Reculer30s => "nav.reculer30",
            Action::Avancer5s => "nav.avancer5",
            Action::Avancer1s => "nav.avancer1",
            Action::Avancer30s => "nav.avancer30",
            Action::VolumePlus => "son.plus",
            Action::VolumeMoins => "son.moins",
            Action::Muet => "son.muet",
            Action::Boucle => "lecture.boucle",
            Action::Aleatoire => "playlist.aleatoire",
            Action::Reprise => "lecture.reprise",
            Action::PointA => "lecture.point_a",
            Action::PointB => "lecture.point_b",
            Action::SignetAjouter => "signets.ajouter",
            Action::SignetSuivant => "signets.suivant",
            Action::SignetPrecedent => "signets.precedent",
            Action::SignetSupprimer => "signets.supprimer",
            Action::VitesseMoins => "vitesse.moins",
            Action::VitessePlus => "vitesse.plus",
            Action::VitesseNormale => "vitesse.normale",
            Action::ChapitrePrecedent => "chapitre.precedent",
            Action::ChapitreSuivant => "chapitre.suivant",
            Action::ImageSuivante => "image.suivante",
            Action::ImagePrecedente => "image.precedente",
            Action::SousTitresVisibles => "soustitres.visibles",
            Action::SousTitreSuivant => "soustitres.suivant",
            Action::PleinEcran => "affichage.plein_ecran",
            Action::Echap => "affichage.echap",
            Action::Capture => "image.capture",
            Action::Aspect => "image.aspect",
            Action::MiroirHorizontal => "image.miroir_h",
            Action::MiroirVertical => "image.miroir_v",
            Action::ZoneRecadrage => "image.zone_recadrage",
            Action::ToujoursAuDessus => "affichage.toujours_au_dessus",
            Action::MasquerInterface => "affichage.masquer_interface",
            Action::Osd => "affichage.osd",
            Action::Statistiques => "affichage.statistiques",
            Action::PanneauPlaylist => "playlist.panneau",
            Action::AideRaccourcis => "aide.raccourcis",
            Action::APropos => "aide.apropos",
            Action::Preferences => "reglages.preferences",
            Action::DecodageMateriel => "decodage.materiel",
        }
    }

    pub fn libelle(&self) -> &'static str {
        match self {
            Action::Fermer => "Fermer le lecteur",
            Action::LecturePause => "Lecture / pause",
            Action::ElementSuivant => "Élément suivant de la playlist",
            Action::ElementPrecedent => "Élément précédent de la playlist",
            Action::Reculer5s => "Reculer de 5 s",
            Action::Reculer1s => "Reculer de 1 s",
            Action::Reculer30s => "Reculer de 30 s",
            Action::Avancer5s => "Avancer de 5 s",
            Action::Avancer1s => "Avancer de 1 s",
            Action::Avancer30s => "Avancer de 30 s",
            Action::VolumePlus => "Volume +",
            Action::VolumeMoins => "Volume −",
            Action::Muet => "Couper / rétablir le son",
            Action::Boucle => "Lecture en boucle",
            Action::Aleatoire => "Lecture aléatoire (playlist)",
            Action::Reprise => "Reprise à la position quittée",
            Action::PointA => "Répétition A-B : point A",
            Action::PointB => "Répétition A-B : point B",
            Action::SignetAjouter => "Signet : ajouter à la position",
            Action::SignetSuivant => "Signet : aller au suivant",
            Action::SignetPrecedent => "Signet : aller au précédent",
            Action::SignetSupprimer => "Signet : supprimer celui de la position",
            Action::VitesseMoins => "Vitesse −0,1",
            Action::VitessePlus => "Vitesse +0,1",
            Action::VitesseNormale => "Vitesse normale (1,0×)",
            Action::ChapitrePrecedent => "Chapitre précédent",
            Action::ChapitreSuivant => "Chapitre suivant",
            Action::ImageSuivante => "Image suivante",
            Action::ImagePrecedente => "Image précédente",
            Action::SousTitresVisibles => "Afficher / masquer les sous-titres",
            Action::SousTitreSuivant => "Piste de sous-titres suivante",
            Action::PleinEcran => "Plein écran",
            Action::Echap => "Fermer la fenêtre / sortir du plein écran",
            Action::Capture => "Capture d'écran (PNG)",
            Action::Aspect => "Rapport d'aspect suivant",
            Action::MiroirHorizontal => "Miroir horizontal",
            Action::MiroirVertical => "Miroir vertical",
            Action::ZoneRecadrage => "Afficher / masquer la zone de recadrage",
            Action::ToujoursAuDessus => "Fenêtre toujours au-dessus",
            Action::MasquerInterface => "Masquer / afficher l'interface",
            Action::Osd => "Bandeau d'informations (OSD)",
            Action::Statistiques => "Fenêtre des statistiques",
            Action::PanneauPlaylist => "Panneau de playlist",
            Action::AideRaccourcis => "Aide des raccourcis",
            Action::APropos => "À propos",
            Action::Preferences => "Préférences",
            Action::DecodageMateriel => "Décodage matériel ↔ logiciel",
        }
    }

    pub fn categorie(&self) -> &'static str {
        match self {
            Action::LecturePause
            | Action::ElementSuivant
            | Action::ElementPrecedent
            | Action::Fermer
            | Action::Boucle
            | Action::Aleatoire
            | Action::Reprise
            | Action::PointA
            | Action::PointB
            | Action::SignetAjouter
            | Action::SignetSuivant
            | Action::SignetPrecedent
            | Action::SignetSupprimer => "Lecture",
            Action::Reculer5s
            | Action::Reculer1s
            | Action::Reculer30s
            | Action::Avancer5s
            | Action::Avancer1s
            | Action::Avancer30s => "Navigation",
            Action::VitesseMoins
            | Action::VitessePlus
            | Action::VitesseNormale
            | Action::ChapitrePrecedent
            | Action::ChapitreSuivant
            | Action::ImageSuivante
            | Action::ImagePrecedente => "Vitesse et chapitres",
            Action::VolumePlus | Action::VolumeMoins | Action::Muet => "Son",
            Action::SousTitresVisibles | Action::SousTitreSuivant => "Sous-titres",
            Action::PleinEcran
            | Action::Echap
            | Action::ToujoursAuDessus
            | Action::MasquerInterface => "Affichage",
            Action::Capture
            | Action::Aspect
            | Action::MiroirHorizontal
            | Action::MiroirVertical
            | Action::ZoneRecadrage
            | Action::DecodageMateriel => "Image",
            Action::Osd | Action::Statistiques | Action::PanneauPlaylist => "Fenêtres",
            Action::AideRaccourcis | Action::APropos | Action::Preferences => "Aide et réglages",
        }
    }

    /// Combinaisons par défaut, séparées par des virgules (`""` = aucune).
    pub fn defaut(&self) -> &'static str {
        match self {
            Action::Fermer => "Q",
            Action::LecturePause => "Espace, ClicMilieu",
            Action::ElementSuivant => "N",
            Action::ElementPrecedent => "P",
            Action::Reculer5s => "Gauche",
            Action::Reculer1s => "Maj+Gauche",
            Action::Reculer30s => "Ctrl+Gauche",
            Action::Avancer5s => "Droite",
            Action::Avancer1s => "Maj+Droite",
            Action::Avancer30s => "Ctrl+Droite",
            Action::VolumePlus => "Haut, MoletteHaut",
            Action::VolumeMoins => "Bas, MoletteBas",
            Action::Muet => "M",
            Action::Boucle => "Ctrl+L",
            Action::Aleatoire => "Ctrl+A",
            Action::Reprise => "Ctrl+R",
            Action::PointA => "A",
            Action::PointB => "B",
            Action::SignetAjouter => "Ctrl+B",
            Action::SignetSuivant => "Ctrl+Maj+Droite",
            Action::SignetPrecedent => "Ctrl+Maj+Gauche",
            Action::SignetSupprimer => "Ctrl+Maj+B",
            Action::VitesseMoins => "[",
            Action::VitessePlus => "]",
            Action::VitesseNormale => "\\",
            Action::ChapitrePrecedent => "PagePrec",
            Action::ChapitreSuivant => "PageSuiv",
            Action::ImageSuivante => ".",
            Action::ImagePrecedente => ",",
            Action::SousTitresVisibles => "V, DoubleClic",
            Action::SousTitreSuivant => "J",
            Action::PleinEcran => "F, Entree, F11",
            Action::Echap => "Echap",
            Action::Capture => "S",
            Action::Aspect => "K",
            Action::MiroirHorizontal => "Ctrl+H",
            Action::MiroirVertical => "Ctrl+Maj+H",
            Action::ZoneRecadrage => "R",
            Action::ToujoursAuDessus => "T",
            Action::MasquerInterface => "Tab",
            Action::Osd => "O",
            Action::Statistiques => "I",
            Action::PanneauPlaylist => "L",
            Action::AideRaccourcis => "F1",
            Action::APropos => "F2",
            Action::Preferences => "F5",
            Action::DecodageMateriel => "H",
        }
    }
}

/// Un bouton ou une molette de souris, utilisable comme raccourci.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Souris {
    MoletteHaut,
    MoletteBas,
    ClicMilieu,
    DoubleClic,
    ClicDroit,
}

/// La touche d'une combinaison : clavier ou souris.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Touche {
    Clavier(egui::Key),
    Souris(Souris),
}

/// Une combinaison complète : modificateurs + touche.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Combinaison {
    pub ctrl: bool,
    pub maj: bool,
    pub alt: bool,
    pub touche: Touche,
}

impl Combinaison {
    /// Comparaison avec un événement clavier d'egui.
    pub fn correspond_clavier(&self, key: egui::Key, ctrl: bool, maj: bool, alt: bool) -> bool {
        matches!(self.touche, Touche::Clavier(k) if k == key)
            && self.ctrl == ctrl
            && self.maj == maj
            && self.alt == alt
    }

    pub fn correspond_souris(&self, s: Souris) -> bool {
        matches!(self.touche, Touche::Souris(v) if v == s)
            // Une molette ne porte pas de modificateur dans egui : on n'exige rien
            // sur ceux qui n'en ont pas, mais un raccourci qui en demande un ne
            // doit pas se déclencher par hasard.
            && !self.ctrl
            && !self.maj
            && !self.alt
    }

    /// Texte normalisé, tel qu'il est écrit dans la configuration.
    pub fn texte(&self) -> String {
        let mut parties: Vec<String> = Vec::new();
        if self.ctrl {
            parties.push("Ctrl".to_string());
        }
        if self.maj {
            parties.push("Maj".to_string());
        }
        if self.alt {
            parties.push("Alt".to_string());
        }
        parties.push(match self.touche {
            Touche::Clavier(k) => nom_touche(k).to_string(),
            Touche::Souris(Souris::MoletteHaut) => "MoletteHaut".to_string(),
            Touche::Souris(Souris::MoletteBas) => "MoletteBas".to_string(),
            Touche::Souris(Souris::ClicMilieu) => "ClicMilieu".to_string(),
            Touche::Souris(Souris::DoubleClic) => "DoubleClic".to_string(),
            Touche::Souris(Souris::ClicDroit) => "ClicDroit".to_string(),
        });
        parties.join("+")
    }

    /// Libellé lisible pour l'interface (accents, espaces).
    pub fn affichage(&self) -> String {
        let mut parties: Vec<&str> = Vec::new();
        if self.ctrl {
            parties.push("Ctrl");
        }
        if self.maj {
            parties.push("Maj");
        }
        if self.alt {
            parties.push("Alt");
        }
        let touche = match self.touche {
            Touche::Clavier(k) => libelle_touche(k),
            Touche::Souris(Souris::MoletteHaut) => "molette ↑",
            Touche::Souris(Souris::MoletteBas) => "molette ↓",
            Touche::Souris(Souris::ClicMilieu) => "clic du milieu",
            Touche::Souris(Souris::DoubleClic) => "double-clic",
            Touche::Souris(Souris::ClicDroit) => "clic droit",
        };
        if parties.is_empty() {
            touche.to_string()
        } else {
            format!("{}+{}", parties.join("+"), touche)
        }
    }
}

/// Nom **stable** (sans accent, sans espace) d'une touche de clavier.
fn nom_touche(k: egui::Key) -> &'static str {
    use egui::Key::*;
    match k {
        ArrowDown => "Bas",
        ArrowLeft => "Gauche",
        ArrowRight => "Droite",
        ArrowUp => "Haut",
        Escape => "Echap",
        Tab => "Tab",
        Backspace => "Retour",
        Enter => "Entree",
        Space => "Espace",
        Insert => "Inser",
        Delete => "Suppr",
        Home => "Debut",
        End => "Fin",
        PageUp => "PagePrec",
        PageDown => "PageSuiv",
        Comma => ",",
        Period => ".",
        Semicolon => ";",
        Colon => ":",
        OpenBracket => "[",
        CloseBracket => "]",
        Backslash => "\\",
        Slash => "/",
        Pipe => "|",
        Questionmark => "?",
        Backtick => "`",
        Minus => "-",
        Plus => "+",
        Equals => "=",
        Quote => "'",
        Num0 => "0",
        Num1 => "1",
        Num2 => "2",
        Num3 => "3",
        Num4 => "4",
        Num5 => "5",
        Num6 => "6",
        Num7 => "7",
        Num8 => "8",
        Num9 => "9",
        A => "A",
        B => "B",
        C => "C",
        D => "D",
        E => "E",
        F => "F",
        G => "G",
        H => "H",
        I => "I",
        J => "J",
        K => "K",
        L => "L",
        M => "M",
        N => "N",
        O => "O",
        P => "P",
        Q => "Q",
        R => "R",
        S => "S",
        T => "T",
        U => "U",
        V => "V",
        W => "W",
        X => "X",
        Y => "Y",
        Z => "Z",
        F1 => "F1",
        F2 => "F2",
        F3 => "F3",
        F4 => "F4",
        F5 => "F5",
        F6 => "F6",
        F7 => "F7",
        F8 => "F8",
        F9 => "F9",
        F10 => "F10",
        F11 => "F11",
        F12 => "F12",
        F13 => "F13",
        F14 => "F14",
        F15 => "F15",
        F16 => "F16",
        F17 => "F17",
        F18 => "F18",
        F19 => "F19",
        F20 => "F20",
        _ => "?",
    }
}

/// Libellé lisible d'une touche (pour l'aide et les réglages).
pub fn libelle_touche(k: egui::Key) -> &'static str {
    use egui::Key::*;
    match k {
        ArrowDown => "↓",
        ArrowLeft => "←",
        ArrowRight => "→",
        ArrowUp => "↑",
        Escape => "Échap",
        Enter => "Entrée",
        PageUp => "Page↑",
        PageDown => "Page↓",
        Backspace => "Retour arrière",
        Insert => "Inser",
        Delete => "Suppr",
        Home => "Début",
        End => "Fin",
        other => nom_touche(other),
    }
}

/// Analyse une combinaison écrite par l'utilisateur (`"Ctrl+Maj+H"`).
///
/// Tolérant sur la casse, les espaces et quelques synonymes (`Shift` pour `Maj`,
/// `Esc` pour `Échap`), strict sur le reste : une combinaison incomprise est
/// signalée à l'utilisateur plutôt que silencieusement ignorée.
/// Retire les accents des noms de touches écrits en français (« Entrée »).
///
/// Sans cela, « Entrée » (accentué, comme l'affiche l'interface) ne serait pas
/// reconnu et le défaut `F, Entrée, F11` perdrait une de ses combinaisons —
/// exactement le genre de perte silencieuse que ce module doit éviter.
fn sans_accent(texte: &str) -> String {
    texte
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'û' | 'ü' | 'ù' => 'u',
            'ç' => 'c',
            autre => autre,
        })
        .collect()
}

pub fn analyser(texte: &str) -> Option<Combinaison> {
    let mut ctrl = false;
    let mut maj = false;
    let mut alt = false;
    let mut touche: Option<Touche> = None;

    for brut in texte.split('+') {
        // « + » et « Ctrl++ » : le dernier segment peut être la touche « + ».
        let morceau = brut.trim();
        if morceau.is_empty() {
            continue;
        }
        let m = sans_accent(&morceau.to_lowercase());
        match m.as_str() {
            "ctrl" | "control" => ctrl = true,
            "maj" | "shift" => maj = true,
            "alt" => alt = true,
            _ => {
                if touche.is_some() {
                    return None; // deux touches dans une combinaison
                }
                let touche_souris = match m.as_str() {
                    "molettehaut" | "wheelup" => Some(Souris::MoletteHaut),
                    "molettebas" | "wheeldown" => Some(Souris::MoletteBas),
                    "clicmilieu" | "middleclick" => Some(Souris::ClicMilieu),
                    "doubleclic" | "doubleclick" => Some(Souris::DoubleClic),
                    "clicdroit" | "rightclick" => Some(Souris::ClicDroit),
                    _ => None,
                };
                touche = Some(match touche_souris {
                    Some(s) => Touche::Souris(s),
                    None => Touche::Clavier(cle_depuis_nom(&m)?),
                });
            }
        }
    }
    Some(Combinaison {
        ctrl,
        maj,
        alt,
        touche: touche?,
    })
}

/// Retrouve une touche d'egui depuis son nom (avec synonymes).
fn cle_depuis_nom(nom: &str) -> Option<egui::Key> {
    use egui::Key::*;
    let k = match nom {
        "bas" | "down" => ArrowDown,
        "gauche" | "left" => ArrowLeft,
        "droite" | "right" => ArrowRight,
        "haut" | "up" => ArrowUp,
        "echap" | "esc" | "escape" => Escape,
        "tab" => Tab,
        "retour" | "backspace" => Backspace,
        "entree" | "enter" | "return" => Enter,
        "espace" | "space" => Space,
        "inser" | "insert" => Insert,
        "suppr" | "delete" | "del" => Delete,
        "debut" | "home" => Home,
        "fin" | "end" => End,
        "pageprec" | "pageup" | "pagehaut" => PageUp,
        "pagesuiv" | "pagedown" | "pagebas" => PageDown,
        "," | "virgule" => Comma,
        "." | "point" | "period" => Period,
        ";" | "pointvirgule" => Semicolon,
        ":" | "deuxpoints" => Colon,
        "[" | "crochetgauche" => OpenBracket,
        "]" | "crochtdroit" | "crochdroit" => CloseBracket,
        "\\" | "antislash" | "backslash" => Backslash,
        "/" | "slash" => Slash,
        "|" | "pipe" | "barre" => Pipe,
        "?" | "interrogation" => Questionmark,
        "`" | "backtick" | "grave" => Backtick,
        "-" | "moins" | "minus" => Minus,
        "=" | "egal" | "equals" => Equals,
        "'" | "apostrophe" | "quote" => Quote,
        "0" => Num0,
        "1" => Num1,
        "2" => Num2,
        "3" => Num3,
        "4" => Num4,
        "5" => Num5,
        "6" => Num6,
        "7" => Num7,
        "8" => Num8,
        "9" => Num9,
        "f1" => F1,
        "f2" => F2,
        "f3" => F3,
        "f4" => F4,
        "f5" => F5,
        "f6" => F6,
        "f7" => F7,
        "f8" => F8,
        "f9" => F9,
        "f10" => F10,
        "f11" => F11,
        "f12" => F12,
        // Une seule lettre : « a » → A. Accepte aussi la forme accentuée « altgr ».
        s if s.chars().count() == 1 => {
            let c = s.chars().next()?.to_ascii_uppercase();
            match c {
                'A' => A, 'B' => B, 'C' => C, 'D' => D, 'E' => E, 'F' => F, 'G' => G,
                'H' => H, 'I' => I, 'J' => J, 'K' => K, 'L' => L, 'M' => M, 'N' => N,
                'O' => O, 'P' => P, 'Q' => Q, 'R' => R, 'S' => S, 'T' => T, 'U' => U,
                'V' => V, 'W' => W, 'X' => X, 'Y' => Y, 'Z' => Z,
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(k)
}

/// Analyse une liste de combinaisons séparées par des virgules.
pub fn analyser_liste(texte: &str) -> Vec<Combinaison> {
    texte
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .filter_map(analyser)
        .collect()
}

/// Table des raccourcis effective : défauts, écrasés par la configuration.
#[derive(Debug, Clone, Default)]
pub struct Tableau {
    entrees: Vec<(Action, Vec<Combinaison>)>,
}

impl Tableau {
    /// Construit la table à partir des surcharges (`id d'action` → combinaisons).
    ///
    /// Une entrée absente garde son défaut ; une entrée présente mais **vide**
    /// désactive l'action (c'est ainsi qu'on libère une touche).
    pub fn construire(surcharges: &BTreeMap<String, String>) -> Self {
        let entrees = Action::TOUTES
            .iter()
            .map(|a| {
                let texte = surcharges
                    .get(a.id())
                    .map(String::as_str)
                    .unwrap_or_else(|| a.defaut());
                (*a, analyser_liste(texte))
            })
            .collect();
        Self { entrees }
    }

    /// Action déclenchée par un appui clavier, ou `None`.
    pub fn pour_clavier(&self, key: egui::Key, ctrl: bool, maj: bool, alt: bool) -> Option<Action> {
        self.entrees
            .iter()
            .find(|(_, combinaisons)| {
                combinaisons
                    .iter()
                    .any(|c| c.correspond_clavier(key, ctrl, maj, alt))
            })
            .map(|(a, _)| *a)
    }

    /// Action déclenchée par un bouton ou la molette, ou `None`.
    pub fn pour_souris(&self, s: Souris) -> Option<Action> {
        self.entrees
            .iter()
            .find(|(_, combinaisons)| combinaisons.iter().any(|c| c.correspond_souris(s)))
            .map(|(a, _)| *a)
    }

    pub fn combinaisons(&self, action: Action) -> &[Combinaison] {
        self.entrees
            .iter()
            .find(|(a, _)| *a == action)
            .map(|(_, c)| c.as_slice())
            .unwrap_or(&[])
    }

    /// Texte d'affichage d'une action (« Espace », « F, Entrée, F11 », « — »).
    pub fn affichage(&self, action: Action) -> String {
        let c = self.combinaisons(action);
        if c.is_empty() {
            "—".to_string()
        } else {
            c.iter()
                .map(|c| c.affichage())
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    /// Table des combinaisons **réellement en conflit** (même combinaison pour
    /// deux actions) : la première déclarée gagne, mais l'utilisateur doit le
    /// savoir — un raccourci qui « ne marche pas » sans explication est un défaut.
    pub fn conflits(&self) -> Vec<(Combinaison, Action, Action)> {
        let mut vus: Vec<(Combinaison, Action)> = Vec::new();
        let mut conflits = Vec::new();
        for (action, combinaisons) in &self.entrees {
            for c in combinaisons {
                if let Some((_, autre)) = vus.iter().find(|(v, _)| v == c) {
                    conflits.push((*c, *autre, *action));
                } else {
                    vus.push((*c, *action));
                }
            }
        }
        conflits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_lettre_simple_se_lit() {
        let c = analyser("a").unwrap();
        assert_eq!(c.touche, Touche::Clavier(egui::Key::A));
        assert!(!c.ctrl && !c.maj && !c.alt);
    }

    #[test]
    fn les_modificateurs_sont_reconnus_en_francais_et_en_anglais() {
        let c = analyser("Ctrl+Maj+H").unwrap();
        assert!(c.ctrl && c.maj && !c.alt);
        let d = analyser("shift+ctrl+h").unwrap();
        assert_eq!(c, d, "la casse et l'ordre ne doivent pas compter");
    }

    #[test]
    fn la_casse_et_les_espaces_sont_toleres() {
        assert_eq!(analyser(" Espace ").unwrap().touche, Touche::Clavier(egui::Key::Space));
        assert_eq!(analyser("page prec").is_none(), true, "« page prec » n'est pas une touche");
        assert_eq!(analyser("PagePrec").unwrap().touche, Touche::Clavier(egui::Key::PageUp));
        assert_eq!(analyser("pageup").unwrap().touche, Touche::Clavier(egui::Key::PageUp));
    }

    #[test]
    fn une_combinaison_incomprise_est_refusee() {
        assert!(analyser("").is_none());
        assert!(analyser("Ctrl").is_none(), "aucune touche");
        assert!(analyser("Ctrl+A+B").is_none(), "deux touches");
        assert!(analyser("ToucheQuiNExistePas").is_none());
    }

    #[test]
    fn les_touches_de_souris_sont_reconnues() {
        assert_eq!(analyser("MoletteHaut").unwrap().touche, Touche::Souris(Souris::MoletteHaut));
        assert_eq!(analyser("doubleclic").unwrap().touche, Touche::Souris(Souris::DoubleClic));
        assert_eq!(analyser("ClicMilieu").unwrap().touche, Touche::Souris(Souris::ClicMilieu));
    }

    #[test]
    fn une_liste_se_decompose_et_ignore_les_vides() {
        let l = analyser_liste("F, Entrée, F11");
        assert_eq!(l.len(), 3);
        assert_eq!(l[1].touche, Touche::Clavier(egui::Key::Enter));
        assert_eq!(analyser_liste(""), Vec::new());
        assert_eq!(analyser_liste(" , ").len(), 0);
    }

    #[test]
    fn le_texte_normalise_se_relit_a_l_identique() {
        // Aller-retour : ce qui est écrit dans la configuration doit être relu
        // exactement pareil (sinon un réglage se perdrait au redémarrage).
        for texte in ["Ctrl+Maj+H", "Espace", "MoletteHaut", "F11", "Ctrl+Gauche"] {
            let c = analyser(texte).unwrap();
            let relu = analyser(&c.texte()).unwrap();
            assert_eq!(c, relu, "aller-retour de « {} »", texte);
        }
    }

    #[test]
    fn le_tableau_par_defaut_repond_aux_defauts() {
        let t = Tableau::construire(&BTreeMap::new());
        assert_eq!(
            t.pour_clavier(egui::Key::Space, false, false, false),
            Some(Action::LecturePause)
        );
        // Shift+Gauche est une AUTRE action que Gauche (1 s au lieu de 5 s).
        assert_eq!(
            t.pour_clavier(egui::Key::ArrowLeft, false, true, false),
            Some(Action::Reculer1s)
        );
        assert_eq!(
            t.pour_clavier(egui::Key::ArrowLeft, false, false, false),
            Some(Action::Reculer5s)
        );
        // Ctrl+L n'est pas L.
        assert_eq!(
            t.pour_clavier(egui::Key::L, true, false, false),
            Some(Action::Boucle)
        );
        assert_eq!(
            t.pour_clavier(egui::Key::L, false, false, false),
            Some(Action::PanneauPlaylist)
        );
    }

    #[test]
    fn une_surcharge_remplace_le_defaut() {
        let mut sur = BTreeMap::new();
        sur.insert("lecture.pause".to_string(), "X".to_string());
        let t = Tableau::construire(&sur);
        assert_eq!(
            t.pour_clavier(egui::Key::X, false, false, false),
            Some(Action::LecturePause)
        );
        // Le défaut ne répond plus : la touche Espace est libérée.
        assert_eq!(t.pour_clavier(egui::Key::Space, false, false, false), None);
    }

    #[test]
    fn une_surcharge_vide_desactive_l_action() {
        let mut sur = BTreeMap::new();
        sur.insert("image.capture".to_string(), String::new());
        let t = Tableau::construire(&sur);
        assert_eq!(t.pour_clavier(egui::Key::S, false, false, false), None);
        assert_eq!(t.affichage(Action::Capture), "—");
    }

    #[test]
    fn les_raccourcis_souris_repondent() {
        let t = Tableau::construire(&BTreeMap::new());
        assert_eq!(t.pour_souris(Souris::MoletteHaut), Some(Action::VolumePlus));
        assert_eq!(t.pour_souris(Souris::MoletteBas), Some(Action::VolumeMoins));
        assert_eq!(t.pour_souris(Souris::ClicMilieu), Some(Action::LecturePause));
        assert_eq!(t.pour_souris(Souris::DoubleClic), Some(Action::SousTitresVisibles));
        // Le clic droit est réservé au menu contextuel : il ne doit rien déclencher.
        assert_eq!(t.pour_souris(Souris::ClicDroit), None);
    }

    #[test]
    fn une_molette_ne_declenche_pas_un_raccourci_avec_modificateur() {
        let mut sur = BTreeMap::new();
        sur.insert("son.plus".to_string(), "Ctrl+MoletteHaut".to_string());
        let t = Tableau::construire(&sur);
        // La molette n'a pas de modificateur dans egui : on refuse, plutôt que de
        // déclencher au hasard.
        assert_eq!(t.pour_souris(Souris::MoletteHaut), None);
    }

    #[test]
    fn aucun_conflit_dans_les_defauts() {
        let t = Tableau::construire(&BTreeMap::new());
        let conflits = t.conflits();
        assert!(
            conflits.is_empty(),
            "conflits de raccourcis par défaut : {:?}",
            conflits
                .iter()
                .map(|(c, a, b)| (c.affichage(), a.id(), b.id()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn un_conflit_introduit_par_l_utilisateur_est_signale() {
        let mut sur = BTreeMap::new();
        sur.insert("image.capture".to_string(), "Espace".to_string());
        let t = Tableau::construire(&sur);
        let c = t.conflits();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].0.affichage(), "Espace");
    }

    #[test]
    fn chaque_action_a_un_identifiant_unique_et_non_vide() {
        let mut ids: Vec<&str> = Action::TOUTES.iter().map(|a| a.id()).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "deux actions partagent le même id");
        assert!(ids.iter().all(|i| !i.is_empty()));
    }

    #[test]
    fn les_defauts_ne_contiennent_que_des_combinaisons_valides() {
        for a in Action::TOUTES {
            let texte = a.defaut();
            if texte.is_empty() {
                continue;
            }
            let lues = analyser_liste(texte);
            let attendues = texte.split(',').filter(|t| !t.trim().is_empty()).count();
            assert_eq!(
                lues.len(),
                attendues,
                "défaut illisible pour {} : « {} »",
                a.id(),
                texte
            );
        }
    }
}

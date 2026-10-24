//! Fenêtre « Préférences » — une section par domaine, dépliable/repliable.
//!
//! **Pourquoi ce module existe.** La fenêtre était une suite de `ui.heading` dans
//! `ui.rs` : elle grandissait à chaque réglage ajouté et finissait **hors de
//! l'écran** (le contenu n'était pas borné, ni défilable). Ajouter un paramètre
//! demandait de retrouver le bon endroit dans 300 lignes.
//!
//! **Architecture.** Chaque domaine est une `Section` **déclarée dans la table
//! `SECTIONS`** : identifiant stable, titre, icône, phrase d'explication, état
//! déplié par défaut, fonction de dessin et fonction « ce domaine est-il modifié ? ».
//! Ajouter un réglage (par exemple l'égaliseur audio, ou des courbes de filtres
//! personnalisées) = **une entrée dans la table + une fonction de dessin**. Rien
//! d'autre à modifier : la fenêtre, le défilement, le bornage à l'écran, les
//! boutons « tout déplier / tout replier » et le compteur de sections modifiées
//! s'appliquent automatiquement.
//!
//! **Règles de lisibilité tenues ici :**
//! - le contenu est dans une `ScrollArea` de hauteur bornée : **la fenêtre ne peut
//!   pas sortir de l'écran**, même avec beaucoup de sections dépliées ;
//! - chaque section a un bandeau cliquable (chevron + icône + titre) ;
//! - à l'intérieur, chaque sous-bloc a un titre discret et un filet de séparation ;
//! - les réglages à curseur sont alignés dans une grille (libellés jamais rognés) ;
//! - une phrase d'explication accompagne les réglages dont l'effet n'est pas évident.

use eframe::egui;

use crate::config::{Config, DeinterlaceParity, EqPreset, ScalingMethod};
use crate::ui::App;

/// Un domaine de réglages.
pub struct Section {
    /// Identifiant stable : il porte l'état déplié/replié (mémorisé par egui).
    pub id: &'static str,
    pub icon: &'static str,
    pub title: &'static str,
    /// Phrase affichée sous le titre : à quoi sert ce domaine.
    pub hint: &'static str,
    pub default_open: bool,
    /// Vrai si au moins un réglage du domaine diffère des valeurs par défaut.
    pub modified: fn(&App) -> bool,
    /// Dessine le corps de la section.
    pub draw: fn(&mut App, &mut egui::Ui),
}

/// Toutes les sections, dans l'ordre d'affichage.
pub const SECTIONS: &[Section] = &[
    Section {
        id: "profils",
        icon: "⭐",
        title: "Profils de réglages",
        hint: "Enregistrer et rappeler un ensemble de réglages (film sombre, anime…)",
        default_open: true,
        modified: |_| false,
        draw: dessiner_profils,
    },
    Section {
        id: "interface",
        icon: "🎨",
        title: "Interface",
        hint: "Apparence, contrôles à l'écran, dossier des captures",
        default_open: true,
        modified: interface_modifiee,
        draw: dessiner_interface,
    },
    Section {
        id: "video",
        icon: "🎬",
        title: "Vidéo",
        hint: "Décodage matériel, mise à l'échelle, netteté, géométrie",
        default_open: true,
        modified: video_modifiee,
        draw: dessiner_video,
    },
    Section {
        id: "image",
        icon: "🖼",
        title: "Image — filtres",
        hint: "Détramage, débruitage, désentrelacement, réglages manuels",
        default_open: true,
        modified: image_modifiee,
        draw: dessiner_image,
    },
    Section {
        id: "audio",
        icon: "🔊",
        title: "Audio",
        hint: "Périphérique de sortie, volume, égaliseur 18 bandes",
        default_open: false,
        modified: audio_modifiee,
        draw: dessiner_audio,
    },
    Section {
        id: "subtitles",
        icon: "📝",
        title: "Sous-titres",
        hint: "Chargement, apparence du texte, synchronisation",
        default_open: false,
        modified: sous_titres_modifies,
        draw: dessiner_sous_titres,
    },
    Section {
        id: "raccourcis",
        icon: "⌨",
        title: "Raccourcis clavier et souris",
        hint: "Changer une touche, un bouton ou la molette ; remettre les valeurs par défaut",
        default_open: false,
        modified: raccourcis_modifies,
        draw: dessiner_raccourcis,
    },
    Section {
        id: "playback",
        icon: "▶",
        title: "Lecture",
        hint: "Reprise de position, vitesse",
        default_open: false,
        modified: lecture_modifiee,
        draw: dessiner_lecture,
    },
];

/// Exporter / importer **toute** la configuration (réglages, raccourcis, règles, fichiers
/// récents) : mêmes fonctions que `--export-config` / `--import-config`, donc mêmes
/// vérifications. Les profils ont leur propre export (boutons ci-dessus).
fn sauvegarde_configuration(app: &mut App, ui: &mut egui::Ui) {
    aide(
        ui,
        "Exporter écrit un fichier .json qu'on peut copier sur une autre machine ; importer \
         remplace tous les réglages par ceux du fichier (refusé sans rien changer s'il n'est \
         pas une configuration de KPlayer).",
    );
    ui.horizontal(|ui| {
        if ui.button("⬆ Exporter la configuration…").clicked() {
            if let Some(chemin) = rfd::FileDialog::new()
                .set_file_name("kplayer-configuration.json")
                .add_filter("Configuration KPlayer", &["json"])
                .save_file()
            {
                match crate::config::exporter_config(&app.config, &chemin) {
                    Ok(()) => {
                        tracing::info!(target: "kinterface", "configuration exportée : {:?}", chemin);
                        app.annonce("Configuration exportée");
                    }
                    Err(e) => app.error_message = Some(format!("Export : {}", e)),
                }
            }
        }
        if ui.button("⬇ Importer une configuration…").clicked() {
            if let Some(chemin) = rfd::FileDialog::new()
                .add_filter("Configuration KPlayer", &["json"])
                .pick_file()
            {
                app.importer_configuration(&chemin);
            }
        }
    });
}

/// Curseur d'un réglage de sous-titre ; retourne vrai s'il a changé (l'appelant enregistre).
fn curseur_st(
    ui: &mut egui::Ui,
    grille: &str,
    libelle: &str,
    survol: &str,
    valeur: &mut f32,
    plage: std::ops::RangeInclusive<f32>,
    suffixe: &str,
    decimales: usize,
) -> bool {
    let mut change = false;
    ligne(ui, grille, libelle, |ui| {
        change = ui
            .add(egui::Slider::new(valeur, plage).suffix(suffixe).fixed_decimals(decimales))
            .on_hover_text(survol)
            .changed();
    });
    change
}

/// Champ de couleur RRGGBB avec aperçu ; retourne vrai si le texte a changé.
fn champ_couleur_st(ui: &mut egui::Ui, grille: &str, libelle: &str, valeur: &mut String) -> bool {
    let mut change = false;
    ligne(ui, grille, libelle, |ui| {
        ui.horizontal(|ui| {
            change = ui.add(egui::TextEdit::singleline(valeur).desired_width(70.0)).changed();
            if let Some(c) = parse_couleur_ui(valeur) {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(22.0, 16.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2, c);
            }
        });
    });
    change
}

/// Vrai si un raccourci diffère de sa valeur par défaut.
fn raccourcis_modifies(app: &App) -> bool {
    !app.config.raccourcis.is_empty()
}

/// Section « Raccourcis » : une ligne par action, avec capture directe.
fn dessiner_raccourcis(app: &mut App, ui: &mut egui::Ui) {
    aide(
        ui,
        "Cliquez sur « Changer », puis appuyez sur la touche (ou utilisez la molette, \
         le clic du milieu, un double-clic). Échap annule. La touche Retour arrière \
         désactive le raccourci.",
    );
    // Conflits : deux actions sur la même combinaison. La première gagne, mais il
    // faut le dire — un raccourci « qui ne marche pas » sans explication est un défaut.
    let conflits = app.raccourcis.conflits();
    if !conflits.is_empty() {
        for (combinaison, garde, ecrasee) in conflits {
            ui.label(
                egui::RichText::new(format!(
                    "⚠ {} est utilisé par « {} » et « {} » : seul le premier répond",
                    combinaison.affichage(),
                    garde.libelle(),
                    ecrasee.libelle()
                ))
                .color(egui::Color32::from_rgb(220, 160, 60)),
            );
        }
    }
    if app.capture_raccourci.is_some() {
        ui.label(
            egui::RichText::new("⏳ Appuyez sur la combinaison à associer (Échap : annuler)…")
                .strong()
                .color(egui::Color32::from_rgb(120, 200, 255)),
        );
    }

    let mut categorie_courante = "";
    for action in crate::raccourcis::Action::TOUTES {
        if action.categorie() != categorie_courante {
            categorie_courante = action.categorie();
            ui.add_space(6.0);
            ui.label(egui::RichText::new(categorie_courante).strong());
        }
        ui.horizontal(|ui| {
            ui.add_sized(
                [240.0, 20.0],
                egui::Label::new(action.libelle()).truncate(),
            );
            let actuelle = app.raccourcis.affichage(*action);
            let en_capture = app.capture_raccourci == Some(*action);
            let libelle = if en_capture {
                "⏳ … ".to_string()
            } else {
                actuelle
            };
            ui.add_sized(
                [170.0, 20.0],
                egui::Label::new(egui::RichText::new(libelle).monospace()).truncate(),
            );
            if en_capture {
                if ui.button("Annuler").clicked() {
                    app.capture_raccourci = None;
                }
            } else if ui.button("Changer").clicked() {
                app.capture_raccourci = Some(*action);
                // Journalisé : le banc vérifie la capture sans deviner l'état.
                tracing::info!(target: "kinterface", "capture de raccourci : {}", action.id());
            }
            // Valeur par défaut, rappelée à côté : on sait toujours d'où l'on part.
            if ui
                .add(egui::Button::new("↺").small())
                .on_hover_text(format!("Défaut : {}", action.defaut()))
                .clicked()
            {
                app.config.raccourcis.remove(action.id());
                let _ = app.config.save();
                app.raccourcis = crate::raccourcis::Tableau::construire(&app.config.raccourcis);
                tracing::info!(target: "kinterface", "raccourci réinitialisé : {}", action.id());
            }
        });
    }

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("↺ Tout remettre par défaut").clicked() {
            app.config.raccourcis.clear();
            let _ = app.config.save();
            app.raccourcis = crate::raccourcis::Tableau::construire(&app.config.raccourcis);
            tracing::info!(target: "kinterface", "tous les raccourcis réinitialisés");
        }
        ui.label(
            egui::RichText::new("Le clic droit (menu contextuel) n'est pas remappable.")
                .small()
                .weak(),
        );
    });
}

/// Enregistre une combinaison capturée pour une action (ou la retire).
pub fn definir_raccourci(app: &mut App, action: crate::raccourcis::Action, combinaisons: &str) {
    let texte = combinaisons.trim().to_string();
    app.config.raccourcis.insert(action.id().to_string(), texte.clone());
    let _ = app.config.save();
    app.raccourcis = crate::raccourcis::Tableau::construire(&app.config.raccourcis);
    tracing::info!(
        target: "kinterface",
        "raccourci {} défini sur « {} »",
        action.id(),
        if texte.is_empty() { "(aucun)" } else { &texte }
    );
    app.annonce(&format!(
        "{} : {}",
        action.libelle(),
        if texte.is_empty() {
            "raccourci désactivé".to_string()
        } else {
            texte
        }
    ));
}

// ---------------------------------------------------------------------------
// Fenêtre
// ---------------------------------------------------------------------------

/// Dessine la fenêtre et renvoie le nouvel état « ouverte » (✕ ou Échap).
pub fn window(ctx: &egui::Context, app: &mut App) -> bool {
    let mut ouvert = app.show_settings;

    // Hauteur maximale : l'écran moins une marge en haut ET en bas. C'est ce qui
    // garantit que la fenêtre (bandeau de titre compris) reste entièrement visible.
    let ecran = ctx.content_rect();
    let hauteur_max = (ecran.height() - 100.0).max(260.0);

    let reponse = egui::Window::new("⚙ Préférences")
        .open(&mut ouvert)
        .collapsible(true)
        .resizable(true)
        .default_pos([24.0, 24.0])
        .default_size([540.0, hauteur_max.min(660.0)])
        .min_width(430.0)
        .max_height(hauteur_max)
        .show(ctx, |ui| {
            // Bandeau fixe (résumé + actions) : il ne défile pas, d'où le filet qui
            // le sépare clairement du contenu — sans lui, le texte défilant venait
            // coller aux boutons « tout déplier / tout replier ».
            barre_outils(ui, app);
            ui.separator();
            ui.add_space(2.0);

            // Le corps défile : c'est lui qui absorbe l'ajout de sections.
            //
            // ATTENTION : ne PAS déduire cette hauteur de `ui.available_height()`.
            // La fenêtre se dimensionne sur son contenu, qui est justement cette zone
            // défilante : `available_height` dépend donc de la taille choisie à
            // l'image précédente, et la fenêtre se réduisait à chaque image
            // (constaté : contenu coupé au milieu de la troisième section).
            // La borne est fixée sur la hauteur de l'écran, hors de cette boucle.
            let hauteur_corps = (hauteur_max - 100.0).max(180.0);
            // `auto_shrink` vertical = true : la zone prend la hauteur du CONTENU,
            // plafonnée par `max_height`. La hauteur de la fenêtre est donc une
            // fonction directe du contenu (aucun retour sur elle-même), et elle
            // cesse de grandir dès que le plafond est atteint — c'est là que le
            // défilement prend le relais, avec une barre visible.
            egui::ScrollArea::vertical()
                .id_salt("preferences_sections")
                .auto_shrink([false, true])
                .max_height(hauteur_corps)
                .show(ui, |ui| {
                    for section in SECTIONS {
                        dessiner_section(ui, section, app);
                        ui.add_space(4.0);
                    }
                });
        });

    if let Some(r) = &reponse {
        // Géométrie journalisée (debug) : l'audit s'en sert pour cliquer le ✕ sans
        // deviner les pixels, et pour vérifier que la fenêtre tient dans l'écran.
        tracing::debug!(
            target: "kinterface",
            "fenetre preférences rect {:?}",
            r.response.rect
        );
    }
    ouvert
}

/// Bandeau du haut : résumé + actions globales.
fn barre_outils(ui: &mut egui::Ui, app: &mut App) {
    let modifiees = SECTIONS.iter().filter(|s| (s.modified)(app)).count();
    ui.horizontal(|ui| {
        if modifiees == 0 {
            ui.label(egui::RichText::new("réglages par défaut").small().weak());
        } else {
            ui.label(
                egui::RichText::new(format!(
                    "{} section{} modifiée{}",
                    modifiees,
                    if modifiees > 1 { "s" } else { "" },
                    if modifiees > 1 { "s" } else { "" }
                ))
                .small()
                .color(ui.visuals().warn_fg_color),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let tout_replier = ui
                .small_button("tout replier")
                .on_hover_text("Replier toutes les sections");
            if tout_replier.clicked() {
                basculer_tout(ui.ctx(), false);
            }
            let tout_deplier = ui
                .small_button("tout déplier")
                .on_hover_text("Déplier toutes les sections");
            if tout_deplier.clicked() {
                basculer_tout(ui.ctx(), true);
            }
            tracing::debug!(
                target: "kinterface",
                "prefs boutons replier {:?} deplier {:?}",
                tout_replier.rect,
                tout_deplier.rect
            );
        });
    });
}

/// Ouvre ou ferme toutes les sections d'un coup.
fn basculer_tout(ctx: &egui::Context, ouvert: bool) {
    for section in SECTIONS {
        let id = egui::Id::new(("preferences_section", section.id));
        let mut etat =
            egui::collapsing_header::CollapsingState::load_with_default_open(ctx, id, true);
        etat.set_open(ouvert);
        etat.store(ctx);
    }
}

/// Une section : bandeau cliquable, puis corps déplié.
fn dessiner_section(ui: &mut egui::Ui, section: &Section, app: &mut App) {
    let id = egui::Id::new(("preferences_section", section.id));
    let mut etat =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, section.default_open);
    let ouvert = etat.is_open();
    let modifie = (section.modified)(app);
    let mut basculer = false;

    let fond = if ouvert {
        ui.visuals().faint_bg_color
    } else {
        egui::Color32::TRANSPARENT
    };

    egui::Frame::new()
        .fill(fond)
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            let bandeau = ui.horizontal(|ui| {
                let chevron = if ouvert { "▾" } else { "▸" };
                if ui
                    .add(
                        egui::Label::new(egui::RichText::new(chevron).monospace())
                            .sense(egui::Sense::click()),
                    )
                    .clicked()
                {
                    basculer = true;
                }
                let titre = ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!("{}  {}", section.icon, section.title))
                            .strong()
                            .size(15.0),
                    )
                    .sense(egui::Sense::click()),
                );
                if titre.clicked() {
                    basculer = true;
                }
                if modifie {
                    ui.label(
                        egui::RichText::new("• modifié")
                            .small()
                            .color(ui.visuals().warn_fg_color),
                    );
                }
            });
            // Géométrie du bandeau journalisée (debug) : l'audit clique dessus sans
            // deviner les pixels, et vérifie l'état déplié/replié sans lire l'image.
            tracing::debug!(
                target: "kinterface",
                "prefs section {} rect {:?} ouvert={}",
                section.id,
                bandeau.response.rect,
                ouvert
            );
            // Phrase d'explication : toujours visible, même replié (on sait ce qu'on
            // ouvre sans avoir à cliquer).
            ui.label(egui::RichText::new(section.hint).small().weak());

            if ouvert {
                ui.add_space(2.0);
                ui.separator();
                ui.add_space(4.0);
                (section.draw)(app, ui);
            }
        });

    if basculer {
        etat.set_open(!ouvert);
        etat.store(ui.ctx());
    }
}

// ---------------------------------------------------------------------------
// Petits éléments de mise en page (réutilisables par toute nouvelle section)
// ---------------------------------------------------------------------------

/// Sous-bloc : titre discret + filet. Sert à séparer « Apparence » de « Contrôles ».
fn sous_section(ui: &mut egui::Ui, titre: &str, contenu: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(titre)
            .small()
            .strong()
            .color(ui.visuals().weak_text_color()),
    );
    ui.separator();
    ui.add_space(2.0);
    contenu(ui);
}

/// Phrase d'aide sous un réglage.
fn aide(ui: &mut egui::Ui, texte: &str) {
    ui.label(egui::RichText::new(texte).small().weak());
}

/// Exemple concret dans le champ « motif » d'une règle : sans exemple, on ne sait
/// pas si l'on doit écrire un dossier, une extension ou une résolution.
fn motif_regle_aide(champ: crate::config::ChampRegle) -> &'static str {
    match champ {
        crate::config::ChampRegle::Chemin => "anime, ~/Films/4K…",
        crate::config::ChampRegle::Extension => "mkv, avi…",
        crate::config::ChampRegle::Resolution => "1920, ≥1920, <1280, 1280x720…",
        crate::config::ChampRegle::Inconnu => "",
    }
}

/// Ligne « libellé + réglage » alignée sur une grille : le libellé garde sa place,
/// le réglage prend la largeur restante (jamais de libellé rogné).
fn ligne(ui: &mut egui::Ui, grille: &str, libelle: &str, contenu: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(grille)
        .num_columns(2)
        .spacing([10.0, 8.0])
        .show(ui, |ui| {
            ui.label(libelle);
            contenu(ui);
            ui.end_row();
        });
}

// ---------------------------------------------------------------------------
// Sections
// ---------------------------------------------------------------------------

/// Section « Profils de réglages ».
///
/// Un profil capture les réglages de **rendu** (vidéo, audio, sous-titres) et les
/// rappelle d'un clic. L'état est tenu par l'application (`app.profiles`) et écrit
/// dans `profiles.json` à chaque modification.
fn dessiner_profils(app: &mut App, ui: &mut egui::Ui) {
    sous_section(ui, "Enregistrer l'état courant", |ui| {
        ui.horizontal(|ui| {
            ui.label("Nom");
            let champ = ui.add(
                egui::TextEdit::singleline(&mut app.profil_nom)
                    .desired_width(220.0)
                    .hint_text("Film sombre, Anime, DVD entrelacé…"),
            );
            tracing::debug!(
                target: "kinterface",
                "prefs champ nom profil rect {:?}",
                champ.rect
            );
            let nom = app.profil_nom.trim().to_string();
            let possible = !nom.is_empty();
            let bouton = ui
                .add_enabled(possible, egui::Button::new("💾 Enregistrer"))
                .on_hover_text("Capture les réglages vidéo, audio et sous-titres actuels");
            tracing::debug!(
                target: "kinterface",
                "prefs bouton enregistrer profil rect {:?}",
                bouton.rect
            );
            if bouton.clicked() {
                let profil = crate::config::Profil::capture(&nom, &app.config);
                tracing::info!(
                    target: "kinterface",
                    "profil enregistré : « {} » ({})",
                    nom,
                    profil.resume()
                );
                if let Some(existant) = app.profiles.iter_mut().find(|p| p.name == nom) {
                    *existant = profil;
                } else {
                    app.profiles.push(profil);
                }
                app.profiles.sort_by(|a, b| a.name.cmp(&b.name));
                match crate::config::save_profiles(&app.profiles) {
                    Ok(()) => app.annonce(&format!("Profil « {} » enregistré", nom)),
                    Err(e) => {
                        tracing::warn!("Enregistrement des profils impossible : {}", e);
                        app.error_message = Some(format!("Profils : {}", e));
                    }
                }
                app.profil_nom.clear();
            }
        });
        aide(
            ui,
            "Un profil retient la vidéo (filtres, cadrage, aspect), l'audio (égaliseur, \
             traitements) et les sous-titres (style, langue) — pas l'interface ni le \
             périphérique de sortie son, propres à la machine.",
        );
    });

    sous_section(ui, "Profils enregistrés", |ui| {
        if app.profiles.is_empty() {
            ui.label(
                egui::RichText::new("aucun profil — enregistrez l'état courant ci-dessus")
                    .weak(),
            );
            return;
        }
        // Copie de la liste : on peut supprimer/renommer en la parcourant.
        let profils = app.profiles.clone();
        let mut a_appliquer: Option<usize> = None;
        let mut a_supprimer: Option<usize> = None;
        let mut a_exporter: Option<usize> = None;
        for (i, profil) in profils.iter().enumerate() {
            let actif = app.profil_actif.as_deref() == Some(profil.name.as_str());
            ui.horizontal(|ui| {
                ui.label(if actif { "●" } else { "○" });
                ui.strong(&profil.name);
                ui.label(egui::RichText::new(profil.resume()).small().weak());
            });
            ui.horizontal(|ui| {
                let appliquer = ui.button("▶ Appliquer");
                tracing::debug!(
                    target: "kinterface",
                    "prefs profil {} bouton appliquer rect {:?}",
                    i,
                    appliquer.rect
                );
                if appliquer.clicked() {
                    a_appliquer = Some(i);
                }
                if ui.button("⬆ Exporter…").clicked() {
                    a_exporter = Some(i);
                }
                if ui.button("🗑 Supprimer").clicked() {
                    a_supprimer = Some(i);
                }
            });
            if i + 1 < profils.len() {
                ui.separator();
            }
        }
        if let Some(i) = a_appliquer {
            let profil = profils[i].clone();
            profil.appliquer(&mut app.config);
            // Tout rejouer sur le moteur : filtres vidéo, chaîne audio, sous-titres.
            app.appliquer_filtres();
            // `set_audio_filters` (et non `refresh_audio_chain`) : seul le premier
            // relit la configuration. `refresh_audio_chain` recalcule à partir des
            // réglages que le lecteur détient DÉJÀ — l'égaliseur du profil
            // n'atteignait donc pas le moteur (défaut constaté en session 27).
            app.player.set_audio_filters(&app.config.audio);
            app.profil_actif = Some(profil.name.clone());
            let _ = app.config.save();
            tracing::info!(
                target: "kinterface",
                "profil appliqué : « {} » ({})",
                profil.name,
                profil.resume()
            );
            app.annonce(&format!("Profil « {} » appliqué", profil.name));
        }
        if let Some(i) = a_supprimer {
            let nom = profils[i].name.clone();
            app.profiles.retain(|p| p.name != nom);
            if app.profil_actif.as_deref() == Some(nom.as_str()) {
                app.profil_actif = None;
            }
            match crate::config::save_profiles(&app.profiles) {
                Ok(()) => {
                    tracing::info!(target: "kinterface", "profil supprimé : « {} »", nom);
                    app.annonce(&format!("Profil « {} » supprimé", nom));
                }
                Err(e) => tracing::warn!("Suppression impossible : {}", e),
            }
        }
        if let Some(i) = a_exporter {
            let profil = profils[i].clone();
            if let Some(chemin) = rfd::FileDialog::new()
                .set_file_name(format!("kplayer-{}.json", profil.name))
                .add_filter("Profil KPlayer", &["json"])
                .save_file()
            {
                let contenu = serde_json::to_string_pretty(&profil).unwrap_or_default();
                match std::fs::write(&chemin, contenu) {
                    Ok(()) => {
                        tracing::info!(target: "kinterface", "profil exporté : {:?}", chemin);
                        app.annonce("Profil exporté");
                    }
                    Err(e) => app.error_message = Some(format!("Export : {}", e)),
                }
            }
        }
    });

    sous_section(ui, "Profil appliqué automatiquement", |ui| {
        if app.profiles.is_empty() {
            ui.label(
                egui::RichText::new(
                    "enregistrez d'abord un profil : une règle désigne un profil par son nom",
                )
                .weak(),
            );
            return;
        }

        // Liste des règles — la première qui correspond gagne.
        let regles = app.config.regles.clone();
        if regles.is_empty() {
            ui.label(
                egui::RichText::new("aucune règle — le profil reste à choisir à la main").weak(),
            );
        }
        tracing::debug!(
            target: "kinterface",
            "prefs regles : {} règle(s)",
            regles.len()
        );
        let mut a_supprimer: Option<usize> = None;
        for (i, regle) in regles.iter().enumerate() {
            let profil_existe = app.profiles.iter().any(|p| p.name == regle.profil);
            ui.horizontal(|ui| {
                ui.label(format!("{}.", i + 1));
                ui.strong(regle.resume());
                if profil_existe {
                    ui.label(format!("→ « {} »", regle.profil));
                } else {
                    ui.label(
                        egui::RichText::new(format!("→ « {} » (profil absent)", regle.profil))
                            .color(egui::Color32::from_rgb(255, 170, 60)),
                    );
                }
                let bouton = ui.button("🗑");
                tracing::debug!(
                    target: "kinterface",
                    "prefs regle {} bouton supprimer rect {:?}",
                    i,
                    bouton.rect
                );
                if bouton.clicked() {
                    a_supprimer = Some(i);
                }
            });
        }
        if let Some(i) = a_supprimer {
            let retiree = app.config.regles.remove(i);
            if let Err(e) = app.config.save() {
                app.error_message = Some(format!("Règle non enregistrée : {}", e));
            }
            tracing::info!(
                target: "kinterface",
                "règle de profil supprimée : {} → « {} »",
                retiree.resume(),
                retiree.profil
            );
            app.annonce("Règle supprimée");
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Quand");
            egui::ComboBox::from_id_salt("regle_champ")
                .selected_text(app.regle_champ.label())
                .show_ui(ui, |ui| {
                    for champ in crate::config::ChampRegle::CHOIX {
                        ui.selectable_value(&mut app.regle_champ, champ, champ.label());
                    }
                });
            let champ = ui.add(
                egui::TextEdit::singleline(&mut app.regle_motif)
                    .desired_width(180.0)
                    .hint_text(motif_regle_aide(app.regle_champ)),
            );
            tracing::debug!(
                target: "kinterface",
                "prefs champ motif regle rect {:?}",
                champ.rect
            );
        });
        // Le brouillon vise par défaut le premier profil enregistré : sans cela, le
        // bouton « Ajouter » serait inactif tant que la liste n'a pas été ouverte.
        if app.regle_profil.is_empty() {
            if let Some(premier) = app.profiles.first() {
                app.regle_profil = premier.name.clone();
            }
        }
        let noms: Vec<String> = app.profiles.iter().map(|p| p.name.clone()).collect();
        ui.horizontal(|ui| {
            ui.label("alors le profil");
            egui::ComboBox::from_id_salt("regle_profil")
                .selected_text(app.regle_profil.clone())
                .show_ui(ui, |ui| {
                    for nom in &noms {
                        ui.selectable_value(&mut app.regle_profil, nom.clone(), nom);
                    }
                });
            let possible = !app.regle_motif.trim().is_empty()
                && noms.iter().any(|n| n == &app.regle_profil);
            let bouton = ui
                .add_enabled(possible, egui::Button::new("➕ Ajouter la règle"))
                .on_hover_text("Le profil de ce type de média s'appliquera à l'ouverture");
            tracing::debug!(
                target: "kinterface",
                "prefs bouton ajouter regle rect {:?}",
                bouton.rect
            );
            if bouton.clicked() {
                let regle = crate::config::Regle {
                    champ: app.regle_champ,
                    motif: app.regle_motif.trim().to_string(),
                    profil: app.regle_profil.clone(),
                };
                tracing::info!(
                    target: "kinterface",
                    "règle de profil ajoutée : {} → « {} »",
                    regle.resume(),
                    regle.profil
                );
                app.config.regles.push(regle);
                match app.config.save() {
                    Ok(()) => app.annonce("Règle ajoutée"),
                    Err(e) => app.error_message = Some(format!("Règle non enregistrée : {}", e)),
                }
                app.regle_motif.clear();
            }
        });
        aide(
            ui,
            "Les règles sont essayées dans cet ordre : la première qui correspond au média \
             ouvert s'applique (une règle dont le profil a été supprimé est ignorée).",
        );
    });

    sous_section(ui, "Importer un profil", |ui| {
        if ui.button("📂 Choisir un fichier .json…").clicked() {
            if let Some(chemin) = rfd::FileDialog::new()
                .add_filter("Profil KPlayer", &["json"])
                .pick_file()
            {
                match std::fs::read_to_string(&chemin)
                    .map_err(|e| e.to_string())
                    .and_then(|t| serde_json::from_str::<crate::config::Profil>(&t).map_err(|e| e.to_string()))
                {
                    Ok(profil) => {
                        tracing::info!(
                            target: "kinterface",
                            "profil importé : « {} » — {}",
                            profil.name,
                            profil.resume()
                        );
                        if let Some(existant) = app.profiles.iter_mut().find(|p| p.name == profil.name) {
                            *existant = profil;
                        } else {
                            app.profiles.push(profil);
                        }
                        app.profiles.sort_by(|a, b| a.name.cmp(&b.name));
                        let _ = crate::config::save_profiles(&app.profiles);
                        app.annonce("Profil importé");
                    }
                    Err(e) => {
                        tracing::warn!("Profil illisible : {}", e);
                        app.error_message = Some(format!("Profil illisible : {}", e));
                    }
                }
            }
        }
        aide(ui, "Un fichier exporté depuis KPlayer (JSON). Le nom du fichier est libre.");
    });

    sous_section(ui, "Sauvegarde de toute la configuration", |ui| {
        sauvegarde_configuration(app, ui);
    });
}

fn dessiner_interface(app: &mut App, ui: &mut egui::Ui) {
    sous_section(ui, "Apparence", |ui| {
        if ui
            .checkbox(&mut app.config.interface.dark_theme, "Thème sombre")
            .changed()
        {
            let _ = app.config.save();
        }
        // Thème externe : un fichier JSON par thème dans ~/.config/kplayer/themes/.
        let themes = crate::theme::lister();
        let mut choisi = app.config.interface.theme.clone();
        ligne(ui, "grille_theme_externe", "Thème externe", |ui| {
            egui::ComboBox::from_id_salt("choix_theme_externe")
                .selected_text(if choisi.is_empty() { "Intégré".to_string() } else { choisi.clone() })
                .width(200.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut choisi, String::new(), "Intégré");
                    for nom in &themes {
                        ui.selectable_value(&mut choisi, nom.clone(), nom);
                    }
                });
        });
        if choisi != app.config.interface.theme {
            app.config.interface.theme = choisi;
            let _ = app.config.save();
        }
        ui.horizontal(|ui| {
            if ui.button("Créer un thème d'exemple").clicked() {
                if let Some(dossier) = crate::theme::dossier() {
                    let _ = std::fs::create_dir_all(&dossier);
                    let chemin = dossier.join("exemple-nuit.json");
                    match std::fs::write(&chemin, crate::theme::Theme::EXEMPLE) {
                        Ok(()) => app.annonce(&format!("Thème d'exemple écrit : {}", chemin.display())),
                        Err(e) => app.error_message = Some(format!("Thème d'exemple : {e}")),
                    }
                }
            }
        });
        aide(
            ui,
            "Un thème est un fichier .json (base dark/light, couleurs fond, zone, texte, accent en RRGGBB) \
             dans ~/.config/kplayer/themes/. Un thème illisible est refusé avec la raison.",
        );
        if ui
            .checkbox(
                &mut app.config.interface.show_title,
                "Afficher le titre du média",
            )
            .changed()
        {
            let _ = app.config.save();
        }
    });

    sous_section(ui, "Contrôles à l'écran", |ui| {
        if ui
            .checkbox(
                &mut app.config.interface.show_progress_bar,
                "Barre de progression et contrôles",
            )
            .changed()
        {
            let _ = app.config.save();
        }
        if ui
            .checkbox(
                &mut app.config.interface.controls_always_visible,
                "Garder les contrôles visibles en plein écran",
            )
            .changed()
        {
            let _ = app.config.save();
        }
        if ui
            .checkbox(&mut app.config.interface.show_osd, "Afficher l'OSD (touche O)")
            .changed()
        {
            let _ = app.config.save();
        }
        let mut osd_ms = app.config.interface.osd_duration;
        ligne(ui, "grille_osd", "Durée de l'OSD", |ui| {
            if ui
                .add(
                    egui::Slider::new(&mut osd_ms, 500..=10_000)
                        .suffix(" ms")
                        .fixed_decimals(0),
                )
                .changed()
            {
                app.config.interface.osd_duration = osd_ms;
                let _ = app.config.save();
            }
        });
    });

    sous_section(ui, "Fichiers", |ui| {
        ligne(ui, "grille_captures", "Captures d'écran", |ui| {
            if ui
                .text_edit_singleline(&mut app.config.interface.screenshots_dir)
                .changed()
            {
                let _ = app.config.save();
            }
        });
        aide(ui, "Dossier où la touche S écrit les PNG (le caractère ~ est accepté).");
    });
}

fn dessiner_video(app: &mut App, ui: &mut egui::Ui) {
    let mut geometrie_modifiee = false;

    sous_section(ui, "Décodage", |ui| {
        let mut hwdec = app.player.hardware_decoding_enabled();
        if ui
            .checkbox(
                &mut hwdec,
                "Décodage matériel VA-API (repli logiciel automatique)",
            )
            .changed()
        {
            app.apply_hardware_decoding(hwdec);
        }
        ui.label(
            egui::RichText::new(format!(
                "Mode constaté : {}",
                if app.player.is_hardware_decoding() {
                    "VA-API (matériel)"
                } else {
                    "logiciel (CPU)"
                }
            ))
            .small()
            .weak(),
        );
    });

    sous_section(ui, "Mise à l'échelle", |ui| {
        // Réduction faite par le GPU quand le décodage est matériel : mesuré
        // −35 % (1080p→720p) et −56 % (1080p→540p) sur le coût par image.
        let mut gpu = app.config.video.gpu_scaling;
        if ui
            .checkbox(&mut gpu, "Réduire dans le GPU (VA-API)")
            .on_hover_text(
                "Quand le décodage est matériel et qu'aucun filtre CPU n'est demandé, \
                 la réduction d'échelle est faite par le pilote avant la copie en \
                 mémoire : 10,9 → 7,1 ms/image en 1080p affiché en 720p. Le pilote \
                 utilise son propre filtre haute qualité (plus doux que Lanczos) : \
                 décochez pour garder la méthode choisie ci-dessous.",
            )
            .changed()
        {
            app.config.video.gpu_scaling = gpu;
            app.player.set_gpu_scaling(gpu);
            let _ = app.config.save();
        }
        let mut scaling = app.config.video.scaling_method.clone();
        ligne(ui, "grille_echelle", "Méthode", |ui| {
            egui::ComboBox::from_id_salt("choix_echelle")
                .selected_text(scaling_label(&scaling))
                .width(240.0)
                .show_ui(ui, |ui| {
                    for methode in [
                        ScalingMethod::Lanczos,
                        ScalingMethod::Bicubic,
                        ScalingMethod::Bilinear,
                        ScalingMethod::Area,
                        ScalingMethod::NearestNeighbor,
                    ] {
                        if ui
                            .selectable_value(&mut scaling, methode.clone(), scaling_label(&methode))
                            .clicked()
                        {
                            app.config.video.scaling_method = scaling.clone();
                            app.player.set_scaling_method(scaling.clone());
                            let _ = app.config.save();
                        }
                    }
                });
        });
        aide(
            ui,
            "S'applique à la réduction d'image (CPU) ; l'agrandissement est fait par le GPU.",
        );
    });

    sous_section(ui, "Netteté", |ui| {
        let mut nettete = app.config.video.sharpen.min(3);
        let avant = nettete;
        ligne(ui, "grille_nettete", "Accentuation", |ui| {
            if ui
                .add(egui::Slider::new(&mut nettete, 0..=3).custom_formatter(|n, _| {
                    match n as u32 {
                        0 => "arrêtée".to_string(),
                        1 => "légère".to_string(),
                        2 => "moyenne".to_string(),
                        _ => "forte".to_string(),
                    }
                }))
                .on_hover_text(
                    "Filtre unsharp : compense le flou d'une source surcompressée. \
                     Au-delà du niveau 3, des halos apparaissent autour des contours.",
                )
                .changed()
            {
                app.config.video.sharpen = nettete;
                geometrie_modifiee = true;
            }
        });
        if nettete != avant && nettete == 0 {
            aide(ui, "Aucun filtre de netteté : l'image est laissée telle quelle.");
        }
    });

    sous_section(ui, "Rapport d'aspect", |ui| {
        let mut aspect = app.config.video.aspect.clone();
        ligne(ui, "grille_aspect", "Affichage", |ui| {
            egui::ComboBox::from_id_salt("choix_aspect")
                .selected_text(aspect.label())
                .width(240.0)
                .show_ui(ui, |ui| {
                    use crate::config::AspectMode;
                    for choix in [
                        AspectMode::Auto,
                        AspectMode::Ratio16x9,
                        AspectMode::Ratio4x3,
                        AspectMode::Ratio185,
                        AspectMode::Ratio235,
                        AspectMode::Remplir,
                        AspectMode::Rogner,
                        AspectMode::Personnalise,
                    ] {
                        if ui
                            .selectable_value(&mut aspect, choix.clone(), choix.label())
                            .clicked()
                        {
                            app.config.video.aspect = choix.clone();
                            geometrie_modifiee = true;
                        }
                    }
                });
        });
        if let crate::config::AspectMode::Personnalise = app.config.video.aspect {
            let mut rapport = app.config.video.custom_aspect;
            ligne(ui, "grille_aspect_custom", "Rapport", |ui| {
                if ui
                    .add(
                        egui::Slider::new(&mut rapport, 0.5..=3.5)
                            .suffix(" : 1")
                            .fixed_decimals(2),
                    )
                    .changed()
                {
                    app.config.video.custom_aspect = rapport;
                    geometrie_modifiee = true;
                }
            });
        }
        aide(
            ui,
            "« Automatique » respecte le rapport du média. Les autres valeurs déforment \
             volontairement l'image (corriger un encodage mal étiqueté, remplir l'écran). \
             Touche K pour faire défiler les modes.",
        );
    });

    sous_section(ui, "Fréquence d'image", |ui| {
        let mut plafond = app.config.video.max_fps as f32;
        ligne(ui, "grille_max_fps", "Maximum affiché", |ui| {
            if ui
                .add(
                    egui::Slider::new(&mut plafond, 0.0..=120.0)
                        .custom_formatter(|v, _| if v < 1.0 { "celle du film".to_string() } else { format!("{:.0} i/s", v) }),
                )
                .on_hover_text("Réduit le travail de rendu : les images en trop sont décodées mais non affichées")
                .changed()
            {
                app.config.video.max_fps = plafond as u32;
                geometrie_modifiee = true;
            }
        });
    });

    sous_section(ui, "Décodeur par codec", |ui| {
        // Pour chaque codec courant : « Automatique » (le choix de FFmpeg) ou l'un des décodeurs
        // qu'il propose. Pris en compte à la prochaine ouverture d'un film.
        for (codec, libelle) in [
            ("h264", "H.264"),
            ("hevc", "H.265 / HEVC"),
            ("vp9", "VP9"),
            ("av1", "AV1"),
            ("mpeg2video", "MPEG-2"),
            ("mpeg4", "MPEG-4"),
        ] {
            let disponibles = crate::player::decodeurs_pour(codec);
            if disponibles.len() < 2 {
                continue; // un seul décodeur : rien à choisir
            }
            let actuel = app.config.video.decoders.get(codec).cloned().unwrap_or_default();
            let mut choix = actuel.clone();
            ligne(ui, &format!("grille_decodeur_{codec}"), libelle, |ui| {
                egui::ComboBox::from_id_salt(format!("choix_decodeur_{codec}"))
                    .selected_text(if choix.is_empty() { "Automatique".to_string() } else { choix.clone() })
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut choix, String::new(), "Automatique");
                        for nom in &disponibles {
                            ui.selectable_value(&mut choix, nom.clone(), nom);
                        }
                    });
            });
            if choix != actuel {
                if choix.is_empty() {
                    app.config.video.decoders.remove(codec);
                } else {
                    app.config.video.decoders.insert(codec.to_string(), choix);
                }
                geometrie_modifiee = true;
            }
        }
        aide(
            ui,
            "Ne montre que les codecs qui ont plusieurs décodeurs. S'il ne s'ouvre pas, le décodeur \
             choisi est remplacé par l'automatique. Pris en compte à la prochaine ouverture.",
        );
    });

    sous_section(ui, "Conversion des couleurs", |ui| {
        let mut matrice = app.config.video.color_matrix;
        ligne(ui, "grille_matrice", "Matrice", |ui| {
            egui::ComboBox::from_id_salt("choix_matrice")
                .selected_text(matrice.label())
                .width(200.0)
                .show_ui(ui, |ui| {
                    use crate::config::ColorMatrix;
                    for choix in [ColorMatrix::Auto, ColorMatrix::Bt601, ColorMatrix::Bt709, ColorMatrix::Bt2020] {
                        if ui.selectable_value(&mut matrice, choix, choix.label()).clicked() {
                            app.config.video.color_matrix = choix;
                            geometrie_modifiee = true;
                        }
                    }
                });
        });
        let mut plage = app.config.video.color_range;
        ligne(ui, "grille_plage", "Plage", |ui| {
            egui::ComboBox::from_id_salt("choix_plage")
                .selected_text(plage.label())
                .width(200.0)
                .show_ui(ui, |ui| {
                    use crate::config::ColorRange;
                    for choix in [ColorRange::Auto, ColorRange::Limited, ColorRange::Full] {
                        if ui.selectable_value(&mut plage, choix, choix.label()).clicked() {
                            app.config.video.color_range = choix;
                            geometrie_modifiee = true;
                        }
                    }
                });
        });
        aide(
            ui,
            "« Automatique » suit ce que déclare le film (à défaut : HD → BT.709, SD → BT.601). \
             À changer seulement si les couleurs d'un film mal étiqueté sont fausses, ou si les \
             noirs paraissent grisés (plage) ou écrasés.",
        );
    });

    sous_section(ui, "Glisser l'image", |ui| {
        let mut px = app.config.video.pan_x_percent;
        let mut py = app.config.video.pan_y_percent;
        let mut change = false;
        ligne(ui, "grille_pan_x", "Horizontal", |ui| {
            change |= ui
                .add(egui::Slider::new(&mut px, -50.0..=50.0).suffix(" %").fixed_decimals(0))
                .on_hover_text("Décale l'image vers la gauche (−) ou la droite (+) ; ce qui sort est rogné")
                .changed();
        });
        ligne(ui, "grille_pan_y", "Vertical", |ui| {
            change |= ui
                .add(egui::Slider::new(&mut py, -50.0..=50.0).suffix(" %").fixed_decimals(0))
                .on_hover_text("Décale l'image vers le haut (−) ou le bas (+)")
                .changed();
        });
        if change {
            app.config.video.pan_x_percent = px;
            app.config.video.pan_y_percent = py;
            let _ = app.config.save();
        }
    });

    sous_section(ui, "Bandes noires", |ui| {
        let mut c = (
            app.config.video.crop_top,
            app.config.video.crop_bottom,
            app.config.video.crop_left,
            app.config.video.crop_right,
        );
        let avant = c;
        egui::Grid::new("grille_crop")
            .num_columns(4)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label("Haut");
                ui.add(egui::DragValue::new(&mut c.0).range(0..=2000).suffix(" px"));
                ui.label("Bas");
                ui.add(egui::DragValue::new(&mut c.1).range(0..=2000).suffix(" px"));
                ui.end_row();
                ui.label("Gauche");
                ui.add(egui::DragValue::new(&mut c.2).range(0..=2000).suffix(" px"));
                ui.label("Droite");
                ui.add(egui::DragValue::new(&mut c.3).range(0..=2000).suffix(" px"));
                ui.end_row();
            });
        if c != avant {
            app.config.video.crop_top = c.0;
            app.config.video.crop_bottom = c.1;
            app.config.video.crop_left = c.2;
            app.config.video.crop_right = c.3;
            geometrie_modifiee = true;
            // Bouger un curseur entre en mode réglage : on veut voir l'image
            // d'origine et le repère, puis valider.
            app.crop_reglage = true;
        }
        ui.horizontal(|ui| {
            if ui
                .button("🔍 Détecter automatiquement")
                .on_hover_text(
                    "Analyse plusieurs images du film et mesure les bandes sombres \n\
                     (médiane sur les échantillons : une scène sombre isolée ne suffit pas)",
                )
                .clicked()
            {
                app.crop_reglage = true;
                let (h, b, g, d) = app.player.detect_black_bars(5);
                app.config.video.crop_top = h;
                app.config.video.crop_bottom = b;
                app.config.video.crop_left = g;
                app.config.video.crop_right = d;
                geometrie_modifiee = true;
                tracing::info!(
                    target: "kinterface",
                    "bandes noires détectées : haut={} bas={} gauche={} droite={}",
                    h, b, g, d
                );
                app.annonce(&format!(
                    "Bandes noires : haut {}, bas {}, gauche {}, droite {}",
                    h, b, g, d
                ));
            }
            if app.config.video.crop_active() && ui.button("↺ Annuler le recadrage").clicked() {
                app.crop_reglage = false;
                app.config.video.crop_top = 0;
                app.config.video.crop_bottom = 0;
                app.config.video.crop_left = 0;
                app.config.video.crop_right = 0;
                geometrie_modifiee = true;
            }
        });
        // Le repère visuel : sans lui, régler le recadrage se fait en aveugle.
        if ui
            .checkbox(
                &mut app.config.video.crop_overlay,
                "Afficher la zone de recadrage sur l'image",
            )
            .on_hover_text(
                "Assombrit la partie retirée et encadre la partie conservée, avec \
                 ses dimensions en pixels",
            )
            .changed()
        {
            let _ = app.config.save();
        }
        // --- Validation du réglage (même logique que le menu contextuel) ---
        ui.horizontal(|ui| {
            // Boutons partagés avec le menu contextuel : même libellé, même effet,
            // même journalisation (`ui::bouton_etat`) — donc vérifiables de la même
            // façon par le banc.
            if crate::ui::bouton_etat(
                ui,
                app.config.video.crop_active() && app.crop_reglage,
                "✔ Valider le recadrage",
            )
            .on_hover_text("Applique le recadrage et retire le repère (Entrée)")
            .clicked()
            {
                app.valider_recadrage();
            }
            if crate::ui::bouton_etat(
                ui,
                app.config.video.crop_active() && !app.crop_reglage,
                "↺ Régler à nouveau",
            )
            .on_hover_text("Réaffiche l'image d'origine pour ajuster")
            .clicked()
            {
                app.entrer_reglage_recadrage();
            }
            if app.crop_reglage {
                ui.label(
                    egui::RichText::new("réglage en cours")
                        .small()
                        .color(egui::Color32::from_rgb(120, 200, 255)),
                );
            }
        });
        if app.config.video.crop_active() && app.config.video.crop_overlay {
            let (l, h) = app
                .player
                .media_info()
                .map(|i| {
                    (
                        i.resolution.0.saturating_sub(
                            app.config.video.crop_left + app.config.video.crop_right,
                        ),
                        i.resolution.1
                            .saturating_sub(app.config.video.crop_top + app.config.video.crop_bottom),
                    )
                })
                .unwrap_or((0, 0));
            aide(
                ui,
                &format!(
                    "Zone conservée : {} × {} px (repère affiché sur la vidéo).",
                    l, h
                ),
            );
        }
        aide(
            ui,
            "Le recadrage change la résolution utile (filtre crop). Pendant le réglage, \
             l'image d'origine est affichée avec le repère : « Valider » (ou Entrée) \
             applique le recadrage et retire le repère.",
        );
        if app.config.video.aspect_impose() && app.config.video.crop_active() {
            aide(
                ui,
                "Recadrage et aspect imposé se cumulent : le recadrage agit sur les \
                 pixels, l'aspect sur la forme affichée.",
            );
        }
    });

    sous_section(ui, "Géométrie", |ui| {
        let mut rotation = app.config.video.rotate;
        ligne(ui, "grille_rotation", "Rotation", |ui| {
            egui::ComboBox::from_id_salt("choix_rotation")
                .selected_text(format!("{}°", rotation))
                .width(120.0)
                .show_ui(ui, |ui| {
                    for angle in [0u16, 90, 180, 270] {
                        if ui
                            .selectable_value(&mut rotation, angle, format!("{}°", angle))
                            .clicked()
                        {
                            app.config.video.rotate = angle;
                            geometrie_modifiee = true;
                        }
                    }
                });
        });
        if ui
            .checkbox(
                &mut app.config.video.flip_h,
                "Miroir horizontal (gauche ↔ droite)",
            )
            .on_hover_text("Filtre hflip : utile pour une webcam ou un encodage inversé")
            .changed()
        {
            geometrie_modifiee = true;
        }
        if ui
            .checkbox(
                &mut app.config.video.flip,
                "Miroir vertical (haut ↔ bas)",
            )
            .on_hover_text("Filtre vflip : source filmée à l'envers")
            .changed()
        {
            geometrie_modifiee = true;
        }

        let mut zoom = app.config.video.zoom;
        ligne(ui, "grille_zoom", "Zoom numérique", |ui| {
            if ui
                .add(
                    egui::Slider::new(&mut zoom, 1.0..=4.0)
                        .suffix(" ×")
                        .fixed_decimals(2),
                )
                .on_hover_text("Recadre au centre puis agrandit (filtres crop + scale)")
                .changed()
            {
                app.config.video.zoom = zoom;
                geometrie_modifiee = true;
            }
        });
        if (zoom - 1.0).abs() > 1e-3 {
            aide(
                ui,
                "Le zoom recadre : la résolution affichée est réduite d'autant, \
                 puis agrandie par le GPU.",
            );
        }

        if app.config.video.geometry_changed() {
            aide(
                ui,
                "La rotation, le miroir et le zoom modifient la géométrie : le recadrage \
                 et le rapport d'aspect sont ajustés en conséquence.",
            );
        }

        if geometrie_modifiee {
            app.appliquer_filtres();
            let _ = app.config.save();
        }
    });
}

fn dessiner_image(app: &mut App, ui: &mut egui::Ui) {
    let mut modifie = false;

    sous_section(ui, "Corrections automatiques", |ui| {
        if ui
            .checkbox(&mut app.config.video.deband, "Détramage des dégradés")
            .on_hover_text("Filtre deband : atténue les bandes des ciels et des fondus")
            .changed()
        {
            modifie = true;
        }
        if ui
            .checkbox(
                &mut app.config.video.deinterlace,
                "Désentrelacement (trames entrelacées seulement)",
            )
            .on_hover_text(
                    "bwdif ou yadif, au choix ci-dessous : une source progressive \
                     n'est pas modifiée",
                )
            .changed()
        {
            modifie = true;
        }
        let mut denoise = app.config.video.denoise.min(3);
        ligne(ui, "grille_bruit", "Débruitage", |ui| {
            if ui
                .add(egui::Slider::new(&mut denoise, 0..=3).custom_formatter(|n, _| {
                    match n as u32 {
                        0 => "arrêté".to_string(),
                        1 => "léger".to_string(),
                        2 => "moyen".to_string(),
                        _ => "fort".to_string(),
                    }
                }))
                .on_hover_text("Filtre hqdn3d : bruit de compression, spatial et temporel")
                .changed()
            {
                app.config.video.denoise = denoise;
                modifie = true;
            }
        });
    });

    sous_section(ui, "HDR et 10 bits", |ui| {
        // Ce que le flux annonce, en clair : sans cette ligne, l'utilisateur ne peut
        // pas savoir si le réglage s'applique (un film SDR ne déclenche rien).
        let etat = app.player.hdr_description();
        ui.label(
            egui::RichText::new(match &etat {
                Some(texte) => format!("Flux : {}", texte),
                None => "Flux : aucun média ouvert".to_string(),
            })
            .small()
            .weak(),
        );
        // Ce que l'affichage permet : écran (EDID) **et** sortie. C'est la réponse à
        // « mon écran est HDR, pourquoi est-ce converti ? » — la cause est souvent la
        // fenêtre, pas l'écran, et c'est écrit là.
        let contexte = app.player.hdr_contexte();
        let couleur = if contexte.passthrough_possible() {
            egui::Color32::from_rgb(120, 200, 120)
        } else {
            ui.visuals().weak_text_color()
        };
        ui.label(
            egui::RichText::new(format!("Affichage : {}", contexte.resume()))
                .small()
                .color(couleur),
        );

        let mut mode = app.config.video.hdr.mode;
        ligne(ui, "grille_hdr_mode", "Conversion tonale HDR", |ui| {
            egui::ComboBox::from_id_salt("choix_hdr_mode")
                .selected_text(mode.libelle())
                .width(280.0)
                .show_ui(ui, |ui| {
                    for choix in crate::config::HdrMode::CHOIX {
                        ui.selectable_value(&mut mode, choix, choix.libelle());
                    }
                });
            if mode != app.config.video.hdr.mode {
                app.config.video.hdr.mode = mode;
                modifie = true;
            }
        });

        let actif = app.config.video.hdr.mode != crate::config::HdrMode::Jamais;
        ui.add_enabled_ui(actif, |ui| {
            let mut operateur = app.config.video.hdr.tonemap;
            ligne(ui, "grille_hdr_tonemap", "Opérateur", |ui| {
                egui::ComboBox::from_id_salt("choix_hdr_tonemap")
                    .selected_text(operateur.libelle())
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        for choix in crate::config::Tonemap::CHOIX {
                            ui.selectable_value(&mut operateur, choix, choix.libelle());
                        }
                    });
                if operateur != app.config.video.hdr.tonemap {
                    app.config.video.hdr.tonemap = operateur;
                    modifie = true;
                }
            });

            let mut gamut = app.config.video.hdr.gamut;
            ligne(ui, "grille_hdr_gamut", "Couleurs hors gamut", |ui| {
                egui::ComboBox::from_id_salt("choix_hdr_gamut")
                    .selected_text(gamut.libelle())
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        for choix in crate::config::Gamut::CHOIX {
                            ui.selectable_value(&mut gamut, choix, choix.libelle());
                        }
                    });
                if gamut != app.config.video.hdr.gamut {
                    app.config.video.hdr.gamut = gamut;
                    modifie = true;
                }
            });

            let mut contraste = app.config.video.hdr.contrast_recovery;
            ligne(ui, "grille_hdr_contraste", "Détail des hautes lumières", |ui| {
                if ui
                    .add(
                        egui::Slider::new(&mut contraste, 0.0..=1.0)
                            .custom_formatter(|n, _| format!("{:.0} %", n * 100.0)),
                    )
                    .on_hover_text(
                        "Restitue le contraste perdu dans les zones très claires \
                         (nuages, lampes). Au-delà de 60 %, le bruit de la source remonte aussi.",
                    )
                    .changed()
                {
                    app.config.video.hdr.contrast_recovery = contraste;
                    modifie = true;
                }
            });

            if ui
                .checkbox(
                    &mut app.config.video.hdr.peak_detect,
                    "Détection de crête dynamique",
                )
                .on_hover_text(
                    "Adapte la conversion image par image au lieu d'une crête fixe : \
                     les scènes sombres ne sont plus écrasées.",
                )
                .changed()
            {
                modifie = true;
            }

            aide(
                ui,
                "Le moteur GPU (libplacebo, Vulkan) est utilisé quand il est \
                 disponible ; sinon le repli CPU (zscale + tonemap) prend le relais. \
                 Le journal dit lequel des deux travaille.",
            );
        });

        let mut dither = app.config.video.hdr.dither;
        ligne(ui, "grille_hdr_dither", "Tramage 10 → 8 bits", |ui| {
            egui::ComboBox::from_id_salt("choix_hdr_dither")
                .selected_text(dither.libelle())
                .width(280.0)
                .show_ui(ui, |ui| {
                    for choix in crate::config::Dither::CHOIX {
                        ui.selectable_value(&mut dither, choix, choix.libelle());
                    }
                });
            if dither != app.config.video.hdr.dither {
                app.config.video.hdr.dither = dither;
                modifie = true;
            }
        });

        if ui
            .checkbox(
                &mut app.config.video.hdr.gpu_deband,
                "Débanding par le GPU",
            )
            .on_hover_text(
                "Remplace le débanding CPU : une passe de moins sur le processeur, et \
                 il travaille sur les 10 bits d'origine.",
            )
            .changed()
        {
            modifie = true;
        }
        ui.add_enabled_ui(app.config.video.hdr.gpu_deband, |ui| {
            let mut rayon = app.config.video.hdr.gpu_deband_radius;
            ligne(ui, "grille_hdr_deband_rayon", "Rayon (GPU)", |ui| {
                if ui
                    .add(egui::Slider::new(&mut rayon, 4.0..=64.0).suffix(" px").fixed_decimals(0))
                    .changed()
                {
                    app.config.video.hdr.gpu_deband_radius = rayon;
                    modifie = true;
                }
            });
            let mut seuil = app.config.video.hdr.gpu_deband_threshold;
            ligne(ui, "grille_hdr_deband_seuil", "Seuil (GPU)", |ui| {
                if ui
                    .add(
                        egui::Slider::new(&mut seuil, 0.0..=0.2)
                            .fixed_decimals(3)
                            .custom_formatter(|n, _| format!("{:.3}", n)),
                    )
                    .on_hover_text(
                        "Sensibilité de la détection des bandes : plus haut, plus de \
                         dégradés traités (et plus de grain ajouté).",
                    )
                    .changed()
                {
                    app.config.video.hdr.gpu_deband_threshold = seuil;
                    modifie = true;
                }
            });
            let mut grain = app.config.video.hdr.gpu_deband_grain;
            ligne(ui, "grille_hdr_deband_grain", "Grain (GPU)", |ui| {
                if ui
                    .add(egui::Slider::new(&mut grain, 0.0..=64.0).fixed_decimals(0))
                    .on_hover_text(
                        "Grain ajouté après débanding : sans lui, les aplats deviennent \
                         trop lisses et paraissent artificiels.",
                    )
                    .changed()
                {
                    app.config.video.hdr.gpu_deband_grain = grain;
                    modifie = true;
                }
            });
        });

        let mut pic = app.config.video.hdr.pic_nits;
        ligne(ui, "grille_hdr_pic", "Pic de la source", |ui| {
            if ui
                .add(egui::Slider::new(&mut pic, 100.0..=4000.0).suffix(" nits").fixed_decimals(0))
                .on_hover_text(
                    "Pic lumineux du master (1000 = HDR10 courant, 4000 = master cinéma). \
                     Utilisé par le repli CPU ; le moteur GPU mesure le pic réel image par image.",
                )
                .changed()
            {
                app.config.video.hdr.pic_nits = pic;
                modifie = true;
            }
        });

        ui.add_space(4.0);
        if ui.button("↺ Réglages HDR par défaut").clicked() {
            app.config.video.hdr.reinitialiser();
            modifie = true;
        }
    });

    sous_section(ui, "Réglages manuels", |ui| {
        let mut curseur = |ui: &mut egui::Ui, libelle: &str, id: &str, valeur: &mut f32, plage: std::ops::RangeInclusive<f32>| {
            ligne(ui, id, libelle, |ui| {
                if ui
                    .add(egui::Slider::new(valeur, plage).fixed_decimals(2))
                    .changed()
                {
                    modifie = true;
                }
            });
        };
        curseur(ui, "Luminosité", "grille_lum", &mut app.config.video.brightness, -1.0..=1.0);
        curseur(ui, "Contraste", "grille_con", &mut app.config.video.contrast, 0.0..=3.0);
        curseur(ui, "Saturation", "grille_sat", &mut app.config.video.saturation, 0.0..=3.0);
        curseur(ui, "Gamma", "grille_gam", &mut app.config.video.gamma, 0.1..=10.0);
        curseur(ui, "Teinte", "grille_teinte", &mut app.config.video.hue, -180.0..=180.0);

        // Niveaux : deux bornes, exprimées en pourcentage — c'est ainsi qu'on les
        // lit sur une waveform. Un intervalle resserré recrée le contraste d'un
        // vieux transfert aux noirs lavés.
        ligne(ui, "grille_niveaux_noir", "Niveau noir", |ui| {
            let mut v = app.config.video.niveaux_noir * 100.0;
            if ui
                .add(egui::Slider::new(&mut v, 0.0..=60.0).suffix(" %").fixed_decimals(1))
                .on_hover_text(
                    "Filtre colorlevels : en dessous de ce niveau, tout devient noir. \
                     Un vieux transfert vidéo a souvent des noirs à 10-15 % au lieu de 0.",
                )
                .changed()
            {
                app.config.video.niveaux_noir = v / 100.0;
                modifie = true;
            }
        });
        ligne(ui, "grille_niveaux_blanc", "Niveau blanc", |ui| {
            let mut v = app.config.video.niveaux_blanc * 100.0;
            if ui
                .add(egui::Slider::new(&mut v, 50.0..=100.0).suffix(" %").fixed_decimals(1))
                .on_hover_text("Au-dessus de ce niveau, tout devient blanc.")
                .changed()
            {
                app.config.video.niveaux_blanc = v / 100.0;
                modifie = true;
            }
        });

        let mut adoucir = app.config.video.adoucir.min(3);
        ligne(ui, "grille_adoucir", "Adoucir", |ui| {
            if ui
                .add(egui::Slider::new(&mut adoucir, 0..=3).custom_formatter(|n, _| {
                    match n as u32 {
                        0 => "arrêté".to_string(),
                        1 => "léger".to_string(),
                        2 => "moyen".to_string(),
                        _ => "fort".to_string(),
                    }
                }))
                .on_hover_text(
                    "Flou gaussien (gblur) : atténue un bruit de compression tenace ou \
                     une mise à l'échelle dure. Complémentaire du débruitage, qui garde \
                     les contours nets.",
                )
                .changed()
            {
                app.config.video.adoucir = adoucir;
                modifie = true;
            }
        });

        ui.add_space(4.0);
        if ui.button("↺ Revenir aux valeurs neutres").clicked() {
            app.config.video.reset_filters();
            modifie = true;
        }
    });

    // --- Sous-bloc « Avancé » : les paramètres FFmpeg eux-mêmes ---
    // C'est ici qu'un nouveau réglage fin de filtre vient s'ajouter (section déjà
    // en place, il suffit d'ajouter une ligne `ligne(...)`).
    sous_section(ui, "Avancé — paramètres des filtres", |ui| {
        let deband_actif = app.config.video.deband;
        let yadif_actif = app.config.video.deinterlace;

        ui.add_enabled_ui(deband_actif, |ui| {
            let mut rayon = app.config.video.deband_range;
            ligne(ui, "grille_db_range", "Rayon du détramage", |ui| {
                if ui
                    .add(
                        egui::Slider::new(&mut rayon, 4..=64)
                            .suffix(" px")
                            .custom_formatter(|n, _| format!("{:.0}", n)),
                    )
                    .on_hover_text("deband=range : étendue de la recherche de bandes")
                    .changed()
                {
                    app.config.video.deband_range = rayon;
                    modifie = true;
                }
            });
            let mut seuil = app.config.video.deband_threshold;
            ligne(ui, "grille_db_seuil", "Seuil du détramage", |ui| {
                if ui
                    .add(
                        egui::Slider::new(&mut seuil, 0.001..=0.2)
                            .custom_formatter(|n, _| format!("{:.3}", n)),
                    )
                    .on_hover_text("deband=1thr/2thr/3thr : plus haut = agit plus fort")
                    .changed()
                {
                    app.config.video.deband_threshold = seuil;
                    modifie = true;
                }
            });
        });
        if !deband_actif {
            aide(ui, "Activez d'abord le détramage ci-dessus.");
        }

        ui.add_enabled_ui(yadif_actif, |ui| {
            // Choix du filtre : `bwdif` (défaut) ou `yadif`. Le classement est
            // mesuré (tools/bench-desentrelacement.sh) et rappelé dans le survol,
            // pour que l'utilisateur ne choisisse pas au hasard.
            let mut filtre = app.config.video.deinterlace_filter;
            let mut rect_filtre = egui::Rect::NOTHING;
            ligne(ui, "grille_filtre_desent", "Filtre", |ui| {
                rect_filtre = egui::ComboBox::from_id_salt("choix_filtre_desent")
                    .selected_text(filtre_label(&filtre))
                    .width(160.0)
                    .show_ui(ui, |ui| {
                        for choix in crate::config::DeinterlaceFilter::CHOIX {
                            ui.selectable_value(&mut filtre, choix, filtre_label(&choix));
                        }
                    })
                    .response
                    .rect;
            });
            tracing::debug!(
                target: "kinterface",
                "prefs filtre desentrelacement rect {:?}",
                rect_filtre
            );
            if filtre != app.config.video.deinterlace_filter {
                app.config.video.deinterlace_filter = filtre;
                modifie = true;
            }
            aide(
                ui,
                "bwdif conserve le détail des zones fixes : mesuré 45,8 dB contre 43,4 \
                 dB pour yadif sur une image immobile (20,6 contre 9,2 dB sur une mire \
                 en lignes de 1 px), pour un coût inférieur (11,2 contre 13,2 ms par \
                 image 1080p). En mouvement, les deux rendus sont équivalents.",
            );
            let mut parite = app.config.video.deinterlace_parity.clone();
            ligne(ui, "grille_yadif", "Parité des trames", |ui| {
                egui::ComboBox::from_id_salt("choix_parite")
                    .selected_text(parite_label(&parite))
                    .width(160.0)
                    .show_ui(ui, |ui| {
                        for choix in [
                            DeinterlaceParity::Auto,
                            DeinterlaceParity::TopFirst,
                            DeinterlaceParity::BottomFirst,
                        ] {
                            if ui
                                .selectable_value(&mut parite, choix.clone(), parite_label(&choix))
                                .clicked()
                            {
                                app.config.video.deinterlace_parity = parite.clone();
                                modifie = true;
                            }
                        }
                    });
            });
        });
        if !yadif_actif {
            aide(ui, "Activez d'abord le désentrelacement ci-dessus.");
        }

        // La chaîne réellement construite, en clair : c'est la preuve visible que le
        // réglage atteint FFmpeg (elle est aussi journalisée).
        ui.add_space(6.0);
        let chaine = app.player.filters_description();
        ui.label(
            egui::RichText::new(if chaine.is_empty() {
                "chaîne active : aucune (image d'origine)".to_string()
            } else {
                format!("chaîne active : {}", chaine)
            })
            .small()
            .monospace(),
        );
    });

    if modifie {
        // Un seul point d'application : la chaîne est reconstruite à l'image suivante.
        app.appliquer_filtres();
        let _ = app.config.save();
    }
}

fn dessiner_audio(app: &mut App, ui: &mut egui::Ui) {
    let mut egaliseur_modifie = false;

    sous_section(ui, "Piste audio du film", |ui| {
        // Langues par priorité : « fre, eng » essaie fre, puis eng. Le texte en cours de frappe est
        // gardé à part (voir la page des sous-titres : recomposer depuis la liste effacerait la
        // virgule qu'on vient de taper).
        let id_saisie = egui::Id::new("saisie_langues_audio");
        let mut saisie: String = ui.memory_mut(|m| {
            m.data
                .get_temp_mut_or_insert_with(id_saisie, || {
                    let a = &app.config.audio;
                    if a.preferred_languages.is_empty() { a.default_language.clone() } else { a.preferred_languages.join(", ") }
                })
                .clone()
        });
        ligne(ui, "grille_langues_audio", "Langues (priorité)", |ui| {
            if ui
                .add(egui::TextEdit::singleline(&mut saisie).hint_text("fre, eng"))
                .on_hover_text(
                    "Plusieurs langues séparées par des virgules, la plus voulue d'abord. Les pistes de \
                     commentaire ou d'audiodescription ne sont choisies qu'à défaut d'autre piste.",
                )
                .changed()
            {
                app.config.audio.preferred_languages = saisie
                    .split(',')
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect();
                let _ = app.config.save();
                app.player.set_track_preferences(
                    &app.config.audio.langues_preferees(),
                    &app.config.subtitles.langues_preferees(),
                    app.config.subtitles.use_embedded,
                );
            }
        });
        ui.memory_mut(|m| m.data.insert_temp(id_saisie, saisie));
        if ui
            .checkbox(&mut app.config.audio.remember_track, "Mémoriser la piste audio choisie pour chaque film")
            .on_hover_text("La piste choisie à la main (Ctrl+J, menu Audio) revient à la réouverture du même film")
            .changed()
        {
            app.player.set_audio_remember(app.config.audio.remember_track);
            let _ = app.config.save();
        }
    });

    sous_section(ui, "Sortie", |ui| {
        let actuel = if app.config.audio.device.is_empty() {
            "Périphérique par défaut".to_string()
        } else {
            app.config.audio.device.clone()
        };
        let mut choisi = app.config.audio.device.clone();
        let mut change = false;
        let peripheriques = app.audio_devices.clone().unwrap_or_default();

        ligne(ui, "grille_periph", "Périphérique", |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("audio_device")
                    .selected_text(actuel)
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_value(&mut choisi, String::new(), "Périphérique par défaut")
                            .clicked()
                        {
                            change = true;
                        }
                        for (nom, taux, canaux) in peripheriques {
                            let libelle = format!("{} ({} Hz, {} canaux)", nom, taux, canaux);
                            if ui.selectable_value(&mut choisi, nom, libelle).clicked() {
                                change = true;
                            }
                        }
                    });
                if ui
                    .button("⟳")
                    .on_hover_text("Relire la liste des périphériques")
                    .clicked()
                {
                    app.audio_devices = None;
                    app.refresh_audio_devices();
                    app.audio_devices = Some(crate::audio::available_output_devices());
                }
            });
        });

        if change {
            app.config.audio.device = choisi.clone();
            app.player.set_audio_device(&choisi);
            let _ = app.config.save();
        }
    });

    sous_section(ui, "Volume", |ui| {
        let mut vol_max = app.config.audio.volume_max;
        ligne(ui, "grille_volmax", "Volume maximal", |ui| {
            if ui
                .add(egui::Slider::new(&mut vol_max, 100..=300).suffix(" %"))
                .changed()
            {
                app.config.audio.volume_max = vol_max;
                app.player.set_volume_max(vol_max);
                let _ = app.config.save();
            }
        });
        let mut volume = app.player.get_volume() * 100.0;
        ligne(ui, "grille_vol", "Volume courant", |ui| {
            if ui
                .add(egui::Slider::new(&mut volume, 0.0..=vol_max as f32).suffix(" %"))
                .changed()
            {
                app.player.set_volume(volume / 100.0);
            }
        });
        // `toggle_mute` est une bascule : un bouton (et non une case à cocher dont
        // l'état serait à maintenir en double avec le lecteur).
        if ui.button("🔇 Muet / son (M)").clicked() {
            app.player.toggle_mute();
        }
    });

    sous_section(ui, "Égaliseur", |ui| {
        let mut actif = app.config.audio.eq_enabled;
        if ui
            .checkbox(&mut actif, "Activer l'égaliseur (18 bandes)")
            .on_hover_text("Filtre superequalizer de FFmpeg, appliqué au signal envoyé à la carte son")
            .changed()
        {
            app.config.audio.eq_enabled = actif;
            egaliseur_modifie = true;
        }

        if !app.config.audio.eq_enabled {
            aide(
                ui,
                "Désactivé : le signal traverse le lecteur sans transformation (aucun coût).",
            );
        } else {
            // Le profil affiché est celui qui CORRESPOND aux gains courants ; dès
            // qu'un curseur est déplacé, la courbe ne correspond plus à aucun profil
            // et l'affichage le dit (« Personnalisé ») au lieu de mentir.
            let preset = preset_courant(&app.config.audio);
            tracing::debug!(
                target: "kinterface",
                "prefs eq profil {}",
                match preset {
                    Some(p) => p.label(),
                    None => "Personnalisé",
                }
            );
            let mut choisi = preset;
            ligne(ui, "grille_preset", "Profil", |ui| {
                egui::ComboBox::from_id_salt("choix_eq_preset")
                    .selected_text(match preset {
                        Some(p) => p.label(),
                        None => "Personnalisé",
                    })
                    .width(260.0)
                    .show_ui(ui, |ui| {
                        // Une **liste unique** (`EqPreset::CHOIX`) : sans elle, un
                        // preset ajouté au moteur restait invisible dans l'interface
                        // (et le compilateur signalait une variante jamais construite).
                        for choix in EqPreset::CHOIX {
                            if ui
                                .selectable_value(&mut choisi, Some(choix), choix.label())
                                .clicked()
                            {
                                app.config.audio.set_eq_preset(choix);
                                egaliseur_modifie = true;
                            }
                        }
                    });
            });
            if app.config.audio.eq_is_neutral() {
                aide(
                    ui,
                    "Toutes les bandes sont neutres : aucun filtre n'est inséré \
                     (le profil « Neutre » suffit à revenir à l'original).",
                );
            }

            // --- Bandes, en deux colonnes : 18 curseurs tiennent sans défilement ---
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Bandes (dB — 0 = neutre)")
                    .small()
                    .strong()
                    .color(ui.visuals().weak_text_color()),
            );
            ui.separator();
            egui::Grid::new("grille_eq_bandes")
                .num_columns(4)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    for (i, freq) in crate::config::EQ_FREQUENCIES.iter().enumerate() {
                        let libelle = if *freq >= 1000 {
                            format!("{:.1} kHz", *freq as f32 / 1000.0)
                        } else {
                            format!("{} Hz", freq)
                        };
                        ui.label(egui::RichText::new(libelle).small());
                        // Le stockage est un multiplicateur linéaire (ce qu'attend
                        // FFmpeg) ; l'affichage est en décibels, seule unité lisible
                        // pour une oreille. ×4 = +12 dB.
                        let db_actuel = 20.0 * app.config.audio.gain(i).max(1e-3).log10();
                        let mut db = db_actuel;
                        let reponse = ui.add(
                            egui::Slider::new(&mut db, -24.0..=26.0)
                                .suffix(" dB")
                                .fixed_decimals(1)
                                .show_value(true),
                        );
                        // Géométrie journalisée (debug) : l'audit clique un curseur
                        // précis sans deviner les pixels.
                        tracing::debug!(
                            target: "kinterface",
                            "prefs eq bande {} rect {:?} valeur={:.3}",
                            i + 1,
                            reponse.rect,
                            db_actuel
                        );
                        // PIÈGE egui : `fixed_decimals` réécrit la valeur ARRONDIE dans
                        // la variable (et `.changed()` devient vrai), même sans
                        // interaction. Sans le seuil ci-dessous, chaque affichage
                        // modifiait les gains : ×2 devenait ×1,9953 (mesuré), et la
                        // courbe ne correspondait plus à aucun profil. On n'agit donc
                        // que si l'écart dépasse l'arrondi d'affichage.
                        if reponse.changed() && (db - db_actuel).abs() > 0.05f32 {
                            app.config.audio.eq_gains[i] =
                                (10f32.powf(db / 20.0)).clamp(0.0, 20.0);
                            egaliseur_modifie = true;
                        }
                        // Deux paires (libellé, curseur) par ligne.
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });

            ui.add_space(4.0);
            if ui.button("↺ Toutes les bandes à 0 dB").clicked() {
                app.config.audio.eq_gains = vec![1.0; crate::config::EQ_BANDS];
                egaliseur_modifie = true;
            }
            aide(
                ui,
                "Gains mesurés par le banc d'essai : ×4 = +12 dB, ×0,25 = −12 dB \
                 (tools/verif-audio.sh).",
            );
        }
    });


    // --- Traitement audio (équivalents des réglages PotPlayer) ---
    sous_section(ui, "Traitement", |ui| {
        let mut change = false;

        if ui
            .checkbox(&mut app.config.audio.normalizer, "Normaliseur de volume")
            .on_hover_text(
                "Égalise le niveau entre les scènes calmes et les passages forts \
                 (filtre dynaudnorm). Converge en ~2 s : l'effet devient net après \
                 quelques secondes de lecture.",
            )
            .changed()
        {
            change = true;
        }
        if ui
            .checkbox(&mut app.config.audio.night_mode, "Mode nuit (compression)")
            .on_hover_text(
                "Réduit l'écart entre les murmures et les explosions, sans écrêter \
                 (acompressor + limiteur). Mesuré : écart fort/faible 24 dB → 14 dB.",
            )
            .changed()
        {
            change = true;
        }

        let mut bruit = app.config.audio.noise_reduction.min(3);
        ligne(ui, "grille_audio_bruit", "Réduction de bruit", |ui| {
            if ui
                .add(egui::Slider::new(&mut bruit, 0..=3).custom_formatter(|n, _| {
                    match n as u32 {
                        0 => "arrêtée".to_string(),
                        1 => "légère".to_string(),
                        2 => "moyenne".to_string(),
                        _ => "forte".to_string(),
                    }
                }))
                .on_hover_text(
                    "Filtre afftdn. Mesuré sur un son noyé dans du bruit : rapport \
                     signal/bruit 19 dB → 43 dB au niveau fort, sans atténuer le signal.",
                )
                .changed()
            {
                app.config.audio.noise_reduction = bruit;
                change = true;
            }
        });

        let mut large = app.config.audio.stereo_widen.min(3);
        ligne(ui, "grille_audio_large", "Élargissement stéréo", |ui| {
            if ui
                .add(egui::Slider::new(&mut large, 0..=3).custom_formatter(|n, _| {
                    match n as u32 {
                        0 => "arrêté".to_string(),
                        1 => "léger".to_string(),
                        2 => "moyen".to_string(),
                        _ => "large".to_string(),
                    }
                }))
                .on_hover_text(
                    "Filtre extrastereo : augmente l'écart entre canaux (mesuré ×2,1 au \
                     maximum). Sans effet sur une source mono.",
                )
                .changed()
            {
                app.config.audio.stereo_widen = large;
                change = true;
            }
        });

        if ui
            .checkbox(&mut app.config.audio.crossfeed, "Crossfeed (écoute au casque)")
            .on_hover_text(
                "Adoucit la séparation gauche/droite (mesuré : écart entre canaux \
                 divisé par 2,3) — moins fatigant sur une longue écoute.",
            )
            .changed()
        {
            change = true;
        }
        if ui
            .checkbox(&mut app.config.audio.karaoke, "Suppression de la voix (karaoké)")
            .on_hover_text(
                "Retire le canal central : la voix disparaît quand elle est au centre \
                 (mesuré : un son centré tombe à 0,003 % de son niveau).",
            )
            .changed()
        {
            change = true;
        }

        let mut reverb = app.config.audio.reverb.min(3);
        ligne(ui, "grille_audio_reverb", "Réverbération", |ui| {
            if ui
                .add(egui::Slider::new(&mut reverb, 0..=3).custom_formatter(|n, _| {
                    match n as u32 {
                        0 => "arrêtée".to_string(),
                        1 => "petite pièce".to_string(),
                        2 => "pièce moyenne".to_string(),
                        _ => "grande salle".to_string(),
                    }
                }))
                .on_hover_text("Filtre aecho : ajoute des réflexions (mesuré sur une impulsion).")
                .changed()
            {
                app.config.audio.reverb = reverb;
                change = true;
            }
        });
        // Équilibre direct / réverbéré : grisé quand la réverbération est arrêtée
        // (un réglage sans effet visible est un piège).
        let reverb_active = app.config.audio.reverb > 0;
        ui.add_enabled_ui(reverb_active, |ui| {
            let mut mix = app.config.audio.reverb_mix;
            ligne(ui, "grille_audio_reverb_mix", "Part de réverbération", |ui| {
                if ui
                    .add(
                        egui::Slider::new(&mut mix, 0.0..=1.0)
                            .custom_formatter(|n, _| format!("{:.0} %", n * 100.0)),
                    )
                    .on_hover_text(
                        "Équilibre son direct / réflexions (aecho : in_gain / out_gain). \
                         0 % = presque sec, 100 % = très humide — le direct baisse un peu \
                         quand on monte, sinon le total sature.",
                    )
                    .changed()
                {
                    app.config.audio.reverb_mix = mix;
                    change = true;
                }
            });
        });
        if !reverb_active {
            aide(ui, "Activez d'abord la réverbération ci-dessus.");
        }

        let mut retard = app.config.audio.delay_ms;
        ligne(ui, "grille_audio_delay", "Décalage audio", |ui| {
            if ui
                .add(
                    egui::Slider::new(&mut retard, -2000..=2000)
                        .suffix(" ms")
                        .fixed_decimals(0),
                )
                .on_hover_text(
                    "Synchronisation lèvres/son : positif = retarder le son, \
                     négatif = couper le début (mesuré : +500 ms décale de 510 ms).",
                )
                .changed()
            {
                app.config.audio.delay_ms = retard;
                change = true;
            }
        });

        ui.add_space(4.0);
        if ui
            .checkbox(
                &mut app.config.audio.pitch_preserved,
                "Conserver la hauteur du son à vitesse ≠ 1",
            )
            .on_hover_text(
                "Filtre atempo : à 2×, un son de 440 Hz reste à 440 Hz (mesuré). \
                 Sans cette option, la vitesse modifie la hauteur.",
            )
            .changed()
        {
            change = true;
        }

        ui.add_space(4.0);
        if ui.button("↺ Désactiver tous les traitements").clicked() {
            app.config.audio.reset_processing();
            change = true;
        }

        // Résumé honnête : ce qui est réellement inséré dans la chaîne. La chaîne
        // brute est longue (18 bandes + plusieurs filtres) : elle est donc repliée,
        // avec le nombre de filtres actifs visible d'un coup d'œil.
        let chaine = app.player.audio_filters_description();
        ui.add_space(6.0);
        if chaine.is_empty() {
            ui.label(
                egui::RichText::new("chaîne audio : aucune (signal d'origine)")
                    .small()
                    .weak(),
            );
        } else {
            let nb = chaine.split(',').count();
            egui::CollapsingHeader::new(format!("chaîne audio : {} filtre(s)", nb))
                .id_salt("chaine_audio_detail")
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(chaine.to_string()).small().monospace());
                });
        }

        if change {
            app.player.set_audio_filters(&app.config.audio);
            let _ = app.config.save();
            egaliseur_modifie = true;
        }
    });

    if egaliseur_modifie {
        // Un seul point d'application : le graphe audio est reconstruit à la
        // première trame suivante.
        app.player.set_audio_filters(&app.config.audio);
        let _ = app.config.save();
    }
}

fn dessiner_sous_titres(app: &mut App, ui: &mut egui::Ui) {
    sous_section(ui, "Chargement", |ui| {
        if ui
            .checkbox(
                &mut app.config.subtitles.auto_load,
                "Charger les sous-titres homonymes",
            )
            .changed()
        {
            let _ = app.config.save();
        }
        ligne(ui, "grille_langue", "Langue préférée", |ui| {
            if ui
                .add(egui::TextEdit::singleline(
                    &mut app.config.subtitles.default_language,
                ))
                .on_hover_text("Suffixe prioritaire : .fr.srt, .en.ass…")
                .changed()
            {
                let _ = app.config.save();
            }
        });
        ligne(ui, "grille_alphabet", "Alphabet (encodage)", |ui| {
            if ui
                .add(egui::TextEdit::singleline(&mut app.config.subtitles.encoding).hint_text("automatique"))
                .on_hover_text(
                    "Vide = automatique (UTF-8, sinon windows-1252). Sinon un nom d'encodage : \
                     windows-1250 (Europe centrale), windows-1251 (cyrillique), iso-8859-15, gbk, shift_jis…",
                )
                .changed()
            {
                let _ = app.config.save();
            }
        });
        // Langues par ordre de priorité (session 37) : « fre, eng » essaie fre, puis eng.
        // Le texte en cours de frappe est gardé à part : le recomposer depuis la liste à
        // chaque image effacerait la virgule qu'on vient de taper.
        let id_saisie = egui::Id::new("saisie_langues_sous_titres");
        let mut saisie: String = ui.memory_mut(|m| {
            m.data
                .get_temp_mut_or_insert_with(id_saisie, || {
                    app.config.subtitles.preferred_languages.join(", ")
                })
                .clone()
        });
        ligne(ui, "grille_langues", "Langues (priorité)", |ui| {
            if ui
                .add(egui::TextEdit::singleline(&mut saisie).hint_text("fre, eng"))
                .on_hover_text(
                    "Plusieurs langues séparées par des virgules, la plus voulue d'abord. \
                     Vide = seulement la langue préférée ci-dessus.",
                )
                .changed()
            {
                app.config.subtitles.preferred_languages = saisie
                    .split(',')
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect();
                let _ = app.config.save();
                app.player.set_track_preferences(
                    &app.config.audio.langues_preferees(),
                    &app.config.subtitles.langues_preferees(),
                    app.config.subtitles.use_embedded,
                );
            }
        });
        ui.memory_mut(|m| m.data.insert_temp(id_saisie, saisie));
        if ui
            .checkbox(
                &mut app.config.subtitles.remember_track,
                "Mémoriser la piste choisie pour chaque film",
            )
            .on_hover_text("La piste choisie à la main (touche J, menu Sous-titres) revient à la réouverture du même film")
            .changed()
        {
            let _ = app.config.save();
        }
        let mut st = app.config.subtitles.clone();
        let mut change = false;
        let mut mini = st.min_duration_ms as f32;
        let mut maxi = st.max_duration_ms as f32;
        change |= curseur_st(ui, "grille_duree_min", "Durée minimale d'affichage",
            "Un sous-titre trop bref reste affiché au moins ce temps (0 = durée du fichier)",
            &mut mini, 0.0..=10_000.0, " ms", 0);
        change |= curseur_st(ui, "grille_duree_max", "Durée maximale d'affichage",
            "Un sous-titre trop long disparaît après ce temps (0 = durée du fichier)",
            &mut maxi, 0.0..=20_000.0, " ms", 0);
        if change {
            st.min_duration_ms = mini as u32;
            st.max_duration_ms = maxi as u32;
            app.config.subtitles = st;
            let _ = app.config.save();
        }
    });
    sous_section(ui, "Apparence du texte", |ui| {
        let mut taille = app.config.subtitles.font_size;
        ligne(ui, "grille_police", "Taille", |ui| {
            if ui
                .add(egui::Slider::new(&mut taille, 8.0..=160.0).suffix(" pt"))
                .changed()
            {
                app.config.subtitles.font_size = taille;
                let _ = app.config.save();
            }
        });

        // --- Famille de police : la seule chose qui manquait encore par rapport à
        // PotPlayer (« Sous-titres › Style police » : Police, Alphabet, Interlettre).
        // Le nom est résolu par fontconfig au moment où il change, pas à chaque image.
        let mut famille = app.config.subtitles.font_family.clone();
        ligne(ui, "grille_police_subs", "Police", |ui| {
            let reponse = ui.add(
                egui::TextEdit::singleline(&mut famille)
                    .desired_width(180.0)
                    .hint_text("police par défaut"),
            );
            tracing::debug!(
                target: "kinterface",
                "prefs champ police sous-titres rect {:?}",
                reponse.rect
            );
            if reponse.changed() && famille != app.config.subtitles.font_family {
                app.config.subtitles.font_family = famille.clone();
                let _ = app.config.save();
                // Installation réelle dans egui : sans cet appel, le réglage serait
                // enregistré sans effet (défaut déjà vu ailleurs dans ce projet).
                crate::ui::installer_police_sous_titre(ui.ctx(), &famille);
                tracing::info!(
                    target: "kinterface",
                    "police des sous-titres : « {} »",
                    famille
                );
                ui.ctx().request_repaint();
            }
        });
        // Choix rapide parmi les familles présentes sur la machine.
        ui.horizontal(|ui| {
            ui.label("Disponibles :");
            egui::ComboBox::from_id_salt("choix_police_subs")
                .selected_text(if famille.is_empty() {
                    "par défaut".to_string()
                } else {
                    famille.clone()
                })
                .width(200.0)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(famille.is_empty(), "par défaut").clicked() {
                        app.config.subtitles.font_family = String::new();
                        let _ = app.config.save();
                        crate::ui::installer_police_sous_titre(ui.ctx(), "");
                        ui.ctx().request_repaint();
                    }
                    for nom in FAMILLES_POLICES.iter() {
                        if ui.selectable_label(&famille == nom, *nom).clicked() {
                            app.config.subtitles.font_family = nom.to_string();
                            let _ = app.config.save();
                            crate::ui::installer_police_sous_titre(ui.ctx(), nom);
                            tracing::info!(
                                target: "kinterface",
                                "police des sous-titres choisie : « {} »",
                                nom
                            );
                            ui.ctx().request_repaint();
                        }
                    }
                });
        });
        aide(
            ui,
            "Nom d'une police installée (« DejaVu Serif », « Liberation Mono »…). \
             Vide = police du lecteur. Ne s'applique qu'au texte : un ASS porte sa \
             propre mise en forme, et un sous-titre image (PGS/DVD) est déjà dessiné.",
        );

        // --- Couleurs : deux champs hexadécimaux + un aperçu ---
        let mut couleur = app.config.subtitles.color.clone();
        let mut contour = app.config.subtitles.outline_color.clone();
        ligne(ui, "grille_coul_texte", "Couleur du texte", |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut couleur).desired_width(70.0));
                if let Some(c) = parse_couleur_ui(&couleur) {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(22.0, 16.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2, c);
                }
                if couleur != app.config.subtitles.color {
                    app.config.subtitles.color = couleur.clone();
                    let _ = app.config.save();
                }
            });
        });
        ligne(ui, "grille_coul_contour", "Contour", |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut contour).desired_width(70.0));
                if let Some(c) = parse_couleur_ui(&contour) {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(22.0, 16.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2, c);
                }
                if contour != app.config.subtitles.outline_color {
                    app.config.subtitles.outline_color = contour.clone();
                    let _ = app.config.save();
                }
            });
        });
        aide(ui, "Format RRGGBB (ex. FFFFFF = blanc, 000000 = noir).");

        let mut epaisseur = app.config.subtitles.outline_width;
        ligne(ui, "grille_contour_ep", "Épaisseur du contour", |ui| {
            if ui
                .add(egui::Slider::new(&mut epaisseur, 0.0..=6.0).suffix(" px").fixed_decimals(1))
                .changed()
            {
                app.config.subtitles.outline_width = epaisseur;
                let _ = app.config.save();
            }
        });

        let mut fond = app.config.subtitles.background_alpha;
        ligne(ui, "grille_fond", "Bandeau de fond", |ui| {
            if ui
                .add(egui::Slider::new(&mut fond, 0..=255).custom_formatter(|n, _| {
                    if n < 1.0 { "aucun".to_string() } else { format!("{} / 255", n as u32) }
                }))
                .changed()
            {
                app.config.subtitles.background_alpha = fond;
                let _ = app.config.save();
            }
        });

        let mut position = app.config.subtitles.position_percent;
        ligne(ui, "grille_position", "Position verticale", |ui| {
            if ui
                .add(
                    egui::Slider::new(&mut position, 0.0..=100.0)
                        .suffix(" %")
                        .custom_formatter(|n, _| {
                            if n < 10.0 { "haut".to_string() }
                            else if n > 90.0 { "bas".to_string() }
                            else { format!("{} %", n as u32) }
                        }),
                )
                .changed()
            {
                app.config.subtitles.position_percent = position;
                let _ = app.config.save();
            }
        });

        let mut position_h = app.config.subtitles.position_h_percent;
        ligne(ui, "grille_position_h", "Position horizontale", |ui| {
            if ui
                .add(
                    egui::Slider::new(&mut position_h, 0.0..=100.0)
                        .suffix(" %")
                        .custom_formatter(|n, _| {
                            if (n - 50.0).abs() < 0.5 {
                                "centre".to_string()
                            } else if n < 5.0 {
                                "bord gauche".to_string()
                            } else if n > 95.0 {
                                "bord droit".to_string()
                            } else {
                                format!("{} %", n as u32)
                            }
                        }),
                )
                .on_hover_text(
                    "Point d'ancrage du bloc de texte, en pourcentage de la largeur. \
                     50 % = centre (valeur par défaut).",
                )
                .changed()
            {
                app.config.subtitles.position_h_percent = position_h;
                let _ = app.config.save();
            }
        });

        // Interlettre et marges latérales (session 34) : les deux réglages de
        // « Dialog(383) » de PotPlayer qui manquaient encore à cette section.
        let mut interlettre = app.config.subtitles.letter_spacing;
        ligne(ui, "grille_interlettre", "Interlettre", |ui| {
            if ui
                .add(
                    egui::Slider::new(&mut interlettre, -3.0..=12.0)
                        .suffix(" pt")
                        .fixed_decimals(1),
                )
                .on_hover_text(
                    "Espace ajouté entre deux caractères : négatif = resserré, positif = aéré",
                )
                .changed()
            {
                app.config.subtitles.letter_spacing = interlettre;
                let _ = app.config.save();
            }
        });
        let mut marge_g = app.config.subtitles.margin_left_percent;
        ligne(ui, "grille_marge_g", "Marge gauche", |ui| {
            if ui
                .add(egui::Slider::new(&mut marge_g, 0.0..=40.0).suffix(" %").fixed_decimals(0))
                .on_hover_text("Borne la zone de texte : un sous-titre long ne déborde plus à gauche")
                .changed()
            {
                app.config.subtitles.margin_left_percent = marge_g;
                let _ = app.config.save();
            }
        });
        let mut marge_d = app.config.subtitles.margin_right_percent;
        ligne(ui, "grille_marge_d", "Marge droite", |ui| {
            if ui
                .add(egui::Slider::new(&mut marge_d, 0.0..=40.0).suffix(" %").fixed_decimals(0))
                .on_hover_text("Idem à droite")
                .changed()
            {
                app.config.subtitles.margin_right_percent = marge_d;
                let _ = app.config.save();
            }
        });

        // Mise en page et effets (session 37).
        let mut st = app.config.subtitles.clone();
        let mut change = false;
        change |= curseur_st(ui, "grille_marge_h", "Marge haute", "Borne la zone verticale par le haut",
            &mut st.margin_top_percent, 0.0..=40.0, " %", 0);
        change |= curseur_st(ui, "grille_marge_b", "Marge basse", "Borne la zone verticale par le bas",
            &mut st.margin_bottom_percent, 0.0..=40.0, " %", 0);
        change |= curseur_st(ui, "grille_interligne", "Interligne", "1,25 = valeur d'origine",
            &mut st.line_spacing, 0.8..=3.0, " ×", 2);
        change |= curseur_st(ui, "grille_echelle_x", "Échelle horizontale", "Étire le texte en largeur",
            &mut st.scale_x, 0.5..=2.0, " ×", 2);
        change |= curseur_st(ui, "grille_echelle_y", "Échelle verticale", "Étire le texte en hauteur",
            &mut st.scale_y, 0.5..=2.0, " ×", 2);
        change |= curseur_st(ui, "grille_contour2_ep", "Second contour", "Épaisseur d'un contour autour du premier (0 = aucun)",
            &mut st.outline2_width, 0.0..=6.0, " px", 1);
        change |= champ_couleur_st(ui, "grille_contour2_coul", "Couleur du second contour", &mut st.outline2_color);
        change |= curseur_st(ui, "grille_ombre", "Ombre", "Décalage vers le bas-droite (0 = pas d'ombre)",
            &mut st.shadow_offset, 0.0..=12.0, " px", 1);
        change |= champ_couleur_st(ui, "grille_ombre_coul", "Couleur de l'ombre", &mut st.shadow_color);
        let mut fondu = st.fade_ms as f32;
        if curseur_st(ui, "grille_fondu", "Fondu", "Durée d'apparition et de disparition (0 = coupe franche)",
            &mut fondu, 0.0..=2000.0, " ms", 0)
        {
            st.fade_ms = fondu as u32;
            change = true;
        }
        if change {
            app.config.subtitles = st;
            let _ = app.config.save();
        }

        let mut align = app.config.subtitles.align;
        ligne(ui, "grille_align", "Alignement", |ui| {
            egui::ComboBox::from_id_salt("choix_align_subs")
                .selected_text(match align {
                    0 => "Gauche",
                    2 => "Droite",
                    _ => "Centre",
                })
                .width(140.0)
                .show_ui(ui, |ui| {
                    for (v, t) in [(0u8, "Gauche"), (1, "Centre"), (2, "Droite")] {
                        if ui.selectable_value(&mut align, v, t).clicked() {
                            app.config.subtitles.align = v;
                            let _ = app.config.save();
                        }
                    }
                });
        });

        ui.horizontal(|ui| {
            let mut gras = app.config.subtitles.bold;
            if ui.checkbox(&mut gras, "Gras").changed() {
                app.config.subtitles.bold = gras;
                let _ = app.config.save();
            }
            let mut italique = app.config.subtitles.italic;
            if ui.checkbox(&mut italique, "Italique").changed() {
                app.config.subtitles.italic = italique;
                let _ = app.config.save();
            }
        });
    });

    sous_section(ui, "Synchronisation", |ui| {
        let mut retard = app.config.subtitles.delay_ms;
        ligne(ui, "grille_delai_subs", "Décalage", |ui| {
            if ui
                .add(
                    egui::Slider::new(&mut retard, -10_000..=10_000)
                        .suffix(" ms")
                        .fixed_decimals(0),
                )
                .on_hover_text("Positif = sous-titres plus tard ; négatif = plus tôt")
                .changed()
            {
                app.config.subtitles.delay_ms = retard;
                let _ = app.config.save();
            }
        });
        aide(
            ui,
            "Corrige un fichier dont les temps sont décalés. S'applique aussi aux pistes \
             du conteneur. Touches : Ctrl+Maj+←/→ dans VLC, ici ce curseur.",
        );
    });
}

fn dessiner_lecture(app: &mut App, ui: &mut egui::Ui) {
    sous_section(ui, "Reprise", |ui| {
        if ui
            .checkbox(
                &mut app.config.playback.resume,
                "Reprendre à la position quittée",
            )
            .changed()
        {
            app.player.set_resume(app.config.playback.resume);
            let _ = app.config.save();
        }
    });
    sous_section(ui, "Fenêtre et lecture", |ui| {
        // Pause / reprise selon le focus : deux réglages distincts, parce qu'ils
        // rendent service séparément (pauser en changeant de fenêtre n'implique pas
        // de reprendre au retour).
        let mut mode = app.config.interface.pause_sur_perte_focus.min(2);
        ligne(ui, "grille_pause_focus", "Mettre en pause", |ui| {
            egui::ComboBox::from_id_salt("choix_pause_focus")
                .selected_text(match mode {
                    0 => "jamais",
                    1 => "dès que la fenêtre perd le focus",
                    _ => "seulement si la fenêtre est réduite",
                })
                .width(300.0)
                .show_ui(ui, |ui| {
                    for (v, texte) in [
                        (0u8, "jamais"),
                        (1u8, "dès que la fenêtre perd le focus"),
                        (2u8, "seulement si la fenêtre est réduite"),
                    ] {
                        ui.selectable_value(&mut mode, v, texte);
                    }
                });
            if mode != app.config.interface.pause_sur_perte_focus {
                app.config.interface.pause_sur_perte_focus = mode;
                let _ = app.config.save();
                tracing::info!("Pause sur perte de focus : mode {}", mode);
            }
        });
        ui.add_enabled_ui(mode > 0, |ui| {
            if ui
                .checkbox(
                    &mut app.config.interface.reprendre_sur_focus,
                    "Reprendre au retour au premier plan",
                )
                .on_hover_text(
                    "Ne reprend que les pauses décidées par la perte de focus : une pause \
                     volontaire reste une pause",
                )
                .changed()
            {
                let _ = app.config.save();
            }
        });
        if mode == 0 {
            aide(ui, "La lecture continue quand la fenêtre est en arrière-plan.");
        }
    });

    sous_section(ui, "Playlist", |ui| {
        // Les deux modes sont ici **et** dans le panneau (boutons d'état) : la
        // préférence est l'endroit où on les cherche, le panneau l'endroit où on
        // les bascule en écoutant.
        if ui
            .checkbox(
                &mut app.config.playback.shuffle,
                "Lecture aléatoire",
            )
            .on_hover_text("Chaque élément de la playlist passe une fois avant qu'un cycle recommence (Ctrl+A)")
            .changed()
        {
            let actif = app.config.playback.shuffle;
            app.player.set_shuffle(actif);
            let _ = app.config.save();
            tracing::info!("Lecture aléatoire (préférences) : {}", actif);
        }
        // La boucle vit dans le moteur (état de séance), pas dans la configuration :
        // une copie locale suffit pour la case à cocher, et c'est le moteur qui
        // décide ensuite.
        let mut boucle = app.player.loop_enabled;
        if ui
            .checkbox(&mut boucle, "Lecture en boucle")
            .on_hover_text("Relire la playlist en boucle (Ctrl+L)")
            .changed()
        {
            app.player.toggle_loop();
            tracing::info!("Boucle (préférences) : {}", app.player.loop_enabled);
        }
        aide(
            ui,
            "Dans le panneau de playlist (L), les éléments se réordonnent par              glisser-déposer ; l'ordre est enregistré et retrouvé au lancement.",
        );
    });

    sous_section(ui, "Flux réseau : pré-cache", |ui| {
        // Sans réserve, chaque segment est demandé au moment exact où il devient
        // nécessaire : la moindre gigue se voit. Avec réserve, le lecteur lit en
        // avance et une coupure courte passe inaperçue (le prix est la mémoire).
        let mut ms = app.config.playback.precache_ms;
        ligne(ui, "grille_precache", "Réserve", |ui| {
            let reponse = ui
                .add(
                    egui::Slider::new(&mut ms, 0..=crate::audio::CAPACITE_MAX_MS)
                        .suffix(" ms")
                        .custom_formatter(|v, _| {
                            if v < 1.0 {
                                "désactivé".to_string()
                            } else {
                                format!("{:.0} s", v / 1000.0)
                            }
                        }),
                )
                .on_hover_text(
                    "Profondeur lue en avance sur un flux (http, HLS…) : la mémoire                      consommée est d'environ 350 Ko par seconde de réserve",
                );
            tracing::debug!(
                target: "kinterface",
                "prefs curseur precache rect {:?}",
                reponse.rect
            );
            if reponse.changed() {
                app.config.playback.precache_ms = ms;
                app.player.set_precache_ms(ms);
                let _ = app.config.save();
                tracing::info!(
                    target: "kinterface",
                    "pré-cache réseau réglé à {} ms",
                    ms
                );
            }
        });
        aide(
            ui,
            "S'applique aux flux (http, HLS, RTSP…) : le lecteur garde le média en              avance et traverse une coupure courte sans s'arrêter. Un fichier local              n'est jamais pré-caché (le disque ne prend pas de retard).",
        );
    });

    sous_section(ui, "Système", |ui| {
        let mut auto = crate::systeme::autostart_actif();
        if ui
            .checkbox(&mut auto, "Lancer KPlayer à l'ouverture de session")
            .on_hover_text("Écrit (ou retire) ~/.config/autostart/kplayer.desktop")
            .changed()
        {
            match crate::systeme::regler_autostart(auto) {
                Ok(()) => app.annonce(if auto { "Démarrage automatique activé" } else { "Démarrage automatique désactivé" }),
                Err(e) => app.error_message = Some(format!("Démarrage automatique : {e}")),
            }
        }
        let mut zone = app.config.interface.tray_icon;
        if ui
            .checkbox(&mut zone, "Icône dans la zone de notification")
            .on_hover_text("Clic : afficher / masquer la fenêtre ; menu : lecture, suivant, précédent, quitter")
            .changed()
        {
            app.config.interface.tray_icon = zone;
            let _ = app.config.save();
        }
        let mut priorite = app.config.playback.priority;
        ligne(ui, "grille_priorite", "Priorité du processus", |ui| {
            egui::ComboBox::from_id_salt("choix_priorite")
                .selected_text(priorite.label())
                .width(140.0)
                .show_ui(ui, |ui| {
                    use crate::systeme::Priorite;
                    for choix in [Priorite::Basse, Priorite::Normale, Priorite::Elevee] {
                        ui.selectable_value(&mut priorite, choix, choix.label());
                    }
                });
        });
        if priorite != app.config.playback.priority {
            app.config.playback.priority = priorite;
            let _ = app.config.save();
            match crate::systeme::appliquer_priorite(priorite) {
                Ok(()) => app.annonce(&format!("Priorité : {}", priorite.label())),
                Err(e) => app.error_message = Some(e),
            }
        }
        aide(ui, "« Élevée » aide en 4K quand le processeur est chargé ; « Basse » laisse la main aux autres programmes.");
    });

    sous_section(ui, "Répétition A-B", |ui| {
        // Touches A et B : posent les deux points. Ici : combien de fois la boucle tourne,
        // et le temps de pause entre deux passages.
        let mut retours = app.config.playback.ab_repetitions;
        let mut delai = app.config.playback.ab_delay_ms;
        ligne(ui, "grille_ab_retours", "Retours", |ui| {
            ui.add(
                egui::Slider::new(&mut retours, 0..=crate::config::AB_REPETITIONS_MAX)
                    .custom_formatter(|v, _| {
                        if v < 1.0 { "sans fin".to_string() } else { format!("{:.0}", v) }
                    }),
            )
            .on_hover_text("Nombre de retours au point A avant que le film reprenne son cours");
        });
        ligne(ui, "grille_ab_delai", "Pause entre deux passages", |ui| {
            ui.add(
                egui::Slider::new(&mut delai, 0..=crate::config::AB_DELAI_MAX_MS)
                    .custom_formatter(|v, _| {
                        if v < 1.0 { "aucune".to_string() } else { format!("{:.1} s", v / 1000.0) }
                    }),
            )
            .on_hover_text("Temps d'arrêt en B avant de repartir de A");
        });
        if retours != app.config.playback.ab_repetitions || delai != app.config.playback.ab_delay_ms {
            app.config.playback.ab_repetitions = retours;
            app.config.playback.ab_delay_ms = delai;
            app.player.set_boucle_ab(retours, delai);
            let _ = app.config.save();
        }
    });

    sous_section(ui, "Vitesse", |ui| {
        let mut vitesse = app.player.get_speed();
        ligne(ui, "grille_vitesse", "Vitesse", |ui| {
            if ui
                .add(
                    egui::Slider::new(
                        &mut vitesse,
                        crate::player::VITESSE_MIN..=crate::player::VITESSE_MAX,
                    )
                        .suffix(" ×")
                        .fixed_decimals(2),
                )
                .changed()
            {
                app.player.set_speed(vitesse);
            }
        });
        if app.config.audio.pitch_preserved {
            aide(
                ui,
                "La hauteur du son est conservée (atempo) : à 2×, la durée est divisée \
                 par deux et le timbre reste le même.",
            );
        } else {
            aide(
                ui,
                "Sans correction de hauteur, la vitesse modifie le timbre (effet bande \
                 magnétique). L'option se règle dans la section Audio.",
            );
        }
    });
}

// ---------------------------------------------------------------------------
// Libellés et détection « section modifiée »
// ---------------------------------------------------------------------------

fn scaling_label(methode: &ScalingMethod) -> &'static str {
    match methode {
        ScalingMethod::NearestNeighbor => "Plus proche voisin (le plus net, pixelisé)",
        ScalingMethod::Bilinear => "Bilinéaire (rapide)",
        ScalingMethod::Bicubic => "Bicubique (compromis)",
        ScalingMethod::Lanczos => "Lanczos (le plus net en réduction)",
        ScalingMethod::Area => "Moyenne de zone (réduction forte)",
    }
}

/// Profil dont les gains correspondent exactement aux réglages courants
/// (`None` = courbe personnalisée).
/// Convertit « RRGGBB » en couleur, pour l'aperçu des préférences.
fn parse_couleur_ui(texte: &str) -> Option<egui::Color32> {
    let t = texte.trim().trim_start_matches('#');
    if t.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&t[0..2], 16).ok()?;
    let v = u8::from_str_radix(&t[2..4], 16).ok()?;
    let b = u8::from_str_radix(&t[4..6], 16).ok()?;
    Some(egui::Color32::from_rgb(r, v, b))
}

/// Gain linéaire converti en décibels (0,001 borne le logarithme).
fn db(gain: f32) -> f32 {
    20.0 * gain.max(1e-3).log10()
}

fn preset_courant(audio: &crate::config::AudioSettings) -> Option<EqPreset> {
    for preset in [
        EqPreset::Flat,
        EqPreset::Bass,
        EqPreset::Voice,
        EqPreset::Treble,
        EqPreset::Loudness,
    ] {
        let gains = preset.gains();
        // Comparaison en DÉCIBELS : l'affichage est au dixième de dB, donc un gain
        // de 2,0 peut être stocké 1,9953 après un aller-retour par l'interface
        // (0,02 dB d'écart). Comparer les gains linéaires ferait croire à une
        // courbe personnalisée alors que c'est le profil « Basses renforcées ».
        let identique = gains
            .iter()
            .enumerate()
            .all(|(i, g)| (db(audio.gain(i)) - db(*g)).abs() < 0.15);
        if identique {
            return Some(preset);
        }
    }
    None
}

/// Polices proposées dans le sélecteur.
///
/// Liste fixe plutôt qu'un appel à `fc-list` : celle-ci coûte plusieurs dizaines de
/// millisecondes et rendrait la fenêtre saccadée si elle était refaite à chaque
/// image. Ce sont les familles présentes sur à peu près tout bureau Linux ; le
/// champ de saisie libre couvre les autres (police achetée, police de marque).
const FAMILLES_POLICES: &[&str] = &[
    "DejaVu Sans",
    "DejaVu Serif",
    "DejaVu Sans Mono",
    "Liberation Sans",
    "Liberation Serif",
    "Liberation Mono",
    "Noto Sans",
    "Noto Serif",
    "FreeSans",
    "Ubuntu",
    "Cantarell",
];

/// Libellé du filtre de désentrelacement (le nom du filtre est celui de FFmpeg).
fn filtre_label(f: &crate::config::DeinterlaceFilter) -> &'static str {
    match f {
        crate::config::DeinterlaceFilter::Bwdif => "bwdif (détail préservé)",
        crate::config::DeinterlaceFilter::Yadif => "yadif (classique)",
    }
}

fn parite_label(parite: &DeinterlaceParity) -> &'static str {
    match parite {
        DeinterlaceParity::Auto => "Automatique",
        DeinterlaceParity::TopFirst => "Trame du haut d'abord",
        DeinterlaceParity::BottomFirst => "Trame du bas d'abord",
    }
}

// Comparaisons avec les valeurs par défaut : elles alimentent le compteur du
// bandeau et le repère « • modifié ». Une section marquée modifiée n'est donc
// jamais un mensonge : les champs comparés sont exactement ceux qu'elle dessine.

fn interface_modifiee(app: &App) -> bool {
    let d = Config::default().interface;
    let a = &app.config.interface;
    a.dark_theme != d.dark_theme
        || a.theme != d.theme
        || a.show_title != d.show_title
        || a.show_progress_bar != d.show_progress_bar
        || a.controls_always_visible != d.controls_always_visible
        || a.show_osd != d.show_osd
        || a.osd_duration != d.osd_duration
        || a.screenshots_dir != d.screenshots_dir
}

fn video_modifiee(app: &App) -> bool {
    let d = Config::default().video;
    let a = &app.config.video;
    a.hardware_decoding != d.hardware_decoding
        || a.gpu_scaling != d.gpu_scaling
        || a.scaling_method != d.scaling_method
        || a.sharpen != d.sharpen
        || a.rotate != d.rotate
        || a.flip != d.flip
        || (a.zoom - d.zoom).abs() > 1e-3
        || a.aspect != d.aspect
        || a.max_fps != d.max_fps
        || a.decoders != d.decoders
        || a.color_matrix != d.color_matrix
        || a.color_range != d.color_range
        || a.pan_x_percent != d.pan_x_percent
        || a.pan_y_percent != d.pan_y_percent
        || a.crop_active()
}

fn image_modifiee(app: &App) -> bool {
    let d = Config::default().video;
    let a = &app.config.video;
    a.filters_active()
        || a.deband_range != d.deband_range
        || (a.deband_threshold - d.deband_threshold).abs() > 1e-6
        || a.deinterlace_parity != d.deinterlace_parity
}

fn audio_modifiee(app: &App) -> bool {
    let d = Config::default().audio;
    let a = &app.config.audio;
    a.device != d.device
        || a.volume_max != d.volume_max
        || a.preferred_languages != d.preferred_languages
        || a.remember_track != d.remember_track
        || a.eq_enabled != d.eq_enabled
        || !a.eq_is_neutral()
        || a.processing_active()
        || a.pitch_preserved != d.pitch_preserved
}

fn sous_titres_modifies(app: &App) -> bool {
    let d = Config::default().subtitles;
    let a = &app.config.subtitles;
    a.auto_load != d.auto_load
        || (a.font_size - d.font_size).abs() > 1e-6
        || a.default_language != d.default_language
        || a.color != d.color
        || a.outline_color != d.outline_color
        || (a.outline_width - d.outline_width).abs() > 1e-6
        || a.background_alpha != d.background_alpha
        || (a.position_percent - d.position_percent).abs() > 1e-6
        || a.align != d.align
        || a.bold != d.bold
        || a.italic != d.italic
        || a.delay_ms != d.delay_ms
        || a.margin_top_percent != d.margin_top_percent
        || a.margin_bottom_percent != d.margin_bottom_percent
        || a.line_spacing != d.line_spacing
        || a.shadow_offset != d.shadow_offset
        || a.shadow_color != d.shadow_color
        || a.outline2_width != d.outline2_width
        || a.outline2_color != d.outline2_color
        || a.scale_x != d.scale_x
        || a.scale_y != d.scale_y
        || a.fade_ms != d.fade_ms
        || a.preferred_languages != d.preferred_languages
        || a.encoding != d.encoding
        || a.min_duration_ms != d.min_duration_ms
        || a.max_duration_ms != d.max_duration_ms
        || a.remember_track != d.remember_track
}

fn lecture_modifiee(app: &App) -> bool {
    let d = Config::default().playback;
    let a = &app.config.playback;
    a.resume != d.resume
        || a.ab_repetitions != d.ab_repetitions
        || a.ab_delay_ms != d.ab_delay_ms
        || a.priority != d.priority
        || crate::systeme::autostart_actif()
}

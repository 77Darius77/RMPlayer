//! Fenêtres flottantes détachables.
//!
//! Une `egui::Window` est dessinée DANS la fenêtre du lecteur : elle ne peut ni en sortir ni être
//! posée sur un autre écran, et une petite fenêtre de lecteur la rogne. « Détacher » la rend à
//! l'état de vraie fenêtre système (viewport immédiat d'egui/eframe) : le gestionnaire de fenêtres
//! la déplace, la redimensionne et la pose où l'on veut, y compris hors du lecteur.
//!
//! **Un seul contenu, deux enveloppes** : l'appelant fournit le contenu une fois ; il est dessiné
//! soit dans une `egui::Window` (rattachée), soit dans le panneau central d'un viewport (détachée).
//! La mémoire du choix est `interface.detached_windows` (lue au lancement par `est_detachee`).

use eframe::egui;

/// Ce que `afficher` rapporte à l'appelant.
pub struct Retour {
    /// Rectangle de la fenêtre rattachée (pour le journal de géométrie des audits).
    pub rect: Option<egui::Rect>,
    /// L'utilisateur a cliqué « Détacher » / « Rattacher » : à l'appelant de basculer l'état.
    pub bascule: bool,
}

/// Géométrie du bouton, journalisée (debug) : les bancs cliquent là où le programme l'a dessiné
/// sans deviner de pixels. Le repère est celui de la fenêtre qui porte le bouton.
fn journaliser(id: &str, rect: egui::Rect, detachee: bool) {
    tracing::debug!(
        target: "kinterface",
        "fenetre-detacher {} rect {:?} detachee={}",
        id,
        rect,
        detachee
    );
}

/// Dessine `contenu` dans une fenêtre rattachée (`fenetre`) ou, si `detachee`, dans une fenêtre
/// système de titre « RMPlayer — <nom> ».
///
/// - `ouvert` : passe à `false` à la fermeture (✕, bouton du gestionnaire de fenêtres, Échap dans
///   la fenêtre détachée) ;
/// - `defilement` : enveloppe le contenu d'une zone défilante quand la fenêtre est détachée (le
///   contenu qui défile déjà lui-même — les Préférences — passe `false`) ;
/// - `contenu(ui, detachee)` : le second argument permet d'adapter la hauteur disponible.
#[allow(clippy::too_many_arguments)]
pub fn afficher<'a>(
    ctx: &egui::Context,
    id: &'static str,
    nom: &str,
    detachee: bool,
    ouvert: &'a mut bool,
    taille_detachee: [f32; 2],
    defilement: bool,
    fenetre: egui::Window<'a>,
    mut contenu: impl FnMut(&mut egui::Ui, bool),
) -> Retour {
    let mut bascule = false;
    if detachee {
        let viewport = egui::ViewportId::from_hash_of(("rmplayer-fenetre", id));
        let constructeur = egui::ViewportBuilder::default()
            .with_title(format!("RMPlayer — {nom}"))
            .with_inner_size(taille_detachee)
            .with_min_inner_size([300.0, 160.0]);
        ctx.show_viewport_immediate(viewport, constructeur, |ctx, _classe| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let r = ui
                        .button("⧉ Rattacher")
                        .on_hover_text("Remettre la fenêtre à l'intérieur du lecteur");
                    journaliser(id, r.rect, true);
                    bascule |= r.clicked();
                });
                ui.separator();
                if defilement {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| contenu(ui, true));
                } else {
                    contenu(ui, true);
                }
            });
            // Fermeture : bouton du gestionnaire de fenêtres, ou Échap dans cette fenêtre.
            // (Les touches de la fenêtre détachée ne passent pas par la boucle du lecteur.)
            if ctx.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape)) {
                *ouvert = false;
            }
        });
        Retour { rect: None, bascule }
    } else {
        let reponse = fenetre.open(ouvert).show(ctx, |ui| {
            let r = ui
                .small_button("⧉ Détacher")
                .on_hover_text("Ouvrir dans une vraie fenêtre, déplaçable hors du lecteur");
            journaliser(id, r.rect, false);
            bascule |= r.clicked();
            contenu(ui, false);
        });
        Retour { rect: reponse.map(|r| r.response.rect), bascule }
    }
}

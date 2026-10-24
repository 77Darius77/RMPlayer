//! Analyse des fichiers **ASS/SSA** : styles, surcharges en ligne, position.
//!
//! **Pourquoi un module séparé de `subtitles.rs`** : `subtitles.rs` ne garde que le
//! texte et les temps — c'est suffisant pour un SRT, pas pour un ASS. Un ASS décrit
//! une *mise en page* : une résolution de référence (`PlayResX/Y`), une table de
//! styles, et par événement des surcharges (`{\pos(…)}{\c&H…&}`). Ce module traduit
//! tout cela en valeurs absolues, prêtes à dessiner.
//!
//! **Trois pièges du format, tous vérifiés par les tests en bas de fichier :**
//!
//! 1. **Les couleurs sont en BGR, pas en RGB** : `&H0000FF&` est du **rouge**, pas du
//!    bleu. Se tromper donne une image plausible mais fausse — le genre d'erreur
//!    qu'on ne voit pas sans comparaison.
//! 2. **L'alpha est inversé** : `&H00……` est **opaque** et `&HFF……` **transparent**
//!    (c'est un taux de transparence, pas d'opacité).
//! 3. **Les tailles et positions sont dans la résolution de référence du fichier**
//!    (`PlayResY`, souvent 288 ou 720), pas en pixels écran : tout doit être mis à
//!    l'échelle, sinon un sous-titre de 20 pt devient illisible ou géant.
//!
//! **Périmètre** : `\pos`, `\an`/`\a`, `\c`/`\1c`, `\alpha`, `\fs`, `\b`, `\i`,
//! `\bord`, `\fad`, `\move`, `\t`, le karaoké `\k`/`\kf`/`\K`, les découpages
//! `\clip`/`\iclip`, la rotation `\frz`/`\fr` avec son origine `\org`, les sauts de
//! ligne `\N`/`\n`, les styles nommés et les marges. **Non géré** : `\p` (dessins
//! vectoriels), les rotations 3D (`\frx`/`\fry`), `\fscx`/`\fscy`, `\blur`.
//!
//! **Deux limites assumées de `\clip`**, mesurées plutôt que devinées :
//! - un chemin vectoriel **non rectangulaire** (triangle, « L », courbe) n'est retenu
//!   que par ses **bornes** — le champ `Clip::exact` le dit, et le journal le signale
//!   en `debug` ;
//! - `\frz` combiné à `\kf` découpe le balayage sur les bornes tournées de la bande
//!   (le découpage d'egui reste aligné sur les axes).
//!
//! Les balises non gérées sont retirées proprement : le texte reste lisible, il perd
//! seulement l'effet — jamais de balise brute affichée.

/// Couleur RGBA telle qu'affichée (alpha déjà converti en opacité).
pub type Rgba = [u8; 4];

/// Style nommé d'un fichier ASS (section `[V4+ Styles]`).
#[derive(Debug, Clone)]
pub struct AssStyle {
    pub fontname: String,
    pub fontsize: f32,
    pub primary: Rgba,
    /// Couleur secondaire : c'est elle qui sert de fond au karaoké (`\k`) avant que
    /// la syllabe soit « chantée ».
    pub secondary: Rgba,
    pub outline_colour: Rgba,
    pub back_colour: Rgba,
    pub bold: bool,
    pub italic: bool,
    /// 1 = contour + ombre, 3 = cadre opaque (les deux styles réels du format).
    pub border_style: u8,
    pub outline: f32,
    pub shadow: f32,
    /// Alignement pavé numérique : 1 = bas-gauche … 5 = milieu … 9 = haut-droite.
    pub alignment: u8,
    pub margin_l: f32,
    pub margin_r: f32,
    pub margin_v: f32,
}

impl Default for AssStyle {
    fn default() -> Self {
        Self {
            fontname: "sans-serif".to_string(),
            fontsize: 20.0,
            primary: [255, 255, 255, 255],
            secondary: [255, 0, 0, 255],
            outline_colour: [0, 0, 0, 255],
            back_colour: [0, 0, 0, 128],
            bold: false,
            italic: false,
            border_style: 1,
            outline: 2.0,
            shadow: 0.0,
            alignment: 2,
            margin_l: 10.0,
            margin_r: 10.0,
            margin_v: 10.0,
        }
    }
}

/// Fragment de texte d'un événement, avec son style effectif.
///
/// Un événement ASS peut changer de couleur ou de taille **au milieu** de la ligne
/// (`{\c&H00FF00&}vert{\c&HFF&}rouge`). On découpe donc le texte en fragments plutôt
/// que de garder un style unique par événement : sinon les sous-titres bilingues ou
/// les mots mis en valeur perdraient leur mise en forme.
#[derive(Debug, Clone)]
pub struct AssRun {
    pub text: String,
    pub fontsize: f32,
    pub primary: Rgba,
    pub secondary: Rgba,
    pub bold: bool,
    pub italic: bool,
    /// Karaoké : début (ms, relatif à l'événement) et durée de la syllabe.
    /// `dur == 0` = fragment ordinaire, pas de karaoké.
    pub karaoke_start_ms: u32,
    pub karaoke_dur_ms: u32,
    /// `\kf` / `\K` : le balayage est progressif (au lieu d'un changement net).
    pub karaoke_sweep: bool,
}

/// Événement (`Dialogue:`) avec sa mise en page résolue.
#[derive(Debug, Clone)]
pub struct AssEvent {
    pub start_ms: u64,
    pub end_ms: u64,
    /// Couche : un événement de couche supérieure passe devant (0 par défaut).
    pub layer: i32,
    pub style_name: String,
    pub runs: Vec<AssRun>,
    /// Position absolue demandée par `\pos`, en unités de `PlayRes`.
    pub pos: Option<(f32, f32)>,
    /// Alignement effectif (style, éventuellement surchargé par `\an`).
    pub alignment: u8,
    /// Marge verticale effective (surchargée par `\an`/style).
    pub margin_v: f32,
    pub margin_l: f32,
    pub margin_r: f32,
    /// Contour : couleur et épaisseur (unités de `PlayRes`), surchargeables en ligne.
    pub outline_colour: Rgba,
    pub outline: f32,
    /// Ombre portée en unités de `PlayRes`.
    pub shadow: f32,
    /// Cadre opaque (`BorderStyle=3`) : la couleur de fond est `back_colour`.
    pub opaque_box: bool,
    pub back_colour: Rgba,
    /// Fondu d'entrée/sortie en millisecondes (`\fad`), 0 = pas de fondu.
    pub fade_in_ms: u32,
    pub fade_out_ms: u32,
    /// Déplacement animé (`\move`) : interpolé entre `t1` et `t2`.
    pub mouvement: Option<Mouvement>,
    /// Transformations progressives (`\t(t1,t2,…)`), appliquées dans l'ordre.
    pub transformations: Vec<Transformation>,
    /// Découpages (`\clip`, `\iclip`) : ils se **cumulent** (intersection), comme
    /// dans libass — un événement peut en porter plusieurs.
    pub clips: Vec<Clip>,
    /// Rotation autour de l'axe Z (`\frz` / `\fr`), en degrés. Positif = sens horaire
    /// à l'écran, comme le veut le format.
    pub rotation_z: f32,
    /// Origine de rotation (`\org(x,y)`), en unités de `PlayRes`. Sans elle, la
    /// rotation se fait autour du point d'ancrage du texte (`\pos`, ou le coin que
    /// désigne l'alignement).
    pub origin: Option<(f32, f32)>,
    /// Dessin vectoriel (`\p1`) : rien à afficher en texte, événement ignoré.
    pub drawing: bool,
}

impl AssEvent {
    /// Vrai si l'événement a du texte affichable.
    pub fn a_du_texte(&self) -> bool {
        !self.drawing && self.runs.iter().any(|r| !r.text.trim().is_empty())
    }

    /// Texte brut (sans mise en forme) — utile pour l'export et les tests.
    pub fn texte(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

/// Un fichier ASS analysé.
#[derive(Debug, Clone, Default)]
pub struct AssScript {
    /// Résolution de référence du script (`PlayResX`, `PlayResY`).
    pub play_res: (u32, u32),
    pub styles: Vec<(String, AssStyle)>,
    pub events: Vec<AssEvent>,
}

impl AssScript {
    pub fn style(&self, nom: &str) -> Option<&AssStyle> {
        self.styles.iter().find(|(n, _)| n == nom).map(|(_, s)| s)
    }

    /// Événements actifs à un instant donné, dans l'ordre des couches (le plus
    /// prioritaire en dernier, donc dessiné au-dessus).
    pub fn events_at(&self, position_ms: u64) -> Vec<&AssEvent> {
        let mut actifs: Vec<&AssEvent> = self
            .events
            .iter()
            .filter(|e| position_ms >= e.start_ms && position_ms < e.end_ms)
            .filter(|e| e.a_du_texte())
            .collect();
        actifs.sort_by_key(|e| e.layer);
        actifs
    }

    /// Opacité d'un événement à un instant donné, compte tenu de son fondu.
    pub fn opacite(&self, e: &AssEvent, position_ms: u64) -> f32 {
        let mut o = 1.0f32;
        if e.fade_in_ms > 0 {
            let ecoule = position_ms.saturating_sub(e.start_ms) as f32;
            o = o.min((ecoule / e.fade_in_ms as f32).clamp(0.0, 1.0));
        }
        if e.fade_out_ms > 0 {
            let reste = e.end_ms.saturating_sub(position_ms) as f32;
            o = o.min((reste / e.fade_out_ms as f32).clamp(0.0, 1.0));
        }
        o
    }
}

/// Découpage d'un événement : `\clip(x1,y1,x2,y2)`, `\clip(m …)` (chemin) ou
/// `\iclip` (tout est masqué SAUF la zone).
///
/// Coordonnées en unités de `PlayRes`, comme tout le reste du fichier.
///
/// **Ce qui est exact et ce qui ne l'est pas.** Un rectangle (`\clip(x1,y1,x2,y2)`)
/// et un chemin qui décrit un rectangle aligné sur les axes — la forme réellement
/// employée par les fansubs pour un texte en deux colonnes — sont découpés
/// **exactement**. Un chemin quelconque (courbe, polygone concave) n'est retenu que
/// par ses **bornes** : le cas est signalé (`exact == false`) et documenté comme
/// limite, plutôt que de laisser croire à un découpage fidèle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clip {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    /// `\iclip` : le découpage est inversé.
    pub inverse: bool,
    /// Vrai si le découpage vient d'un chemin vectoriel (et non d'un rectangle).
    pub vecteur: bool,
    /// Faux si le chemin a dû être réduit à ses bornes (découpage approché).
    pub exact: bool,
}

impl Clip {
    /// Rectangle normalisé (x1 < x2, y1 < y2).
    fn normalise(mut self) -> Self {
        if self.x1 > self.x2 {
            std::mem::swap(&mut self.x1, &mut self.x2);
        }
        if self.y1 > self.y2 {
            std::mem::swap(&mut self.y1, &mut self.y2);
        }
        self
    }
}

/// Découpage traduit en pixels de la zone affichée.
///
/// `garde` est la zone où l'on dessine ; `exclusions` sont les rectangles à retirer
/// (`\iclip`). Les deux peuvent coexister (un `\clip` puis un `\iclip`).
#[derive(Debug, Clone, PartialEq)]
pub struct ClipPlace {
    pub garde: Option<(f32, f32, f32, f32)>,
    pub exclusions: Vec<(f32, f32, f32, f32)>,
}

impl ClipPlace {
    pub fn est_vide(&self) -> bool {
        self.garde.is_none() && self.exclusions.is_empty()
    }
}

/// `\move(x1,y1,x2,y2[,t1,t2])` : déplacement linéaire pendant l'événement.
#[derive(Debug, Clone, Copy)]
pub struct Mouvement {
    pub de: (f32, f32),
    pub vers: (f32, f32),
    /// Bornes de l'animation, en ms **relatives au début de l'événement**. Quand elles
    /// sont absentes, le déplacement couvre tout l'événement.
    pub t1: u32,
    pub t2: u32,
}

/// `\t(t1,t2,modifications)` : fait varier des propriétés entre `t1` et `t2`.
///
/// Seules les propriétés **interpolables** sont retenues : taille, couleurs, contour,
/// alpha. Une propriété booléenne (gras, italique) bascule en fin d'intervalle — c'est
/// ce que fait aussi libass pour les propriétés non continues.
#[derive(Debug, Clone)]
pub struct Transformation {
    pub t1: u32,
    pub t2: u32,
    pub fontsize: Option<f32>,
    pub primary: Option<Rgba>,
    pub secondary: Option<Rgba>,
    pub outline_colour: Option<Rgba>,
    pub bord: Option<f32>,
    pub gras: Option<bool>,
}

impl Transformation {
    /// Avancement de la transformation (0 → 1) à l'instant donné.
    pub fn avancement(&self, temps_ms: u32) -> f32 {
        if temps_ms <= self.t1 {
            return 0.0;
        }
        if self.t2 <= self.t1 {
            // Durée nulle : bascule immédiate.
            return 1.0;
        }
        if temps_ms >= self.t2 {
            return 1.0;
        }
        (temps_ms - self.t1) as f32 / (self.t2 - self.t1) as f32
    }
}

/// Interpole deux couleurs (composante par composante, en linéaire sur les octets).
fn melanger_couleurs(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| -> u8 {
        (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8
    };
    [m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2]), m(a[3], b[3])]
}

/// Interpole deux nombres.
fn melanger_nombres(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// Un fragment de texte prêt à dessiner, position en pixels de l'image affichée.
#[derive(Debug, Clone)]
pub struct RunPlace {
    pub text: String,
    /// Coin supérieur gauche du texte.
    pub x: f32,
    pub y: f32,
    pub font_px: f32,
    pub colour: Rgba,
    pub outline_px: f32,
    pub outline_colour: Rgba,
    pub bold: bool,
    /// Cadre opaque demandé (`BorderStyle=3`), couleur déjà calculée.
    pub box_colour: Option<Rgba>,
    /// Largeur du texte, pour le cadre ou le diagnostic.
    pub width: f32,
    /// Ombre portée (décalage en pixels) et sa couleur.
    pub shadow_px: f32,
    pub shadow_colour: Rgba,
    /// Hauteur de ligne, pour dessiner le cadre opaque.
    pub height: f32,
    /// Karaoké : couleur « non encore chantée » et avancement du balayage (0 → 1).
    /// `None` = pas de karaoké sur ce fragment.
    pub karaoke: Option<(Rgba, f32)>,
    /// Rotation du fragment (`\frz`), en radians. `0.0` = pas de rotation : le chemin
    /// de dessin reste alors celui d'origine (aucun risque de régression).
    pub angle: f32,
    /// Découpage à appliquer, en pixels relatifs à la zone du sous-titre.
    pub clip: Option<ClipPlace>,
}

/// Place les fragments d'un événement dans une zone d'affichage.
///
/// `mesurer` donne la largeur d'un texte à une taille donnée : on l'injecte plutôt
/// que de dépendre d'egui, ce qui rend le calcul **testable sans écran** (les tests
/// ci-dessous utilisent un mesureur synthétique).
///
/// Les tailles et positions du fichier sont ramenées à la zone par le rapport
/// `zone / PlayRes`, dans chaque axe : un ASS conçu pour 384×288 s'affiche
/// correctement sur une image 16:9 comme sur une 4:3.
pub fn placer(
    e: &AssEvent,
    largeur: f32,
    hauteur: f32,
    play_res: (u32, u32),
    opacite: f32,
    temps_ms: u32,
    mesurer: &dyn Fn(&str, f32, bool) -> f32,
) -> Vec<RunPlace> {
    if largeur < 2.0 || hauteur < 2.0 {
        return Vec::new();
    }
    let sx = largeur / play_res.0.max(1) as f32;
    let sy = hauteur / play_res.1.max(1) as f32;
    // Le texte est mis à l'échelle par la hauteur : c'est l'axe qui porte la taille
    // des caractères dans le format (PlayResY est la référence des polices).
    let echelle_police = sy;

    // --- Découpage en lignes ---
    struct Morceau<'a> {
        run: &'a AssRun,
        texte: String,
    }
    let mut lignes: Vec<Vec<Morceau>> = vec![Vec::new()];
    for run in &e.runs {
        let mut morceaux = run.text.split('\n').peekable();
        while let Some(m) = morceaux.next() {
            if !m.is_empty() {
                lignes
                    .last_mut()
                    .expect("au moins une ligne")
                    .push(Morceau {
                        run,
                        texte: m.to_string(),
                    });
            }
            if morceaux.peek().is_some() {
                lignes.push(Vec::new());
            }
        }
    }
    lignes.retain(|l| !l.is_empty());
    if lignes.is_empty() {
        return Vec::new();
    }

    // --- Mesures ---
    let mut largeurs_lignes: Vec<f32> = Vec::new();
    let mut hauteurs_lignes: Vec<f32> = Vec::new();
    for ligne in &lignes {
        let mut w = 0.0f32;
        let mut h = 0.0f32;
        for m in ligne {
            let police = m.run.fontsize * echelle_police;
            w += mesurer(&m.texte, police, m.run.bold);
            h = h.max(police * 1.2);
        }
        largeurs_lignes.push(w);
        hauteurs_lignes.push(h);
    }
    let largeur_bloc = largeurs_lignes.iter().cloned().fold(0.0f32, f32::max);
    let hauteur_bloc: f32 = hauteurs_lignes.iter().sum();

    // --- Origine du bloc ---
    let colonne = match e.alignment {
        1 | 4 | 7 => 0,
        3 | 6 | 9 => 2,
        _ => 1,
    };
    let rangee = match e.alignment {
        1..=3 => 2, // bas
        4..=6 => 1, // milieu
        _ => 0,     // haut
    };
    // --- Animations de position -------------------------------------------------
    // `\move` interpolé entre t1 et t2 ; `t2 = 0` signifie « jusqu'à la fin de
    // l'événement ». Le résultat remplace `\pos` (les deux sont incompatibles).
    let position_animee: Option<(f32, f32)> = e.mouvement.as_ref().map(|m| {
        let t2 = if m.t2 == 0 {
            (e.end_ms - e.start_ms) as u32
        } else {
            m.t2
        };
        let avancement = if temps_ms <= m.t1 {
            0.0
        } else if t2 <= m.t1 || temps_ms >= t2 {
            1.0
        } else {
            (temps_ms - m.t1) as f32 / (t2 - m.t1) as f32
        };
        (
            melanger_nombres(m.de.0, m.vers.0, avancement),
            melanger_nombres(m.de.1, m.vers.1, avancement),
        )
    });
    let position_effective = position_animee.or(e.pos);

    let (x0, y0) = match position_effective {
        Some((px, py)) => {
            let ax = px * sx;
            let ay = py * sy;
            let x = match colonne {
                0 => ax,
                2 => ax - largeur_bloc,
                _ => ax - largeur_bloc / 2.0,
            };
            let y = match rangee {
                2 => ay - hauteur_bloc,
                0 => ay,
                _ => ay - hauteur_bloc / 2.0,
            };
            (x, y)
        }
        None => {
            let marge_g = e.margin_l * sx;
            let marge_d = e.margin_r * sx;
            let marge_v = e.margin_v * sy;
            let x = match colonne {
                0 => marge_g,
                2 => largeur - marge_d - largeur_bloc,
                _ => (largeur - largeur_bloc) / 2.0,
            };
            let y = match rangee {
                2 => hauteur - marge_v - hauteur_bloc,
                0 => marge_v,
                _ => (hauteur - hauteur_bloc) / 2.0,
            };
            (x, y)
        }
    };

    // --- Découpage `\clip` / `\iclip` -------------------------------------------
    // Calculé une fois pour l'événement : il ne dépend ni du fragment ni du temps.
    let clip = {
        let place = clips_en_pixels(&e.clips, sx, sy);
        if e.clips.iter().any(|c| !c.exact) {
            tracing::debug!(
                "sous-titre : \\clip vectoriel non rectangulaire réduit à ses bornes \
                 (découpage approché)"
            );
        }
        place
    };

    // --- Rotation `\frz` / `\fr` avec origine `\org` -----------------------------
    // Le pivot est `\org` s'il est donné, sinon le point d'ancrage du texte — c'est
    // ce que fait libass, et c'est ce qui rend une simple rotation prévisible.
    let angle = e.rotation_z.to_radians();
    let pivot = if angle.abs() > 1e-4 {
        e.origin
            .map(|(ox, oy)| (ox * sx, oy * sy))
            .unwrap_or_else(|| {
                let ax = match colonne {
                    0 => x0,
                    2 => x0 + largeur_bloc,
                    _ => x0 + largeur_bloc / 2.0,
                };
                let ay = match rangee {
                    2 => y0 + hauteur_bloc,
                    0 => y0,
                    _ => y0 + hauteur_bloc / 2.0,
                };
                (ax, ay)
            })
    } else {
        (0.0, 0.0)
    };
    let (sin_a, cos_a) = angle.sin_cos();
    // Point tourné autour du pivot (l'angle est appliqué au texte par le dessinateur).
    let tourner = |x: f32, y: f32| -> (f32, f32) {
        if angle.abs() <= 1e-4 {
            return (x, y);
        }
        let dx = x - pivot.0;
        let dy = y - pivot.1;
        (
            pivot.0 + dx * cos_a - dy * sin_a,
            pivot.1 + dx * sin_a + dy * cos_a,
        )
    };

    // --- Transformations `\t` : appliquées au style de base, dans l'ordre écrit ----
    // On calcule un multiplicateur de taille, une couleur principale et une couleur de
    // contour « effectives » pour l'événement. Les transformations par fragment (à
    // l'intérieur d'un run) sont rares et resteraient ambiguës : on les applique à
    // l'ensemble de l'événement, ce qui correspond au cas réel (`\t` sur une ligne).
    let mut facteur_taille = 1.0f32;
    let mut couleur_evt: Option<Rgba> = None;
    let mut contour_evt: Option<Rgba> = None;
    let mut secondaire_evt: Option<Rgba> = None;
    let mut gras_evt = false;
    for tr in &e.transformations {
        let a = tr.avancement(temps_ms);
        if let Some(cible) = tr.fontsize {
            let base = e.runs.first().map(|r| r.fontsize).unwrap_or(cible);
            if base > 0.0 {
                facteur_taille = melanger_nombres(base, cible, a) / base;
            }
        }
        if let Some(cible) = tr.primary {
            let base = e.runs.first().map(|r| r.primary).unwrap_or(cible);
            couleur_evt = Some(melanger_couleurs(base, cible, a));
        }
        if let Some(cible) = tr.outline_colour {
            contour_evt = Some(melanger_couleurs(e.outline_colour, cible, a));
        }
        if let Some(cible) = tr.secondary {
            let base = e.runs.first().map(|r| r.secondary).unwrap_or(cible);
            secondaire_evt = Some(melanger_couleurs(base, cible, a));
        }
        if let Some(true) = tr.gras {
            if a >= 1.0 {
                gras_evt = true;
            }
        }
    }
    if (facteur_taille - 1.0).abs() > 1e-4 {
        // La taille animée change aussi la hauteur des lignes : on la recalcule.
        for (i, ligne) in lignes.iter().enumerate() {
            let mut h = 0.0f32;
            for m in ligne {
                h = h.max(m.run.fontsize * echelle_police * facteur_taille * 1.2);
            }
            hauteurs_lignes[i] = h;
        }
        for (i, ligne) in lignes.iter().enumerate() {
            let mut w = 0.0f32;
            for m in ligne {
                w += mesurer(
                    &m.texte,
                    m.run.fontsize * echelle_police * facteur_taille,
                    m.run.bold || gras_evt,
                );
            }
            largeurs_lignes[i] = w;
        }
    }

    // --- Fragments placés ---
    let mut sortie: Vec<RunPlace> = Vec::new();
    let mut y = y0;
    for (i, ligne) in lignes.iter().enumerate() {
        let lg = largeurs_lignes[i];
        let mut x = match colonne {
            0 => x0,
            2 => x0 + largeur_bloc - lg,
            _ => x0 + (largeur_bloc - lg) / 2.0,
        };
        for m in ligne {
            let police = m.run.fontsize * echelle_police * facteur_taille;
            let gras = m.run.bold || gras_evt;
            let w = mesurer(&m.texte, police, gras);
            let mut couleur = couleur_evt.unwrap_or(m.run.primary);
            couleur[3] = ((couleur[3] as f32) * opacite.clamp(0.0, 1.0)) as u8;
            let mut contour = contour_evt.unwrap_or(e.outline_colour);
            contour[3] = ((contour[3] as f32) * opacite.clamp(0.0, 1.0)) as u8;
            // Rotation : la position du fragment tourne autour du pivot. Le dessinateur
            // fait tourner les glyphes eux-mêmes du même angle (le reste — ombre,
            // contour — suit le même décalage tourné).
            let (xr, yr) = tourner(x, y);
            sortie.push(RunPlace {
                text: m.texte.clone(),
                x: xr,
                y: yr,
                font_px: police,
                colour: couleur,
                outline_px: e.outline.max(0.0) * sy,
                outline_colour: contour,
                bold: gras,
                box_colour: if e.opaque_box {
                    let mut c = e.back_colour;
                    c[3] = ((c[3] as f32) * opacite.clamp(0.0, 1.0)) as u8;
                    Some(c)
                } else {
                    None
                },
                width: w,
                shadow_px: e.shadow.max(0.0) * sy,
                shadow_colour: {
                    let mut c = e.back_colour;
                    c[3] = ((c[3] as f32) * opacite.clamp(0.0, 1.0)) as u8;
                    c
                },
                height: police * 1.2,
                // Karaoké : avancement du balayage sur cette syllabe.
                karaoke: if m.run.karaoke_dur_ms > 0 {
                    let debut = m.run.karaoke_start_ms;
                    let fin = debut + m.run.karaoke_dur_ms;
                    let avancement = if temps_ms <= debut {
                        0.0
                    } else if !m.run.karaoke_sweep {
                        // `\k` : bascule nette au début de la syllabe.
                        1.0
                    } else if temps_ms >= fin {
                        1.0
                    } else {
                        (temps_ms - debut) as f32 / m.run.karaoke_dur_ms as f32
                    };
                    Some((secondaire_evt.unwrap_or(m.run.secondary), avancement))
                } else {
                    None
                },
                angle,
                clip: clip.clone(),
            });
            x += w;
        }
        y += hauteurs_lignes[i];
    }
    sortie
}

/// Convertit une couleur ASS (`&HAABBGGRR`, `&HBBGGRR&`, `&HBBGGRR`, `HBBGGRR`) en
/// RGBA affichable.
///
/// **BGR** : les octets sont dans l'ordre bleu, vert, rouge. **Alpha inversé** :
/// l'octet d'alpha est une *transparence* (FF = invisible).
pub fn couleur_ass(texte: &str) -> Option<Rgba> {
    let mut t = texte.trim();
    if let Some(reste) = t.strip_prefix("&H").or_else(|| t.strip_prefix("&h")) {
        t = reste;
    } else if let Some(reste) = t.strip_prefix("H").or_else(|| t.strip_prefix("h")) {
        t = reste;
    }
    // Le format admis met souvent un « & » final.
    t = t.trim_end_matches('&');
    if t.is_empty() || t.len() > 8 || !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(t, 16).ok()?;
    let (alpha, b, g, r) = if t.len() > 6 {
        // 8 chiffres : AABBGGRR
        (
            ((v >> 24) & 0xFF) as u8,
            ((v >> 16) & 0xFF) as u8,
            ((v >> 8) & 0xFF) as u8,
            (v & 0xFF) as u8,
        )
    } else {
        (
            0u8, // pas d'alpha fourni = opaque
            ((v >> 16) & 0xFF) as u8,
            ((v >> 8) & 0xFF) as u8,
            (v & 0xFF) as u8,
        )
    };
    // Transparence → opacité.
    Some([r, g, b, 255u8.saturating_sub(alpha)])
}

/// Analyse une ligne de style (`Format:` puis `Style:`).
fn analyser_styles(lignes: &[&str], script: &mut AssScript) {
    // La ligne `Format:` du fichier donne l'ordre des champs ; on s'y fie plutôt qu'à
    // un ordre supposé, car les fichiers réels varient (SSA vs ASS, outils divers).
    let mut ordre: Vec<String> = Vec::new();
    for ligne in lignes {
        let l = ligne.trim();
        if let Some(reste) = l.strip_prefix("Format:") {
            ordre = reste.split(',').map(|c| c.trim().to_lowercase()).collect();
        } else if let Some(reste) = l.strip_prefix("Style:") {
            let champs: Vec<&str> = reste.splitn(ordre.len().max(1), ',').collect();
            let mut style = AssStyle::default();
            let mut nom = String::from("Default");
            for (i, cle) in ordre.iter().enumerate() {
                let Some(valeur) = champs.get(i).map(|v| v.trim()) else {
                    continue;
                };
                match cle.as_str() {
                    "name" => nom = valeur.to_string(),
                    "fontname" => style.fontname = valeur.to_string(),
                    "fontsize" => style.fontsize = valeur.parse().unwrap_or(style.fontsize),
                    "primarycolour" => {
                        if let Some(c) = couleur_ass(valeur) {
                            style.primary = c;
                        }
                    }
                    "secondarycolour" => {
                        if let Some(c) = couleur_ass(valeur) {
                            style.secondary = c;
                        }
                    }
                    "outlinecolour" => {
                        if let Some(c) = couleur_ass(valeur) {
                            style.outline_colour = c;
                        }
                    }
                    "backcolour" => {
                        if let Some(c) = couleur_ass(valeur) {
                            style.back_colour = c;
                        }
                    }
                    "bold" => style.bold = valeur.trim() == "1" || valeur.trim() == "-1",
                    "italic" => style.italic = valeur.trim() == "1" || valeur.trim() == "-1",
                    "borderstyle" => style.border_style = valeur.parse().unwrap_or(1),
                    "outline" => style.outline = valeur.parse().unwrap_or(style.outline),
                    "shadow" => style.shadow = valeur.parse().unwrap_or(0.0),
                    "alignment" => style.alignment = valeur.parse().unwrap_or(2),
                    "marginl" => style.margin_l = valeur.parse().unwrap_or(style.margin_l),
                    "marginr" => style.margin_r = valeur.parse().unwrap_or(style.margin_r),
                    "marginv" => style.margin_v = valeur.parse().unwrap_or(style.margin_v),
                    _ => {}
                }
            }
            script.styles.push((nom, style));
        }
    }
}

/// Analyse une ligne `Dialogue:`.
fn analyser_dialogue(
    reste: &str,
    ordre: &[String],
    script: &AssScript,
    evenements: &mut Vec<AssEvent>,
) {
    // Le texte est toujours le dernier champ et peut contenir des virgules : on
    // découpe donc au plus `ordre.len()` fois.
    let champs: Vec<&str> = reste.splitn(ordre.len(), ',').collect();
    let mut start_ms = 0;
    let mut end_ms = 0;
    let mut layer = 0;
    let mut style_name = String::from("Default");
    let mut texte_brut = String::new();
    for (i, cle) in ordre.iter().enumerate() {
        let Some(valeur) = champs.get(i).map(|v| v.trim()) else {
            continue;
        };
        match cle.as_str() {
            "layer" | "marked" => layer = valeur.parse().unwrap_or(0),
            "start" => start_ms = timestamp_ass(valeur),
            "end" => end_ms = timestamp_ass(valeur),
            "style" => style_name = valeur.to_string(),
            "text" => texte_brut = champs[i..].join(","),
            _ => {}
        }
    }
    if texte_brut.is_empty() || end_ms <= start_ms {
        return;
    }

    let style = script.style(&style_name).cloned().unwrap_or_default();
    let mut run = AssRun {
        text: String::new(),
        fontsize: style.fontsize,
        primary: style.primary,
        secondary: style.secondary,
        bold: style.bold,
        italic: style.italic,
        karaoke_start_ms: 0,
        karaoke_dur_ms: 0,
        karaoke_sweep: false,
    };
    // Karaoké : le temps « chanté » s'accumule d'une syllabe à l'autre dans l'événement.
    let mut karaoke_cumul: u32 = 0;
    let mut karaoke_attente: u32 = 0;
    let mut karaoke_balayage = false;
    let mut mouvement: Option<Mouvement> = None;
    let mut transformations: Vec<Transformation> = Vec::new();
    let mut couleur_contour = style.outline_colour;
    let mut bord = style.outline;
    let mut alignement = style.alignment;
    let mut pos: Option<(f32, f32)> = None;
    let mut marge_v = style.margin_v;
    let mut fad = (0u32, 0u32);
    let mut drawing = false;
    let mut clips: Vec<Clip> = Vec::new();
    let mut rotation_z = 0.0f32;
    let mut origin: Option<(f32, f32)> = None;
    let mut runs: Vec<AssRun> = Vec::new();

    let mut reste = texte_brut.as_str();
    while let Some(ouvrante) = reste.find('{') {
        // Texte avant la balise.
        let avant = &reste[..ouvrante];
        if !avant.is_empty() {
            run.text.push_str(&avant.replace("\\N", "\n").replace("\\n", "\n"));
        }
        let Some(fin) = reste[ouvrante..].find('}') else {
            // Balise non fermée : on garde le reste tel quel.
            run.text.push_str(&reste[ouvrante..]);
            reste = "";
            break;
        };
        let bloc = &reste[ouvrante + 1..ouvrante + fin];
        // Le style du fragment peut changer : on clôt le fragment courant.
        if !run.text.is_empty() {
            runs.push(run.clone());
            run.text.clear();
            // Le karaoké appartient à la syllabe qui vient d'être poussée : sans cette
            // remise à zéro, la syllabe SUIVANTE héritait de la durée de la précédente
            // (mesuré : « Ka » et « ra » tous deux à 0+2000).
            run.karaoke_start_ms = 0;
            run.karaoke_dur_ms = 0;
            run.karaoke_sweep = false;
        }

        appliquer_balises(
            bloc,
            &mut run,
            &mut couleur_contour,
            &mut bord,
            &mut alignement,
            &mut pos,
            &mut marge_v,
            &mut fad,
            &mut drawing,
            &mut mouvement,
            &mut transformations,
            &mut karaoke_attente,
            &mut karaoke_balayage,
            &mut clips,
            &mut rotation_z,
            &mut origin,
        );
        // La balise vient d'être lue : si elle a armé un karaoké, c'est le run
        // (encore vide) qui porte la syllabe suivante. Il faut donc l'assigner
        // MAINTENANT — après la balise, pas avant.
        if karaoke_attente > 0 && run.text.is_empty() {
            run.karaoke_start_ms = karaoke_cumul;
            run.karaoke_dur_ms = karaoke_attente;
            run.karaoke_sweep = karaoke_balayage;
            karaoke_cumul += karaoke_attente;
            karaoke_attente = 0;
            karaoke_balayage = false;
        }
        reste = &reste[ouvrante + fin + 1..];
    }
    if !reste.is_empty() {
        run.text.push_str(&reste.replace("\\N", "\n").replace("\\n", "\n"));
    }
    if !run.text.is_empty() {
        runs.push(run);
    }
    if runs.is_empty() {
        return;
    }

    evenements.push(AssEvent {
        start_ms,
        end_ms,
        layer,
        style_name,
        runs,
        pos,
        alignment: alignement,
        margin_v: marge_v,
        margin_l: style.margin_l,
        margin_r: style.margin_r,
        outline_colour: couleur_contour,
        outline: bord,
        shadow: style.shadow,
        opaque_box: style.border_style == 3,
        back_colour: style.back_colour,
        fade_in_ms: fad.0,
        fade_out_ms: fad.1,
        mouvement,
        transformations,
        clips,
        rotation_z,
        origin,
        drawing,
    });
}

/// Applique les balises d'un bloc `{…}` (sans les accolades).
#[allow(clippy::too_many_arguments)]
fn appliquer_balises(
    bloc: &str,
    run: &mut AssRun,
    couleur_contour: &mut Rgba,
    bord: &mut f32,
    alignement: &mut u8,
    pos: &mut Option<(f32, f32)>,
    _marge_v: &mut f32,
    fad: &mut (u32, u32),
    drawing: &mut bool,
    mouvement: &mut Option<Mouvement>,
    transformations: &mut Vec<Transformation>,
    karaoke_attente: &mut u32,
    karaoke_balayage: &mut bool,
    clips: &mut Vec<Clip>,
    rotation_z: &mut f32,
    origin: &mut Option<(f32, f32)>,
) {
    // Découpage : chaque balise commence par « \ ». Les paramètres entre parenthèses
    // peuvent contenir des virgules, d'où le découpage manuel.
    let mut morceaux: Vec<String> = Vec::new();
    let mut courant = String::new();
    let mut profondeur = 0;
    for c in bloc.chars() {
        match c {
            '(' | '&' => {
                profondeur += 1;
                courant.push(c);
            }
            ')' => {
                profondeur = (profondeur - 1).max(0);
                courant.push(c);
            }
            '\\' if profondeur == 0 => {
                if !courant.is_empty() {
                    morceaux.push(std::mem::take(&mut courant));
                }
                courant.push(c);
            }
            _ => courant.push(c),
        }
    }
    if !courant.is_empty() {
        morceaux.push(courant);
    }

    for m in &morceaux {
        let m = m.trim_start_matches('\\');
        let (nom, arg) = separer_balise(m);
        match nom {
            "pos" => {
                if let Some((x, y)) = arg
                    .trim_start_matches('(')
                    .trim_end_matches(')')
                    .split_once(',')
                {
                    if let (Ok(x), Ok(y)) = (x.trim().parse::<f32>(), y.trim().parse::<f32>()) {
                        *pos = Some((x, y));
                    }
                }
            }
            // `\an` : alignement pavé numérique explicite.
            "an" => {
                if let Ok(v) = arg.trim().parse::<u8>() {
                    if (1..=9).contains(&v) {
                        *alignement = v;
                    }
                }
            }
            // `\a` : ancien codage SSA (1 à 11), converti en pavé numérique.
            "a" => {
                if let Ok(v) = arg.trim().parse::<u8>() {
                    *alignement = match v {
                        1 => 1,
                        2 => 2,
                        3 => 3,
                        5 => 7,
                        6 => 8,
                        7 => 9,
                        9 => 4,
                        10 => 5,
                        11 => 6,
                        _ => *alignement,
                    };
                }
            }
            "c" | "1c" => {
                if let Some(c) = couleur_ass(arg) {
                    run.primary = c;
                }
            }
            "2c" => {
                // Couleur secondaire : celle que prend une syllabe non encore chantée.
                if let Some(c) = couleur_ass(arg) {
                    run.secondary = c;
                }
            }
            "3c" => {
                if let Some(c) = couleur_ass(arg) {
                    *couleur_contour = c;
                }
            }
            "alpha" => {
                // `\alpha&H80&` : transparence appliquée à toutes les couleurs.
                if let Some(c) = couleur_ass(&format!("H{}", arg.trim_start_matches('&').trim_end_matches('&'))) {
                    let transparence = 255i32 - c[3] as i32;
                    run.primary[3] = (255 - transparence).clamp(0, 255) as u8;
                }
            }
            "fs" => {
                if let Ok(v) = arg.trim().parse::<f32>() {
                    run.fontsize = v;
                }
            }
            "b" => run.bold = arg.trim() != "0",
            "i" => run.italic = arg.trim() != "0",
            "bord" => {
                if let Ok(v) = arg.trim().parse::<f32>() {
                    *bord = v;
                }
            }
            "fad" => {
                let nombres: Vec<u32> = arg
                    .trim_start_matches('(')
                    .trim_end_matches(')')
                    .split(',')
                    .filter_map(|v| v.trim().parse().ok())
                    .collect();
                if nombres.len() == 2 {
                    *fad = (nombres[0], nombres[1]);
                }
            }
            "p" => {
                // `\p1` commence un dessin vectoriel : il n'y a pas de texte à placer.
                // On ne revient PAS en arrière sur `\p0` : un événement qui contient un
                // dessin ne doit pas être affiché comme du texte (ses coordonnées
                // « m 0 0 l 100 0 » s'afficheraient sinon en clair).
                if arg.trim() != "0" {
                    *drawing = true;
                }
            }
            // `\move(x1,y1,x2,y2[,t1,t2])` : déplacement linéaire.
            "move" => {
                if let Some(m) = analyser_mouvement(arg) {
                    *mouvement = Some(m);
                    // Un `\move` remplace `\pos` : les deux sont incompatibles.
                    *pos = None;
                }
            }
            // `\clip(x1,y1,x2,y2)` ou `\clip(m …)`, `\iclip` : découpage.
            "clip" | "iclip" => {
                if let Some(c) = analyser_clip(arg, nom == "iclip") {
                    clips.push(c);
                }
            }
            // Rotation autour de l'axe Z (`\frz`, et son synonyme `\fr`).
            "frz" | "fr" => {
                if let Ok(v) = arg.trim().parse::<f32>() {
                    *rotation_z = v;
                }
            }
            // `\org(x,y)` : origine de la rotation.
            "org" => {
                if let Some((x, y)) = arg
                    .trim()
                    .trim_start_matches('(')
                    .trim_end_matches(')')
                    .split_once(',')
                {
                    if let (Ok(x), Ok(y)) = (x.trim().parse::<f32>(), y.trim().parse::<f32>()) {
                        *origin = Some((x, y));
                    }
                }
            }
            // `\t(t1,t2,modifications)` : transformation progressive.
            "t" => {
                if let Some(tr) = analyser_transformation(arg) {
                    transformations.push(tr);
                }
            }
            // Karaoké : `\k` (net), `\kf` et `\K` (balayage), `\ko` (contour seul,
            // traité comme `\k` : l'effet de contour n'a pas de sens sans rendu dédié).
            "k" | "kf" | "ko" | "K" => {
                if let Ok(cs) = arg.trim().parse::<u32>() {
                    // Le format compte en centièmes de seconde.
                    *karaoke_attente = cs.saturating_mul(10);
                    // `\kf` et `\K` balaient ; `\k` (et `\ko`, contour seul) basculent.
                    *karaoke_balayage = nom == "kf" || nom == "K";
                }
            }
            // Balises connues sans effet ici : animations, karaoké, transformations,
            // polices nommées. On les ignore volontairement (le texte reste correct).
            _ => {}
        }
    }
}

/// Traduit les découpages en pixels de la zone affichée.
///
/// Les `\clip` se cumulent par **intersection** et les `\iclip` par **exclusion**,
/// comme dans libass. Un `garde` dégénéré (largeur ou hauteur nulle) signifie « rien
/// ne s'affiche » : c'est le résultat correct d'un découpage impossible.
fn clips_en_pixels(clips: &[Clip], sx: f32, sy: f32) -> Option<ClipPlace> {
    if clips.is_empty() {
        return None;
    }
    let mut place = ClipPlace {
        garde: None,
        exclusions: Vec::new(),
    };
    for c in clips {
        let c = c.normalise();
        let r = (c.x1 * sx, c.y1 * sy, c.x2 * sx, c.y2 * sy);
        if c.inverse {
            place.exclusions.push(r);
        } else {
            place.garde = Some(match place.garde {
                None => r,
                Some(g) => (g.0.max(r.0), g.1.max(r.1), g.2.min(r.2), g.3.min(r.3)),
            });
        }
    }
    (!place.est_vide()).then_some(place)
}

/// Analyse l'argument de `\clip(...)` / `\iclip(...)`.
///
/// Deux formes sont admises par le format :
/// - `(x1,y1,x2,y2)` — un rectangle ;
/// - `([échelle,] chemin)` où le chemin est une suite de commandes `m`, `n`, `l`,
///   `b`, `s`, `p`, `c` suivies de coordonnées (en ASS, **sans virgules** entre les
///   nombres, sauf parfois une échelle en tête).
///
/// Un chemin qui décrit un rectangle aligné sur les axes (le cas réel : deux colonnes
/// de texte) est reconnu **exactement** ; tout autre chemin est réduit à ses bornes,
/// ce qui est signalé par `exact == false`.
fn analyser_clip(arg: &str, inverse: bool) -> Option<Clip> {
    let interieur = arg.trim().trim_start_matches('(').trim_end_matches(')').trim();
    if interieur.is_empty() {
        return None;
    }
    // Forme rectangle : quatre nombres séparés par des virgules, sans commande.
    let sans_espace: String = interieur.chars().filter(|c| !c.is_whitespace()).collect();
    if !sans_espace.chars().any(|c| c.is_ascii_alphabetic()) {
        let n: Vec<f32> = sans_espace
            .split(',')
            .filter_map(|v| v.trim().parse::<f32>().ok())
            .collect();
        if n.len() == 4 {
            let c = Clip {
                x1: n[0],
                y1: n[1],
                x2: n[2],
                y2: n[3],
                inverse,
                vecteur: false,
                exact: true,
            };
            return Some(c.normalise());
        }
        return None;
    }

    // Forme chemin. Découpage en jetons : une lettre = une commande, un nombre = une
    // coordonnée. Les virgules et espaces ne sont que des séparateurs.
    enum Jeton {
        Commande(char),
        Nombre(f32),
    }
    let mut jetons: Vec<Jeton> = Vec::new();
    let mut courant = String::new();
    let pousser = |courant: &mut String, jetons: &mut Vec<Jeton>| {
        if let Ok(v) = courant.trim().parse::<f32>() {
            jetons.push(Jeton::Nombre(v));
        }
        courant.clear();
    };
    for c in interieur.chars() {
        match c {
            '0'..='9' | '.' | '-' | '+' => courant.push(c),
            c if c.is_ascii_alphabetic() => {
                pousser(&mut courant, &mut jetons);
                jetons.push(Jeton::Commande(c.to_ascii_lowercase()));
            }
            ',' | ' ' | '\t' | '\n' => pousser(&mut courant, &mut jetons),
            _ => {}
        }
    }
    pousser(&mut courant, &mut jetons);

    let mut points: Vec<(f32, f32)> = Vec::new();
    let mut commandes = 0usize;
    let mut seulement_droites = true;
    let mut nombre_courant: Vec<f32> = Vec::new();
    let mut en_cours = 'm';
    let avaler = |en_cours: char,
                  nombres: &mut Vec<f32>,
                  points: &mut Vec<(f32, f32)>,
                  commandes: &mut usize,
                  seulement_droites: &mut bool| {
        if matches!(en_cours, 'b' | 's' | 'p' | 'c') {
            *seulement_droites = false;
        }
        let mut i = 0;
        while i + 1 < nombres.len() {
            points.push((nombres[i], nombres[i + 1]));
            i += 2;
        }
        *commandes += 1;
        nombres.clear();
    };
    for j in jetons {
        match j {
            Jeton::Commande(c) => {
                // La commande précédente a accumulé ses nombres : on les range.
                if matches!(en_cours, 'm' | 'n' | 'l' | 'b' | 's' | 'p' | 'c') && !nombre_courant.is_empty() {
                    avaler(
                        en_cours,
                        &mut nombre_courant,
                        &mut points,
                        &mut commandes,
                        &mut seulement_droites,
                    );
                }
                en_cours = c;
                if !matches!(c, 'm' | 'n' | 'l' | 'b' | 's' | 'p' | 'c') {
                    // Jeton inattendu (échelle en tête, par exemple) : on ignore.
                    en_cours = 'm';
                    seulement_droites = false;
                }
            }
            Jeton::Nombre(v) => nombre_courant.push(v),
        }
    }
    if !nombre_courant.is_empty() {
        avaler(
            en_cours,
            &mut nombre_courant,
            &mut points,
            &mut commandes,
            &mut seulement_droites,
        );
    }
    if points.is_empty() {
        return None;
    }
    let x1 = points.iter().fold(f32::INFINITY, |a, p| a.min(p.0));
    let y1 = points.iter().fold(f32::INFINITY, |a, p| a.min(p.1));
    let x2 = points.iter().fold(f32::NEG_INFINITY, |a, p| a.max(p.0));
    let y2 = points.iter().fold(f32::NEG_INFINITY, |a, p| a.max(p.1));

    // Le chemin décrit-il exactement le rectangle de ses bornes ? C'est le cas si
    // tous les segments sont horizontaux ou verticaux et si aucun point n'est à
    // l'intérieur des bornes (sinon des pixels seraient masqués en trop).
    let mut exact = seulement_droites && points.len() >= 4;
    if exact {
        for w in points.windows(2) {
            let (a, b) = (w[0], w[1]);
            if (a.0 - b.0).abs() > 0.01 && (a.1 - b.1).abs() > 0.01 {
                exact = false; // segment en diagonale
            }
        }
    }
    if exact {
        for p in &points {
            let sur_bord = (p.0 - x1).abs() < 0.01
                || (p.0 - x2).abs() < 0.01
                || (p.1 - y1).abs() < 0.01
                || (p.1 - y2).abs() < 0.01;
            if !sur_bord {
                exact = false;
            }
        }
    }
    let _ = commandes;
    Some(
        Clip {
            x1,
            y1,
            x2,
            y2,
            inverse,
            vecteur: true,
            exact,
        }
        .normalise(),
    )
}

/// Analyse l'argument de `\move(x1,y1,x2,y2[,t1,t2])`.
fn analyser_mouvement(arg: &str) -> Option<Mouvement> {
    let nombres: Vec<f32> = arg
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split(',')
        .filter_map(|v| v.trim().parse::<f32>().ok())
        .collect();
    if nombres.len() < 4 {
        return None;
    }
    let (t1, t2) = if nombres.len() >= 6 {
        (nombres[4].max(0.0) as u32, nombres[5].max(0.0) as u32)
    } else {
        // Sans bornes : tout l'événement. `t2 = 0` est interprété par `placer` comme
        // « jusqu'à la fin de l'événement ».
        (0, 0)
    };
    Some(Mouvement {
        de: (nombres[0], nombres[1]),
        vers: (nombres[2], nombres[3]),
        t1,
        t2,
    })
}

/// Analyse l'argument de `\t(t1,t2,modifications)`.
///
/// Le contenu après la deuxième virgule est un bloc de balises ordinaires
/// (`\fs40\c&H00FF00&`) : on le réanalyse avec la même machinerie, ce qui évite de
/// dupliquer la connaissance du format.
fn analyser_transformation(arg: &str) -> Option<Transformation> {
    let contenu = arg.trim().trim_start_matches('(').trim_end_matches(')');
    // On ne découpe PAS sur toutes les virgules : le bloc de balises peut en contenir
    // (`\t(0,500,\3c&HFF0000&)` n'en a pas, mais `\t(0,500,\pos(1,2))` si).
    // On isole donc les deux premiers champs puis on garde le reste tel quel.
    let mut morceaux = contenu.splitn(3, ',');
    let premier = morceaux.next()?.trim();
    let second = morceaux.next()?.trim();
    let reste = morceaux.next()?;

    // `	(s40)` : une seule borne donnée — c'est l'accélération, on prend 0..valeur.
    let (t1, t2) = match (premier.parse::<f32>(), second.parse::<f32>()) {
        (Ok(a), Ok(b)) => (a.max(0.0) as u32, b.max(0.0) as u32),
        _ => return None,
    };
    if reste.trim().is_empty() {
        return None;
    }

    let mut tr = Transformation {
        t1,
        t2,
        fontsize: None,
        primary: None,
        secondary: None,
        outline_colour: None,
        bord: None,
        gras: None,
    };
    // Analyse des modifications : même découpage que le bloc principal.
    let mut run_vierge = AssRun {
        text: String::new(),
        fontsize: 0.0,
        primary: [0, 0, 0, 0],
        secondary: [0, 0, 0, 0],
        bold: false,
        italic: false,
        karaoke_start_ms: 0,
        karaoke_dur_ms: 0,
        karaoke_sweep: false,
    };
    let mut contour_vierge: Rgba = [0, 0, 0, 0];
    let mut bord_vierge = 0.0f32;
    let mut alignement_vierge = 0u8;
    let mut pos_vierge = None;
    let mut marge_vierge = 0.0f32;
    let mut fad_vierge = (0u32, 0u32);
    let mut drawing_vierge = false;
    let mut mouvement_vierge = None;
    let mut transformations_vierge: Vec<Transformation> = Vec::new();
    let mut karaoke_attente_vierge = 0u32;
    let mut karaoke_balayage_vierge = false;

    appliquer_balises(
        &format!("\\{}", reste.trim().trim_start_matches('\\')),
        &mut run_vierge,
        &mut contour_vierge,
        &mut bord_vierge,
        &mut alignement_vierge,
        &mut pos_vierge,
        &mut marge_vierge,
        &mut fad_vierge,
        &mut drawing_vierge,
        &mut mouvement_vierge,
        &mut transformations_vierge,
        &mut karaoke_attente_vierge,
        &mut karaoke_balayage_vierge,
        &mut Vec::new(),
        &mut 0.0f32,
        &mut None,
    );

    // On ne retient que ce qui a réellement été écrit dans le bloc.
    if run_vierge.fontsize > 0.0 {
        tr.fontsize = Some(run_vierge.fontsize);
    }
    if run_vierge.primary[3] != 0 {
        tr.primary = Some(run_vierge.primary);
    }
    if contour_vierge[3] != 0 {
        tr.outline_colour = Some(contour_vierge);
    }
    if run_vierge.secondary[3] != 0 {
        tr.secondary = Some(run_vierge.secondary);
    }
    if run_vierge.bold {
        tr.gras = Some(true);
    }
    if bord_vierge > 0.0 {
        tr.bord = Some(bord_vierge);
    }
    Some(tr)
}

/// Noms de balises connus, **du plus long au plus court**.
///
/// Une heuristique « trois lettres maximum » ne suffit pas : `\move` serait lu `mov`
/// et l'argument `e(0,0,200,100)` ne serait plus un nombre (défaut trouvé par les
/// tests : `\move` et `\kf` n'étaient pas analysés). On reconnaît donc les noms
/// réellement définis par le format.
const BALISES_CONNUES: &[&str] = &[
    "iclip", "alpha", "fscx", "fscy", "clip", "fade", "move", "bord", "shad", "blur",
    "frx", "fry", "frz", "fax", "fay", "fsp", "pos", "org", "fad", "fn", "fr", "fs", "be",
    "4c", "3c", "2c", "1c", "4a", "3a", "2a", "1a", "an", "kf", "ko", "c", "p", "t",
    "b", "i", "u", "s", "q", "r", "a", "K", "k",
];

/// Sépare le nom d'une balise de son argument : `fs20` → (`fs`, `20`).
///
/// Les noms sont comparés **avec leur casse** : `\K` (balayage, ancien format) et
/// `\k` (net) sont deux balises différentes.
fn separer_balise(m: &str) -> (&str, &str) {
    for nom in BALISES_CONNUES {
        if let Some(reste) = m.strip_prefix(nom) {
            return (nom, reste);
        }
    }
    // Balise inconnue : on consomme les premiers caractères alpha-numériques pour que
    // l'appelant puisse au moins l'ignorer sans avaler la suite du texte.
    let fin = m
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(m.len());
    (&m[..fin], &m[fin..])
}

/// Horodatage ASS (`H:MM:SS.cc`) en millisecondes.
fn timestamp_ass(ts: &str) -> u64 {
    let parties: Vec<&str> = ts.split(':').collect();
    if parties.len() != 3 {
        return 0;
    }
    let h = parties[0].trim().parse::<u64>().unwrap_or(0);
    let m = parties[1].trim().parse::<u64>().unwrap_or(0);
    let mut it = parties[2].split('.');
    let s = it.next().unwrap_or("0").trim().parse::<u64>().unwrap_or(0);
    let frac = it.next().unwrap_or("0");
    // ASS compte en centièmes ; certains outils écrivent trois chiffres (millièmes).
    let ms = match frac.len() {
        0 => 0,
        1 => frac.parse::<u64>().unwrap_or(0) * 100,
        2 => frac.parse::<u64>().unwrap_or(0) * 10,
        _ => frac[..3].parse::<u64>().unwrap_or(0),
    };
    (h * 3600 + m * 60 + s) * 1000 + ms
}

/// Analyse complète d'un fichier ASS/SSA.
pub fn analyser(contenu: &str) -> AssScript {
    let mut script = AssScript {
        play_res: (384, 288), // valeurs par défaut du format quand rien n'est écrit
        styles: Vec::new(),
        events: Vec::new(),
    };

    let lignes: Vec<&str> = contenu.lines().collect();
    let mut evenements: Vec<AssEvent> = Vec::new();
    let mut section = String::new();
    let mut ordre_dialogue: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lignes.len() {
        let ligne = lignes[i].trim();
        if ligne.starts_with('[') && ligne.ends_with(']') {
            section = ligne.trim_matches(|c| c == '[' || c == ']').to_lowercase();
            i += 1;
            // Les sections de styles sont analysées en bloc (il leur faut `Format:`).
            if section.starts_with("v4") {
                let debut = i;
                while i < lignes.len() && !lignes[i].trim().starts_with('[') {
                    i += 1;
                }
                analyser_styles(&lignes[debut..i], &mut script);
            }
            continue;
        }
        match section.as_str() {
            "script info" => {
                if let Some(valeur) = ligne.strip_prefix("PlayResX:") {
                    if let Ok(v) = valeur.trim().parse::<u32>() {
                        if v > 0 {
                            script.play_res.0 = v;
                        }
                    }
                } else if let Some(valeur) = ligne.strip_prefix("PlayResY:") {
                    if let Ok(v) = valeur.trim().parse::<u32>() {
                        if v > 0 {
                            script.play_res.1 = v;
                        }
                    }
                }
            }
            "events" => {
                if let Some(reste) = ligne.strip_prefix("Format:") {
                    ordre_dialogue = reste
                        .split(',')
                        .map(|c| c.trim().to_lowercase())
                        .collect();
                } else if let Some(reste) = ligne.strip_prefix("Dialogue:") {
                    if ordre_dialogue.is_empty() {
                        // Format standard quand `Format:` manque.
                        ordre_dialogue = [
                            "layer", "start", "end", "style", "name", "marginl", "marginr",
                            "marginv", "effect", "text",
                        ]
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    }
                    // `analyser_dialogue` ne lit que les styles : les événements sont
                    // collectés dans un vecteur séparé, puis rattachés à la fin (sinon
                    // le même `script` serait emprunté mutable et immuable à la fois).
                    analyser_dialogue(reste, &ordre_dialogue, &script, &mut evenements);
                }
            }
            _ => {}
        }
        i += 1;
    }
    script.events = evenements;
    script
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn couleur_bgr_et_alpha_inverse() {
        // &H0000FF& = rouge (B=00, G=00, R=FF) : le piège du format.
        assert_eq!(couleur_ass("&H0000FF&"), Some([255, 0, 0, 255]));
        assert_eq!(couleur_ass("&HFF0000&"), Some([0, 0, 255, 255]));
        assert_eq!(couleur_ass("&H00FF00&"), Some([0, 255, 0, 255]));
        // 8 chiffres : AABBGGRR, alpha &H80 = à moitié transparent donc à moitié opaque.
        assert_eq!(couleur_ass("&H80FFFFFF"), Some([255, 255, 255, 127]));
        assert_eq!(couleur_ass("&HFFFFFFFF"), Some([255, 255, 255, 0]));
    }

    #[test]
    fn horodatage_centiemes() {
        assert_eq!(timestamp_ass("0:00:01.50"), 1500);
        assert_eq!(timestamp_ass("0:01:02.34"), 62340);
        assert_eq!(timestamp_ass("1:00:00.00"), 3_600_000);
        // Certains outils écrivent des millièmes.
        assert_eq!(timestamp_ass("0:00:01.500"), 1500);
    }

    const EXEMPLE: &str = "[Script Info]
PlayResX: 384
PlayResY: 288

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,0,2,10,10,10,1
Style: Haut,Arial,26,&H0000FFFF,&H000000FF,&H00000000,&H80000000,-1,0,0,0,100,100,0,0,1,3,0,8,10,10,10,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,Ligne simple
Dialogue: 0,0:00:04.00,0:00:06.00,Default,,0,0,0,,{\\pos(192,250)\\c&H0000FF&}Rouge positionne
Dialogue: 0,0:00:07.00,0:00:08.00,Haut,,0,0,0,,{\\an8}En haut
Dialogue: 0,0:00:09.00,0:00:10.00,Default,,0,0,0,,Debut{\\c&H00FF00&}vert{\\c&HFF&}rouge2
Dialogue: 0,0:00:11.00,0:00:12.00,Default,,0,0,0,,{\\p1}m 0 0 l 100 0 100 100{\\p0}
";

    #[test]
    fn styles_nommes() {
        let s = analyser(EXEMPLE);
        assert_eq!(s.play_res, (384, 288));
        assert_eq!(s.styles.len(), 2);
        let haut = s.style("Haut").expect("style Haut");
        assert!(haut.bold);
        assert_eq!(haut.alignment, 8);
        assert_eq!(haut.fontsize, 26.0);
        // &H0000FFFF = jaune.
        assert_eq!(haut.primary, [255, 255, 0, 255]);
    }

    #[test]
    fn position_et_couleur_en_ligne() {
        let s = analyser(EXEMPLE);
        let e = s
            .events
            .iter()
            .find(|e| e.texte().contains("Rouge"))
            .expect("événement avec \\pos");
        assert_eq!(e.pos, Some((192.0, 250.0)));
        assert_eq!(e.runs[0].primary, [255, 0, 0, 255]);
        assert_eq!(e.start_ms, 4000);
        assert_eq!(e.end_ms, 6000);
    }

    #[test]
    fn alignement_surcharge() {
        let s = analyser(EXEMPLE);
        let e = s.events.iter().find(|e| e.texte() == "En haut").expect("an8");
        assert_eq!(e.alignment, 8);
    }

    #[test]
    fn fragments_de_couleur() {
        let s = analyser(EXEMPLE);
        let e = s
            .events
            .iter()
            .find(|e| e.texte().contains("vert"))
            .expect("fragments");
        assert_eq!(e.runs.len(), 3);
        assert_eq!(e.runs[0].text, "Debut");
        assert_eq!(e.runs[0].primary, [255, 255, 255, 255]);
        assert_eq!(e.runs[1].text, "vert");
        assert_eq!(e.runs[1].primary, [0, 255, 0, 255]);
        assert_eq!(e.runs[2].text, "rouge2");
        // Piège : `&HFF&` est ROUGE, pas bleu — les octets de poids fort manquants
        // valent zéro, donc B=0, G=0, R=FF. Le bleu s'écrit `&HFF0000&`.
        assert_eq!(e.runs[2].primary, [255, 0, 0, 255]);
        assert_eq!(couleur_ass("&HFF0000&"), Some([0, 0, 255, 255]));
    }

    #[test]
    fn dessin_vectoriel_ignore() {
        let s = analyser(EXEMPLE);
        let e = s.events.iter().find(|e| e.drawing).expect("événement \\p1");
        assert!(!e.a_du_texte(), "un dessin ne doit pas être affiché comme du texte");
        assert!(s.events_at(11_500).is_empty());
    }

    #[test]
    fn selection_par_temps_et_fondu() {
        let s = analyser(EXEMPLE);
        assert!(s.events_at(500).is_empty());
        assert_eq!(s.events_at(2000).len(), 1);
        assert_eq!(s.events_at(2000)[0].texte(), "Ligne simple");
    }

    /// Mesureur synthétique : chaque caractère fait la moitié de la taille de police.
    /// Il rend les positions calculables à la main dans les tests.
    fn mesurer(texte: &str, police: f32, _gras: bool) -> f32 {
        texte.chars().count() as f32 * police * 0.5
    }

    #[test]
    fn placement_centre_bas_par_defaut() {
        let s = analyser(EXEMPLE);
        let e = s.events.iter().find(|e| e.texte() == "Ligne simple").unwrap();
        // Zone 384×288 = PlayRes : aucune mise à l'échelle, calcul lisible.
        let places = placer(e, 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        assert_eq!(places.len(), 1);
        let p = &places[0];
        // « Ligne simple » = 12 caractères × 20 pt × 0,5 = 120 px de large.
        assert!((p.width - 120.0).abs() < 0.01, "largeur {}", p.width);
        // Alignement 2 (bas-centre) : centré horizontalement.
        assert!((p.x - (384.0 - 120.0) / 2.0).abs() < 0.01, "x {}", p.x);
        // Marge verticale 10 : le texte finit 10 px au-dessus du bas.
        assert!((p.y + 20.0 * 1.2 + 10.0 - 288.0).abs() < 0.01, "y {}", p.y);
    }

    #[test]
    fn placement_pos_et_alignement_haut() {
        let s = analyser(EXEMPLE);
        let e = s.events.iter().find(|e| e.texte().contains("Rouge")).unwrap();
        let places = placer(e, 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        // \pos(192,250) avec alignement 2 : le bas du texte est à y=250, centré en x.
        let p = &places[0];
        assert!((p.y + 24.0 - 250.0).abs() < 0.01, "bas du texte à 250 : y={}", p.y);
        assert!((p.x + p.width / 2.0 - 192.0).abs() < 0.01, "centré sur 192 : x={}", p.x);
    }

    #[test]
    fn mise_a_l_echelle_de_la_resolution_de_reference() {
        let s = analyser(EXEMPLE);
        let e = s.events.iter().find(|e| e.texte() == "Ligne simple").unwrap();
        // Zone deux fois plus grande que PlayRes : police deux fois plus grosse.
        let p1 = placer(e, 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        let p2 = placer(e, 768.0, 576.0, (384, 288), 1.0, 0, &mesurer);
        assert!((p2[0].font_px - p1[0].font_px * 2.0).abs() < 0.01);
        assert!((p2[0].width - p1[0].width * 2.0).abs() < 0.01);
    }

    #[test]
    fn multi_lignes_empilees_dans_l_ordre() {
        let s = analyser(
            "[Script Info]\nPlayResX: 384\nPlayResY: 288\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,Premiere\\NDeuxieme\n",
        );
        let e = &s.events[0];
        let places = placer(e, 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        assert_eq!(places.len(), 2);
        assert_eq!(places[0].text, "Premiere");
        assert_eq!(places[1].text, "Deuxieme");
        assert!(places[1].y > places[0].y, "la 2e ligne doit être sous la 1re");
    }

    #[test]
    fn opacite_du_fondu() {
        let texte = "[Script Info]\nPlayResY: 288\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:10.00,0:00:12.00,Default,,0,0,0,,{\\fad(1000,1000)}Fondu\n";
        let s = analyser(texte);
        let e = &s.events[0];
        assert!((s.opacite(e, 10_000) - 0.0).abs() < 0.01, "début du fondu");
        assert!((s.opacite(e, 10_500) - 0.5).abs() < 0.02, "milieu du fondu");
        assert!((s.opacite(e, 11_000) - 1.0).abs() < 0.01, "pleine opacité");
        assert!((s.opacite(e, 11_500) - 0.5).abs() < 0.02, "fondu de sortie");
    }

    /// Fichier minimal pour tester une seule ligne de `Dialogue:`.
    fn script_simple(dialogue: &str) -> AssScript {
        analyser(&format!(
            "[Script Info]\nPlayResX: 384\nPlayResY: 288\n\n\
             [V4+ Styles]\n\
             Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n\
             Style: Default,Arial,20,&H00FFFFFF,&H000000FF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,2,0,2,10,10,10,1\n\n\
             [Events]\n\
             Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
             Dialogue: 0,0:00:00.00,0:00:10.00,Default,,0,0,0,,{}\n",
            dialogue
        ))
    }

    #[test]
    fn mouvement_interpole_la_position() {
        // \move(0,0,200,100) sur tout l'événement (10 s).
        let s = script_simple("{\\move(0,0,200,100)}Bouge");
        let e = &s.events[0];
        assert!(e.pos.is_none(), "\\move remplace \\pos");
        assert!(e.mouvement.is_some());

        let debut = placer(e, 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        let milieu = placer(e, 384.0, 288.0, (384, 288), 1.0, 5000, &mesurer);
        let fin = placer(e, 384.0, 288.0, (384, 288), 1.0, 10_000, &mesurer);
        // L'alignement 2 est « bas-centre » : `\move` donne le CENTRE du bloc, pas son
        // coin gauche (piège : mes tests comparaient le coin gauche, d'où un écart de
        // la demi-largeur du texte).
        let centre = |p: &RunPlace| p.x + p.width / 2.0;
        assert!(centre(&debut[0]).abs() < 1.0, "début centre={}", centre(&debut[0]));
        assert!(
            (centre(&milieu[0]) - 100.0).abs() < 1.0,
            "milieu centre={}",
            centre(&milieu[0])
        );
        assert!(
            (centre(&fin[0]) - 200.0).abs() < 1.0,
            "fin centre={}",
            centre(&fin[0])
        );
        // La hauteur suit la même interpolation : le BAS du bloc va de 0 à 100
        // (alignement « bas-centre » : `y` est le haut du bloc, pas sa ligne de base).
        let bas = |p: &RunPlace| p.y + p.height;
        assert!(bas(&debut[0]).abs() < 1.0, "début bas={}", bas(&debut[0]));
        assert!((bas(&fin[0]) - 100.0).abs() < 1.0, "fin bas={}", bas(&fin[0]));
    }

    #[test]
    fn mouvement_avec_bornes_temporelles() {
        // \move(0,0,200,0,2000,4000) : immobile jusqu'à 2 s, puis 2 s de déplacement.
        let s = script_simple("{\\move(0,0,200,0,2000,4000)}Bouge");
        let e = &s.events[0];
        let avant = placer(e, 384.0, 288.0, (384, 288), 1.0, 1000, &mesurer);
        let milieu = placer(e, 384.0, 288.0, (384, 288), 1.0, 3000, &mesurer);
        let apres = placer(e, 384.0, 288.0, (384, 288), 1.0, 6000, &mesurer);
        // Le centre du bloc (alignement bas-centre), pas son coin gauche.
        let centre = |p: &RunPlace| p.x + p.width / 2.0;
        assert!(centre(&avant[0]).abs() < 1.0, "avant t1 : {}", centre(&avant[0]));
        assert!(
            (centre(&milieu[0]) - 100.0).abs() < 1.0,
            "milieu : {}",
            centre(&milieu[0])
        );
        assert!(
            (centre(&apres[0]) - 200.0).abs() < 1.0,
            "après t2 : {}",
            centre(&apres[0])
        );
    }

    #[test]
    fn transformation_de_taille_progressive() {
        // \t(0,1000,\fs40) : la police passe de 20 à 40 pendant la première seconde.
        let s = script_simple("{\\t(0,1000,\\fs40)}Grandis");
        let e = &s.events[0];
        let t0 = placer(e, 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        let t500 = placer(e, 384.0, 288.0, (384, 288), 1.0, 500, &mesurer);
        let t1000 = placer(e, 384.0, 288.0, (384, 288), 1.0, 1000, &mesurer);
        assert!((t0[0].font_px - 20.0).abs() < 0.5, "t0 : {}", t0[0].font_px);
        assert!((t500[0].font_px - 30.0).abs() < 0.5, "t=500 : {}", t500[0].font_px);
        assert!((t1000[0].font_px - 40.0).abs() < 0.5, "t=1000 : {}", t1000[0].font_px);
    }

    #[test]
    fn transformation_de_couleur_progressive() {
        // De blanc (&H00FFFFFF) vers rouge (&H000000FF) en 1 s.
        let s = script_simple("{\\t(0,1000,\\c&H0000FF&)}Rougis");
        let e = &s.events[0];
        let t0 = placer(e, 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        let t500 = placer(e, 384.0, 288.0, (384, 288), 1.0, 500, &mesurer);
        let t1000 = placer(e, 384.0, 288.0, (384, 288), 1.0, 1000, &mesurer);
        assert_eq!(t0[0].colour[0..3], [255, 255, 255], "départ blanc");
        // À mi-parcours : rouge à ~255, vert et bleu à ~128.
        assert!(t500[0].colour[0] > 240, "rouge à mi-parcours : {:?}", t500[0].colour);
        assert!(
            (t500[0].colour[1] as i32 - 128).abs() < 12,
            "vert à mi-parcours : {:?}",
            t500[0].colour
        );
        assert_eq!(t1000[0].colour[0..3], [255, 0, 0], "arrivée rouge");
    }

    #[test]
    fn karaoke_balayage_kf() {
        // Deux syllabes de 1 s chacune : « A » puis « B ».
        // &H0000FF& = rouge (couleur principale), secondaire = &H000000FF (bleu).
        let s = script_simple("{\\kf100}A{\\kf100}B");
        let e = &s.events[0];
        // Le karaoké doit découper l'événement en deux fragments porteurs de durée.
        assert_eq!(e.runs.len(), 2, "fragments : {:?}", e.runs.iter().map(|r| &r.text).collect::<Vec<_>>());
        assert_eq!(e.runs[0].karaoke_dur_ms, 1000);
        assert_eq!(e.runs[0].karaoke_start_ms, 0);
        assert_eq!(e.runs[1].karaoke_start_ms, 1000);
        assert!(e.runs[0].karaoke_sweep, "\\kf doit balayer");

        let t0 = placer(e, 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        let t500 = placer(e, 384.0, 288.0, (384, 288), 1.0, 500, &mesurer);
        let t1000 = placer(e, 384.0, 288.0, (384, 288), 1.0, 1000, &mesurer);
        // Début : rien n'est encore chanté.
        assert_eq!(t0[0].karaoke.map(|k| k.1), Some(0.0));
        // Mi-syllabe : à moitié balayée.
        assert!(
            (t500[0].karaoke.unwrap().1 - 0.5).abs() < 0.02,
            "mi-parcours : {:?}",
            t500[0].karaoke
        );
        // Fin de la 1re syllabe : entièrement chantée, la 2e commence.
        assert_eq!(t1000[0].karaoke.map(|k| k.1), Some(1.0));
        assert_eq!(t1000[1].karaoke.map(|k| k.1), Some(0.0));
    }

    #[test]
    fn karaoke_net_k_bascule_au_debut() {
        // `\k` change de couleur d'un coup au début de la syllabe (pas de balayage).
        let s = script_simple("{\\k100}Net");
        let e = &s.events[0];
        assert!(!e.runs[0].karaoke_sweep);
        let juste_avant = placer(e, 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        let juste_apres = placer(e, 384.0, 288.0, (384, 288), 1.0, 10, &mesurer);
        assert_eq!(juste_avant[0].karaoke.map(|k| k.1), Some(0.0));
        assert_eq!(juste_apres[0].karaoke.map(|k| k.1), Some(1.0));
    }

    #[test]
    fn sans_karaoke_pas_de_balayage() {
        let s = script_simple("Texte ordinaire");
        let e = &s.events[0];
        let p = placer(e, 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        assert!(p[0].karaoke.is_none());
    }

    #[test]
    fn clip_rectangulaire_est_lu_et_normalise() {
        let s = script_simple("{\\clip(110,60,10,20)}Découpé");
        let e = &s.events[0];
        assert_eq!(e.clips.len(), 1);
        let c = e.clips[0];
        assert!(!c.inverse && !c.vecteur && c.exact);
        // Les bornes inversées sont remises dans l'ordre : un fichier qui écrit
        // x2 < x1 doit découper pareil, pas ne rien afficher.
        assert_eq!((c.x1, c.y1, c.x2, c.y2), (10.0, 20.0, 110.0, 60.0));
        // Le texte n'est pas modifié par la balise.
        assert_eq!(e.texte(), "Découpé");
    }

    #[test]
    fn iclip_est_marque_comme_inverse() {
        let s = script_simple("{\\iclip(10,20,110,60)}Troué");
        let c = s.events[0].clips[0];
        assert!(c.inverse);
        let e = &s.events[0];
        let p = placer(e, 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        let place = p[0].clip.as_ref().expect("découpage présent");
        assert!(place.garde.is_none(), "un \\iclip ne garde rien");
        assert_eq!(place.exclusions.len(), 1);
        assert_eq!(place.exclusions[0], (10.0, 20.0, 110.0, 60.0));
    }

    #[test]
    fn clip_chemin_rectangulaire_est_exact() {
        // Forme réelle des fansubs : deux colonnes de texte, découpées par un chemin.
        let s = script_simple("{\\clip(m 0 0 l 100 0 100 50 0 50)}Colonne");
        let c = s.events[0].clips[0];
        assert!(c.vecteur);
        assert!(c.exact, "un chemin rectangulaire doit être reconnu exactement");
        assert_eq!((c.x1, c.y1, c.x2, c.y2), (0.0, 0.0, 100.0, 50.0));
    }

    #[test]
    fn clip_chemin_quelconque_est_seulement_borne() {
        // Un triangle : le découpage exact n'est pas possible, on garde les bornes
        // et on le SIGNALE (exact = false) au lieu de faire semblant.
        let s = script_simple("{\\clip(m 10 10 l 90 10 50 70)}Triangle");
        let c = s.events[0].clips[0];
        assert!(!c.exact, "un triangle ne doit pas être annoncé exact");
        assert_eq!((c.x1, c.y1, c.x2, c.y2), (10.0, 10.0, 90.0, 70.0));
        // Un rectangle « en L » (point intérieur) n'est pas non plus exact.
        let s2 = script_simple("{\\clip(m 0 0 l 100 0 100 100 50 50 0 100)}En L");
        assert!(!s2.events[0].clips[0].exact);
    }

    #[test]
    fn clip_avec_echelle_en_tete_est_lu() {
        // `\clip(2,m 0 0 l 50 25)` : l'échelle de dessin précède le chemin, elle ne
        // doit pas être prise pour une coordonnée.
        let s = script_simple("{\\clip(2,m 0 0 l 50 25)}Echelle");
        let c = s.events[0].clips[0];
        assert_eq!((c.x1, c.y1, c.x2, c.y2), (0.0, 0.0, 50.0, 25.0));
    }

    #[test]
    fn plusieurs_clips_se_cumulent() {
        let s = script_simple("{\\clip(0,0,200,200)\\clip(50,50,300,300)}Deux");
        let e = &s.events[0];
        assert_eq!(e.clips.len(), 2);
        let p = placer(e, 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        let place = p[0].clip.as_ref().unwrap();
        // Intersection : le plus petit des deux.
        assert_eq!(place.garde, Some((50.0, 50.0, 200.0, 200.0)));
    }

    #[test]
    fn le_decoupage_suit_lechelle_de_la_zone() {
        // PlayRes 384×288 affiché dans 768×576 : tout est doublé, découpage compris.
        let s = script_simple("{\\clip(10,20,110,60)}Echelle zone");
        let e = &s.events[0];
        let p = placer(e, 768.0, 576.0, (384, 288), 1.0, 0, &mesurer);
        let place = p[0].clip.as_ref().unwrap();
        assert_eq!(place.garde, Some((20.0, 40.0, 220.0, 120.0)));
    }

    #[test]
    fn sans_clip_aucun_decoupage_n_est_applique() {
        let s = script_simple("Texte nu");
        let p = placer(&s.events[0], 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        assert!(p[0].clip.is_none());
        assert_eq!(p[0].angle, 0.0);
    }

    #[test]
    fn frz_et_fr_tournent_le_texte_autour_de_org() {
        // `\org(192,144)` = centre de la zone ; `\frz90` fait tourner d'un quart de
        // tour dans le sens horaire (à l'écran, y vers le bas).
        let s = script_simple("{\\pos(192,144)\\org(192,144)\\frz90\\an5}Tourne");
        let e = &s.events[0];
        assert_eq!(e.rotation_z, 90.0);
        assert_eq!(e.origin, Some((192.0, 144.0)));
        let p = placer(e, 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        assert!((p[0].angle - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
        // Sans rotation : « Tourne » (6 caractères × 20 × 0,5 = 60 px), centré en
        // (192,144), a son coin haut-gauche en (162,132). Un quart de tour horaire
        // autour du pivot amène ce coin en (192 + 12, 144 − 30) = (204,114).
        let sans = script_simple("{\\pos(192,144)\\an5}Tourne");
        let q = placer(&sans.events[0], 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        assert!((q[0].x - 162.0).abs() < 0.5, "sans rotation, x = {}", q[0].x);
        assert!((q[0].y - 132.0).abs() < 0.5, "sans rotation, y = {}", q[0].y);
        assert!((p[0].x - 204.0).abs() < 0.5, "après rotation, x = {}", p[0].x);
        assert!((p[0].y - 114.0).abs() < 0.5, "après rotation, y = {}", p[0].y);
    }

    #[test]
    fn fr_alias_de_frz() {
        let s = script_simple("{\\fr45}Incline");
        assert_eq!(s.events[0].rotation_z, 45.0);
        // `\frx` / `\fry` restent hors périmètre (rotation 3D) : ils ne doivent pas
        // être confondus avec la rotation Z.
        let s2 = script_simple("{\\frx45}3D");
        assert_eq!(s2.events[0].rotation_z, 0.0);
    }

    #[test]
    fn rotation_sans_org_se_fait_autour_du_point_d_ancrage() {
        // Sans `\org`, le pivot est le point d'ancrage : ici un texte centré en bas
        // (an2 par défaut) `\pos(192,250)` → le pivot est (192,250), et non (0,0).
        let s = script_simple("{\\pos(192,250)\\frz180}Retourne");
        let e = &s.events[0];
        let p = placer(e, 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        let sans = script_simple("{\\pos(192,250)}Retourne");
        let q = placer(&sans.events[0], 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        // Sans rotation : « Retourne » (8 × 10 = 80 px), aligné en bas-centre sur
        // (192,250), a son coin haut-gauche en (152,226). Un demi-tour autour du pivot
        // (192,250) — et non autour de (0,0), ce qui donnerait (−152,−226) — amène ce
        // coin en (232,274) : le bloc passe sous le point d'ancrage, tête en bas.
        assert!((q[0].x - 152.0).abs() < 0.5, "sans rotation, x = {}", q[0].x);
        assert!((q[0].y - 226.0).abs() < 0.5, "sans rotation, y = {}", q[0].y);
        assert!((p[0].x - 232.0).abs() < 0.5, "après demi-tour, x = {}", p[0].x);
        assert!((p[0].y - 274.0).abs() < 0.5, "après demi-tour, y = {}", p[0].y);
        assert!((p[0].angle - std::f32::consts::PI).abs() < 1e-4);
    }

    #[test]
    fn une_rotation_nulle_ne_change_rien_au_placement() {
        let a = script_simple("{\\pos(100,100)\\frz0}Identique");
        let b = script_simple("{\\pos(100,100)}Identique");
        let pa = placer(&a.events[0], 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        let pb = placer(&b.events[0], 384.0, 288.0, (384, 288), 1.0, 0, &mesurer);
        assert_eq!(pa[0].x, pb[0].x);
        assert_eq!(pa[0].y, pb[0].y);
        assert_eq!(pa[0].angle, 0.0);
    }

    #[test]
    fn balise_inconnue_ne_reste_pas_dans_le_texte() {
        let s = analyser(
            "[Script Info]\nPlayResY: 288\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,{\\t(0,500,\\fs40)}Avec transformation\n",
        );
        let e = &s.events[0];
        assert_eq!(e.texte(), "Avec transformation");
        assert!(!e.texte().contains('{'));
    }
}

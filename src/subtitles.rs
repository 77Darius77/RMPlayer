//! Gestion des sous-titres externes (SRT, ASS/SSA)
//!
//! Fonctionnalités :
//!   - Analyse des formats SRT et ASS/SSA (analyseur interne, sans dépendance externe)
//!   - Chargement automatique : fichier homonyme, suffixe de langue, sous-dossiers
//!   - Rendu en surimpression par l'interface
//!
//! Limite connue : la mise en forme ASS (position, couleurs, styles) est retirée ;
//! seul le texte des événements est conservé (voir docs/AVANCEMENT.md).

use std::path::{Path, PathBuf};

/// Sous-titre chargé
pub struct Subtitle {
    pub path: PathBuf,
    pub language: String,
    pub events: Vec<SubtitleEvent>,
    /// Analyse ASS complète (styles, positions, couleurs), absente pour un SRT.
    ///
    /// Les champs `events` ci-dessus restent la vue « texte simple » utilisée par le
    /// rendu uniforme et par la recherche ; `ass` porte la mise en page réelle.
    pub ass: Option<crate::ass::AssScript>,
}

/// Événement de sous-titre (timing + texte)
#[derive(Debug, Clone)]
pub struct SubtitleEvent {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

/// Gestionnaire de sous-titres
pub struct SubtitleManager {
    /// Sous-titres chargés
    subtitles: Vec<Subtitle>,
    /// Sous-titre actif
    active_subtitle: Option<usize>,
    /// Visible
    visible: bool,
}

impl SubtitleManager {
    /// Créer un nouveau gestionnaire
    pub fn new() -> Self {
        tracing::info!("Gestionnaire de sous-titres initialisé");
        Self {
            subtitles: Vec::new(),
            active_subtitle: None,
            visible: true,
        }
    }
    
    /// Charger un fichier de sous-titres (ASS/SSA ou SRT selon l'extension)
    pub fn load_subtitle(&mut self, path: PathBuf) -> Result<(), String> {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Erreur lecture sous-titre: {}", e))?;

        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        let est_srt =
            ext == "srt" || content.trim_start().starts_with("1\n") || content.contains("-->");
        let (events, ass) = if est_srt {
            (Self::parse_srt_content(&content)?, None)
        } else {
            let script = crate::ass::analyser(&content);
            tracing::info!(
                "ASS : {} style(s), {} événement(s), référence {}×{}",
                script.styles.len(),
                script.events.len(),
                script.play_res.0,
                script.play_res.1
            );
            (Self::parse_ass_content(&content)?, Some(script))
        };

        let subtitle_language = Self::detect_language(&path);
        self.subtitles.push(Subtitle {
            path: path.clone(),
            language: subtitle_language.clone(),
            events,
            ass,
        });
        tracing::info!("Sous-titre chargé: {} (langue: {})", 
            path.display(), subtitle_language);
        Ok(())
    }
    
    /// Parser le contenu ASS/SSA
    fn parse_ass_content(content: &str) -> Result<Vec<SubtitleEvent>, String> {
        let mut events = Vec::new();
        
        for line in content.lines() {
            if line.starts_with("Dialogue:") {
                // Format: Dialogue: Layer,Start,End,Style,Name,MarginL,MarginR,MarginV,Effect,Text
                if let Some(parts) = line.strip_prefix("Dialogue:") {
                    let fields: Vec<&str> = parts.splitn(10, ',').collect();
                    if fields.len() >= 10 {
                        let start_str = fields[1].trim();
                        let end_str = fields[2].trim();
                        let text = fields[9].trim().to_string();
                        
                        // Parser les timestamps (Format: H:MM:SS.cc)
                        let start_ms = Self::parse_timestamp(start_str);
                        let end_ms = Self::parse_timestamp(end_str);
                        
                        events.push(SubtitleEvent {
                            start_ms,
                            end_ms,
                            text,
                        });
                    }
                }
            }
        }
        
        Ok(events)
    }
    
    /// Parser un fichier SRT (SubRip).
    fn parse_srt_content(content: &str) -> Result<Vec<SubtitleEvent>, String> {
        let mut events = Vec::new();
        let mut lines = content.lines().peekable();

        while let Some(line) = lines.next() {
            // Une ligne de temps SRT contient "-->"
            if !line.contains("-->") {
                continue;
            }
            let Some((start_str, end_str)) = line.split_once("-->") else {
                continue;
            };
            let start_ms = Self::parse_srt_timestamp(start_str.trim());
            let end_ms = Self::parse_srt_timestamp(end_str.trim());

            // Le texte du bloc court jusqu'à la ligne vide suivante
            let mut text = String::new();
            while let Some(t) = lines.peek() {
                let t = t.trim_end_matches('\r');
                if t.trim().is_empty() {
                    lines.next();
                    break;
                }
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(t);
                lines.next();
            }

            if !text.trim().is_empty() {
                events.push(SubtitleEvent {
                    start_ms,
                    end_ms,
                    text: Self::clean_html_tags(text.trim()),
                });
            }
        }

        Ok(events)
    }

    /// Parser un timestamp SRT (HH:MM:SS,mmm ou HH:MM:SS.mmm)
    fn parse_srt_timestamp(ts: &str) -> u64 {
        // Retirer d'éventuelles coordonnées de position SRT (X1:.. X2:..)
        let ts = ts.split_whitespace().next().unwrap_or("").replace(',', ".");
        let parts: Vec<&str> = ts.split(':').collect();
        let (h, m, rest) = match parts.len() {
            3 => (
                parts[0].parse::<u64>().unwrap_or(0),
                parts[1].parse::<u64>().unwrap_or(0),
                parts[2],
            ),
            2 => (0, parts[0].parse::<u64>().unwrap_or(0), parts[1]),
            _ => return 0,
        };
        let mut it = rest.splitn(2, '.');
        let secs = it.next().unwrap_or("0").parse::<u64>().unwrap_or(0);
        let frac = it.next().unwrap_or("0");
        let millis = match frac.len() {
            0 => 0,
            1 => frac.parse::<u64>().unwrap_or(0) * 100,
            2 => frac.parse::<u64>().unwrap_or(0) * 10,
            _ => frac[..3].parse::<u64>().unwrap_or(0),
        };
        (h * 3600 + m * 60 + secs) * 1000 + millis
    }

    /// Retirer les balises HTML simples (<i>, <b>, <font ...>) des sous-titres SRT.
    fn clean_html_tags(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut depth = 0usize;
        for c in text.chars() {
            match c {
                '<' => depth += 1,
                '>' => depth = depth.saturating_sub(1),
                _ if depth == 0 => out.push(c),
                _ => {}
            }
        }
        out
    }

    /// Parser un timestamp ASS (H:MM:SS.cc → ms)
    fn parse_timestamp(ts: &str) -> u64 {
        // Format: H:MM:SS.cc ou MM:SS.cc
        let parts: Vec<&str> = ts.split(':').collect();
        if parts.len() == 3 {
            let hours = parts[0].parse::<u64>().unwrap_or(0);
            let minutes = parts[1].parse::<u64>().unwrap_or(0);
            let sec_parts: Vec<&str> = parts[2].split('.').collect();
            let seconds = sec_parts[0].parse::<u64>().unwrap_or(0);
            let centis = sec_parts.get(1).map(|s| s.parse::<u64>().unwrap_or(0)).unwrap_or(0);
            
            // ASS compte en centisecondes : ×10 pour obtenir des millisecondes
            (hours * 3600 + minutes * 60 + seconds) * 1000 + centis * 10
        } else if parts.len() == 2 {
            let minutes = parts[0].parse::<u64>().unwrap_or(0);
            let sec_parts: Vec<&str> = parts[1].split('.').collect();
            let seconds = sec_parts[0].parse::<u64>().unwrap_or(0);
            let centis = sec_parts.get(1).map(|s| s.parse::<u64>().unwrap_or(0)).unwrap_or(0);

            (minutes * 60 + seconds) * 1000 + centis * 10
        } else {
            0
        }
    }
    
    /// Détecter la langue du sous-titre depuis le nom de fichier
    fn detect_language(path: &Path) -> String {
        let filename = path.file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        
        if filename.contains("fra") || filename.contains("fre") || filename.contains("french") {
            "fr".to_string()
        } else if filename.contains("eng") || filename.contains("english") {
            "en".to_string()
        } else if filename.contains("deu") || filename.contains("german") {
            "de".to_string()
        } else if filename.contains("spa") || filename.contains("spanish") {
            "es".to_string()
        } else if filename.contains("jpn") || filename.contains("japanese") {
            "ja".to_string()
        } else if filename.contains("kor") || filename.contains("korean") {
            "ko".to_string()
        } else {
            "und".to_string() // indéterminé
        }
    }
    
    /// Charge automatiquement les sous-titres d'un média.
    ///
    /// Ordre de préférence (le premier chargé devient la piste active) :
    ///   1. `<nom>.<langue>.<ext>`  — piste correspondant à la langue demandée
    ///   2. `<nom>.<ext>`           — piste homonyme sans suffixe de langue
    ///   3. fichiers des sous-dossiers (`subs/`, `subtitles/`, `sous-titres/`, `sub/`)
    ///      contenant la langue demandée, puis les autres
    pub fn auto_load(&mut self, video_path: &Path, preferred_language: &str) {
        let dir = video_path.parent().unwrap_or(Path::new(""));
        let stem = video_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let lang = preferred_language.trim().to_lowercase();
        let extensions = ["ass", "ssa", "srt", "sub"];

        let mut candidates: Vec<PathBuf> = Vec::new();
        let push_unique = |list: &mut Vec<PathBuf>, path: PathBuf| {
            if path.is_file() && !list.contains(&path) {
                list.push(path);
            }
        };

        // 1) homonymes avec suffixe de langue
        if !lang.is_empty() {
            for ext in &extensions {
                push_unique(&mut candidates, dir.join(format!("{}.{}.{}", stem, lang, ext)));
            }
        }
        // 2) homonymes simples
        for ext in &extensions {
            push_unique(&mut candidates, dir.join(format!("{}.{}", stem, ext)));
        }
        // 3) sous-dossiers : langue d'abord, autres ensuite
        let is_subtitle = |path: &Path| {
            path.extension()
                .map(|e| extensions.iter().any(|x| e.to_string_lossy().to_lowercase() == *x))
                .unwrap_or(false)
        };
        let subdirs = ["subs", "subtitles", "sous-titres", "sub"];
        let mut preferred: Vec<PathBuf> = Vec::new();
        let mut others: Vec<PathBuf> = Vec::new();
        for subdir in &subdirs {
            let sub_path = dir.join(subdir);
            if !sub_path.is_dir() {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&sub_path) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !is_subtitle(&path) {
                    continue;
                }
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                let matches_lang = !lang.is_empty()
                    && (name.contains(&format!(".{}.", lang))
                        || name.contains(&format!("{}.", lang))
                        || name.starts_with(&stem));
                if matches_lang {
                    push_unique(&mut preferred, path);
                } else {
                    push_unique(&mut others, path);
                }
            }
        }
        for path in preferred.into_iter().chain(others) {
            push_unique(&mut candidates, path);
        }

        let count = candidates.len();
        for path in candidates {
            let shown = path.display().to_string();
            if let Err(e) = self.load_subtitle(path) {
                tracing::warn!("Erreur chargement auto sous-titre: {}", e);
            } else {
                tracing::info!("Sous-titre chargé automatiquement: {}", shown);
            }
        }

        // La première piste chargée est la meilleure candidate : elle devient active
        if !self.subtitles.is_empty() && self.active_subtitle.is_none() {
            self.active_subtitle = Some(0);
        }
        if count > 1 {
            tracing::info!("{} pistes de sous-titres externes chargées", count);
        }
    }

    /// Obtenir les sous-titres actifs pour un temps donné (ms)
    pub fn get_active_subtitles(&self, time_ms: u64) -> Vec<String> {
        if !self.visible || self.active_subtitle.is_none() {
            return vec![];
        }
        
        let mut result = Vec::new();
        
        if let Some(idx) = self.active_subtitle {
            if let Some(subtitle) = self.subtitles.get(idx) {
                for event in &subtitle.events {
                    if time_ms >= event.start_ms && time_ms <= event.end_ms {
                        // Nettoyer les codes de style ASS ({\pos(...)}, {\c...}, etc.)
                        let clean_text = Self::clean_ass_tags(&event.text);
                        result.push(clean_text);
                    }
                }
            }
        }
        
        result
    }
    
    /// Supprimer les balises de style ASS du texte
    fn clean_ass_tags(text: &str) -> String {
        let mut result = text.to_string();
        
        // Supprimer les balises de position
        while result.contains("{\\pos(") {
            if let Some(start) = result.find("{\\pos(") {
                if let Some(end) = result[start..].find('}') {
                    result.replace_range(start..start + end + 1, "");
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        
        // Supprimer les balises de couleur
        while result.contains("{\\c") {
            if let Some(start) = result.find("{\\c") {
                if let Some(end) = result[start..].find('}') {
                    result.replace_range(start..start + end + 1, "");
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        
        // Supprimer les autres balises ASS ({...})
        let mut clean = String::new();
        let mut in_brace = false;
        for c in result.chars() {
            if c == '{' {
                in_brace = true;
            } else if c == '}' {
                in_brace = false;
            } else if !in_brace {
                clean.push(c);
            }
        }
        
        clean
    }
    
    /// Changer le sous-titre actif
    pub fn next_subtitle(&mut self) {
        if self.subtitles.is_empty() {
            return;
        }
        
        let current = self.active_subtitle.unwrap_or(0);
        let next = (current + 1) % self.subtitles.len();
        self.active_subtitle = Some(next);
        tracing::info!("Sous-titre actif: {}", next);
    }
    
    /// Activer/désactiver l'affichage des sous-titres
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Les sous-titres sont-ils affichés ?
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Basculer l'affichage
    pub fn toggle_visible(&mut self) -> bool {
        self.visible = !self.visible;
        self.visible
    }

    /// Nom du sous-titre actif (fichier) s'il y en a un
    pub fn active_name(&self) -> Option<String> {
        let idx = self.active_subtitle?;
        self.subtitles
            .get(idx)
            .and_then(|s| s.path.file_name().map(|n| n.to_string_lossy().to_string()))
    }

    /// Langue détectée de la piste active (« fr », « und »…), affichée dans le menu.
    pub fn active_language(&self) -> Option<String> {
        let idx = self.active_subtitle?;
        self.subtitles.get(idx).map(|s| s.language.clone())
    }

    /// Nombre de sous-titres chargés
    pub fn count(&self) -> usize {
        self.subtitles.len()
    }
    
    /// Rendre les sous-titres actifs (texte à afficher)
    /// Script ASS du sous-titre actif, s'il s'agit d'un ASS/SSA.
    ///
    /// Sert au rendu par événement (position, couleur, taille du fichier) : sans lui,
    /// un ASS est affiché comme un SRT, en perdant toute sa mise en page.
    pub fn active_ass(&self) -> Option<&crate::ass::AssScript> {
        let i = self.active_subtitle?;
        self.subtitles.get(i)?.ass.as_ref()
    }

    pub fn render_active(&self, time_ms: u64) -> String {
        let subs = self.get_active_subtitles(time_ms);
        subs.join("\n")
    }
}

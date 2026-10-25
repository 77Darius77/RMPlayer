//! Fonctions utilitaires
//! 
//! - Détection de fichiers vidéo
//! - Formatage du temps
//! - Formatage du bitrate
//! - Informations système

use std::path::Path;

/// Vérifier si un fichier est une vidéo
/// Vrai si la chaîne est une URL de flux que FFmpeg sait ouvrir directement.
///
/// http(s)/HLS/RTSP/RTMP/SMB couvrent l'essentiel : un site qui sert du HLS, une
/// caméra RTSP, un partage réseau. FFmpeg gère le protocole ; le lecteur n'a qu'à
/// transmettre l'adresse. (Pas de yt-dlp : hors périmètre, et une dépendance
/// externe pour un seul usage.)
pub fn is_stream_url(texte: &str) -> bool {
    let t = texte.trim().to_lowercase();
    ["http://", "https://", "rtsp://", "rtmp://", "rtmps://", "mms://", "smb://", "ftp://"]
        .iter()
        .any(|p| t.starts_with(p))
}

/// Types MIME annoncés par RMPlayer — **une seule liste** pour le lanceur
/// (`assets/rmplayer.desktop.in`) et pour MPRIS (`SupportedMimeTypes`).
///
/// Tous sont présents dans shared-mime-info (vérifié en session 33 : quatre types
/// envisagés n'existent pas et ont été retirés — les annoncer ne pouvait rien
/// déclencher).
pub const TYPES_MIME: &[&str] = &[
    "video/mp4",
    "video/x-matroska",
    "video/webm",
    "video/quicktime",
    "video/x-msvideo",
    "video/mpeg",
    "video/mp2t",
    "video/x-flv",
    "video/3gpp",
    "video/3gpp2",
    "video/ogg",
    "video/x-ms-wmv",
    "video/x-ms-asf",
    "video/x-m4v",
    "application/vnd.rn-realmedia",
    "application/x-matroska",
];

pub fn is_video_file(path: &Path) -> bool {
    let extensions = [
        "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", 
        "m4v", "3gp", "ogv", "ts", "mts", "m2ts",
        "divx", "vob", "rm", "rmvb",
    ];
    
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| extensions.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}


/// Formater le temps en format lisible (MM:SS ou HH:MM:SS)
pub fn format_time(ms: u64) -> String {
    let total_secs = ms / 1000;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    
    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{:02}:{:02}", minutes, seconds)
    }
}

/// Formater la taille en format lisible
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Formater le bitrate en format lisible
pub fn format_bitrate(bps: u64) -> String {
    if bps < 1000 {
        format!("{} bps", bps)
    } else if bps < 1_000_000 {
        format!("{:.1} kbps", bps as f64 / 1000.0)
    } else {
        format!("{:.1} Mbps", bps as f64 / 1_000_000.0)
    }
}



/// Obtenir la liste des fichiers vidéo dans un répertoire
pub fn get_video_files_in_dir(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_video_file(&path) {
                files.push(path);
            }
        }
    }
    
    files.sort();
    files
}

/// Écrit un WAV 32 bits flottant (aucune perte, aucun clip) depuis des
/// échantillons f32 entrelacés.
///
/// Écrit à la main plutôt qu'avec une dépendance : le format est trivial, et cela
/// évite d'ajouter une crate pour un outil de mesure (`--dump-audio`).
pub fn write_wav_f32(
    path: &std::path::Path,
    echantillons: &[f32],
    taux: u32,
    canaux: u16,
) -> Result<(), String> {
    use std::io::Write;

    let canaux = canaux.max(1);
    let bits = 32u16;
    let octets_par_echantillon = (bits / 8) as u32;
    let bloc_align = canaux as u32 * octets_par_echantillon;
    let donnees = (echantillons.len() as u32) * octets_par_echantillon;

    let fichier = std::fs::File::create(path).map_err(|e| format!("création: {}", e))?;
    let mut w = std::io::BufWriter::new(fichier);

    let mut entete: Vec<u8> = Vec::with_capacity(44);
    entete.extend_from_slice(b"RIFF");
    entete.extend_from_slice(&(36 + donnees).to_le_bytes());
    entete.extend_from_slice(b"WAVE");
    entete.extend_from_slice(b"fmt ");
    entete.extend_from_slice(&16u32.to_le_bytes());
    // 3 = IEEE float ; indispensable pour que les lecteurs interprètent les
    // octets comme des flottants et non comme des entiers 32 bits.
    entete.extend_from_slice(&3u16.to_le_bytes());
    entete.extend_from_slice(&canaux.to_le_bytes());
    entete.extend_from_slice(&taux.to_le_bytes());
    entete.extend_from_slice(&(taux * bloc_align).to_le_bytes());
    entete.extend_from_slice(&(bloc_align as u16).to_le_bytes());
    entete.extend_from_slice(&bits.to_le_bytes());
    entete.extend_from_slice(b"data");
    entete.extend_from_slice(&donnees.to_le_bytes());

    w.write_all(&entete).map_err(|e| format!("en-tête: {}", e))?;
    let mut octets: Vec<u8> = Vec::with_capacity(donnees as usize);
    for e in echantillons {
        octets.extend_from_slice(&e.to_le_bytes());
    }
    w.write_all(&octets).map_err(|e| format!("données: {}", e))?;
    w.flush().map_err(|e| format!("écriture: {}", e))?;
    Ok(())
}

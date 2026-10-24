//! KPlayer — Lecteur vidéo professionnel Linux
//! Qualité PotPlayer, 100% natif Rust + Vulkan
//! 
//! Architecture :
//!   - FFmpeg (libav) pour le décodage vidéo/audio
//!   - wgpu/Vulkan pour le rendu GPU (équivalent libplacebo)
//!   - egui pour l'interface utilisateur
//!   - cpal pour l'audio PipeWire/PulseAudio

mod ass;
mod config;
mod edid;
mod hdr;
mod instance;
mod mpris;
mod crash;
mod player;
mod pistes;
mod positions;
mod raccourcis;
mod x11;
mod renderer;
mod subtitles;
mod audio;
mod utils;
mod vignettes;
mod ui;
mod settings;
mod signets;
mod systeme;
mod tray;

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Rapport de crash : handler async-signal-safe (celui de Rust interbloque
    // sur SIGSEGV, ce qui gèle l'application au lieu de produire un rapport).
    crash::install();

    // Filet de sécurité : journalise TOUT panic dans un fichier, car un panic
    // sur le thread principal fait avorter le processus (signal 6) et le message
    // peut être perdu si stderr n'est pas consulté.
    std::panic::set_hook(Box::new(|info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        let msg = format!("=== PANIC ===\n{}\n\nBACKTRACE:\n{}\n", info, backtrace);
        eprintln!("{}", msg);
        let path = dirs::cache_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join("kplayer");
        let _ = std::fs::create_dir_all(&path);
        let _ = std::fs::write(path.join("panic.log"), &msg);
    }));

    // Initialisation du logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kplayer=info,wgpu=warn".into())
        )
        .init();

    // Option --fullscreen : démarrer directement en plein écran
    let start_fullscreen = std::env::args().any(|a| a == "--fullscreen" || a == "-f");

    // Option de test : provoque volontairement un SIGSEGV pour valider le
    // rapport de crash (notamment la pile de secours).
    if std::env::args().any(|a| a == "--crash-test") {
        tracing::warn!("--crash-test : déclenchement volontaire d'un SIGSEGV");
        unsafe {
            // read_volatile empêche le compilateur de remplacer l'accès par `ud2`
            // (il ne peut pas prouver que le pointeur est invalide) : on obtient
            // bien un vrai SIGSEGV.
            let p: *const u64 = std::ptr::null();
            let v = std::ptr::read_volatile(p);
            std::hint::black_box(v);
        }
    }

    // Option --no-hwdec : forcer le décodage logiciel (utile pour comparer, ou
    // pour contourner un pilote récalcitrant). Le réglage persistant reste
    // `video.hardware_decoding` dans la configuration.
    let force_software = std::env::args().any(|a| a == "--no-hwdec");

    // Charger les fichiers passés en argument CLI (KDE drag&drop ou "Ouvrir avec").
    // Une adresse de flux (http, HLS, RTSP…) est acceptée telle quelle : elle est
    // traitée comme un média à ouvrir, pas comme un chemin de fichier.
    // Les valeurs d'options ne sont pas des médias : on saute les arguments qui
    // commencent par « - » et la valeur de celles qui en attendent une. Sans cela,
    // `--speed 2` ou `--dump-images-pgm image.pgm` produisaient un avertissement
    // « fichier non supporté » à chaque lancement.
    let args_bruts: Vec<String> = std::env::args().skip(1).collect();
    let options_a_valeur = [
        "--seek",
        "--speed",
        "--profil",
        "--audio-track",
        "--sub-track",
        "--bench",
        "--bench-taille",
        "--ass-dump",
        "--dump-subs",
        "--dump-audio",
        "--dump-frame",
        "--dump-images-pgm",
        "--audio-buffer",
        "--vignette",
        "--vignette-pos",
        "--vignette-sortie",
        "--vignette-largeur",
        "--mpris-test",
        "--hdr-info",
        "--export-config",
        "--import-config",
        "--autostart",
    ];
    // **Options à deux valeurs** (`--dump-frame <pos> <image.png> <fichier>`,
    // `--vignette <pos> <image.png> <fichier>`) : le second argument est un chemin
    // de SORTIE. Il était pris pour un média dès que le fichier existait déjà —
    // c'est-à-dire à partir de la deuxième exécution (mesuré : une vignette de
    // 320x180 pour une source de 200x120, qui était en fait le PNG de la veille
    // relu comme média, et une position toujours à 0).
    // **Compté option par option**, et vérifié sur l'usage : `--dump-frame` et
    // `--vignette` prennent DEUX valeurs (position, fichier de sortie), mais
    // `--dump-images-pgm` n'en prend qu'UNE (le .pgm) — l'avoir classée « deux »
    // faisait sauter le chemin du média, donc la mesure des sous-titres image
    // (`verif-sousimages.sh`) ne produisait plus rien (régression de la s.34,
    // attrapée par la suite complète).
    let options_a_deux_valeurs = ["--dump-frame", "--vignette", "--boucle-ab"];
    let mut valeurs_options: Vec<usize> = Vec::new();
    for (i, a) in args_bruts.iter().enumerate() {
        if options_a_valeur.contains(&a.as_str()) {
            valeurs_options.push(i + 1);
        }
        if options_a_deux_valeurs.contains(&a.as_str()) {
            valeurs_options.push(i + 1);
            valeurs_options.push(i + 2);
        }
    }

    let cli_files: Vec<PathBuf> = args_bruts
        .iter()
        .enumerate()
        .filter_map(|(i, arg)| {
            if valeurs_options.contains(&i) || arg.starts_with('-') {
                return None;
            }
            let texte = arg.clone();
            if utils::is_stream_url(&texte) {
                return Some(PathBuf::from(texte));
            }
            let path = PathBuf::from(&texte);
            if path.is_file() {
                // **Pas de filtrage par extension.** Un fichier transmis par le
                // gestionnaire de fichiers a été choisi par l'utilisateur : refuser
                // `.mpg`, `.asf` ou `.m2v` au motif qu'ils ne figurent pas dans la
                // liste des extensions connues ferait échouer le double-clic sans
                // que rien ne s'ouvre. C'est FFmpeg qui décide ensuite (il sonde le
                // contenu) ; en cas d'échec, le lecteur affiche l'erreur du décodeur,
                // qui est actionnable — contrairement à un avertissement de liste.
                Some(path)
            } else {
                tracing::warn!("Fichier introuvable: {:?}", path);
                None
            }
        })
        .collect();

    // Chargement de la configuration utilisateur
    let mut config = config::Config::load()?;
    // Profils disponibles : chargés une fois, ils servent au choix explicite
    // (`--profil`), aux règles automatiques et à `--list-regles`.
    let profils_disponibles = config::load_profiles();

    let args: Vec<String> = std::env::args().collect();

    // --- Démarrage automatique : `--autostart on|off|status` ---
    if let Some(pos) = args.iter().position(|a| a == "--autostart") {
        match args.get(pos + 1).map(String::as_str) {
            Some(v @ ("on" | "off")) => match systeme::regler_autostart(v == "on") {
                Ok(()) => {
                    println!("démarrage automatique : {}", if v == "on" { "activé" } else { "désactivé" });
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("--autostart : {e}");
                    std::process::exit(1);
                }
            },
            Some("status") => {
                println!("démarrage automatique : {}", if systeme::autostart_actif() { "activé" } else { "désactivé" });
                std::process::exit(0);
            }
            _ => {
                eprintln!("usage : kplayer --autostart on|off|status");
                std::process::exit(2);
            }
        }
    }

    // --- Export / import de la configuration ---
    //
    // `--export-config <fichier>` écrit la configuration courante ; `--import-config <fichier>`
    // la remplace par un fichier validé (refusé sans rien écraser s'il n'est pas une
    // configuration de KPlayer). À lancer **lecteur fermé** : une fenêtre ouverte réécrirait
    // sa propre configuration à la fermeture.
    for (option, importer) in [("--export-config", false), ("--import-config", true)] {
        let Some(pos) = args.iter().position(|a| a == option) else { continue };
        let Some(fichier) = args.get(pos + 1).filter(|a| !a.starts_with('-')) else {
            eprintln!("{option} : indiquez un fichier");
            std::process::exit(2);
        };
        let fichier = PathBuf::from(fichier);
        let resultat = if importer {
            match config::config_path() {
                Some(destination) => config::importer_config(&fichier, &destination).map(|_| ()),
                None => Err("répertoire de configuration introuvable".to_string()),
            }
        } else {
            config::exporter_config(&config, &fichier)
        };
        match resultat {
            Ok(()) => {
                println!(
                    "configuration {} : {}",
                    if importer { "importée depuis" } else { "exportée vers" },
                    fichier.display()
                );
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("{option} refusé : {e}");
                std::process::exit(1);
            }
        }
    }

    // Option --speed <facteur> : vitesse de départ (0,1 à 8). Utile pour mesurer la
    // correction de hauteur sans passer par l'interface.
    // Options de choix de piste (vérification et usage direct en ligne de commande).
    // Application d'un profil avant tout chargement : c'est le cas d'usage réel
    // (un raccourci par type de contenu).
    let mut profil_demande: Option<String> = None;
    if let Some(pos) = args.iter().position(|a| a == "--profil") {
        if let Some(nom) = args.get(pos + 1) {
            match profils_disponibles.iter().find(|p| &p.name == nom) {
                Some(p) => {
                    p.appliquer(&mut config);
                    tracing::info!("Profil appliqué : « {} » — {}", p.name, p.resume());
                    profil_demande = Some(p.name.clone());
                }
                None => {
                    eprintln!(
                        "profil « {} » introuvable (profils : {})",
                        nom,
                        profils_disponibles
                            .iter()
                            .map(|p| p.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    std::process::exit(2);
                }
            }
        }
    }
    let _ = &profil_demande;

    // Démarrage à une position précise (`--seek 90` ou `--seek 1:30`), en secondes.
    // Utile pour reprendre un film, et indispensable aux bancs d'essai : sans cela, on
    // ne peut pas capturer un instant choisi (les captures de vérification étaient
    // toutes prises au même moment).
    let position_depart: Option<u64> = args
        .iter()
        .position(|a| a == "--seek")
        .and_then(|p| args.get(p + 1))
        .and_then(|v| {
            let parties: Vec<&str> = v.split(':').collect();
            let mut total = 0u64;
            for (i, p) in parties.iter().enumerate() {
                let n: u64 = p.trim().parse().ok()?;
                total += n * 60u64.pow((parties.len() - 1 - i) as u32);
            }
            Some(total * 1000)
        });

    // `--boucle-ab <A> <B>` (secondes, comme `--seek`) : répétition A-B active dès l'ouverture.
    let boucle_ab_depart: Option<(u64, u64)> = args
        .iter()
        .position(|a| a == "--boucle-ab")
        .and_then(|p| {
            let a: f64 = args.get(p + 1)?.parse().ok()?;
            let b: f64 = args.get(p + 2)?.parse().ok()?;
            Some(((a * 1000.0) as u64, (b * 1000.0) as u64))
        });

    let piste_audio: Option<i32> = args
        .iter()
        .position(|a| a == "--audio-track")
        .and_then(|p| args.get(p + 1))
        .and_then(|v| v.parse().ok());
    let piste_subs: Option<i32> = args
        .iter()
        .position(|a| a == "--sub-track")
        .and_then(|p| args.get(p + 1))
        .and_then(|v| v.parse().ok());

    let vitesse_depart: Option<f32> = args
        .iter()
        .position(|a| a == "--speed")
        .and_then(|p| args.get(p + 1))
        .and_then(|v| v.parse::<f32>().ok());

    // Profondeur du tampon audio (`--audio-buffer 400`), en millisecondes.
    // Sert à la vérification : avec l'horloge audio maître, la profondeur du
    // tampon ne doit AUCUN effet sur la synchronisation image/son (voir
    // tools/verif-horloge.sh). Valeur par défaut : `audio::CIBLE_TAMPON_MS`.
    let tampon_audio: Option<u64> = args
        .iter()
        .position(|a| a == "--audio-buffer")
        .and_then(|p| args.get(p + 1))
        .and_then(|v| v.parse::<u64>().ok());

    // Taille d'affichage simulée, partagée par `--bench` et `--dump-frame` : elle
    // permet de comparer **les pixels** produits par deux chemins de rendu.
    let taille_simulee = args
        .iter()
        .position(|a| a == "--bench-taille")
        .and_then(|p| args.get(p + 1))
        .and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))
        })
        .unwrap_or((0, 0));

    // --- Option de mesure : --bench <nombre_images> ---
    //
    // Décode N images sans affichage et donne le débit réel. C'est ainsi qu'on
    // chiffre le coût d'un filtre : pendant la lecture, le rendu logiciel de
    // l'écran virtuel domine complètement le temps CPU mesuré.
    if let Some(pos) = args.iter().position(|a| a == "--bench") {
        let images: usize = args.get(pos + 1).and_then(|v| v.parse().ok()).unwrap_or(200);
        let Some(media) = cli_files.first() else {
            eprintln!("--bench : aucun fichier média fourni");
            std::process::exit(2);
        };
        let mut player = player::VideoPlayer::new(config.clone());
        if force_software {
            player.set_hardware_decoding(false);
        }
        if let Err(e) = player.load_file(media.clone()) {
            eprintln!("--bench : chargement impossible : {}", e);
            std::process::exit(1);
        }
        // La vitesse s'applique APRÈS le chargement : avant, aucun flux audio
        // n'existe et le rééchantillonnage (ou `atempo`) n'aurait rien à configurer.
        if let Some(v) = vitesse_depart {
            player.set_speed(v);
        }
        // `--bench-taille WxH` mesure comme si l'image était affichée à cette
        // taille : c'est le seul moyen de chiffrer la réduction (et donc l'échelle
        // matérielle) sans ouvrir de fenêtre.
        let (secondes, compte) = player.bench_decode_taille(images, taille_simulee);
        if compte == 0 {
            eprintln!("--bench : aucune image décodée");
            std::process::exit(1);
        }
        let filtres = player.filters_description();
        println!(
            "{} image(s) en {:.3} s → {:.1} i/s ({:.2} ms/image) — filtres : {}",
            compte,
            secondes,
            compte as f64 / secondes,
            secondes * 1000.0 / compte as f64,
            if filtres.is_empty() { "aucun" } else { filtres }
        );
        if let Some((entrees, sorties)) = player.filter_frame_counts() {
            println!("graphe : {} entrée(s) / {} sortie(s)", entrees, sorties);
        }
        let _ = player.quit();
        std::process::exit(0);
    }

    // --- Option de mesure : --ass-dump <fichier.ass> ---
    //
    // Affiche l'analyse d'un fichier ASS telle que le lecteur la comprend : styles,
    // résolution de référence, événements, position, alignement, couleurs et taille.
    // Permet de vérifier la mise en page SANS écran (les tests unitaires du module
    // couvrent les cas de base ; ici, c'est le fichier réel de l'utilisateur).
    if let Some(pos) = args.iter().position(|a| a == "--ass-dump") {
        let Some(chemin) = args.get(pos + 1) else {
            eprintln!("usage : kplayer --ass-dump <fichier.ass>");
            std::process::exit(2);
        };
        let contenu = match std::fs::read_to_string(chemin) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("lecture impossible : {}", e);
                std::process::exit(1);
            }
        };
        let script = ass::analyser(&contenu);
        println!(
            "référence : {}×{} — {} style(s), {} événement(s)",
            script.play_res.0,
            script.play_res.1,
            script.styles.len(),
            script.events.len()
        );
        for (nom, st) in &script.styles {
            println!(
                "style « {} » : police {} {:.0}, couleur #{:02X}{:02X}{:02X}, contour {:.1}, \
                 alignement {}, marges {:.0}/{:.0}/{:.0}{}",
                nom,
                st.fontname,
                st.fontsize,
                st.primary[0],
                st.primary[1],
                st.primary[2],
                st.outline,
                st.alignment,
                st.margin_l,
                st.margin_r,
                st.margin_v,
                if st.bold { ", gras" } else { "" }
            );
        }
        for e in &script.events {
            let pos_txt = match e.pos {
                Some((x, y)) => format!("pos({:.0},{:.0})", x, y),
                None => "marges".to_string(),
            };
            let couleurs: Vec<String> = e
                .runs
                .iter()
                .map(|r| format!("#{:02X}{:02X}{:02X}", r.primary[0], r.primary[1], r.primary[2]))
                .collect();
            let anim = if let Some(m) = &e.mouvement {
                let t = if m.t2 == 0 {
                    "tout l'événement".to_string()
                } else {
                    format!("{}–{} ms", m.t1, m.t2)
                };
                format!(
                    " move({:.0},{:.0}→{:.0},{:.0} sur {})",
                    m.de.0, m.de.1, m.vers.0, m.vers.1, t
                )
            } else {
                String::new()
            };
            let transfo = if e.transformations.is_empty() {
                String::new()
            } else {
                let d: Vec<String> = e
                    .transformations
                    .iter()
                    .map(|t| {
                        let mut p = Vec::new();
                        if let Some(v) = t.fontsize {
                            p.push(format!("fs{:.0}", v));
                        }
                        if let Some(c) = t.primary {
                            p.push(format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]));
                        }
                        format!("t({},{},{})", t.t1, t.t2, p.join("+"))
                    })
                    .collect();
                format!(" [{}]", d.join(" "))
            };
            let kara = {
                let syllabes: Vec<String> = e
                    .runs
                    .iter()
                    .filter(|r| r.karaoke_dur_ms > 0)
                    .map(|r| {
                        format!(
                            "{}:{}+{}{}",
                            r.text.trim(),
                            r.karaoke_start_ms,
                            r.karaoke_dur_ms,
                            if r.karaoke_sweep { "f" } else { "" }
                        )
                    })
                    .collect();
                if syllabes.is_empty() {
                    String::new()
                } else {
                    format!(" karaoké[{}]", syllabes.join(" "))
                }
            };
            println!(
                "[{} → {}] couche {} style « {} » {} alignement {}{}{}{}{} : « {} » [{}]{}",
                e.start_ms,
                e.end_ms,
                e.layer,
                e.style_name,
                pos_txt,
                e.alignment,
                if e.fade_in_ms > 0 || e.fade_out_ms > 0 {
                    format!(" fondu {}/{}", e.fade_in_ms, e.fade_out_ms)
                } else {
                    String::new()
                },
                anim,
                transfo,
                kara,
                e.texte().replace('\n', " ⏎ "),
                couleurs.join(" "),
                if e.drawing { " (dessin ignoré)" } else { "" }
            );
        }
        std::process::exit(0);
    }

    // --- Profils de réglages ---
    //
    // `--list-profiles` et `--profil <nom>` rendent les profils vérifiables sans
    // passer par l'interface : on peut mesurer qu'un profil change réellement le rendu.
    if args.iter().any(|a| a == "--list-profiles") {
        let profils = config::load_profiles();
        if profils.is_empty() {
            println!("aucun profil enregistré");
        }
        for p in &profils {
            println!("{} — {}", p.name, p.resume());
        }
        std::process::exit(0);
    }

    // --- Règles de profil automatiques ---
    //
    // `--list-regles` rend l'automatisation vérifiable sans écran : quelles règles
    // existent, laquelle désigne un profil présent, laquelle est orpheline.
    if args.iter().any(|a| a == "--list-regles") {
        if config.regles.is_empty() {
            println!("aucune règle");
        }
        for (i, regle) in config.regles.iter().enumerate() {
            let etat = if profils_disponibles.iter().any(|p| p.name == regle.profil) {
                "profil présent"
            } else {
                "profil ABSENT — règle ignorée"
            };
            println!(
                "règle {} : {} → « {} » ({})",
                i + 1,
                regle.resume(),
                regle.profil,
                etat
            );
        }
        if profils_disponibles.is_empty() {
            println!("aucun profil enregistré");
        }
        std::process::exit(0);
    }

    // --- Option de mesure : --detect-bars ---
    //
    // Affiche les bandes noires détectées (haut, bas, gauche, droite) en pixels. Sert
    // au banc d'essai du recadrage : la détection doit être vérifiable sans écran.
    if args.iter().any(|a| a == "--detect-bars") {
        let Some(media) = cli_files.first() else {
            eprintln!("--detect-bars : aucun fichier média fourni");
            std::process::exit(2);
        };
        let mut player = player::VideoPlayer::new(config.clone());
        if force_software {
            player.set_hardware_decoding(false);
        }
        if let Err(e) = player.load_file(media.clone()) {
            eprintln!("--detect-bars : chargement impossible : {}", e);
            std::process::exit(1);
        }
        let (h, b, g, d) = player.detect_black_bars(5);
        println!("bandes noires : haut={} bas={} gauche={} droite={}", h, b, g, d);
        let _ = player.quit();
        std::process::exit(0);
    }

    // --- Option d'inventaire : --list-tracks ---
    //
    // Liste les pistes du conteneur : c'est ce que l'interface affiche dans ses
    // menus, et ce que le banc d'essai compare au contenu réel du fichier.
    if args.iter().any(|a| a == "--list-tracks") {
        let Some(media) = cli_files.first() else {
            eprintln!("--list-tracks : aucun fichier média fourni");
            std::process::exit(2);
        };
        let mut player = player::VideoPlayer::new(config.clone());
        if let Err(e) = player.load_file(media.clone()) {
            eprintln!("--list-tracks : chargement impossible : {}", e);
            std::process::exit(1);
        }
        if let Some(info) = player.media_info() {
            println!("vidéo : {} ({}x{}, {:.2} i/s)", info.video_codec, info.resolution.0, info.resolution.1, info.fps);
            if let Some((nom, materiel)) = player.video_decoder() {
                println!("décodeur : {} ({})", nom, if materiel { "matériel" } else { "logiciel" });
            }
            for (i, p) in info.audio_tracks.iter().enumerate() {
                let actif = if player.current_audio_stream() == Some(p.index) { "*" } else { " " };
                println!("{} audio[{}] flux {} : {}", actif, i, p.index, p.label());
            }
            for (i, c) in info.chapters.iter().enumerate() {
                println!(
                    "  chapitre[{}] {} → {} : {}",
                    i,
                    c.start_ms,
                    c.end_ms,
                    if c.title.is_empty() { "(sans titre)" } else { &c.title }
                );
            }
            for (i, p) in info.subtitle_tracks.iter().enumerate() {
                let actif = if player.current_subtitle_stream() == Some(p.index) { "*" } else { " " };
                println!("{} sous-titres[{}] flux {} : {}", actif, i, p.index, p.label());
            }
        }
        let _ = player.quit();
        std::process::exit(0);
    }

    // --- Option de mesure : --dump-subs <fichier.txt> ---
    //
    // Extrait les sous-titres embarqués avec leurs minutages. Permet de vérifier le
    // texte ET les temps sans passer par l'affichage.
    if let Some(pos) = args.iter().position(|a| a == "--dump-subs") {
        let Some(sortie) = args.get(pos + 1).map(PathBuf::from) else {
            eprintln!("usage : kplayer --dump-subs <fichier.txt> <fichier_média>");
            std::process::exit(2);
        };
        let Some(media) = cli_files.first() else {
            eprintln!("--dump-subs : aucun fichier média fourni");
            std::process::exit(2);
        };
        let mut player = player::VideoPlayer::new(config.clone());
        if let Err(e) = player.load_file(media.clone()) {
            eprintln!("--dump-subs : chargement impossible : {}", e);
            std::process::exit(1);
        }
        if let Some(index) = piste_subs {
            if let Err(e) = player.select_subtitle_stream(index) {
                eprintln!("--dump-subs : {}", e);
                std::process::exit(1);
            }
        }
        let cues = player.dump_subtitles();
        let mut texte = String::new();
        for c in &cues {
            texte.push_str(&format!("{} --> {}\n{}\n\n", c.start_ms, c.end_ms, c.text));
        }
        match std::fs::write(&sortie, &texte) {
            Ok(()) => {
                println!(
                    "{} sous-titre(s) écrit(s) dans {}",
                    cues.len(),
                    sortie.display()
                );
                let _ = player.quit();
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("--dump-subs : écriture impossible : {}", e);
                std::process::exit(1);
            }
        }
    }

    // --- Option de mesure : --dump-images [--dump-images-pgm fichier.pgm] ---
    //
    // Décode la piste de sous-titres **image** (PGS, DVD) et imprime chaque
    // événement : minutage, rectangles, et une empreinte des pixels. C'est ce qui
    // permet de vérifier les sous-titres image SANS écran — l'équivalent de
    // `--dump-subs` pour le texte. Avec un chemin `.pgm`, le premier rectangle
    // est aussi écrit en niveaux de gris, pour inspection visuelle.
    if args.iter().any(|a| a == "--dump-images") {
        // Chemin PGM **séparé** : une option qui prendrait « l'argument suivant »
        // finirait par avaler le nom du média (défaut constaté : l'image écrasait
        // le fichier vidéo passé en argument).
        let sortie: Option<PathBuf> = args
            .iter()
            .position(|a| a == "--dump-images-pgm")
            .and_then(|p| args.get(p + 1))
            .map(PathBuf::from);
        let Some(media) = cli_files.first() else {
            eprintln!("--dump-images : aucun fichier média fourni");
            std::process::exit(2);
        };
        let mut player = player::VideoPlayer::new(config.clone());
        if let Err(e) = player.load_file(media.clone()) {
            eprintln!("--dump-images : chargement impossible : {}", e);
            std::process::exit(1);
        }
        if let Some(index) = piste_subs {
            if let Err(e) = player.select_subtitle_stream(index) {
                eprintln!("--dump-images : {}", e);
                std::process::exit(1);
            }
        }
        let cues = player.dump_bitmap_cues();
        let mut premiers: Option<(u32, u32, Vec<u8>)> = None;
        for c in &cues {
            println!("{} --> {}", c.start_ms, c.end_ms);
            for r in &c.rects {
                // Empreinte FNV-1a des octets RGBA : deux rendus identiques ont la
                // même empreinte, une différence de pixel la change.
                let mut h: u64 = 0xcbf2_9ce4_8422_2325;
                for b in &r.rgba {
                    h ^= *b as u64;
                    h = h.wrapping_mul(0x100_0000_01b3);
                }
                let opaques = r
                    .rgba
                    .chunks_exact(4)
                    .filter(|p| p[3] > 0)
                    .count();
                println!(
                    "  {}x{} en ({},{}) — {} pixels opaques — empreinte {:016x}",
                    r.width, r.height, r.x, r.y, opaques, h
                );
                if premiers.is_none() && opaques > 0 {
                    premiers = Some((r.width, r.height, r.rgba.clone()));
                }
            }
        }
        println!("{} événement(s) image", cues.len());
        if let (Some(chemin), Some((w, h, rgba))) = (sortie, premiers) {
            // PGM (P5) : un octet par pixel, alpha appliqué à la luminance.
            let mut pgm = format!("P5\n{} {}\n255\n", w, h).into_bytes();
            for p in rgba.chunks_exact(4) {
                let lum = ((p[0] as u32 * 30 + p[1] as u32 * 59 + p[2] as u32 * 11) / 100) as u8;
                pgm.push(((lum as u32 * p[3] as u32) / 255) as u8);
            }
            match std::fs::write(&chemin, &pgm) {
                Ok(()) => println!("image du 1er rectangle écrite : {}", chemin.display()),
                Err(e) => {
                    eprintln!("--dump-images : écriture impossible : {}", e);
                    std::process::exit(1);
                }
            }
        }
        let _ = player.quit();
        std::process::exit(0);
    }

    // --- Option de mesure : --dump-audio <fichier.wav> ---
    //
    // Décode tout l'audio du média, égaliseur compris, vers un WAV : c'est ce qui
    // permet de vérifier une courbe d'égalisation par mesure (transformée de
    // Fourier) plutôt que par une écoute subjective.
    if let Some(pos) = args.iter().position(|a| a == "--dump-audio") {
        let sortie = match args.get(pos + 1) {
            Some(p) => PathBuf::from(p),
            None => {
                eprintln!("usage : kplayer --dump-audio <fichier.wav> <fichier_média>");
                std::process::exit(2);
            }
        };
        let Some(media) = cli_files.first() else {
            eprintln!("--dump-audio : aucun fichier média fourni");
            std::process::exit(2);
        };
        let mut player = player::VideoPlayer::new(config.clone());
        if force_software {
            player.set_hardware_decoding(false);
        }
        if let Err(e) = player.load_file(media.clone()) {
            eprintln!("--dump-audio : chargement impossible : {}", e);
            std::process::exit(1);
        }
        if let Some(index) = piste_audio {
            if let Err(e) = player.select_audio_stream(index) {
                eprintln!("--dump-audio : {}", e);
                std::process::exit(1);
            }
        }
        // La vitesse s'applique APRÈS le chargement : avant, aucun flux audio
        // n'existe et le rééchantillonnage (ou `atempo`) n'aurait rien à configurer.
        if let Some(v) = vitesse_depart {
            player.set_speed(v);
        }
        // Règle de profil : un `--profil` explicite l'emporte (choix de l'utilisateur).
        if profil_demande.is_none() {
            if let Some(resolution) = player.media_info().map(|i| i.resolution) {
                if let Some(regle) = appliquer_regles_sans_interface(
                    &mut player,
                    &mut config,
                    &profils_disponibles,
                    media,
                    resolution,
                ) {
                    println!(
                        "règle de profil appliquée : {} → « {} »",
                        regle.resume(),
                        regle.profil
                    );
                }
            }
        }
        match player.dump_audio() {
            Some((echantillons, taux, canaux)) => {
                match utils::write_wav_f32(&sortie, &echantillons, taux, canaux) {
                    Ok(()) => {
                        println!(
                            "audio écrit: {} ({} échantillons, {} Hz, {} canaux)",
                            sortie.display(),
                            echantillons.len() / canaux.max(1) as usize,
                            taux,
                            canaux
                        );
                        let desc = player.audio_filters_description();
                        println!(
                            "égaliseur : {}",
                            if desc.is_empty() { "aucun (signal d'origine)" } else { desc }
                        );
                        if let Some((entrees, sorties)) = player.audio_filter_counts() {
                            println!("graphe audio : {} éch. entrés / {} sortis", entrees, sorties);
                        }
                        let _ = player.quit();
                        std::process::exit(0);
                    }
                    Err(e) => {
                        eprintln!("--dump-audio : écriture impossible : {}", e);
                        std::process::exit(1);
                    }
                }
            }
            None => {
                eprintln!("--dump-audio : aucun échantillon audio décodé");
                std::process::exit(1);
            }
        }
    }

    // --- Option de diagnostic : --dump-frame <position_ms> <fichier.png> ---
    //
    // Décode l'image à la position demandée (chaîne de filtres et conversion
    // comprises), à la résolution NATIVE du média, et l'écrit en PNG **sans
    // ouvrir de fenêtre**. C'est ce qui rend les filtres vérifiables : une
    // capture d'écran dépend de la taille de la fenêtre et du rééchantillonnage.
    // Utilisé par tools/verif-filtres.sh.
    // --vignette <position_ms> <image.png> <fichier> [--vignette-largeur N]
    //
    // Chemin **sans interface** du module de vignettes : il permet de vérifier le
    // décodeur autonome seul (`tools/verif-vignettes.sh`), avant toute intégration
    // dans le panneau. C'est la règle du projet pour un nouveau module : le tester
    // isolément d'abord, puis après son branchement.
    // --hdr-info <fichier> : métadonnées HDR du flux et étage qui serait inséré.
    //
    // Chemin de vérification du module HDR **sans interface ni décodage** : le banc
    // (`tools/verif-hdr.sh`) lit ces lignes pour contrôler la détection, et
    // `--dump-frame` mesure ensuite les pixels réellement produits.
    if let Some(pos) = args_bruts.iter().position(|a| a == "--hdr-info") {
        let Some(chemin) = args_bruts.get(pos + 1).map(PathBuf::from) else {
            eprintln!("usage : kplayer --hdr-info <fichier>");
            std::process::exit(2);
        };
        match hdr::inspecter_fichier(&chemin) {
            Ok(info) => {
                let contexte = hdr::contexte();
                let plan = hdr::plan_avec(Some(&info), &config.video.hdr, &contexte);
                println!("média: {}", chemin.display());
                println!("info: {}", info.resume());
                println!("ecran: {}", contexte.resume());
                println!("ecran_hdr: {}", if contexte.ecran_hdr() { "oui" } else { "non" });
                println!(
                    "passthrough: {}",
                    if contexte.passthrough_possible() { "oui" } else { "non" }
                );
                println!("hdr: {}", if info.est_hdr() { "oui" } else { "non" });
                println!("transfert: {}", info.transfert.libelle());
                println!("primaires: {}", info.primaires.libelle());
                println!("bits: {}", info.bits);
                println!("moteur: {}", plan.moteur.libelle());
                println!("etage: {}", plan.etage);
                println!("explication: {}", plan.explication);
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("--hdr-info : {}", e);
                std::process::exit(1);
            }
        }
    }

    // --mpris-test [secondes] : le module MPRIS **seul**, sans fenêtre ni média.
    //
    // C'est le chemin de vérification du module avant son intégration : le serveur
    // publie un état connu, imprime chaque commande reçue (`commande: …`) et fait
    // varier son statut pour qu'un observateur puisse constater les
    // `PropertiesChanged`. Aucune interface graphique, aucun décodeur : ce qui est
    // mesuré est bien le module, pas le reste de l'application.
    if args_bruts.iter().any(|a| a == "--mpris-test") {
        let duree: u64 = args_bruts
            .iter()
            .position(|a| a == "--mpris-test")
            .and_then(|p| args_bruts.get(p + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(20);
        let serveur = match mpris::demarrer("KPlayer") {
            mpris::Demarrage::Actif(s) => s,
            mpris::Demarrage::Indisponible(raison) => {
                eprintln!("--mpris-test : bus indisponible : {}", raison);
                std::process::exit(3);
            }
        };
        use std::io::Write;
        let mut etat = mpris::Etat {
            lecture: mpris::Lecture::Lecture,
            // Position de départ non nulle : une position à 0 ne prouverait rien
            // (elle vaut 0 aussi quand la conversion est cassée).
            position_ms: 12_000,
            duree_ms: 60_000,
            titre: "Mire de test MPRIS".to_string(),
            artiste: "KPlayer".to_string(),
            rang: 0,
            volume: 0.8,
            aleatoire: false,
            boucle: false,
            vitesse: 1.0,
            peut_precedent: true,
            peut_suivant: true,
        };
        // **Publier avant d'annoncer** : le banc (ou un bureau) peut interroger dès
        // la première ligne ; il verrait sinon l'état par défaut (`Stopped`, titre
        // vide) et conclurait à un module cassé. Constaté en écrivant le banc.
        serveur.publier(etat.clone());
        println!("mpris pret: {}", serveur.nom());
        let _ = std::io::stdout().flush();
        let debut = std::time::Instant::now();
        let mut dernier_battement = debut;
        let mut cycle = 0u32;
        while debut.elapsed().as_secs() < duree {
            for c in serveur.commandes() {
                // Trace lisible ET exploitable : le banc compare cette ligne.
                println!("commande: {:?}", c);
                let _ = std::io::stdout().flush();
            }
            // La position avance, le statut change toutes les 2 s : l'observateur
            // doit voir un `PropertiesChanged` sans avoir à provoquer un appel.
            if dernier_battement.elapsed() >= std::time::Duration::from_millis(500) {
                dernier_battement = std::time::Instant::now();
                etat.position_ms = (etat.position_ms + 500).min(etat.duree_ms);
                serveur.publier(etat.clone());
            }
            if debut.elapsed().as_secs() % 2 == 0 && debut.elapsed().as_secs() != cycle as u64 {
                cycle = debut.elapsed().as_secs() as u32;
                etat.lecture = if cycle % 4 == 0 {
                    mpris::Lecture::Pause
                } else {
                    mpris::Lecture::Lecture
                };
                serveur.publier(etat.clone());
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        println!("mpris fin");
        std::process::exit(0);
    }

    if let Some(pos) = args.iter().position(|a| a == "--vignette") {
        let position_ms: u64 = args.get(pos + 1).and_then(|v| v.parse().ok()).unwrap_or(0);
        let sortie = match args.get(pos + 2) {
            Some(p) => PathBuf::from(p),
            None => {
                eprintln!("usage : kplayer --vignette <position_ms> <image.png> <fichier> [--vignette-largeur N]");
                std::process::exit(2);
            }
        };
        let largeur = args
            .iter()
            .position(|a| a == "--vignette-largeur")
            .and_then(|p| args.get(p + 1))
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(vignettes::LARGEUR_DEFAUT);
        let Some(media) = cli_files.first() else {
            eprintln!("--vignette : aucun fichier média fourni");
            std::process::exit(2);
        };
        let debut = std::time::Instant::now();
        match vignettes::extraire(media, position_ms, largeur) {
            Ok(v) => match ui::save_png(&sortie, v.largeur, v.hauteur, &v.rgba) {
                Ok(()) => {
                    println!(
                        "vignette écrite: {} ({}x{}, position réelle {} ms, demandée {} ms, {} ms)",
                        sortie.display(),
                        v.largeur,
                        v.hauteur,
                        v.position_ms,
                        position_ms,
                        debut.elapsed().as_millis()
                    );
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("--vignette : écriture impossible : {}", e);
                    std::process::exit(1);
                }
            },
            Err(e) => {
                eprintln!("--vignette : {}", e);
                std::process::exit(1);
            }
        }
    }

    if let Some(pos) = args.iter().position(|a| a == "--dump-frame") {
        let position_ms: u64 = args.get(pos + 1).and_then(|v| v.parse().ok()).unwrap_or(0);
        let sortie = match args.get(pos + 2) {
            Some(p) => PathBuf::from(p),
            None => {
                eprintln!("usage : kplayer --dump-frame <position_ms> <image.png> <fichier>");
                std::process::exit(2);
            }
        };
        let Some(media) = cli_files.first() else {
            eprintln!("--dump-frame : aucun fichier média fourni");
            std::process::exit(2);
        };
        let mut player = player::VideoPlayer::new(config.clone());
        if force_software {
            player.set_hardware_decoding(false);
        }
        if let Err(e) = player.load_file(media.clone()) {
            eprintln!("--dump-frame : chargement impossible : {}", e);
            std::process::exit(1);
        }
        // La vitesse s'applique APRÈS le chargement : avant, aucun flux audio
        // n'existe et le rééchantillonnage (ou `atempo`) n'aurait rien à configurer.
        if let Some(v) = vitesse_depart {
            player.set_speed(v);
        }
        if taille_simulee.0 >= 16 && taille_simulee.1 >= 16 {
            player.set_dump_taille(taille_simulee.0, taille_simulee.1);
        }
        // Règle de profil (même logique que l'interface et que `--dump-audio`).
        if profil_demande.is_none() {
            if let Some(resolution) = player.media_info().map(|i| i.resolution) {
                if let Some(regle) = appliquer_regles_sans_interface(
                    &mut player,
                    &mut config,
                    &profils_disponibles,
                    media,
                    resolution,
                ) {
                    println!(
                        "règle de profil appliquée : {} → « {} »",
                        regle.resume(),
                        regle.profil
                    );
                }
            }
        }
        match player.dump_frame_at(position_ms) {
            Some(frame) => {
                match ui::save_png(&sortie, frame.width, frame.height, &frame.data) {
                    Ok(()) => {
                        println!(
                            "image écrite: {} ({}x{}, PTS {} ms, filtres « {} »)",
                            sortie.display(),
                            frame.width,
                            frame.height,
                            frame.pts_ms,
                            player.filters_description()
                        );
                        if let Some((entrees, sorties)) = player.filter_frame_counts() {
                            println!("graphe de filtres : {} entrée(s), {} sortie(s)", entrees, sorties);
                        }
                        let _ = player.quit();
                        std::process::exit(0);
                    }
                    Err(e) => {
                        eprintln!("--dump-frame : écriture impossible : {}", e);
                        std::process::exit(1);
                    }
                }
            }
            None => {
                eprintln!("--dump-frame : aucune image décodée à {} ms", position_ms);
                std::process::exit(1);
            }
        }
    }

    // Instance unique : un fichier ouvert depuis le gestionnaire de fichiers doit
    // arriver dans la fenêtre déjà ouverte, pas en ouvrir une seconde.
    //
    // **Quand on ne le fait pas** : sans média à transmettre (lancement depuis le
    // menu), en mode mesure (`--dump-frame`, `--bench`… : ces chemins n'ouvrent
    // aucune fenêtre et doivent s'exécuter même si une instance tourne), avec
    // `--nouvelle-instance`, ou si la mécanique est refusée par l'environnement.
    let options_sans_instance = [
        "--dump-frame",
        "--dump-audio",
        "--dump-subs",
        "--dump-images-pgm",
        "--dump-images",
        "--ass-dump",
        "--bench",
        "--list-profiles",
        "--list-regles",
        "--export-config",
        "--import-config",
        "--crash-test",
    ];
    let mode_mesure = args_bruts.iter().any(|a| options_sans_instance.contains(&a.as_str()));
    let nouvelle_instance = args_bruts.iter().any(|a| a == "--nouvelle-instance");
    let instance = if mode_mesure || cli_files.is_empty() {
        instance::Demarrage::Desactive("mode mesure ou aucun média")
    } else {
        instance::Instance::demarrer(&cli_files, nouvelle_instance)
    };
    let mut instance = match instance {
        instance::Demarrage::Transfere { destinataire } => {
            // L'autre instance a pris les fichiers : ce processus n'a plus rien à
            // faire. Sortie 0 — le gestionnaire de fichiers ne doit pas signaler
            // d'erreur, l'ouverture a réussi.
            tracing::info!(
                "Fichier(s) transmis à l'instance déjà ouverte ({}) — ce processus se termine",
                destinataire.display()
            );
            return Ok(());
        }
        instance::Demarrage::Unique(i) => Some(i),
        instance::Demarrage::Desactive(raison) => {
            tracing::debug!("instance unique désactivée : {}", raison);
            None
        }
    };

    // Configuration de l'application egui
    //
    // Géométrie : on reprend la dernière connue (session 32). C'est une mémoire
    // d'usage — la fenêtre doit se rouvrir là où on l'avait laissée, à la même
    // taille, et agrandie si elle l'était. Sans valeur enregistrée, on garde le
    // 1280×720 d'avant et le placement du gestionnaire de fenêtres.
    //
    // La taille et l'état agrandi se restituent partout ; la **position** seulement
    // sous X11. Sous Wayland, le compositeur place la fenêtre et le client n'a pas
    // voix au chapitre : demander une position serait ignoré (au mieux) et donnerait
    // l'illusion d'un réglage appliqué. Pour une position fixe sous Wayland, c'est
    // une règle de fenêtre du compositeur qui s'en charge — `tools/regle-fenetre-kwin.sh`.
    let wayland = config::session_est_wayland();
    let (taille_enregistree, position_enregistree) = config.interface.geometrie_de_demarrage();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(taille_enregistree.unwrap_or([1280.0, 720.0]))
        .with_min_inner_size(crate::config::TAILLE_MIN_FENETRE)
        .with_title("KPlayer — Lecteur Vidéo");
    if let Some(position) = position_enregistree.filter(|_| !wayland) {
        viewport = viewport.with_position(position);
    }
    if wayland && position_enregistree.is_some() {
        tracing::info!(
            "Session Wayland : position enregistrée ignorée ({:.0},{:.0}) — le \
             compositeur décide du placement (taille et état agrandi, eux, sont restitués)",
            position_enregistree.unwrap_or([0.0, 0.0])[0],
            position_enregistree.unwrap_or([0.0, 0.0])[1]
        );
    }
    // `--fullscreen` l'emporte : restaurer une fenêtre agrandie en plus du plein
    // écran ferait clignoter la fenêtre puis laisserait un état incohérent.
    if config.interface.window_maximized && !start_fullscreen {
        viewport = viewport.with_maximized(true);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    // Priorité du processus (préférences › Lecture) : posée avant la création de la fenêtre,
    // pour que les fils créés ensuite en héritent.
    match systeme::appliquer_priorite(config.playback.priority) {
        Ok(()) if config.playback.priority != systeme::Priorite::Normale => {
            tracing::info!("Priorité du processus : {}", config.playback.priority.label());
        }
        Ok(()) => {}
        Err(e) => tracing::warn!("{e}"),
    }

    // Lancer l'application eframe avec les fichiers CLI
    eframe::run_native(
        "KPlayer",
        options,
        Box::new(move |cc| {
            // Police : egui ne dessine que ce que ses polices contiennent. Or la
            // police par défaut ne fournit NI les flèches (← ↑ → ↓), NI `●`, `✕`,
            // `⏯` — ils s'affichaient donc en carrés vides, y compris dans l'aide
            // des raccourcis (« ← →  Reculer / avancer de 5 s »). On ajoute une
            // police système en secours ; à défaut, `ui::police_de_secours`
            // remplace les caractères non dessinables par des équivalents sûrs.
            ui::installer_police_de_secours(&cc.egui_ctx);
            // Police des sous-titres : installée dès le démarrage si le réglage la
            // demande (sinon le texte garde la police par défaut du lecteur).
            ui::installer_police_sous_titre(&cc.egui_ctx, &config.subtitles.font_family);
            let mut app = ui::App::new(config);
            app.brancher_instance(instance.take());
            if force_software {
                app.player.set_hardware_decoding(false);
            }
            if let Some(v) = vitesse_depart {
                app.player.set_speed(v);
            }
            if let Some(ms) = tampon_audio {
                app.player.set_audio_buffer_ms(ms);
                tracing::info!("Tampon audio imposé à {} ms", ms);
            }
            if start_fullscreen {
                app.set_startup_fullscreen();
            }

            // Les fichiers passés en ligne de commande forment une playlist ;
            // on démarre la lecture sur le premier.
            for file in &cli_files {
                if !app.config.playlist.contains(file) {
                    app.config.playlist.push(file.clone());
                }
                // La playlist du moteur doit refléter la session courante, même si
                // le fichier figurait déjà dans la configuration persistée : c'est
                // elle qui sert à la navigation et au rechargement d'un média.
                if !app.player.playlist.contains(file) {
                    app.player.add_to_playlist(file.clone());
                }
            }
            if let Some(first) = cli_files.first() {
                app.load_media(first);
            }
            // APRÈS le chargement : avant, il n'y a pas de contexte de format et le
            // seek est sans effet (défaut constaté : trois captures identiques).
            if let Some(ms) = position_depart {
                let _ = app.player.seek(ms);
                tracing::info!("Démarrage à {} ms", ms);
            }
            // Boucle A-B posée dès le départ (`--boucle-ab 2 4` : de 2 s à 4 s).
            if let Some((a, b)) = boucle_ab_depart {
                if app.player.set_loop_points(a, b) {
                    tracing::info!("Répétition A-B posée au départ : {} → {} ms", a, b);
                } else {
                    tracing::warn!("--boucle-ab : B doit suivre A ({} → {})", a, b);
                }
            }
            // La playlist alimentée par la ligne de commande doit survivre à la
            // session : elle n'était auparavant jamais enregistrée.
            if !cli_files.is_empty() {
                let _ = app.config.save();
            }

            Ok(Box::new(app))
        }),
    )
    .map_err(|e| format!("Erreur eframe: {}", e).into())
}

/// Applique au moteur la règle de profil qui concerne le média ouvert, sur les
/// chemins **sans interface** (`--dump-frame`, `--dump-audio`).
///
/// Pourquoi ici : la résolution n'est connue qu'après le chargement, et les filtres
/// comme la chaîne audio ont été construits à ce moment-là. Il faut donc rejouer
/// les deux sur le lecteur — c'est exactement ce que fait l'interface après
/// l'application d'un profil. Sans ce chemin, une règle de résolution n'aurait
/// aucun effet mesurable et resterait invérifiable.
fn appliquer_regles_sans_interface(
    lecteur: &mut player::VideoPlayer,
    config: &mut config::Config,
    profils: &[config::Profil],
    chemin: &std::path::Path,
    resolution: (u32, u32),
) -> Option<config::Regle> {
    let regle = config::appliquer_regle(config, profils, chemin, Some(resolution))?;
    lecteur.set_video_filters(&config.video);
    lecteur.set_audio_filters(&config.audio);
    Some(regle)
}

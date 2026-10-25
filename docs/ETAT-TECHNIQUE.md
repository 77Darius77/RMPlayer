# RMPlayer — État technique (instantané)

> État d'ensemble et travail restant : [`ETAT-DU-PROJET.md`](ETAT-DU-PROJET.md).
> Ce document décrit l'environnement, l'API du moteur et les procédures de diagnostic.

**Date :** 2026-10-16  
**Binaire validé :** `target/release/rmplayer` (51 Mo, `debug = "line-tables-only"`)

---

## Environnement de référence

| Élément | Valeur |
|---|---|
| FFmpeg | 7.1.3 — `/opt/ffmpeg-7.1`, compilé avec `--enable-libdav1d --enable-libplacebo --enable-libzimg --enable-vulkan` (session 35). Ancienne installation (sans HDR) conservée dans `/opt/ffmpeg-7.1.avant-hdr` |
| Bibliothèques | `LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib` |
| Serveur audio | PipeWire 1.6 (`pipewire-pulse`), sinks : HDMI `alsa_output.pci-0000_0c_00.1.hdmi-stereo` + Bluetooth |
| Format de sortie audio obtenu | `44100 Hz, 2 canaux (F32)` |
| GPU / OpenGL | AMD Radeon RX 6700 XT, Mesa 25.3.6, OpenGL 4.6 (backend `glow`) |
| Affichage | X11 (`DISPLAY=:0`), écran 2560×1080 |
| Outils de diagnostic | `gdb`, `coredumpctl`, `zstd`, `addr2line`, `scrot`, `xdotool`, `vainfo` |
| Décodage matériel | VA-API 1.20 (libva 2.12), `radeonsi_drv_video.so`, Mesa Gallium 25.3.6 |
| Profils VA-API utiles | H.264 (hors `yuvj420p`), HEVC Main/Main10, VP9, **AV1**, MPEG-2, VC-1, MJPEG |

---

## Dépendances (`Cargo.toml`)

| Crate | Version | Usage |
|---|---|---|
| `ffmpeg-sys-next` | 7.1 | FFI FFmpeg (features : avcodec, avformat, avfilter, swscale, **swresample**) |
| `egui` / `eframe` / `egui_extras` | 0.33 | Interface (backends `x11` + `wayland`, rendu `glow`) |
| `cpal` | 0.15 | Sortie audio |
| `serde` / `serde_json` / `dirs` | 1.0 / 1.0 / 6.0 | Configuration persistante |
| `rfd` | 0.15 | Dialogues de fichiers |
| `tracing` / `tracing-subscriber` | 0.1 / 0.3 | Journalisation (`RUST_LOG=rmplayer=debug`) |
| `libc` | 0.2 | Handler de signal de `crash.rs` |
| `chrono` | 0.4 | Dates |

> `rodio` a été **retiré** : la sortie audio passe exclusivement par cpal.

---

## Fichiers clés

| Fichier | Lignes | Rôle |
|---|---|---|
| `src/player.rs` | 6812 | Moteur de lecture (décodage VA-API, mise à l'échelle, reprise) |
| `src/ui.rs` | 4605 | Interface (menus, OSD, playlist, préférences, mémoire de la fenêtre) |
| `src/ass.rs` | 2058 | Rendu ASS (styles, animations, karaoké, sous-titres image) |
| `src/settings.rs` | 2789 | Fenêtre « Préférences » : sections dépliables (table `SECTIONS`) |
| `src/config.rs` | 2220 | Configuration (réglages réellement appliqués) |
| `src/raccourcis.rs` | 925 | Table des raccourcis (défauts, personnalisation, aide F1) |
| `src/main.rs` | 924 | Point d'entrée, options de ligne de commande, géométrie de fenêtre |
| `src/audio.rs` | 688 | Sortie cpal + liste des périphériques |
| `src/subtitles.rs` | 478 | Sous-titres externes |
| `src/crash.rs` | 365 | Rapport de crash |
| `src/utils.rs` | 142 | Utilitaires |
| `src/hdr.rs` | 920 | HDR : détection PQ/HLG/BT.2020, plan de conversion (GPU/CPU), tramage, débanding — fonctions pures testables |
| `src/edid.rs` | 309 | EDID : lecture et décodage des blocs CEA-861 (PQ, HLG, BT.2020, luminance max) des écrans allumés |
| `src/mpris.rs` | 640 | MPRIS : interfaces D-Bus (racine + lecteur), état publié, file de commandes, réveil de l'interface |
| `src/vignettes.rs` | 519 | Vignettes de la barre : décodeur FFmpeg autonome, fil de travail, regroupement par tranche |
| `src/instance.rs` | 370 | Instance unique : socket Unix, transmission des chemins, balayage des sockets orphelins |
| `src/x11.rs` | 114 | « Fenêtre toujours au-dessus » (`_NET_WM_STATE`, sans passer par la commande d'eframe qui panique) |
| `src/positions.rs` | 73 | Reprise de lecture |
| `src/renderer.rs` | 43 | Transport des frames |
| **Total** | **25299** | (hors `tools/`, 19 modules) |
| `tools/bench-hwdec.sh` | 78 | Banc d'essai matériel/logiciel (CPU par thread) |
| `tools/capture-lecteur.sh` | 56 | Lecture sur écran X virtuel + capture PNG |
| `tools/diag-touches.sh` | 39 | Diagnostic du clavier sous Xvfb |
| `tools/zones-barre.py` | 29 | Détection des boutons de la barre (analyse d'image) |
| `tools/audit-ui.sh` | 512 | Audit de l'interface (boutons, raccourcis, fenêtres, gel) |
| `tools/check-ctx-reentrance.py` | 114 | Interblocages egui : `ctx.*` dans une fermeture de saisie |
| `tools/vivacite.sh` | 40 | Détecteur de gel (délivrance réelle d'un message) |
| `tools/test-vivacite.sh` | 78 | Preuve que le détecteur de gel détecte (`SIGSTOP`/`SIGCONT`) |
| `tools/diag-gel.sh` | 90 | Reproduit un gel et affiche la pile (lancement sous `gdb` + `SIGSTOP`) |
| `tools/verif-filtres.sh` | 313 | Vérifie chaque filtre par une mesure (12 contrôles, `--dump-frame`) |
| `tools/audit-prefs.sh` | 303 | Fenêtre Préférences : bornage, dépliage, défilement (géométrie journalisée) |
| `tools/metrics-image.py` | 100 | Métriques d'image : bandes, bruit, peigne, luminance, saturation |
| `tools/verif-fenetre.sh` | 272 | Mémoire de la taille et de la position de la fenêtre (9 contrôles : déplacement → configuration → relance → géométrie réelle) |
| `tools/verif-fenetre-wayland.sh` | 293 | Mémoire de la fenêtre sous Wayland (12 contrôles ; mesure par script KWin, fenêtre ciblée par PID, `kwinrulesrc` sauvegardé/restauré) |
| `tools/installer-association.sh` | 205 | « Ouvrir avec » : `.desktop`, icône, `mimeinfo.cache`, `--par-defaut`, `--etat`, `--retirer` |
| `tools/verif-hdr.sh` | 390 | HDR (23 contrôles, mires en nits, VA-API vs logiciel) |
| `tools/verif-focus.sh` | 233 | Pause selon le focus de la fenêtre (6 contrôles, deux fenêtres réelles) |
| `tools/verif-mpris.sh` | 290 | MPRIS — module seul (27 contrôles, mode `--mpris-test`, sans écran) |
| `tools/verif-vignettes.sh` | 230 | Vignettes — module seul (14 contrôles, sans écran) |
| `tools/verif-playlist.sh` | 512 | Playlist : aléatoire, glisser-déposer, clic et ✕, volet borné (18 contrôles ; lignes visées depuis la géométrie journalisée) |
| `tools/verif-rapide.sh` | 110 | Vérification ciblée : ne rejoue que les bancs d'un sujet (`playlist`, `ui`, `fenetre`…) |
| `tools/verif-association.sh` | 393 | Association et instance unique (38 contrôles ; `Exec` réellement exécuté) |
| `tools/regle-fenetre-kwin.sh` | 227 | Règle de fenêtre KWin : la position de RMPlayer sous Wayland (`--position`, `--depuis-fenetre`, `--etat`, `--retirer`) |
| `tools/verif-ass.sh` | 412 | Rendu ASS : 21 tests unitaires + position, couleur, alignement, animations, karaoké |
| `tools/verif-profils.sh` | 334 | Profils de réglages |
| `tools/verif-pistes.sh` | 305 | Pistes embarquées : inventaire, bascule à chaud, sous-titres (16 contrôles) |
| `tools/verif-audio.sh` | 514 | Audio : 29 contrôles (9 traitements mesurés) |
| `tools/analyse-audio.py` | 143 | 8 mesures audio (niveaux, crête, S/B, canaux, fréquence, queue, début) |

*(Compteurs `src/` et de `verif-fenetre.sh` mesurés en session 32 ; les autres lignes de `tools/` datent de la session 20 : indicatives.)*

---

## API du moteur (`VideoPlayer`)

### Chargement et état

| Méthode | Rôle |
|---|---|
| `new(config)` | Initialise FFmpeg (`avformat_network_init`) |
| `load_file(path) -> Result<MediaInfo, String>` | Ouvre le média, ouvre les décodeurs vidéo/audio, prépare swresample, arme l'horloge |
| `close_media()` | Libère format/contexte/décodeurs/swr (la sortie audio est conservée) |
| `state() -> PlayerState` | `Idle` / `Loading` / `Playing` / `Paused` / `Stopped` / `Error` |
| `media_info() -> Option<&MediaInfo>` | Nom, durée, résolution, FPS, bitrate, codecs |
| `quit()` | Libération complète |

### Décodage et cadence

| Méthode | Rôle |
|---|---|
| `pump_video_frame() -> Option<VideoFrame>` | Renvoie la prochaine frame (file d'attente ou décodage), gère l'EOF et la boucle |
| `next_frame_delay(pts_ms) -> Option<Duration>` | Délai avant la frame suivante (auto-correcteur) |
| `drift_ms(pts_ms) -> i64` | Écart entre le PTS et l'horloge (> 0 = en avance) |
| `generation() -> u64` | Change à chaque chargement / seek / reprise (permet à l'UI de recaler) |
| `sync_clock_to(pts_ms)` | Recale l'horloge sur une frame précise |
| `frame_dimensions() -> (u32, u32)` | Dimensions de la dernière frame |

### Navigation et réglages

| Méthode | Rôle |
|---|---|
| `seek(position_ms)` | Seek précis (stream vidéo + décodage-saut vidéo **et** audio) |
| `toggle_pause()` | Pause / reprise (vide le tampon audio en pause) |
| `stop()` | Arrêt (position remise à zéro) |
| `play_next()` / `play_previous()` / `play_index(i)` | Navigation dans la playlist |
| `playlist_index()`, `playlist_index_advance()`, `remove_from_playlist(i)` | Gestion de la playlist |
| `is_finished()` | Vrai en fin de flux |
| `set_volume()`, `toggle_mute()`, `set_speed()`, `toggle_loop()`, `toggle_subtitles()` | Réglages |
| `audio_buffered_ms()`, `has_audio()` | État audio (tampon en ms, présence d'une piste) |

### Décodage et réglages (tous réellement appliqués)

| Méthode | Rôle |
|---|---|
| `set_hardware_decoding(bool)` | Autorise/interdit VA-API (crée le périphérique à la demande) |
| `hardware_decoding_enabled() -> bool` | Mode **demandé** (avant ouverture du flux) |
| `is_hardware_decoding() -> bool` | Mode **constaté** (une frame VA-API a-t-elle été produite ?) |
| `set_scaling_method(ScalingMethod)` | Méthode de réduction transmise à `sws_getContext` (5 choix) |
| `set_display_target(w, h)` | Taille de conversion RGBA (pixels). `(0, 0)` = natif ; hystérésis de 16 px |
| `set_volume_max(u16)` | Plafond de volume (100–300 %) ; ramène le volume courant dans la limite |
| `set_audio_device(&str)` | Périphérique de sortie (nom cpal, `""` = défaut) ; sortie recréée au chargement suivant |
| `set_speed(f32)` | Vitesse 0,1–8× **et** rééchantillonnage audio synchrone |
| `set_resume(bool)` / `persist_position()` | Reprise de lecture (positions par média) |
| `current_path() -> Option<&PathBuf>` | Média réellement ouvert (indépendant de la playlist) |
| `reload_current_media()` | Recharge le média courant (position + pause conservées) — applique un changement de mode ou de périphérique |

### Structures

- `VideoFrame` : `data: Vec<u8>` (RGBA), `width`, `height`, `pts_ms`
- `MediaInfo` : métadonnées affichées dans l'OSD
- `PlayerState` : état du lecteur

**Règle du projet :** tout champ de `Config` doit être lu par le moteur — sinon il est
supprimé. C'était l'objet de la session 9 : les réglages décoratifs (détramage, HDR,
interpolation, profils, égaliseur, back-end audio, table de raccourcis) ont été retirés,
et les champs/fonctions morts du moteur aussi. Depuis, la compilation ne produit plus
**aucun** avertissement `never used` / `never read`.

Test rapide avant d'ajouter un réglage :

```bash
grep -rn "config\.<section>\.<champ>" src/ | grep -v 'src/config.rs'
```

Si la commande ne renvoie rien, le réglage n'existe pas du point de vue du moteur.

---

### Préférences (session 14)

La fenêtre est décrite par une **table** : `src/settings.rs`, `SECTIONS`. Pour ajouter un
réglage ou un domaine :

1. ajouter le champ dans `Config` (avec `#[serde(default)]`) — et le brancher côté moteur ;
2. si c'est un nouveau domaine : ajouter une `Section { id, icon, title, hint,
   default_open, modified, draw }` dans `SECTIONS` ;
3. écrire `dessiner_<domaine>(app: &mut App, ui: &mut Ui)`, en utilisant `sous_section()`
   pour séparer et `ligne()` pour aligner un libellé et un réglage.

Rien d'autre : bornage à l'écran, défilement, boutons globaux, compteur de modifications et
journalisation de géométrie sont pris en charge par `settings::window`.

### Rendu ASS (session 19, `src/ass.rs`)

| Élément | Rôle |
|---|---|
| `analyser(&str) -> AssScript` | analyse `[Script Info]`, `[V4+ Styles]`, `Dialogue:` |
| `AssScript::events_at(ms)` | événements actifs, triés par couche |
| `AssScript::opacite(e, ms)` | opacité compte tenu du fondu `\fad` |
| `placer(e, w, h, play_res, opacite, mesurer)` | fragments placés en pixels (mesureur injecté → testable sans écran) |
| `couleur_ass(&str)` | `&HAABBGGRR` → RGBA (BGR, alpha inversé) |

### Profils (session 19, `src/config.rs`)

`Profil::capture`, `Profil::appliquer` (préserve le périphérique audio), `Profil::resume`,
`load_profiles`, `save_profiles`, `profiles_path` ; ligne de commande `--profil`,
`--list-profiles`.

### Pistes embarquées (session 17)

| Méthode | Rôle |
|---|---|
| `select_audio_stream(index)` | change de piste audio à chaud (décodeur + convertisseur + filtres reconstruits) |
| `current_audio_stream()` | index du flux audio ouvert |
| `select_subtitle_stream(index)` | ouvre une piste de sous-titres du conteneur (`-1` = aucune) |
| `embedded_text_at(ms)` | texte de la piste embarquée à cet instant |
| `dump_subtitles()` | tous les événements de la piste (texte + minutages) |
| `set_track_preferences(audio_lang, sub_lang, embedded)` | préférences appliquées aux prochains chargements |

Réglages : `audio.default_language`, `subtitles.default_language`, `subtitles.use_embedded`.
Structures : `TrackInfo` (index, codec, langue, titre, canaux, taux, `bitmap`),
`EmbeddedCue` (début, fin, texte).

### Égaliseur audio (session 15)

| Méthode | Rôle |
|---|---|
| `set_audio_filters(&AudioSettings)` | reconstruit le graphe audio à la trame suivante |
| `audio_filters_description()` | chaîne active ("" = aucune) |
| `audio_filter_counts()` | (échantillons entrés, sortis) — doivent rester égaux |
| `dump_audio()` | décode tout l'audio, égaliseur compris (`--dump-audio`, WAV f32) |

Réglages audio : `eq_enabled`, `eq_gains` (18 multiplicateurs, bornés 0–20, complétés à
18 au chargement), `normalizer`, `night_mode`, `pitch_preserved`, `noise_reduction`
(0–3), `stereo_widen` (0–3), `crossfeed`, `karaoke`, `reverb` (0–3), `delay_ms`
(−2000…2000). Profils : `config::EqPreset`.

Réglages vidéo ajoutés : `sharpen` (0–3), `rotate` (0/90/180/270), `flip`, `zoom` (1–4).

Traduction : `player::filter_description` (vidéo) et `player::filtres_audio` (audio,
ordre documenté). La vitesse passe par `atempo` quand `pitch_preserved` est vrai, sinon
par rééchantillonnage.

### Filtres (session 13)

| Méthode | Rôle |
|---|---|
| `set_video_filters(&VideoSettings)` | reconstruit la chaîne à l'image suivante (sans recharger le média) ; journalise la chaîne |
| `filters_description()` | description active ("" = aucune) |
| `filter_frame_counts()` | (entrées, sorties) du graphe — doivent rester égaux (filtres 1:1) |
| `dump_frame_at(ms)` | image à la position demandée, résolution native, filtres compris (`--dump-frame`) |
| `bench_decode(n)` | débit de décodage filtres compris (`--bench`) |

Réglages : `video.deband` (bool), `video.denoise` (0–3), `video.deinterlace` (bool),
`video.brightness` (-1–1), `video.contrast` (0–3), `video.saturation` (0–3),
`video.gamma` (0,1–10). Traduction en filtres : `player::filter_description` (seul endroit).

---

## Points de vigilance pour la suite

1. **Ne jamais redéfinir les structs FFmpeg à la main** — toujours `ffmpeg_sys_next`.
2. **Toujours valider la lecture en `--release`** (le debug est ~6× plus lent et plafonne
   la cadence, ce qui simule un bug de synchronisation).
3. **Le callback audio ne doit prendre aucun verrou bloquant**, et l'UI ne doit jamais
   appeler `play()` / `pause()` sur le flux cpal.
4. **Le format audio de sortie est celui du périphérique**, jamais celui du fichier.
5. Les sous-titres se dessinent dans **`image.rect.intersect(ui.clip_rect())`** (l'image
   peut être rognée par son panneau).
6. `XDG_RUNTIME_DIR` doit être défini pour que cpal/PipeWire trouve le serveur audio.
7. Un SIGSEGV fait **interbloquer** le handler de la std Rust : c'est `src/crash.rs` qui
   fournit l'information exploitable (`~/.cache/rmplayer/crash.log`).
   *Attention : `--crash-test` produit de vrais cores SIGSEGV — vérifier l'argument de
   lancement avant de conclure à un plantage.*
8. **Un décodeur ouvert en VA-API n'implique pas l'usage du matériel** : si le pilote
   refuse le profil, libavcodec bascule en logiciel sans erreur. Le mode affiché est
   celui **constaté** (`is_hardware_decoding`), pas celui demandé.
9. **Le contexte swscale doit être reconstruit** quand le format ou les dimensions
   d'entrée/sortie changent (4 champs mémorisés dans `VideoStream`).
10. **En cas de test sur écran virtuel** : sans gestionnaire de fenêtres, il faut
    `xdotool windowfocus --sync <id>` (et pointer la souris sur la fenêtre) pour que les
    touches atteignent l'application ; `xdotool key --window` est ignoré par winit.

---

## Procédure de diagnostic de crash

```bash
# 1. Consulter le rapport applicatif (le précédent est conservé : crash.1.log)
cat ~/.cache/rmplayer/crash.log

# 2. Lire en premier `si_code`, `si_pid`, `sender` et `origine` :
#    - origine=defaut interne au processus  → vrai plantage, continuer en 3
#    - origine=ARRET FORCE PAR UN AUTRE PROCESSUS → le processus a été tué
#      (gestionnaire de fenêtres : KWin envoie SIGABRT puis SIGKILL sur une
#      fenêtre gelée). Chercher alors un GEL, pas un bug de mémoire.

# 3. Symboliser les adresses (rip_rel est un offset absolu dans le binaire)
addr2line -f -C -e target/release/rmplayer <rip_rel>

# 4. Reproduire un gel et obtenir la pile complète, en une commande :
tools/diag-gel.sh q /chemin/film.mp4 :99 12

# 5. Si besoin, extraire un core système
coredumpctl list --no-pager | grep rmplayer
coredumpctl dump <PID> -o /tmp/core      # sinon : zstd -d -c /var/lib/.../*.zst > /tmp/core
gdb -batch -q -ex "info proc mappings" -ex "thread apply all bt" ./target/release/rmplayer /tmp/core
#    ATTENTION : dans le core, `si_code`/`si_pid` décrivent le RELANCEMENT du
#    signal par notre handler (SI_TKILL, pid = le nôtre), pas l'origine réelle.
```

### Gel de l'interface (interblocage) — méthode qui a fonctionné

```bash
# 1. Contrôle statique (le plus rapide) : tout `ctx.*` dans une fermeture de saisie
tools/check-ctx-reentrance.py

# 2. Vivacité : une application gelée reste vivante, seule la réception d'un
#    NOUVEAU message prouve que le fil principal progresse
tools/vivacite.sh <journal> :99 5

# 3. Pile complète. `ptrace_scope = 1` interdit `gdb -p` : lancer SOUS gdb, puis
#    figer le processus — gdb reprend la main et affiche toute la pile.
gdb -q -nx -batch -x script.gdb ./target/release/rmplayer
#    script.gdb :
#      shell ( sleep 12; <envoyer la touche fautive>; kill -STOP $(pgrep -x rmplayer) ) &
#      run <film.mp4>
#      thread apply all bt 40
# 4. Chercher dans la pile : `lock_shared`/`lock_exclusive_slow` = verrou egui
#    (epaint::mutex → parking_lot). Le verrou EXCLUSIF est tenu par la fermeture
#    de saisie en cours (frame `Context::write` ← `Context::input`).
```

---

## Validation effectuée (dernière session)

| Test | Résultat |
|---|---|
| Lecture 60 s (H.264 1280×720 @30) | ✅ dérive ±7 ms, tampon audio 30–90 ms |
| Lecture 5 min (H.264 640×360 @25) | ✅ 2 min sans incident |
| Pause / reprise | ✅ horloge recalée, tampon vidé |
| Seek ×3 | ✅ cible respectée, audio resynchronisé |
| Muet / volume | ✅ |
| Enchaînement playlist (3 médias) | ✅ 1 seule sortie audio créée |
| Sous-titres SRT en surimpression | ✅ |
| Plein écran (`--fullscreen`) | ✅ UI masquée, image mise à l'échelle |
| Rapport de crash (`--crash-test`) | ✅ `signal=11`, core préservé, pile symbolisable |
| Origine d'un arrêt externe | ✅ `kill -ABRT <pid>` → `si_code=0`, `si_pid=690806`, `sender=bash`, `origine=ARRET FORCE PAR UN AUTRE PROCESSUS` |
| Rotation du rapport | ✅ `crash.log` → `crash.1.log` au démarrage suivant |
| Touche `Q` | ✅ ferme le lecteur (auparavant : gel définitif de l'application) |
| Détecteur de gel | ✅ `tools/test-vivacite.sh` : vert → `SIGSTOP` → gel détecté → `SIGCONT` → vert |
| Détramage (`deband`) | ✅ plateau d'un dégradé 12,97 → 2,57 lignes (mesuré sur l'image décodée) |
| Débruitage (`hqdn3d`) | ✅ énergie haute fréquence 17,56 → 4,54 (niveau 3) |
| Désentrelacement (`yadif`) | ✅ peigne : écart vertical 195 → 0 |
| Luminosité / saturation (`eq`) | ✅ luminance 123 → 187 (+0,25) ; saturation 0 → écart entre canaux 169 → 3,7 |
| Filtres, décodage matériel | ✅ identique en VA-API (nv12) et en logiciel (yuv420p) |
| Coût des filtres (`--bench`) | ✅ 1080p : 11,4 ms/image sans filtre → 21,1 avec toute la chaîne (47 i/s) |
| Neutralité des filtres | ✅ aucun graphe construit quand aucun réglage n'est actif |
| Égaliseur — basses ×4 | ✅ +11,95 dB à 65 Hz, +11,98 à 131 Hz (attendu +12,04) |
| Égaliseur — aigus ×4 | ✅ +12,04 dB à 8,4 kHz, +12,05 à 16,7 kHz |
| Égaliseur — atténuation ×0,25 | ✅ −12,13 dB à 65 Hz, −0,03 dB à 1 kHz |
| Égaliseur — neutre | ✅ signal strictement identique à la référence (écart 0,000000) |
| Égaliseur — intégrité | ✅ 132 637 échantillons entrés = sortis |
| Égaliseur — réglages hors plage | ✅ bornés (50 → 20, −3 → 0), bandes complétées |
| Préférences — égaliseur | ✅ profil identifié, 18 bandes, curseur actif, aucune dérive à la réouverture |
| Correction de hauteur (`atempo`) | ✅ 440 Hz restent 440,0 Hz à 2× ; durée 6,02 s → 3,00 s |
| Normaliseur (`dynaudnorm`) | ✅ écart fort/faible 24,4 → 9,0 dB |
| Mode nuit (`acompressor`) | ✅ écart 24,4 → 13,8 dB ; crête 0,95 (limiteur actif) |
| Réduction de bruit (`afftdn`) | ✅ rapport signal/bruit 19,7 → 45,1 dB, signal inchangé |
| Outils stéréo | ✅ élargissement 0,097 → 0,202 ; crossfeed → 0,042 ; karaoké 0,003 % |
| Réverbération (`aecho`) | ✅ queue après impulsion 0,00002 → 0,0154 |
| Décalage audio (`adelay`) | ✅ +500 ms → premier son à 510 ms |
| Netteté (`unsharp`) | ✅ énergie haute fréquence 17,6 → 44,5 |
| Rotation / zoom | ✅ 1280×720 → 720×1280 ; zoom 2× écart 32/255, dimensions conservées |
| Pistes — inventaire | ✅ 4 pistes (2 audio, 2 sous-titres) avec langues et titres |
| Pistes — contenu | ✅ piste 1 = 300,0 Hz, piste 2 = 900,0 Hz (FFT) |
| Pistes — langue préférée | ✅ « fre » → 900 Hz, « eng » → 300 Hz |
| Pistes — sous-titres | ✅ texte exact, minutages 1000→3000 et 4000→6000 |
| Pistes — bascule à chaud | ✅ menu → « flux 2 — fre », lecture non interrompue |
| Pistes — affichage | ✅ sous-titre embarqué dessiné (écart d'image 12,94/255) |
| Décodage VA-API — H.264 720p / 4K | ✅ matériel confirmé (CPU 22 % → 22 %, 56 % → 37 %) |
| Décodage VA-API — AV1 4K 60 i/s | ✅ 52 % CPU (aucun décodeur logiciel dans ce FFmpeg) |
| Repli automatique — H.264 `yuvj420p` 1080p | ✅ repli signalé, lecture normale |
| Bascule `H` en cours de lecture | ✅ rechargement à la position (00:09 conservé), mode affiché mis à jour |
| Enchaînement de playlist (2 médias) | ✅ (non-régression après le correctif de playlist) |
| Rapport d'aspect (source 4:3 dans fenêtre 16:9) | ✅ 1,3309 mesuré / 1,3333 attendu (gigue de ±2 px du détecteur) |
| Mise à l'échelle (damier 4K → ~900 px) | ✅ écart-type 1,8 (Lanczos) / 6,6 (Moyenne de zone) / 121,2 (Plus proche voisin) |
| Capture d'écran (`S`) | ✅ PNG 1212×682 RGBA écrit dans `~/Images/Captures-video/` |
| OSD (`O`) | ✅ bandeau dessiné (média, position, décodage, audio, volume, vitesse) |
| Panneau playlist (`L`) | ✅ affiché, élément courant mis en évidence, restauration au démarrage |
| Ouvrir dossier | ✅ playlist constituée, premier fichier lu |
| Reprise de lecture | ✅ `Reprise de lecture à 29300 ms` après réouverture |
| Vitesse 1,2× | ✅ `Audio rééchantillonné pour 1.20x (44100 Hz → 36750 Hz)` |
| Changement de périphérique audio | ✅ sortie recréée, préférences listant les périphériques cpal |
| Enchaînement playlist (2 médias) | ✅ |
| Rapport de crash (`--crash-test`) | ✅ `signal=11` |
| Compilation | ✅ **0 avertissement** (aucun `never used` / `never read`) |
| Barre de progression | ✅ clic 25 % → 15 149 ms ; 50 % → 29 550 ms ; 75 % → 44 550 ms ; glissement appliqué |
| Raccourcis clavier (18 touches) | ✅ `tools/audit-ui.sh` |
| Répétition A-B | ✅ retour au point A constaté (3 passages en 10 s) |
| Fenêtre toujours au-dessus | ✅ `_NET_WM_STATE` via `src/x11.rs` |
| Masquage de l'interface (Tab) · aide (F1) | ✅ validés par comparaison d'images |
| **Audit complet de l'interface** | ✅ **54/54** (`tools/audit-ui.sh` : boutons, 18 raccourcis, 4 fenêtres, contrôle statique des interblocages, vivacité, fermeture par `Q`) |
| Fenêtres de réglages (4) | ✅ fermables par ✕ **et** par Échap (géométrie journalisée) |
| Journal de diagnostic | ✅ géométrie des fenêtres + touches reçues, via `RUST_LOG=rmplayer=info,kinterface=debug` |

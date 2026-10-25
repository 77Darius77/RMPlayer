# RMPlayer — État technique (instantané)

> État d'ensemble et travail restant : [`ETAT-DU-PROJET.md`](ETAT-DU-PROJET.md).
> Ce document décrit l'environnement, l'API du moteur et les procédures de diagnostic.

**Date :** 2026-10-25 (session 37)  
**Binaire validé :** `target/release/rmplayer` (51 Mo, `debug = "line-tables-only"`)

---

## Environnement de référence

| Élément | Valeur |
|---|---|
| FFmpeg | 7.1.3 — `/opt/ffmpeg-7.1`, compilé avec `--enable-libdav1d --enable-libplacebo --enable-libzimg --enable-vulkan` (session 35). Ancienne installation (sans HDR) conservée dans `/opt/ffmpeg-7.1.avant-hdr` |
| Bibliothèques | `LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib` |
| Serveur audio | PipeWire 1.6 (`pipewire-pulse`), sorties : HDMI `alsa_output.pci-0000_0c_00.1.hdmi-stereo` + enceinte Bluetooth. Outils utilisés par le lecteur : `pw-dump`, `pw-metadata` (sélection de la sortie) ; par les bancs : `pw-record`, `pactl` |
| Format de sortie audio obtenu | `44100 Hz, 2 canaux (F32)` |
| GPU / OpenGL | AMD Radeon RX 6700 XT, Mesa 25.3.6, OpenGL 4.6 (backend `glow`) |
| Affichage | **Plasma 6.8 sous Wayland** (la session X11 a disparu), trois écrans SDR. Les bancs tournent sous **Xvfb** (écran virtuel 1500×900) |
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
| `zbus` | 5.19 | MPRIS (D-Bus, `blocking-api`, serveur dans son propre fil) |
| `ksni` | 0.3 | Icône de zone de notification (StatusNotifierItem) |
| `encoding_rs` | 0.8 | Encodages des sous-titres externes (alphabet imposé) |
| `png` | 0.18 | Captures d'écran (encodeur pur Rust) |
| `x11rb` | 0.13 | « Fenêtre toujours au-dessus » (X11 seulement) |

> `rodio` a été **retiré** : la sortie audio passe exclusivement par cpal.

---

## Fichiers clés

| Fichier | Lignes | Rôle |
|---|---|---|
| `src/player.rs` | 7418 | Moteur de lecture (décodage VA-API, mise à l'échelle, cadence, pistes, filtres, reprise) |
| `src/ui.rs` | 5306 | Interface (menus, OSD, playlist, fenêtres, mémoire de la fenêtre) |
| `src/settings.rs` | 3338 | Fenêtre « Préférences » : sections dépliables (table `SECTIONS`) |
| `src/config.rs` | 2724 | Configuration (réglages réellement appliqués), profils, export / import |
| `src/ass.rs` | 2059 | Rendu ASS (styles, animations, karaoké, sous-titres image) |
| `src/main.rs` | 1295 | Point d'entrée, options de ligne de commande, géométrie de fenêtre |
| `src/hdr.rs` | 998 | HDR : détection PQ/HLG/BT.2020, plan de conversion (GPU/CPU), tramage, débanding |
| `src/raccourcis.rs` | 990 | Table unique des raccourcis clavier et souris (défauts, personnalisation, aide F1) |
| `src/audio.rs` | 695 | Sortie cpal, tampon circulaire, horloge audio |
| `src/subtitles.rs` | 650 | Sous-titres externes (SRT / ASS), encodage |
| `src/mpris.rs` | 640 | MPRIS : interfaces D-Bus, état publié, file de commandes, réveil de l'interface |
| `src/vignettes.rs` | 519 | Vignettes de la barre : décodeur FFmpeg autonome, fil de travail |
| `src/instance.rs` | 370 | Instance unique : socket Unix, transmission des chemins, balayage des sockets orphelins |
| `src/crash.rs` | 365 | Rapport de crash |
| `src/sortie_audio.rs` | 313 | Sorties PipeWire : liste lisible (`pw-dump`) et redirection du flux (`pw-metadata`) |
| `src/edid.rs` | 309 | EDID : capacités annoncées par les écrans allumés |
| `src/utils.rs` | 167 | Utilitaires |
| `src/signets.rs` | 166 | Signets par média |
| `src/systeme.rs` | 136 | Démarrage automatique et priorité du processus |
| `src/theme.rs` | 131 | Thèmes externes JSON |
| `src/migration.rs` | 128 | Reprise des données de l'ancien nom (KPlayer) |
| `src/tray.rs` | 120 | Icône de zone de notification (StatusNotifierItem) |
| `src/pistes.rs` | 117 | Mémoire des pistes choisies à la main |
| `src/x11.rs` | 114 | « Fenêtre toujours au-dessus » (`_NET_WM_STATE`) |
| `src/fenetre.rs` | 97 | Fenêtres flottantes détachables (viewport immédiat) |
| `src/positions.rs` | 73 | Reprise de lecture |
| `src/renderer.rs` | 43 | Transport des images |
| **Total** | **29280** | 27 modules (hors `tools/`) |

`tools/` : 44 bancs lancés par `tout-verifier.sh`, plus les outils de diagnostic (`audit-ui.sh`,
`check-ctx-reentrance.py`, `vivacite.sh`, `diag-gel.sh`, `capture-lecteur.sh`…) et les
installateurs (`installer-association.sh`, `regle-fenetre-kwin.sh`). **Liste, rôle et nombre de
contrôles de chacun : `README.md`, § Outils** (c'est la table tenue à jour). Instruments
partagés : `env-bancs.sh` (environnement des bancs), `analyse-audio.py`, `metrics-image.py`,
`pw-flux.py` (où va le flux audio dans PipeWire).

*(Compteurs de `src/` mesurés en session 37.)*

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
| `set_audio_device(&str) -> bool` | Sortie audio : `""` = défaut, `pw:<nom>` = sortie PipeWire (le flux est **redirigé** sans coupure), autre = nom ALSA ouvert tel quel. Renvoie `true` si la sortie a dû être recréée (l'appelant recharge alors le média) |
| `set_speed(f32)` | Vitesse 0,1–8× **et** rééchantillonnage audio synchrone |
| `set_resume(bool)` / `persist_position()` | Reprise de lecture (positions par média) |
| `current_path() -> Option<&PathBuf>` | Média réellement ouvert (indépendant de la playlist) |
| `reload_current_media()` | Recharge le média courant (position + pause conservées) — applique un changement de mode ou de sortie ALSA |

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
| `set_track_preferences(audio: &[String], subs: &[String], embedded)` | listes de langues **par priorité**, appliquées aux prochains chargements |
| `memoriser_piste_audio(index)` / `set_audio_remember(bool)` | mémoire de la piste choisie à la main, par média (`pistes-audio.json` du cache) |

Réglages : `audio.preferred_languages` (et `default_language` à défaut), `audio.remember_track`, `subtitles.preferred_languages`, `subtitles.use_embedded`. Choix : `choisir_piste_audio` écarte commentaires et audiodescriptions (drapeau du conteneur ou mot du titre), puis suit la liste de langues, puis la piste « par défaut » du fichier.
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

## Validation historique (sessions 9 à 20)

> **Ces mesures datent des sessions 9 à 20.** L'état courant est mesuré par la suite complète
> (`docs/DERNIERE-SUITE.txt`) et décrit module par module dans `README.md` (§ Outils). Quelques
> lignes ci-dessous sont **corrigées depuis** (AV1 logiciel : `libdav1d` depuis la session 31 ; choix
> du périphérique audio : refait en session 37, voir `src/sortie_audio.rs`).

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
| Décodage VA-API — AV1 4K 60 i/s | ✅ 52 % CPU (le décodage logiciel AV1 est disponible depuis la session 31) |
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
| Changement de périphérique audio | ✅ session 9 : sortie recréée. **Refait en session 37** : redirection du flux PipeWire en cours de lecture (`verif-peripheriques`) |
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

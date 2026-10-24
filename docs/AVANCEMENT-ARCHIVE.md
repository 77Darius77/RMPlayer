# AVANCEMENT — archive (sessions 2 à 31)

Contenu déplacé de `AVANCEMENT.md` le 2026-10-18 (session 31) pour que le fichier
courant reste court et lisible à la reprise. **Rien n'a été supprimé** : mêmes textes,
même ordre. Les pièges (§7 d'origine) sont restés dans le fichier courant, avec
l'état actuel, l'architecture, les fonctionnalités et les raccourcis.

---

## 1. État actuel

Le lecteur **fonctionne** : ouverture d'un fichier, décodage vidéo et audio, affichage,
cadence respectée, seek, playlist, sous-titres, plein écran. Aucun crash constaté sur
les tests de validation (jusqu'à 2 min de lecture continue, enchaînements, seek, pause).

| Domaine | État |
|---|---|
| Décodage vidéo (H.264/HEVC/VP9/AV1…) | ✅ fonctionnel |
| Affichage egui (mise à l'échelle, ratio conservé) | ✅ fonctionnel |
| Cadence / synchronisation temporelle (PTS) | ✅ dérive ≈ 0 (33,33 ms/frame à 30 fps) |
| Audio (PipeWire / PulseAudio / ALSA via cpal) | ✅ fonctionnel |
| Seek précis | ✅ vidéo + audio |
| Playlist + enchaînement automatique | ✅ fonctionnel |
| Sous-titres externes (SRT, ASS/SSA) | ✅ affichés en surimpression |
| Plein écran | ✅ fonctionnel |
| Décodage matériel (VA-API) | ✅ fonctionnel, repli logiciel automatique (§6.10) |
| Mise à l'échelle (5 méthodes) | ✅ appliquée par libswscale (§6.11) |
| **Filtres d'image** (détramage, débruitage, désentrelacement, netteté, géométrie, eq) | ✅ appliqués par libavfilter et **mesurés** (§6.21) |
| **Égaliseur audio 18 bandes** | ✅ `superequalizer`, mesuré au spectre (§6.21) |
| **Traitements audio PotPlayer** (normaliseur, mode nuit, bruit, stéréo, réverbération, décalage, hauteur) | ✅ 9 traitements, chacun mesuré (§6.21) |
| **Chapitres, cadrage, aspect, style des sous-titres, URL** | ✅ 7 réglages, chacun mesuré (§6.16) |
| **Pistes embarquées** (audio et sous-titres du conteneur) | ✅ inventaire, changement à chaud, minutages exacts (§6.21) |
| **Géométrie vidéo** (rotation, miroir, zoom) et **netteté** | ✅ mesurés (§6.21) |
| Capture d'écran PNG | ✅ fichier réellement écrit |
| OSD, statistiques, playlist | ✅ affichés et interactifs |
| Reprise de lecture, playlist persistante | ✅ `~/.cache/kplayer/positions.json` |
| Sous-titres embarqués dans le conteneur | ❌ à faire (limite assumée) |
| Sélection de piste audio | ❌ à faire (limite assumée) |
| Rapport de crash | ✅ `src/crash.rs` (voir §6) |

---


## 3. Architecture

```
src/
├── main.rs        Point d'entrée : logging, rapport de crash, arguments CLI, eframe
├── player.rs      Moteur FFmpeg : démux, décodage, cadence, seek, playlist
├── ui.rs          Interface egui : contrôles, OSD, playlist, sous-titres, plein écran
├── audio.rs       Sortie cpal : tampon circulaire + callback temps réel
├── subtitles.rs   Sous-titres externes : parsing SRT / ASS / SSA
├── crash.rs       Rapport de crash sûr (handler de signal async-signal-safe)
├── config.rs      Configuration JSON persistante (~/.config/kplayer/config.json)
├── renderer.rs    Transport des frames RGBA vers egui
└── utils.rs       Utilitaires (formatage temps/taille/bitrate, détection de format)
```

| Fichier | Lignes |
|---|---|
| `src/player.rs` | 1127 |
| `src/ui.rs` | 862 |
| `src/subtitles.rs` | 399 |
| `src/crash.rs` | 273 |
| `src/config.rs` | 255 |
| `src/audio.rs` | 207 |
| `src/utils.rs` | 151 |
| `src/main.rs` | 117 |
| `src/renderer.rs` | 43 |
| **Total** | **3434** |

### Chaîne de traitement

```
avformat_open_input
  → avformat_find_stream_info
  → sélection des flux (vidéo / audio / sous-titres)
  → avcodec_parameters_to_context + avcodec_open2     [décodeurs ouverts]

boucle de lecture (thread UI, cadencée par l'horloge média)
  → av_read_frame
      ├─ paquet vidéo → avcodec_send_packet/receive_frame
      │     → sws_scale (YUV → RGBA) → egui TextureHandle → affichage
      └─ paquet audio → avcodec_send_packet/receive_frame
            → swr_convert (format du périphérique, f32 entrelacé)
            → tampon circulaire → callback cpal → carte son
```

### Horloge de lecture (cadence) — la carte son est maître

- L'application redessine **en continu** tant que la lecture est active.
- La décision « avancer d'une frame » est prise **en début de `update`**, en comparant le
  PTS de la dernière frame affichée à la **position média de référence**.
- Cette référence est l'**horloge audio** quand elle existe : `audio::Ancrage` mémorise la
  position média du premier échantillon poussé après une remise à zéro (seek, pause,
  changement de piste) et y ajoute le nombre de **trames que la carte son a réellement
  consommées** (`played_samples`, compté par le callback ; le silence de sous-alimentation
  n'est pas compté). Le retard du périphérique (`OutputCallbackInfo::timestamp().playback`)
  est retranché quand l'hôte le renseigne.
- Le **temps mural** (`clock_start` / `clock_base_pts_ms`, recalés sur le PTS réel de la
  première frame) ne sert plus que de **repli** : média sans audio, pas de sortie audio,
  ou horloge audio muette depuis plus de 250 ms (piste audio terminée, périphérique
  suspendu) — sinon l'image resterait figée sur une horloge arrêtée.
- `tick_audio()` (appelé à chaque tour de boucle, **avant** `next_frame_delay`) entretient
  deux choses : le garde-fou ci-dessus, et l'**alimentation** du tampon audio à sa
  profondeur cible. L'alimentation est indispensable : sans elle, on ne lirait un paquet
  que pour afficher une image, et l'horloge audio s'arrêterait faute de son à consommer
  (interblocage — constaté et corrigé, voir §6.24).
- `next_frame_delay()` est **auto-correcteur** : il vise `pts + intervalle média` (divisé
  par la vitesse), donc une erreur ponctuelle ne s'accumule pas.
- Conséquence mesurable : l'image est toujours **en retard d'une image au plus** sur le son
  (jamais en avance), et la profondeur du tampon n'a plus aucune influence sur cet écart.

---

### Sous-titres image (session 23)

- Les pistes **PGS** (Blu-ray), **sous-images DVD** et **DVB** sont décodées en bitmaps :
  le décodeur rend des rectangles d'**indices de palette**, convertis en RGBA une fois par
  événement (`bitmap_vers_rgba`). La palette livrée par FFmpeg est en **0xAARRGGBB**, déjà
  convertie du YCbCr d'origine.
- Composition à l'échelle de l'image, position dans le repère du média (1920×1080 pour un
  Blu-ray) reportée sur la zone affichée. Les réglages de police ne s'appliquent pas : la
  mise en forme vient du disque.
- **Extinction** : un flux PGS éteint son sous-titre par une composition **sans objet** ;
  elle termine l'événement précédent (`end_ms = début de la composition vide`).
- Mémoire bornée **par volume** (64 Mo), pas par nombre : une composition plein écran 1080p
  pèse 8 Mo.
- `--dump-images [--dump-images-pgm fichier.pgm]` : équivalent de `--dump-subs` pour les
  images — minutage, rectangles, nombre de pixels opaques, empreinte des pixels.

### Flux réseau et reconnexion (session 22)

- Une adresse (`http`, `https`, `rtsp`, `rtmp`, `smb`, `ftp`, playlist `.m3u8`) s'ouvre
  comme un fichier ; elle est acceptée en ligne de commande au même titre qu'un chemin.
- Un **EOF n'est pas forcément une fin** : le démultiplexeur HLS saute les segments en
  erreur puis rend `AVERROR_EOF`. La distinction se fait sur la **position** : un flux qui
  se termine loin de sa durée annoncée (ou sans durée connue, donc un direct) est une
  coupure.
- Sur coupure : lecture suspendue, bandeau « reconnexion en cours », tentatives à 0,3 s,
  0,6 s, 1,2 s… plafonnées à 8 s, 8 essais au total. Chaque essai commence par une **sonde
  en arrière-plan** (ouverture dans un thread, délai court) pour que l'interface reste
  réactive même si le serveur ne répond pas.
- Réouverture réussie → **reprise à l'image affichée** (utile pour une VOD ; sur un direct,
  le seek échoue et la lecture reprend au bord du direct, ce qui est voulu).
- Épuisement des essais → état d'erreur, message **affiché** dans le bandeau d'erreur.
- Les E/S sont bornées par un `interrupt_callback` (3 s par lecture de paquet, 10 s pour
  l'ouverture) : `rw_timeout` seul ne suffit pas, car le démultiplexeur HLS ouvre les
  segments avec ses propres contextes d'E/S.

---

## 4. Fonctionnalités

### Lecture
- Formats conteneurs gérés par FFmpeg : MP4, MKV, AVI, MOV, WMV, FLV, WebM…
- Décodage **matériel VA-API** (H.264, HEVC, VP9, AV1, MPEG-2, VC-1, MJPEG…),
  avec **repli logiciel automatique** si le pilote refuse le profil du flux (voir §6.10)
- Décodage logiciel H.264, HEVC, VP9, AV1*, MPEG-4, etc.
  (*attention : dans le FFmpeg 7.1 de `/opt`, le décodeur AV1 **n'a pas de chemin
  logiciel** — il exige VA-API ; voir §6.10)
- Détection automatique des flux vidéo / audio / sous-titres (premier de chaque type)
- Fin de fichier propre : purge du décodeur, état `Stopped`, **dernière image figée**
- Vitesse de lecture 0,1× – 8× (appliquée à l'horloge de cadence)

### Audio
- Sortie cpal (PipeWire / PulseAudio / ALSA), formats matériels **F32** et **I16**
- Conversion vers le **format du périphérique** (taux + canaux) via libswresample
- Tampon circulaire inter-thread, volume (0–150 %), mute
- Le flux audio est créé **une seule fois** pour toute la vie du programme
- Dégradation gracieuse si aucun périphérique n'est disponible

### Image
- Conversion YUV → RGBA via libswscale, **directement à la taille affichée**
  (la source n'est réduite qu'une fois ; jamais agrandie côté CPU)
- **Méthode de mise à l'échelle au choix** : Lanczos (défaut), Bicubique, Bilinéaire,
  Moyenne de zone, Plus proche voisin — appliquée côté CPU lors de la réduction
- Texture GPU **réutilisée** (pas de recréation à chaque frame)
- Mise à l'échelle avec **préservation du rapport d'aspect** (bandes noires si besoin)

### Sous-titres (fichiers externes)
- Formats **SRT** (SubRip) et **ASS/SSA**
- Chargement automatique : fichier homonyme de la vidéo (`.srt`, `.ass`, `.ssa`, `.sub`)
  puis sous-dossiers `subs/`, `subtitles/`, `sous-titres/`, `sub/`
- Rendu en surimpression : bande semi-transparente, texte centré, **contour noir**
- Sélection de la piste active, chargement manuel via le menu Sous-titres

### Interface
- Barre de menu (Fichier, Lecture, Audio, Sous-titres, Playlist, Préférences)
- **Barre de progression cliquable et glissable** : la position du curseur déclenche
  réellement une recherche (clic bref *et* glissement), avec limitation à ~6 seeks/s
- **Répétition A-B** : passage en boucle entre deux points (touches `A` / `B`)
- **Fenêtre toujours au-dessus** (touche `T`, via `_NET_WM_STATE` sous X11)
- **Masquage complet de l'interface** (touche `Tab`, mode présentation)
- **F1 : fenêtre d'aide des raccourcis** (table unique `SHORTCUTS` dans `ui.rs`)
- Barre de contrôles : play/pause, stop, précédent/suivant, barre de progression,
  volume, vitesse, mute, plein écran (masquable, ou maintenue en plein écran par option)
- **OSD** (`O`) : bandeau d'informations (média, position, décodage, audio, volume, vitesse)
- **Fenêtre de statistiques** (`I`) : résolution, codecs, mode de décodage, débit,
  taille du fichier, position, vitesse, volume
- **Panneau playlist** (`L`) : clic pour lire (index courant synchronisé), suppression (❌),
  ajout, vidage ; **restaurée au démarrage**
- **Capture d'écran** (`S`) : PNG horodaté dans le dossier configuré
- Glisser-déposer de fichiers, « Ouvrir… », « Ouvrir dossier… » (playlist + lecture du 1er)
- Configuration persistante — **uniquement des réglages réellement appliqués** (§6.11)

**Retiré car non implémentable dans le périmètre** (détail : `INVENTAIRE-FONCTIONNALITES.md` §3) :
détramage, mappage tonal HDR, interpolation de mouvement, synchronisation d'affichage,
profils qualité, égaliseur, normalisation du downmix, choix du back-end audio et table de
raccourcis re-mappables. Ces réglages étaient **présents dans le JSON et sans effet** : ils
ont été supprimés plutôt que laissés en place. La raison technique est unique : ils exigent
une **chaîne de filtres FFmpeg (`libavfilter`)**, que le projet n'implémente pas.

---

## 5. Raccourcis clavier

| Touche | Action |
|---|---|
| `Espace` | Lecture / pause |
| `←` / `→` | Reculer / avancer de 5 s |
| `Maj + ←/→` | Reculer / avancer de 1 s |
| `Ctrl + ←/→` | Reculer / avancer de 30 s |
| `↑` / `↓` | Volume +/- |
| `M` | Muet |
| `V` | Afficher / masquer les sous-titres |
| `F` ou `Entrée` | Basculer le plein écran |
| `Échap` | Quitter le plein écran |
| `S` | Capture d'écran (PNG dans `~/Images/Captures-video/`) |
| `O` | Afficher / masquer l'OSD |
| `I` | Afficher / masquer les statistiques |
| `L` | Afficher / masquer le panneau playlist |
| `F5` | Préférences (liste des sorties audio, mise à l'échelle, volume max…) |
| `F1` | Aide des raccourcis |
| `F2` | À propos — ⚠️ **pas `F12`** : F12 est réservé par le bureau (Yakuake sous KDE) et n'arrive jamais à l'application |
| `Échap` | Ferme la fenêtre ouverte (Préférences, Aide, Statistiques, À propos), sinon sort du plein écran |
| `[` `]` | Vitesse −0,1 / +0,1 — `\` revient à 1,0 (l'audio suit) |
| `A` / `B` | Répétition A-B : définir le point A / le point B (puis annuler) |
| `T` | Fenêtre toujours au-dessus des autres |
| `Tab` | Masquer / afficher toute l'interface |
| `J` | Piste de sous-titres suivante (fichiers externes) |
| `Ctrl+L` | Lecture en boucle de tout le média |
| `Ctrl+R` | Reprise à la position quittée (activer / désactiver) |
| `H` | Basculer décodage matériel ↔ logiciel (le média est rechargé à la même position) |
| `Q` | Quitter |

> **Ordre des motifs de touches :** dans `ui.rs`, un motif avec modificateur
> (`egui::Key::L if ctrl`) doit être placé **avant** le motif simple (`egui::Key::L`),
> sinon le premier gagne et Ctrl+L devient inatteignable.

---


---

# Bugs corrigés — sessions 2 à 31

> Les entrées sont classées de la plus récente à la plus ancienne. Les causes racines
> sont détaillées car plusieurs corrections « évidentes » se sont révélées inutiles.

### 6.36 Sous-titres : famille de police et position horizontale (session 31)

**Ce que la vérification de PotPlayer avait nommé.** La page `Dialog(383)` de son
`French.ini` liste `Police`, `Alphabet`, `Interlettre`, `Position hori. (%)`,
`Position verti. (%)`, `Ruby`… Or KPlayer ne réglait que la **taille** : la police était
celle d'egui (DejaVu en secours), et le texte ne pouvait être placé qu'au bord ou au
centre. Deux champs comblent l'écart le plus visible.

**Famille de police.** `subtitles.font_family` (nom connu de fontconfig, vide = police du
lecteur). La résolution se fait par **`fc-match`** plutôt qu'en ajoutant une dépendance
(`font-kit`, `fontdb`) pour une seule chaîne : fontconfig est présent sur tout bureau
Linux, et c'est lui qui décide déjà quelle police système correspond à un nom. Le fichier
TTF est ensuite chargé dans egui sous une **famille dédiée** (`soustitre`), distincte de
la police de secours : seul le sous-titre l'utilise, l'interface garde la sienne.

**Position horizontale.** `subtitles.position_h_percent` (50 % = centre, valeur par
défaut). Combinée à `align`, elle dit où est **ancré** le bloc : `align` = 0 place le début
du texte sur l'ancre, 1 le centre dessus, 2 sa fin. Avec la valeur par défaut, le calcul
redonne exactement l'ancien rendu (bord, ou centre) : aucune régression d'aspect.

**Deux pièges évités, tous deux déjà connus du projet.**
1. **Un réglage enregistré n'est pas un réglage appliqué** (§7.14) : sans appel explicite à
   `installer_police_sous_titre(ctx, …)` au moment du changement, la valeur serait écrite
   dans `config.json` sans qu'un seul pixel bouge. Les trois chemins (champ libre, sélecteur,
   retour au défaut) appellent donc l'installation, et le démarrage aussi.
2. **`egui::Context::set_fonts` remplace l'ensemble des polices** : réinstaller seulement la
   police des sous-titres aurait fait disparaître la **police de secours** (celle qui
   dessine `←→↑↓●`, corrigée en session 24). La fonction réinstalle donc les deux.

**Mesures** (`tools/verif-soustitres.sh`, 5 contrôles) :

| Contrôle | Mesure |
|---|---|
| Police par défaut / Liberation Mono / DejaVu Serif | largeur du texte **268 / 307 / 332** px — trois largeurs distinctes |
| Fichiers réellement chargés | `LiberationMono-Regular.ttf`, `DejaVuSerif.ttf` (journal) |
| Position horizontale 20 / 50 / 80 % | centre du bloc **262 / 638 / 1013** px |
| Police inexistante (`PoliceQuiNExistePasXYZ`) | aucun écran vide : substitut de fontconfig, trace au journal |
| Configuration **sans** les nouveaux champs | centre à **638** px : l'ancien aspect est conservé |

Un test unitaire de plus (95 au total) verrouille les valeurs par défaut (50 %, police
vide), pour qu'un fichier écrit avant ces champs reste lisible et ne change pas d'aspect.

**Reste de l'écart avec `Dialog(383)`** : `Alphabet` (limitation à un jeu de caractères),
`Interlettre`, `2nd contour`, `Contour flou`, `Ruby`. Aucun n'est planifié : ce sont des
détails de composition, alors que police et position se voient au premier coup d'œil.

---

### 6.37 Optimisations appliquées : échelle GPU, désentrelacement GPU, AV1 logiciel (session 31)

Les trois premières recommandations du comparatif mesuré sont **implémentées et mesurées**.

**1. Réduction d'échelle dans le GPU** (`scale_vaapi`). Quand le décodage est matériel et
qu'aucun filtre CPU n'est demandé, les surfaces VA-API sont réduites **avant** d'être
rapatriées, puis converties en RGBA à la taille finale. Mesuré : 1080p affiché en 720p
**10,85 → 6,3 ms/image (−42 %)** ; en 540p 11,55 → 5,06 (−56 %). Réglable (`video.gpu_scaling`,
défaut oui) et réversible à chaud (case à cocher) — le pilote utilise son filtre HQ, plus doux
que Lanczos (PSNR 30,7 dB sur mire), ce que le journal et l'infobulle disent explicitement.

**2. Désentrelacement matériel** (`deinterlace_vaapi`). 1080i affiché en 720p :
**11,05 → 6,8 ms/image (−38 %)**, c'est-à-dire le coût du désentrelacement devenu quasi nul.
Le filtre n'est déporté que s'il est **seul** dans la chaîne (tout autre filtre n'a pas
d'équivalent VA-API ici : `procamp_vaapi` échoue, « filter caps: 20 – not implemented »).

**3. AV1 logiciel** (`libdav1d`). FFmpeg 7.1.3 recompilé avec `--enable-libdav1d` (mêmes
options par ailleurs) : AV1 **enfin décodable en logiciel** — le décodeur natif de FFmpeg 7.1
échoue sur un AV1 réel (« Error submitting packet to decoder: Function not implemented »).
Mesuré : 179 i/s en 720p, 2,60 ms/image.

**Détail d'implémentation qui compte.** La chaîne GPU est **conservée entre les images**
(`GpuChain`, dans `VideoStream`) : reconstruire un graphe par image coûtait le gain, et un
désentrelaceur a besoin d'un état (reconstruit à chaque fois, il ne sort rien — constaté :
`EAGAIN` permanent). `avfilter_graph_create_filter` refuse un `buffer` sans arguments : il
faut `avfilter_graph_alloc_filter` + `av_buffersrc_parameters_set` + `avfilter_init_str`, et
`frame->time_base` vaut souvent `0/1` (le passer tel quel donne « Invalid argument »).

**Régression trouvée et corrigée** (elle justifie la suite complète) : la première version
sautait la chaîne CPU quand `transferred` était non nul — or il l'est **toujours** après le
rapatriement simple, donc **tous les filtres CPU étaient ignorés en décodage matériel**
(recadrage et miroirs perdus : 2 échecs dans `verif-personnalisation.sh`). Corrigé par un
drapeau explicite `filtres_faits_par_gpu`, et le banc repasse à 19/19.

Nouveau banc `tools/bench-optimisation.sh` (4 contrôles, intégré à `tout-verifier.sh`) : il
chiffre les deux chemins GPU, vérifie que la géométrie ne change pas (PSNR > 25 dB) et que
l'AV1 se décode en logiciel.

### 6.1 CRASH `SIGABRT` + `kwin_killer` (2026-10-16, session 5)

**Symptôme.** `Signal: 6 (ABRT)` avec core dump, à la lecture d'une vidéo.
Deux instances ont planté à la même seconde.

**Investigation.** Les cores ne contenaient **aucun message de panic** Rust. Le rapport
du noyau ne montrait qu'un `syscall`. En croisant le core, le binaire et le journal :

- le thread principal était bloqué dans un **futex** ;
- la pile pointait vers une zone contenant la chaîne
  `"deadlock in SIGSEGV handler / fatal runtime error: a thread received SIGSEGV"` ;
- le journal affichait `kwin_killer_helper_x11: Failed to resolve exe of pid …`.

**Chaîne d'événements reconstituée.**

1. Un **SIGSEGV** survient dans le processus.
2. Le handler de SIGSEGV de la **bibliothèque standard Rust** prend des verrous pour
   rédiger son rapport. Un second thread fautif, ou un verrou déjà tenu, provoque
   l'**interblocage du handler**.
3. Le processus ne meurt donc pas : il **gèle** (« la fenêtre ne répond pas »).
4. KWin ferme la fenêtre → le processus est tué → core en **SIGABRT**.

Autrement dit, le SIGABRT n'était **pas le bug** : c'était la conséquence du gel.

**Correctif — `src/crash.rs` (nouveau).**

- Handler maison pour `SIGSEGV`, `SIGBUS`, `SIGILL`, `SIGFPE`, `SIGABRT`, installé au
  début de `main`.
- **Async-signal-safe** : descripteur ouvert à l'avance, `write(2)` uniquement,
  **aucun verrou ni allocation** (tampons sur la pile), pile alternative `SA_ONSTACK`
  de 256 Kio (indispensable en cas de débordement de pile).
- Consigne dans `~/.cache/kplayer/crash.log` : `exe_base`, signal, `fault_addr`,
  `si_code`, registres (`rip`, `rip_rel`, `rsp`, `rbp`, `rax`, `rdi`) et un **vidage
  brut de la pile** autour de RSP.
- Rétablit le handler par défaut puis relance le signal : **le core dump reste produit**.
- Symbolisation immédiate :
  `addr2line -f -C -e target/release/kplayer <rip_rel>`
- Option `--crash-test` pour valider le dispositif (testé : `signal=11`,
  `fault_addr=0x0`, code de sortie 139, core toujours généré).

**Correctifs de robustesse audio (causes probables du gel).**

1. Le callback audio **ne bloque plus jamais** : `try_lock` sur le tampon circulaire
   (silence si le tampon est occupé). Avant, il pouvait attendre un verrou détenu par
   le thread d'interface → inversion de priorité et gel possible.
2. **Plus d'appel de contrôle bloquant** (`stream.play()` / `pause()`) depuis
   l'interface : ces appels peuvent attendre le thread audio et donc figer l'UI.
   Un simple drapeau atomique suffit ; le callback émet du silence à l'arrêt.
3. Le **flux audio est créé une seule fois** pour toute la vie du programme, au lieu
   d'un flux par média. Plus de création/destruction ALSA-PipeWire à chaque
   chargement (source d'erreurs et de blocages). La sortie n'est plus détruite à la
   fermeture d'un média.

**Bug de vitesse corrigé au passage.** Les échantillons étaient convertis au format
**du fichier** alors que le callback les consomme au format **du périphérique** : un
fichier **mono** joué sur une sortie **stéréo** défilait donc **2× trop vite**.
La conversion cible désormais le taux et le nombre de canaux du périphérique
(`44100 Hz, 2 canaux`), ce que confirme la stabilité du tampon (30–90 ms).

### 6.2 🔴 Cause racine majeure — structs FFmpeg redéfinies à la main (session 2)

**Symptôme.** Crash systématique à la lecture, en debug (`misaligned pointer
dereference`) **et** en release (`SIGSEGV`).

**Cause réelle.** `src/player.rs` **redéfinissait manuellement toutes les structs
FFmpeg** avec des dispositions incorrectes, alors que la dépendance `ffmpeg-sys-next`
était déclarée mais **jamais utilisée** :

| Élément | Valeur locale (fausse) | Valeur réelle FFmpeg 7.x |
|---|---|---|
| `AVFormatContext` | `nb_streams` à l'offset 0 | `av_class` (pointeur) à l'offset 0, `nb_streams` à ~44 |
| `AVPixelFormat::RGBA` | 53 | **26** |
| `AVMediaType::Subtitle` | 2 | **3** |
| `AV_CODEC_ID_HEVC` | 183 | **173** |
| `AV_CODEC_ID_AAC` | 86 | **86018** |

Conséquence : on lisait un pointeur comme un `u32` → valeur aberrante → segfault.
En outre, `avcodec_open2()` n'était **jamais appelé** (décodeurs non initialisés) et
`avcodec_parameters_to_context()` non plus.

Les corrections précédentes (copies de pointeurs, `read_unaligned`,
`copy_nonoverlapping`) ne pouvaient pas fonctionner : elles s'appuyaient sur des
dispositions fausses. L'adresse « mal alignée » était en réalité un décalage de champ
erroné.

**Correctif.** `src/player.rs` **entièrement réécrit** avec les vrais bindings
`ffmpeg_sys_next::*`, en ajoutant `avcodec_parameters_to_context` + `avcodec_open2`,
`av_packet_alloc`/`free`, `avcodec_flush_buffers` au seek et `sws_scale` →
`AV_PIX_FMT_RGBA`.

### 6.3 Pipeline de rendu jamais branché (session 2)

La fonction `get_current_frame()` n'était **jamais appelée** par l'interface : rien ne
s'affichait. Corrigé dans `App::update` (décodage en continu + `request_repaint`), et
la fausse conversion BGRA → RGBA a été retirée (les frames sortent déjà en RGBA).

### 6.4 Sortie audio absente (session 3)

`src/audio.rs` reposait sur rodio et n'était **jamais appelé**. Réécrit autour de
**cpal** : tampon circulaire partagé, volume atomique, gestion de la sous-alimentation
et du débordement. Côté `player.rs`, ajout d'un **SwrContext** pour convertir le format
du décodeur en f32 entrelacé. Support des formats matériels F32 et I16.

### 6.5 Cadence de lecture et dérive (session 3)

Le décodage se faisait « au plus vite » : la vidéo défilait trop vite. Mise en place de
l'horloge média décrite au §3.

> **Attention perf :** le pipeline coûte ~35 ms/frame en **debug** (plafonné à ~28 fps).
> En **release**, ~6 ms/frame → dérive nulle. **Toujours tester la lecture en release.**

### 6.6 Seek imprécis + audio désynchronisé après seek (session 3)

`avformat_seek_file(..., -1, ...)` atterrissait jusqu'à 3 s avant la cible (structure
GOP), et l'audio décodé pendant le rattrapage s'accumulait (tampon > 2 s).

Correctifs : seek ciblé sur le **flux vidéo** via `av_rescale_q` dans sa base de temps,
et champ `seek_target_ms` qui fait **jeter frames vidéo et paquets audio** antérieurs à
la cible (décodage-saut) jusqu'à l'atteinte exacte.

### 6.7 Sous-titres jamais affichés + parsing ASS erroné (session 4)

1. `SubtitleManager` existait mais **n'était jamais instancié** par l'interface.
2. **Bug de conversion** : les timestamps ASS (centisecondes) étaient traités comme des
   millisecondes → décalage d'un facteur 10 (`00:00:01.50` donnait 150 ms au lieu de
   1500 ms).
3. Format **SRT** non géré malgré les extensions annoncées.

Correctifs : `parse_timestamp()` corrigé (`(h*3600 + m*60 + s)*1000 + centis*10`),
ajout de `parse_srt_content()` / `parse_srt_timestamp()` (gère `,` et `.`, blocs
multi-lignes) + nettoyage des balises HTML, instanciation du gestionnaire dans `App`
avec chargement automatique, et rendu `draw_subtitles()`.

### 6.8 Plein écran incomplet (session 4)

La bascule ne fonctionnait que dans un sens, l'interface restait affichée et la vidéo
gardait sa taille native. Correctifs : état interne piloté par `F`/`Entrée` (bascule) et
`Échap` (sortie), masquage des panneaux, option CLI `--fullscreen` (demande envoyée au
premier tour de `update`, sinon trop tôt pour le viewport), et mise à l'échelle
`scale = min(dispo.w / l, dispo.h / h)`.

### 6.9 Playlist inerte (session 4)

Les boutons ne faisaient rien et l'élément courant était codé en dur (`i == 0`).
Ajout de `playlist_index()`, `play_index()`, `remove_from_playlist()`,
`playlist_index_advance()` ; l'UI utilise `selectable_label`, permet le clic pour lire
et ❌ pour retirer ; **enchaînement automatique** en fin de média.

### 6.10 Décodage matériel VA-API (session 7)

**Ajouté** : le décodeur vidéo est désormais ouvert sur un périphérique VA-API
(`av_hwdevice_ctx_create`) avec un callback `get_format` qui choisit
`AV_PIX_FMT_VAAPI`. Les surfaces matérielles sont rapatriées en mémoire système
(`av_hwframe_transfer_data`) puis converties en RGBA par libswscale.

**Repli automatique** : le pilote Mesa refuse certains profils (typiquement le H.264
« pleine échelle » `yuvj420p` des vidéos de téléphone). `avcodec_open2()` échoue alors,
et le flux est rouvert en logiciel — sans message d'erreur pour l'utilisateur. Le repli
est donc **réel mais silencieux** : l'interface affiche le mode réellement constaté, et
non celui demandé (une frame VA-API a-t-elle été produite ?).

**Mesures** (`tools/bench-hwdec.sh`, fenêtre 1280×720, 16 cœurs) :

| Fichier | Logiciel | VA-API |
|---|---|---|
| H.264 1280×720 30 i/s | 22 % | 22 % |
| H.264 3840×2160 30 i/s | 56 % | **37 %** |
| AV1 3840×2160 60 i/s | impossible* | **52 %** |
| H.264 1920×1080 `yuvj420p` (caméra) | 28 % | 27 % (= repli logiciel) |

\* Dans le FFmpeg 7.1 de `/opt`, le décodeur AV1 natif (`av1dec.c`) **ne contient pas de
décodeur logiciel** : sans accélération matérielle il renvoie `ENOSYS`
(« Your platform doesn't support hardware accelerated AV1 decoding »). Un fichier AV1
était donc **injouable avant cette session** ; il l'est maintenant, à 52 % de CPU.
Le même constat a été vérifié avec le binaire `ffmpeg` en ligne de commande, donc il ne
s'agit pas d'un défaut du lecteur.

**Deuxième gain, indépendant du décodage** : la conversion RGBA se fait désormais à la
**taille affichée** au lieu de la taille native. Sur une source 2160p c'est ~28 % de CPU
en moins ; le contexte swscale est reconstruit si le format ou la taille changent
(hystérésis de 16 px pour ne pas recalculer à chaque frame).

**Correction associée** : l'interface calculait l'échelle d'affichage à partir des
dimensions du tampon de frame. Comme celui-ci peut être déjà réduit, le rapport d'aspect
était appliqué deux fois (image étirée d'environ 3 %). L'échelle part maintenant de la
**résolution native** du média (`MediaInfo.resolution`).

**Option** : `--no-hwdec` force le logiciel ; touche `H` bascule en direct (le média est
rechargé à la position courante, car le mode de décodage se choisit à l'ouverture du
flux) ; `video.hardware_decoding` est persisté dans la configuration.

### 6.11 Fonctions annoncées mais absentes — corrigées (session 9)

Un audit du code (session 8) a montré que plusieurs fonctions étaient **annoncées sans
exister** : la capture d'écran n'écrivait aucun fichier, l'OSD n'était jamais dessiné, le
panneau playlist était inatteignable, et **12 réglages** de la configuration n'étaient lus
par aucun chemin d'exécution. Tout a été soit implémenté, soit retiré.

**Implémenté pour de bon**

| Fonction | Ce qui a été fait |
|---|---|
| Capture d'écran | encodage PNG réel (crate `png`, pur Rust), nom horodaté, dossier configurable |
| OSD | bandeau dessiné (`egui::Area`), contenu issu de l'état réel du moteur, durée configurable |
| Panneau playlist | accessible par la touche `L` et le menu ; clic = `play_index()` (index courant synchronisé — c'était un bug) |
| « Ouvrir dossier… » | constitue une playlist et lit le **premier** fichier (avant : tous ouverts, seul le dernier restait) |
| Statistiques | affichent en plus la taille du fichier et la présence audio |
| **Mise à l'échelle** | méthode choisie **réellement transmise** à `sws_getContext` (voir vérification ci-dessous) |
| **Volume maximal** | plafonne réellement `set_volume` |
| **Sortie audio** | liste des périphériques cpal, choix mémorisé et appliqué |
| **Vitesse de lecture** | l'audio est **rééchantillonné** en même temps que la vidéo (fini la désynchronisation) |
| **Reprise de lecture** | positions enregistrées toutes les 10 s + à la fermeture, reprises au chargement |
| **Playlist persistante** | restaurée au démarrage (était sauvegardée… mais jamais relue) |
| **Langue des sous-titres** | le suffixe de langue est prioritaire (`.fr.srt` avant `.srt`) |

**Vérification de la mise à l'échelle** (damier 4K réduit à ~900 px, `sws_scale`) —
écart-type de la luminance dans la zone vidéo :

| Méthode | Écart-type | Interprétation |
|---|---|---|
| Lanczos | 1,8 | damier correctement moyenné (le plus net) |
| Moyenne de zone | 6,6 | lissage un peu plus doux |
| Plus proche voisin | 121,2 | crénelage massif (attendu : un damier n'est pas moyenné) |

**Vérification de la vitesse audio** : `Audio rééchantillonné pour 1.20x (44100 Hz → 36750 Hz)`.

**Retiré, avec la raison** (aucun de ces réglages n'était lu ; les garder aurait été mentir) :
détramage, HDR, interpolation de mouvement, synchronisation d'affichage, profils qualité,
égaliseur, normalisation du downmix, choix du back-end audio, table de raccourcis.
**Raison unique et technique :** ils exigent une chaîne de filtres **FFmpeg (`libavfilter`)**,
que le projet n'implémente pas. Les champs morts correspondants ont aussi été supprimés
(`MediaInfo::duration_ms`, `VideoStream::width/height`, `utils::get_system_info`…).

**Résultat mesurable :** plus aucun avertissement du compilateur (`never used`/`never read`),
et la configuration ne contient plus que 6 sections appliquées :
`video`, `audio`, `subtitles`, `interface`, `playback`, `playlist`.

### 6.12 Audit de l'interface et raccourcis « façon PotPlayer » (session 10)

**Audit automatisé** (`tools/audit-ui.sh`) : la fenêtre est placée à une taille connue,
photographiée, et **les zones de boutons sont détectées dans l'image** (aucune coordonnée
devinée). Chaque commande est validée par le journal ; les bascules purement visuelles
(OSD, statistiques, aide, playlist) sont validées par comparaison d'images avant/après.
**Résultat : 44/44.**

**Trois bugs réels trouvés par cet audit :**

1. **La barre de progression n'était reliée à rien.** Sa valeur n'était lue par aucun
   chemin : cliquer ou glisser ne produisait aucun seek (seul le sélecteur bougeait).
   Elle déclenche désormais une recherche (au clic **et** pendant le glissement), et un
   clic très bref est traité à partir des événements de pointeur bruts (egui exige que le
   bouton soit maintenu au moins une image pour voir un curseur).
2. **La répétition A-B ne se déclenchait jamais en lecture normale.** Le contrôle
   n'existait que dans la boucle des frames déjà en file, pas dans la boucle de décodage à
   la demande (celle qui alimente la lecture courante). Le contrôle est maintenant
   centralisé (`accept_for_display`), appelé pour toute frame rendue.
3. **`T` (toujours au-dessus) tuait le lecteur.** `ViewportCommand::WindowLevel`
   d'eframe/egui-winit fait un `.expect()` sur une requête X11 : quand elle échoue, le
   processus **panique**. Vérifié : la touche `T` était reçue une fois puis l'application
   disparaissait. Remplacé par un envoi direct du message EWMH `_NET_WM_STATE`
   (`src/x11.rs`, x11rb), qui renvoie une erreur au lieu de planter.

**Deux pièges de saisie corrigés :**

- **Les modificateurs doivent être lus sur l'événement** (`Event::Key { modifiers, .. }`)
  et non sur l'état global `i.modifiers` : si l'appui et le relâchement de Ctrl arrivent
  dans la même image, l'état global est déjà revenu à « relâché » et **un raccourci sur
  deux était perdu** (constaté : `Ctrl+L`, `Ctrl+R`).
- **Le focus clavier s'obtient par un clic dans la fenêtre** : sur X11 réel (KWin), une
  simple activation ne suffit pas. Utile à savoir pour tout test automatisé — et
  `xdotool key --window` est ignoré par winit.

**Raccourcis ajoutés** (tous validés par l'audit) : `T`, `Tab`, `F1`, `A`, `B`, `J`,
`Ctrl+L` (boucle), `Ctrl+R` (reprise).

### 6.14 Animations ASS : `\move`, `\t`, karaoké (session 20)

Le rendu ASS statique (session 19) est complété par les **animations**, qui sont
l'essentiel d'un sous-titre de fansub ou de clip.

| Balise | Effet | Mesure |
|---|---|---|
| `\move(x1,y1,x2,y2[,t1,t2])` | déplacement linéaire ; sans bornes = tout l'événement | centre x **23,0 % → 79,0 %** |
| `\t(t1,t2,\fs…)` | taille progressive (interpolée) | texte **4108 → 13249** pixels |
| `\t(t1,t2,\c…)` | couleur progressive | blanc → rouge vérifié à mi-parcours |
| `\k` | karaoké **net** (bascule au début de la syllabe) | bascule à 10 ms de la borne |
| `\kf` / `\K` | karaoké **balayé** (gauche → droite) | **798 → 1267** pixels chantés |
| `\2c` | couleur secondaire (syllabe non chantée) | jaune présent, puis **0** à la fin |

**Trois défauts trouvés par les tests, tous de vrais défauts :**

1. **Le nom des balises était tronqué à trois lettres** : `\move` était lu `mov` et son
   argument `e(0,0,200,100)` n'était plus un nombre, donc `\move` et `\kf` n'étaient
   **jamais** analysés. Correction : table des noms connus (`BALISES_CONNUES`), comparée
   **avec la casse** — `\K` et `\k` sont deux balises différentes.
2. **Le karaoké était décalé d'une syllabe** : la durée était assignée avant la lecture de
   la balise, donc « Ka » sortait à 0 ms et la syllabe suivante héritait de la précédente
   (mesuré : `Ka:0+2000` puis `ra:0+2000`). Correction : assigner **après** la balise, et
   remettre à zéro à la clôture du fragment.
3. **Mes propres tests se trompaient** : avec l'alignement « bas-centre », `\move` donne le
   **centre** du bloc, pas son coin gauche, et `y` est le haut du bloc, pas sa ligne de
   base. Deux tests échouaient alors que le code était juste.

**Un défaut d'outillage, plus insidieux** : mes trois captures d'écran destinées à vérifier
les animations étaient **identiques**. Cause : l'option `--seek` n'existait pas — le
lecteur ignorait silencieusement l'argument et chaque capture était prise au même instant.
`--seek <s>` (ou `--seek 1:30`) est donc ajouté : utile à l'utilisateur, et indispensable à
toute vérification temporelle. Il devait aussi être appliqué **après** le chargement du
média (avant, il n'y a pas de contexte de format et le déplacement est sans effet).

**Vérification** : `tools/verif-ass.sh` (**17/17**, dont 6 nouveaux contrôles d'animation) et
**21 tests unitaires** dans `src/ass.rs`.

### 6.15 Profils de réglages et rendu ASS par événement (session 19)

Deux chantiers : **des profils nommés** (pour ne plus refaire dix réglages à chaque film)
et le **rendu ASS par événement** (pour qu'un fichier ASS affiche sa propre mise en page).

#### Profils de réglages

| Fonction | Détail |
|---|---|
| Contenu | vidéo (filtres, géométrie, cadrage, aspect), audio (égaliseur, traitements), sous-titres (style, langue) |
| Enregistrer / appliquer / supprimer | depuis *Préférences ▸ Profils de réglages* |
| Export / import | fichier JSON, partageable |
| Ligne de commande | `--profil <nom>` (un raccourci par type de contenu), `--list-profiles` |
| Robustesse | fichier illisible → liste vide, **la lecture continue** ; profils en double → dédoublonnés |

**Une décision : le profil n'embarque pas le périphérique audio.** Un profil partagé
couperait le son chez celui qui l'importe. La valeur de la machine est préservée à
l'application (contrôlé par le banc d'essai).

**Défaut trouvé par la mesure** : un profil écrit à la main avec trois champs était
**rejeté en bloc** (`missing field auto_load`) — donc ignoré sans que rien ne le dise.
`#[serde(default)]` sur chaque structure de réglages permet désormais à tout champ absent
de prendre sa valeur par défaut. C'est un cas réel : importer le profil d'une version
antérieure.

#### Rendu ASS par événement (`src/ass.rs`)

Un ASS n'est pas un SRT : il porte une **mise en page**. Le lecteur analyse maintenant
`[Script Info]` (dont `PlayResX/Y`), `[V4+ Styles]` et chaque `Dialogue:`.

| Élément | État |
|---|---|
| `\pos(x,y)` | ✅ position absolue, mesurée à l'écran |
| `\an` / `\a` | ✅ alignement pavé numérique (y compris l'ancien codage SSA) |
| `\c`, `\1c`, `\3c`, `\alpha` | ✅ couleurs (BGR), alpha inversé |
| `\fs`, `\b`, `\i`, `\bord` | ✅ taille, graisse, italique, épaisseur de contour |
| `\fad(entrée,sortie)` | ✅ fondu, mesuré à mi-parcours |
| Multi-lignes `\N` | ✅ empilées dans l'ordre |
| Fragments de couleur | ✅ changement de style au milieu d'une ligne |
| `BorderStyle=3` (cadre) | ✅ rectangle opaque derrière le texte |
| Mise à l'échelle `PlayRes` | ✅ un ASS conçu pour 384×288 s'affiche à la bonne taille partout |
| `\move`, `\t`, `\k`, `\clip` | ❌ ignorés **proprement** (le texte reste lisible) |
| `\p` (dessins vectoriels) | ❌ événement écarté (sinon les coordonnées s'afficheraient en clair) |

**Trois pièges du format**, tous couverts par des tests unitaires (**14/14**) :

1. **Les couleurs sont en BGR** : `&H0000FF&` est **rouge**, pas bleu. Et `&HFF&` est
   **rouge aussi** (les octets de poids fort manquants valent zéro) — je m'étais trompé
   là-dessus dans mon propre test.
2. **L'alpha est une transparence** : `&H00…` = opaque, `&HFF…` = invisible.
3. **Tout est en unités de `PlayRes`**, pas en pixels écran : sans mise à l'échelle, un
   sous-titre est illisible ou géant.

**Le défaut le plus coûteux** : egui **fige la couleur dans la galerie** lors de la mise
en page ; l'argument couleur de `painter.galley` n'est qu'un repli. Une galerie créée
blanche dessinait donc **tout en blanc**, quelles que soient les couleurs du fichier —
mesuré : rouge et jaune à **0 pixel**. Correction : galerie en `Color32::PLACEHOLDER` +
`galley_with_override_text_color`. Sans la mesure de pixels, ce bug serait passé pour un
« rendu qui marche ».

**Vérification (`tools/verif-ass.sh`, 11 contrôles)** : 14 tests unitaires ; `--ass-dump`
sur un fichier de référence ; à l'écran, `\pos(192,60)` mesuré à **18,1 %** de la hauteur
et `\pos(192,200)` à **70,1 %** ; titre `\an8` à **10,7 %** (preuve que la mise en page du
fichier sert, le rendu uniforme plaçant toujours en bas) ; taille du texte = celle du
fichier, sans double rendu.

### 6.16 Personnalisation avancée : chapitres, cadrage, style des sous-titres, URL (session 18)

**Objectif** : combler les manques de personnalisation face à PotPlayer/VLC. Sept
fonctions ajoutées, **chacune mesurée**.

| Réglage | Ce que ça fait | Mesure (`tools/verif-cadrage.sh` et `verif-pistes.sh`) |
|---|---|---|
| **Chapitres** | lus dans le conteneur (titres compris), menu *Lecture*, **Page↑/Page↓** | 3 chapitres lus avec leurs titres |
| **Recadrage des bandes noires** | 4 valeurs (haut/bas/gauche/droite), filtre `crop` | 1280×720 → **1280×540** pour 90 px retirés |
| **Détection automatique des bandes** | analyse de plusieurs images, **médiane** par bord | **haut=90 bas=90** exact ; **0** sur un clip sans bandes |
| **Rapport d'aspect imposé** | Auto / 16:9 / 4:3 / 1,85 / 2,35 / Remplir / Personnalisé, touche **K** | largeur affichée **1251 → 937 px** en 4:3 |
| **Style des sous-titres** | couleur, contour (épaisseur), bandeau, taille, position verticale, alignement, gras, italique | pixels rouges **16242 vs 0** ; texte en haut **12 %** vs en bas **82 %** ; 18 pt → 48 pt = **3848 → 15418** pixels |
| **Décalage des sous-titres** | −10 s à +10 s (fichiers mal minutés, pistes du conteneur aussi) | décalé de 5,5 s : texte **hors fenêtre** (15418 → 60 pixels) |
| **URL réseau** | `http(s)`, HLS, RTSP, RTMP, SMB, FTP : fenêtre *Ouvrir une adresse* | ouverture par FFmpeg (pas de yt-dlp : hors périmètre) |
| **Image par image** | touches **`,`** et **`.`** (comme VLC) | avance d'une durée de trame, affichage forcé |

**Décisions de conception, pas des détails :**

1. **La détection des bandes noires se fait par analyse d'image, pas avec `cropdetect`.**
   Ce filtre ne communique son résultat que par un message de journal, qu'il faudrait
   analyser — fragile et invérifiable. Ici : quelques images réparties dans le film,
   mesure directe des lignes/colonnes sombres **et uniformes** (écart-type faible : un
   ciel nocturne est sombre mais bruité, donc pas une bande), puis **médiane** des
   mesures. La médiane est essentielle : le maximum, essayé d'abord, recadrait tout le
   film à cause d'un seul plan noir (mesuré : 68 px détectés sur un dégradé sans bandes).
2. **Une bande de moins de 1 % de la dimension est ignorée** : c'est du contenu sombre
   en bord d'image, pas une bande — la rogner ferait perdre de l'image utile.
3. **« Automatique » ne déforme rien.** Les autres modes déforment **volontairement**
   (corriger un encodage mal étiqueté, remplir l'écran) et l'interface le dit.

**Deux pièges de méthode :**

- **Une mire `testsrc2` ne permet pas de mesurer une couleur** : elle contient déjà du
  rouge, si bien que le test « la couleur du texte est appliquée » échouait alors que le
  code était bon (133 851 pixels rouges avec un texte blanc !). Il faut un **fond uni
  gris** comme média de test.
- **Le contour à 4 directions laisse des trous** dès 2 px d'épaisseur : 8 directions
  au-delà de 1 px.

### 6.17 Pistes embarquées : audio et sous-titres du conteneur (session 17)

**Ce qui manquait.** Le moteur prenait le **premier** flux audio et ignorait les
sous-titres du conteneur. Un MKV courant contient pourtant plusieurs langues audio et
plusieurs pistes de sous-titres : il fallait remuxer le fichier pour changer de version.

**Ce qui a été fait.**

| Fonction | Détail |
|---|---|
| Inventaire des pistes | `TrackInfo` : index de flux, codec, `language`, `title`, canaux, taux. Journalisé au chargement |
| Choix de la piste audio | langue préférée (`fre`/`fr` → `fre`), sinon **première piste** (aucune régression) |
| Changement **à chaud** | `select_audio_stream` : décodeur, convertisseur et chaîne de filtres reconstruits, tampon vidé |
| Sous-titres embarqués | `select_subtitle_stream` + décodage des paquets en événements (`EmbeddedCue`) |
| Priorité d'affichage | **fichier externe d'abord**, puis la piste du conteneur |
| Menus | « Audio » liste les pistes avec pastille d'actif + langue préférée ; « Sous-titres » liste les pistes du fichier, permet de les désactiver |
| Ligne de commande | `--list-tracks`, `--audio-track <flux>`, `--sub-track <flux>`, `--dump-subs <fichier>` |

**Vérification (`tools/verif-pistes.sh`, 16 contrôles) — sur un MKV construit pour être
mesurable** : deux pistes audio de fréquences différentes (300 Hz « eng », 900 Hz « fre »)
et deux pistes SRT (fr/en).

| Contrôle | Résultat |
|---|---|
| Inventaire | ✔ quatre pistes annoncées, langues et titres lus |
| Contenu des pistes | ✔ piste 1 = **300,0 Hz**, piste 2 = **900,0 Hz** (FFT) |
| Langue préférée | ✔ « fre » → 900 Hz, « eng » → 300 Hz |
| Texte des sous-titres | ✔ « Ligne française un » (une seule piste à la fois) |
| Minutages | ✔ **1000→3000, 4000→6000** |
| Désactivation | ✔ aucune piste extraite |
| Affichage réel | ✔ écart d'image 12,94/255 entre activé et désactivé |
| Changement à chaud (interface) | ✔ clic dans le menu → « flux 2 — fre — Français », **lecture non interrompue** |

**Deux défauts trouvés par la mesure :**

1. 🔴 **Les durées de sous-titres étaient fausses** (3 s par défaut au lieu de 2 s) : le
   décodeur SubRip ne renseigne pas `end_display_time` sur un MKV, la durée est dans
   **`duration` du paquet** (unités de la base de temps du flux). Mesuré avant correction :
   `1000→4000` au lieu de `1000→3000`.
2. **Le banc d'essai n'activait pas `kinterface=debug`** : les menus n'étaient pas tracés,
   donc aucun test d'interface possible — la correction a consisté à aligner le niveau de
   journal, pas le code.

**Limite assumée** : les sous-titres **image** (PGS des Blu-ray, DVD) sont détectés et
signalés (« image, non affichable ») mais **non rendus** : cela demande de décoder des
bitmaps et de les composer sur la vidéo, un chantier distinct.

### 6.18 Traitement audio et vidéo étendus — parité avec PotPlayer (session 16)

**Objectif.** Compléter l'égaliseur (session 15) avec les traitements que PotPlayer
propose, en n'ajoutant **que** ce que ce build FFmpeg sait réellement faire, et en
mesurant chaque ajout.

**Audio — huit traitements ajoutés** (`superequalizer` existait déjà) :

| Réglage | Filtre | Mesure (`tools/verif-audio.sh`) |
|---|---|---|
| Conserver la hauteur à vitesse ≠ 1 | `atempo` | 440 Hz restent **440,0 Hz à 2×**, durée 6,02 s → 3,00 s |
| Normaliseur de volume | `dynaudnorm` | écart fort/faible **24,4 → 9,0 dB** |
| Mode nuit (compression) | `acompressor` + `alimiter` | écart **24,4 → 13,8 dB**, crête 0,950 |
| Réduction de bruit (3 niveaux) | `afftdn` | rapport signal/bruit **19,7 → 45,1 dB** |
| Élargissement stéréo (3 niveaux) | `extrastereo` | écart entre canaux **0,097 → 0,202** |
| Crossfeed (casque) | `crossfeed` | écart **0,097 → 0,042** |
| Suppression de la voix (karaoké) | `pan` | son centré : **0,0031 %** du niveau |
| Réverbération (3 tailles) | `aecho` | queue après impulsion **0,00002 → 0,0154** |
| Décalage audio ±2 s | `adelay` / `atrim` | +500 ms → premier son à **510 ms** |

**Vidéo — quatre ajouts** (`tools/verif-filtres.sh`, 12 contrôles) :

| Réglage | Filtre | Mesure |
|---|---|---|
| Netteté (3 niveaux) | `unsharp` | énergie haute fréquence **17,6 → 44,5** |
| Rotation 0/90/180/270° | `transpose` | dimensions **1280×720 → 720×1280** |
| Miroir vertical | `vflip` | (même graphe) |
| Zoom numérique 1–4× | `crop` + `scale` | écart à l'original **32/255**, dimensions conservées |

**Ordre des filtres — raisonné, pas arbitraire** (`player::filtres_audio`) :
`atempo` → réduction de bruit → égaliseur → réverbération → outils stéréo →
compression/normalisation → **limiteur de sécurité** → décalage. Justification : nettoyer
avant d'amplifier, corriger la courbe avant la dynamique, compresser avant de normaliser
(sinon le normaliseur annule la compression), et limiter en dernier.

**Trois pièges trouvés par la mesure :**

1. 🔴 **`alimiter` avec son option « auto level » (activée par défaut) NORMALISE la
   sortie** : un égaliseur neutre sortait 0,45 dB plus bas, et les mesures de gain
   étaient faussées d'autant. `level=false` rend le limiteur transparent tant qu'aucune
   crête ne dépasse `limit` — vérifié : 0,5 reste 0,5, une crête à 1,4 tombe à 0,98.
2. **`dynaudnorm` converge lentement** : avec les valeurs par défaut (fenêtre 500 ms,
   lissage 31) il lui faut **6 s** pour atteindre son gain ; sur un extrait court, on
   croit qu'il ne fait rien. Fenêtre 200 ms + lissage 11 : **2,5 s** pour un résultat
   équivalent.
3. **`--dump-audio` déclarait le taux du FICHIER** au lieu de celui produit par
   `swresample` : en mode « hauteur non préservée », le WAV annonçait 48 kHz en
   contenant du 24 kHz — la mesure de fréquence était donc fausse d'un facteur 2.

**Décision documentée : le HDR n'est pas proposé.** Sur les écrans de l'utilisateur il
n'apporterait rien (SDR), et sur ce build il dégraderait l'image : `tonemap` sans `zscale`
écrase le signal (43/1023 au lieu de 504). Aucun réglage n'est donc exposé — c'est noté
dans `INVENTAIRE-FONCTIONNALITES.md` §2 comme limite mesurée, pas comme oubli.

**Nouvel outil** : `tools/analyse-audio.py` (8 mesures : niveaux, crête, rapport
signal/bruit, écart entre canaux, fréquence, énergie de queue, début audible). Il lit les
WAV en flottant à la main, le module `wave` de Python refusant ce format.

### 6.19 Égaliseur audio 18 bandes (`superequalizer`) (session 15)

**Ce qui manquait.** L'égaliseur avait été retiré en session 9 (« nécessite un traitement
audio non implémenté »). La brique existait pourtant dans `libavfilter`.

**Ce qui a été fait.** Un second graphe `abuffer → superequalizer → abuffersink`, inséré
**après la conversion au format du périphérique** (f32 entrelacé) : il agit donc exactement
sur le signal envoyé à la carte son. 18 bandes (65 Hz → 20 kHz), fréquences imposées par
FFmpeg, gains stockés en multiplicateur linéaire (ce qu'attend le filtre) et affichés en
dB. Cinq profils : Neutre, Basses, Aigus, Voix, Isolation — plus une courbe personnalisée.

**Trois défauts trouvés par la mesure, invisibles à l'écoute :**

1. **Signal deux fois trop long.** Un paquet audio ne ressort pas forcément dans l'appel
   courant : le filtre a une ligne de retard. En conservant les échantillons d'entrée
   quand le puits ne rendait rien, le WAV produit contenait **248 640 échantillons pour
   132 637 en entrée**. Corrigé : on collecte tout ce qui est disponible et on vide le
   tampon ; un `flush` final récupère la ligne de retard. Contrôle d'intégrité désormais
   exact : 132 637 = 132 637.
2. **Lecture hors tampon.** `superequalizer` sort en format **planar** (`fltp`) : chaque
   canal dans un plan distinct. Lire `nb_samples × canaux` flottants depuis `data[0]`
   débordait — le WAV contenait des **NaN et des valeurs à 3,2e38**. Corrigé en forçant
   la négociation (`,aformat=sample_fmts=flt:channel_layouts=Nc`) **et** en vérifiant le
   format réellement produit avant de lire.
3. **Dérive d'arrondi dans l'interface.** `egui::Slider::fixed_decimals` réécrit la valeur
   **arrondie** dans la variable, et `.changed()` devient vrai même sans interaction :
   un gain de ×2 devenait ×1,9953 dès l'affichage, et la courbe ne correspondait plus à
   aucun profil. Corrigé par un seuil (on n'agit que si l'écart dépasse l'arrondi) et par
   une comparaison des profils **en décibels**.

**Vérification chiffrée — `tools/verif-audio.sh` (17 contrôles, sans carte son).**
Bruit blanc décodé deux fois (`--dump-audio`, même chaîne que la lecture), puis spectres
comparés par transformée de Fourier :

| Réglage | Attendu | Mesuré |
|---|---|---|
| bandes 1–4 à ×4 | +12 dB sous 200 Hz, 0 ailleurs | **+11,95 / +11,98 dB**, −0,09 dB à 262 Hz |
| bandes 15–18 à ×4 | +12 dB au-dessus de 8 kHz | **+12,04 / +12,05 dB**, −0,02 dB à 2 kHz |
| bandes 1–4 à ×0,25 | −12 dB | **−12,13 dB**, −0,03 dB à 1 kHz |
| égaliseur actif mais neutre | aucun graphe construit | signal **strictement identique** (écart 0,000000) |
| gains hors plage (`50`, `-3`, vecteur de 3 valeurs) | bornés et complétés | `1b=20.000`, `2b=0.000`, `18b=1.000` |

**Interface** (section « Audio » des préférences) : interrupteur, liste de profils
(« Personnalisé » dès qu'un curseur bouge), 18 curseurs en deux colonnes, bouton « toutes
les bandes à 0 dB ». Les curseurs sont dessinés **uniquement si l'égaliseur est actif**,
et le profil affiché est celui qui correspond réellement aux gains.

**Contrôles ajoutés à `tools/audit-prefs.sh` (10/10)** : profil identifié, 18 bandes
dessinées, déplacement d'un curseur mesuré (5,105 dB → −7,000 dB), et **absence de dérive**
à la réouverture (6,021 → 6,021 dB — c'est le contrôle qui aurait détecté le défaut n° 3).

**Piège de script.** `printf` de bash suit la locale : en français il produit « 11,95 » et
`%f` refuse ensuite ce texte. Le formatage des mesures se fait désormais en Python.

### 6.20 Fenêtre « Préférences » : sections dépliables et bornage à l'écran (session 14)

**Symptôme signalé.** Le bandeau du menu sortait de la fenêtre, les sections se
succédaient sans séparation nette, et la fenêtre grandissait à chaque réglage ajouté.

**Cause.** La fenêtre était une suite de `ui.heading` directement dans `ui.rs` : aucun
conteneur borné, donc sa hauteur suivait le contenu **indéfiniment** (rien ne limitait
sa taille à l'écran). Ajouter un paramètre obligeait à retrouver le bon endroit dans
300 lignes mêlant interface, filtres, audio et sous-titres.

**Architecture retenue — `src/settings.rs`, table déclarative.**

```rust
pub struct Section {
    id, icon, title, hint,        // identifiant stable (porte l'état déplié), titre, explication
    default_open: bool,
    modified: fn(&App) -> bool,   // ce domaine diffère-t-il des valeurs par défaut ?
    draw: fn(&mut App, &mut Ui),  // le corps de la section
}
pub const SECTIONS: &[Section] = &[ … Interface, Vidéo, Image, Audio, Sous-titres, Lecture ];
```

**Ajouter un réglage** (prochain candidat : l'égaliseur audio) = **une entrée dans
`SECTIONS` + une fonction `dessiner_…`**. La fenêtre, le défilement, le bornage à
l'écran, les boutons « tout déplier/replier », le compteur de sections modifiées et la
journalisation de géométrie s'appliquent **sans autre modification**.

**Lisibilité, point par point :**

| Exigence | Mise en œuvre |
|---|---|
| Le bandeau ne sort pas de la fenêtre | hauteur maximale = écran − 100 px ; corps dans une `ScrollArea` plafonnée ; `resizable(true)` pour l'ajuster |
| Sections bien séparées | chaque section dans un `Frame` (fond, coins arrondis), chevron ▸/▾, icône, titre, **phrase d'explication toujours visible** (même repliée) |
| Sous-sections lisibles | `sous_section()` : titre discret + filet, ex. « Apparence », « Contrôles à l'écran », « Avancé — paramètres des filtres » |
| Libellés jamais rognés | `ligne()` : `egui::Grid` à 2 colonnes, le libellé garde sa place |
| Repérage de ce qui est modifié | repère « • modifié » par section + compteur en tête |
| Accès rapide | boutons « tout déplier » / « tout replier » |

**Accueil de nouveaux paramètres : le sous-bloc « Avancé ».** Il expose désormais les
paramètres FFmpeg eux-mêmes, avec ces champs dans `Config` : `deband_range` (4–64 px),
`deband_threshold` (0,001–0,2), `deinterlace_parity` (`Auto` / `Tff` / `Bff`). Ils sont
grisés quand le filtre correspondant est inactif, avec la raison affichée. La chaîne
**réellement construite** est affichée en clair dans la fenêtre (et journalisée) :
`deband=range=64:1thr=0.1500:…`.

**Piège majeur rencontré — boucle de rétroaction sur la hauteur.** Première version :
la hauteur du corps était calculée avec `ui.available_height()`. Or la fenêtre se
dimensionne sur son contenu, qui **est** cette zone défilante : `available_height`
dépendait donc de la taille choisie à l'image précédente, et la fenêtre **rétrécissait à
chaque image** (contenu coupé au milieu de la 3ᵉ section — visible sur la capture).
Corrigé en fixant la borne hors de cette boucle (hauteur de l'écran), puis en laissant
`auto_shrink` vertical faire que la hauteur soit une fonction directe du contenu.

**Vérification — `tools/audit-prefs.sh` (7 contrôles, piloté par la géométrie
journalisée, aucune coordonnée devinée) :**

| Contrôle | Résultat |
|---|---|
| la fenêtre tient dans l'application | ✔ (24,24) → (578,616) dans 1280×720 |
| « tout déplier » ouvre les sections | ✔ 6 sections ouvertes |
| tout déplié, la fenêtre reste à l'écran | ✔ (le défilement prend le relais) |
| la molette fait défiler le contenu | ✔ écart d'image 5,75/255 |
| « tout replier » referme les sections | ✔ 6 sections fermées |
| un clic sur un bandeau bascule **une** section | ✔ `ouvert=false → true` |
| aucune chaîne de filtres refusée | ✔ |

**Piège de script (à retenir).** Compter des états en découpant la ligne de journal en
champs (`awk '{print $4}'`) est fragile : l'horodatage et le préfixe « DEBUG
kinterface: » décalent tout (le comptage renvoyait 1 au lieu de 6). L'extraction se fait
maintenant par expression régulière sur le texte du message.

**Paramètres avancés vérifiés de bout en bout** (`tools/verif-filtres.sh` §6, 9/9) : la
valeur saisie se retrouve dans la chaîne (`range=64`, `1thr=0.1500`) **et** change
l'image (plateau moyen 2,57 → 2,14 lignes pour un seuil plus élevé).

### 6.21 Chaîne de filtres `libavfilter` : détramage, débruitage, désentrelacement, image (session 13)

**Ce qui existait.** Depuis la session 9, ces fonctions étaient **absentes** et les réglages
correspondants avaient été retirés, au motif qu'ils exigent une chaîne de filtres FFmpeg.
La bibliothèque était pourtant déjà là (`/opt/ffmpeg-7.1/lib/libavfilter.so`, feature
`avfilter` active dans `Cargo.toml`) : il ne manquait que le code.

**Ce qui a été fait.** Un graphe `buffer → filtres → buffersink` construit à la demande
(`player::build_filter_chain`), inséré **entre le décodage et la conversion RGBA**, donc
après le rapatriement VA-API en mémoire système et **avant** la mise à l'échelle.

| Réglage (préférences) | Filtre FFmpeg | Effet mesuré (`tools/verif-filtres.sh`) |
|---|---|---|
| Détramage | `deband` | bandes d'un dégradé : plateau moyen **12,97 → 2,57 lignes** |
| Débruitage (3 niveaux) | `hqdn3d=8:8:6:6` / `25:25:20:20` / `60:60:50:50` | énergie haute fréquence : **17,56 → 4,54** (niveau 3) |
| Désentrelacement | `yadif=mode=send_frame:deint=interlaced` | peigne : écart vertical **195 → 0** |
| Luminosité / contraste / saturation / gamma | `eq=…` | luminance **123 → 187** (+0,25) ; saturation 0 → image grise (`écart entre canaux 169 → 3,7`) |
| *(aucun réglage)* | **aucun graphe construit** | 0 filtre, aucun coût (vérifié) |

**Décisions qui ont demandé une mesure.**

1. **Filtres 1 image → 1 image uniquement.** Le moteur avance d'une image par tour
   d'affichage ; `yadif` en mode `send_field` doublerait le nombre d'images et casserait la
   cadence. D'où `mode=send_frame` + `deint=interlaced` (une source progressive n'est pas
   touchée). Un contrôle d'intégrité signale dans le journal toute chaîne déséquilibrée
   (entrées ≠ sorties).
2. **Les forces de `hqdn3d` ne sont pas intuitives.** Une progression « ×2 / ×4 » donnait
   8:12:4,5:9 — mesuré : **1 % de bruit en moins**, c'est-à-dire rien. Les trois niveaux
   retenus (8 / 25 / 60) sont ceux qui donnent 1 %, 17 % et 37 % de réduction sur un bruit
   gaussien de test.
3. **`deband` ne « lisse » pas les marches : il les dissout par tramage.** Conséquence
   contre-intuitive : les indicateurs de type dérivée seconde **augmentent**. La métrique
   retenue est donc la longueur des plateaux (ce que l'œil perçoit comme une bande), la
   seule qui diminue réellement.
4. **Un pas de quantification trop grossier n'est pas du banding** : sur des marches de
   21 niveaux, `deband` ne fait rien (vérifié avec ffmpeg) — il voit des contours. Le clip
   de test utilise donc des marches de 6 niveaux.

**HDR : mesuré impossible en l'état (et donc non livré).** Le filtre `tonemap` existe, mais
il exige un signal en **lumière linéaire** ; or `zscale` (libzimg) est absent de ce build et
`colorspace` refuse `smpte2084` (« Unsupported input transfer characteristics »). Appliqué
tel quel sur un flux PQ, `tonemap` produit une image plate : **luminance moyenne 43/1023 au
lieu de 504**, plage utile 19–66. Aucun réglage HDR n'a donc été ajouté : un sélecteur qui
dégrade l'image serait pire que son absence.

**Coût mesuré** (`--bench`, décodage seul, sans affichage) :

| Configuration | 640×360 | 1920×1080 |
|---|---|---|
| Aucun filtre | 1,29 ms/image (775 i/s) | 11,42 ms/image (87 i/s) |
| `deband` seul | 1,51 ms/image (661 i/s) | 13,15 ms/image (76 i/s) |
| Tous les filtres | 3,22 ms/image (310 i/s) | 21,10 ms/image (47 i/s) |

À 1080p, la chaîne complète reste **2× au-dessus** du besoin d'un film à 24 i/s. Le coût
n'est payé que si un réglage est actif.

**Deux défauts d'affichage trouvés en regardant la fenêtre (pas en lisant le code) :**
les libellés des curseurs étaient rognés (« minosité », « ntraste ») — `ui.horizontal`
laissait le curseur prendre toute la largeur disponible. Corrigé par une `egui::Grid`
(libellés alignés) et une position par défaut explicite : sans elle, egui centrait la
fenêtre et elle débordait à gauche de l'écran virtuel.

**Deux outils ajoutés pour que ce soit vérifiable, pas seulement annoncé :**

- `--dump-frame <ms> <image.png> <média>` : décode une image à la position demandée, filtres
  compris, **à la résolution native, sans fenêtre**, et l'écrit en PNG. Une capture d'écran
  dépendrait de la taille de la fenêtre et du rééchantillonnage — donc de rien de mesurable.
- `--bench <n>` : débit de décodage filtres compris, sans affichage (le rendu logiciel de
  l'écran virtuel consommait 888 % d'un cœur et noyait toute mesure pendant la lecture).
- `tools/verif-filtres.sh` : banc complet (7 contrôles), `tools/metrics-image.py` : métriques
  (bandes, bruit, peigne, luminance, saturation).

**Piège FFmpeg rencontré.** `avfilter_inout_free` libère le champ `name` avec `av_free` :
y placer l'adresse d'une chaîne statique (`"in"`) provoque `free(): invalid pointer` et tue
le processus au premier essai (constaté). Le nom doit être alloué par FFmpeg (`av_strdup`).

### 6.22 🔴 La fenêtre se fige, KWin la tue : `ctx.*` pendant `ctx.input(...)` (session 12)

**Symptôme.** `Signal: 6 (ABRT)` **hors `--crash-test`**, `crash.log` vide, thread
principal bloqué sur un **verrou partagé** dans `ui::update` — core conservé du
2026-10-16 à 23:11. La session 11 avait laissé le dossier « à élucider ».

**Enquête — deux découvertes indépendantes.**

**1. Ce n'était pas un plantage, mais un arrêt forcé.** Le core enregistre
`si_code = SI_TKILL` avec `si_pid` = son **propre** PID, ce qui fait conclure à une
erreur interne. C'est un artefact : le handler de `crash.rs` relance lui-même le signal
(`signal(SIG_DFL); raise(sig)`) pour que le core soit produit, et c'est **ce
relancement** que le noyau consigne — l'origine réelle est écrasée. Le journal, lui,
est sans ambiguïté : `kwin_killer_helper_x11` démarre à **23:10:43**, le processus meurt
à **23:11:08**. Le désassemblage de l'assistant installé
(`/usr/lib/x86_64-linux-gnu/libexec/kwin_killer_helper_x11`) le confirme :

```asm
97e2: mov $0x6,%esi ; call kill      ; SIGABRT (pour obtenir un core exploitable par DrKonqi)
…    (boucle de reprises, pauses de 100 ms)
984a: mov $0x9,%esi ; call kill      ; SIGKILL
```

**Le SIGABRT est donc la conséquence d'un gel de l'interface, pas sa cause.** Second
piège : `crash.log` était **tronqué à chaque démarrage**, si bien que le rapport du
23:11 avait disparu avant d'être lu — « rapport vide » ne prouvait pas que le handler
ne s'était pas déclenché.

**2. Le gel était déterministe et reproductible en quelques minutes.** Lecteur lancé sur
un écran X virtuel, un appui sur `Q`, puis `bt` sous `gdb` : la pile obtenue est le
**calque exact** de celle du core (mêmes noms de fonctions, mêmes lignes).

```
#11 viewport_id                                  (egui : verrou PARTAGÉ demandé)
#12 viewport_id                                  egui/context.rs:3779
#13 send_viewport_cmd                            egui/context.rs:3846
#14 {closure#4}                                  src/ui.rs:518   ← la touche Q
#15 {closure#0}<(), update::{closure_env#4}>     egui/context.rs:914
#16 write<(), input::closure#0<…>>               egui/context.rs:760  ← verrou EXCLUSIF DÉTENU
#17 input<(), update::{closure_env#4}>           egui/context.rs:914
#18 update                                       src/ui.rs:497
```

**Cause racine.** En egui 0.33, `Context::input()` prend le verrou **exclusif** du
contexte (`Context::input` → `Context::write`). Le verrou utilisé
(`parking_lot::RwLock`, par `epaint::mutex`) **n'est pas réentrant** : appeler
`ctx.send_viewport_cmd(Close)` depuis la fermeture redemande ce verrou (au passage par
`Context::viewport_id()`), cette fois en **partagé** → le fil s'attend lui-même,
**définitivement**. Autrement dit, `Q` — « quitter » — était précisément la touche qui
gelait le lecteur : l'utilisateur appuie sur `Q`, la fenêtre ne répond plus, il la force
à quitter, et le SIGABRT de KWin ressemble à un plantage.

**Correctif.** Les commandes de fenêtre demandées par un raccourci sont **différées** :
`App::pending_viewport_cmds` est vidé au tour suivant, hors de toute fermeture de saisie,
avec un `request_repaint()` qui garantit ce tour même en pause. Plus aucun `ctx.*` n'est
appelé depuis une fermeture de saisie (`ctx.input`, `ctx.viewport`).

**Gardes-fous ajoutés** — un correctif sans contrôle qui l'accompagne revient toujours :

| Outil | Ce qu'il vérifie | Preuve qu'il **détecte** vraiment |
|---|---|---|
| `tools/check-ctx-reentrance.py` | statique : tout `ctx.*` dans une fermeture `ctx.input/viewport(...)` → interblocage garanti | défaut réintroduit exprès dans une copie de `ui.rs` → signalé (`ui.rs:544`) ; 0 problème sur le code corrigé |
| `tools/vivacite.sh` | envoie une touche **non affectée** (`X`) et exige une **nouvelle ligne de journal** (une application gelée reste vivante : seule la réception d'un nouveau message prouve que le fil principal progresse) | `tools/test-vivacite.sh` : vert → `SIGSTOP` → **le détecteur voit le gel** → `SIGCONT` → il repasse au vert |
| `tools/audit-ui.sh` | contrôle statique en tête, **vivacité après chaque touche**, et test final « `Q` ferme le lecteur » qui exige la **mort du processus** | audit **54/54**, dont 1 statique + 1 fermeture |
| `tools/diag-gel.sh` | reproduit un gel en une commande (lancement **sous** `gdb` — obligatoire quand `ptrace_scope=1` —, envoi de la touche, gel par `SIGSTOP`) et désigne l'interblocage | défaut volontairement réintroduit : pile `lock_shared_slow` ← `viewport_id` ← `send_viewport_cmd` ← **`src/ui.rs:544`** ; sur le code corrigé : aucune attente de verrou |
| `src/crash.rs` | rotation `crash.log` → `crash.1.log` (plus d'effacement) et consignation de `timestamp`, `si_code`, `si_pid`, `si_uid` et du **nom du processus émetteur** | `kill -ABRT <pid>` → `si_code=0`, `si_pid=690806`, `sender=bash`, `origine=ARRET FORCE PAR UN AUTRE PROCESSUS` |

**Leçon transférable.** Trois fois dans ce projet, un signal `SIGABRT` a été pris pour un
plantage alors qu'il signalait un **gel**. Ce qui débloque le diagnostic : (1) lire
`si_code`/`si_pid` **dans le rapport**, pas seulement dans le core (le relancement les
écrase) ; (2) chercher l'émetteur **hors** du processus (`kwin_killer_helper_x11`,
`/proc/<pid>/comm`) ; (3) se souvenir qu'un verrou non réentrant transforme « lire l'état
du contexte » en attente infinie dès que le fil détient déjà le verrou exclusif.

### 6.23 Fenêtres de réglages : fermeture (session 11)

**Symptôme signalé :** impossible de fermer les fenêtres *Préférences* et *À propos*.

**Causes, toutes réelles :**

| Défaut | Explication |
|---|---|
| Aucun bouton de fermeture | Les fenêtres étaient déclarées sans `.open(&mut bool)` : **egui ne dessine alors aucun ✕**, et rien dans l'interface ne permettait de les fermer |
| *À propos* impossible à fermer | Elle n'était atteignable que par le menu et n'avait **ni touche ni bouton** de fermeture |
| Échap sans effet sur les fenêtres | Échap ne traitait que le plein écran |
| *Statistiques* disparaissait toute seule | Le minuteur de l'OSD remettait `show_statistics` à zéro après quelques secondes : la fenêtre s'effaçait juste après son ouverture |
| Aucune trace | La fermeture par le ✕ ne journalisait rien : impossible de la diagnostiquer |

**Corrections :**

- `.open(&mut ouvert)` sur les quatre fenêtres → ✕ dessiné, état rapatrié dans le champ
  (`°` puis journal « Fenêtre fermée: … » et message d'état) ;
- `Échap` ferme la fenêtre la plus en avant (`close_top_window`), puis sort du plein écran ;
- le minuteur de l'OSD n'éteint plus que le bandeau, plus les fenêtres ;
- **`F2`** ouvre/ferme *À propos* (touche accessible ; voir le piège F12 ci-dessous).

**Pour tester proprement (leçon utile) :** chaque fenêtre **journalise sa propre géométrie**
(target `kinterface`, niveau debug). L'audit en déduit le coin supérieur droit — donc la
position du ✕ — au lieu de deviner des pixels. Les heuristiques d'image échouent dès que la
vidéo est sombre ou contient du texte clair (essayé : trois implémentations, trois échecs).

---

### 6.24 Horloge audio maître : la carte son cadence l'image (session 21)

**Le problème.** La cadence comparait le PTS des images au **temps mural** (une simple
horloge système), alors que le son sort au rythme de la **carte son**. Trois conséquences :
sur un périphérique dont l'horloge diffère de celle du système (Bluetooth, HDMI, carte
externe : quelques centièmes de pour cent suffisent), le son et l'image divergent
lentement ; l'erreur ne se voyait qu'au bout de plusieurs minutes, sous la forme de
sous-alimentations ou de sauts ; et la profondeur du tampon (mesurée entre 42 et 85 ms,
variable selon l'entrelacement du conteneur) s'ajoutait telle quelle à l'écart image/son.

**Ce qui a changé.**

1. **Le compteur de consommation** (`src/audio.rs`). Le callback audio compte les
   échantillons du média qu'il sort réellement du tampon. Le silence de sous-alimentation
   n'est **pas** compté : il n'appartient à aucun instant du média. C'est la seule mesure
   du temps qui soit cadencée par la carte.
2. **L'ancrage** (`audio::Ancrage`, arithmétique pure, 10 tests unitaires). Il retient la
   position média du premier échantillon poussé depuis la dernière remise à zéro, puis
   `position = ancre + trames consommées / taux × vitesse − retard du périphérique`, borné
   à l'ancre (au démarrage, le retard du périphérique ne doit pas retarder l'image).
3. **La référence de cadence** (`VideoPlayer::media_clock_ms`) : horloge audio si elle
   existe, temps mural sinon.
4. **L'alimentation du tampon** (`feed_audio`) : elle lit des paquets **indépendamment de
   l'affichage**, jusqu'à la profondeur cible (120 ms par défaut, `--audio-buffer`).
5. **La file de paquets vidéo** (`queued_video_packets`) : les paquets lus pendant
   l'alimentation sont **gardés en attente** (copie par comptage de références, quelques
   dizaines de Ko) au lieu d'être décodés en images RGBA (plusieurs Mo). Le décodeur ne les
   prend qu'au moment de l'affichage (voir le §« piège » ci-dessous).
6. **Le garde-fou** : si l'horloge audio ne progresse plus pendant 250 ms (piste audio
   terminée, périphérique suspendu, tampon qui ne se remplit plus), on repasse au temps
   mural et on journalise l'événement — l'image ne peut pas se figer.

**Le piège qui a coûté le plus de temps : le cercle vicieux.** Alimenter l'audio en lisant
des paquets **décode aussi les paquets vidéo rencontrés**. Une première version les décodait
en images et bornait la pile à 6 images : le tampon ne pouvait alors plus se remplir
(6 images pile = alimentation arrêtée), l'horloge audio s'arrêtait, donc aucune image n'était
consommée, donc la pile restait pleine. Résultat observé : cadence divisée par deux, horloge
qui repartait par à-coups toutes les 250 ms, 35 à 190 sous-alimentations. La sortie du
cercle n'est pas un réglage de seuil mais un **changement de représentation** : garder des
paquets (petits) plutôt que des images (grosses). Le garde-fou de 250 ms reste comme filet.

**Deux erreurs de mesure dans le premier jet** — elles expliquent des chiffres aberrants
(« l'audio avance deux fois moins vite que le réel ») :
- division par le nombre de canaux **deux fois** (une fois pour convertir les échantillons en
  trames, une seconde dans la formule). Les tests unitaires l'ont révélé : une seconde de
  trames consommées doit donner 1000 ms, pas 500 ;
- `frame_interval()` renvoyait déjà un délai mural (divisé par la vitesse) et servait
  d'intervalle média dans `next_frame_delay()` : la vitesse était appliquée deux fois. Il
  renvoie maintenant l'intervalle **média**, et l'avance image par image (`.`) l'utilise tel
  quel.

**Mesures** (`tools/verif-horloge.sh`, 18 contrôles, 31 tests unitaires) :

| Contrôle | Avant (temps mural) | Après (carte son maître) |
|---|---|---|
| Profondeur du tampon | 42–85 ms, subie | **127–148 ms**, maintenue par une alimentation dédiée |
| Écart image/son | la profondeur du tampon s'y ajoutait | **−34 … −8 ms** (moins d'une image, jamais en avance) |
| Tampon imposé à 400 ms | aurait décalé le son d'autant | **écart inchangé** (−8 … −34 ms) |
| Sous-alimentations | 0 à 1 / 12 s | **0** |
| Dérive horloge audio / temps mural | — | 5 → 6 ms (pas de dérive) |
| Vitesse 2× | média × 2 du réel | média × 2,00 du réel |
| Audio terminé avant la vidéo | — | repli après 251 ms, lecture jusqu'au bout |
| Coût | — | 0 sous-alimentation, RSS stable (183 Mo sur un 1080p VA-API) |

---

### 6.25 Reconnexion d'un flux interrompu (session 22)

**Le problème.** Un flux qui se coupe **arrêtait la lecture**. Le code traitait toute
lecture en échec comme une fin de flux (`eof = true`), si bien qu'une coupure réseau, un
serveur redémarré ou un segment manquant mettaient fin au média définitivement — alors que
la source, elle, pouvait très bien revenir une seconde plus tard.

**Le piège central : FFmpeg cache la coupure derrière un EOF.** Instrumenté sur un flux
HLS réellement coupé (503 sur les segments), `av_read_frame` rend
`ret=-541478725` (`AVERROR_EOF`) **et** `avio_feof()` est vrai — exactement ce que rend une
fin de playlist normale. Le démultiplexeur HLS a, entre-temps, sauté tous les segments en
erreur en quelques millisecondes (journal FFmpeg : « Segment N of playlist 0 failed too many
times, skipping »). Conséquence pratique : **aucun code de retour ne permet de distinguer
une coupure d'une fin**. Le seul critère fiable est la **position** : à 9,4 s d'une playlist
annoncée à 60 s, ce n'est pas une fin.

**Ce qui a été mis en place.**

1. **Distinction fin/coupure** (`clore_ou_reconnecte` + `fin_legitime`, testée) : fin si la
   position atteint la durée annoncée à 3 s près ; coupure sinon, y compris quand la durée
   est inconnue (direct).
2. **Reconnexion** (`tenter_reconnexion`) : temporisation doublante 0,3 → 8 s, 8 essais,
   réouverture de l'URL, reprise à l'image affichée. L'état de panne est porté par la
   structure `NetworkStream` et survit à la réouverture (qui recrée un contexte neuf).
3. **Sonde en arrière-plan** (`sonde_url` + `JoinHandle`) : chaque essai commence par une
   ouverture dans un thread. Sans elle, un serveur muet figeait le fil d'interface pendant
   tout le délai, à chaque tentative.
4. **Garde-temps d'E/S** (`DelaiReseau` + `AVIOInterruptCB`) : **`rw_timeout` ne suffit
   pas**. Constaté : serveur qui accepte la connexion puis se tait → blocage de plus de
   8 s malgré `rw_timeout=3 s`, parce que le démultiplexeur HLS ouvre les segments avec des
   contextes d'E/S qui ne reçoivent pas les options d'ouverture. Le seul mécanisme consulté
   par **toutes** les boucles d'E/S de FFmpeg est `interrupt_callback` : armé à 3 s par
   lecture de paquet, il borne le blocage (mesuré : **3 003 ms**).
5. **Visibilité** : bandeau persistant pendant la reconnexion (+ bouton « Arrêter »),
   bandeau d'erreur à l'abandon ; le journal consigne chaque tentative.

**Mesures** (`tools/verif-reseau.sh`, 20 contrôles, 9 tests unitaires) :

| Scénario | Résultat mesuré |
|---|---|
| 503 sur les segments, coupure de 7 s | détectée, reprise à **9 200 ms**, lecture jusqu'à 15 235 ms |
| Synchronisation après reprise | dérive image/son **35 ms** |
| Serveur tué puis relancé | « connexion refusée » détectée, reprise après redémarrage |
| Connexion fermée sans réponse | coupure détectée, flux rouvert |
| Serveur muet (connexion acceptée, aucune réponse) | blocage **3 003 ms**, interface libérée en 507 ms |
| Panne prolongée | 8 tentatives (≈ 33 s), abandon explicite, **interface réactive** (Q ferme) |
| Fin normale de playlist | fin de média, **aucune** reconnexion |
| Fichier local illisible | aucune reconnexion tentée |

**Limite assumée** : la lecture d'un paquet et l'ouverture se font dans le fil d'interface
(le démultiplexeur n'est pas déporté). Une source muette fige donc la boucle d'événements
au plus 3 s (lecture) ou 10 s (ouverture), une fois. Supprimer totalement ce blocage
demanderait de déplacer tout le démultiplexeur dans un thread et de faire circuler les
paquets — un chantier à part entière, hors de cette session.

---

### 6.35 PotPlayer : arborescence des réglages **vérifiée sur l'installation** (session 30)

**Pourquoi reprendre ce travail.** La comparaison de la session 29 s'appuyait sur la
documentation publique, avec une réserve explicite (« les intitulés varient »). La réserve
était justifiée : la vérification a corrigé **trois erreurs**.

**Comment.** PotPlayer est installé dans un préfixe Wine de test
(`wine-test-pot/drive_c/Program Files/DAUM/PotPlayer/`). Les libellés de l'interface sont
dans `Language/French.ini` (5 640 lignes) — c'est le texte que la fenêtre charge. La version
est celle que l'application écrit elle-même dans le registre : **`v260819`**.

**Deux obstacles, tous deux levés par la mesure :**

1. **Le binaire est packé Themida** (section `.themida`) et sa ressource `VERSIONINFO` est
   vidée (`0.0.0.0`). Chercher les libellés *dans l'exécutable* ne donne que du bruit de
   packer : le fichier de langue est la bonne surface.
2. **Les outils de chaînes de REA répondaient `provider_unavailable` — diagnostic
   rectifié.** Première explication donnée : « ghidra non configuré et la machine n'a que le
   JDK 11 ». **Faux sur le second point** : Ghidra **12.1.4**
   (`/media/blaise/LINUX_DATA/ghidra_12.1.4_PUBLIC`) et un **JDK 21**
   (`/media/blaise/LINUX_DATA/jdk-21.0.12.1+1`) sont installés, et `~/.bashrc` les exporte
   déjà. La vraie cause : le **serveur REA en cours** (fils de `ajean web`) a été lancé
   **sans** `GHIDRA_INSTALL_DIR` ni `JAVA_HOME` — son `/proc/<pid>/environ` ne contient ni
   l'un ni l'autre, d'où le repli sur le `java` du système (11) et le refus.

   **Vérifié depuis**, avec les deux variables : `rea doctor` passe **tous** les contrôles
   Ghidra (installation, version 12.1.4, `analyzeHeadless`, décompileur natif
   `linux_x86_64`, java 21.0.12.1 64 bits accepté) et `rea search notepad.exe "Wine"`
   renvoie `0x140008256 = "Wine Notepad"` (preuve `ev_452a9170…`). En revanche
   `rea search PotPlayer64.dll` renvoie **`provider_timeout`** : 30 Mo packés Themida,
   analyse au-delà du budget de 330 s.

   **Correctif, en deux temps (le premier était au mauvais endroit)** :
   `~/.config/environment.d/50-rea-ghidra.conf` ne sert à rien ici — `ajean web` est un
   **service systemd système**, pas une application de session. Le bon endroit est un
   drop-in `/etc/systemd/system/ajean-ui.service.d/rea-ghidra.conf`
   (`Environment=GHIDRA_INSTALL_DIR=…` + `Environment=JAVA_HOME=…`), suivi de
   `systemctl daemon-reload` et du redémarrage du service.

   **Vérifié après redémarrage** (serveur REA 17 300 67) : variables présentes dans
   `/proc/<pid>/environ` ; `binary_session` → ghidra **`available`**, **12.1.4**, java
   **21.0.12.1** ; et l'appel réel **passé par le MCP** :
   `open_binary(notepad.exe)` puis `search_strings("Wine")` →
   **`0x140008256 = "Wine Notepad"`** (`ev_452a9170…`). Décompilation et inventaire de
   chaînes sont donc utilisables depuis l'agent.

   Ce qui **a** fonctionné sans moteur : `inspect_pe_resources` (fournisseur
   `rea-pe-resources`) → PE `pe32-plus`, machine 34404, inventaire des ressources complet
   (`ev_9baa890c…`).

**Les trois corrections.** `Lecture` n'est pas une catégorie mais une sous-page de
**Général** ; il n'y a **pas** de catégorie « Capture » mais **Général › Capture d'écran** ;
le bloc de filtres s'appelle **Contrôle du filtre** et ne compte que quatre pages
(Source/Séparateur, Décodeurs vidéo, Décodeurs audio, Priorité globale) — le
désentrelacement, la colorimétrie et les effets sont sous **Vidéo**.

**Livrable** : `docs/POTPLAYER-REGLAGES-VERIFIES.md` — arborescence exacte (identifiants
2055→2098), libellés **page par page** avec le numéro de `[Dialog(N)]` d'où ils viennent
(Colorimétrie 384, Désentrelacement 385, Effets 386, Style police 383, Rendu vidéo 355,
Égaliseur 363, Normaliseur/Freeverb 387, Général 354, Souris 458, A-B 453), digests des
fichiers. La comparaison (`COMPARAISON-POTPLAYER.md`) est mise à jour avec ces libellés, et
les manques se précisent : **Teinte**, **WarpSharp**, **mouvement flou**, **10 bits**, **HDR
matériel**, **SuperEQ**, **famille de police**, **alphabet/interlettre/second contour**,
**position horizontale**, **Ruby**.

---

### 6.34 Pré-cache des flux réseau (session 30)

**La question posée.** « L'application prend-elle en charge le cache ou le pré-cache ? »
Réponse mesurée avant d'écrire quoi que ce soit : **non**. Un serveur HLS local
journalisant chaque requête montrait des segments demandés **au moment exact de leur
usage** (intervalles de 2,0 s pour des segments de 2 s, zéro avance), aucune écriture
disque, et aucun réglage FFmpeg de mémoire réseau (`buffer_size`, `max_delay`…). Seuls
existaient des **tampons mémoire** : anneau audio (~1 s), file de 240 paquets vidéo,
24 images décodées en avance.

**Ce qui a été ajouté**, et pourquoi à cet endroit :

- `playback.precache_ms` (défaut **6 s**, 0 = désactivé, plafond 30 s) — un fichier local
  n'a rien à y gagner, un flux si ;
- la **capacité du tampon audio** devient réglable (`AudioOutput::set_capacite_ms`) : c'est
  lui qui garde le média en avance. Un fichier local reste à ~1 s ; un flux monte à
  `réserve + profondeur + 0,5 s` (mesuré : 6 620 ms pour 6 s de réserve) ;
- `precharger_reseau()`, appelé dans `tick_audio` juste après l'alimentation normale : il
  lit des paquets **en avance** jusqu'à ce que le dernier paquet vidéo en attente soit
  `precache_ms` devant l'image affichée. Les paquets vidéo restent **non décodés**
  (quelques dizaines de Ko au lieu de plusieurs Mo par image) ;
- trois garde-fous : `MAX_QUEUE_BYTES` (48 Mo, car un paquet 4K pèse bien plus qu'un paquet
  720p — le nombre de paquets ne suffit pas), le plafond de 240 paquets, et l'arrêt du
  préchargement **avant** la saturation du tampon audio (sinon `push` jetterait les
  échantillons les plus anciens, c'est-à-dire la réserve qu'on vient de constituer) ;
- réglage visible : préférences ▸ Lecture ▸ « Flux réseau : pré-cache », avec la mémoire
  consommée annoncée (≈ 350 Ko par seconde).

**Deux défauts trouvés en chemin, par la mesure.**

1. **La reprise après reconnexion reculait.** Elle utilisait `current_position_ms` (dernier
   paquet vidéo décodé) ; avec une réserve, les paquets lus sont très en avance sur
   l'image, et la reprise se faisait **4 s en arrière** (mesuré : reprise à 9 421 ms alors
   que le son en était à 13 389 ms). Corrigé : on reprend à l'**horloge audio**, et à
   `current_position_ms` seulement si le son s'est tu — jamais au repli mural, qui a
   continué d'avancer pendant le gel et ferait sauter du film.
2. **Le banc lui-même mesurait mal le gel.** Il cherchait des positions identiques
   consécutives ; or pendant un blocage, **le journal cesse simplement d'être écrit** (le
   fil d'interface attend la réponse) : aucune position répétée, donc « aucun gel ».
   La mesure correcte est le **trou entre deux relevés** moins la période normale (1 s).

**Trois régressions trouvées par la suite complète, et corrigées** (aucune n'était
visible dans le banc du pré-cache) :

1. **Une fin de playlist était prise pour une coupure** : le préchargement rencontrait
   l'erreur de fin cinq secondes avant que la lecture n'y arrive, et le moteur décidait
   « coupure » à la place de « fin ». Désormais une erreur pendant le préchargement est
   **mémorisée** (`prefetch_fini`), pas décidée : la lecture tranchera quand elle y sera.
2. **L'image prenait du retard (−154 ms)** : remplir 6 s d'un coup (jusqu'à 512 paquets,
   avec décodage et égalisation de l'audio) bloquait la boucle d'interface. Le
   préchargement est maintenant borné à **8 paquets par tour d'affichage** (60 × 8 =
   480 paquets/s, largement suffisant) : dérive revenue à 27 ms.
3. **Une reconnexion inutile** : une erreur survenue alors que la réserve était encore
   pleine déclenchait la reconnexion (donc couper la lecture). `reserve_disponible()`
   fait désormais **ignorer** l'erreur tant qu'il reste de quoi jouer : mesuré, un serveur
   absent 4 s donne **0,12 s de blocage au total**, sans interruption.

**Mesures** (`tools/verif-precache.sh`, 9 contrôles, deux passes) :

| Contrôle | Sans réserve | Avec 6 s |
|---|---|---|
| Écarts entre requêtes de segments | 1,9 / 2,0 / 2,0 / 2,0 / 2,0 s | 0,0 / 0,0 / 0,0 puis 2,0 s (rafale initiale) |
| Avance publiée par le lecteur | 200 ms | **6 000 ms** |
| Coupure de 3 s (503) : blocage le plus long | **4,34 s** | **0,46 s** |
| Recul de position après reconnexion | — | 0 (reprise à 13 901 ms, en continuité) |
| Serveur absent 4 s (test de `verif-reseau.sh`) | reconnexion après la panne | **aucune interruption** : blocage 0,12 s, recul 0 s |
| Réserve à 0 | avance 200 ms, capacité 960 ms (comportement d'origine) | — |
| Fichier local | avance 0 ms, capacité 960 ms (jamais pré-caché) | — |

---

### 6.33 Démarrage audio : la carte attend le tampon (session 29)

**Symptôme.** `verif-horloge.sh` échouait **une fois sur trois** sur « 2 blocs de
silence » — intermittent, donc accusé d'être du bruit de mesure. La trace ajoutée au
compteur a tranché : les deux blocs se produisaient à **70 et 96 ms** de lecture, avec
2 266 et 2 352 échantillons manquants sur un bloc.

**Cause.** Le déroulement était : `sync_audio_state()` démarre la carte dès que l'état
passe à « lecture », puis la boucle d'interface alimente le tampon. Or le premier tour
d'interface est le plus lent (police de secours, atlas de glyphes, première texture), et
le périphérique demande des blocs de **4 096 trames ≈ 93 ms** : le tampon encore vide ne
peut pas les fournir. Ce n'est donc pas un défaut de l'horloge, mais **un début de
lecture qui grinçait** — deux blocs de silence audibles, et un compteur qui les
signalait à juste titre.

**Correction, en deux temps et dans cet ordre.**
1. `tick_audio` **remplit le tampon avant** de rendre la main à la carte, et
   `sync_audio_state` ne démarre le périphérique que si ce préremplissage a eu lieu
   (`audio_prefill_fait`). Tant que la carte n'est pas lancée, elle sort du silence sans
   que ce soit un manque : le compteur ne bouge pas.
2. **Le préremplissage ne doit jamais bloquer la lecture.** Une première version
   attendait la cible complète sans limite : après un seek près de la fin de la piste
   audio, le tampon ne l'atteint jamais et l'image restait figée (mesuré : « lecture
   figée à 6 003 ms »). D'où une **borne de 200 ms** : au-delà, on démarre avec ce qu'on
   a — au pire un bloc de silence, jamais un gel.

Une erreur de ma part au passage, corrigée par la mesure : le drapeau changeait sans
appeler `sync_audio_state()`, qui n'est pas exécuté à chaque image ; la carte ne
démarrait donc jamais et le tampon montait jusqu'à son plafond (1 088 ms), horloge
audio nulle. Le journal le disait (`tampon 127/959`, puis `1088/1088`) : la ligne de
diagnostic existante suffisait à voir le défaut.

**Résultat.** Cinq passes consécutives de `verif-horloge.sh` : **19/19** (le contrôle
« le tampon est prérempli avant le démarrage de la carte » a été ajouté), **0
sous-alimentation**. Contre 1 échec sur 3 auparavant, avec 2 blocs à chaque fois.

---

### 6.32 `bwdif` contre `yadif` : le désentrelacement choisi par la mesure (session 29)

**La question posée par la liste « reste à faire »** était précise : « `bwdif` est présent
dans le build, à comparer à `yadif` par mesure (coût/qualité) avant de proposer le choix ».
Elle se tranche, et le résultat n'était pas celui qu'on attendait naïvement.

**Ce qui rend la mesure possible.** Le désentrelacement est un des rares filtres dont on
peut connaître la **vérité** : on fabrique soi-même le contenu entrelacé à partir d'une
image progressive connue (trames paires de l'image *2k*, impaires de l'image *2k+1*). On
mesure donc non seulement « le peigne a disparu » — que deux filtres réussissent — mais
« l'image retrouvée est proche de l'originale » (PSNR).

**Résultats** (`tools/bench-desentrelacement.sh`, reproductible) :

| Contenu | `yadif` | `bwdif` |
|---|---|---|
| Image ordinaire **immobile** (PSNR, reconstruction exacte possible) | 43,4 dB | **45,8 dB** |
| Mire en lignes de 1 px (pire cas) | 9,2 dB | **20,6 dB** |
| Écart en **mouvement** | — | 0,25 niveau en moyenne (équivalents) |
| Coût par image 1080p | 13,2 ms | **11,2 ms** |
| `estdif` (essayé, écarté) | — | 8,9 dB pour ≈ 65 ms/image : dix fois plus cher et moins bon |

`bwdif` l'emporte donc sur les deux tableaux, ce qui est rare : meilleur **et** moins cher.
D'où le **défaut = `bwdif`**, `yadif` restant offert (filtre historique, comportement
parfois préféré sur un télécinéma mal détecté — au moins le choix est explicite).

**⚠️ Le piège d'indicateur, qui a failli faire conclure l'inverse.** L'écart vertical moyen
(« peigne ») **reste élevé** après `bwdif` sur une image immobile : 195 sur 195 pour la mire
en lignes de 1 px. Ce n'est pas un défaut — c'est le **détail conservé**. Un filtre à
détection de mouvement qui ne voit pas de mouvement entre les deux trames laisse l'image
telle quelle, et c'est correct : dans une vraie source entrelacée, une scène immobile n'a
pas de peigne du tout (les deux trames y sont identiques). Conséquence pratique : **le banc
`verif-filtres.sh` ne pouvait plus tester la résorption du peigne sur une mire figée** —
il jugeait `bwdif` sur un cas qui n'existe pas. Il utilise désormais deux mires :
un objet **en mouvement** sur fond uni (le peigne réel : écart vertical 1,17 → 0,12) et une
mire de **détail fin fixe** (où l'on vérifie que `bwdif` conserve 195/195 et `yadif` tombe à
0). C'est la même leçon que le détramage en session 13, où la « rugosité » augmentait après
filtrage : l'indicateur doit correspondre à ce que l'œil voit, pas à ce qu'on croit mesurer.

**Ce qui est réglable.** `--` Préférences ▸ Image ▸ filtres ▸ « Filtre » (bwdif ou yadif),
avec la mesure rappelée sous le réglage ; `video.deinterlace_filter` dans `config.json` ;
la chaîne réellement construite (`bwdif=mode=send_frame:parity=auto:deint=interlaced`) est
affichée et journalisée, comme avant. Les deux filtres reçoivent la même parité et le même
`deint=interlaced` : une source progressive reste intacte.

**Vérifications** : `verif-filtres.sh` **16/16** (4 contrôles de plus : peigne réel résorbé
par les deux filtres, détail fin conservé par `bwdif`, effacé par `yadif`, et preuve que le
choix change réellement l'image), 4 tests unitaires sur la construction de la chaîne (nom du
filtre, parité pour les deux, aucun filtre quand c'est désactivé, relecture tolérante).

---

### 6.31 `\clip`, `\iclip`, `\frz`, `\org` : découpe et rotation ASS (session 28)

**Ce qui manquait.** Trois balises étaient retirées proprement — « le texte reste lisible,
il perd l'effet ». C'est exactement ce qui rend un sous-titre de fansub illisible : les
traductions en deux colonnes (japonais/anglais) reposent sur `\clip`, et beaucoup de
génériques sur `\frz`.

**Découpe.** `\clip(x1,y1,x2,y2)` et `\iclip` sont traduits en pixels de la zone affichée
(comme le reste : rapport `zone / PlayRes` par axe), puis en peintres egui restreints. Un
`\iclip` ne se dessine pas « à l'extérieur d'un rectangle » avec egui : on peint donc
**quatre bandes** autour de la zone retirée, sur un même fragment. Les `\clip` se
**cumulent par intersection** et les `\iclip` par **exclusion**, comme libass.

**Ce que `\clip` ne fait pas, et qui est dit.** Un chemin (`\clip(m 0 0 l 100 0 …)`) n'est
découpé exactement que s'il décrit un **rectangle aligné sur les axes** — le cas réel des
colonnes. Un triangle, une courbe, un « L » sont réduits à leurs bornes, et le champ
`Clip::exact` le consigne (journal `debug`). La différence est mesurable : masquer en plus
les coins d'un triangle n'est pas la même image, et il valait mieux l'annoncer que de
laisser croire à un rendu fidèle.

**Rotation.** `\frz` (et son synonyme `\fr`) fait tourner le texte autour de `\org(x,y)`
s'il est donné, sinon autour du point d'ancrage du texte (`\pos` ou le coin désigné par
l'alignement) — c'est ce que fait libass et c'est ce qui rend une rotation prévisible.

**Le portage a demandé de déplacer du code, pas seulement d'en ajouter.** Un fragment peut
maintenant être peint **plusieurs fois** (un `\iclip` = quatre bandes) : le corps du dessin
d'un fragment est donc sorti de `App::draw_ass` vers une fonction libre
`dessiner_fragment_ass`. Recopier ce bloc pour chaque peintre aurait fait diverger les
copies au premier changement. Le chemin sans rotation est conservé à l'identique
(`galley_with_override_text_color`) : c'est celui que tous les bancs précédents valident.
Quand il y a rotation, on passe par `TextShape` d'epaint (`angle`), et l'ombre, le contour
et le cadre tournent avec le texte — sans quoi l'ombre part du mauvais côté, ce qui se voit
immédiatement sur un titre incliné.

**Mesures** (`tools/verif-ass-clip.sh`, 11 contrôles, deux passes identiques) :

| Contrôle | Mesure |
|---|---|
| Texte de référence | 9 550 px, boîte x[234..1046], centré |
| `\clip(0,0,192,288)` (moitié gauche) | texte jusqu'à **x = 639** pour une frontière à **640** ; partie gauche intacte |
| `\iclip(144,0,240,288)` | bande centrale **0 pixel**, 3 114 px à gauche et 3 075 px à droite |
| `\frz90\org(192,144)` | boîte **185×61 → 61×185** (inversée), 2 181 px conservés (aucune lettre perdue) |
| `\org(192,250)` au lieu de `\org`=`\pos` | centre déplacé de **184 px** vers la droite |
| `\frx60` (hors périmètre) | texte inchangé : aucune balise brute, aucun effet parasite |

**Par ailleurs** : 11 tests unitaires de plus (89 au total) couvrent l'analyse des balises
(rectangle, bornes inversées, chemin rectangulaire exact, triangle non exact, échelle en
tête de chemin, cumul de deux `\clip`, mise à l'échelle du découpage) et la géométrie de la
rotation (pivot `\org`, pivot = point d'ancrage, `\frz0` sans effet, `\fr` = `\frz`).

**Piège du banc, qui aurait pu faire valider n'importe quoi** : le **pointeur de la
souris** est peint dans une capture d'écran X11 et se lit comme du texte clair. Il
ajoutait 32 pixels à x = 800, y = 452, ce qui faisait « déborder » le texte découpé
au-delà de sa frontière (mesuré x = 808 au lieu de 640). Deux corrections : souris rangée
dans un coin avant la capture, et filtrage des pixels isolés (un glyphe a des voisins, une
tache de 1 pixel n'en a pas).

---

### 6.30 Profils appliqués par règle : chemin, extension, résolution (session 27)

**La demande, dans les mots du « reste à faire »** : « les profils se choisissent à la
main ; une règle “ce dossier = anime” éviterait le clic ». Le point est plus important
qu'il n'y paraît : un profil *oublié* n'est pas un profil appliqué.

**Ce qu'est une règle.** Un triplet `(champ, motif, profil)` dans `config.json`
(`"regles": [...]`) :
- `chemin` — sous-chaîne du chemin du média, sans tenir compte de la casse ; `~/` est
  développé (`~/Films/4K` fonctionne alors que le média arrive en absolu) ;
- `extension` — comparaison sur l'extension, avec ou sans point, plusieurs valeurs
  séparées par des virgules (`mkv, avi`) ;
- `résolution` — seuil ou dimensions : `1920` (= au moins 1920 de large), `>=1920`,
  `>1280`, `<=1280`, `<1280`, `=1920`, `1280x720` (et `×`).

**Ordre, et pourquoi.** Les règles sont essayées **dans l'ordre de la liste**, la première
qui correspond gagne. Une « spécificité » calculée (dossier > extension > résolution)
aurait l'air plus savante, mais serait impossible à corriger quand elle surprend : ici,
l'utilisateur voit l'ordre à l'écran et le change en supprimant/ajoutant une ligne.

**Décision prise après chargement.** Une règle de résolution ne peut pas décider avant
que le média soit ouvert. L'application se fait donc dans `App::load_media` /
`play_playlist_index`, **après** `load_file` (résolution connue) et **avant**
`appliquer_filtres` (la chaîne doit être construite avec les bons réglages). Les chemins
sans interface (`--dump-frame`, `--dump-audio`) suivent la même logique, sinon la fonction
serait invérifiable.

**Une règle orpheline n'est pas une règle.** Si le profil visé a été supprimé, la règle est
ignorée (journalisée) et la suivante est essayée ; l'interface l'affiche en orange
« profil absent ». Un fichier édité à la main est lu avec tolérance : un `champ` inconnu ou
un motif vide est **écarté** (et signalé), le reste de la configuration est conservé.

**Défaut trouvé en chemin (et corrigé).** Appliquer un profil depuis l'interface appelait
`player.refresh_audio_chain()`, qui **recalcule à partir des réglages que le lecteur détient
déjà** — il ne relit pas la configuration. L'égaliseur du profil n'atteignait donc jamais le
moteur (le cas passait inaperçu : l'image, elle, changeait). Le bon point d'entrée est
`player.set_audio_filters(&config.audio)`, qui pose les réglages **et** reconstruit le
graphe. Deux traces ont été ajoutées pour que ce genre d'écart devienne visible :
`Traitement audio : chaîne mise à jour (<chaîne réelle>)` et, en `debug`,
`Traitement audio inchangé : demandé « … », actif « … »`.

**Interface** (préférences → Profils de réglages → « Profil appliqué automatiquement ») :
liste numérotée des règles avec leur cible, corbeille par règle, formulaire
(champ / motif avec exemple en filigrane / profil) et bouton « ➕ Ajouter la règle ».
Tout est journalisé (`prefs regles : N règle(s)`, `prefs regle i bouton supprimer rect`,
`prefs bouton ajouter regle rect`) : le banc d'essai clique sur la géométrie réelle.

**Mesures** (`tools/verif-regles.sh`, 24 contrôles, deux passes identiques) :

| Contrôle | Mesure |
|---|---|
| Règle de chemin | luminance 128,20 (hors dossier) → **168,38** (dans « anime ») |
| Règle d'extension | écart entre canaux RVB : .mp4 **3,5** (noir et blanc) / .mkv **246,1** |
| Résolution `>=1920` sur du 1280×720 | image **inchangée** (128,20) |
| Résolution `<=1280` sur le même fichier | **168,70** |
| Dimensions exactes `720x576` | écart RVB **3,5** |
| Deux règles qui correspondent | la première gagne (168,38 / couleur conservée) |
| Règle orpheline alors qu'une autre correspond | la suivante s'applique (3,5) |
| `--profil Gris` explicite | l'emporte sur la règle (3,5 ; luminosité non éclaircie) |
| Audio par règle (`--dump-audio`) | `superequalizer=1b=2.000…` installé |
| Interface : ajout / suppression par les boutons | journal + `config.json` (2 → 1 règle) |
| Interface : « Appliquer » d'un profil | `Traitement audio : chaîne mise à jour (superequalizer…)` |

Onze tests unitaires couvrent l'analyseur (seuils, bornes, listes d'extensions, `~`,
motif illisible, champ inconnu, ordre, orpheline, compatibilité d'un ancien fichier).

---

### 6.29 Recadrage : régler **et valider** (session 26)

**Le défaut signalé.** Le repère de recadrage s'affichait bien, mais il n'existait aucun
moyen de **terminer** le réglage : le recadrage restait en aperçu et n'était donc jamais
appliqué à l'image. La cause tenait à une confusion d'état : `crop_overlay` (une
*préférence*, « afficher le repère ») servait aussi de condition pour suspendre le
recadrage — si bien qu'avec le repère activé (état par défaut), la chaîne de filtres était
construite **sans** le recadrage, indéfiniment.

**Ce qui a été séparé.**
- `crop_overlay` : préférence persistée, « je veux voir le repère quand je règle » ;
- `App::crop_reglage` : **état de séance** (non persisté), « je suis en train de régler ».

L'aperçu (image d'origine affichée, recadrage suspendu) n'existe que lorsque
`crop_reglage && crop_active()`. Un recadrage enregistré est donc **appliqué au démarrage**,
ce qui est le comportement attendu et ce qui n'était pas le cas.

**Le cycle complet**, accessible par trois chemins équivalents :
1. bouger un curseur de recadrage (menu contextuel ou préférences) ou lancer la détection
   automatique → entre en réglage ;
2. la touche `R` : entre en réglage si un recadrage existe, **valide** si on est en réglage ;
3. les boutons « ✔ Valider le recadrage » / « ↺ Régler à nouveau », présents **dans les deux**
   interfaces (mêmes libellés, même fonction, même journalisation) ;
et `Entrée` valide, avec priorité sur le raccourci « plein écran » (Entrée en fait partie) —
la touche est consommée pour ce tour, sans interrompre le rendu.

**Faux défaut audio corrigé au passage.** Le banc d'horloge échouait de façon intermittente
sur « 1 bloc de sous-alimentation ». Diagnostic : ce bloc provenait du chemin `try_lock`
(l'interface tenait le tampon, le callback a émis du silence plutôt que d'attendre) — ce
n'est **pas** un manque de données. Les deux cas sont désormais comptés séparément
(`underruns` et `contentions`, tous deux affichés dans le journal), et une sous-alimentation
n'est comptée qu'après que le tampon a atteint sa profondeur cible au moins une fois. Le banc
reste strict (0 sous-alimentation) et passe deux fois de suite : le seuil n'a pas été
assoupli, le défaut était dans la mesure.

**Mesures** (`verif-personnalisation.sh`, 19 contrôles) : entrée en réglage par `R`, `Entrée`
valide, « le recadrage est réellement appliqué » (le filtre `crop` passe dans la chaîne), et
le cadre de réglage disparaît après validation — vérifié au pixel sur les captures. Le bouton
du menu contextuel est cliqué **à partir de la géométrie journalisée** (`menu-item … rect`),
jamais d'une coordonnée devinée.

---

### 6.28 Personnalisation : clic droit, raccourcis, repère de recadrage, miroir (session 25)

**Quatre demandes, quatre implémentations mesurées.** Le fil commun : tout ce qui était
« écrit en dur » devient une **donnée** (une table, une configuration), donc modifiable et
vérifiable.

**1. Menu contextuel au clic droit.** `Response::context_menu` — un vrai menu egui, qui se
ferme au clic extérieur ou par Échap. Il regroupe ce qui sert *pendant* la lecture :
aspect, miroirs, rotation, recadrage (curseurs + repère), vitesse, sous-titres, accès aux
préférences et au plein écran.
*Défaut trouvé* : la zone vidéo était un `egui::Image` **sans `Sense::click()`**. Un widget
sans sens n'est qu'un décor : egui ne lui transmet ni clic droit, ni double-clic, ni clic du
milieu (constaté : le menu ne s'ouvrait jamais, alors que le code était correct).

**2. Raccourcis clavier ET souris personnalisables.** Nouveau module `src/raccourcis.rs` :
41 actions, chacune avec un identifiant stable, un libellé, une catégorie et ses
combinaisons par défaut ; analyseur tolérant (`Ctrl+Maj+H`, `shift+ctrl+h`, `Entrée`/`Entree`,
synonymes anglais) ; table effective = défauts écrasés par la configuration.
Conséquences directes :
- le `match` géant de l'interface ne teste plus des **touches** mais des **actions** ;
- les trois vitesses de déplacement (`←`, `Maj+←`, `Ctrl+←`) deviennent **trois actions
  distinctes**, donc remappables séparément — c'était impossible auparavant ;
- l'aide `F1` est **construite à partir de la table** : elle ne peut plus mentir (l'ancienne
  liste écrite en dur aurait été fausse dès le premier raccourci modifié) ;
- un écran de réglages dédié : capture directe de la touche, remise au défaut par action,
  remise globale, et **détection des conflits** (deux actions sur la même combinaison : la
  première gagne, mais c'est écrit en orange).
- la souris passe par la même table : molette, clic du milieu, double-clic.

**3. Repère visuel du recadrage.** Bandes retirées assombries (45 %), zone conservée
encadrée (trait noir + trait blanc, visible sur toute image), étiquette des dimensions
conservées.
*Piège de conception* : appliquer le recadrage **retire** les bandes de l'image affichée ;
dessiner le repère dessus assombrirait du contenu **conservé** (mesuré : voile sur les
bords intérieurs de l'image). Le repère n'a de sens que sur l'**image d'origine** : d'où
`crop_apercu`, qui construit la chaîne de filtres **sans** le recadrage tant que le repère
est affiché, et l'étiquette « aperçu ». Le réglage terminé, le recadrage réel s'applique.

**4. Miroir horizontal.** Champ `flip_h` (filtre `hflip`), à côté du miroir vertical
existant (`vflip`, dont le champ garde son nom historique `flip` pour ne pas perdre les
configurations existantes). Les deux se cumulent ; `Ctrl+H` et `Ctrl+Maj+H` par défaut.

**Défaut latent corrigé au passage** : `Config`, `InterfaceSettings` et `PlaybackSettings`
n'avaient pas `#[serde(default)]`. Un champ manquant dans le fichier de configuration
faisait rejeter **toute** la configuration, qui retombait sur les valeurs par défaut — en
silence (une seule ligne d'avertissement). Constaté en écrivant un fichier de test partiel :
les raccourcis n'étaient jamais pris en compte. Désormais un fichier partiel est accepté.

**Mesures** (`tools/verif-personnalisation.sh`, 13 contrôles, 16 tests unitaires de plus) :

| Contrôle | Résultat |
|---|---|
| Clic droit | panneau ouvert (10,7 % de la zone modifiée) et une entrée agit (hflip appliqué) |
| Touche réaffectée | Espace → X : l'ancienne touche ne déclenche **plus rien**, la nouvelle met en pause |
| Action désactivée | combinaison vide : la touche ne répond plus |
| Conflit | signalé en orange à l'écran |
| Repère de recadrage | cadre de y=57 à y=624, bandes **51** contre **107** au centre |
| Sans recadrage | aucun repère dessiné |
| Miroir horizontal | inversion mesurée à **1,8/255** (bord gauche ↔ ancien bord droit) |
| Miroir vertical | se cumule (`hflip,vflip`) |

**Limite assumée** : le clic droit est réservé au menu contextuel ; il n'est donc pas
proposé comme raccourci (l'interface le dit explicitement dans les réglages).

---

### 6.27 Menus : audit entrée par entrée, glyphes et ordre (session 24)

**Ce qui a été fait.** Les six menus ont été photographiés et relus un par un, puis un banc
les vérifie automatiquement (`tools/audit-menus.sh`, 31 contrôles) : présence des entrées,
lisibilité (rectangle dans la fenêtre, largeur plausible) et **effet réel** de 19 entrées.

**Défauts trouvés et corrigés.**

| Défaut | Constat | Correction |
|---|---|---|
| 🔴 **Glyphes non dessinés** | `← → ↑ ↓ ● ✕` s'affichaient en **carrés vides**, y compris dans l'aide des raccourcis (« ← → Reculer / avancer de 5 s ») | police système en **secours** (DejaVuSans) ajoutée en dernier recours + filet `texte_sur` ; vérifié dans les tables de glyphes que 69 des 70 caractères non-ASCII utilisés sont désormais couverts |
| `⏯` (Lecture/Pause) | absent de **toutes** les polices disponibles : carré vide | remplacé par `⏸`/`▶` selon l'action à venir, et le libellé **dit** l'action (« Pause » / « Lecture ») |
| « Fermer » (menu Fichier) | ne libérait que le média : la fenêtre restait ouverte, vide, sans rien annoncer | ferme la fenêtre (`ViewportCommand::Close`) |
| « (image, non affichable) » | devenu **faux** depuis la session 23 | « (image) » |
| Volume du menu Audio | borné à 150 % **en dur** : le réglage « volume maximal » (jusqu'à 300 %) ne s'appliquait pas | borne = `volume_max` configuré |
| « Mute » | anglicisme isolé dans une interface française | « Couper le son » / « Rétablir le son » (libellé selon l'état) |
| Vitesse (menu Lecture) | « Vitesse: » + slider + « 1.00x » = trois fois la même information | un seul libellé ; bornes partagées `VITESSE_MIN`/`VITESSE_MAX` (le moteur, le menu et les préférences divergeaient) |
| Ordre du menu Sous-titres | commençait par un **comportement** (« Chargement automatique ») au-dessus de l'affichage immédiat | affichage → piste du fichier → fichiers externes → chargement automatique |
| Playlist | aucune indication de contenu | « N élément(s) — i/N » |
| Préférences | « Interface » ouvrait en fait **tous** les réglages ; « Statistiques » ne pouvait qu'ouvrir, jamais fermer | « Réglages… (F5) », « Statistiques (I) » bascule, « À propos (F2) » |
| 7 entrées **muettes** | aucune trace au journal (affichage des sous-titres, panneau playlist, ouverture des fenêtres…) : impossible de les diagnostiquer | chacune journalise son effet (règle du projet : une action doit être vérifiable) |

**Le banc, et ses propres pièges.** Trois mécanismes ont dû être corrigés **dans l'audit**,
chacun produisant de faux échecs (menus parfaitement fonctionnels déclarés inertes) :

1. **Le journal n'était pas borné à l'ouverture du menu** : les entrées des menus déjà
   ouverts restaient dans la liste et l'audit cliquait des rectangles périmés → marque de
   position (`MARQUE`) avant chaque ouverture.
2. **Une fenêtre laissée ouverte capte les clics suivants** (« À propos » masquait tout le
   reste) → fermeture explicite entre deux contrôles, par clic dans le vide (Échap ferme la
   fenêtre au-dessus, pas le menu).
3. **Un bandeau d'état (OSD) décale le menu de ~25 px** quand il apparaît ou disparaît :
   après un changement de piste audio, un contrôle sur deux échouait → le rectangle est relu
   jusqu'à stabilité avant le clic.

Le critère d'effet a aussi été rendu robuste : on compare le **nombre d'occurrences** du
message attendu avant et après le clic, au lieu de compter les lignes apparues (le vidage du
journal faisait manquer l'effet).

**Résultat** : 31 contrôles, **0 échec**, deux passes consécutives identiques.

---

### 6.26 Sous-titres image PGS/DVD : rendu et défaut de cadrage (session 23)

**Ce qui manquait.** Les pistes image étaient reconnues puis **refusées** : « aucun texte à
afficher ». Leur rendu demande de décoder les rectangles d'indices de palette du décodeur,
de les convertir en RGBA et de les composer sur la vidéo à leur place.

**Défaut de cadrage trouvé au passage (il touchait AUSSI le texte et l'ASS).** Les
sous-titres étaient positionnés dans `image.rect`, la zone **allouée** au widget. Sous
`centered_and_justified`, cette zone vaut tout l'espace disponible, donc elle est **plus
grande que l'image** dès que le rapport d'aspect du média diffère de celui de la fenêtre.
Conséquence mesurée : un rectangle de 400 px s'affichait sur 264 px (étiré de 7 %), et tous
les sous-titres texte étaient décalés d'une dizaine de pixels. Correction : la zone est
recalculée comme `Rect::from_center_size(response.rect.center(), disp)` — `disp` étant la
taille d'affichage réellement demandée. Après correction : 252×51 px pour 252×50 attendus.

**Le format, mesuré et non supposé.** Sans encodeur PGS dans FFmpeg, il a fallu écrire le
flux à la main (`tools/fabrique-pgs.py`). Trois détails ne se devinent pas :

| Détail | Ce qui échoue | Ce qui marche |
|---|---|---|
| Taille du segment dans `.sup` | 3 octets (taille « logique ») → « Picture size 0x0 » | **2 octets** (`sup` demuxer) |
| État de composition (PCS) | `0x02` → aucune image | **`0x80`** (début d'époque, bits hauts) |
| RLE | `couleur, longueur` sur 2 octets → « Decoded 2 pixels, line should be 200 » | **`0x00, drapeaux[, longueur basse][, couleur]`** |

Le RLE a été reconstitué depuis `libavcodec/pgssubdec.c` (`decode_rle`) : un octet non nul =
1 pixel, `0x00` + drapeaux = bloc (longueur = `F & 0x3f`, `F & 0x40` → longueur sur 2 octets,
`F & 0x80` → couleur explicite), `0x00 0x00` = fin de ligne. Avec ces trois corrections, le
flux se décode sans un seul avertissement et se convertit en `dvdsub` (ce qui a fourni un
second média de test, format DVD).

**Mesures** (`tools/verif-sousimages.sh`, 10 contrôles, 10 tests unitaires) :

| Contrôle | Résultat |
|---|---|
| Géométrie décodée | 400×80 en (760,900), conforme au flux écrit |
| Pixels | 32 000 opaques (400 × 80), ni plus ni moins |
| Déterminisme | empreinte `77ef325bcc38e525` identique à chaque exécution |
| Taille à l'écran | 252×51 px pour 252×50 attendus (±1 px) |
| Couleur | R=255 G=249 B=0 — palette respectée |
| Extinction | 0 pixel clair après la fin de l'événement |
| Non-régression | sous-titres texte toujours lus et affichés |

**Limite assumée** : après un **saut de position**, un sous-titre image déjà commencé
n'apparaît qu'au **prochain** événement. Le format décrit tout par « compositions » : le
décodeur ne peut reconstituer l'état qu'en recevant le display set qui l'a établi, et celui-ci
est antérieur au point de reprise. Les événements en mémoire sont vidés au seek (sinon un
événement sans extinction resterait affiché indéfiniment). Les sous-titres texte, eux, se
réaffichent immédiatement.

---

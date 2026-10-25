# RMPlayer — Rust Media Player

Lecteur vidéo pour Linux, écrit en **Rust**, s'appuyant directement sur **FFmpeg 7.x**
(libav + libswscale + libswresample) et **egui/eframe** pour l'interface, **cpal** pour
l'audio (PipeWire / PulseAudio / ALSA).

Objectif : les fonctionnalités d'un lecteur de bureau complet, sans dépendre d'un lecteur
externe ni d'un moteur web.

---

## Compilation

```bash
cargo build --release
```

**Prérequis**

| Élément | Détail |
|---|---|
| FFmpeg | 7.1 installé dans `/opt/ffmpeg-7.1` (voir `.cargo/config.toml`) |
| Audio | PipeWire, PulseAudio ou ALSA. La **sélection de la sortie** par son nom demande PipeWire et ses outils `pw-dump` / `pw-metadata` (sans eux : sortie par défaut et sorties ALSA directes) |
| Affichage | X11 ou Wayland |

> Le profil `release` conserve volontairement `debug = "line-tables-only"` et
> `strip = false` : le binaire est plus gros (~52 Mo) mais les rapports de crash sont
> exploitables (`addr2line`). Voir `docs/AVANCEMENT.md` §6.1.

---

## Lancement

```bash
# Script fourni : positionne LD_LIBRARY_PATH (et refuse de démarrer sans session graphique)
./run.sh film.mp4

# Plusieurs fichiers = playlist (le premier est lu)
./run.sh a.mp4 b.mkv c.avi

# Démarrage en plein écran
./run.sh --fullscreen film.mp4

# Forcer le décodage logiciel (comparaison, ou pilote récalcitrant)
./run.sh --no-hwdec film.mp4
```

Directement :

```bash
LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib ./target/release/rmplayer film.mp4
```

### Journaux détaillés

Les messages d'FFmpeg sont redirigés vers le journal de l'application et n'apparaissent
qu'en mode verbeux :

```bash
RUST_LOG=rmplayer=debug,ffmpeg=debug ./run.sh film.mkv
```

### Si le son ne fonctionne pas

Depuis un shell hors session graphique, exportez le dossier d'exécution utilisateur :

```bash
XDG_RUNTIME_DIR=/run/user/1000 LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib \
  ./target/release/rmplayer film.mp4
```

Sans cela, PipeWire est injoignable et ALSA renvoie `Host is down (112)`. Le lecteur
continue alors sans son (avertissement « lecture vidéo seule »).

---

## Utilisation

### Raccourcis clavier

| Touche | Action |
|---|---|
| `Q` | Fermer le lecteur |
| `Espace`, `clic du milieu` | Lecture / pause |
| `N` | Élément suivant de la playlist |
| `P` | Élément précédent de la playlist |
| `←`, `Ctrl+Molette↓` | Reculer de 5 s |
| `Maj+←` | Reculer de 1 s |
| `Ctrl+←` | Reculer de 30 s |
| `→`, `Ctrl+Molette↑` | Avancer de 5 s |
| `Maj+→` | Avancer de 1 s |
| `Ctrl+→` | Avancer de 30 s |
| `↑`, `Molette↑` | Volume + |
| `↓`, `Molette↓` | Volume − |
| `M` | Couper / rétablir le son |
| `Ctrl+L` | Lecture en boucle |
| `Ctrl+A` | Lecture aléatoire (playlist) |
| `Ctrl+R` | Reprise à la position quittée |
| `A` | Répétition A-B : point A |
| `B` | Répétition A-B : point B |
| `Ctrl+B` | Signet : ajouter à la position |
| `Ctrl+Maj+→` | Signet : aller au suivant |
| `Ctrl+Maj+←` | Signet : aller au précédent |
| `Ctrl+Maj+B` | Signet : supprimer celui de la position |
| `Ctrl+Maj+N` | Signet : renommer celui de la position |
| `[`, `Maj+Molette↓` | Vitesse −0,1 |
| `]`, `Maj+Molette↑` | Vitesse +0,1 |
| `\` | Vitesse normale (1,0×) |
| `Page↑` | Chapitre précédent |
| `Page↓` | Chapitre suivant |
| `.` | Image suivante |
| `,` | Image précédente |
| `V`, `double-clic` | Afficher / masquer les sous-titres |
| `J` | Piste de sous-titres suivante |
| `Ctrl+J` | Piste audio suivante |
| `Ctrl+Maj+J` | Sortie audio suivante |
| `F`, `Entrée`, `F11` | Plein écran |
| `Échap` | Fermer la fenêtre / sortir du plein écran |
| `S` | Capture d'écran (PNG) |
| `K` | Rapport d'aspect suivant |
| `Ctrl+K` | Remplir la fenêtre en rognant (bascule) |
| `Alt+←` | Image : glisser vers la gauche |
| `Alt+→` | Image : glisser vers la droite |
| `Alt+↑` | Image : glisser vers le haut |
| `Alt+↓` | Image : glisser vers le bas |
| `Alt+0` | Image : recentrer |
| `Ctrl+H` | Miroir horizontal |
| `Ctrl+Maj+H` | Miroir vertical |
| `R` | Afficher / masquer la zone de recadrage |
| `T` | Fenêtre toujours au-dessus |
| `Tab` | Masquer / afficher l'interface |
| `O` | Bandeau d'informations (OSD) |
| `I` | Fenêtre des statistiques |
| `L` | Panneau de playlist |
| `F1` | Aide des raccourcis |
| `F2` | À propos |
| `F5` | Préférences |
| `H` | Décodage matériel ↔ logiciel |

Cette table est celle de `src/raccourcis.rs` : **tous** ces raccourcis (clavier et souris) se
réaffectent dans *Préférences ▸ Raccourcis*, et l'aide `F1` affiche ceux qui sont réellement actifs.
`Clic sur la barre` de progression place la lecture à la position cliquée (ou glisser).

Les fenêtres *Préférences*, *Aide des raccourcis*, *Statistiques* et *À propos* se ferment par
leur bouton **✕** ou par **`Échap`**, et peuvent être **détachées** (voir plus bas).

> `F12` n'est **pas** utilisable : sous KDE, ce raccourci est réservé par le bureau
> (Yakuake) et n'atteint jamais l'application — d'où le choix de `F2`.

### La fenêtre retrouve sa place

Taille, état agrandi et — selon la session — position sont mémorisés
(`interface.window_pos`, `window_size`, `window_maximized`) : le lecteur se rouvre comme on
l'avait laissé. L'écriture attend **0,6 s de stabilité** (pendant qu'on tire un bord, la
taille change à chaque image) et n'a jamais lieu en plein écran ni fenêtre réduite.

**Plasma 6.8 a supprimé la session X11** (octobre 2026), et Wayland ne donne pas la position
d'une fenêtre à son client ni ne l'autorise à se placer : le lecteur y restitue la **taille**
et l'**état agrandi**, jamais une position inventée. Pour la position, c'est une **règle de
fenêtre du compositeur** qui s'en charge :

```bash
tools/regle-fenetre-kwin.sh                     # position 240,200 par défaut
tools/regle-fenetre-kwin.sh --position 100,80   # une place précise
tools/regle-fenetre-kwin.sh --depuis-fenetre    # adopter la place de la fenêtre ouverte
tools/regle-fenetre-kwin.sh --etat | --retirer
```

Le script sauvegarde `~/.config/kwinrulesrc` avant de le modifier (et `--retirer` défait
tout). Sous X11/XWayland, le lecteur fait le travail seul : position et taille sont
restituées (`tools/verif-fenetre.sh`, 9 contrôles ; `tools/verif-fenetre-wayland.sh`,
12 contrôles sur la session réelle).

### HDR : films HDR10 et HLG sur un écran ordinaire

Un film HDR lu tel quel apparaît **lavé** (noirs gris, hautes lumières écrasées). RMPlayer
détecte le transfert (PQ = HDR10, HLG = diffusion), les primaires BT.2020 et la profondeur, puis
applique une **conversion tonale** vers SDR :

- **moteur GPU** (`libplacebo`, Vulkan) : BT.2390 par défaut, traitement du gamut, détection de
  crête dynamique, tramage, débanding — tout en une passe sur la carte ;
- **repli CPU borné** (`zscale` + `tonemap`) : si Vulkan est refusé à l'exécution, l'étage CPU
  prend le relais pour les sources qui tiennent dans le budget mesuré (≤ 1080p30, 720p60) ; au-delà
  (1080p60, 1440p, 4K), une conversion saccadée serait pire qu'une image non convertie, donc
  l'image est jouée **telle quelle** (délavée sur un écran SDR). Dans les deux cas le refus est
  journalisé et un **avis s'affiche à l'écran** (`verif-hdr`, section 12). Le moteur CPU se force
  aussi à la main (`RMPLAYER_HDR_MOTEUR=cpu`), sans condition de budget.

Réglages dans *Préférences ▸ Image — filtres ▸ HDR et 10 bits* : mode (auto / toujours / jamais),
opérateur, gamut, détail des hautes lumières, tramage 10→8 bits, débanding GPU.

**Limite assumée** : la fenêtre est rendue en 8 bits SDR, donc pas encore de *passthrough* HDR
même sur un écran HDR. Le module est prêt pour le jour où une surface 10 bits existera : c'est le
mode « jamais » qu'il faudra brancher.

*Prérequis* : FFmpeg compilé avec `--enable-libplacebo --enable-libzimg --enable-vulkan`
(`tools/etat-court.sh` affiche « HDR (libplacebo) oui/non »).

### Touches média et widget du bureau (MPRIS)

RMPlayer expose `org.mpris.MediaPlayer2.rmplayer` sur le bus de session : les touches média du
clavier (lecture/pause, suivant, précédent) et le widget « lecture en cours » de Plasma/GNOME
agissent sur le lecteur et affichent ce qui joue (titre, durée, position).

Les commandes reçues passent par **les chemins de l'interface** (les mêmes que les boutons) :
règles de profil, sous-titres et OSD s'appliquent, et la fenêtre est **réveillée** même en pause
— sans quoi une touche média semblait morte dès que la lecture était arrêtée.

```bash
gdbus call --session --dest org.mpris.MediaPlayer2.rmplayer \
  --object-path /org/mpris/MediaPlayer2 \
  --method org.mpris.MediaPlayer2.Player.PlayPause
```

Sans bus de session (session minimale, banc d'essai), le lecteur fonctionne normalement :
MPRIS est simplement indisponible, et le journal le dit.

### Ouvrir un fichier : association et instance unique

```bash
tools/installer-association.sh                 # lanceur + icône + « Ouvrir avec »
tools/installer-association.sh --par-defaut    # et lecteur par défaut des formats gérés
tools/installer-association.sh --etat | --retirer
```

Le lanceur (`~/.local/share/applications/rmplayer.desktop`) déclare 16 types MIME — tous
présents dans shared-mime-info — et deux actions (plein écran, décodage logiciel). Sans
`--par-defaut`, RMPlayer apparaît dans « Ouvrir avec » **sans prendre la place** de VLC ou mpv.

**Une seule fenêtre.** Si le lecteur tourne déjà, un fichier ouvert depuis Dolphin lui est
transmis (socket Unix dans `$XDG_RUNTIME_DIR`, nom dérivé du dossier de configuration) : la
fenêtre existante charge le média et démarre la lecture, le second processus sort aussitôt
avec le code 0. `--nouvelle-instance` force une seconde fenêtre,
`RMPLAYER_INSTANCE_UNIQUE=0` désactive la mécanique (utile aux bancs).

### Décodage matériel (VA-API)

Le décodage vidéo passe par **VA-API** (Mesa / `radeonsi`) et retombe
automatiquement en logiciel si le pilote refuse le profil du flux — c'est le cas des
vidéos de téléphone en H.264 « pleine échelle » (`yuvj420p`), exactement comme avec mpv.
Le **repli est silencieux** du côté de libavcodec : c'est pourquoi la fenêtre
*Statistiques* (`i`) affiche le mode **réellement constaté** (« VA-API (matériel) » ou
« logiciel (CPU) »), et non le mode demandé.

La touche `H` bascule les deux modes en direct (le média est rechargé à la même
position : le mode se choisit à l'ouverture du flux). Le réglage est conservé dans
`video.hardware_decoding` (fichier de configuration JSON).

Gains mesurés (fenêtre 1280×720, 16 cœurs) :

| Source | Logiciel | VA-API |
|---|---|---|
| H.264 720p 30 i/s | 22 % | 22 % |
| H.264 2160p 30 i/s | 56 % | **37 %** |
| AV1 2160p 60 i/s | **décodable** depuis la s.31 (`libdav1d`) | **52 %** |

Le **AV1** se décode désormais **en logiciel** : le FFmpeg 7.1.3 de `/opt` a été recompilé
avec `--enable-libdav1d` (session 31). C'était nécessaire — le décodeur natif de FFmpeg 7.1
échoue sur un AV1 réel (« Error submitting packet to decoder: Function not implemented »),
et `av1dec` exige l'accélération matérielle. Le repli logiciel n'est donc plus une limite.

### Choisir la sortie audio

Le lecteur propose les **vraies sorties du système** par leur nom (enceinte Bluetooth, casque,
écran HDMI…), pas les ~35 noms ALSA bruts (`dmix:CARD=HDMI,DEV=3`) que `cpal` énumère. Trois
façons de choisir, qui passent par le même code :

- le menu **Audio** (section « Sortie audio ») ;
- *Préférences ▸ Audio ▸ Sortie* (les périphériques ALSA `hw:` / `plughw:` restent proposés en
  « avancé ») ;
- **`Ctrl+Maj+J`** : sortie suivante, pendant la lecture.

Le changement est **immédiat et sans coupure** : le flux est redirigé dans PipeWire, le film n'est
pas rouvert. « Périphérique par défaut » suit la sortie du système. Une sortie disparue (enceinte
éteinte) laisse la lecture sur la sortie par défaut. `rmplayer --list-audio` liste les sorties
(`pw:<nom> — <description>`). Réglage : `audio.device` (`""`, `pw:<nom>`, ou un ancien nom ALSA).
Mesuré au son, sur deux sorties factices : `tools/verif-peripheriques.sh`.

### Fenêtres détachables

Les fenêtres *Préférences*, *Statistiques*, *Aide des raccourcis* et *À propos* sont dessinées dans
la fenêtre du lecteur : elles n'en sortent pas, et une petite fenêtre les rogne. Le bouton
**⧉ Détacher** en fait de **vraies fenêtres système**, déplaçables sur un autre écran ; **⧉
Rattacher** les ramène, `Échap` les ferme. Le choix est mémorisé fenêtre par fenêtre
(`interface.detached_windows`). Les menus déroulants de la barre et la boîte « Ouvrir une
adresse » restent dans le lecteur. Mesuré côté système : `tools/verif-fenetres-detachables.sh`.

### Captures d'écran

`S` enregistre l'image affichée. *Préférences ▸ Interface ▸ Captures d'écran* règle :

- le **format** : **PNG** (sans perte, défaut), **JPEG** (qualité 1–100, 92 par défaut) ou **WebP**
  (toujours *sans perte* : le seul encodeur disponible hors ligne ; un WebP avec perte demanderait
  `libwebp`) ;
- la **taille** : l'image telle qu'affichée (réglages d'image et HDR appliqués), ou la **taille
  d'origine du film** (décodée à part, sans ces réglages) ;
- le **nom** : un modèle avec `{media}` (titre du film), `{position}` (HH-MM-SS), `{date}` et
  `{heure}` — par défaut `rmplayer-{date}-{heure}`. Les caractères interdits (`/`, `:`…) deviennent
  `_`, et un fichier existant n'est jamais écrasé (`-2`, `-3`…) ;
- le **dossier**.

### Au quotidien

- **Fichiers récents** (menu *Fichier*, 10 entrées, « effacer ») ; **lecture aléatoire** et
  **glisser-déposer** dans la playlist ; reprise à la position quittée.
- **Signets** par média : `Ctrl+B` pose, `Ctrl+Maj+→/←` navigue, `Ctrl+Maj+B` supprime,
  `Ctrl+Maj+N` **renomme** celui de la position (ou le crayon ✎ du menu *Lecture ▸ Signets*) ;
  conservés d'une session à l'autre.
- **Répétition A-B** (`A` / `B`) avec nombre de répétitions et délai ; `--boucle-ab A B` depuis la
  ligne de commande.
- **Vignettes** : survoler la barre de progression montre l'image de l'instant pointé.
- **Pause selon le focus** : jamais, à la perte de focus, ou fenêtre réduite ; reprise au retour.
- **Souris avec modificateurs** : `Ctrl+molette` ±5 s, `Maj+molette` vitesse (personnalisable).
- **Saut en pause** : l'image et la réplique de sous-titre suivent un saut même en pause.

### Intégration au système

- **Icône de zone de notification** (StatusNotifierItem, KDE Plasma) : *Préférences ▸ Lecture ▸ Système*.
- **Démarrage automatique** à l'ouverture de session : *Préférences ▸ Lecture ▸ Système* ou
  `rmplayer --autostart on|off|status`.
- **Priorité du processus** (nice de tous les fils) et **fréquence d'images maximale**.
- **Décodeur par codec** : forcer un décodeur précis (repli sur le défaut s'il ne s'ouvre pas) ;
  `--list-tracks` indique le décodeur réellement ouvert.
- **Thèmes externes** : un fichier JSON dans `~/.config/rmplayer/themes/` (un thème illisible est
  refusé avec la raison).
- **Export / import de la configuration** : `rmplayer --export-config f.json`,
  `--import-config f.json` (refus sans écraser si le fichier est invalide).
- **Reprise de l'ancien nom** : au premier lancement, les données de KPlayer (`~/.config/kplayer`,
  `~/.cache/kplayer`) sont **copiées** dans `rmplayer` — jamais supprimées.

### Préférences (F5)

Fenêtre organisée en **sections dépliables** — `src/settings.rs`, table `SECTIONS` — bornée
à l'écran et défilante. Chaque section affiche son rôle, un repère « • modifié » quand elle
s'écarte des valeurs par défaut, et deux boutons « tout déplier / tout replier » sont en
tête. Tous les réglages affichés sont **réellement appliqués** :

- **Interface** : thème, titre, barre de progression, contrôles en plein écran, OSD et sa
  durée, **captures d'écran** (dossier, format, qualité, taille, nom : voir plus bas) ;
- **Vidéo** : décodage matériel VA-API, méthode de mise à l'échelle (Lanczos, Bicubique,
  Bilinéaire, Moyenne de zone, Plus proche voisin) et **mode de décodage constaté** ;
- **Image — filtres** : détramage, désentrelacement (`bwdif` par défaut, `yadif` au choix,
  départagés par mesure : +2,4 dB sur une image immobile et −15 % de temps de calcul pour
  `bwdif`), débruitage (0–3), luminosité,
  contraste, saturation, gamma, retour aux valeurs neutres ; sous-bloc **Avancé** avec le
  rayon et le seuil du détramage, la parité des trames, et la **chaîne FFmpeg réellement
  construite** ;
- **Vidéo** : en plus du décodage et de la mise à l'échelle — **netteté** (3 niveaux),
  **rotation** 0/90/180/270°, **miroir**, **zoom numérique** 1–4× ;
- **Audio** : **sortie** choisie parmi les vraies sorties du système (enceinte, casque, écran — par leur nom lisible, changement **en cours de lecture** sans coupure : menu Audio, préférences ou **Ctrl+Maj+J**), volume maximal, volume courant, muet,
  **égaliseur 18 bandes** (profils + courbe personnalisée, réglages en dB), **normaliseur
  de volume**, **mode nuit**, **réduction de bruit** (3 niveaux), **élargissement stéréo**,
  **crossfeed**, **karaoké**, **réverbération** (3 tailles), **décalage ±2 s**,
  **correction de hauteur** à vitesse ≠ 1 ;
- **Sous-titres** : chargement automatique, **style** (police, taille, couleur, contour et second
  contour, ombre, interligne, marges, échelle X/Y, fondu), **langues par priorité**, durées mini/maxi,
  **encodage** imposé des fichiers externes, mémoire de la piste choisie ; les pistes
  **image** du conteneur (**PGS** des Blu-ray, sous-images **DVD**, **DVB**) sont rendues
  telles quelles — position, taille et couleurs du disque — et non plus refusées ;
- **Lecture** : reprise à la position quittée, vitesse.

### Personnalisation de l'image

- **Rapport d'aspect** : automatique, 16:9, 4:3, 1,85:1, 2,35:1, remplir (étirer), **rogner**
  (couvrir la fenêtre sans bandes noires, en gardant le rapport), personnalisé — `K` les fait tous
  défiler (clavier comme souris) ; **`Ctrl+K`** bascule « rogner » et revient au mode précédent.
- **Glisser l'image** : `Alt+←/→/↑/↓` la décalent de 2 % par appui (jusqu'à ±50 %), `Alt+0` la
  recentre — les mêmes curseurs que *Préférences ▸ Vidéo*.
- **Bandes noires** : recadrage manuel (haut/bas/gauche/droite) ou **détection
  automatique** (analyse plusieurs images et retient la médiane des bandes mesurées).
- **Chapitres** : menu *Lecture*, touches `Page↑` / `Page↓`.
- **Avance image par image** : `,` et `.`.
- **Clic droit** : menu contextuel sur l'image — aspect, miroirs, rotation, recadrage,
  vitesse, sous-titres, préférences.
- **Raccourcis personnalisables** : clavier **et** souris (molette, clic du milieu,
  double-clic). *Réglages ▸ Raccourcis clavier et souris* : capture directe de la
  combinaison, remise au défaut, conflits signalés. L'aide `F1` affiche les raccourcis
  réellement actifs.
- **Recadrage en deux temps** : on **règle** (l'image d'origine s'affiche, la partie retirée
  est assombrie et la partie conservée encadrée avec ses dimensions), puis on **valide**
  — « ✔ Valider le recadrage » dans le menu contextuel ou les préférences, ou la touche
  `R`, ou `Entrée`. Le recadrage s'applique alors réellement ; « ↺ Régler à nouveau »
  réaffiche l'image d'origine pour ajuster.
- **Miroir horizontal** (`Ctrl+H`) et vertical (`Ctrl+Maj+H`), cumulables.
- **Flux réseau** : *Fichier ▸ Ouvrir une adresse* (http, HLS, RTSP, RTMP, SMB, FTP), ou
  directement `rmplayer https://exemple.test/flux.m3u8`. Une **coupure n'arrête plus la
  lecture** : le flux est rouvert automatiquement (8 essais, 0,3 s → 8 s), la lecture
  reprend là où l'on en était, un bandeau l'indique et reste affiché pendant l'attente.
- **Pré-cache d'un flux** : le flux est lu **en avance** (réserve réglable, 6 s par défaut,
  dans *Préférences ▸ Lecture*). Une coupure plus courte que la réserve passe donc
  inaperçue : mesuré 0,46 s de blocage contre 4,34 s sans réserve. Un fichier local n'est
  jamais pré-caché.
- **Synchronisation image/son** : la cadence suit la **carte son** (échantillons réellement
  consommés), et non le temps mural — l'image reste à moins d'une image du son, y compris
  avec un périphérique dont l'horloge est indépendante (Bluetooth, HDMI).

### Profils de réglages

Un **profil** retient la vidéo (filtres, cadrage, aspect), l'audio (égaliseur, traitements)
et les sous-titres (style, langue). Utile pour ne pas refaire dix réglages à chaque film
sombre, anime ou vieux DVD.

*Préférences ▸ Profils de réglages* : enregistrer, appliquer, exporter et importer (JSON).
Ligne de commande : `rmplayer --profil "Film sombre" film.mkv`, `rmplayer --list-profiles`.
Le périphérique de sortie audio n'est jamais écrasé par un profil (il est propre à la
machine).

**Application automatique par règle.** Une règle dit « ce genre de média → ce profil » :

| Champ | Motif | Exemple |
|---|---|---|
| `chemin` | sous-chaîne du chemin, sans tenir compte de la casse (`~/` accepté) | `anime`, `~/Films/4K` |
| `extension` | extensions, avec ou sans point, séparées par des virgules | `mkv, avi` |
| `résolution` | seuil de largeur, dimensions exactes | `1920`, `>=1920`, `<1280`, `720x576` |

Elles sont essayées **dans l'ordre affiché** (la première qui correspond gagne) ; une règle
dont le profil a été supprimé est ignorée et la suivante prend le relais. `--profil <nom>`
reste prioritaire sur toute règle. Réglage et liste :
*Préférences ▸ Profils de réglages ▸ Profil appliqué automatiquement* ; lecture seule et
diagnostic : `rmplayer --list-regles`.

### Sous-titres ASS

Les fichiers **ASS/SSA** sont rendus avec **leur propre mise en page** : position (`\pos`),
alignement (`\an`), couleurs, tailles, contour, ombre, fond, fondus (`\fad`) et sauts de
ligne — à l'échelle de la résolution de référence du fichier (`PlayRes`).

Les **animations** sont prises en charge : déplacement (`\move`), taille et couleurs
progressives (`\t`), **karaoké** (`\k` net, `\kf`/`\K` balayé, couleur secondaire `\2c`).
**Découpe et rotation** : `\clip(x1,y1,x2,y2)` et `\iclip` limitent le texte à une zone
(le cas des sous-titres en deux colonnes), `\frz`/`\fr` le font pivoter, `\org` fixe le
point de rotation. Un chemin `\clip` qui décrit un rectangle est découpé exactement ; un
chemin quelconque (triangle, courbe) est réduit à ses bornes — c'est une limite assumée.

Ne sont pas gérés (et sont ignorés proprement, le texte restant lisible) : les
**dessins vectoriels** `\p` et les rotations 3D `\frx`/`\fry`.

`rmplayer --ass-dump fichier.ass` affiche l'analyse (styles, positions, couleurs) sans
écran : utile pour comprendre pourquoi un sous-titre s'affiche ailleurs que prévu.

### Style des sous-titres

Couleur du texte et du contour, épaisseur du contour, bandeau de fond, taille, position
verticale, alignement, gras — et **décalage temporel de −10 s à +10 s** pour rattraper un
fichier mal minuté. S'applique aux fichiers externes comme aux pistes du conteneur.

### Ligne de commande utile

`--seek <secondes|mm:ss>` démarre à une position précise, `--speed <0.1-8>` fixe la vitesse
de départ, `--profil <nom>` applique un profil, `--list-tracks` / `--list-profiles` / `--list-regles`
inventorient, `--ass-dump <fichier.ass>` montre comment un ASS est compris (styles,
positions, animations), `--dump-subs` / `--dump-images` / `--dump-audio` / `--dump-frame` servent aux mesures
(`--dump-images` détaille les sous-titres **image** : rectangles, pixels, empreinte).
Une adresse de flux peut remplacer le nom de fichier. Autres options : `--boucle-ab <A> <B>`, `--export-config` / `--import-config`, `--autostart on|off|status`, `--list-audio`, `--hdr-info <fichier>`, `--nouvelle-instance`, `--fullscreen`, `--no-hwdec`. `--audio-buffer <ms>` impose la profondeur du tampon audio : c'est l'option de mesure qui
démontre que cette profondeur n'a **aucun** effet sur la synchronisation image/son (la
carte son fait foi) — voir `tools/verif-horloge.sh`.

### Pistes du fichier

Le lecteur énumère les pistes **audio** et de **sous-titres** du conteneur :

- menu **Audio** : liste des pistes (langue, titre, codec, canaux, taux), changement **à chaud**
  sans recharger le média ; `Ctrl+J` passe à la suivante. Au chargement, le choix suit une
  **liste de langues par priorité** (*Préférences ▸ Audio*, ex. `fre, eng`), écarte les pistes de
  **commentaire et d'audiodescription** (drapeau du conteneur ou mot du titre) tant qu'une autre
  existe, retient la piste « par défaut » du fichier à défaut de langue, et peut **mémoriser par
  film** la piste choisie à la main ;
- menu **Sous-titres** : liste des pistes du fichier, possibilité de les désactiver ;
  un fichier de sous-titres externe reste **prioritaire** s'il est chargé.

Les sous-titres **image** (PGS des Blu-ray, DVD, DVB) sont rendus tels quels (position, taille et
couleurs du disque).

En ligne de commande : `--list-tracks`, `--audio-track <n>`, `--sub-track <n>`,
`--dump-subs <fichier.txt>`.

### Sous-titres externes

Les fichiers **SRT** et **ASS/SSA** portant le même nom que la vidéo sont chargés
automatiquement (ainsi que ceux des sous-dossiers `subs/`, `subtitles/`,
`sous-titres/`, `sub/`). Les pistes peuvent aussi être chargées via le menu
**Sous-titres**.

---

## Diagnostic

### Rapport de crash

En cas de plantage, un rapport est écrit dans :

```
~/.cache/rmplayer/crash.log
```

Il contient l'adresse de base du binaire, le signal, l'adresse fautive, les registres et
un vidage de pile. Pour retrouver la fonction et la ligne :

```bash
addr2line -f -C -e target/release/rmplayer <rip_rel>
```

Le dispositif est autonome et **préserve le core dump** du système.

### Tester le rapport de crash

```bash
./target/release/rmplayer --crash-test
```

---

## Documentation

- **[docs/ETAT-COURT.md](docs/ETAT-COURT.md)** — **fiche de reprise d'une page** : chiffres
  mesurés (régénérés par `tools/etat-court.sh`), optimisations en place, chantiers suivants,
  cinq pièges. C'est le document à lire pour reprendre.
- **[docs/AVANCEMENT-ARCHIVE.md](docs/AVANCEMENT-ARCHIVE.md)** — historique détaillé des
  sessions 2 à 31 (déplacé de `AVANCEMENT.md` pour que celui-ci reste court).
- **[docs/ETAT-DU-PROJET.md](docs/ETAT-DU-PROJET.md)** — ⭐ **état d'ensemble et travail
  restant** : ce qui fonctionne, ce qui reste, règles du projet, sauvegarde.
- **[docs/AVANCEMENT.md](docs/AVANCEMENT.md)** — détail des sessions, bugs corrigés (avec
  les causes racines), pièges à connaître.
- **[docs/INVENTAIRE-FONCTIONNALITES.md](docs/INVENTAIRE-FONCTIONNALITES.md)** — ce qui
  fonctionne réellement vs ce qui est annoncé sans l'être.
- **[docs/PROCHAINES-FONCTIONS.md](docs/PROCHAINES-FONCTIONS.md)** — ce qui reste à faire, avec
  la raison de chaque fonction non faite.
- **[docs/AJOUTER-UNE-FONCTION.md](docs/AJOUTER-UNE-FONCTION.md)** — la recette pour ajouter une
  fonction sans produire un faux « fait » (mesure d'abord, témoin positif).
- **[docs/HDR.md](docs/HDR.md)** — la chaîne HDR et ses limites.
- **[docs/OPTIMISATION.md](docs/OPTIMISATION.md)** — où part le temps
  (mesuré) et le plan d'optimisation chiffré.
- **[docs/ETAT-TECHNIQUE.md](docs/ETAT-TECHNIQUE.md)** — environnement, API du moteur,
  procédure de diagnostic.

---

## Limites connues

L'inventaire complet, avec les preuves, est tenu dans
**[docs/INVENTAIRE-FONCTIONNALITES.md](docs/INVENTAIRE-FONCTIONNALITES.md)**.

- le **retard du périphérique** n'est pas toujours annoncé par le greffon ALSA (0 ms ici
  avec PipeWire) : la synchronisation image/son est alors corrigée du tampon, mais pas du
  retard de la carte elle-même (sur un casque Bluetooth, cela peut représenter ~200 ms) ;
- une source réseau **muette** (connexion acceptée, aucune réponse) fige l'interface au
  plus 3 s par lecture (10 s à l'ouverture) : le démultiplexeur travaille dans le fil
  d'interface, faute d'être déporté dans un thread ;
- si la **piste audio se termine avant la vidéo**, la fin du film repasse au temps mural
  (l'horloge audio est abandonnée après 250 ms de silence) ;
- après un **saut de position**, un sous-titre image (PGS/DVD) déjà commencé n'apparaît
  qu'au prochain événement : le format décrit tout par « compositions », et celle qui a
  établi le sous-titre en cours est antérieure au point de reprise ;
- en ASS, les **dessins vectoriels** `\p` et les rotations 3D `\frx`/`\fry` ne sont pas
  rendus, et un `\clip` vectoriel non rectangulaire est réduit à ses bornes (balises
  ignorées proprement, texte toujours lisible) ;
- **HDR** : la conversion tonale vers SDR est faite (voir plus haut), mais pas le *passthrough* —
  la fenêtre est rendue en 8 bits SDR (`docs/HDR.md`). Sans Vulkan, le repli CPU est borné
  (≤ 1080p30) ; au-delà l'image n'est pas convertie, avec un avis à l'écran ;
- **volume maximal** plafonné à 300 % (au-delà, `cpal` écrête) ;
- **interpolation de mouvement** : non implémentée ;
- **sortie audio** : pas de reprise propre si le périphérique disparaît en cours de lecture
  (PipeWire reroute le flux : rien d'observable ici, non mesuré pour une enceinte Bluetooth qui
  s'éteint puis se rallume) ; pas de piste audio **externe** (`.mka`, `.ac3` à côté du film) ;
- **encodage des sous-titres** : l'encodage imposé ne s'applique qu'aux fichiers **externes** ;
- **plein écran sur plusieurs moniteurs** : les autres écrans noircis ne sont pas gérés (non
  mesurable sous Wayland) ; pas de langue d'interface (le français seul) ;
- la liste complète du reste à faire est dans `docs/PROCHAINES-FONCTIONS.md`.

Ce qui **est** implémenté et mesuré (filtres d'image, égaliseur 18 bandes, traitements
audio, pistes embarquées, ASS, profils) est listé dans
[docs/INVENTAIRE-FONCTIONNALITES.md](docs/INVENTAIRE-FONCTIONNALITES.md) — chaque ligne
renvoie au banc d'essai qui la vérifie.

---

## Outils (`tools/`)

| Script | Usage |
|---|---|
| `bench-hwdec.sh <fichier> [durée_s] [écran] [materiel\|logiciel\|both]` | Compare la charge CPU des deux modes (somme des threads, son désactivé) |
| `verif-ass.sh [écran]` | **Rendu ASS** : position/couleur/alignement/**animations**/karaoké mesurés à l'écran — 17 contrôles |
| `verif-ass-clip.sh [écran]` | **Découpe et rotation ASS** : `\clip` arrêté à la frontière mesurée, `\iclip` (bande centrale vide), `\frz` 90° (boîte inversée), `\org` — 11 contrôles |
| `verif-profils.sh [écran]` | **Profils** : application réelle, persistance, robustesse (fichier corrompu, doublons) — 17 contrôles |
| `verif-regles.sh [écran]` | **Profils par règle** : chemin / extension / résolution mesurés sur l'image et le son, ordre, règles orphelines, ajout et suppression par l'interface — 24 contrôles |
| `etat-court.sh [--check]` | **Régénère la fiche de reprise** `docs/ETAT-COURT.md` : lignes de code, bancs, dernier résultat de la suite, tests, avertissements, archive, capacités du FFmpeg installé |
| `tout-verifier.sh` | Démarre l'écran virtuel si besoin, prend un **verrou** (une seule suite à la fois), enchaîne **tous** les bancs et imprime le total (le dernier résultat est dans `docs/DERNIERE-SUITE.txt`, ~50 min). Pose d'abord l'environnement des bancs (`env-bancs.sh` : Python avec numpy/PIL, session X11, écran virtuel 1500×900) |
| `verif-cadrage.sh [écran]` | **Cadrage et sous-titres** : recadrage, détection des bandes, chapitres, couleur/position/taille/délai du texte, rapport d'aspect — 10 contrôles |
| `verif-pistes.sh [écran]` | **Pistes embarquées** : inventaire, changement de piste audio mesuré en fréquence, sous-titres (texte, minutages, affichage) — 16 contrôles |
| `capture-lecteur.sh <fichier> <image.png> [attente_s] [touches…]` | Lecture sur un écran X virtuel et capture PNG (validation visuelle) |
| `diag-touches.sh` | Diagnostic des touches sous Xvfb (focus, fenêtre cible) |
| `audit-prefs.sh [écran]` | Fenêtre « Préférences » : bornage à l'écran, dépliage/repliage des sections, défilement (10 contrôles, géométrie journalisée) |
| `verif-audio.sh` | **Vérifie les traitements audio par mesure** : 34 contrôles (égaliseur, hauteur, normaliseur, mode nuit, bruit, stéréo, réverbération, décalage), sans carte son |
| `verif-personnalisation.sh [écran]` | **Personnalisation** : clic droit, raccourcis réaffectés (et ancienne touche libérée), repère de recadrage mesuré au pixel, miroir horizontal mesuré — 19 contrôles |
| `audit-menus.sh [écran]` | **Menus** : les six menus un par un — présence des entrées, lisibilité, et **effet réel** de 19 d'entre elles (31 contrôles) ; clique à partir de la géométrie journalisée, sans deviner les pixels |
| `capture-menus.sh` | Capture chaque menu déroulant (inspection visuelle de l'ordre et de la lisibilité) |
| `verif-sousimages.sh [écran]` | **Sous-titres image (PGS/DVD)** : géométrie, pixels opaques, déterminisme, taille et couleur à l'écran, extinction, non-régression du texte — 10 contrôles ; `fabrique-pgs.py` génère le flux de test |
| `fabrique-pgs.py <sortie.sup> [--largeur … --x … --couleur …]` | Écrit un flux PGS minimal valide (aucun encodeur PGS n'existe dans FFmpeg) |
| `bench-optimisation.sh [écran]` | **Chiffre les optimisations** : échelle GPU contre CPU (−42 %), désentrelacement GPU contre `bwdif` (−38 %), géométrie inchangée, AV1 logiciel (dav1d) — 4 contrôles |
| `verif-soustitres.sh [écran]` | **Style des sous-titres** : trois familles de police mesurées (largeurs 268/307/332 px), position horizontale 20/50/80 %, police inconnue, non-régression — 5 contrôles |
| `installer-association.sh [--par-defaut] [--etat] [--retirer]` | **« Ouvrir avec » RMPlayer** : lanceur `.desktop`, icône, cache des types MIME ; `--par-defaut` l'impose pour les formats qu'il gère |
| — | **revue externe** : `docs/BRIEF-REVUE-EXTERNE.md` — ce qu'on demande à un relecteur extérieur, budget de lecture, six vérifications qui valent mieux que la lecture, deux pièges à faux positifs |
| `verif-capture.sh [écran]` | **Captures** : format écrit (signature et décodage du fichier), qualité JPEG qui agit (taille et écart de pixels), WebP identique au PNG, taille native, modèle de nom, ancienne configuration — 9 contrôles |
| `verif-hdr-saut.sh [écran]` | **HDR après un saut** : la fenêtre est filmée pendant des sauts (en lecture, en pause, cinq rapprochés) et pendant une répétition A-B ; chaque image est classée convertie / non convertie — 5 contrôles |
| `verif-hdr.sh` | **HDR** (sans écran) : détection PQ/HLG/BT.2020, signature de conversion en nits, deux opérateurs, moteur CPU forcé, tramage, débanding GPU, décodage VA-API identique au logiciel, **détection des écrans par EDID** et branchement de la décision, **Vulkan coupé** (repli CPU borné, avis, pas de reconstruction à chaque image) — 36 contrôles |
| `verif-focus.sh [écran]` | **Pause selon le focus** : jamais / perte de focus / fenêtre réduite, reprise au retour, respect d'une pause volontaire — 11 contrôles, avec une seconde fenêtre réelle ; la pause est mesurée aussi sur l'image (deux captures identiques), avec un témoin en lecture. Non couvert : une vraie réduction (mode 2), qui exige un gestionnaire de fenêtres |
| `verif-recents.sh [écran]` | **Fichiers récents** : `config.json` relu après de vraies ouvertures (ordre, doublon, borne de 10) — 7 contrôles |
| `verif-config-io.sh` | **Export / import de la configuration** : aller-retour octet pour octet, refus sans écrasement — 12 contrôles |
| `verif-boucle-ab.sh [écran]` | **Répétition A-B** : nombre de retours et pause, comptés sur la position MPRIS et au journal — 6 contrôles |
| `verif-soustitres-style.sh [écran]` | **Style des sous-titres** : contour, marges haut/bas, interligne, ombre, second contour, échelle X/Y, fondu — pixels de la fenêtre — 9 contrôles |
| `verif-soustitres-langues.sh [écran]` | **Langues par priorité, durées mini/maxi, piste mémorisée, alphabet** — 12 contrôles |
| `verif-couleur.sh` | **Matrice et plage de couleur** : couleur RVB d'origine retrouvée (BT.601/709, limitée/complète) — 11 contrôles |
| `verif-affichage.sh [écran]` | **Rogner et glisser** (réglages **et raccourcis clavier**, cycle complet de `K`), et couleur identique par la chaîne GPU et la chaîne CPU — 16 contrôles |
| `verif-signets.sh [écran]` | **Signets** par média : ajouter, suivant/précédent, supprimer, **renommer** (saisie réelle), après redémarrage ; **les touches tapées dans un champ de texte ne déclenchent aucun raccourci** — 13 contrôles |
| `verif-souris.sh [écran]` | **Souris avec Ctrl / Maj / Alt** : effets lus sur MPRIS — 4 contrôles |
| `verif-decodeur.sh` | **Décodeur par codec** : décodeur réellement ouvert, repli sur le défaut — 7 contrôles |
| `verif-systeme.sh [écran]` | **Démarrage automatique, priorité (nice de tous les fils), fréquence maximale** — 10 contrôles |
| `verif-tray.sh [écran]` | **Icône de notification** (StatusNotifierItem) pilotée par D-Bus — 7 contrôles |
| `verif-theme.sh [écran]` | **Thèmes externes** : pixel de la barre de menus, thème illisible refusé — 6 contrôles |
| `verif-saut-pause.sh [écran]` | **Saut en pause** : l'image (teinte qui dit l'instant) et la réplique intégrée suivent un saut, le pas à pas change l'image — 5 contrôles |
| `verif-migration.sh` | **Renommage KPlayer → RMPlayer** : un vrai lancement reprend les réglages, les positions de reprise et les thèmes de l'ancien dossier (copié, jamais déplacé), sans écraser le nouveau — 7 contrôles |
| `verif-fenetres-detachables.sh [écran]` | **Fenêtres détachables** : Préférences, Statistiques, Aide, À propos deviennent de vraies fenêtres système (fenêtre distincte, déplacée hors du lecteur, contenu visible, rattachement, Échap, mémoire du choix, ancienne configuration) — 24 contrôles |
| `verif-peripheriques.sh [écran]` | **Sortie audio** : deux sorties PipeWire factices ; le 440 Hz arrive sur la sortie choisie (enregistrement de son moniteur), changement en cours de lecture (raccourci et clic de menu), retour au défaut, ancien réglage ALSA, sortie disparue — 17 contrôles |
| `verif-audio-pistes.sh [écran]` | **Pistes audio** : langues par priorité, commentaires écartés, piste par défaut, source mono, mémoire par film — mesuré au son — 11 contrôles |
| `verif-mpris.sh` | **MPRIS** (module seul, mode `--mpris-test`, sans fenêtre) : nom sur le bus, métadonnées en µs, commandes reçues, `SetPosition` filtré par piste, réglages Volume/Shuffle/LoopStatus/Rate, `PropertiesChanged`, nom rendu, absence de bus gérée — 27 contrôles |
| `verif-vignettes.sh` | **Vignettes de la barre** (module seul, chemin `--vignette`) : taille, position réelle à une image près, deux positions → deux images, déterminisme, largeur bornée, coût, sortie réutilisée, largeur non multiple de 32, hors durée — 14 contrôles, sans écran |
| `verif-playlist.sh [écran]` | **Playlist** : lecture aléatoire (quatre avances → quatre fichiers distincts, mode mémorisé), **glisser-déposer réel à la souris**, clic sur une ligne, ✕ qui retire, volet borné avec des noms longs — 28 contrôles ; les lignes sont visées depuis la géométrie journalisée, jamais devinée |
| `verif-rapide.sh <sujet…>` | **Vérification ciblée** : ne rejoue que les bancs concernés par une modification (`verif-rapide.sh --liste` donne les sujets : `ui`, `audio`, `peripheriques`, `fenetres`, `association`, `mpris`…) ; un nom de banc est aussi accepté — de quelques minutes au lieu de ~50 |
| `pw-flux.py <sink\|etat\|defaut> [pid]` | Où va le flux audio d'un processus dans PipeWire (`pw-dump`) : l'instrument de `verif-peripheriques.sh` |
| `verif-association.sh [écran]` | **Association et instance unique** : la ligne `Exec` est réellement exécutée (le média se charge), types MIME vérifiés contre le système, seconde ouverture absorbée par la première instance, socket orphelin repris puis balayé, désinstallation — 38 contrôles, `~/.local` de l'utilisateur intact |
| `verif-fenetre-wayland.sh` | **Mémoire de la fenêtre sous Wayland** : taille enregistrée, aucune position inventée, règle KWin appliquée puis retirée, redimensionnement suivi — mesure par script KWin chargé en D-Bus, fenêtre ciblée par PID, `kwinrulesrc` restauré — 12 contrôles (ignoré si aucune session Wayland) |
| `regle-fenetre-kwin.sh [--position x,y] [--depuis-fenetre] [--etat] [--retirer]` | **Règle de fenêtre KWin** : la position de RMPlayer sous Wayland (le client ne peut pas se placer lui-même) |
| `verif-fenetre.sh [écran]` | **Mémoire de la fenêtre** : déplacement et redimensionnement par `xdotool`, configuration relue, puis relance et mesure de la géométrie réelle — taille, position et état agrandi restitués, écriture temporisée (aucune réécriture sans changement), géométrie hors écran recentrée, plein écran sans effet sur la géométrie — 9 contrôles |
| `verif-precache.sh [écran]` | **Pré-cache des flux** : un serveur HLS local journalise chaque requête — avance des segments, coupure de 3 s absorbée (blocage 0,46 s contre 4,34 s), reprise en continuité — 9 contrôles |
| `verif-reseau.sh [écran]` | **Reconnexion d'un flux interrompu** : sert un HLS local et simule les pannes (503, serveur tué, connexion fermée, serveur muet), vérifie la reprise à la position, la synchronisation, l'abandon après 8 essais et qu'une vraie fin n'est pas prise pour une coupure — 21 contrôles |
| `serveur-hls.py <dossier> <port> <sentinelles>` | Serveur HTTP de test à pannes pilotées par fichiers (`panne`, `refus`, `gel`) |
| `verif-horloge.sh` | **La cadence suit-elle la carte son ?** 19 contrôles : écart image/son borné à une image, tampon maintenu, tampon approfondi sans effet, seek, 2×, pause, replis (média muet, audio terminé) — crée son propre puits audio et restaure la sortie de l'utilisateur |
| `analyse-audio.py <commande> <wav>` | Mesures élémentaires (niveaux, crête, S/B, canaux, fréquence, queue, début) |
| `bench-desentrelacement.sh` | **Désentrelaceurs comparés par la mesure** : fabrique un contenu entrelacé dont l'original est connu, puis compare `yadif`, `bwdif` (et `estdif`) en PSNR, détail fin conservé et coût par image 1080p |
| `verif-filtres.sh` | **Vérifie chaque filtre d'image par une mesure** (détramage, débruitage, désentrelacement `bwdif`/`yadif`, luminosité, saturation, netteté, rotation, zoom) : 16 contrôles chiffrés, sans écran |
| `metrics-image.py <banding\|bruit\|peigne\|luma\|couleur\|diff> <image.png> [ref]` | Métriques utilisées par `verif-filtres.sh` |
| `audit-ui.sh <fichier1> [fichier2] [écran]` | **Audit de l'interface** : boutons, raccourcis, fenêtres de réglages, vitalité de la boucle d'événements, fermeture par `Q` (54 contrôles) |
| `check-ctx-reentrance.py [src/]` | Contrôle **statique** : détecte les `ctx.*` appelés depuis une fermeture de saisie egui — interblocage garanti de la fenêtre |
| `vivacite.sh <journal> [écran] [délai_s]` | Détecteur de gel : prouve que la boucle d'événements traite encore des messages |
| `test-vivacite.sh [écran]` | Vérifie que le détecteur de gel **détecte** vraiment (gel volontaire par `SIGSTOP`, puis reprise) |
| `diag-gel.sh <touche> <fichier> [écran] [s]` | Reproduit un gel et en donne la **pile complète** : lance sous `gdb` (obligatoire avec `ptrace_scope=1`), envoie la touche, fige par `SIGSTOP`, puis signale un interblocage de verrou egui et la ligne `src/ui.rs` fautive |

---

## Organisation du code

```
src/
├── main.rs          Point d'entrée : journal, migration des données, rapport de crash, ligne de commande, eframe
├── player.rs        Moteur FFmpeg : démux, décodage, cadence, seek, playlist, pistes, filtres
├── ui.rs            Interface egui : menus, contrôles, OSD, playlist, sous-titres, plein écran
├── settings.rs      Fenêtre Préférences (sections dépliables)
├── fenetre.rs       Fenêtres flottantes détachables (vraies fenêtres système)
├── raccourcis.rs    Table unique des raccourcis clavier et souris, personnalisable
├── audio.rs         Sortie cpal : tampon circulaire, callback temps réel, horloge audio
├── sortie_audio.rs  Sorties PipeWire : liste lisible et redirection du flux
├── subtitles.rs     Sous-titres externes : SRT / ASS / SSA, encodage
├── ass.rs           Analyse ASS/SSA : styles, balises, animations
├── config.rs        Configuration JSON persistante, profils, export / import
├── hdr.rs           HDR : détection, conversion tonale, tramage
├── edid.rs          Capacités annoncées par l'écran (EDID)
├── mpris.rs         MPRIS : touches média et widget du bureau
├── instance.rs      Instance unique (« Ouvrir avec » arrive dans la fenêtre ouverte)
├── tray.rs          Icône de zone de notification (StatusNotifierItem)
├── systeme.rs       Démarrage automatique et priorité du processus
├── theme.rs         Thèmes externes JSON
├── signets.rs       Signets par média
├── positions.rs     Positions de reprise
├── pistes.rs        Mémoire des pistes choisies à la main, par média
├── vignettes.rs     Vignettes de la barre de progression
├── migration.rs     Reprise des données de l'ancien nom (KPlayer)
├── crash.rs         Rapport de crash sûr (handler de signal async-signal-safe)
├── renderer.rs      Transport des images RGBA vers egui
├── x11.rs           Fenêtre toujours au-dessus (X11)
├── utils.rs         Utilitaires (formatage, types MIME)
```

---

## Versions et sauvegardes

Le projet est sous **git** (branche `main` ; une branche `fonction/<nom>` par fonction, fusionnée
avec `--no-ff` ; étiquettes `session36`, `session37`…) et publié sur
<https://github.com/77Darius77/RMPlayer>. Des archives horodatées complètent le dépôt dans
`sauvegardes/` (non suivi par git) :

```bash
ls -la sauvegardes/
tar xzf sauvegardes/rmplayer-AAAA-MM-JJ.tar.gz -C /chemin/de/restauration
```

(les archives antérieures au renommage s'appellent `kplayer-AAAA-MM-JJ-…`).

## Licence

[MIT](LICENSE)

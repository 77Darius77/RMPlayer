# KPlayer face à PotPlayer — paramètres par catégorie

**Méthode.** Côté **KPlayer** : le code (`src/config.rs`, `src/settings.rs`,
`src/raccourcis.rs`, `src/main.rs`), donc exact.

Côté **PotPlayer** : d'abord la documentation publique, puis — **depuis, et c'est ce qui fait
foi** — l'**installation elle-même** : arborescence de `F5` relevée dans
`Language/French.ini` de la version **v260819**, libellés page par page, digests des fichiers.
Voir [docs/POTPLAYER-REGLAGES-VERIFIES.md](POTPLAYER-REGLAGES-VERIFIES.md).

Trois corrections que cette vérification a imposées au premier jet de ce document :

1. **« Lecture » n'est pas une catégorie** : c'est une sous-page de **Général**.
2. **Il n'y a pas de catégorie « Capture »** : c'est **Général › Capture d'écran**.
3. Le bloc de filtres s'appelle **« Contrôle du filtre »** et ne compte que **quatre** pages
   (Source/Séparateur, Décodeurs vidéo, Décodeurs audio, Priorité globale) ; le
   désentrelacement, la colorimétrie et les effets sont sous **Vidéo**, l'égaliseur et le
   normaliseur sous **Audio**.

Les noms « General / Playback / Device / Filter Control » utilisés d'après la documentation
anglaise sont donc à lire comme des équivalents : la version française installée dit
**Général**, **Périphérique**, **Contrôle du filtre**, **Vidéo**, **Audio**, **Sous-titres**,
**Kakao TV**.

**Légende** : ✅ repris · 🟡 partiel ou autrement fait · ❌ absent · ➖ sans objet sous Linux.

> **Listing complet, paramètre par paramètre, classé par catégorie** :
> [docs/POTPLAYER-VERS-KPLAYER.md](POTPLAYER-VERS-KPLAYER.md) — c'est le document de
> travail de l'optimisation (état de chaque réglage, façon de le faire en Rust/Linux,
> effort et effet). Celui-ci reste la vue d'ensemble.

---

## 1. Général

| PotPlayer | KPlayer | |
|---|---|---|
| Démarrage : reprendre la dernière lecture, ouvrir une action | Reprise de position par média (`playback.resume`, `~/.cache/kplayer/positions.json`), playlist restaurée | 🟡 fichiers, pas la session entière (pas de « rouvrir le dernier film ») |
| Langue de l'interface | Interface en français uniquement | ❌ (aucun mécanisme de traduction) |
| Raccourcis clavier (*Key Assignments*) | 41 actions, clavier **et** souris (molette, clic du milieu, double-clic), écran dédié avec capture directe et détection des conflits (`raccourcis.rs`) | ✅ |
| Fermeture : réduire dans la zone de notification / quitter | Fermeture normale (touche `Q`, ✕) | ❌ pas de tray |
| Association de fichiers (double-clic ouvre PotPlayer) | Lanceur installé par `tools/installer-association.sh` (16 types MIME, icône, « Ouvrir avec »), instance unique : le fichier arrive dans la fenêtre ouverte (s.33) | ✅ |
| Skins / thèmes | Deux thèmes (sombre/clair), rien d'autre | 🟡 |
| Mise à jour automatique | — | ➖ |
| Export/import de **toute** la configuration (`.reg`) | Export/import **par profil** (JSON) ; `config.json` éditable à la main | 🟡 |
| Vignettes de prévisualisation au survol de la barre | — | ❌ |
| Mémorisation de la taille et de la position de la fenêtre | Taille, position et état agrandi relus au lancement ; hors écran → recentrée (s.32) ; position non imposable sous Wayland | ✅ (X11) / 🟡 (Wayland) |

---

## 2. Général › Lecture

| PotPlayer | KPlayer | |
|---|---|---|
| Lecture / arrêt / pause, image par image | ✅ (`Espace`, `,` et `.`) | ✅ |
| Vitesse 0,1×–12× (audio inclus) | 0,1×–8× avec audio, hauteur conservée (`atempo`) | ✅ (borne plus basse, **mesurée**) |
| Reprise automatique (*Auto-resume*) | `playback.resume`, positions toutes les 10 s + à la fermeture | ✅ |
| Priorité du processus (temps réel, haute…) | — | ❌ |
| Traitement (*Processing*) : décodage en arrière-plan, threads | — | ❌ |
| Mémoire tampon réseau / pré-cache | `playback.precache_ms` (défaut 6 s, 0–30 s), capacité du tampon audio réglable | ✅ (s.30, mesuré) |
| Répétition A-B, boucle, lecture aléatoire | A-B (`A`/`B`), enchaînement de playlist | 🟡 pas de lecture aléatoire |
| Chapitres | `Page↑`/`Page↓`, menu *Lecture* | ✅ |
| Enchaînement / file de lecture | Playlist persistante, panneau `L` | 🟡 pas de glisser-déposer |
| Saut de publicité / signets | — | ❌ |

---

## 3. Périphérique

| PotPlayer | KPlayer | |
|---|---|---|
| Lecteur DVD / Blu-ray / TV / capture | Ouverture d'un `BDMV`, d'un disque monté ou d'une URL (http, HLS, RTSP, RTMP, SMB, FTP) par FFmpeg | 🟡 disques et flux oui, pas de syntoniseur TV ni de capture |
| Télécommande, périphériques HID | — | ➖ |
| Périphériques audio/vidéo d'entrée | — | ➖ |

---

## 4. Vidéo (Video)

### 4.1 Décodage et rendu

| PotPlayer | KPlayer | |
|---|---|---|
| Décodeur matériel : DXVA / CUDA / QuickSync | **VA-API** avec repli logiciel automatique, indicateur de mode fiable (vérifié par frame), bascule `H` / `--no-hwdec` | ✅ (équivalent Linux) |
| **Rendu vidéo** (`Dialog(355)`) : Rendu vidéo, Présentation/Périph. Direct3D, Format de surface, Surface VMR, Tampon, Plein écran exclusif, **Sortie 10-bits**, Palette limitée 16~235, **Mode HDR matériel**, **D3D11 GPU Super Résolution**, **RTX Vidéo HDR**, Vsync | Rendu via egui/OpenGL, pas de choix | 🟡 un seul chemin ; pas de 10 bits, pas de HDR, pas de Vsync réglable (chantier `gpu-next`) |
| Sortie 10 bits / espace colorimétrique (RGB 0-255 / 16-235), dithering | — | ❌ |
| Copie arrière (copy-back) et zéro-copie | VA-API avec rapatriement des surfaces ; **la réduction et le désentrelacement se font désormais dans le GPU** avant la copie (**−42 %** et **−38 %** mesurés, `bench-optimisation.sh`) | 🟡 amélioré : il reste la copie et le téléversement de la texture |

### 4.2 Traitement de l'image

| PotPlayer | KPlayer | |
|---|---|---|
| **Vidéo › Colorimétrie** (`Dialog(384)`) : Luminosité, Contraste, Saturation, **Teinte**, Table YCbCr↔RGB, Palette YCbCr, Colorimétrie de sortie, Conversion directe | `eq` : luminosité, contraste, saturation, gamma | 🟡 **pas de teinte**, pas de table de colorimétrie, pas de conversion directe |
| Netteté (*sharpen*) | `unsharp`, 3 niveaux bornés (mesuré) | ✅ |
| Flou (*blur*) | — | ❌ |
| Rotation, miroir | `rotate` 0/90/180/270, miroirs vertical **et** horizontal | ✅ |
| Déformation (*warp*, perspective) | — | ❌ |
| **Vidéo › Effets** (`Dialog(386)`) : Netteté, Débruiter 3D, Débruitage graduel, **WarpSharp**, **Mouvement flou**, Débander (seuil + rayon), Adoucir, retournements, 270° | `hqdn3d` (3 niveaux calibrés), `unsharp`, `deband` (seuil + rayon), rotation, miroirs H+V | ✅ sur l'essentiel ; ❌ **WarpSharp**, **mouvement flou**, **Adoucir** |
| Détramage (*debanding*) | `deband` + rayon et seuil réglables (section *Avancé*) | ✅ |
| Désentrelacement | `bwdif` (défaut) ou `yadif`, parité réglable — **choisi par mesure** ; **désormais aussi `deinterlace_vaapi` dans le GPU** quand `video.gpu_scaling` est actif (**−38 %** mesuré) | ✅ |
| Post-traitement / filtres externes (avisynth, ffdshow) | Chaîne `libavfilter` interne, pas de greffon externe | 🟡 |
| Interpolation de mouvement (60 i/s) | — | ❌ (hors périmètre, documenté) |

### 4.3 Espace écran et cadrage

| PotPlayer | KPlayer | |
|---|---|---|
| Format d'image (16:9, 4:3, personnalisé, remplir) | 7 modes dont personnalisé, touche `K` (mesuré) | ✅ |
| Recadrage manuel + détection automatique des bandes noires | Recadrage aux 4 bords, détection automatique (médiane), repère visuel, réglage/validation | ✅ |
| Zoom, déplacement dans l'image | `zoom` (molette Ctrl) | ✅ |
| Redimensionnement de la fenêtre, plein écran | Plein écran `F`/`F11`/bouton, `Tab` masque l'interface | ✅ |

### 4.4 3D, 360°, HDR

| PotPlayer | KPlayer | |
|---|---|---|
| 3D stéréoscopique, 360° | — | ❌ |
| HDR / tone mapping / RTX Video HDR | — (mesuré comme dégradant ici : 43/1023 au lieu de 504) | ❌ assume, à rouvrir avec `gpu-next` |

---

## 5. Audio

### 5.1 Sortie

| PotPlayer | KPlayer | |
|---|---|---|
| Choix du périphérique de sortie (dont deux cartes son) | Liste cpal, appliquée (rechargement automatique) | ✅ |
| *Renderer* audio (WASAPI, DirectSound, exclusif) | `cpal` (PipeWire/ALSA) | ➖ |
| Volume, muet, volume maximal | Borné par `volume_max` (100–300 %), `M` | ✅ |
| Configuration des enceintes, downmix | Ce que le périphérique expose (2 canaux typiquement) | 🟡 |

### 5.2 Traitements

| PotPlayer | KPlayer | |
|---|---|---|
| **Audio › Egaliseur** (`Dialog(363)`) : `SuperEQ`, bandes 60/170/310/600/1K/3K/6K/12K/14K/16K, Préréglage, Activer | `superequalizer` **18 bandes** + presets (plat, graves, aigus, voix, loudness) | ✅ (plus de bandes que lui) ; ❌ **SuperEQ** (courbe libre) |
| Égaliseur paramétrique à courbe libre | — | ❌ (`firequalizer`, second graphe) |
| Normalisation du volume (*loudness*) | `dynaudnorm` (fenêtre 200 ms, mesure à l'appui) | ✅ |
| Mode nuit / compression dynamique | `acompressor` (mode nuit) | ✅ |
| Suppression du bruit | `afftdn` 3 niveaux | ✅ |
| Spatialisation, élargissement stéréo | `stereowiden`, `crossfeed` | ✅ |
| **Audio › Normaliseur/Freeverb** (`Dialog(387)`) : Normaliseur (contrôle auto du volume), **Renforcer l'audio**, Freeverb (salle, étouffement, Wet/Dry, largeur), « fonctionne sur 2 canaux » | `dynaudnorm`, réverbération 3 niveaux | ✅ sur l'essentiel ; ❌ pas de réglage fin de la réverbération (salle/Wet/Dry) |
| Karaoké / suppression de la voix | `karaoke` (annulation centrale) | ✅ |
| Décalage audio/vidéo | `delay_ms` ±2000 ms | ✅ |
| Hauteur préservée à vitesse ≠ 1 | `atempo` | ✅ |
| Enregistrement de la sortie audio | — | ❌ |

---

## 6. Sous-titres (Subtitles)

| PotPlayer | KPlayer | |
|---|---|---|
| Chargement automatique du fichier homonyme, sous-dossiers | ✅ + langue préférée (`.fr.` d'abord) | ✅ |
| Choix de la piste du conteneur | Liste des pistes, bascule à chaud | ✅ |
| **Sous-titres › Style police** (`Dialog(383)`) : Police, Taille, **Alphabet**, Interlettre, Texte/Contour/Ombre/2nd contour, **Contour flou** et **gaussien**, Échelle X/Y, Transparence, Boîte opaque, marges L/R/haut/bas, Interligne, **Position hori. et verti. (%)**, Alignement, **Ruby** | **Police** (résolue par fontconfig), taille, gras, italique, couleur, contour (couleur/épaisseur), bandeau, **position verticale ET horizontale**, alignement | ✅ depuis la s.31 ; restent ❌ alphabet, interlettre, second contour, contour flou, Ruby |
| Taille, gras, italique | `font_size`, `bold`, `italic` | ✅ |
| Couleur du texte, couleur et épaisseur du contour | `color`, `outline_color`, `outline_width` | ✅ |
| Bandeau de fond et son opacité | `background_alpha` 0–255 | ✅ |
| Position verticale, alignement | `position_percent`, `align` (gauche/centre/droite) | 🟡 vertical seulement, pas de position horizontale libre |
| Synchronisation / décalage | `delay_ms` ±10 s | ✅ |
| Rendu **ASS/SSA** du fichier (styles, `\pos`, `\move`, `\t`, karaoké, `\clip`, `\frz`) | Rendu par événement, mesuré | ✅ très large ; restent `\p`, `\fscx/\fscy`, `\blur`, `\frx/\fry`, `\t` par fragment |
| Sous-titres **image** (PGS/Blu-ray, DVD, DVB) | Décodés en RGBA et composés (position, échelle, extinction) | ✅ |
| Sous-titres texte **dans le flux** (CEA-608/708, *closed captions*) | — | ❌ |
| Police personnalisée par style ASS | Styles du fichier respectés (taille, couleurs, contour) | ✅ |
| Traduction en temps réel, dictionnaire, parole → texte | — | ❌ |
| Plusieurs sous-titres simultanés (deux langues) | — | ❌ |

---

## 7. Contrôle du filtre

| PotPlayer | KPlayer | |
|---|---|---|
| Priorité des filtres, filtres externes enregistrés, gestionnaire | Chaîne `libavfilter` construite par le moteur (`filter_description`), affichée et journalisée | 🟡 interne uniquement, pas de greffon |
| Décodeur vidéo / audio par codec, DXVA intégré | Choix **global** matériel/logiciel (VA-API ou logiciel), pas par codec | 🟡 |
| *Source/Splitter* par format | Démultiplexeur FFmpeg unique | ➖ |
| OpenCodec / codecs additionnels | Bibliothèques FFmpeg natives | ➖ |
| Désentrelacement, détramage, débruitage dans les filtres | Section *Image — filtres* des préférences (paramètres « Avancé ») | ✅ (mieux placé) |

---

## 8. Général › Capture d'écran, et outils

| PotPlayer | KPlayer | |
|---|---|---|
| Capture d'image (format, dossier, raccourci) | PNG (`S`), dossier réglable | ✅ |
| Capture d'une séquence, GIF, capture audio | — | ❌ |
| Enregistrement du flux, transcodage | — | ❌ (chantier « transcodage/extraction ») |
| Extraction d'extraits, remux | — | ❌ (même chantier) |
| Diffusion (streaming) | Lecture de flux uniquement | ➖ |
| Messages / notifications de l'application | OSD, bandeaux d'état (annonces, reconnexion, erreurs) | 🟡 |

---

## Ce que KPlayer a et que PotPlayer n'a pas (ou autrement)

- **Profils appliqués par règle** : chemin, extension, résolution — automatique à l'ouverture,
  avec ordre explicite et règles orphelines ignorées (`verif-regles.sh` 24/24). PotPlayer
  demande de changer de profil à la main.
- **Choix du désentrelaceur mesuré** (`bwdif` contre `yadif`, +2,4 dB sur les scènes fixes).
- **Pré-cache réseau réglable et mesuré** (coupure de 3 s absorbée).
- **Raccourcis souris** de première classe (molette, clic du milieu, double-clic), dans la
  même table que le clavier, aide `F1` construite depuis la table.
- **Repli matériel → logiciel** explicite et observable (indicateur de mode par frame).

---

## Synthèse

| Catégorie | Couverture |
|---|---|
| Général | 🟡 l'essentiel (raccourcis), mais pas d'association de fichiers, ni vignettes, ni tray, ni langue, ni mémorisation de fenêtre |
| Lecture | ✅ très proche ; manquent priorité du processus et lecture aléatoire |
| Périphérique | 🟡 disques et flux oui ; TV/capture sans objet |
| Vidéo | ✅ filtres et cadrage ; ❌ rendu GPU, 10 bits, flou, warp, interpolation, HDR, 3D/360 |
| Audio | ✅ quasi complet côté traitements ; ❌ égaliseur à courbe libre, capture |
| Sous-titres | ✅ excellent sur ASS et image ; ❌ choix de police, CEA-608, traduction |
| Filtres | 🟡 interne seulement (pas de greffons) — cohérent avec Linux/FFmpeg |
| Capture/outils | 🟡 image seulement |

**Les cinq manques qui se ressentent le plus, à coût croissant** :

1. **Choix de la police des sous-titres** (et position horizontale) — petit, très visible.
2. **Vignettes de prévisualisation** au survol de la barre — moyen, gros confort de navigation.
3. **Association de fichiers + mémorisation de la taille de fenêtre** — petit, attendu d'un lecteur.
4. **Lecture aléatoire** et **glisser-déposer** dans la playlist — petit (le second est déjà dans la liste).
5. **Rendu GPU (`gpu-next`)** — lourd côté outillage, ouvrirait aussi HDR et 10 bits.

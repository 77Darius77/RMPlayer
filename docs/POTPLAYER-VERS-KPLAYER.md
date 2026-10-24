# PotPlayer → KPlayer : tous les paramétrages, classés par catégorie

**But.** Relever ce que PotPlayer propose, catégorie par catégorie, et dire pour chaque
paramètre : ce que KPlayer fait **aujourd'hui**, ce qui manque, et **comment le faire**
dans un lecteur Rust/Linux. C'est le document de travail de l'optimisation — pas une
comparaison de principe.

**Sources.** PotPlayer : `Language/French.ini` de l'installation réelle (**v260819**), dont
tous les blocs `[Dialog(N)]` sont cités (`Dialog(383)` = page « Style police », etc.) ;
arborescence F5 vérifiée dans [POTPLAYER-REGLAGES-VERIFIES.md](POTPLAYER-REGLAGES-VERIFIES.md).
KPlayer : le code (`src/config.rs`, `src/settings.rs`, `src/player.rs`), les bancs de
`tools/`, et les mesures de [OPTIMISATION-COMPARATIF.md](OPTIMISATION-COMPARATIF.md).

**Légende** : ✅ fait et vérifié · 🟡 partiel ou autrement fait · ❌ absent · ➖ sans objet
sous Linux (DirectShow, DXVA, TV, Kakao).

---

## 0. Ce qui a été appliqué dans cette session (et son effet mesuré)

Trois optimisations issues du comparatif chiffré. Toutes mesurées, toutes réversibles.

| Optimisation | Réglage | Gain mesuré | Vérification |
|---|---|---|---|
| **Réduction d'échelle dans le GPU** (`scale_vaapi`, surfaces VA-API conservées jusqu'au rapatriement) | `video.gpu_scaling` (défaut **oui**) | 1080p→720p : **10,85 → 6,3 ms/image (−42 %)** ; 1080p→540p : 11,55 → 5,06 (−56 %) | `tools/bench-optimisation.sh` ① |
| **Désentrelacement matériel** (`deinterlace_vaapi`) | `video.gpu_scaling` + `video.deinterlace` | 1080i→720p : **11,05 → 6,8 ms/image (−38 %)** | `tools/bench-optimisation.sh` ② |
| **AV1 logiciel** (`libdav1d` compilé dans FFmpeg) | aucune : automatique | AV1 **enfin décodable** en logiciel (le décodeur natif de FFmpeg 7.1 échoue sur un AV1 réel : « Function not implemented ») ; 179 i/s en 720p | `tools/bench-optimisation.sh` ④ |

Deux points de transparence, tous deux mesurés :

- le filtre d'échelle du pilote est **plus doux** que Lanczos (PSNR **30,7 dB** sur une mire
  à contours nets) : la case à cocher le dit, le journal le dit, et la décocher rend Lanczos ;
- `procamp_vaapi` (luminosité/contraste/saturation) **échoue sur ce pilote**
  (« filter caps: 20 – not implemented ») : les corrections d'image restent donc sur le CPU,
  et un filtre CPU ramène de toute façon les images en mémoire système.

Effet de bord assumé : le chemin GPU réduit puis convertit en NV12→RGBA ; l'ordre des filtres
est inchangé, et un filtre CPU quelconque fait revenir à l'ancien chemin (aucune régression
possible sur l'existant).

---

## 1. Général (`Dialog(354)`, pages 2055-2068, 2095-2096)

| Paramètre PotPlayer (libellé exact) | KPlayer | État | Comment le faire / remarque |
|---|---|---|---|
| **Langue d'interface** (3287) | interface en français uniquement | ❌ | solution Linux : fichiers `.po`/`fluent` + détection `LANG`. Effort moyen, valeur faible pour un usage francophone |
| **Instances multiples** (3293), « Instances multiples non chevauchantes » (3339) | une seule instance | ❌ | verrou `flock` sur un fichier de `$XDG_RUNTIME_DIR`, argument `--new-instance` |
| **Instances multiples** (reçue via IPC) | `--nouveau` / paramètres CLI | 🟡 | KPlayer ne transmet pas de fichier à une instance existante : c'est ce qui manque à l'association de fichiers |
| **Stocker réglages dans fichier .ini** (3083) | `~/.config/kplayer/config.json` | ✅ | JSON, lisible et versionnable |
| **Auto-vérif. mises à jour** (3291) | aucune | ➖ | pas de dépôt de publication : hors sujet |
| **Suspendre la lecture à la perte de focus** (3066) | non | ❌ | `ctx.input(|i| i.viewport().focused)` dans egui → pause. Petit |
| **Suspendre la lecture si écran verrouillé** (3130) | non | ➖ | D-Bus `org.freedesktop.ScreenSaver` ; possible mais peu utile |
| **Suspendre la lecture quand réduit** (3110) | non | ❌ | événement de viewport egui. Petit |
| **Taille fenêtre par défaut à la fermeture fichier** (3123) | non : la fenêtre garde sa taille | ❌ | **mémoriser taille/position** : `ViewportCommand::OuterPosition/InnerSize` + `interface.window_x/y/w/h`. Petit, très attendu |
| **Redimensionner la fenêtre autour du centre** (3126) | non | ❌ | même chantier que ci-dessus |
| **Position fixe de fenêtre si réduite** (3129) | non | ➖ | gestionnaire de fenêtres Linux |
| **S'accrocher au bord/fenêtre** (3073) | non | ➖ | aimantation fournie par KDE/GNOME |
| **Cacher toutes fenêtres quand réduit** (3094) | non | ➖ | idem |
| **Auto-démarrer à l'amorçage** (3116) | non | ❌ | fichier `.desktop` dans `~/.config/autostart`. Petit |
| **En avant-plan** (3286) | ✅ « Toujours au-dessus » (raccourci + menu) | ✅ | `ViewportCommand::WindowLevel` |
| **Icône Tâches/Notif.** (3290) | icône de fenêtre ; pas d'icône de notification | 🟡 | `ksni`/D-Bus StatusNotifier si besoin |
| **Contrôle de transport médias du système** (3337) | non | ❌ | D-Bus **MPRIS** (`org.mpris.MediaPlayer2`) : intégration KDE/GNOME réelle. Effort moyen, valeur forte sur bureau |
| **Récents dans les favoris** (3127) | non (file de lecture seulement) | ❌ | `playback.recent` (liste bornée) + menu |
| **Interactions glisser/déposer avancées** (3340) | non | ❌ | voir ④ plus bas (`dnd_drop_zone` d'egui) |
| **Ignorer messages d'erreur si lecture débute** (3341) | messages d'erreur explicites | 🟡 | choix inverse assumé : KPlayer explique l'échec |
| **Clavier** (2056) | raccourcis **tous** personnalisables, conflits détectés | ✅ **au-delà** | `src/raccourcis.rs`, `verif-raccourcis.sh` |
| **Souris** (2057, `Dialog(458)` : Ctrl/Maj/Alt + action) | molette = volume/seek, clic milieu, double-clic | 🟡 | les modificateurs (Ctrl+clic…) ne sont pas tous exposés |
| **Thèmes** (2058, `Dialog(376)` : dossier « Thème », mode sombre système) | thème sombre intégré | 🟡 | `egui::Visuals` ; pas de thèmes externes chargeables |
| **Messages OSD** (2062) | OSD ✅, durée réglable, désactivable | ✅ | |
| **Logo** (2059), **Visualisation** (2060) | non | ➖ | spécifiques PotPlayer |
| **Association** (2095) | non | ❌ | `.desktop` + `MimeType=video/*` + `xdg-mime default`. **Petit et attendu** (③) |
| **Configuration** (2096, import/export) | fichier JSON à la main | 🟡 | export/import depuis les préférences serait trivial |

**Priorités de cette catégorie** : mémorisation de fenêtre, association de fichiers, MPRIS,
glisser-déposer. Les trois premiers sont « petit effort, gros effet attendu ».

---

## 2. Général › Lecture (page 2064)

| Paramètre | KPlayer | État | Remarque |
|---|---|---|---|
| **Reprise de la lecture** | `playback.resume` + position par média | ✅ | vérifié par `verif-reprise.sh` |
| **Vitesse de lecture** (0,1× à 16×) | 0,1× à 4×, hauteur préservée si demandé | ✅ | `verif-audio.sh` |
| **Saut avant/arrière** | 1 s / 5 s / 30 s + chapitre | ✅ | table de raccourcis |
| **Répétition A-B** (`Dialog(453)` : points A/B, nb de boucles, délai) | A, B, boucle ✅ | 🟡 | **nombre de boucles** et **délai de répétition** absents |
| **Boucle** (fichier / liste) | ✅ | ✅ | |
| **Lecture aléatoire** | ❌ | ❌ | `playlist.shuffle` + tirage sans répétition (Fisher-Yates). **Petit** (④) |
| **Priorité du processus** | ❌ | ❌ | `libc::setpriority` ; utile seulement en 4K |
| **Précache / tampon réseau** | ✅ `playback.precache_ms` (défaut 6 s) | ✅ **au-delà** | PotPlayer n'a pas d'équivalent mesuré ; voir `verif-precache.sh` |
| **Reprise après reconnexion** | ✅ suit l'horloge audio | ✅ **au-delà** | |

---

## 3. Général › Capture d'écran (page 2068, `Dialog(411)`)

| Paramètre | KPlayer | État |
|---|---|---|
| Capture d'une image (touche S) | ✅ PNG, dossier configurable | ✅ |
| Nom de fichier avec info fichier / temps / extension | 🟡 horodatage | 🟡 |
| Générer des vignettes par recherche d'images-clés | ❌ | ❌ → chantier **⑤ vignettes** |
| Traitement par lot d'une liste de fichiers | ❌ | ➖ |

---

## 4. Périphérique (pages 2067-2073, 2098)

Webcam, TV analogique (WDM), TV numérique (BDA), canaux TV, DVD/Blu-ray, capture audio :
**➖ sans objet** — ce sont des API Windows (DirectShow/BDA). Sous Linux l'équivalent serait
V4L2, hors du périmètre d'un lecteur.

Ce qui **existe** côté KPlayer et n'a pas d'équivalent PotPlayer : lecture **DVD/Blu-ray par
fichier/dossier**, flux **HLS/réseau**, **piste image** (PGS, sous-image DVD).

---

## 5. Contrôle du filtre (pages 2074-2078, `Dialog(423)`)

| Paramètre | KPlayer | État | Remarque |
|---|---|---|---|
| **Décodeurs vidéo disponibles** (`Dialog(423)`) | FFmpeg + VA-API, repli logiciel observé | 🟡 | pas de choix manuel du décodeur ; Linux = FFmpeg |
| **Utiliser l'accélération matérielle** (DXVA → ici VA-API) | ✅ `video.hardware_decoding` | ✅ | mode réel affiché |
| **Priorité FFmpeg / D3D11** | — | ➖ | spécifique Windows |
| **File d'attente en sortie** | file interne + pré-cache | ✅ | |
| **Réduire la fréquence d'image pour adapter le récepteur** | non | ❌ | `fps`/`framestep` ; usage marginal |
| **Reconnaissance d'entrelacement TV et film** | ✅ `deinterlace` + parité auto + `deint=interlaced` | ✅ | **et désormais côté GPU** |
| **Compatibilité H.264**, **limite de résolution**, **boucle saut filtre** | — | ➖ | réglages DXVA |
| **Greffons externes** (DirectShow) | ❌ | ➖ | remplacé par la chaîne libavfilter |

---

## 6. Vidéo (pages 2082-2088)

### 6.1 Décodage et rendu

| Paramètre | KPlayer | État | Remarque |
|---|---|---|---|
| Accélération matérielle (DXVA/D3D11/CUVID/QSV/AMF chez PotPlayer) | **VA-API** + repli mesuré | ✅ | couvre Intel/AMD ; NVIDIA passe par `nvidia-vaapi-driver` |
| **Sortie 10 bits** | non (RGBA 8 bits) | ❌ | demanderait `Rgba16`/P010 → gros chantier d'affichage |
| **HDR / tone mapping** | non | ❌ | `zscale`+`tonemap` absents du build — mais `libplacebo` serait la vraie réponse |
| **Rendu GPU à filtres** (`gpu-next`) | ❌ | ❌ | lourd ; voir §0 pour ce qui est **déjà** gagné sans lui |
| **Réduction d'échelle** | ✅ **nouveau : GPU** (`scale_vaapi`) ou CPU (5 méthodes) | ✅ | mesuré −42 % |
| **Super résolution / RTX VSR** | non | ➖ | matériel Windows |
| **Vsync / tampon / Direct3D** | vsync d'egui | ➖ | géré par le compositeur |
| **Palette limitée 16-235** | conversion standard | 🟡 | pas de bascule manuelle |

### 6.2 Colorimétrie (`Dialog(384)`)

| Paramètre | KPlayer | État |
|---|---|---|
| **Luminosité, Contraste, Saturation** | ✅ `eq` (CPU) | ✅ |
| **Teinte (hue)** | ❌ | ❌ → filtre `hue=h=…`, trivial à ajouter |
| **Gamma** | ✅ | ✅ |
| **Colorimétrie d'entrée/sortie (BT.601/709/2020)** | automatique | 🟡 pas de choix manuel |
| **Contrôles de couleur matériels** | ❌ (pilote : `procamp_vaapi` non implémenté **ici**) | ➖ mesuré |
| **Conversion HQ / multitâches** | swscale `SWS_ACCURATE_RND` | ✅ |

### 6.3 Désentrelacement (`Dialog(385)`)

| Paramètre | KPlayer | État |
|---|---|---|
| **Désentrelacement matériel** (méthode, ordre de champ) | ✅ **nouveau** : `deinterlace_vaapi` (GPU) | ✅ −38 % |
| **Désentrelacement logiciel** : `bwdif` / `yadif` | ✅ les deux, `bwdif` par défaut (meilleur : 49,0 contre 43,4 dB) | ✅ |
| Ordre de champ forcé (tff/bff) | ✅ | ✅ |
| **Mode** (envoyer trame / champ) | `send_frame` imposé | ✅ choix assumé (cadence 1:1) |

### 6.4 Effets (`Dialog(386)`)

| Paramètre | KPlayer | État |
|---|---|---|
| **Netteté** (luminance/chrominance) | ✅ `unsharp` 3 niveaux | ✅ |
| **Débruiter 3D** (luma/chroma/temps) | ✅ `hqdn3d` 3 niveaux mesurés | ✅ |
| **Débander** (seuil, rayon) | ✅ `deband` réglable | ✅ |
| **Adoucir** (rayon, luma, chroma) | ❌ | ❌ → `gblur`/`boxblur`, trivial |
| **WarpSharp** (profondeur, seuil) | ❌ | ❌ → filtre `warpsharp2` non compilé (plugin `avfilter` externe) |
| **Mouvement flou** | ❌ | ❌ → `tmix`/`minterpolate`, coûteux |
| **Rotation 90/180/270**, **miroirs H/V** | ✅ | ✅ |
| **Glisser (décalage)** | ❌ | ❌ → `crop`+`pad` |

### 6.5 Redimensionner / Étendre-Rogner / Niveaux (`Dialog(456)`, 2084-2087)

| Paramètre | KPlayer | État |
|---|---|---|
| **Zoom / recadrage** | ✅ zoom 1-4×, recadrage des bandes | ✅ |
| **Rapport d'aspect** (auto, 16:9, 4:3, personnalisé) | ✅ | ✅ |
| **Étendre à l'écran / rogner** | 🟡 recadrage seulement | 🟡 |
| **Niveaux / décalage** | ❌ | ❌ → `colorlevels`, trivial |
| **Plein écran : résolution, autres moniteurs en noir** | 🟡 plein écran oui | 🟡 |

---

## 7. Audio (pages 2089-2094)

| Paramètre | KPlayer | État |
|---|---|---|
| **Contrôle du volume** (méthode, volume de démarrage, accélération) | volume 0-150 %, muet, `Ctrl+↑/↓` | ✅ |
| **Égaliseur** (`Dialog(363)` : 18 bandes SuperEQ + préréglages) | **18 bandes** `superequalizer` | ✅ (préréglages à ajouter ➖) |
| **Normaliseur** (niveau, méthode, relâchement, étouffement) | ✅ `dynaudnorm` réglable | ✅ |
| **Freeverb** (taille de salle, wet/dry, largeur) | ✅ `aecho` 3 tailles | 🟡 pas de wet/dry séparé |
| **Réduction du bruit** (`Dialog(388)`) | ✅ 3 niveaux `afftdn` mesurés | ✅ |
| **Cristallisation / stéréo extra / crossfeed / écho** | ✅ élargissement stéréo, crossfeed, karaoké | 🟡 |
| **Plugins Winamp DSP** | ❌ | ➖ : remplacé par libavfilter |
| **Sélection du périphérique de sortie** | ✅ `audio.device` | ✅ |
| **Capture audio** | ➖ | |

---

## 8. Sous-titres (pages 2079-2081)

### 8.1 Style police (`Dialog(383)`)

| Paramètre | KPlayer | État |
|---|---|---|
| **Police** | ✅ **nouveau (s.31)** : nom résolu par `fc-match` | ✅ |
| **Taille** | ✅ 8-160 pt | ✅ |
| **Alphabet** (limiter à un jeu de caractères) | ❌ | ❌ usage très marginal en France |
| **Interlettre** | ❌ | ❌ `layout` egui : espacement manuel, effort moyen |
| **Texte / Contour / Ombre / 2nd contour** (couleur, transparence) | texte ✅, contour ✅ (8 directions), ombre ✅ en partie | 🟡 **2nd contour absent** |
| **Contour flou / flou gaussien** | ❌ | ❌ coûteux au rendu par image |
| **Échelle X / Y** | ❌ | ❌ |
| **Boîte opaque** | ✅ bandeau (opacité 0-255) | ✅ |
| **Marges haut/gauche/droite/bas** | 🟡 marges de 12 px fixes + position % | 🟡 |
| **Interligne** | ✅ (1,25 × taille) | 🟡 non réglable |
| **Position hori. (%)** | ✅ **nouveau (s.31)** : ancrage début/centre/fin | ✅ |
| **Position verti. (%)** | ✅ 0-100 % | ✅ |
| **Alignement** | ✅ gauche / centre / droite | ✅ |
| **Ruby** (furigana) | ❌ | ❌ japonais uniquement |
| **Assombrir l'arrière-plan**, « Encadrer l'arrière-plan » | ✅ bandeau | ✅ |

### 8.2 Fondu / 3D / Emplacement (page 2081) et priorité (`Dialog(442)`)

| Paramètre | KPlayer | État |
|---|---|---|
| **Fondu** des sous-titres | ❌ | ❌ `alpha` par image, facile mais cosmétique |
| Position 3D, profondeur | ➖ | |
| **Langues préférées**, priorité, durées mini/maxi d'affichage | 🟡 `default_language` seulement | 🟡 |
| **Mémoriser la piste choisie** | 🟡 par média (reprise) | 🟡 |
| **Sous-titres CEA-608/708, SMS/TTS** | ❌ | ➖ |
| **Rendu ASS complet** (styles, `\clip`, `\pos`, `\frz`…) | ✅ (limites documentées) | ✅ **au-delà** de la plupart des lecteurs |

---

## 9. Kakao TV / diffusion (page 2097, `Dialog(304)`, `Dialog(445)`)

**➖ sans objet** (services coréens, FTP/SSL). KPlayer lit les flux **HLS/réseau** avec
précache, ce qui couvre l'usage réel.

---

## 10. Ce que KPlayer a et que PotPlayer n'a pas

- **Profils/règles automatiques** (résolution, motif de nom → réglages) — `src/config.rs`,
  24 contrôles dans `verif-regles.sh`.
- **Précache réseau mesuré** (6 s d'avance, coupure de 3 s absorbée).
- **Pistes image** (PGS, sous-image DVD) et **ASS avancé** (`\clip`, `\iclip`, rotations).
- **Bancs de vérification** : 25 bancs, **460 contrôles** — chaque réglage a une mesure (vérification ciblée pendant l'implémentation des modules : `tools/verif-rapide.sh`).
- **Journal explicite** de chaque décision (quelle police, quel filtre, quel chemin GPU).

---

## 11. Plan classé : ce qu'il reste à faire, du meilleur au moins bon

| # | Catégorie | Chantier | Effort | Effet |
|---|---|---|---|---|
| ~~①~~ | Général | ~~**Mémoriser taille et position de fenêtre**~~ | **fait (s.32)** | X11 : `interface.window_pos/size/maximized` (`verif-fenetre.sh` 9/9). Wayland (Plasma 6.8) : taille + agrandi par le lecteur, position par **règle KWin** (`tools/regle-fenetre-kwin.sh`, `verif-fenetre-wayland.sh` 12/12) |
| ~~②~~ | Général | ~~**Association de fichiers**~~ | **fait (s.33)** | `.desktop` + icône, `--par-defaut` sur demande, instance unique par socket (`installer-association.sh`, `verif-association.sh` 38/38) |
| ~~③~~ | Général › Lecture | ~~**Lecture aléatoire** + **glisser-déposer**~~ | **fait (s.34)** | permutation suivie (une fois par cycle), `Ctrl+A`, 🔀, DnD avec repère, ordre persisté, volet borné (`verif-playlist.sh` 29/29) |
| ~~④~~ | Vidéo | ~~**Vignettes de prévisualisation**~~ | **fait (s.34)** | `src/vignettes.rs`, 14/14 module seul + 3 d'intégration |
| ~~⑤~~ | Général | ~~**MPRIS**~~ | **fait (s.34)** | `src/mpris.rs` + zbus : `org.mpris.MediaPlayer2.kplayer`, propriétés modifiables, réveil de l'interface (`verif-mpris.sh` 27/27 + 8 d'intégration) |
| ~~⑥~~ | Vidéo | ~~**Teinte**, **niveaux**, **adoucir**~~ | **fait (s.34)** | `video.hue`, `niveaux_noir/blanc`, `adoucir` ; mesuré : rouge→vert à 90°, gris lavé 38→0, HF 78→4,5 (`verif-filtres` 20/20) |
| ~~⑦~~ | Sous-titres | ~~**Interlettre** et **marges L/R**~~ | **fait (s.34)** | `subtitles.letter_spacing`, `margin_left/right_percent` ; largeur 268→340 px, bloc décalé de 313 px à 25 % (`verif-soustitres` 7/7) |
| ~~⑧~~ | Audio | ~~Préréglages d'égaliseur, équilibre direct/réverbéré~~ | **fait (s.34)** | presets **Cinéma** et **Podcast** (+ `EqPreset::CHOIX`) ; `audio.reverb_mix` exposant `aecho` in/out ; mesuré (`verif-audio` 34/34) |
| ~~⑨~~ | Général | ~~**Suspendre à la perte de focus / quand réduit**~~ | **fait (s.34)** | `interface.pause_sur_perte_focus` (jamais / focus / réduite) + reprise séparée ; `verif-focus` 6/6 |
| ~~⑩~~ | Vidéo | ~~**`libplacebo` (HDR, 10 bits, deband GPU)**~~ | **fait (s.35)** | FFmpeg recompilé avec libplacebo/libzimg/vulkan ; `src/hdr.rs` : détection PQ/HLG/BT.2020, conversion tonale (GPU ou repli CPU), tramage 10→8, débanding GPU ; `verif-hdr.sh` **29/29**, dont décodage VA-API identique au logiciel et détection de l'écran par EDID ; passthrough branché mais impossible (fenêtre 8 bits — `docs/HDR.md`) |
| ⑪ | Vidéo | 10 bits + affichage sans rapatriement (dmabuf/wgpu) | **très lourd** | perf 4K |

Les six premiers sont dans l'ordre où ils rendent le lecteur **plus proche de PotPlayer au
quotidien** ; les trois derniers ne se justifient que si le besoin (HDR, 4K, qualité
maximale) se présente réellement.

---

## 12. Ce que ce document ne dit pas

- Les réglages **Windows sans objet** ne sont pas détaillés au-delà d'une ligne : les
  reproduire n'aurait aucun sens sous Linux.
- Les **valeurs par défaut de PotPlayer** ne sont pas toutes connues : son `.ini` de
  configuration n'est pas livré avec l'installation, et je n'ai pas lancé le programme sous
  Wine pour les relever (mesure de débit non représentative de toute façon).
- La **qualité** d'un filtre n'est citée que lorsqu'elle a été mesurée en PSNR sur cette
  machine (`bwdif` vs `yadif`, échelle GPU vs Lanczos).

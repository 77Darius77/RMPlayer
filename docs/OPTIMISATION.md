# Optimisation — mesures et plan

Établi le 2026-10-18 (session 31). **Toutes les valeurs de ce document sont mesurées** sur
cette machine, avec les binaires installés ; aucune n'est reprise d'une documentation.
La commande ou l'outil qui produit chaque chiffre est indiqué.

---

## 1. Provenance

| Élément | Valeur vérifiée |
|---|---|
| Notre FFmpeg | `/opt/ffmpeg-7.1` → **7.1.3**, `Lavc61.19.101` / `Lavf61.7.100` |
| Notre configuration FFmpeg | `--enable-gpl --enable-nonfree --enable-shared --disable-static --enable-pic` — **aucune bibliothèque externe** |
| Nos accélérations | `-hwaccels` → **vaapi, drm, vulkan** ; `libdav1d`, `libplacebo`, `zscale` : **absents** |
| Source de la mesure de coût | `rmplayer --bench 200` et `ffmpeg` (le même 7.1.3), média **H.264 1080p 25 i/s**, 200 images, meilleure de 3 passes |

> **Correction d'une affirmation antérieure** : j'avais écrit que notre build n'avait « ni
> `hwcontext_vulkan` ». C'est **faux** : `-hwaccels` liste `vulkan` et `-init_hw_device
> vulkan=vk` aboutit (code 0). Ce qui manque est **`libplacebo`** (et `zimg`/`zscale`), pas
> le contexte Vulkan. Il n'y a en revanche **aucun filtre Vulkan** (`-filters | grep vulkan`
> → 0), donc le contexte Vulkan ne sert aujourd'hui à rien dans la chaîne.

---

## 2. Où part le temps, aujourd'hui (notre chaîne)

Chemin actuel, pour chaque image : décodage → **copie GPU→CPU** (`hwdownload`) → **mise à
l'échelle CPU** (swscale) → **conversion RGBA** → téléversement de la texture.

| Réglage | ms / image (1080p, 200 images) | Coût du réglage |
|---|---|---|
| Décodage matériel (VA-API), aucun filtre | **11,96** | référence |
| Décodage **logiciel**, aucun filtre | 12,92 | +0,96 (le matériel ne rapporte presque rien) |
| + luminosité (`eq`) | 12,35 | +0,39 |
| + désentrelacement (`bwdif`, source progressive) | 12,11 | +0,15 |
| + détramage (`deband`) | 13,99 | +2,03 |
| + netteté (`unsharp`) | 13,98 | +2,02 |
| + débruitage force 3 (`hqdn3d`) | **17,20** | **+5,24** |

**Le fait marquant** : le décodage matériel ne rapporte que **1 ms/image** dans notre
chaîne. Ce n'est pas que VA-API soit inutile — c'est que les 12 ms sont presque toutes
dans la **copie et la conversion**, pas dans le décodage. C'est exactement là que fait la
différence un moteur qui conserve les surfaces du GPU jusqu'à l'affichage.

---

## 3. Le gain mesuré qui est à notre portée : l'échelle dans le GPU

Même média, même FFmpeg (le nôtre), deux chaînes, 1080p → 720p, sortie RGBA dans les deux
cas (c'est ce que notre affichage consomme) :

| Chaîne | ms / image | Images produites |
|---|---|---|
| **A — ce que fait RMPlayer** : `hwdownload` → échelle CPU Lanczos → RGBA | **9,95** | 200 |
| **B — échelle dans le GPU** : `scale_vaapi` → `hwdownload` → RGBA | **4,60** | 200 |

**−54 % sur l'étape dominante**, sans changer de matériel, sans recompiler FFmpeg : notre
build possède déjà `scale_vaapi`. Le principe : ne pas rapatrier des surfaces 1080p pour
les réduire à 720p sur le CPU, mais les réduire **dans le GPU** et ne rapatrier que le
résultat.

### Ce qui marche et ce qui ne marche pas sur cette machine

Testé avec `-hwaccel_output_format vaapi` (surfaces conservées) :

| Filtre VA-API | État mesuré |
|---|---|
| `scale_vaapi` | ✅ fonctionne (200 images) |
| `deinterlace_vaapi` | ✅ fonctionne |
| `procamp_vaapi` (luminosité/contraste/saturation/teinte) | ❌ **« Failed to query procamp filter caps: 20 (not implemented) »** — le pilote ne l'implémente pas ici |
| `sharpen_vaapi` | présente, options à préciser (l'échec mesuré était une erreur de syntaxe, pas de pilote) |

Conséquence de conception : **les corrections de couleur resteront sur le CPU** sur cette
machine (le pilote ne sait pas les faire), et tout filtre CPU force de toute façon le
rapatriement. Le gain s'applique donc **au cas courant** : lecture simple (éventuellement
redimensionnée), et désentrelacement — c'est-à-dire la majorité des lectures.

---

## 4. Ce qui manque au moteur

| Élément | État dans RMPlayer |
|---|---|
| Version FFmpeg | Lavc61.19 (7.1.3) |
| AV1 **logiciel** | **absent** au moment de la mesure → AV1 impossible sans VA-API (corrigé depuis, §5) |
| Rendu GPU à filtres | `libplacebo` **absent** (pas de `zscale` non plus) au moment de la mesure |
| Décodage matériel | VA-API (+ repli logiciel) |
| Surfaces conservées jusqu'à l'affichage | non : rapatriement systématique |

Deux de ces écarts sont déjà dans la liste « priorité moyenne » (`gpu-next`, AV1 logiciel) ;
ce document leur donne **un chiffre** et un **ordre**.

---

## 5. Plan d'optimisation, par rapport gain / effort

> **État au 2026-10-18 (session 31)** : les chantiers **1, 2 et 3 de ce tableau sont
> IMPLÉMENTÉS et mesurés** — échelle GPU **−42 %** (10,85 → 6,3 ms/image), désentrelacement
> GPU **−38 %** (11,05 → 6,8), AV1 logiciel opérationnel. Voir `AVANCEMENT.md` §6.37 et
> `tools/bench-optimisation.sh`. Le tableau ci-dessous est conservé comme raisonnement
> d'origine ; les chiffres de la colonne « gain attendu » sont les mesures de la §3.

| # | Chantier | Gain attendu | Effort | Vérification |
|---|---|---|---|---|
| 1 | **Échelle VA-API** (`scale_vaapi`) quand le décodeur matériel est actif et qu'aucun filtre CPU n'est demandé ; sinon chemin actuel | **−54 % mesurés** sur l'étape dominante (9,95 → 4,60 ms/image en 1080p→720p) | moyen : une branche dans la construction de la chaîne, plus la gestion des deux formats de sortie | `--bench` avant/après sur 1080p et 4K ; contrôle d'image (même rendu à 1 niveau près) |
| 2 | **Désentrelacement VA-API** (`deinterlace_vaapi`) en remplacement de `bwdif` quand VA-API est actif et qu'aucun filtre CPU ne suit | faible en CPU sur source progressive (+0,15 ms), réel sur source entrelacée 1080i (bwdif CPU ≈ 2-3 ms) | moyen (même branche que 1) | `verif-filtres.sh` (peigne résorbé) + `--bench` sur un 1080i |
| 3 | **AV1 logiciel** (`libdav1d`) | rend l'AV1 lisible sans VA-API ; décodeur souvent plus rapide que le natif | **court** : `libdav1d-dev` 1.4.1 est installé, recompiler FFmpeg avec `--enable-libdav1d` | AV1 de test lu en `--no-hwdec`, comparaison CPU via `bench-hwdec.sh` |
| 4 | **`libplacebo`** (rendu GPU à filtres, tone-mapping, deband GPU) | qualitatif + décharge CPU (le débruitage/détramage coûtent 2 à 5 ms/image) | **lourd** : `libplacebo-dev` absent, sources FFmpeg absentes, recompilation complète | mesure identique à §6.32 (`bench-desentrelacement.sh` comme modèle) |
| 5 | **Affichage sans rapatriement** (dmabuf/EGL, rendu `eframe` en `wgpu`) | supprime les ~5 ms restants de copie et de téléversement | **très lourd** : change la couche d'affichage | mesure d'images/s et de CPU à 4K |

**Ce qu'il ne faut pas attendre** : recompiler pour FFmpeg 8.x ne donne pas, à lui seul, de
gain mesurable ici (le décodage n'est pas le goulot). C'est **le déplacement du travail vers
le GPU** qui compte.

---

## 6. Ordre recommandé

1. **Échelle VA-API** (n° 1) : gain mesuré le plus fort, aucun outillage à installer, et
   le banc existe déjà (`--bench` + `tout-verifier.sh` contre les régressions d'image).
2. **AV1 logiciel** (n° 3) : effort le plus faible de la liste, supprime une limite
   fonctionnelle réelle (AV1 illisible en logiciel).
3. **Désentrelacement VA-API** (n° 2) : à faire dans la même foulée que 1, même branche.
4. `libplacebo` et zéro-copie : seulement si le besoin HDR/qualité se présente.

---

## 7. Ce que ce comparatif ne dit pas

- Le coût **d'affichage** (egui/OpenGL, rendu de la texture à l'écran) n'est pas mesuré par
  `--bench`, qui décode sans fenêtre. Il peut dominer en 4K.
- Les chiffres valent pour **cette machine** (pilote VA-API Mesa, GPU intégré) : `procamp_vaapi`
  peut fonctionner ailleurs ; il faut refaire le test sur la cible avant de généraliser.

# ETAT-COURT — fiche de reprise

**À lire en premier, et c'est tout.** Une page suffit pour reprendre le projet sans relire
les longs documents. Le bloc ci-dessous est **mesuré** (régénéré par `tools/etat-court.sh`) ;
le reste est du jugement, écrit à la main et tenu à jour à chaque session.

<!-- AUTO:debut — bloc régénéré par tools/etat-court.sh, ne pas éditer à la main -->
```
Généré le            2026-10-24 16:00
Code                 25306 lignes, 19 modules (src/)
Bancs de contrôle    26 (tools/tout-verifier.sh)
Dernière suite       2026-10-24 15:56 — 505 contrôles, 0 échec(s), 26 bancs
Tests unitaires      133 (cargo test --release)
Avertissements       0 (cargo build --release)
Dernière archive     kplayer-2026-10-24-session36.tar.gz — 87 fichiers, 628 Ko
FFmpeg               7.1.3 (/opt/ffmpeg-7.1)
  AV1 logiciel       oui (libdav1d)
  scale_vaapi        oui
  deinterlace_vaapi  oui
  HDR (libplacebo)   oui (Vulkan)
```
<!-- AUTO:fin -->
> **Tout est vérifié** : dernière suite complète **2026-10-24 15:56 — 505 contrôles,
> 0 échec, 26 bancs** (lancée depuis un shell Wayland brut, voir piège 114), 133 tests
> unitaires, 0 avertissement. **Tout le plan PotPlayer est
> fait** : les cinq chantiers (s.32-34), la finition des réglages (⑥→⑨) et le **HDR** (⑩,
> s.35 : conversion tonale + détection d'écran par EDID). Le **passthrough HDR est branché
> mais inatteignable** — la fenêtre est en 8 bits (`docs/HDR.md`).


## Le projet en trois lignes

Lecteur vidéo **Rust + egui** (Linux), sans dépendance à un lecteur externe : FFmpeg lie
directement (`ffmpeg-sys-next`, FFmpeg 7.1.3 de `/opt/ffmpeg-7.1`), audio par `cpal`,
réglages en JSON. Dépôt : `/media/blaise/LINUX_DATA/lecteur-linux`. **Contrôle de version
git depuis la session 36** (branche `main`, étiquette `session36` = l'état vérifié à 505
contrôles ; pas de dépôt distant) ; les archives de `sauvegardes/` restent comme copies hors
dépôt (non suivies par git).

## Les trois optimisations en place (mesurées, session 31)

| Chemin | Gain | Réglage |
|---|---|---|
| Réduction d'échelle **dans le GPU** (`scale_vaapi`) | 1080p→720p : 10,85 → **6,3 ms/image (−42 %)** | `video.gpu_scaling` (défaut oui) |
| **Désentrelacement matériel** (`deinterlace_vaapi`) | 1080i→720p : 11,05 → **6,8 ms/image (−38 %)** | même drapeau + `video.deinterlace` |
| **AV1 logiciel** (`libdav1d` compilé) | AV1 lisible sans matériel, 179 i/s en 720p | automatique |

Contreparties assumées, vérifiées : le filtre du pilote est plus doux que Lanczos
(PSNR 30,7 dB) → la case à cocher rend Lanczos ; `procamp_vaapi` n'existe pas sur ce pilote
→ luminosité/contraste/saturation restent sur le CPU.

## Fait récemment (sessions 32-34)

| Chantier | Ce qui marche | Preuve |
|---|---|---|
| **Mémoire de la fenêtre** (s.32) | taille + état agrandi partout, **position sous X11** ; sous Wayland (Plasma 6.8, seule session) la position relève d'une règle KWin — `tools/regle-fenetre-kwin.sh` | `verif-fenetre` 9/9, `verif-fenetre-wayland` 12/12 |
| **Association de fichiers** (s.33) | « Ouvrir avec », menu, icône ; **instance unique** (le fichier arrive dans la fenêtre ouverte) ; `--par-defaut` sur demande | `verif-association` 38/38 |
| **Réglages** (s.34) | teinte, niveaux, adoucir (vidéo) ; interlettre et marges des sous-titres ; presets Cinéma/Podcast et équilibre de réverbération ; pause selon le focus | `verif-filtres` 20/20, `verif-soustitres` 7/7, `verif-audio` 34/34, `verif-focus` 6/6 |
| **HDR** (s.35) | conversion tonale HDR→SDR (libplacebo/Vulkan ; moteur CPU seulement sur demande, **sans Vulkan l'image reste non convertie** — `docs/HDR.md`), tramage 10→8 bits, débanding GPU ; détecte PQ/HLG/BT.2020 **et la capacité de chaque écran par EDID** (passthrough branché mais impossible : fenêtre 8 bits — `docs/HDR.md`) | `verif-hdr` 29/29 |
| **MPRIS** (s.34) | touches média et widget du bureau : `org.mpris.MediaPlayer2.kplayer`, état publié, Volume/Shuffle/LoopStatus/Rate réglables, commandes exécutées par l'interface (réveil même en pause) | `verif-mpris` 27/27 (module seul) + 8 contrôles d'intégration |
| **Vignettes** (s.34) | survoler la barre de progression montre l'image de l'instant pointé (décodeur autonome, fil de travail, 320×180, 4 à 175 ms) | `verif-vignettes` 14/14 + 3 contrôles d'intégration |
| **Playlist** (s.34) | lecture **aléatoire** (chaque élément une fois, `Ctrl+A`, 🔀, mémorisé) et **glisser-déposer** avec repère, ordre enregistré ; volet **borné** (300 px, max 420), lignes à interaction unique (clic = lire, glisser = déplacer), ✕ toujours visible, texte tronqué | `verif-playlist` 18/18 |

Réglages : `interface.window_pos/size/maximized`, `playback.shuffle`. Outils :
`installer-association.sh`, `regle-fenetre-kwin.sh`. Détail et mesures : §6.38 à §6.40 de
`docs/AVANCEMENT.md`.

## Ce qu'il reste à faire, dans l'ordre


**Tout le plan est fait** : les cinq chantiers (fenêtre, association, playlist, vignettes,
MPRIS), la finition des réglages (⑥→⑨) **et le HDR** (⑩, session 35 : conversion tonale,
tramage 10→8, débanding GPU). Il ne reste que l'**affichage sans rapatriement** (dmabuf/EGL),
qui n'a de sens qu'avec une surface capable de HDR — la fenêtre est aujourd'hui en RGBA 8 bits
SDR, et c'est dit dans `src/hdr.rs`. Voir **§11 de `docs/POTPLAYER-VERS-KPLAYER.md`**.

**Pour de nouvelles fonctions « à la PotPlayer »** : le reste à faire réel (vérifié dans le
code, la matrice de `POTPLAYER-VERS-KPLAYER.md` étant périmée) est dans
**`docs/PROCHAINES-FONCTIONS.md`**, et la façon de les ajouter sans produire un faux « fait »
dans **`docs/AJOUTER-UNE-FONCTION.md`**. Premiers candidats : fichiers récents, nombre de
boucles A-B, export/import de la configuration, puis marges haut/bas et interligne des
sous-titres, puis marque-pages.

## Les sept pièges qui coûtent une session si on les oublie

1. **Un réglage enregistré n'est pas un réglage appliqué** : vérifier qu'un pixel change.
2. **`ctx.*` pendant `ctx.input(...)` fige la fenêtre** et KWin la tue.
3. **`egui::Context::set_fonts` remplace TOUTES les polices** (réinstaller la police de secours).
4. **Ne jamais sauter un filtre parce qu'une copie a eu lieu** : `transferred != null` est vrai
   toujours → tous les filtres CPU sont perdus en décodage matériel (régression de la s.31).
5. **Wayland** : pas de position de fenêtre pour le client (règle KWin), et une **sonde de
   protocole doit partager le tampon** de la connexion — sinon elle avale la première donnée.
6. **Un chemin d'interface qui appelle le moteur en direct saute tout le reste** (règles de
   profil, sous-titres, OSD) : « suivant » le faisait, et `let _ =` cachait l'échec.
7. **Deux suites de vérification en parallèle rendent des résultats faux sans le dire** : un
   banc qui capture l'écran mesure la fenêtre de l'autre. `tout-verifier.sh` prend un verrou.

Les **116 pièges**, avec le détail de chacun, sont en **§4 de `docs/AVANCEMENT.md`** — c'est la
seule section longue qu'il vaut la peine de parcourir avant d'écrire du code.

## Où lire quoi (uniquement sur question précise)

| Question | Fichier |
|---|---|
| Ce qui marche / ce qui reste, par domaine | `docs/ETAT-DU-PROJET.md` |
| **Quelle fonction ajouter ensuite** (ce qui manque vraiment vs PotPlayer) | `docs/PROCHAINES-FONCTIONS.md` |
| **Comment ajouter une fonction** (recette, pièges, bancs) | `docs/AJOUTER-UNE-FONCTION.md` |
| Un réglage précis face à PotPlayer (colonnes de statut **périmées**, voir ci-dessus) | `docs/POTPLAYER-VERS-KPLAYER.md` (chercher `Dialog(N)`) |
| Chiffres et plan d'optimisation | `docs/OPTIMISATION-COMPARATIF.md` |
| API, structures, modules | `docs/ETAT-TECHNIQUE.md` |
| Fonctionnalités et limites assumées | `docs/INVENTAIRE-FONCTIONNALITES.md` |
| Sessions 2 à 31 (détail historique) | `docs/AVANCEMENT-ARCHIVE.md` |
| **Faire relire le projet par un tiers** (Claude, un collègue) | `docs/BRIEF-REVUE-EXTERNE.md` — budget de lecture, six vérifications qui valent mieux que la lecture, deux pièges à faux positifs |

## Vérifier avant de conclure

```bash
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
cargo test --release                    # 133 tests
tools/verif-rapide.sh <sujet>           # module touché : hdr, playlist, vignettes, mpris, ui…
tools/verif-hdr.sh                      # HDR seul (32 contrôles, mires en nits)
tools/verif-mpris.sh                    # le module seul (27 contrôles, --mpris-test)
tools/verif-vignettes.sh                # le module seul (14 contrôles, ~20 s)
tools/tout-verifier.sh                  # 26 bancs, 505 contrôles, 0 échec attendu (~21 min)
tools/etat-court.sh                     # remet cette fiche à jour (après la suite)
tools/bench-optimisation.sh             # re-mesure les gains GPU et l'AV1
```

Puis **committer** (`git add -A && git commit`, et `git tag session<N>` en fin de session), et
éventuellement **archiver** : `tar czf sauvegardes/kplayer-<date>-session<N>.tar.gz src docs tools
assets icon.png Cargo.toml Cargo.lock .cargo run.sh README.md .gitignore`
(`assets/` et `icon.png` servent à `installer-association.sh` : les oublier donne une archive qui ne reconstruit pas l'association de fichiers — omission constatée à la session 36), et **vérifier** par extraction
comparée (`diff -r`), pas seulement `gzip -t`.

# Prochaines fonctions — ce qui manque réellement par rapport à PotPlayer

*Établi le 2026-10-24 (session 36). À lire avant de choisir quoi implémenter ; la recette est
dans [`AJOUTER-UNE-FONCTION.md`](AJOUTER-UNE-FONCTION.md).*

## Pourquoi ce document existe

La matrice de [`POTPLAYER-VERS-KPLAYER.md`](POTPLAYER-VERS-KPLAYER.md) (§1 à §9) date de la
session 31 : **ses colonnes de statut sont périmées**. Elle marque encore ❌ des choses faites
depuis — teinte, niveaux, adoucir, association de fichiers, MPRIS, lecture aléatoire, pause à
la perte de focus, mémoire de la fenêtre, HDR, interlettre, marges latérales, vignettes. Se
fier à elle ferait refaire du travail. **Ce document remplace ses colonnes de statut** ; le
§11 de ce même fichier reste la trace du plan des sessions 32-35 (tout y est fait sauf ⑪).

**Comment les statuts ont été établis.** Pour chaque ligne ❌ ou 🟡 de la matrice, recherche du
réglage dans `src/config.rs` (un champ de `Config` qui n'existe pas = fonction absente) et
recoupement avec `docs/ETAT-COURT.md`. Aucune de ces fonctions n'a été essayée dans PotPlayer :
le périmètre vient du catalogue de ses réglages, pas d'un tour de ses menus (voir « Ce que ce
document ne couvre pas »).

**Les efforts sont indicatifs** (petit = moins d'une session, moyen = une session, gros = plus).
**L'ordre est un jugement**, expliqué plus bas ; rien n'a été arbitré avec vous.

---

## Lot 1 — petit, attendu à l'usage, testable sans interface

| # | Fonction | PotPlayer | Où | Comment le prouver | Effort |
|---|---|---|---|---|---|
| A1 | **Fichiers récents** : liste bornée, menu, « effacer » | `Dialog` 3127 | `playback.recent` ; menu dans `ui.rs` | ouvrir 3 fichiers : `config.json` contient 3 entrées, la plus récente d'abord, sans doublon ni dépassement de la borne | petit |
| A2 | **Boucle A-B : nombre de boucles et délai** | `Dialog(453)` | `loop_a_ms` / `loop_b_ms` dans `player.rs` ; 2 champs `playback.*` | N boucles puis arrêt (retours comptés au journal **et** par la position), délai mesuré entre deux passages | petit |
| A3 | **Exporter / importer la configuration** | 2096 | bouton dans les préférences + `--export-config` / `--import-config` en ligne de commande | aller-retour : exporter, modifier, importer → fichier identique octet pour octet ; fichier invalide refusé sans écraser | petit |

## Lot 2 — style des sous-titres (même méthode que `verif-soustitres.sh` : mesure sur les pixels)

| # | Fonction | PotPlayer | Où | Effort |
|---|---|---|---|---|
| B1 | **Marges haut et bas** réglables (aujourd'hui 12 px fixes + position en %) | marges, 2079-2081 | `subtitles.margin_top/bottom_percent`, à côté de `margin_left/right_percent` | petit |
| B2 | **Interligne** réglable (aujourd'hui 1,25 × la taille, fixe) | interligne | `subtitles.line_spacing` | petit |
| B3 | **Second contour** et **ombre** (couleur, transparence, décalage) | `Dialog` 383 | `subtitles.outline2_*`, `shadow_*` ; rendu dans `ui.rs` | moyen |
| B4 | **Échelle X / Y** du texte | style police | `subtitles.scale_x/y` | moyen |
| B5 | **Fondu** d'apparition et de disparition | fondu | `subtitles.fade_ms` ; alpha par image | petit |
| B6 | **Langues préférées avec priorité**, durées mini/maxi d'affichage, **piste mémorisée** | 2079 | `subtitles.preferred_languages`, `min_duration_ms`, `max_duration_ms` ; seul `default_language` existe | moyen |

## Lot 3 — vidéo par libavfilter (même méthode que `verif-filtres.sh`)

| # | Fonction | Où | Effort |
|---|---|---|---|
| C1 | **Matrice de couleur** (BT.601 / 709 / 2020) et **plage** 16-235 / 0-255 choisies à la main, au lieu de l'automatique | `scale=in_color_matrix=…:in_range=…` dans `filter_description` (`player.rs`) | petit |
| C2 | **Glisser** (décalage de l'image) | `crop` + `pad` dans `filter_description` | petit |
| C3 | **Étendre à l'écran / rogner** (aujourd'hui : recadrage seulement) | cadrage dans `player.rs` / `ui.rs` | moyen |
| C4 | **Plein écran : autres moniteurs en noir** | fenêtres noires, `main.rs` / `x11.rs` | moyen (piège Wayland : pas de positionnement client) |

## Lot 4 — fonction PotPlayer hors du catalogue des réglages

| # | Fonction | Où | Effort |
|---|---|---|---|
| D1 | **Marque-pages (signets)** : ajouter / aller à / supprimer, par média, persistés | `positions.rs` comme modèle de mécanisme (JSON relu au chargement, écrit à la fermeture) ; raccourci + menu | moyen |

Confirmé absent dans le code : aucun terme `bookmark`, `signet` ou `favori` dans `src/`.
**Deux différences avec `positions.rs`** à ne pas copier : il stocke **une** position par
média (`HashMap<String, u64>`), alors qu'un signet est une **liste** par média ; et il écrit dans
`~/.cache/kplayer/`, un cache qu'on peut effacer sans perdre de réglage — des signets
doivent vivre avec la configuration, pas dans le cache.

## Lot 5 — système et intégration (valeur plus faible)

| # | Fonction | Remarque | Effort |
|---|---|---|---|
| E1 | **Souris : actions avec Ctrl / Maj / Alt** | PotPlayer `Dialog(458)` ; aujourd'hui molette = volume / seek, clic milieu, double-clic | moyen |
| E2 | **Choix manuel du décodeur** | `Dialog(423)` ; utile surtout pour forcer le logiciel quand VA-API se comporte mal | moyen |
| E3 | **Icône de notification** (StatusNotifier) | `ksni` / D-Bus | moyen |
| E4 | **Démarrage automatique** | un `.desktop` dans `~/.config/autostart` | petit |
| E5 | **Priorité du processus** | `setpriority` ; utile en 4K seulement | petit |
| E6 | **Réduire la fréquence d'image** | usage marginal | petit |

## Lot 6 — conditionnel, à décider (pas de recommandation de les faire)

| # | Sujet | Pourquoi c'est à part |
|---|---|---|
| F1 | ~~**Repli HDR automatique vers le moteur CPU**~~ | **fait (s.36), borné** : l'étage CPU remplace l'étage GPU refusé si la source tient dans 62 Mpx/s (≤ 1080p30) ; au-delà, image non convertie et avis à l'écran. Budget et mesures dans `docs/HDR.md` ; `verif-hdr` §12 (36/36). Corrigé au passage : l'échec de Vulkan était rejoué à chaque image dès qu'un autre filtre était actif |
| F2 | **Passthrough HDR / sortie 10 bits** (⑪) | bloqué par winit (aucune API de couleur) ; inatteignable ici : 3 écrans, tous SDR |
| F3 | **Langue d'interface** (fluent / `.po`) | valeur faible pour un usage francophone |
| F4 | **Thèmes externes** | aujourd'hui un thème sombre intégré |
| F5 | **WarpSharp**, **mouvement flou**, **contour flou**, **ruby**, **alphabet** | plugin absent du FFmpeg compilé, ou coûteux, ou usage très marginal en France |
| F6 | **Téléchargement de sous-titres en ligne**, **enregistrement / transcodage** | aucun code ; l'enregistrement est déclaré hors périmètre dans `INVENTAIRE-FONCTIONNALITES.md` |

---

## Ordre conseillé, et pourquoi

1. **A1, A2, A3** : petits, visibles à l'usage, et **testables en ligne de commande** (pas de
   clic nécessaire) — donc rapides à prouver. Ils servent aussi à rôder la recette sur un
   terrain sans risque.
2. **B1 + B2 ensemble** (marges haut/bas et interligne) : même mécanisme que les marges
   latérales et l'interlettre faits en session 34, donc le chemin est connu ; puis B5.
3. **D1 (marque-pages)** : la seule fonction « de tous les jours » de PotPlayer qui manque
   entièrement, et `positions.rs` donne le modèle de stockage.
4. **C1 puis C2** : deux lignes de filtre chacune, mais C1 touche la couleur — à mesurer avec
   les mires déjà utilisées par `verif-hdr.sh`.
5. **B3, B4, B6, C3, C4** : plus longs ; à prendre quand les précédents sont rangés.
6. **Lots 5 et 6** : seulement sur un besoin réel. F1 (repli HDR) est fait ; il reste F2 à F6,
   qui dépendent de matériel ou de choix de périmètre.

## Ce que ce document ne couvre pas

- **Les menus de PotPlayer** (lecture, navigation, outils, gestion de la liste) n'ont pas été
  parcourus : seul le catalogue des réglages l'a été. D1 est la seule fonction hors catalogue
  identifiée ; il y en a sans doute d'autres, qu'un relevé des menus de PotPlayer ferait
  apparaître.
- **La qualité de rendu** de chaque fonction, par rapport à PotPlayer, n'est pas comparée ; seul
  l'effet mesuré (pixels, son, position) l'est.
- **Les efforts** sont des ordres de grandeur, pas des engagements.

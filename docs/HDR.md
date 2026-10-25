# HDR — ce qui est fait, ce qui manque, et pourquoi

*Document de référence de la session 35. Complète `§6.45` de `AVANCEMENT.md`.*

## En une phrase

**Le traitement du HDR est complet et mesuré ; sa *sortie* ne l'est pas.** RMPlayer convertit
correctement HDR10 (PQ) et HLG vers un écran SDR, en le détectant par l'EDID ; il ne peut pas
encore **afficher** du HDR, parce que la bibliothèque de fenêtrage ne sait pas produire autre
chose que du RGBA 8 bits.

## Les trois capacités, à ne pas confondre

| Capacité | Où elle se lit | Sur ce poste (2026-10-21) |
|---|---|---|
| **L'écran** est HDR | son **EDID**, `/sys/class/drm/<connecteur>/edid` | **non** — DP-3, DP-2 et HDMI-A-1 n'annoncent ni PQ/HLG ni BT.2020 (une extension CEA-861 sans bloc Colorimetry ni HDR Static Metadata) |
| **Le compositeur** sait transporter des couleurs étendues | globaux Wayland | **oui** — `wp_color_manager_v1` v3 et `zwp_linux_dmabuf_v1` v6 avec `ABGR2101010` |
| **Notre fenêtre** sait en produire | API de winit / eframe | **non** — winit 0.30 n'a **aucune** API de couleur (pas de choix d'espace, pas de format 10 bits) ; egui/eframe n'a qu'un réglage de tramage |

Un passthrough HDR exige **les trois**. Deux sur trois sont réunies ; la troisième est une
limite de bibliothèque, pas de matériel.

## Ce qui est branché aujourd'hui

`src/edid.rs` lit l'EDID de **chaque écran allumé** et en extrait PQ, HLG, BT.2020, métadonnées
statiques et luminance maximale. `src/hdr.rs` en déduit un `Contexte` :

- mode **Auto** + flux HDR + **tous** les écrans HDR + sortie capable → **passthrough** (aucun
  étage, les métadonnées HDR restent sur les images) ;
- mode **Auto** + flux HDR + un écran SDR **ou** sortie SDR → conversion tonale, et
  l'explication nomme la cause (« écran SDR (EDID) », ou « écran HDR détecté, mais … ») ;
- modes **Toujours** / **Jamais** : forcés, comme avant.

**Pourquoi « tous les écrans »** : Wayland ne dit pas au client sur quel écran sa fenêtre est
affichée. Avec un HDR et un SDR branchés, envoyer du HDR reviendrait à parier — le passthrough
est donc refusé, et le résumé l'indique (« au moins un HDR, mais pas tous »).

Le contexte est relu à **chaque** recalcul de la chaîne : brancher un écran HDR pendant la
lecture change la décision sans redémarrer.

## Ce qu'il faudrait pour le passthrough

1. **Une surface 10 bits** : `Rgb10a2`/`Abgr2101010` dans `wl_surface` via `zwp_linux_dmabuf_v1`,
   ou un format de swapchain Vulkan 10 bits. Cela suppose une API dans winit (ou un rendu
   Vulkan direct, sans passer par la texture RGBA d'egui).
2. **Un espace colorimétrique déclaré** : `wp_color_manager_v1` (`set_primaries`,
   `set_transfer_function` en PQ/HLG, `set_luminances`) sur la surface. Aucune de ces fonctions
   n'est exposée par winit 0.30.
3. **Le chemin de conversion désactivé** : c'est déjà prêt (`Moteur::Aucun`, mode « jamais »,
   métadonnées d'image conservées).

Le jour où (1) et (2) existeront, il suffira de renseigner `Contexte::sortie_hdr`, et le
branchement déjà testé fera le reste.

## Quand Vulkan est inutilisable (mesuré, `verif-hdr` section 12)

Si `libplacebo` est refusé à l'exécution (pilote absent, session distante, mémoire GPU, mise à
jour de Mesa qui casse Vulkan), la lecture ne s'arrête jamais. Ce qui se passe dépend de la
**source**, car le moteur CPU a un coût mesuré (voir plus bas) :

| Source | Comportement | Journal | Avis à l'écran |
|---|---|---|---|
| **dans le budget** (≤ 1080p30, 720p60, plus petit) | l'étage CPU (`zscale` + `tonemap`) **remplace** l'étage GPU : l'image est convertie | « Étage HDR refusé (…) — repli CPU : … » | « Vulkan indisponible, conversion par le processeur (qualité réduite) » |
| **au-delà** (1080p60, 1440p, 4K) | **aucune conversion** : l'image HDR est jouée telle quelle, donc délavée sur un écran SDR (mesuré : bandes à 61 / 98 / 190 au lieu de 47 / 107 / 253) | « Étage HDR : repli CPU refusé (source 3840x2160 à 25 i/s = 207 Mpx/s, au-delà des 62 Mpx/s …) » | « Vulkan indisponible et source trop lourde pour le processeur — image non convertie (…) » |

**Le budget** est `hdr::BUDGET_REPLI_CPU_MPX_S` = 1920 × 1080 × 30 ≈ 62 Mpx/s, soit environ 26 %
de la capacité CPU mesurée : le reste du processeur reste au décodage, à l'audio et à
l'interface. Pourquoi refuser plutôt que tenter : une conversion saccadée en 4K est pire à
regarder qu'une image fluide mais délavée.

**Pourquoi pas « réduire d'abord, convertir ensuite »** (mesuré : 4K → 1080p avant le CPU passe
de 28 à 77 i/s) : l'étage HDR est volontairement **en tête, sur les bits d'origine**, et le
recadrage, le zoom et la rotation viennent après ; réduire avant décalerait ces réglages.

**Défaut corrigé en même temps** (session 36) : avec Vulkan coupé et un **autre filtre actif**
(contraste, etc.), le lecteur retentait Vulkan, échouait et reconstruisait le graphe à **chaque
image** (20 refus en 5 s à 5 i/s ; 24 à 60 par seconde en lecture normale). La cause : le graphe
de repli portait la signature de la chaîne réduite, pas celle demandée. `verif-hdr` §12c le
vérifie sur une vraie lecture.

Le moteur CPU peut toujours être **forcé** (`RMPLAYER_HDR_MOTEUR=cpu`) sans condition de
budget : c'est un outil de diagnostic.

### Ce que coûte le moteur CPU (mesuré le 2026-10-24) — origine du budget

Poste : Ryzen 7 5800X (8 cœurs / 16 fils), AMD RX 6700 XT (Vulkan RADV), Mesa 25.3. Chaînes
exactes de `hdr.rs`, appliquées par `ffmpeg` (FFmpeg 7.1.3 de `/opt`) à 288 images d'une
mire PQ/BT.2020 10 bits, sortie jetée. **Source synthétique, sans décodage, sans sous-titres,
sans affichage** : ordre de grandeur, pas un test de lecture réelle.

| Résolution | Source seule | GPU `libplacebo` | CPU `zscale` + `tonemap` |
|---|---|---|---|
| 3840×2160 | 224 i/s | **85 i/s**, 0,5 cœur | **26 i/s**, 9,3 cœurs |
| 1920×1080 | 951 i/s | **236 i/s**, 0,5 cœur | **114 i/s**, 10,2 cœurs |

Lecture : en 4K, le CPU n'a que 8 % de marge sur du 24 i/s (aucune sur du 60) et occupe
presque tout le processeur — auquel il faudrait ajouter le décodage, l'audio et l'interface.
En 1080p il tient avec de la marge. Le moteur CPU est aussi **moins fidèle** : opérateur Hable,
crête fixée à 1000 nits, pas de détection de crête dynamique.

**Un Vulkan logiciel est installé** (`lavapipe` / `llvmpipe`, vu par `vulkaninfo`). Il ne sert
pas de secours : avec lui seul, `libplacebo` répond « Found no suitable device » et refuse
(donc même chemin que le Vulkan coupé de la section 12), il ne tourne pas lentement sur CPU.

## Vérifier

```bash
tools/verif-hdr.sh          # 36 contrôles, mires calibrées en nits, VA-API vs logiciel, EDID, Vulkan coupé
tools/sante-hdr.sh         # ≈ 10 s : le GPU sert-il encore au HDR ? À lancer après une mise à jour de Mesa / pilote / noyau
tools/etat-court.sh         # affiche « HDR (libplacebo) oui/non »
rmplayer --hdr-info film.mkv # métadonnées du flux + écrans lus + décision + explication
```

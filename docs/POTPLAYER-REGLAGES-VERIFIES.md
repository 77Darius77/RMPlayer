# PotPlayer — arborescence des réglages, **vérifiée sur l'installation**

Ce document remplace les intitulés « de mémoire » du premier jet de
`COMPARAISON-POTPLAYER.md`. Tout ce qui suit est **relevé dans les fichiers de
l'installation**, pas dans une documentation en ligne.

## Provenance (vérifiable)

| Élément | Valeur |
|---|---|
| Installation | `~/.wine` de test → `drive_c/Program Files/DAUM/PotPlayer/` (préfixe `wine-test-pot`) |
| Version du programme | **`ProgramVersion = v260819`** (écrit par l'application elle-même dans `Software\DAUM\PotPlayer64`) |
| Fichier des libellés français | `Language/French.ini` — 5 640 lignes, SHA-256 `a7722004…9f99056` |
| Fichier des libellés anglais | `Language/English.ini` — 5 662 lignes, SHA-256 `e65847a6…cdbecd2` |
| Options de ligne de commande | `CmdLine64.txt` — 20 langues dont le **français**, SHA-256 `155b6fd3…a33a9870` |
| Binaire principal | `PotPlayerMini64.exe`, SHA-256 `6f0f63be…457a2e1cb7` |
| Bibliothèque | `PotPlayer64.dll`, SHA-256 `157feac1…1fad78d` (30 030 352 octets) |

**Deux constats qui expliquent la méthode retenue :**

1. **Le binaire est protégé par Themida** (section `.themida` présente) et sa ressource
   `VERSIONINFO` est vidée (`0.0.0.0`). Extraire des chaînes *du binaire* donne donc du
   bruit de packer, pas les libellés de l'interface.
2. Les libellés affichés **sont** dans `Language/French.ini` : c'est le même texte que
   l'interface charge. C'est donc la meilleure surface de vérification, et elle est
   reproductible (digest ci-dessus).

**Apport de REA — corrigé après vérification.** `inspect_pe_resources` (fournisseur
`rea-pe-resources`, sans moteur) a confirmé la structure PE et l'inventaire des ressources
de `PotPlayer64.dll` (`pe32-plus`, machine 34404) — preuve `ev_9baa890c579ee3…`.

J'avais d'abord écrit que « la machine n'a que le JDK 11 » : **c'était faux**. Ghidra
**12.1.4** et un **JDK 21** sont bien installés sur le disque du projet
(`/media/blaise/LINUX_DATA/ghidra_12.1.4_PUBLIC`, `/media/blaise/LINUX_DATA/jdk-21.0.12.1+1`),
et `~/.bashrc` les exporte déjà. Le problème était ailleurs : **le serveur REA en cours
d'exécution** (processus fils de `ajean web`) avait été lancé **sans** ces deux variables —
son environnement ne contenait ni `GHIDRA_INSTALL_DIR` ni `JAVA_HOME`, d'où le repli sur le
`java` du système (`/usr/lib/jvm/java-11-openjdk-amd64`) et le refus
`provider_unavailable`.

**Vérifications faites depuis**, toutes reproductibles :

| Commande | Résultat |
|---|---|
| `GHIDRA_INSTALL_DIR=… JAVA_HOME=… rea doctor --json` | **tous** les contrôles Ghidra verts : installation, version **12.1.4**, `support/analyzeHeadless`, décompileur natif `linux_x86_64`, **java 21.0.12.1 64 bits accepté** |
| `rea search …/windows/notepad.exe "Wine"` | fonctionne : `0x140008256 = "Wine Notepad"` (fournisseur **ghidra 12.1.4**, preuve `ev_452a9170…`) |
| `rea search …/PotPlayer64.dll "Color"` | **`provider_timeout`** : analyse > 330 s — la DLL pèse 30 Mo et est **packée Themida** |

**Correctif — et sa correction.** Premier emplacement tenté :
`~/.config/environment.d/50-rea-ghidra.conf`. **Insuffisant** : `ajean web` n'est pas une
application de session mais un **service systemd système**
(`/etc/systemd/system/ajean-ui.service`), qui ne lit pas `environment.d` (réservé à la
session utilisateur). Le bon endroit est un *drop-in* sur l'unité :

```
/etc/systemd/system/ajean-ui.service.d/rea-ghidra.conf
  [Service]
  Environment=GHIDRA_INSTALL_DIR=/media/blaise/LINUX_DATA/ghidra_12.1.4_PUBLIC
  Environment=JAVA_HOME=/media/blaise/LINUX_DATA/jdk-21.0.12.1+1
```

puis `systemctl daemon-reload` et redémarrage du service.

**État vérifié après redémarrage** (processus 1730067, démarré à 21:37) : les deux
variables sont dans `/proc/<pid>/environ` ; `binary_session` annonce ghidra
**`status: available`**, `provider_version 12.1.4`, `java_home …/jdk-21.0.12.1+1`,
`java_version 21.0.12.1` ; et l'appel réel, **passé par le MCP** :
`open_binary(notepad.exe)` → `search_strings("Wine")` →
**`0x140008256 = "Wine Notepad"`** (preuve `ev_452a9170…`, autorité `shipped-artifact`).
Les outils de chaînes et de décompilation sont donc opérationnels depuis l'agent.

**Conséquence pour ce document** : les libellés PotPlayer ont été relevés dans
`Language/French.ini` **par lecture de fichier**, pas par REA. Cela reste la bonne
méthode, pour deux raisons mesurées : le binaire est **packé Themida** (les chaînes du
code sont chiffrées au repos) et son analyse dépasse le budget de REA
(`provider_timeout` à 330 s pour 30 Mo). Maintenant que Ghidra fonctionne, une piste
reste ouverte : `inspect_managed_members` ou `inspect_pe_resources` sur les DLL **non
packées** (`ATextOut64.dll`, `KakaoUtil64.dll`, `ffcodec64.dll`) — mais les libellés
d'interface ne sont pas là.

## Arborescence de la fenêtre « Préférences » (F5)

Libellés exacts, dans l'ordre où ils sont définis (identifiants `2055`→`2098` de
`French.ini`). L'ordre est vérifié ; le **regroupement** ci-dessous suit la hiérarchie
à deux niveaux de la fenêtre et les pages retrouvées par leur contenu (identifiants de
dialogue cités entre parenthèses).

- **Général** — 2055
  - Clavier (2056) · Souris (2057) · Thèmes (2058) · Logo (2059) · Visualisation (2060)
  - Navigation (2061) · Messages OSD (2062) · **Liste de lecture** (2063) · **Lecture** (2064)
  - Avancé (2065) · Saut temporel (2066)
  - **Capture d'écran** (2068)
  - Association (2095) · Configuration (2096)
  - *Page « Réglages généraux » relevée en `Dialog(354)`* : « Langue d'interface », « Instances
    multiples », « Stocker réglages dans fichier .ini », « Suspendre la lecture à la perte de
    focus », « Suspendre la lecture si écran verrouillé », « Taille fenêtre par défaut à la
    fermeture fichier », « Réglages généraux », « Auto-vérif. mises à jour »…
- **Périphérique** — 2067
  - Webcam (2069) · TV analogique (WDM) (2070) · TV numérique (BDA) (2071) · Canaux TV (2072)
  - DVD/Blu-ray (2073) · Util périph capture audio (2098)
- **Contrôle du filtre** — 2074
  - Source/Séparateur (2075) · Décodeurs vidéo (2076) · Décodeurs audio (2077) · Priorité (globale) (2078)
  - *`Dialog(325)` liste ces pages : « Source/Séparateur | Décodeurs vidéo | Décodeurs audio |
    Filtre | Priorité filtre »*
- **Sous-titres** — 2079
  - **Style police** (2080) · **Fondu/3D/Emplacement** (2081)
- **Vidéo** — 2082
  - **Colorimétrie** (2083) · **Redimensionner** (2084) · **Etendre/Rogner** (2085)
  - **Désentrelacement** (2086) · **Niveaux/Décalage** (2087) · **Effets** (2088)
- **Audio** — 2089
  - Contrôle du volume (2090) · **Egaliseur** (2091) · **Normaliseur/Freeverb** (2092)
  - **Cristallité/Réduction bruit** (2093) · Plugins Winamp DSP (2094)
- **Kakao TV** — 2097

> Ce qui n'existe **pas** dans cette arborescence, contrairement à ce que laissait croire
> mon premier jet : pas de grande catégorie « Lecture » de premier niveau (elle est sous
> **Général**), pas de catégorie « Capture » (c'est **Général › Capture d'écran**), et le
> contrôle des filtres s'appelle **« Contrôle du filtre »** avec quatre pages seulement.

## Page par page : libellés réellement présents

Relevés dans les blocs `[Dialog(N)]` de `French.ini`.

### Vidéo › Colorimétrie — `Dialog(384)`

`Luminosité :` · `Contraste :` · `Saturation :` · **`Teinte :`** · `Table YCbCr <-> RGB :` ·
`Palette YCbCr :` · `Palette YCbCr en RGB :` · `Colorimétrie de sortie :` ·
`Conversion directe :` · `Contrôles de couleur` · `Utiliser les contrôles de couleur
matériels (si possible)` · `Conversion YUY2/RGB24/32 de haute qualité` · `Conversion en
multitâches`.

### Vidéo › Désentrelacement — `Dialog(385)`

`Désentrelacement matériel` / `Désentrelacement logiciel` · `Méthode :` · `Ordre champ :` ·
`Réglages de désentrelacement` · `Mode de désentrelacement :` · `Ordre par défaut, si
forcé :` · `Permuter les champs (haut <-> bas) :` · `Vidéos marquées entrelacées :` ·
`Vidéos non marquées entrelacées :` — et les valeurs les plus bas : `Auto`, `Forcer le
mode WEAVE (pas désentrelacer)`, `Forcer le mode BOB (désentrelacer)`, `Champ du haut en
premier`, `Champ du bas en premier`.

### Vidéo › Effets — `Dialog(386)`

`Retourner verticalement` · `Retourner horizontalement` · `270 degrés` · `Adoucir - Rayon /
Luminance / Chrominance` · `Netteté - Luminance / Chrominance` · `Débruiter 3D - Luminance /
Chrominance / Temps` · `Débruitage graduel` · `WarpSharp - Profondeur / Seuil` ·
`**Débander - Seuil / Rayon**` · `Mouvement flou` · `Traitement vidéo en multitâches`.

### Vidéo › Redimensionner / Etendre/Rogner — `Dialog(456)` et chaînes 2099-2116

Algorithmes : `Bilinéaire rapide`, `Bilinéaire`, `Bicubique`, `Gaussien`, **`Lanczos`**,
`Spline` ; corrections : `Pas de correction de Format`, `Garder le format d'image
original`, `Etendre l'image en format`, `Etendre l'image en pourcentage` ; `Etendre
l'image vidéo à :`, `Résolution :`, `Affichage :`, `Scinder l'image vidéo`.

### Vidéo › Rendu vidéo (sous Vidéo, atteint par `Dialog(355)`)

`Rendu vidéo :` · `Présentation Direct3D :` · `Périph. rendu Direct3D :` · `Format de la
surface :` · `Surface VMR :` · `Tampon :` · `Plein écran exclusif :` · `Sortie 10-bits` ·
`Palette de couleurs limitée (16~235)` · `Mode de sortie HW HDR (Imagerie à grande gamme
dynamique)` · `D3D11 GPU Super Résolution` · `D3D11 GPU RTX Vidéo HDR` · `Activer la
compatibilité flip` · `Utiliser le calcul de temps Vsync` / `nouvelle méthode Vsync` ·
`Ne pas attendre la synchro verticale` · `Sous-titres 3D : pas de correction de profondeur`.

### Audio › Egaliseur — `Dialog(363)`

`Egaliseur` · `Activer` · `Préréglage` · `SuperEQ` · bandes affichées : `60`, `170`, `310`,
`600`, `1K`, `3K`, `6K`, `12K`, `14K`, `16K` · `0 dB`.

### Audio › Normaliseur/Freeverb — `Dialog(387)`

`Normaliseur (contrôle automatique du volume)` · `Renforcer l'audio` · `Niveau (%d%%) :` ·
`Activer (pour diffusion)` · `Fonctionnel uniquement en la lecture vidéo` ·
`Freeverb (réverbération stéréo)` · `Taille de la salle (%g) :` · `Etouffement (%g) :` ·
`Niveau "Wet" / "Dry"` · `Largeur (%g) :` · `Méthode :` · `Durée du relâchement (%d) :` ·
`* Ne fonctionne que sur 2 canaux.`

### Sous-titres › Style police — `Dialog(383)`

`Police` (×2) · `Taille :` · `Alphabet :` · `Interlettre :` · `Texte :` · `Contour :` ·
`Ombre :` · `2nd contour :` · `Contour flou :` · `Contour flou gaussien :` · `Echelle X :` /
`Echelle Y :` · `Transparence` · `Boîte opaque` · `Encadrer l'arrière-plan (1ère couleur
contour)` · `2nd contour à échelle` · `Marge haut / gauche / droite / bas` · `Interligne :` ·
`Position hori. (%) :` / `Position verti. (%) :` · `Alignement :` · `Position "Ruby"` ·
`Activer` · `Limiter à l'alphabet Coréen`.

### Général › Souris — `Dialog(458)`

`Assigner une opération en combinant "Ctrl/Maj/Alt" à l'action souris visée` ·
`Action souris :` · `Ctrl + action souris :` · `Maj + action souris :` ·
`Alt + action souris :` · `Ctrl + Alt + …` · `Ctrl + Maj + …` · `Maj + Alt + …`.

### Général › Saut temporel / répétition A-B — `Dialog(453)` et `Dialog(503)`

`Régler répétition A-B` · `Régler Point A :` · `Régler Point B :` · `Nb boucles :` ·
`Délai de répétition (ms)` · `Délai à la fin de la répétition` · `Répéter la section
suivante une fois terminé` · `Répétition de "section sous-titre"`.

---

## Conséquence pour la comparaison

| Catégorie réelle (vérifiée) | KPlayer | Écart principal |
|---|---|---|
| **Général** (Clavier, Souris, Thèmes, Logo, Visualisation, Navigation, OSD, Liste de lecture, Lecture, Avancé, Saut temporel, Capture d'écran, Association, Configuration) | 🟡 réparti dans *Préférences ▸ Interface / Lecture / Raccourcis* + capture PNG | Pas d'équivalent de **Thèmes/Logo/Visualisation**, ni **Capture d'écran** comme page riche, ni **Association**, ni **Configuration** (export/import global) |
| **Périphérique** (Webcam, TV WDM, TV BDA, Canaux TV, DVD/Blu-ray, Util périph capture audio) | 🟡 disques montés + URL ; pas de tuner ni de webcam | Tout le bloc TV/capture est sans objet ici, sauf DVD/Blu-ray |
| **Contrôle du filtre** (Source/Séparateur, Décodeurs vidéo, Décodeurs audio, Priorité globale) | 🟡 chaîne FFmpeg interne, choix global matériel/logiciel | Pas de choix **par codec**, pas de filtres externes |
| **Sous-titres** (Style police, Fondu/3D/Emplacement) | ✅ style complet côté texte/image ; ❌ **pas de choix de famille de police** | La page `Dialog(383)` montre l'ampleur : police, interligne, position X **et** Y, Ruby, contour flou, second contour |
| **Vidéo** (Colorimétrie, Redimensionner, Etendre/Rogner, Désentrelacement, Niveaux/Décalage, Effets) | ✅ filtres équivalents (netteté, débruitage, débruitage 3D ≈ `hqdn3d`, débandage, rotation, miroirs, cadrage, aspect, désentrelacement) | Manquent **Teinte**, **WarpSharp**, **mouvement flou**, choix de l'algorithme de rendu, **10 bits**, HDR, `Résolution`/`Plein écran exclusif` |
| **Audio** (Contrôle du volume, Egaliseur, Normaliseur/Freeverb, Cristallité/Réduction bruit, Plugins Winamp DSP) | ✅ volume plafonné, égaliseur 18 bandes, normaliseur, réverbération, anti-bruit, stéréo, karaoké | Pas de **SuperEQ** paramétrique, pas de greffons DSP externes, pas de « Renforcer l'audio » |
| **Kakao TV** | ➖ (service tiers) | — |

**Trois corrections à retenir** par rapport à mon premier jet : (1) « Lecture » est une
sous-page de **Général**, pas une catégorie ; (2) il n'y a **pas** de catégorie « Capture »,
mais **Général › Capture d'écran** ; (3) le bloc filtres s'appelle **Contrôle du filtre**
et n'a que quatre pages — le reste (désentrelacement, effets) est sous **Vidéo**.

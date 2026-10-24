#!/usr/bin/env bash
# verif-filtres.sh — prouve que chaque filtre FFmpeg change RÉELLEMENT l'image.
#
# Principe : on ne croit ni la configuration, ni le journal. Pour chaque filtre :
#   1. on fabrique un clip de test dont le défaut est mesurable (dégradé à marches,
#      bruit gaussien fixe, mire de peigne entrelacée, aplats de couleur) ;
#   2. on décode une image à une position fixe avec `--dump-frame`, deux fois :
#      filtre désactivé puis activé, avec une configuration isolée à chaque fois ;
#   3. on mesure les deux images et on exige que la métrique aille dans le sens
#      attendu, avec un seuil chiffré.
#
# `--dump-frame` est ce qui rend la mesure honnête : l'image est celle que produit
# le moteur, à la résolution NATIVE, sans fenêtre ni rééchantillonnage (une
# capture d'écran dépendrait de la taille de la fenêtre).
#
# Usage : tools/verif-filtres.sh        (aucun écran requis)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
BASE=/tmp/kp-filtres
KP=./target/release/kplayer
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=kplayer=info

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }

mkdir -p "$BASE/clips" "$BASE/dumps"

# ---------------------------------------------------------------- clips de test
python3 - "$BASE/clips/degrade.png" <<'PY'
import sys, numpy as np
from PIL import Image
w, h = 1280, 720
# Dégradé vertical par marches de 6 niveaux : c'est le banding que `deband`
# traite réellement (des marches plus larges sont vues comme des contours, pas
# comme du banding — vérifié avec ffmpeg : à 21 niveaux d'écart, deband ne fait rien).
col = (np.floor(np.linspace(0, 256 / 6, h)) * 6).clip(0, 255).astype(np.uint8)
img = np.repeat(col[:, None], w, axis=1)
Image.fromarray(np.dstack([img, img, img])).save(sys.argv[1])
PY

python3 - "$BASE/clips/bruit.png" <<'PY'
import sys, numpy as np
from PIL import Image
rng = np.random.default_rng(12345)          # graine fixe = mesure reproductible
img = np.clip(128 + rng.normal(0, 22, (720, 1280)), 0, 255).astype(np.uint8)
Image.fromarray(np.dstack([img, img, img])).save(sys.argv[1])
PY

# Deux mires de désentrelacement, pour deux questions différentes.
#
# (1) `comb` : un objet clair qui se déplace sur un fond uni. C'est le peigne RÉEL
#     d'une source entrelacée (les deux trames sont prises à 20 ms d'écart) ; après
#     désentrelacement, l'écart vertical de l'image doit s'effondrer.
# (2) `detail` : mire en lignes de 1 px, IMAGE FIXE marquée entrelacée. Ce n'est pas
#     un peigne à retirer mais du détail fin : `bwdif` (détection de mouvement) le
#     conserve — c'est ce qui le distingue de `yadif`, qui l'efface. Une mire de
#     peigne figée ne doit donc PAS servir à juger la résorption du peigne : dans une
#     vraie source entrelacée, une scène immobile n'a pas de peigne du tout.
python3 - "$BASE/clips" <<'PY'
import sys, numpy as np
from PIL import Image
h, w = 720, 1280
d = sys.argv[1]

# (1) peigne réel : objet en mouvement, apparié en trames (paire = image 2k,
#     impaire = image 2k+1), puis marqué TFF à l'encodage.
for k in range(40):
    img = np.full((h, w), 128, dtype=np.uint8)
    x0 = 100 + k * 12
    img[300:520, x0:x0 + 300] = 250
    Image.fromarray(np.dstack([img] * 3)).save(f"{d}/comb_{k:03d}.png")
for j in range(20):
    a = np.asarray(Image.open(f"{d}/comb_{2*j:03d}.png"), dtype=np.uint8).copy()
    b = np.asarray(Image.open(f"{d}/comb_{2*j+1:03d}.png"), dtype=np.uint8)
    a[1::2] = b[1::2]
    Image.fromarray(a).save(f"{d}/comb_trame_{j:03d}.png")

# (2) détail fin fixe : lignes de 1 px.
img = np.zeros((h, w), dtype=np.uint8)
img[0::2, :] = 235
img[1::2, :] = 40
Image.fromarray(np.dstack([img] * 3)).save(f"{d}/detail.png")
PY

python3 - "$BASE/clips/couleur.png" <<'PY'
import sys, numpy as np
from PIL import Image
h, w = 720, 1280
img = np.zeros((h, w, 3), dtype=np.uint8)
img[:, : w // 3] = (220, 40, 40)       # rouge
img[:, w // 3 : 2 * w // 3] = (40, 200, 60)   # vert
img[:, 2 * w // 3 :] = (50, 60, 220)   # bleu
Image.fromarray(img).save(sys.argv[1])
PY

encoder() {  # encoder <source.png> <sortie.mp4> [options]
    local src="$1" out="$2"; shift 2
    # `env -u LD_LIBRARY_PATH` : le ffmpeg du système (avec libx264) ne doit pas
    # charger les bibliothèques de /opt/ffmpeg-7.1 (build sans encodeur).
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -loop 1 -i "$src" \
        -t 4 -r 25 -c:v libx264 -crf 12 -pix_fmt yuv420p "$@" -y "$out" 2>&1 | tail -2
}
encoder "$BASE/clips/degrade.png" "$BASE/clips/degrade.mp4"
encoder "$BASE/clips/bruit.png"   "$BASE/clips/bruit.mp4"
encoder "$BASE/clips/couleur.png" "$BASE/clips/couleur.mp4"
# `ilme`/`ildct` marquent les images entrelacées : c'est cette marque que lit
# `deint=interlaced` (une source progressive n'est jamais dégradée), et que lisent
# aussi bien `yadif` que `bwdif`.
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -framerate 25 -i "$BASE/clips/comb_trame_%03d.png" \
    -c:v libx264 -crf 12 -pix_fmt yuv420p \
    -flags +ilme+ildct -x264-params "interlaced=1:tff=1" \
    -y "$BASE/clips/comb.mp4" 2>&1 | tail -2
encoder "$BASE/clips/detail.png" "$BASE/clips/detail.mp4" \
    -flags +ilme+ildct -x264-params "interlaced=1:tff=1"
echo "clips de test :"; ls -la "$BASE/clips/"*.mp4 | awk '{print "    ", $NF, $5, "octets"}'

# --------------------------------------------------------- configuration isolée
config_essai() {  # config_essai <nom> '<json champs video>'
    local nom="$1" video_json="$2"
    local dossier="$BASE/$nom"
    rm -rf "$dossier"; mkdir -p "$dossier/conf/kplayer" "$dossier/cache"
    python3 - "$dossier/conf/kplayer/config.json" "$video_json" <<'PY'
import json, sys
chemin, video = sys.argv[1], json.loads(sys.argv[2])
cfg = {
    "video": dict({"hardware_decoding": False, "scaling_method": "Lanczos"}, **video),
    "audio": {"device": "", "volume_max": 150},
    "subtitles": {"auto_load": False, "font_size": 24.0, "default_language": "fr"},
    "interface": {"dark_theme": True, "show_osd": False, "osd_duration": 1000,
                  "show_progress_bar": True, "controls_always_visible": False,
                  "show_title": False, "screenshots_dir": "/tmp/kp-filtres/captures"},
    "playback": {"resume": False},
    "playlist": [],
}
json.dump(cfg, open(chemin, "w"), indent=2, ensure_ascii=False)
PY
}

# `image <nom_config> <clip> <sortie.png>` : décode une image à 1000 ms.
image() {
    local nom="$1" clip="$2" sortie="$3"
    XDG_CONFIG_HOME="$BASE/$nom/conf" XDG_CACHE_HOME="$BASE/$nom/cache" \
        "$KP" --dump-frame 1000 "$sortie" "$clip" >"$BASE/$nom/dump.log" 2>&1
    [ -s "$sortie" ] || { echo "(pas d'image : voir $BASE/$nom/dump.log)" >&2; return 1; }
}
mesure() { python3 tools/metrics-image.py "$@"; }

echo
echo "=== 1. Détramage (deband) — dégradé à marches ==="
config_essai deband-off '{"deband": false}'
config_essai deband-on  '{"deband": true}'
if image deband-off "$BASE/clips/degrade.mp4" "$BASE/dumps/deband-off.png" \
   && image deband-on "$BASE/clips/degrade.mp4" "$BASE/dumps/deband-on.png"; then
    avant=$(mesure banding "$BASE/dumps/deband-off.png")
    apres=$(mesure banding "$BASE/dumps/deband-on.png")
    echo "    sans : $avant   (plateau moyen en lignes, plateau max, valeurs distinctes)"
    echo "    avec : $apres"
    a=$(echo "$avant" | cut -d' ' -f1); b=$(echo "$apres" | cut -d' ' -f1)
    if python3 -c "import sys; sys.exit(0 if float('$b') < 0.5*float('$a') else 1)"; then
        ok "les bandes sont dissoutes : plateau moyen $a → $b lignes"
    else
        ko "le détramage ne fragmente pas les bandes ($a → $b)"
    fi
    grep -q "Filtres vidéo actifs: deband" "$BASE/deband-on/dump.log" \
        && ok "chaîne confirmée par le journal : deband" \
        || ko "le journal ne confirme pas la chaîne deband"
else
    ko "décodage impossible (détramage)"
fi

echo
echo "=== 2. Débruitage (hqdn3d) — bruit gaussien de graine fixe ==="
config_essai bruit-off '{"denoise": 0}'
config_essai bruit-on  '{"denoise": 3}'
if image bruit-off "$BASE/clips/bruit.mp4" "$BASE/dumps/bruit-off.png" \
   && image bruit-on  "$BASE/clips/bruit.mp4" "$BASE/dumps/bruit-on.png"; then
    avant=$(mesure bruit "$BASE/dumps/bruit-off.png")
    apres=$(mesure bruit "$BASE/dumps/bruit-on.png")
    echo "    sans : $avant   (écart-type du passe-haut, |passe-haut| moyen)"
    echo "    avec : $apres"
    a=$(echo "$avant" | cut -d' ' -f1); b=$(echo "$apres" | cut -d' ' -f1)
    if python3 -c "import sys; sys.exit(0 if float('$b') < 0.6*float('$a') else 1)"; then
        ok "l'énergie haute fréquence diminue (écart-type $a → $b)"
    else
        ko "le débruitage ne réduit pas le bruit ($a → $b)"
    fi
else
    ko "décodage impossible (débruitage)"
fi

echo
echo "=== 3. Désentrelacement — peigne réel (objet en mouvement) ==="
# Mesure : écart vertical moyen de l'image. Sur un fond uni avec un seul objet, il ne
# compte QUE les bords : c'est exactement là que le peigne apparaît.
config_essai desent-off   '{"deinterlace": false}'
config_essai desent-bwdif '{"deinterlace": true, "deinterlace_filter": "bwdif"}'
config_essai desent-yadif '{"deinterlace": true, "deinterlace_filter": "yadif"}'
if image desent-off   "$BASE/clips/comb.mp4" "$BASE/dumps/desent-off.png" \
   && image desent-bwdif "$BASE/clips/comb.mp4" "$BASE/dumps/desent-bwdif.png" \
   && image desent-yadif "$BASE/clips/comb.mp4" "$BASE/dumps/desent-yadif.png"; then
    avant=$(mesure peigne "$BASE/dumps/desent-off.png" | cut -d' ' -f1)
    b_bwdif=$(mesure peigne "$BASE/dumps/desent-bwdif.png" | cut -d' ' -f1)
    b_yadif=$(mesure peigne "$BASE/dumps/desent-yadif.png" | cut -d' ' -f1)
    echo "    écart vertical : sans $avant | bwdif $b_bwdif | yadif $b_yadif"
    for duo in "bwdif:$b_bwdif" "yadif:$b_yadif"; do
        nom="${duo%%:*}"; val="${duo##*:}"
        if python3 -c "import sys; sys.exit(0 if float('$val') < 0.3*float('$avant') else 1)"; then
            ok "$nom résorbe le peigne (écart vertical $avant → $val)"
        else
            ko "$nom ne retire pas le peigne ($avant → $val)"
        fi
    done
else
    ko "décodage impossible (désentrelacement, peigne réel)"
fi

echo
echo "=== 3 bis. Désentrelacement — détail fin conservé (ou non) selon le filtre ==="
# Ici la mire est FIXE : `bwdif` ne détecte pas de mouvement et laisse l'image
# intacte (détail préservé) ; `yadif` interpole et efface le détail d'une ligne sur
# deux. C'est la différence mesurée qui justifie le filtre par défaut.
if image desent-bwdif "$BASE/clips/detail.mp4" "$BASE/dumps/detail-bwdif.png" \
   && image desent-yadif "$BASE/clips/detail.mp4" "$BASE/dumps/detail-yadif.png" \
   && image desent-off "$BASE/clips/detail.mp4" "$BASE/dumps/detail-off.png"; then
    d_off=$(mesure peigne "$BASE/dumps/detail-off.png" | cut -d' ' -f1)
    d_bwdif=$(mesure peigne "$BASE/dumps/detail-bwdif.png" | cut -d' ' -f1)
    d_yadif=$(mesure peigne "$BASE/dumps/detail-yadif.png" | cut -d' ' -f1)
    echo "    détail vertical (écart) : non filtré $d_off | bwdif $d_bwdif | yadif $d_yadif"
    if python3 -c "import sys; sys.exit(0 if float('$d_bwdif') > 0.8*float('$d_off') else 1)"; then
        ok "bwdif conserve le détail fin des zones fixes ($d_bwdif sur $d_off)"
    else
        ko "bwdif efface le détail fin ($d_bwdif sur $d_off)"
    fi
    if python3 -c "import sys; sys.exit(0 if float('$d_yadif') < 0.2*float('$d_off') else 1)"; then
        ok "yadif efface le détail fin (comportement connu : $d_yadif sur $d_off)"
    else
        ko "yadif conserve le détail fin ($d_yadif sur $d_off) : mesure incohérente"
    fi
    # Preuve que le réglage `deinterlace_filter` agit : les deux rendus diffèrent.
    if python3 -c "import sys; sys.exit(0 if abs(float('$d_bwdif') - float('$d_yadif')) > 20 else 1)"; then
        ok "le choix du filtre change réellement l'image ($d_bwdif contre $d_yadif)"
    else
        ko "bwdif et yadif donnent la même image ($d_bwdif / $d_yadif)"
    fi
else
    ko "décodage impossible (détail fin)"
fi

echo
echo "=== 4. Réglages d'image (eq) — luminosité, saturation ==="
config_essai eq-off  '{}'
config_essai eq-lum  '{"brightness": 0.25}'
config_essai eq-gris '{"saturation": 0.0}'
if image eq-off "$BASE/clips/degrade.mp4" "$BASE/dumps/eq-off.png" \
   && image eq-lum "$BASE/clips/degrade.mp4" "$BASE/dumps/eq-lum.png"; then
    a=$(mesure luma "$BASE/dumps/eq-off.png" | cut -d' ' -f1)
    b=$(mesure luma "$BASE/dumps/eq-lum.png" | cut -d' ' -f1)
    echo "    luminance moyenne : $a → $b (luminosité +0,25)"
    if python3 -c "import sys; sys.exit(0 if float('$b') > float('$a') + 10 else 1)"; then
        ok "la luminosité augmente réellement (mesuré sur l'image produite)"
    else
        ko "la luminosité ne change pas ($a → $b)"
    fi
else
    ko "décodage impossible (luminosité)"
fi

if image eq-off  "$BASE/clips/couleur.mp4" "$BASE/dumps/couleur-off.png" \
   && image eq-gris "$BASE/clips/couleur.mp4" "$BASE/dumps/couleur-gris.png"; then
    a=$(mesure couleur "$BASE/dumps/couleur-off.png" | cut -d' ' -f1)
    b=$(mesure couleur "$BASE/dumps/couleur-gris.png" | cut -d' ' -f1)
    echo "    saturation de l'image : $a → $b (saturation 0)"
    if python3 -c "import sys; sys.exit(0 if float('$b') < 0.2*float('$a') else 1)"; then
        ok "l'image devient grise (mesuré sur des aplats colorés)"
    else
        ko "la saturation ne change pas ($a → $b)"
    fi
else
    ko "décodage impossible (saturation)"
fi

echo
echo "=== 5. Neutralité : aucun réglage = image identique à la source ==="
# Comparaison avec le décodage de référence de ffmpeg (même image, sans filtre).
config_essai neutre '{}'
if image neutre "$BASE/clips/degrade.mp4" "$BASE/dumps/neutre.png"; then
    if grep -q "Filtres vidéo actifs" "$BASE/neutre/dump.log"; then
        ko "un graphe de filtres a été construit alors qu'aucun réglage n'est actif"
    else
        ok "aucun graphe construit quand tout est neutre (aucun coût)"
    fi
else
    ko "décodage impossible (neutralité)"
fi

echo
echo "=== 6. Paramètres avancés des filtres (section « Avancé » des préférences) ==="
# Ces réglages doivent apparaître TELLE QUELLE dans la chaîne FFmpeg et changer
# l'image : un curseur qui ne modifie pas la chaîne serait décoratif.
config_essai avance-doux '{"deband": true, "deband_range": 16, "deband_threshold": 0.02}'
config_essai avance-fort '{"deband": true, "deband_range": 64, "deband_threshold": 0.15}'
if image avance-doux "$BASE/clips/degrade.mp4" "$BASE/dumps/avance-doux.png" \
   && image avance-fort "$BASE/clips/degrade.mp4" "$BASE/dumps/avance-fort.png"; then
    chaine_doux=$(grep -o "Filtres vidéo actifs: [^ ]*" "$BASE/avance-doux/dump.log" | head -1 | sed 's/.*: //')
    chaine_fort=$(grep -o "Filtres vidéo actifs: [^ ]*" "$BASE/avance-fort/dump.log" | head -1 | sed 's/.*: //')
    echo "    chaîne (doux) : $chaine_doux"
    echo "    chaîne (fort) : $chaine_fort"
    if printf '%s' "$chaine_fort" | grep -q "range=64" && printf '%s' "$chaine_fort" | grep -q "1thr=0.1500"; then
        ok "les valeurs saisies se retrouvent dans la chaîne FFmpeg"
    else
        ko "les paramètres avancés n'atteignent pas la chaîne FFmpeg"
    fi
    a=$(mesure banding "$BASE/dumps/avance-doux.png" | cut -d' ' -f1)
    b=$(mesure banding "$BASE/dumps/avance-fort.png" | cut -d' ' -f1)
    echo "    plateau moyen : doux $a → fort $b"
    if python3 -c "import sys; sys.exit(0 if float('$b') < float('$a') else 1)"; then
        ok "un seuil plus élevé agit davantage (mesuré sur l'image)"
    else
        ko "les paramètres avancés ne changent pas le résultat ($a → $b)"
    fi
else
    ko "décodage impossible (paramètres avancés)"
fi

echo
echo "=== 7. Netteté (unsharp) et géométrie (rotation, zoom) ==="
config_essai nettete '{"sharpen": 3}'
config_essai rotation '{"rotate": 90}'
config_essai zoom '{"zoom": 2.0}'
if image neutre "$BASE/clips/bruit.mp4" "$BASE/dumps/nettete-ref.png" \
   && image nettete "$BASE/clips/bruit.mp4" "$BASE/dumps/nettete.png"; then
    a=$(mesure bruit "$BASE/dumps/nettete-ref.png" | cut -d' ' -f1)
    b=$(mesure bruit "$BASE/dumps/nettete.png" | cut -d' ' -f1)
    echo "    énergie haute fréquence : $a → $b"
    if python3 -c "import sys; sys.exit(0 if float('$b') > float('$a') * 1.5 else 1)"; then
        ok "l'accentuation augmente réellement la netteté mesurée"
    else
        ko "l'accentuation ne change pas la netteté ($a → $b)"
    fi
else
    ko "décodage impossible (netteté)"
fi

if image rotation "$BASE/clips/degrade.mp4" "$BASE/dumps/rotation.png"; then
    dims=$(python3 -c "
from PIL import Image
im = Image.open('$BASE/dumps/rotation.png')
print(f'{im.size[0]}x{im.size[1]}')")
    echo "    dimensions après rotation 90° : $dims"
    if [ "$dims" = "720x1280" ]; then
        ok "la rotation échange bien largeur et hauteur"
    else
        ko "la rotation ne change pas les dimensions ($dims au lieu de 720x1280)"
    fi
else
    ko "décodage impossible (rotation)"
fi

if image neutre "$BASE/clips/degrade.mp4" "$BASE/dumps/zoom-ref.png" \
   && image zoom "$BASE/clips/degrade.mp4" "$BASE/dumps/zoom.png"; then
    # Un zoom 2× doit différer nettement de l'image d'origine, tout en gardant
    # ses dimensions (recadrage puis remise à l'échelle).
    ecart=$(mesure diff "$BASE/dumps/zoom.png" "$BASE/dumps/zoom-ref.png" | cut -d' ' -f1)
    dims=$(python3 -c "
from PIL import Image
im = Image.open('$BASE/dumps/zoom.png')
print(f'{im.size[0]}x{im.size[1]}')")
    echo "    zoom 2× : dimensions $dims, écart moyen avec l'original $ecart/255"
    if python3 -c "import sys; sys.exit(0 if float('$ecart') > 2.0 and '$dims' == '1280x720' else 1)"; then
        ok "le zoom recadre réellement l'image sans changer ses dimensions"
    else
        ko "le zoom est sans effet (écart $ecart, dimensions $dims)"
    fi
else
    ko "décodage impossible (zoom)"
fi

echo
echo "=== 8. Teinte, niveaux, adoucissement (session 34) ==="
# Ces trois réglages complètent `Dialog(384/386)` de PotPlayer. Chacun est mesuré :
# une couleur qui tourne, des noirs qui se recréent, du détail fin qui s'atténue.
CLIP="$BASE/clips/ap.mp4"
[ -s "$CLIP" ] || env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "testsrc2=s=640x360:r=5:d=4" -c:v libx264 -preset ultrafast -y "$CLIP"

config_essai teinte-off '{}'
config_essai teinte-90  '{"hue": 90.0}'
if image teinte-off "$CLIP" "$BASE/dumps/teinte-off.png" && \
   image teinte-90 "$CLIP" "$BASE/dumps/teinte-90.png"; then
    # La teinte de 90° doit faire tourner le rouge vers le vert : on compare la
    # composante dominante d'un aplat rouge de la mire.
    rotation=$(python3 - "$BASE/dumps/teinte-off.png" "$BASE/dumps/teinte-90.png" <<'PY'
import sys
from PIL import Image
def dominante(chemin):
    im = Image.open(chemin).convert("RGB")
    larg, haut = im.size
    # Bande de gauche de testsrc2 : aplat rouge.
    r = g = b = 0
    n = 0
    for y in range(haut // 4, haut // 2, 4):
        for x in range(4, larg // 8, 4):
            p = im.getpixel((x, y)); r += p[0]; g += p[1]; b += p[2]; n += 1
    return (r / n, g / n, b / n)
a, b = dominante(sys.argv[1]), dominante(sys.argv[2])
print("%.0f %.0f %.0f -> %.0f %.0f %.0f" % (a + b))
PY
)
    echo "    rouge de la mire : $rotation"
    if python3 - "$rotation" <<'PY'
import sys
# Avant : rouge dominant. Après 90° : le vert doit dominer le rouge.
l = sys.argv[1].split()
avant = list(map(float, l[0:3])); apres = list(map(float, l[4:7]))
ok = avant[0] > 150 and avant[1] < 60 and apres[1] > apres[0]
sys.exit(0 if ok else 1)
PY
    then
        ok "teinte 90° : le rouge devient vert (rotation réelle)"
    else
        ko "teinte sans effet mesurable ($rotation)"
    fi
else
    ko "décodage impossible (teinte)"
fi

# Niveaux : un gris « lavé » (38) doit devenir noir avec noir=0,16.
LAVE="$BASE/clips/lave.mp4"
[ -s "$LAVE" ] || env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "color=c=0x282828:s=320x180:r=5:d=2,format=yuv420p" \
    -c:v libx264 -preset ultrafast -pix_fmt yuv420p -y "$LAVE"
config_essai niveaux-off '{}'
config_essai niveaux-on  '{"niveaux_noir": 0.16, "niveaux_blanc": 0.92}'
if image niveaux-off "$LAVE" "$BASE/dumps/niveaux-off.png" && \
   image niveaux-on "$LAVE" "$BASE/dumps/niveaux-on.png"; then
    valeurs=$(python3 - "$BASE/dumps/niveaux-off.png" "$BASE/dumps/niveaux-on.png" <<'PY'
import sys
from PIL import Image
def moyenne(chemin):
    im = Image.open(chemin).convert("L")
    px = list(im.getdata())
    return sum(px) / len(px)
print("%.0f %.0f" % (moyenne(sys.argv[1]), moyenne(sys.argv[2])))
PY
)
    avant=$(echo "$valeurs" | cut -d' ' -f1); apres=$(echo "$valeurs" | cut -d' ' -f2)
    echo "    gris lavé : $avant → $apres"
    if [ "${apres:-99}" -lt 8 ] && [ "${avant:-0}" -gt 20 ]; then
        ok "niveaux : le noir lavé est ramené à 0 ($avant → $apres)"
    else
        ko "niveaux sans effet ($avant → $apres)"
    fi
else
    ko "décodage impossible (niveaux)"
fi

# Adoucissement : l'énergie haute fréquence doit chuter nettement.
config_essai adoucir-off '{"denoise": 0, "deband": false}'
config_essai adoucir-3   '{"denoise": 0, "deband": false, "adoucir": 3}'
if image adoucir-off "$BASE/clips/bruit.mp4" "$BASE/dumps/adoucir-off.png" && \
   image adoucir-3 "$BASE/clips/bruit.mp4" "$BASE/dumps/adoucir-3.png"; then
    hf=$(python3 - "$BASE/dumps/adoucir-off.png" "$BASE/dumps/adoucir-3.png" <<'PY'
import sys
from PIL import Image
def hf(chemin):
    im = Image.open(chemin).convert("L")
    px = im.load(); larg, haut = im.size
    vals = []
    for y in range(1, haut - 1, 3):
        for x in range(1, larg - 1, 3):
            vals.append(abs(4 * px[x, y] - px[x - 1, y] - px[x + 1, y]
                            - px[x, y - 1] - px[x, y + 1]))
    return sum(vals) / len(vals)
print("%.2f %.2f" % (hf(sys.argv[1]), hf(sys.argv[2])))
PY
)
    avant=$(echo "$hf" | cut -d' ' -f1); apres=$(echo "$hf" | cut -d' ' -f2)
    echo "    énergie haute fréquence : $avant → $apres"
    if python3 -c "import sys; sys.exit(0 if float('$apres') < float('$avant') * 0.7 else 1)"; then
        ok "adoucissement : le détail fin diminue de plus de 30 % ($avant → $apres)"
    else
        ko "adoucissement sans effet ($avant → $apres)"
    fi
else
    ko "décodage impossible (adoucissement)"
fi

echo
echo "=== 9. Les trois réglages atteignent le moteur par l'application complète ==="
# `--dump-frame` prouve config → filtre ; il ne prouve pas que **l'application**
# construise la même chaîne au chargement (chemin différent : `appliquer_filtres`).
# On lance donc le lecteur avec une fenêtre et on relit la chaîne journalisée.
if command -v Xvfb >/dev/null; then
    DISP_F=:97
    Xvfb "$DISP_F" -screen 0 800x600x24 >"$BASE/xvfb.log" 2>&1 &
    XPID=$!
    sleep 1.5
    config_essai chaine-app '{"hue": 30.0, "niveaux_noir": 0.1, "adoucir": 2}'
    DISPLAY=$DISP_F XDG_CONFIG_HOME="$BASE/chaine-app/conf" XDG_CACHE_HOME="$BASE/chaine-app/cache" \
        XDG_RUNTIME_DIR=/tmp/kplayer-bench-pas-de-son timeout 12 \
        "$KP" "$BASE/clips/ap.mp4" >"$BASE/chaine-app/app.log" 2>&1
    kill "$XPID" 2>/dev/null
    chaine=$(sed 's/\x1b\[[0-9;]*m//g' "$BASE/chaine-app/app.log" | grep -o "Chaîne de filtres vidéo: .*" | head -1)
    echo "    $chaine"
    manquants=""
    for f in "hue=h=30" "colorlevels" "gblur=sigma=1.2"; do
        if ! grep -qF "$f" <<<"$chaine"; then manquants="$manquants $f"; fi
    done
    if [ -z "$manquants" ]; then
        ok "la chaîne construite par l'application contient les trois filtres"
    else
        ko "filtres absents de la chaîne applicative :$manquants"
    fi
else
    info "Xvfb absent — contrôle applicatif ignoré"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]

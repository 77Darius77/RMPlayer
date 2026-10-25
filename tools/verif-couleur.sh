#!/usr/bin/env bash
# verif-couleur.sh — matrice de couleur (BT.601 / 709 / 2020) et plage (limitée / complète) de
# la conversion YUV → RVB : automatique, ou imposées à la main.
#
# Principe : on fabrique des clips dont la couleur RVB d'origine est CONNUE, encodés avec une
# matrice et une plage connues (et marquées, ou non). `--dump-frame` donne l'image RVB que
# produit le moteur ; on compare le pixel mesuré au pixel d'origine. Une mauvaise matrice ne
# se voit pas à l'œil sur un film, mais elle déplace franchement une couleur saturée
# (un rouge (200,50,40) devient (185,34,40) lu en BT.601 alors qu'il est BT.709).
#
# Contrôles :
#   1. automatique, clip BT.709 marqué → couleur d'origine ; clip BT.601 marqué → idem (témoin :
#      l'instrument est exact quand la matrice est la bonne) ;
#   2. automatique, clips NON marqués : la définition décide (HD → 709, SD → 601) ;
#   3. matrice imposée : forcer la mauvaise matrice déplace la couleur de la valeur attendue
#      (preuve que le réglage atteint la conversion) ;
#   4. plage : blanc limité lu en « complète » → 235 au lieu de 255 ; gris complet lu en
#      « limitée » → étiré ;
#   5. une ancienne configuration (sans ces champs) reste automatique.
#
# Usage : tools/verif-couleur.sh        (aucun écran requis)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
BASE=/tmp/kp-couleur
KP=./target/release/rmplayer
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=info

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
command -v python3 >/dev/null || { echo "outil manquant : python3"; exit 2; }
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }

rm -rf "$BASE"; mkdir -p "$BASE/clips"

# clip <nom> <couleur 0xRRGGBB> <taille> <matrice> <plage> <marquer: oui|non>
#   <matrice> : bt709 | smpte170m ; <plage> : tv | pc
clip() {
    local nom="$1" couleur="$2" taille="$3" matrice="$4" plage="$5" marquer="$6" balises=""
    [ "$marquer" = oui ] && balises="-colorspace $matrice -color_primaries $matrice -color_trc $matrice -color_range $plage"
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "color=c=$couleur:s=$taille:r=10:d=2,format=rgb24,scale=in_range=full:out_range=$plage:out_color_matrix=$matrice,format=yuv420p" \
        -c:v libx264 -crf 0 $balises -y "$BASE/clips/$nom.mp4" 2>&1 | tail -2
}
ROUGE=0xC83228
clip r709  $ROUGE 640x360 bt709 tv oui
clip r601  $ROUGE 640x360 smpte170m tv oui
clip hd-nu $ROUGE 1280x720 bt709 tv non       # HD, matrice 709, sans balise
clip sd-nu $ROUGE 640x360 smpte170m tv non    # SD, matrice 601, sans balise
clip blanc-tv 0xFFFFFF 640x360 bt709 tv oui
clip gris-pc  0xC8C8C8 640x360 bt709 pc oui

config() {  # config <nom> '<json video>' : isole la configuration
    local d="$BASE/$1"; rm -rf "$d"; mkdir -p "$d/conf/rmplayer" "$d/cache"
    python3 - "$d/conf/rmplayer/config.json" "$2" <<'PY'
import json, sys
video = {"hardware_decoding": False, "scaling_method": "Lanczos"}
video.update(json.loads(sys.argv[2]))
json.dump({"video": video, "audio": {"device": "", "volume_max": 150},
           "subtitles": {"auto_load": False}, "playback": {"resume": False}, "playlist": []},
          open(sys.argv[1], "w"), indent=2)
PY
}
# pixel <config> <clip> : « R G B » du centre de l'image décodée
pixel() {
    XDG_CONFIG_HOME="$BASE/$1/conf" XDG_CACHE_HOME="$BASE/$1/cache" \
        "$KP" --dump-frame 0.5 "$BASE/$1/sortie.png" "$BASE/clips/$2.mp4" >/dev/null 2>&1
    python3 - "$BASE/$1/sortie.png" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert("RGB")
print(*im.getpixel((im.width // 2, im.height // 2)))
PY
}
# proche "R G B" "R G B" tolérance
proche() {
    python3 -c "
import sys
a = list(map(int, '$1'.split())); b = list(map(int, '$2'.split())); t = int('$3')
sys.exit(0 if len(a) == 3 and all(abs(x - y) <= t for x, y in zip(a, b)) else 1)"
}
ORIG="200 50 40"

config auto '{}'
echo "=== 1. Automatique, clips marqués ==="
p=$(pixel auto r709); echo "  BT.709 marqué : $p (attendu $ORIG)"
if proche "$p" "$ORIG" 5; then ok "BT.709 marqué : couleur d'origine retrouvée"; else ko "BT.709 marqué lu $p au lieu de $ORIG"; fi
p=$(pixel auto r601); echo "  BT.601 marqué : $p (attendu $ORIG)"
if proche "$p" "$ORIG" 5; then ok "BT.601 marqué : couleur d'origine retrouvée (témoin)"; else ko "BT.601 marqué lu $p au lieu de $ORIG"; fi

echo "=== 2. Automatique, clips non marqués : la définition décide ==="
p=$(pixel auto hd-nu); echo "  HD non marqué (709) : $p"
if proche "$p" "$ORIG" 5; then ok "HD sans balise → lu en BT.709"; else ko "HD sans balise lu $p au lieu de $ORIG"; fi
p=$(pixel auto sd-nu); echo "  SD non marqué (601) : $p"
if proche "$p" "$ORIG" 5; then ok "SD sans balise → lu en BT.601"; else ko "SD sans balise lu $p au lieu de $ORIG"; fi

echo "=== 3. Matrice imposée à la main ==="
config m601 '{"color_matrix": "Bt601"}'; config m709 '{"color_matrix": "Bt709"}'
p=$(pixel m601 r709); echo "  709 lu en 601 : $p"
if ! proche "$p" "$ORIG" 8 && proche "$p" "185 34 40" 6; then ok "matrice 601 imposée sur du 709 : (185,34,40) comme prévu"; else ko "709 forcé en 601 lu $p (attendu ≈ 185 34 40)"; fi
p=$(pixel m709 r601); echo "  601 lu en 709 : $p"
if ! proche "$p" "$ORIG" 8; then ok "matrice 709 imposée sur du 601 : la couleur se déplace ($p)"; else ko "601 forcé en 709 inchangé : $p"; fi

echo "=== 4. Plage imposée ==="
config pc '{"color_range": "Full"}'; config tv '{"color_range": "Limited"}'
p=$(pixel auto blanc-tv); echo "  blanc limité, auto : $p"
if proche "$p" "255 255 255" 3; then ok "automatique : blanc limité → 255"; else ko "blanc limité auto lu $p"; fi
p=$(pixel pc blanc-tv); echo "  blanc limité lu en complète : $p"
if proche "$p" "235 235 235" 4; then ok "plage complète imposée : le blanc limité reste à 235"; else ko "plage complète imposée lue $p (attendu ≈ 235)"; fi
p=$(pixel auto gris-pc); echo "  gris complet 200, auto : $p"
if proche "$p" "200 200 200" 3; then ok "automatique : gris complet → 200"; else ko "gris complet auto lu $p"; fi
p=$(pixel tv gris-pc); echo "  gris complet lu en limitée : $p"
if proche "$p" "214 214 214" 4; then ok "plage limitée imposée : le gris complet est étiré (≈ 214)"; else ko "plage limitée imposée lue $p (attendu ≈ 214)"; fi

echo "=== 5. Ancienne configuration ==="
config ancienne '{}'
p=$(pixel ancienne r709)
if proche "$p" "$ORIG" 5; then ok "sans les champs : automatique"; else ko "ancienne configuration lue $p"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]

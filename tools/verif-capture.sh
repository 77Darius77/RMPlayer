#!/usr/bin/env bash
# verif-capture.sh — captures d'écran : format (PNG / JPEG / WebP), qualité JPEG, taille native,
# modèle de nom. On ne regarde pas un réglage : on regarde le FICHIER écrit (octets magiques,
# décodage, dimensions, écart de pixels avec le PNG de référence).
#
# Clip de test : 1920×1080 (plus grand que la fenêtre : la capture « affichée » est alors réduite), dégradé fixe (rouge selon X, vert selon Y) — la même image à tout instant,
# donc les captures de deux séances sont comparables pixel à pixel.
#
# Contrôles :
#   0. témoin : sans réglage, PNG nommé « rmplayer-AAAAMMJJ-HHMMSS.png » (signature PNG), c'est la
#      référence des autres mesures ;
#   1. JPEG (qualité 92) : extension .jpg, signature JPEG, mêmes dimensions que la référence, pixels
#      proches (écart moyen < 6) ;
#   2. JPEG qualité 10 : fichier plus PETIT et écart de pixels plus GRAND qu'à 92 (la qualité agit) ;
#   3. WebP : extension .webp, signature RIFF/WEBP, pixels IDENTIQUES au PNG (sans perte) ;
#   4. taille native : l'image écrite fait 1920×1080 (celle du film), pas la taille d'affichage ;
#   5. modèle de nom « {media}_{position} » : nom = titre du film + position, et un « / » dans le modèle
#      ne crée pas de sous-dossier ;
#   6. qualité absurde (0) : la capture est quand même un JPEG valide (bornée à 1) ;
#   7. ancienne configuration (sans aucun de ces champs) : PNG, comme avant.
#
# Usage : tools/verif-capture.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-capture
KP=./target/release/rmplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=info

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
for outil in xdotool python3 ffmpeg; do
    command -v "$outil" >/dev/null || { echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }
export DISPLAY="$DISP"
xdpyinfo >/dev/null 2>&1 || { echo "écran $DISP indisponible — banc non exécutable ici"; exit 2; }

rm -rf "$BASE"; mkdir -p "$BASE/conf/rmplayer" "$BASE/cache" "$BASE/caps"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/rmplayer/config.json"
arreter() {
    for pid in $(pgrep -x rmplayer 2>/dev/null); do
        aff=$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | sed -n 's/^DISPLAY=//p')
        [ "$aff" = "$DISP" ] && kill "$pid" 2>/dev/null
    done
    sleep 0.5
}
trap arreter EXIT
arreter

env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "color=c=black:s=1920x1080:r=10:d=12,format=rgb24,geq=r='X*255/W':g='Y*255/H':b=128" \
    -c:v libx264 -preset ultrafast -crf 0 -pix_fmt yuv444p -colorspace bt709 -color_range pc -y "$BASE/clip.mp4"

# capturer <nom> '<json interface>' : une séance — lancement, pause, touche S ; le fichier écrit est
# dans $BASE/caps/<nom>/ (dossier propre à la séance).
capturer() {
    local nom="$1"
    rm -rf "$BASE/caps/$nom"; mkdir -p "$BASE/caps/$nom"
    python3 - "$CONF" "$2" "$BASE/caps/$nom" <<'PY'
import json, sys
interface = {"show_osd": False, "screenshots_dir": sys.argv[3]}
interface.update(json.loads(sys.argv[2]))
json.dump({"video": {"hardware_decoding": False}, "audio": {"device": ""},
           "subtitles": {"auto_load": False}, "interface": interface,
           "playback": {"resume": False}, "playlist": []}, open(sys.argv[1], "w"), indent=2)
PY
    arreter
    "$KP" --fullscreen "$BASE/clip.mp4" >"$BASE/$nom.log" 2>&1 &
    local pid=$!
    sleep 4.5
    local id; id=$(xdotool search --pid "$pid" --name RMPlayer 2>/dev/null | tail -1)
    xdotool windowfocus "$id" 2>/dev/null; sleep 0.3
    xdotool key --window "$id" space 2>/dev/null; sleep 0.6      # pause : l'image est fixe
    xdotool key --window "$id" s 2>/dev/null; sleep 1.5          # capture
    kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null; sleep 0.3
}
fichier() { find "$BASE/caps/$1" -type f 2>/dev/null | head -1; }
nb_fichiers() { find "$BASE/caps/$1" -type f 2>/dev/null | wc -l; }
# infos <fichier> : « magique largeur hauteur » — magique = PNG / JPEG / WEBP / ?, dimensions par PIL
infos() {
    python3 - "$1" <<'PY'
import sys
from PIL import Image
d = open(sys.argv[1], "rb").read(16)
if d[:8] == b"\x89PNG\r\n\x1a\n": m = "PNG"
elif d[:3] == b"\xff\xd8\xff": m = "JPEG"
elif d[:4] == b"RIFF" and d[8:12] == b"WEBP": m = "WEBP"
else: m = "?"
try:
    w, h = Image.open(sys.argv[1]).size
except Exception:
    w = h = 0
print(m, w, h)
PY
}
# ecart <a> <b> : « moyen max » des écarts absolus RVB entre deux images de même taille (99999 si tailles différentes)
ecart() {
    python3 - "$1" "$2" <<'PY'
import sys
import numpy as np
from PIL import Image
a = np.asarray(Image.open(sys.argv[1]).convert("RGB"), dtype=int)
b = np.asarray(Image.open(sys.argv[2]).convert("RGB"), dtype=int)
if a.shape != b.shape:
    print("99999 99999")
else:
    d = abs(a - b)
    print("%.2f %d" % (d.mean(), d.max()))
PY
}

echo "=== 0. Témoin : sans réglage, un PNG (la référence) ==="
capturer ref '{}'
F=$(fichier ref); n=$(nb_fichiers ref)
read -r M W H <<<"$(infos "$F" 2>/dev/null)"
echo "  fichier : $(basename "${F:-aucun}") — $M ${W}x${H} ($n fichier)"
if [ "$n" -eq 1 ] && [ "$M" = "PNG" ] && [ "${W:-0}" -gt 0 ] && echo "$(basename "$F")" | grep -qE '^rmplayer-[0-9]{8}-[0-9]{6}\.png$'; then
    ok "PNG nommé rmplayer-AAAAMMJJ-HHMMSS.png, ${W}x${H}"
else ko "référence inattendue : $F ($M ${W}x${H}, $n fichier)"; fi
REF="$F"; REFW=$W; REFH=$H

echo "=== 1. JPEG (qualité 92) ==="
capturer jpg92 '{"capture_format": "jpeg", "capture_jpeg_quality": 92}'
F=$(fichier jpg92); read -r M W H <<<"$(infos "$F" 2>/dev/null)"
E=$(ecart "$REF" "$F" 2>/dev/null); set -- $E; MOY92=${1:-99999}; T92=$(stat -c%s "$F" 2>/dev/null || echo 0)
echo "  $(basename "${F:-aucun}") — $M ${W}x${H}, ${T92} octets, écart moyen $MOY92"
if [ "$M" = "JPEG" ] && echo "$F" | grep -qE '\.jpe?g$' && [ "$W" = "$REFW" ] && [ "$H" = "$REFH" ] \
   && python3 -c "import sys; sys.exit(0 if float('$MOY92') < 6 else 1)"; then
    ok "JPEG valide, mêmes dimensions que le PNG, pixels proches (écart moyen $MOY92)"
else ko "JPEG : $M ${W}x${H} écart $MOY92 (référence ${REFW}x${REFH})"; fi

echo "=== 2. La qualité agit (10 contre 92) ==="
capturer jpg10 '{"capture_format": "jpeg", "capture_jpeg_quality": 10}'
F10=$(fichier jpg10); T10=$(stat -c%s "$F10" 2>/dev/null || echo 0)
E=$(ecart "$REF" "$F10" 2>/dev/null); set -- $E; MOY10=${1:-99999}
echo "  qualité 10 : ${T10} octets, écart $MOY10 — qualité 92 : ${T92} octets, écart $MOY92"
if [ "$T10" -gt 0 ] && [ "$T10" -lt "$T92" ] && python3 -c "import sys; sys.exit(0 if float('$MOY10') > float('$MOY92') else 1)"; then
    ok "qualité 10 : fichier plus petit ($T10 < $T92 octets) et image plus éloignée ($MOY10 > $MOY92)"
else ko "la qualité n'agit pas : $T10 contre $T92 octets, écarts $MOY10 contre $MOY92"; fi

echo "=== 3. WebP sans perte ==="
capturer webp '{"capture_format": "webp"}'
F=$(fichier webp); read -r M W H <<<"$(infos "$F" 2>/dev/null)"
E=$(ecart "$REF" "$F" 2>/dev/null); set -- $E
echo "  $(basename "${F:-aucun}") — $M ${W}x${H}, écart moyen ${1:-?} max ${2:-?}"
if [ "$M" = "WEBP" ] && echo "$F" | grep -q '\.webp$' && [ "${2:-1}" = "0" ]; then
    ok "WebP valide, pixels identiques au PNG (sans perte)"
else ko "WebP : $M ${W}x${H}, écart max ${2:-?} (attendu 0)"; fi

echo "=== 4. Taille native ==="
capturer native '{"capture_native": true}'
F=$(fichier native); read -r M W H <<<"$(infos "$F" 2>/dev/null)"
echo "  $(basename "${F:-aucun}") — $M ${W}x${H} (affichée : ${REFW}x${REFH})"
if [ "$M" = "PNG" ] && [ "$W" = "1920" ] && [ "$H" = "1080" ] && { [ "$REFW" != "1920" ] || [ "$REFH" != "1080" ]; }; then
    ok "capture à la taille du film (1920x1080), pas à celle de l'affichage (${REFW}x${REFH})"
else ko "native : ${W}x${H} (attendu 1920x1080, différent de l'affichage ${REFW}x${REFH})"; fi

echo "=== 5. Modèle de nom ==="
capturer nom '{"capture_name": "{media}_{position}"}'
F=$(fichier nom)
echo "  fichier : $(basename "${F:-aucun}")"
if echo "$(basename "$F")" | grep -qE '^clip_[0-9]{2}-[0-9]{2}-[0-9]{2}\.png$'; then ok "nom = titre du film + position (clip_HH-MM-SS.png)"
else ko "nom inattendu : $(basename "${F:-aucun}") (attendu clip_HH-MM-SS.png)"; fi
capturer slash '{"capture_name": "a/b"}'
F=$(fichier slash)
if [ "$(nb_fichiers slash)" -eq 1 ] && [ "$(dirname "$F")" = "$BASE/caps/slash" ] && [ "$(basename "$F")" = "a_b.png" ]; then
    ok "un « / » dans le modèle devient « _ » : aucun sous-dossier"
else ko "slash : $F ($(nb_fichiers slash) fichier(s))"; fi

echo "=== 6. Qualité absurde ==="
capturer q0 '{"capture_format": "jpeg", "capture_jpeg_quality": 0}'
F=$(fichier q0); read -r M W H <<<"$(infos "$F" 2>/dev/null)"
if [ "$M" = "JPEG" ] && [ "${W:-0}" -gt 0 ]; then ok "qualité 0 : JPEG valide (bornée à 1)"
else ko "qualité 0 : $M ${W}x${H}"; fi

echo "=== 7. Ancienne configuration ==="
capturer ancienne '{}'
F=$(fichier ancienne); read -r M W H <<<"$(infos "$F" 2>/dev/null)"
if [ "$M" = "PNG" ] && [ "$W" = "$REFW" ]; then ok "sans les champs : PNG comme avant"
else ko "ancienne configuration : $M ${W}x${H}"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]

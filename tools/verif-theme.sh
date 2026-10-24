#!/usr/bin/env bash
# verif-theme.sh — thèmes externes (`~/.config/kplayer/themes/*.json`).
#
# Principe : un pixel de la barre de menus de la VRAIE fenêtre. Le thème intégré donne une
# couleur connue ; un thème externe doit la remplacer par celle du fichier ; un thème illisible
# doit laisser l'interface intacte (jamais une fenêtre cassée) et être remis à « intégré ».
#
# Contrôles :
#   1. référence : thème intégré (sombre d'egui) ;
#   2. thème externe « nuit » (fond 123456) : le pixel prend cette couleur ;
#   3. thème externe de base claire : le pixel devient clair ;
#   4. thème dont une couleur est illisible : interface intacte (= référence), réglage remis à
#      « intégré » dans config.json ;
#   5. thème absent : même repli ; 6. ancienne configuration (sans le champ) : référence.
#
# Usage : tools/verif-theme.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-theme
KP=./target/release/kplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=kplayer=info

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
for outil in xdotool python3 ffmpeg; do
    command -v "$outil" >/dev/null || { echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }
export DISPLAY="$DISP"
xdpyinfo >/dev/null 2>&1 || { echo "écran $DISP indisponible — banc non exécutable ici"; exit 2; }

rm -rf "$BASE"; mkdir -p "$BASE/conf/kplayer/themes" "$BASE/cache"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/kplayer/config.json"
THEMES="$XDG_CONFIG_HOME/kplayer/themes"

arreter() {
    for pid in $(pgrep -x kplayer 2>/dev/null); do
        aff=$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | sed -n 's/^DISPLAY=//p')
        [ "$aff" = "$DISP" ] && kill "$pid" 2>/dev/null
    done
    sleep 0.5
}
trap arreter EXIT
arreter

env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "color=c=0x202020:s=640x360:r=10:d=30" -c:v libx264 -preset ultrafast -y "$BASE/film.mp4"
printf '{"base": "dark", "fond": "123456", "texte": "FFE0A0"}\n' > "$THEMES/nuit.json"
printf '{"base": "light"}\n' > "$THEMES/clair.json"
printf '{"base": "dark", "fond": "ZZZZZZ"}\n' > "$THEMES/casse.json"

# pixel <nom> '<json interface>' [sans_champ] : lance (fenêtre normale, barre de menus visible) et
# lit un pixel de la barre de menus.
pixel() {
    python3 - "$CONF" "$2" "${3:-}" <<'PY'
import json, sys
interface = {"show_osd": False, "screenshots_dir": "/tmp"}
interface.update(json.loads(sys.argv[2]))
json.dump({"video": {"hardware_decoding": False}, "audio": {"device": "", "volume_max": 150},
           "subtitles": {"auto_load": False}, "interface": interface,
           "playback": {"resume": False}, "playlist": []}, open(sys.argv[1], "w"), indent=2)
PY
    arreter
    "$KP" "$BASE/film.mp4" >"$BASE/$1.log" 2>&1 &
    local pid=$!
    sleep 4.5
    xdotool mousemove 3 700 2>/dev/null; sleep 0.3
    ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x720 -i "$DISP+0,0" -frames:v 1 -y "$BASE/$1.png"
    kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null; sleep 0.3
    python3 -c "
from PIL import Image
im = Image.open('$BASE/$1.png').convert('RGB'); print(*im.getpixel((1000, 8)))"
}
proche() { python3 -c "
a = list(map(int, '$1'.split())); b = list(map(int, '$2'.split()))
import sys; sys.exit(0 if len(a) == 3 and all(abs(x - y) <= $3 for x, y in zip(a, b)) else 1)"; }

echo "=== 1. Référence : thème intégré ==="
REF=$(pixel ref '{}'); echo "  barre de menus : $REF"
if proche "$REF" "27 27 27" 6; then ok "thème intégré sombre : $REF"; else ko "référence inattendue : $REF"; fi

echo "=== 2. Thème externe « nuit » ==="
P=$(pixel nuit '{"theme": "nuit"}'); echo "  $P (attendu 18 52 86)"
if proche "$P" "18 52 86" 3; then ok "la barre de menus prend la couleur du thème (123456)"; else ko "thème nuit : $P"; fi

echo "=== 3. Thème de base claire ==="
P=$(pixel clair '{"theme": "clair"}'); echo "  $P"
if proche "$P" "248 248 248" 12; then ok "base claire : barre de menus claire ($P)"; else ko "thème clair : $P"; fi

echo "=== 4. Thème illisible ==="
P=$(pixel casse '{"theme": "casse"}')
t=$(python3 -c "import json; print(json.load(open('$CONF'))['interface'].get('theme', '?'))")
echo "  barre : $P ; theme dans config.json après l'essai : « $t »"
if proche "$P" "$REF" 2 && [ -z "$t" ]; then ok "interface intacte ($P) et réglage remis à « intégré »"
else ko "thème illisible : pixel $P (référence $REF), theme=« $t »"; fi

echo "=== 5. Thème absent ==="
P=$(pixel absent '{"theme": "n-existe-pas"}')
if proche "$P" "$REF" 2; then ok "thème absent : interface intacte ($P)"; else ko "thème absent : $P"; fi

echo "=== 6. Ancienne configuration ==="
P=$(pixel ancienne '{}')
if proche "$P" "$REF" 2; then ok "sans le champ : thème intégré ($P)"; else ko "ancienne configuration : $P"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]

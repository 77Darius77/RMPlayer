#!/usr/bin/env bash
# verif-saut-pause.sh — sauter dans le film PENDANT LA PAUSE doit rafraîchir l'image (et les
# sous-titres intégrés) : barre de progression, ±5 s, signets, chapitres, MPRIS, pas à pas.
#
# Défaut trouvé en s.37 : le décodage d'une image n'était fait que pour l'état « lecture ». En
# pause, un saut changeait la position mais l'écran gardait l'ancienne image (écart mesuré 0,0), et
# une réplique de piste intégrée n'était pas relue.
#
# Principe : un film dont la COULEUR dit l'instant (teinte tournante : 12° par seconde) ; on
# lit la teinte du centre de la vraie fenêtre, qui doit correspondre à la position visée.
#
# Contrôles :
#   1. témoin, en lecture : un saut change la couleur affichée (l'instrument voit les sauts) ;
#   2. en pause, saut à 40 s puis à 20 s : la teinte affichée est celle de l'instant visé
#      (120° puis 240°, à 25° près) ;
#   3. en pause, pas à pas image (touche « . » ×10) : la position avance d'environ 1 s et
#      l'image change ;
#   4. en pause, saut dans un MKV à réplique intégrée longue : la réplique est affichée ;
#   5. la lecture reprend ensuite normalement (la position avance).
#
# Usage : tools/verif-saut-pause.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-saut-pause
KP=./target/release/kplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=kplayer=info

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
for outil in xdotool python3 ffmpeg gdbus; do
    command -v "$outil" >/dev/null || { echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }
export DISPLAY="$DISP"
xdpyinfo >/dev/null 2>&1 || { echo "écran $DISP indisponible — banc non exécutable ici"; exit 2; }
if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ] && [ -S "$XDG_RUNTIME_DIR/bus" ]; then
    export DBUS_SESSION_BUS_ADDRESS="unix:path=$XDG_RUNTIME_DIR/bus"
fi

rm -rf "$BASE"; mkdir -p "$BASE/conf/kplayer" "$BASE/cache"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"

arreter() {
    for pid in $(pgrep -x kplayer 2>/dev/null); do
        aff=$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | sed -n 's/^DISPLAY=//p')
        [ "$aff" = "$DISP" ] && kill "$pid" 2>/dev/null
    done
    sleep 0.5
}
trap arreter EXIT
arreter

# Teinte tournante : 12° par seconde (à t = 40 s : 480° = 120°, à 20 s : 240°). Images clés
# fréquentes (-g 10) : un saut tombe tout près de la cible.
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "color=c=red:s=640x360:r=10:d=80,hue=h=t*12" -c:v libx264 -preset ultrafast -g 10 -y "$BASE/teinte.mp4"
printf '1\n00:00:00,000 --> 00:00:30,000\nMMMMMMMMMM\n\n' > "$BASE/long.srt"
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "color=c=0x202020:s=640x360:r=10:d=30" -i "$BASE/long.srt" -map 0:v -map 1 \
    -c:v libx264 -preset ultrafast -c:s srt -metadata:s:s:0 language=fre -y "$BASE/replique.mkv"

python3 - "$XDG_CONFIG_HOME/kplayer/config.json" <<'PY'
import json, sys
json.dump({"video": {"hardware_decoding": False}, "audio": {"device": "", "volume_max": 150},
           "subtitles": {"auto_load": False, "use_embedded": True, "font_size": 40.0, "outline_width": 0.0,
                         "background_alpha": 0, "position_percent": 50.0, "default_language": "fre"},
           "interface": {"show_osd": False, "screenshots_dir": "/tmp"},
           "playback": {"resume": False}, "playlist": []}, open(sys.argv[1], "w"), indent=2)
PY

mpris() { gdbus call --session --dest org.mpris.MediaPlayer2.kplayer --object-path /org/mpris/MediaPlayer2 "$@" 2>/dev/null; }
lance() {
    arreter
    "$KP" --fullscreen "$BASE/$1" >"$BASE/dernier.log" 2>&1 &
    K_PID=$!
    sleep 4.5
    ID=$(xdotool search --pid "$K_PID" --name KPlayer 2>/dev/null | tail -1)
    xdotool windowfocus "$ID" 2>/dev/null; sleep 0.3
    PISTE=$(mpris --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player Metadata \
        | sed -n "s/.*trackid[^']*objectpath '\([^']*\)'.*/\1/p" | head -1)
}
pause() { mpris --method org.mpris.MediaPlayer2.Player.Pause >/dev/null; sleep 0.6; }
vers() { mpris --method org.mpris.MediaPlayer2.Player.SetPosition "${PISTE:-/org/kplayer/track/0}" "$(( $1 * 1000000 ))" >/dev/null; sleep 1.6; }
pos_ms() { mpris --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player Position \
    | sed -n 's/.*int64 \([0-9-]*\).*/\1/p' | head -1 | awk '{printf "%d", $1/1000}'; }
capture() { xdotool mousemove 3 3 2>/dev/null; sleep 0.3
    ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x720 -i "$DISP+0,0" -frames:v 1 -y "$BASE/$1.png"; }
# teinte <image> : teinte (degrés) du centre de la fenêtre
teinte() { python3 - "$BASE/$1.png" <<'PY'
import colorsys, sys
from PIL import Image
r, g, b = Image.open(sys.argv[1]).convert("RGB").getpixel((640, 360))
print(int(colorsys.rgb_to_hsv(r / 255, g / 255, b / 255)[0] * 360))
PY
}
ecart_teinte() { python3 -c "d = abs(($1 - $2 + 180) % 360 - 180); print(d)"; }
# ecart_image <a> <b> : écart moyen de luminosité entre deux captures (0 = identiques). La teinte ne
# convient pas partout : près du vert pur la couleur est écrêtée et reste à 120° (piège de l'instrument).
ecart_image() { python3 - "$BASE/$1.png" "$BASE/$2.png" <<'PY'
import sys
import numpy as np
from PIL import Image
a = np.asarray(Image.open(sys.argv[1]).convert("RGB"), dtype=int)
b = np.asarray(Image.open(sys.argv[2]).convert("RGB"), dtype=int)
print("%.2f" % float(abs(a - b).mean()))
PY
}
largeur_texte() { python3 - "$BASE/$1.png" <<'PY'
import sys
import numpy as np
from PIL import Image
a = np.asarray(Image.open(sys.argv[1]).convert("L"), dtype=int); a[:30, :30] = 0
m = a > 200
print(0 if m.sum() < 30 else int(np.ptp(np.nonzero(m)[1])))
PY
}

echo "=== 1. Témoin : un saut en lecture change la couleur ==="
lance teinte.mp4
capture lecture0; h0=$(teinte lecture0)
vers 40; capture lecture1; h1=$(teinte lecture1)
echo "  teinte avant le saut : ${h0}° ; après un saut à 40 s en lecture : ${h1}°"
if [ "$(ecart_teinte "$h0" "$h1")" -ge 30 ]; then ok "en lecture, le saut change la couleur ($h0° → $h1°) : l'instrument voit les sauts"
else ko "même en lecture la couleur ne change pas ($h0° → $h1°) : instrument aveugle"; fi

echo "=== 2. En pause : le saut rafraîchit l'image ==="
pause
vers 20; capture p20; h20=$(teinte p20); p=$(pos_ms)
vers 40; capture p40; h40=$(teinte p40)
echo "  pause + saut à 20 s → ${h20}° (attendu 240°) ; saut à 40 s → ${h40}° (attendu 120°)"
if [ "$(ecart_teinte "$h20" 240)" -le 25 ] && [ "$(ecart_teinte "$h40" 120)" -le 25 ]; then
    ok "en pause, l'image affichée est celle de l'instant visé (240° puis 120°)"
else ko "en pause, l'image ne suit pas le saut : ${h20}° / ${h40}° (attendu 240° / 120°)"; fi

echo "=== 3. En pause : pas à pas image ==="
p0=$(pos_ms); capture pas0
capture pas0bis; sans=$(ecart_image pas0 pas0bis)
for _ in $(seq 10); do xdotool key --window "$ID" period 2>/dev/null; sleep 0.25; done
sleep 1; p1=$(pos_ms); capture pas1; avec=$(ecart_image pas0 pas1)
echo "  position ${p0} → ${p1} ms ; écart d'image : sans pas ${sans}, après 10 pas ${avec}"
if [ "$(( p1 - p0 ))" -ge 700 ] && [ "$(( p1 - p0 ))" -le 1500 ] \
   && python3 -c "import sys; sys.exit(0 if float('$avec') >= 0.5 and float('$sans') < 0.1 else 1)"; then
    ok "10 pas d'une image : la position avance de $(( p1 - p0 )) ms et l'image change (${avec} contre ${sans} sans pas)"
else ko "pas à pas : position $p0 → $p1, écart d'image $avec (sans pas : $sans)"; fi

echo "=== 5. La lecture reprend ==="
mpris --method org.mpris.MediaPlayer2.Player.Play >/dev/null; sleep 0.8
q0=$(pos_ms); sleep 2; q1=$(pos_ms)
if [ "$(( q1 - q0 ))" -ge 1200 ]; then ok "la lecture avance après la reprise ($q0 → $q1 ms)"; else ko "la lecture n'avance pas ($q0 → $q1)"; fi

echo "=== 4. En pause : réplique intégrée longue ==="
lance replique.mkv
pause
vers 8; capture replique; l=$(largeur_texte replique)
echo "  largeur du texte affiché : $l px"
if [ "$l" -gt 120 ]; then ok "la réplique intégrée est affichée après un saut en pause ($l px)"; else ko "aucune réplique après un saut en pause (largeur $l px)"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]

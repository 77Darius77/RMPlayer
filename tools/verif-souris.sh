#!/usr/bin/env bash
# verif-souris.sh — gestes de souris avec modificateurs : Molette seule = volume,
# Ctrl+Molette = chercher dans le film (5 s par cran), Maj+Molette = vitesse ;
# et une personnalisation (Alt+DoubleClic) fonctionne.
#
# Principe : de vrais événements de souris (xdotool, molette = boutons 4/5, avec la touche
# modificatrice tenue), et l'EFFET lu sur MPRIS : Volume, Position, Rate. Chaque geste doit
# bouger sa grandeur et laisser les autres immobiles (c'est ce qui prouve que les modificateurs
# aiguillent bien le geste).
#
# Usage : tools/verif-souris.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-souris
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
CONF="$XDG_CONFIG_HOME/kplayer/config.json"
SHOTS="$BASE/captures"; mkdir -p "$SHOTS"

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
    -f lavfi -i "testsrc2=s=640x360:r=10:d=60" -c:v libx264 -preset ultrafast -y "$BASE/film.mp4"

ecrire_config() {  # ecrire_config '<json raccourcis>'
    python3 - "$CONF" "$1" "$SHOTS" <<'PY'
import json, sys
json.dump({"video": {"hardware_decoding": False}, "audio": {"device": "", "volume_max": 150},
           "subtitles": {"auto_load": False},
           "interface": {"show_osd": False, "screenshots_dir": sys.argv[3]},
           "raccourcis": json.loads(sys.argv[2]),
           "playback": {"resume": False}, "playlist": []}, open(sys.argv[1], "w"), indent=2)
PY
}
mpris() { gdbus call --session --dest org.mpris.MediaPlayer2.kplayer --object-path /org/mpris/MediaPlayer2 "$@" 2>/dev/null; }
lire() {  # lire <propriété> : nombre
    mpris --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player "$1" \
        | sed -n 's/^(<\(int64 \)\?\(-\?[0-9.e+-]*\)>,)$/\2/p' | head -1
}
lance() {
    arreter
    "$KP" --fullscreen "$BASE/film.mp4" >"$BASE/dernier.log" 2>&1 &
    K_PID=$!
    sleep 4.5
    ID=$(xdotool search --pid "$K_PID" --name KPlayer 2>/dev/null | tail -1)
    xdotool windowfocus "$ID" 2>/dev/null; sleep 0.3
    xdotool mousemove 640 360; sleep 0.4
}
vers() {  # vers <ms> : pause puis saut
    local piste
    piste=$(mpris --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player Metadata \
        | sed -n "s/.*trackid[^']*objectpath '\([^']*\)'.*/\1/p" | head -1)
    mpris --method org.mpris.MediaPlayer2.Player.Pause >/dev/null; sleep 0.4
    mpris --method org.mpris.MediaPlayer2.Player.SetPosition "${piste:-/org/kplayer/track/0}" "$(( $1 * 1000 ))" >/dev/null
    sleep 1.0
}
pos_ms() { python3 -c "print(int(float('$(lire Position)')/1000))"; }
molette() {  # molette <haut|bas> <n> [modificateur]
    local bouton=4; [ "$1" = bas ] && bouton=5
    [ -n "${3:-}" ] && xdotool keydown "$3"
    for _ in $(seq "$2"); do xdotool click $bouton; sleep 0.25; done
    [ -n "${3:-}" ] && xdotool keyup "$3"
    sleep 0.8
}
proche() { python3 -c "import sys; sys.exit(0 if abs($1 - $2) <= $3 else 1)"; }

ecrire_config '{}'
lance; vers 20000
v0=$(lire Volume); p0=$(pos_ms); r0=$(lire Rate)
echo "  départ : volume $v0, position $p0 ms, vitesse $r0"

echo "=== 1. Molette seule : le volume bouge, rien d'autre ==="
molette bas 3
v1=$(lire Volume); p1=$(pos_ms); r1=$(lire Rate)
echo "  volume $v0 → $v1, position $p0 → $p1, vitesse $r0 → $r1"
if python3 -c "import sys; sys.exit(0 if float('$v1') < float('$v0') - 0.05 else 1)" \
   && proche "$p1" "$p0" 600 && proche "$r1" "$r0" 0.001; then
    ok "Molette : le volume baisse ; position et vitesse inchangées"
else ko "molette seule : volume $v0→$v1, position $p0→$p1, vitesse $r0→$r1"; fi

echo "=== 2. Ctrl+Molette : on cherche dans le film ==="
vers 20000; v2=$(lire Volume)
molette haut 2 ctrl
p2=$(pos_ms); v3=$(lire Volume)
molette bas 1 ctrl
p3=$(pos_ms)
echo "  position 20000 → $p2 (2 crans vers l'avant) → $p3 (1 cran en arrière) ; volume $v2 → $v3"
# Un cran de la molette X11 peut produire plusieurs pas : on mesure le sens et des pas de 5 s entiers.
avance=$(( p2 - 20000 )); recul=$(( p2 - p3 ))
if [ "$avance" -ge 4000 ] && [ $(( avance % 5000 )) -lt 800 -o $(( avance % 5000 )) -gt 4200 ] \
   && [ "$recul" -ge 4000 ] && [ "$recul" -lt "$avance" ] && proche "$v3" "$v2" 0.001; then
    ok "Ctrl+Molette : avance de $avance ms (pas de 5 s) puis recule de $recul ms ; volume inchangé"
else ko "Ctrl+Molette : avance $avance, recul $recul ms, volume $v2→$v3"; fi

echo "=== 3. Maj+Molette : la vitesse ==="
vers 20000; v4=$(lire Volume); r4=$(lire Rate); p4=$(pos_ms)
molette haut 2 shift
r5=$(lire Rate); v5=$(lire Volume); p5=$(pos_ms)
echo "  vitesse $r4 → $r5, volume $v4 → $v5, position $p4 → $p5"
if python3 -c "import sys; sys.exit(0 if float('$r5') > float('$r4') + 0.1 else 1)" \
   && proche "$v5" "$v4" 0.001 && proche "$p5" "$p4" 600; then
    ok "Maj+Molette : la vitesse monte ; volume et position inchangés"
else ko "Maj+Molette : vitesse $r4→$r5, volume $v4→$v5, position $p4→$p5"; fi

echo "=== 4. Personnalisation : Alt+DoubleClic = capture d'écran ==="
ecrire_config '{"image.capture": "Alt+DoubleClic", "soustitres.visibles": ""}'
lance; vers 5000
av=$(ls "$SHOTS" 2>/dev/null | wc -l)
xdotool mousemove 640 360; sleep 0.3
xdotool click --repeat 2 --delay 80 1; sleep 1.5
ap=$(ls "$SHOTS" 2>/dev/null | wc -l)
xdotool keydown alt; xdotool click --repeat 2 --delay 80 1; xdotool keyup alt; sleep 2
ap2=$(ls "$SHOTS" 2>/dev/null | wc -l)
echo "  captures : $av → $ap (double-clic seul) → $ap2 (Alt+double-clic)"
if [ "$ap" -eq "$av" ] && [ "$ap2" -eq $(( av + 1 )) ]; then
    ok "Alt+DoubleClic prend une capture ; le double-clic seul n'en prend pas"
else ko "captures $av → $ap → $ap2 (attendu $av → $av → $(( av + 1 )))"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]

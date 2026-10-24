#!/bin/bash
# Ouvre chaque menu de la barre et capture son contenu.
# Sert à juger l'ordre, la lisibilité et l'exhaustivité des menus (inspection visuelle).
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
BASE=/tmp/menus
rm -rf "$BASE"; mkdir -p "$BASE"
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export DISPLAY=:94
export XDG_CONFIG_HOME="$BASE/config" XDG_CACHE_HOME="$BASE/cache"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"

Xvfb :94 -screen 0 1280x800x24 >"$BASE/xvfb.log" 2>&1 &
XPID=$!
sleep 2
./target/release/kplayer /tmp/menu-test.mkv >"$BASE/lecteur.log" 2>&1 &
KPID=$!
sleep 3

WID=$(xdotool search --pid $KPID --name KPlayer | head -1)
eval "$(xdotool getwindowgeometry --shell "$WID")"
xdotool mousemove $((X + WIDTH/2)) $((Y + HEIGHT/2))
xdotool windowfocus --sync "$WID"
sleep 1

# Positions des menus, lues dans l'image de la barre (elles dépendent des libellés)
ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x800 -i :94+0,0 -frames:v 1 -y "$BASE/barre.png" 2>/dev/null

capturer() {  # capturer <nom> <x_menu>
    xdotool mousemove "$2" 10
    xdotool click 1
    sleep 0.8
    ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x800 -i :94+0,0 \
        -frames:v 1 -y "$BASE/$1.png" 2>/dev/null
    xdotool key Escape
    sleep 0.4
}

capturer fichier 55
capturer lecture 120
capturer audio 200
capturer soustitres 275
capturer playlist 357
capturer preferences 440

kill $KPID 2>/dev/null; wait $KPID 2>/dev/null
kill $XPID 2>/dev/null
echo "captures dans $BASE"
ls "$BASE"/*.png

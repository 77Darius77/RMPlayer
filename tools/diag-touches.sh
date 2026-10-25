#!/bin/bash
# Diagnostic : les touches atteignent-elles la fenêtre sous Xvfb ?
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib XDG_RUNTIME_DIR=/tmp/rmplayer-bench-pas-de-son

Xvfb :99 -screen 0 1280x800x24 >/tmp/xvfb-diag.log 2>&1 &
XPID=$!
sleep 2
DISPLAY=:99 RUST_LOG=rmplayer=info ./target/release/rmplayer /media/blaise/LINUX_DATA/test-60s.mp4 >/tmp/diag.log 2>&1 &
KPID=$!
sleep 5

echo "--- fenêtres"
DISPLAY=:99 xdotool search --name "" 2>/dev/null | while read -r w; do
    printf "  id=%s name=%s geom=%s\n" "$w" "$(DISPLAY=:99 xdotool getwindowname "$w" 2>/dev/null)" \
        "$(DISPLAY=:99 xdotool getwindowgeometry --shell "$w" 2>/dev/null | tr '\n' ' ')"
done
echo "--- focus actuel : $(DISPLAY=:99 xdotool getwindowfocus 2>&1)"
WID=$(DISPLAY=:99 xdotool search --name RMPlayer | head -1)
echo "--- fenêtre RMPlayer : ${WID:-aucune}"

echo "--- test 1 : key --window"
DISPLAY=:99 xdotool key --window "$WID" i; sleep 2
DISPLAY=:99 ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x800 -i :99 -frames:v 1 -y /tmp/diag1.png

echo "--- test 2 : windowfocus + key"
DISPLAY=:99 xdotool windowfocus --sync "$WID" 2>&1
DISPLAY=:99 xdotool key i; sleep 2
DISPLAY=:99 ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x800 -i :99 -frames:v 1 -y /tmp/diag2.png

echo "--- test 3 : xdotool type + touche h via XTEST après activation"
DISPLAY=:99 xdotool windowactivate --sync "$WID" 2>&1 || true
DISPLAY=:99 xdotool key --clearmodifiers h; sleep 3
DISPLAY=:99 ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x800 -i :99 -frames:v 1 -y /tmp/diag3.png

kill "$KPID" 2>/dev/null; sleep 1; kill "$XPID" 2>/dev/null
echo "--- journal (bascules)"
grep -E "Décodage matériel|VA-API|Recharg" /tmp/diag.log | sed 's/\x1b\[[0-9;]*m//g' | tail -5

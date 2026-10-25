#!/usr/bin/env bash
# test-vivacite.sh — vérifie que le détecteur de gel DÉTECTE vraiment.
#
# Un contrôle qu'on n'a jamais vu échouer ne prouve rien : on fige volontairement
# le lecteur (`SIGSTOP`) et on exige que `tools/vivacite.sh` le constate. Sans
# cette étape, « l'application répond » pourrait tout aussi bien signifier que le
# détecteur ne regarde rien.
#
# Usage : tools/test-vivacite.sh [écran]     (par défaut :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
LOG=/tmp/test-vivacite.log
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=info,kinterface=debug

pkill -x rmplayer 2>/dev/null || true
sleep 1
DISPLAY=$DISP ./target/release/rmplayer /media/blaise/LINUX_DATA/test-60s.mp4 >"$LOG" 2>&1 &
KPID=$!
sleep 6
if ! kill -0 "$KPID" 2>/dev/null; then
    echo "✗ le lecteur n'a pas démarré"; exit 1
fi

touches_avant=$(grep -c "touche X" "$LOG" || true)

# Sans gestionnaire de fenêtres (écran X virtuel), le clavier n'est délivré
# qu'à la fenêtre qui a RÉELLEMENT le focus d'entrée : ni le survol, ni le clic
# ne suffisent (`XGetInputFocus` reste sur PointerRoot = 1). Il faut
# `windowactivate` (repli `windowfocus`) : constaté ici à l'instant.
WID=$(DISPLAY=$DISP xdotool search --pid "$KPID" --name RMPlayer 2>/dev/null | head -1)
[ -z "$WID" ] && WID=$(DISPLAY=$DISP xdotool search --name RMPlayer 2>/dev/null | head -1)
eval "$(DISPLAY=$DISP xdotool getwindowgeometry --shell "$WID")"
focus_lecteur() {
    DISPLAY=$DISP xdotool windowactivate --sync "$WID" 2>/dev/null \
        || DISPLAY=$DISP xdotool windowfocus --sync "$WID" 2>/dev/null || true
    DISPLAY=$DISP xdotool mousemove $((X + WIDTH / 2)) $((Y + HEIGHT / 3))
    sleep 0.2
}
focus_lecteur

# 1) Application normale : le détecteur doit réussir.
focus_lecteur
if DISPLAY=$DISP tools/vivacite.sh "$LOG" "$DISP" 5; then
    echo "✔ 1/3 application normale : le détecteur confirme qu'elle répond"
else
    echo "✗ 1/3 application normale : le détecteur croit qu'elle est gelée"; kill -9 "$KPID"; exit 1
fi

# 2) Application figée : le détecteur doit ÉCHOUER (c'est le cas qui compte).
kill -STOP "$KPID"
sleep 0.5
focus_lecteur
if DISPLAY=$DISP tools/vivacite.sh "$LOG" "$DISP" 3 >/dev/null 2>&1; then
    echo "✗ 2/3 application figée (SIGSTOP) : le détecteur ne l'a PAS vue — contrôle inutile"
    kill -CONT "$KPID"; kill -9 "$KPID"; exit 1
else
    echo "✔ 2/3 application figée (SIGSTOP) : le détecteur l'a bien vue (échec attendu)"
fi

# 3) Reprise : le détecteur doit repasser au vert.
kill -CONT "$KPID"
sleep 1
focus_lecteur
if DISPLAY=$DISP tools/vivacite.sh "$LOG" "$DISP" 5; then
    echo "✔ 3/3 après SIGCONT : le détecteur repasse au vert"
else
    echo "✗ 3/3 après SIGCONT : le détecteur reste au rouge"; kill -9 "$KPID"; exit 1
fi

touches_apres=$(grep -c "touche X" "$LOG" || true)
echo "   (touches « X » traitées : $touches_avant → $touches_apres)"
kill -TERM "$KPID" 2>/dev/null || true
sleep 1
kill -9 "$KPID" 2>/dev/null || true
echo "=== détecteur de gel validé : il voit le gel ET la reprise ==="

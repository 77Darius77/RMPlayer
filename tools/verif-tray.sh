#!/usr/bin/env bash
# verif-tray.sh — icône dans la zone de notification (StatusNotifierItem).
#
# Principe : on parle à l'icône comme le ferait le bureau, par D-Bus (le « watcher » de Plasma
# liste les icônes ; l'icône expose `Activate` et un menu `com.canonical.dbusmenu`), et on
# mesure l'EFFET : la fenêtre disparaît puis revient (xdotool), la lecture se met en pause
# (MPRIS), le lecteur se ferme.
#
# Contrôles :
#   0. témoin : réglage désactivé → aucune icône pour ce processus ;
#   1. réglage activé → l'icône est enregistrée auprès du watcher ; infobulle = nom du film ;
#   2. clic principal (Activate) : la fenêtre se masque, un second clic l'affiche ;
#   3. menu « Pause » : la lecture se met en pause (MPRIS) ; le libellé devient « Lecture » ;
#   4. menu « Quitter » : le lecteur se ferme.
#
# **Effet de bord sur le bureau réel** : l'icône apparaît quelques secondes dans la zone de
# notification de la session (comme RMPlayer apparaît dans le widget multimédia pendant
# verif-mpris). Sans watcher StatusNotifier, le banc se déclare non exécutable.
#
# Usage : tools/verif-tray.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-tray
KP=./target/release/rmplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=info

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
WATCHER=org.kde.StatusNotifierWatcher
items() {
    gdbus call --session --dest $WATCHER --object-path /StatusNotifierWatcher \
        --method org.freedesktop.DBus.Properties.Get $WATCHER RegisteredStatusNotifierItems 2>/dev/null
}
items >/dev/null || { echo "aucun StatusNotifierWatcher sur le bus de session — banc non exécutable ici"; exit 2; }

rm -rf "$BASE"; mkdir -p "$BASE/conf/rmplayer" "$BASE/cache"
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
    -f lavfi -i "testsrc2=s=320x180:r=10:d=60" -c:v libx264 -preset ultrafast -y "$BASE/Mon film de test.mp4"

ecrire_config() {  # ecrire_config <true|false>
    python3 - "$CONF" "$1" <<'PY'
import json, sys
json.dump({"video": {"hardware_decoding": False}, "audio": {"device": "", "volume_max": 150},
           "subtitles": {"auto_load": False},
           "interface": {"show_osd": False, "screenshots_dir": "/tmp", "tray_icon": sys.argv[2] == "true"},
           "playback": {"resume": False}, "playlist": []}, open(sys.argv[1], "w"), indent=2)
PY
}
lance() {
    arreter
    "$KP" "$BASE/Mon film de test.mp4" >"$BASE/dernier.log" 2>&1 &
    K_PID=$!
    sleep 5
}
mon_item() {  # nom de bus de l'icône de CE processus, ou vide
    items | grep -o "org.kde.StatusNotifierItem-$K_PID-[0-9]*" | head -1
}
fenetres_visibles() { xdotool search --onlyvisible --pid "$K_PID" --name RMPlayer 2>/dev/null | wc -l; }
# id_menu <libellé> : identifiant dbusmenu de l'entrée portant ce libellé
id_menu() {
    gdbus call --session --dest "$ITEM" --object-path /MenuBar --method com.canonical.dbusmenu.GetLayout 0 "int32 -1" "@as []" 2>/dev/null \
        | python3 -c "
import re, sys
t = sys.stdin.read()
for m in re.finditer(r\"\\((\\d+), \\{([^}]*)\\}, @av\", t):
    if \"'label': <'$1'>\" in m.group(2):
        print(m.group(1)); break"
}
clic_menu() {  # clic_menu <id>
    gdbus call --session --dest "$ITEM" --object-path /MenuBar --method com.canonical.dbusmenu.Event "$1" clicked "<0>" 0 >/dev/null 2>&1
}
statut() {
    gdbus call --session --dest org.mpris.MediaPlayer2.rmplayer --object-path /org/mpris/MediaPlayer2 \
        --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player PlaybackStatus 2>/dev/null \
        | sed -n "s/.*'\(Playing\|Paused\|Stopped\)'.*/\1/p"
}

echo "=== 0. Témoin : réglage désactivé ==="
ecrire_config false; lance
if [ -z "$(mon_item)" ]; then ok "aucune icône enregistrée pour ce processus"; else ko "une icône existe alors que le réglage est désactivé : $(mon_item)"; fi

echo "=== 1. Réglage activé : l'icône est enregistrée ==="
ecrire_config true; lance
ITEM=$(mon_item)
echo "  icône : ${ITEM:-aucune}"
if [ -n "$ITEM" ]; then ok "icône enregistrée auprès du watcher ($ITEM)"; else ko "aucune icône enregistrée"; echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="; exit 1; fi
tip=$(gdbus call --session --dest "$ITEM" --object-path /StatusNotifierItem --method org.freedesktop.DBus.Properties.Get org.kde.StatusNotifierItem ToolTip 2>/dev/null)
if echo "$tip" | grep -q "Mon film de test.mp4"; then ok "l'infobulle porte le nom du film"; else ko "infobulle : $(echo "$tip" | cut -c1-120)"; fi

echo "=== 2. Clic principal : afficher / masquer ==="
v0=$(fenetres_visibles)
gdbus call --session --dest "$ITEM" --object-path /StatusNotifierItem --method org.kde.StatusNotifierItem.Activate 0 0 >/dev/null 2>&1; sleep 1.5
v1=$(fenetres_visibles)
gdbus call --session --dest "$ITEM" --object-path /StatusNotifierItem --method org.kde.StatusNotifierItem.Activate 0 0 >/dev/null 2>&1; sleep 1.5
v2=$(fenetres_visibles)
echo "  fenêtres visibles : $v0 → $v1 (après un clic) → $v2 (après deux)"
if [ "$v0" -ge 1 ] && [ "$v1" -eq 0 ] && [ "$v2" -ge 1 ]; then ok "la fenêtre se masque puis réapparaît"
else ko "visibilité $v0 → $v1 → $v2 (attendu ≥1 → 0 → ≥1)"; fi

echo "=== 3. Menu : Pause ==="
s0=$(statut)
id=$(id_menu "Pause"); echo "  statut avant : ${s0:-?} ; entrée « Pause » : ${id:-introuvable}"
if [ -n "$id" ]; then
    clic_menu "$id"; sleep 1.2
    s1=$(statut); lib=$(id_menu "Lecture")
    echo "  statut après : ${s1:-?} ; entrée « Lecture » présente : ${lib:-non}"
    if [ "$s0" = Playing ] && [ "$s1" = Paused ]; then ok "le menu met la lecture en pause (MPRIS : Playing → Paused)"
    else ko "statut $s0 → $s1"; fi
    if [ -n "$lib" ]; then ok "le libellé devient « Lecture »"; else ko "le libellé n'a pas changé"; fi
else ko "entrée « Pause » introuvable dans le menu"; fi

echo "=== 4. Menu : Quitter ==="
id=$(id_menu "Quitter")
if [ -n "$id" ]; then
    clic_menu "$id"
    for _ in $(seq 20); do kill -0 "$K_PID" 2>/dev/null || break; sleep 0.25; done
    if ! kill -0 "$K_PID" 2>/dev/null; then ok "« Quitter » ferme le lecteur"; else ko "le lecteur tourne encore"; fi
else ko "entrée « Quitter » introuvable"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]

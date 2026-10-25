#!/usr/bin/env bash
# verif-boucle-ab.sh — répétition A-B : nombre de passages et délai entre deux passages.
#
# Principe : une lecture réelle, et la **position publiée sur MPRIS** échantillonnée à
# ~10 Hz. Un retour au point A est une chute de la position ; on les compte sur la courbe
# (et, en second recours, dans le journal : les deux doivent s'accorder), et on mesure le
# temps entre deux retours. Aucun contrôle ne repose sur un message seul.
#
# Contrôles :
#   1. témoin — sans limite (0 passage = infini, le comportement historique), la boucle
#      tourne sans fin : au moins 4 retours en 12 s. Sans ce témoin, « 3 retours puis
#      arrêt » pourrait être un instrument qui ne voit rien ;
#   2. 3 passages → exactement 3 retours sur la courbe ET 3 dans le journal, puis la lecture
#      dépasse le point B (la boucle est levée, le film continue) ;
#   3. délai : la période entre deux retours augmente du délai demandé (1,5 s ± 0,5 s)
#      par rapport à la même boucle sans délai ;
#   4. un fichier de configuration antérieur (sans les deux champs) donne la boucle
#      historique (infinie, sans délai).
#
# Usage : tools/verif-boucle-ab.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-boucle-ab
KP=./target/release/rmplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=info

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
info() { echo "  · $1"; }

for outil in xdotool python3 gdbus; do
    command -v "$outil" >/dev/null || { echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }
export DISPLAY="$DISP"
xdpyinfo >/dev/null 2>&1 || { echo "écran $DISP indisponible — banc non exécutable ici"; exit 2; }

if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]; then
    bus="${XDG_RUNTIME_DIR}/bus"
    [ -S "$bus" ] && export DBUS_SESSION_BUS_ADDRESS="unix:path=$bus"
fi
gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.ListNames >/dev/null 2>&1 \
    || { echo "pas de bus de session — banc non exécutable ici"; exit 2; }

pkill -x rmplayer 2>/dev/null || true
sleep 1
rm -rf "$BASE"; mkdir -p "$BASE/conf/rmplayer" "$BASE/cache"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/rmplayer/config.json"

nettoyer() {
    [ -n "${K_PID:-}" ] && kill "$K_PID" 2>/dev/null
    pkill -x rmplayer 2>/dev/null || true
}
trap nettoyer EXIT

env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "testsrc2=s=320x180:r=10:d=30" -f lavfi -i "sine=f=440:d=30" \
    -c:v libx264 -preset ultrafast -c:a aac -shortest -y "$BASE/mire.mp4"

# regle <passages> <délai_ms> : écrit les deux réglages (ou les retire si « absent »).
regle() {
    python3 - "$CONF" "$1" "$2" <<'PY'
import json, os, sys
conf, passages, delai = sys.argv[1:4]
c = json.load(open(conf)) if os.path.exists(conf) else {}
pb = c.setdefault("playback", {})
for cle, v in (("ab_repetitions", passages), ("ab_delay_ms", delai)):
    if v == "absent":
        pb.pop(cle, None)
    else:
        pb[cle] = int(v)
json.dump(c, open(conf, "w"), indent=2)
PY
}

# mesure <durée_s> <journal> : lance le lecteur sur la boucle 2 s → 4 s, échantillonne la
# position MPRIS pendant <durée_s>, ferme, et imprime : « retours dates… | maximum_ms ».
mesure() {
    local duree="$1" journal="$2"
    : > "$journal"
    "$KP" --seek 2 --boucle-ab 2 4 "$BASE/mire.mp4" >>"$journal" 2>&1 &
    K_PID=$!
    for _ in $(seq 60); do
        gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
            --method org.freedesktop.DBus.ListNames 2>/dev/null | grep -q org.mpris.MediaPlayer2.rmplayer && break
        sleep 0.25
    done
    python3 - "$duree" <<'PY'
import re, subprocess, sys, time
duree = float(sys.argv[1])
cmd = ["gdbus", "call", "--session", "--dest", "org.mpris.MediaPlayer2.rmplayer",
       "--object-path", "/org/mpris/MediaPlayer2", "--method",
       "org.freedesktop.DBus.Properties.Get", "org.mpris.MediaPlayer2.Player", "Position"]
t0 = time.time(); serie = []
while time.time() - t0 < duree:
    try:
        sortie = subprocess.run(cmd, capture_output=True, text=True, timeout=2).stdout
    except Exception:
        sortie = ""
    m = re.search(r"int64 (-?\d+)", sortie)
    if m:
        serie.append((time.time() - t0, int(m.group(1)) / 1000.0))
    time.sleep(0.08)
retours = [serie[i][0] for i in range(1, len(serie)) if serie[i-1][1] - serie[i][1] > 1000]
maxi = max((p for _, p in serie), default=-1)
print("%d echantillons" % len(serie), file=sys.stderr)
print(" ".join("%.2f" % r for r in retours) + " | %.0f" % maxi)
PY
    kill "$K_PID" 2>/dev/null; wait "$K_PID" 2>/dev/null; K_PID=""; sleep 0.5
}
nb_retours() { echo "$1" | cut -d'|' -f1 | wc -w; }
maximum() { echo "$1" | cut -d'|' -f2 | tr -d ' '; }
periode() { # écart moyen entre retours successifs
    python3 -c "
import sys
t = [float(x) for x in sys.argv[1].split('|')[0].split()]
print('%.2f' % ((t[-1]-t[0])/(len(t)-1)) if len(t) > 1 else 'nan')" "$1"
}

echo "=== 1. Témoin : sans limite, la boucle tourne sans fin ==="
regle 0 0
r=$(mesure 12 "$BASE/j1.txt")
n=$(nb_retours "$r"); echo "  retours : $n (courbe)  maximum : $(maximum "$r") ms"
if [ "$n" -ge 4 ]; then ok "boucle infinie : $n retours en 12 s (l'instrument voit les retours)"
else ko "seulement $n retour(s) en 12 s sans limite — instrument aveugle ou boucle absente"; fi
P0=$(periode "$r")

echo "=== 2. Trois passages, puis la lecture continue ==="
regle 3 0
r=$(mesure 12 "$BASE/j2.txt")
n=$(nb_retours "$r"); nj=$(grep -c "retour au point A" "$BASE/j2.txt"); mx=$(maximum "$r")
echo "  retours : $n (courbe), $nj (journal)  maximum : $mx ms"
if [ "$n" -eq 3 ]; then ok "3 retours sur la courbe de position"; else ko "$n retour(s) sur la courbe (attendu 3)"; fi
if [ "$nj" -eq 3 ]; then ok "3 retours dans le journal (accord avec la courbe)"; else ko "$nj retour(s) au journal (attendu 3)"; fi
if python3 -c "import sys; sys.exit(0 if float('$mx') > 5000 else 1)"; then
    ok "la lecture dépasse le point B (maximum $mx ms > 5000) : la boucle est levée"
else ko "maximum $mx ms : la lecture n'a pas dépassé le point B"; fi

echo "=== 3. Délai entre deux passages ==="
regle 0 1500
r=$(mesure 14 "$BASE/j3.txt")
P1=$(periode "$r"); n=$(nb_retours "$r")
echo "  période sans délai : ${P0} s ; avec 1500 ms : ${P1} s ($n retours)"
if python3 -c "
import sys
a, b = float('$P0'), float('$P1')
sys.exit(0 if abs((b - a) - 1.5) <= 0.5 else 1)" 2>/dev/null; then
    ok "la période augmente de ≈ 1,5 s (${P0} → ${P1} s)"
else ko "période ${P0} → ${P1} s : l'écart n'est pas ≈ 1,5 s"; fi

echo "=== 4. Ancienne configuration (sans les deux champs) ==="
regle absent absent
r=$(mesure 10 "$BASE/j4.txt")
n=$(nb_retours "$r")
if [ "$n" -ge 3 ]; then ok "sans champs : boucle historique, $n retours en 10 s"
else ko "sans champs : $n retour(s) — le comportement historique a changé"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]

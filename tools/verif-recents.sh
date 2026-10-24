#!/usr/bin/env bash
# verif-recents.sh — fichiers récents : la liste est écrite, ordonnée, sans doublon, bornée.
#
# Principe : on ne regarde pas un menu, on regarde ce que le lecteur **a écrit** dans
# `config.json` après avoir ouvert de vrais fichiers, en le relançant à chaque fois (une
# session par fichier : c'est ce qu'un utilisateur fait en ouvrant ses films un à un).
#
# Contrôles :
#   0. témoin : un lancement SANS fichier n'ajoute rien (sinon « 3 entrées » ne prouverait
#      rien : l'instrument inventerait ses entrées) ;
#   1. trois fichiers ouverts → trois entrées, la plus récente d'abord ;
#   2. rouvrir le premier → il remonte en tête, la liste garde 3 entrées (pas de doublon) ;
#   3. un ancien config.json sans le champ se relit et reçoit l'entrée ;
#   4. douze fichiers ouverts → la liste est bornée à 10, les deux plus anciens sont sortis ;
#   5. un fichier disparu reste listé mais n'empêche pas d'en ouvrir un autre.
#
# Usage : tools/verif-recents.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-recents
KP=./target/release/kplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=kplayer=info
BORNE=10

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }

for outil in Xvfb xdotool python3; do
    command -v "$outil" >/dev/null || {
        echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }

pkill -x kplayer 2>/dev/null || true
sleep 1

rm -rf "$BASE"; mkdir -p "$BASE/conf/kplayer" "$BASE/cache" "$BASE/films"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/kplayer/config.json"
JOURNAL="$BASE/journal.txt"

nettoyer() {
    [ -n "${K_PID:-}" ] && kill "$K_PID" 2>/dev/null
    pkill -x kplayer 2>/dev/null || true
}
trap nettoyer EXIT

# Écran partagé de la suite (ou de verif-rapide) : on ne démarre pas de serveur ici.
export DISPLAY="$DISP"
xdpyinfo >/dev/null 2>&1 || { echo "écran $DISP indisponible — banc non exécutable ici"; exit 2; }

for i in $(seq -w 1 12); do
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "testsrc2=s=160x90:r=10:d=3" -c:v libx264 -preset ultrafast \
        -y "$BASE/films/film-$i.mp4"
done
F() { echo "$BASE/films/film-$(printf %02d "$1").mp4"; }

# ouvre <fichier…> : une session du lecteur, attend que la fenêtre existe et que la
# configuration ait été écrite, puis ferme.
ouvre() {
    "$KP" "$@" >>"$JOURNAL" 2>&1 &
    K_PID=$!
    for _ in $(seq 60); do
        xdotool search --pid "$K_PID" --name KPlayer >/dev/null 2>&1 && break
        sleep 0.25
    done
    sleep 1.2   # l'écriture suit l'ouverture du média
    kill "$K_PID" 2>/dev/null
    wait "$K_PID" 2>/dev/null
    K_PID=""
    sleep 0.4
}
# recents : les entrées de playback.recent, une par ligne (noms de fichier seuls).
recents() {
    python3 - "$CONF" <<'PY'
import json, os, sys
try:
    c = json.load(open(sys.argv[1]))
except Exception:
    raise SystemExit
for e in c.get("playback", {}).get("recent", []):
    print(os.path.basename(e))
PY
}
liste() { recents | tr '\n' ' ' | sed 's/ $//'; }

echo "=== 0. Témoin : lancer sans fichier n'ajoute rien ==="
ouvre
avant=$(liste)
if [ -z "$avant" ]; then
    ok "aucun lancement sans fichier n'a produit d'entrée (liste vide)"
else
    ko "des entrées existent sans qu'aucun fichier n'ait été ouvert : $avant"
fi

echo "=== 1. Trois fichiers → trois entrées, la plus récente d'abord ==="
ouvre "$(F 1)"; ouvre "$(F 2)"; ouvre "$(F 3)"
obtenu=$(liste)
attendu="film-03.mp4 film-02.mp4 film-01.mp4"
echo "  liste : $obtenu"
if [ "$obtenu" = "$attendu" ]; then
    ok "ordre : le plus récent en tête ($attendu)"
else
    ko "attendu « $attendu », obtenu « $obtenu »"
fi

echo "=== 2. Rouvrir le premier : il remonte, sans doublon ==="
ouvre "$(F 1)"
obtenu=$(liste)
attendu="film-01.mp4 film-03.mp4 film-02.mp4"
echo "  liste : $obtenu"
if [ "$obtenu" = "$attendu" ]; then
    ok "film-01 remonte en tête, 3 entrées (pas de doublon)"
else
    ko "attendu « $attendu », obtenu « $obtenu »"
fi

echo "=== 3. Ancien config.json sans le champ ==="
python3 - "$CONF" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))
c["playback"].pop("recent", None)
json.dump(c, open(sys.argv[1], "w"), indent=2)
PY
if python3 -c "import json,sys; sys.exit(0 if 'recent' not in json.load(open('$CONF'))['playback'] else 1)"; then
    ouvre "$(F 4)"
    obtenu=$(liste)
    if [ "$obtenu" = "film-04.mp4" ]; then
        ok "config sans « recent » relue sans erreur ; l'entrée est ajoutée"
    else
        ko "attendu « film-04.mp4 », obtenu « $obtenu »"
    fi
else
    ko "la manipulation du fichier de test a échoué"
fi

echo "=== 4. Douze fichiers → borne à $BORNE ==="
for i in $(seq 1 12); do ouvre "$(F "$i")"; done
n=$(recents | wc -l)
tete=$(recents | head -1)
queue=$(recents | tail -1)
echo "  $n entrées, de $tete à $queue"
if [ "$n" -eq "$BORNE" ]; then
    ok "liste bornée à $BORNE entrées (12 ouvertures)"
else
    ko "$n entrées au lieu de $BORNE"
fi
if [ "$tete" = "film-12.mp4" ] && [ "$queue" = "film-03.mp4" ]; then
    ok "les deux plus anciens (film-01, film-02) sont sortis"
else
    ko "tête « $tete » (attendu film-12.mp4), queue « $queue » (attendu film-03.mp4)"
fi

echo "=== 5. Fichier disparu : la liste le garde, l'ouverture d'un autre fonctionne ==="
cp "$(F 5)" "$BASE/films/ephemere.mp4"
ouvre "$BASE/films/ephemere.mp4"
rm -f "$BASE/films/ephemere.mp4"
ouvre "$(F 6)"
liste_apres=$(liste)
if echo "$liste_apres" | grep -q "ephemere.mp4" && [ "$(recents | head -1)" = "film-06.mp4" ]; then
    ok "l'entrée disparue reste listée ; film-06 s'ouvre et passe en tête"
else
    ko "liste : $liste_apres"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]

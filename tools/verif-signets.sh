#!/usr/bin/env bash
# verif-signets.sh — marque-pages (signets) par média : ajouter, aller au suivant / précédent,
# supprimer, retrouver après un redémarrage.
#
# Principe : une vraie fenêtre, pilotée comme un utilisateur (touches via xdotool, position via
# MPRIS), et deux mesures indépendantes : le fichier `signets.json` écrit, et la POSITION de
# lecture publiée sur MPRIS après un « signet suivant ». Les signets vivent avec la
# configuration (`~/.config/rmplayer/`), pas dans le cache qu'on peut effacer.
#
# Contrôles :
#   0. témoin : sans geste, aucun signet n'existe ;
#   1. Ctrl+B à 5 s puis à 12 s → deux signets triés ; un troisième à 12 s n'ajoute rien ;
#   2. le fichier est dans le dossier de configuration, pas dans le cache ;
#   3. après redémarrage : « suivant » depuis 0 → 5 s, puis 12 s ; « précédent » → 5 s ;
#   4. Ctrl+Maj+B à 5 s supprime ce signet ; il reste celui de 12 s ;
#   5. un autre média n'a aucun signet : « suivant » n'y fait rien ;
#   6. renommer (Ctrl+Maj+N) : une vraie saisie au clavier dans la boîte de dialogue change le nom écrit
#      dans signets.json (témoin : le nom d'origine est différent) ; Échap annule sans rien changer ;
#      sans signet à la position, rien n'est renommé ni créé ; la position du signet ne bouge pas ;
#   7. pendant la saisie, les touches vont au champ : la lettre « s » de « Passage » ne déclenche pas la
#      capture d'écran (témoin : « s » seul, hors saisie, en produit une).
#
# Usage : tools/verif-signets.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-signets
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

rm -rf "$BASE"; mkdir -p "$BASE/conf/rmplayer" "$BASE/cache" "$BASE/media"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
SIGNETS="$XDG_CONFIG_HOME/rmplayer/signets.json"
M="$BASE/media"

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
    -f lavfi -i "testsrc2=s=320x180:r=10:d=30" -c:v libx264 -preset ultrafast -y "$M/un.mp4"
cp "$M/un.mp4" "$M/deux.mp4"
mkdir -p "$BASE/captures"
python3 - "$XDG_CONFIG_HOME/rmplayer/config.json" "$BASE/captures" <<'PY'
import json, sys
json.dump({"video": {"hardware_decoding": False}, "audio": {"device": "", "volume_max": 150},
           "subtitles": {"auto_load": False}, "interface": {"show_osd": False, "screenshots_dir": sys.argv[2]},
           "playback": {"resume": False}, "playlist": []}, open(sys.argv[1], "w"), indent=2)
PY

lance() {  # lance <média>
    arreter
    "$KP" --fullscreen "$M/$1" >"$BASE/dernier.log" 2>&1 &
    K_PID=$!
    sleep 4.5
    ID=$(xdotool search --pid "$K_PID" --name RMPlayer 2>/dev/null | tail -1)
    xdotool windowfocus "$ID" 2>/dev/null; sleep 0.3
}
mpris() { gdbus call --session --dest org.mpris.MediaPlayer2.rmplayer --object-path /org/mpris/MediaPlayer2 "$@" 2>/dev/null; }
vers() {  # vers <ms> : pause puis saut à la position
    local piste
    piste=$(mpris --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player Metadata \
        | sed -n "s/.*trackid[^']*objectpath '\([^']*\)'.*/\1/p" | head -1)
    mpris --method org.mpris.MediaPlayer2.Player.Pause >/dev/null; sleep 0.4
    mpris --method org.mpris.MediaPlayer2.Player.SetPosition "${piste:-/org/rmplayer/track/0}" "$(( $1 * 1000 ))" >/dev/null
    sleep 1.0
}
position_ms() {
    mpris --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player Position \
        | sed -n 's/.*int64 \([0-9-]*\).*/\1/p' | head -1 | awk '{printf "%d", $1/1000}'
}
touche() { xdotool key --window "$ID" "$1" 2>/dev/null; sleep 1.0; }
# liste <fichier média> : positions (ms) des signets du média, séparées par des espaces
liste() {
    python3 - "$SIGNETS" "$M/$1" <<'PY'
import json, sys
try:
    d = json.load(open(sys.argv[1]))
except Exception:
    print(""); raise SystemExit
print(" ".join(str(s["ms"]) for s in d.get(sys.argv[2], [])))
PY
}
proche() { python3 -c "import sys; sys.exit(0 if abs($1 - $2) <= $3 else 1)"; }

echo "=== 0. Témoin : sans geste, pas de signet ==="
lance un.mp4; sleep 1
if [ -z "$(liste un.mp4)" ]; then ok "aucun signet avant tout geste"; else ko "signets présents sans geste : $(liste un.mp4)"; fi

echo "=== 1. Ajouter ==="
vers 5000; touche ctrl+b
vers 12000; touche ctrl+b
l=$(liste un.mp4); echo "  signets : ${l:-aucun}"
set -- $l
if [ $# -eq 2 ] && proche "$1" 5000 400 && proche "$2" 12000 400 && [ "$1" -lt "$2" ]; then
    ok "deux signets triés (≈ 5 s et ≈ 12 s)"
else ko "attendu deux signets ≈ 5000 et 12000, obtenu « $l »"; fi
touche ctrl+b
n=$(liste un.mp4 | wc -w)
if [ "$n" -eq 2 ]; then ok "un second ajout au même endroit n'ajoute rien"; else ko "$n signets après un doublon (attendu 2)"; fi

echo "=== 2. Emplacement ==="
if [ -s "$SIGNETS" ] && ! find "$XDG_CACHE_HOME" -name 'signets*' | grep -q .; then
    ok "signets.json est dans le dossier de configuration, pas dans le cache"
else ko "emplacement : config=$(ls "$SIGNETS" 2>&1 | head -1) cache=$(find "$XDG_CACHE_HOME" -name 'signets*')"; fi

echo "=== 3. Après redémarrage : suivant / précédent ==="
arreter
lance un.mp4; vers 0
touche ctrl+shift+Right; p1=$(position_ms)
touche ctrl+shift+Right; p2=$(position_ms)
touche ctrl+shift+Left;  p3=$(position_ms)
echo "  suivant → ${p1:-?} ms, suivant → ${p2:-?} ms, précédent → ${p3:-?} ms"
if proche "${p1:-0}" 5000 700 && proche "${p2:-0}" 12000 700 && proche "${p3:-0}" 5000 700; then
    ok "la position publiée saute à 5 s, 12 s, puis revient à 5 s"
else ko "positions ${p1:-?} / ${p2:-?} / ${p3:-?} (attendu ≈ 5000 / 12000 / 5000)"; fi

echo "=== 4. Supprimer ==="
vers 5000; touche ctrl+shift+b
l=$(liste un.mp4); echo "  signets : ${l:-aucun}"
set -- $l
if [ $# -eq 1 ] && proche "$1" 12000 400; then ok "le signet de 5 s est supprimé, celui de 12 s reste"
else ko "attendu un seul signet ≈ 12000, obtenu « $l »"; fi

echo "=== 5. Un autre média n'en a pas ==="
arreter
lance deux.mp4; vers 3000
touche ctrl+shift+Right; p=$(position_ms)
echo "  position après « suivant » : ${p:-?} ms"
if proche "${p:-0}" 3000 700 && [ -z "$(liste deux.mp4)" ]; then ok "sans signet, « suivant » ne bouge pas"
else ko "position ${p:-?} / signets « $(liste deux.mp4) »"; fi

echo "=== 6. Renommer un signet ==="
nom_du() { python3 - "$SIGNETS" "$M/$1" <<'PY'
import json, sys
try:
    d = json.load(open(sys.argv[1]))
except Exception:
    raise SystemExit
print(" | ".join(f'{s["ms"]}:{s["nom"]}' for s in d.get(sys.argv[2], [])))
PY
}
arreter
lance un.mp4
avant=$(nom_du un.mp4); echo "  avant : ${avant:-aucun}"
vers 12000; touche ctrl+shift+n; sleep 0.8
xdotool type --window "$ID" --delay 70 "Passage cle" 2>/dev/null; sleep 0.5
touche Return; sleep 0.8
apres=$(nom_du un.mp4); echo "  après : ${apres:-aucun}"
set -- $(echo "$apres" | tr -d ' |' | tr ':' ' ')
if echo "$apres" | grep -q "Passage cle" && ! echo "$avant" | grep -q "Passage cle"; then
    ok "le nom saisi est enregistré (avant : « $avant », après : « $apres »)"
else ko "nom attendu « Passage cle » : $apres"; fi
if [ "$(echo "$apres" | grep -o '^[0-9]*')" = "$(echo "$avant" | grep -o '^[0-9]*')" ] && [ "$(echo "$apres" | grep -o '|' | wc -l)" = "$(echo "$avant" | grep -o '|' | wc -l)" ]; then
    ok "la position du signet et leur nombre n'ont pas bougé"
else ko "position ou nombre modifiés : « $avant » → « $apres »"; fi
# Annuler : Échap
vers 12000; touche ctrl+shift+n; sleep 0.6
xdotool type --window "$ID" --delay 70 "Annule" 2>/dev/null; sleep 0.4
touche Escape; sleep 0.6
if echo "$(nom_du un.mp4)" | grep -q "Passage cle" && ! nom_du un.mp4 | grep -q "Annule"; then ok "Échap annule : le nom n'a pas changé"
else ko "après Échap : « $(nom_du un.mp4) »"; fi
# Aucun signet ici : rien n'est renommé ni créé
vers 3000; touche ctrl+shift+n; sleep 0.6
xdotool type --window "$ID" --delay 70 "Zzz" 2>/dev/null; sleep 0.4
touche Return; sleep 0.6
if ! nom_du un.mp4 | grep -q "Zzz" && [ "$(nom_du un.mp4 | grep -o '|' | wc -l)" = "0" ]; then ok "sans signet à la position, rien n'est renommé ni créé"
else ko "signets : « $(nom_du un.mp4) »"; fi

echo "=== 7. Pendant la saisie, les touches vont au champ ==="
nb_captures() { find "$BASE/captures" -type f | wc -l; }
rm -f "$BASE/captures"/*
touche s; sleep 1.0
temoin=$(nb_captures)
if [ "$temoin" -ge 1 ]; then ok "témoin : « s » hors saisie produit une capture ($temoin fichier)"; else ko "témoin : « s » n'a produit aucune capture (l'instrument ne voit pas la capture)"; fi
rm -f "$BASE/captures"/*
vers 12000; touche ctrl+shift+n; sleep 0.6
xdotool type --window "$ID" --delay 70 "Passage s" 2>/dev/null; sleep 0.6
touche Escape; sleep 0.5
pendant=$(nb_captures)
if [ "$pendant" -eq 0 ]; then ok "taper « Passage s » dans la boîte ne déclenche aucune capture"; else ko "$pendant capture(s) produite(s) pendant la saisie : les raccourcis ont reçu les lettres"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]

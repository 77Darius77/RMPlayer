#!/usr/bin/env bash
# verif-peripheriques.sh — choisir la sortie audio : le son arrive RÉELLEMENT sur le périphérique choisi.
#
# Principe : deux sorties PipeWire factices (sinks « null », créés puis retirés par le banc ; ils ne
# touchent ni aux enceintes ni à la sortie par défaut). On joue un film dont l'audio est une sinusoïde
# de 440 Hz et on ENREGISTRE le moniteur de chaque sortie : c'est là, et seulement là, qu'on voit où
# le son est allé. Le graphe PipeWire (`tools/pw-flux.py`) dit sur quelle sortie le flux est relié.
# Un réglage qui s'affiche dans un menu n'est pas une sortie qui change.
#
# Contrôles :
#   1. la liste du lecteur propose les sorties PipeWire par leur NOM lisible (« KP-Test-A »), et plus
#      seulement des noms ALSA bruts (`dmix:CARD=HDMI,DEV=3`) ;
#   2. réglage = sortie A → le 440 Hz est sur A, silence sur B ; témoin : réglage = B → l'inverse
#      (sans le témoin, « silence sur B » passerait aussi avec un instrument sourd) ;
#   3. changement EN COURS DE LECTURE (Ctrl+Maj+J) : le son passe de A à B sans rouvrir le film ;
#   4. retour au périphérique par défaut en cours de lecture : le flux rejoint la sortie par défaut
#      du système et quitte B ;
#   5. la mémoire de WirePlumber (qui retient la dernière destination d'une application) ne vole pas
#      le choix : après 4, relancer avec « par défaut » arrive bien sur la sortie par défaut ;
#   6. un ancien réglage ALSA (« pipewire ») fonctionne toujours ;
#   7. une sortie qui n'existe plus ne casse pas la lecture : repli sur la sortie par défaut ;
#   8. passer d'une sortie ALSA explicite à une autre en cours de lecture ne rend pas le film muet
#      (avant : la sortie était détruite, le son ne revenait qu'à la réouverture) ;
#   9. un clic sur la sortie dans le menu Audio la choisit (son mesuré, choix enregistré) ;
#  10. la sortie par défaut du système n'a pas bougé pendant tout le banc.
#
# Usage : tools/verif-peripheriques.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-peripheriques
KP=./target/release/kplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=kplayer=info

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
for outil in python3 ffmpeg xdotool gdbus pw-dump pw-record pactl; do
    command -v "$outil" >/dev/null || { echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }
pw-dump >/dev/null 2>&1 || { echo "PipeWire injoignable — banc non exécutable ici"; exit 2; }
export DISPLAY="$DISP"
xdpyinfo >/dev/null 2>&1 || { echo "écran $DISP indisponible — banc non exécutable ici"; exit 2; }

rm -rf "$BASE"; mkdir -p "$BASE/conf/kplayer" "$BASE/cache"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/kplayer/config.json"

arreter() {
    for pid in $(pgrep -x kplayer 2>/dev/null); do
        aff=$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | sed -n 's/^DISPLAY=//p')
        [ "$aff" = "$DISP" ] && kill "$pid" 2>/dev/null
    done
    sleep 0.6
}
MODULES=""
nettoyer() {
    arreter
    pkill -f "pw-record.*kp_test" 2>/dev/null
    for m in $MODULES; do pactl unload-module "$m" >/dev/null 2>&1; done
}
trap nettoyer EXIT

DEFAUT0=$(python3 tools/pw-flux.py defaut)
for n in a b; do
    m=$(pactl load-module module-null-sink "sink_name=kp_test_$n" "sink_properties=device.description=KP-Test-${n^^}" 2>/dev/null)
    [ -n "$m" ] && MODULES="$MODULES $m"
done
[ -n "$MODULES" ] || { echo "impossible de créer les sorties factices — banc non exécutable ici"; exit 2; }
sleep 0.5

# Un film dont l'audio est un 440 Hz très bas (la sortie par défaut est de vraies enceintes).
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "color=c=0x303060:s=320x180:r=10:d=60" -f lavfi -i "sine=f=440:d=60,volume=0.01" \
    -c:v libx264 -preset ultrafast -c:a aac -ac 2 -shortest -y "$BASE/film.mp4"

config() {  # config <périphérique>
    python3 - "$CONF" "$1" <<'PY'
import json, sys
json.dump({"video": {"hardware_decoding": False}, "audio": {"device": sys.argv[2], "volume_max": 150},
           "subtitles": {"auto_load": False}, "interface": {"show_osd": False},
           "playback": {"resume": False}, "playlist": []}, open(sys.argv[1], "w"), indent=2)
PY
}
mpris() { gdbus call --session --dest org.mpris.MediaPlayer2.kplayer --object-path /org/mpris/MediaPlayer2 "$@" 2>/dev/null; }
pos_ms() { mpris --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player Position \
    | sed -n 's/.*int64 \([0-9-]*\).*/\1/p' | head -1 | awk '{printf "%d", $1/1000}'; }
lance() {  # lance <périphérique>
    arreter
    config "$1"
    "$KP" --fullscreen "$BASE/film.mp4" >"$BASE/dernier.log" 2>&1 &
    K_PID=$!
    sleep 4.5
    K_ID=$(xdotool search --pid "$K_PID" --name KPlayer 2>/dev/null | tail -1)
    xdotool windowfocus "$K_ID" 2>/dev/null; sleep 0.3
}
touche() { xdotool key --window "$K_ID" "$1" 2>/dev/null; sleep 1.5; }
# enregistre <étiquette> : 2 s du moniteur de A et de B en même temps
enregistre() {
    local pids=""
    for n in a b; do
        rm -f "$BASE/$1-$n.wav"
        timeout 2.2 pw-record -P '{ stream.capture.sink=true }' --target "kp_test_$n" \
            --rate 48000 --channels 2 --format f32 "$BASE/$1-$n.wav" >/dev/null 2>&1 &
        pids="$pids $!"
    done
    wait $pids   # pas de « wait » nu : il attendrait aussi le lecteur, lancé en arrière-plan
}
# mesure <wav> → « fréquence niveau_max » (0 0 si vide)
mesure() {
    local f m
    f=$(python3 tools/analyse-audio.py freq "$1" 2>/dev/null | cut -d' ' -f1)
    m=$(python3 tools/analyse-audio.py max "$1" 2>/dev/null)
    echo "${f:-0} ${m:-0}"
}
# son_present <wav> : 440 Hz audibles ; silence <wav> : rien (un sink nul vide est un zéro exact)
# Le moniteur d'un sink nul atténue (≈ 0,0009 pour un fichier à 0,01) : le seuil de présence est
# bas, et le silence est un zéro exact — l'écart entre les deux est de plusieurs ordres de grandeur.
son_present() { set -- $(mesure "$1"); python3 -c "import sys; sys.exit(0 if abs($1 - 440) < 15 and $2 > 0.0003 else 1)"; }
silence() { set -- $(mesure "$1"); python3 -c "import sys; sys.exit(0 if $2 < 0.00003 else 1)"; }
sink_du_flux() { python3 tools/pw-flux.py sink "$K_PID"; }
etat_du_flux() { python3 tools/pw-flux.py etat "$K_PID"; }
verdict() {  # verdict <description> <wav-présent> <wav-silencieux>
    if son_present "$2" && silence "$3"; then ok "$1 ($(mesure "$2" | cut -d' ' -f1) Hz sur la bonne sortie, silence sur l'autre)"
    else ko "$1 : présent=[$(mesure "$2")] silence attendu=[$(mesure "$3")] (fréquence, niveau)"; fi
}

echo "=== 1. La liste propose des noms lisibles ==="
liste=$("$KP" --list-audio 2>/dev/null)
if echo "$liste" | grep -q "pw:kp_test_a" && echo "$liste" | grep -q "KP-Test-A"; then
    ok "« KP-Test-A » (pw:kp_test_a) est proposée par son nom lisible"
else
    ko "la liste ne contient pas la sortie PipeWire KP-Test-A ; elle contient $(echo "$liste" | wc -l) lignes, dont : $(echo "$liste" | head -3 | tr '\n' '|')"
fi

echo "=== 2. Réglage = A : le son est sur A ; témoin : réglage = B ==="
lance "pw:kp_test_a"; enregistre a1
verdict "réglage A" "$BASE/a1-a.wav" "$BASE/a1-b.wav"
echo "  sortie du flux : $(sink_du_flux)"
lance "pw:kp_test_b"; enregistre b1
verdict "témoin, réglage B" "$BASE/b1-b.wav" "$BASE/b1-a.wav"

echo "=== 3. Changement en cours de lecture (Ctrl+Maj+J) : A → B ==="
lance "pw:kp_test_a"; enregistre c1
if son_present "$BASE/c1-a.wav"; then ok "départ : le son est sur A"; else ko "départ : pas de son sur A"; fi
p0=$(pos_ms); touche ctrl+shift+j; enregistre c2; p1=$(pos_ms)
verdict "après Ctrl+Maj+J" "$BASE/c2-b.wav" "$BASE/c2-a.wav"
if [ "${p1:-0}" -gt "${p0:-0}" ]; then ok "le film n'a pas été rouvert : la lecture continue (${p0} → ${p1} ms)"; else ko "position ${p0} → ${p1} ms : la lecture n'avance plus"; fi

echo "=== 4. Retour au périphérique par défaut en cours de lecture ==="
# Précondition mesurée : le flux est bien sur B. Sans elle, « le flux est sur la sortie par défaut »
# passerait aussi quand rien n'a jamais été routé.
avant=$(sink_du_flux)
if [ "$avant" = "kp_test_b" ]; then ok "précondition : le flux est sur B"; else ko "précondition : le flux est sur « $avant » au lieu de kp_test_b"; fi
touche ctrl+shift+j; enregistre d1
defaut=$(python3 tools/pw-flux.py defaut); cible=$(sink_du_flux)
if [ "$cible" = "$defaut" ]; then ok "le flux est relié à la sortie par défaut du système ($defaut)"; else ko "flux relié à « $cible » au lieu de « $defaut »"; fi
if silence "$BASE/d1-b.wav" && silence "$BASE/d1-a.wav"; then ok "plus rien sur A ni sur B"; else ko "du son reste sur A ou B : $(mesure "$BASE/d1-a.wav") / $(mesure "$BASE/d1-b.wav")"; fi

echo "=== 5. La mémoire de WirePlumber ne vole pas « par défaut » ==="
# D'abord une séance routée vers B (WirePlumber retient la destination), puis « par défaut ».
lance "pw:kp_test_b"
avant=$(sink_du_flux)
if [ "$avant" = "kp_test_b" ]; then ok "précondition : séance routée vers B"; else ko "précondition : flux sur « $avant » au lieu de kp_test_b"; fi
lance ""
cible=$(sink_du_flux); defaut=$(python3 tools/pw-flux.py defaut)
if [ "$cible" = "$defaut" ]; then ok "relancé avec « par défaut » : flux sur $defaut"; else ko "relancé avec « par défaut », le flux est sur « $cible » (défaut : « $defaut »)"; fi

echo "=== 6. Ancien réglage ALSA conservé ==="
lance "pipewire"
etat=$(etat_du_flux); cible=$(sink_du_flux)
if [ "$etat" = "running" ] && [ -n "$cible" ] && [ "$cible" != "aucun" ]; then ok "réglage « pipewire » (ALSA) : flux $etat sur $cible"; else ko "réglage « pipewire » : état $etat, sortie $cible"; fi

echo "=== 7. Sortie disparue : repli sur la sortie par défaut ==="
lance "pw:n_existe_pas"
etat=$(etat_du_flux); cible=$(sink_du_flux); defaut=$(python3 tools/pw-flux.py defaut)
if kill -0 "$K_PID" 2>/dev/null && [ "$etat" = "running" ] && [ "$cible" = "$defaut" ]; then
    ok "lecture en cours, flux $etat sur la sortie par défaut"
else ko "état $etat, sortie $cible (défaut $defaut), lecteur vivant : $(kill -0 "$K_PID" 2>/dev/null && echo oui || echo non)"; fi

echo "=== 8. ALSA explicite → autre sortie en cours de lecture : le film n'est pas muet ==="
lance "pipewire"
p0=$(pos_ms); touche ctrl+shift+j; sleep 2; p1=$(pos_ms)
etat=$(etat_du_flux)
if [ "$etat" = "running" ] && [ "${p1:-0}" -gt $(( ${p0:-0} + 1000 )) ]; then
    ok "après le changement : flux $etat, position ${p0} → ${p1} ms"
else ko "après le changement : flux $etat, position ${p0} → ${p1} ms (film muet ou figé)"; fi

echo "=== 9. Choisir la sortie par un clic dans le menu Audio ==="
# Géométrie lue dans le journal (comme audit-menus) : aucune coordonnée devinée.
arreter; config "pw:kp_test_a"
RUST_LOG=kplayer=info,kinterface=debug "$KP" "$BASE/film.mp4" >"$BASE/menu.log" 2>&1 &
K_PID=$!
sleep 5
K_ID=$(xdotool search --pid "$K_PID" --name KPlayer 2>/dev/null | head -1)
xdotool windowmove "$K_ID" 0 0; xdotool windowsize "$K_ID" 1280 720; xdotool windowactivate --sync "$K_ID" 2>/dev/null
xdotool mousemove 640 400; sleep 1
centre_de() {  # centre_de <motif de ligne> <n-ième (0 = premier, -1 = dernier)> : centre du rectangle journalisé
    python3 - "$BASE/menu.log" "$1" "$2" <<'PY'
import re, sys
trouves = []
for ligne in open(sys.argv[1], encoding="utf-8", errors="replace"):
    ligne = re.sub(r"\x1b\[[0-9;]*m", "", ligne)
    if sys.argv[2] in ligne:
        m = re.search(r"rect \[\[([\d.]+) ([\d.]+)\] - \[([\d.]+) ([\d.]+)\]\]", ligne)
        if m:
            trouves.append(tuple(map(float, m.groups())))
if trouves:
    x0, y0, x1, y1 = trouves[int(sys.argv[3])]
    print(int((x0 + x1) / 2), int((y0 + y1) / 2))
PY
}
# « menu-titre » : Fichier, Lecture, Audio… dans cet ordre — le troisième est Audio.
read -r mx my <<<"$(python3 - "$BASE/menu.log" <<'PY'
import re, sys
r = []
for ligne in open(sys.argv[1], encoding="utf-8", errors="replace"):
    ligne = re.sub(r"\x1b\[[0-9;]*m", "", ligne)
    if "menu-titre rect" in ligne:
        m = re.search(r"rect \[\[([\d.]+) ([\d.]+)\] - \[([\d.]+) ([\d.]+)\]\]", ligne)
        r.append(tuple(map(float, m.groups())))
if len(r) > 2:
    x0, y0, x1, y1 = r[2]
    print(int((x0 + x1) / 2), int((y0 + y1) / 2))
PY
)"
if [ -n "${mx:-}" ]; then
    xdotool mousemove "$mx" "$my"; sleep 0.3; xdotool click 1; sleep 1.0
    read -r ex ey <<<"$(centre_de "KP-Test-B" -1)"
    if [ -n "${ex:-}" ]; then
        xdotool mousemove "$ex" "$ey"; sleep 0.3; xdotool click 1; sleep 1.2
        enregistre m1
        verdict "clic sur « KP-Test-B » dans le menu Audio" "$BASE/m1-b.wav" "$BASE/m1-a.wav"
        if python3 -c "import json,sys; sys.exit(0 if json.load(open('$CONF'))['audio']['device'] == 'pw:kp_test_b' else 1)"; then
            ok "le choix est enregistré dans la configuration (pw:kp_test_b)"
        else ko "configuration : $(python3 -c "import json; print(json.load(open('$CONF'))['audio']['device'])")"; fi
    else ko "l'entrée « KP-Test-B » n'apparaît pas dans le menu Audio (géométrie non journalisée)"; fi
else ko "titre du menu Audio introuvable dans le journal"; fi

echo "=== 10. La sortie par défaut du système n'a pas bougé ==="
arreter
fin=$(python3 tools/pw-flux.py defaut)
if [ "$fin" = "$DEFAUT0" ]; then ok "toujours $fin"; else ko "la sortie par défaut est passée de $DEFAUT0 à $fin"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]

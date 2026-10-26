#!/usr/bin/env bash
# verif-audio-externe.sh — piste audio EXTERNE : un fichier son (.mka, .ac3…) placé à côté du film
# remplace la piste du film, avec le bon alignement, aussi après un saut.
#
# Principe : un film de 10 s dont la piste vaut 300 Hz ; un `film.fr.mka` qui vaut 600 Hz pendant 5 s
# puis 900 Hz pendant 5 s (la bascule à 5 s prouve l'alignement dans le temps) ; un `film.ac3` à
# 1000 Hz (autre codec). La piste jouée se lit au SON (fréquence dominante), jamais dans un journal.
#
# Le film est toujours donné AVANT `--audio-file` : sans cela, tant que l'option n'existe pas, le fichier
# externe devient le média principal et le banc « réussit » en jouant le .mka seul (constaté à l'écriture).
#
# Contrôles :
#   A. `--dump-audio` (même chaîne que la lecture)
#      1. témoin : sans option, 300 Hz ;
#      2. `--audio-file film.fr.mka` : 600 Hz sur 0,5–4,5 s, 900 Hz sur 5,5–9,5 s, bascule à 5 s ± 150 ms ;
#      3. un AC-3 : 1000 Hz ;
#      4. fichier absent, ou sans piste audio : on garde la piste du film (300 Hz), sans planter ;
#      5. `--list-tracks` : les fichiers voisins sont proposés (index virtuels 1000+), le film garde sa piste.
#   B. lecture réelle sur une sortie PipeWire factice (le son qui sort vraiment)
#      6. témoin : sans fichier externe, après deux sauts, on entend 300 Hz (l'instrument voit une fréquence) ;
#      7. avec `--audio-file` : saut à 7 s → 900 Hz, saut à 1 s → 600 Hz (le fichier externe suit les sauts) ;
#      8. menu Audio : un clic sur le fichier voisin le fait entendre ; un clic sur la piste du film
#         le rend (retour à 300 Hz) ;
#      9. un rechargement du film (Ctrl+Maj+E change les enceintes) garde le fichier externe.
#
# Usage : tools/verif-audio-externe.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-audio-externe
KP=./target/release/rmplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=warn

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
for outil in python3 ffmpeg; do
    command -v "$outil" >/dev/null || { echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }

rm -rf "$BASE"; mkdir -p "$BASE/conf/rmplayer" "$BASE/cache" "$BASE/media"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/rmplayer/config.json"
M="$BASE/media"
FF() { env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error "$@"; }

# Le film : 10 s, piste interne à 300 Hz.
FF -f lavfi -i "color=c=0x202020:s=160x90:r=10:d=10" -f lavfi -i "sine=f=300:d=10,volume=0.3" \
   -c:v libx264 -preset ultrafast -c:a aac -ac 2 -shortest -y "$M/film.mkv"
# Le fichier voisin : 600 Hz puis 900 Hz, 5 s chacun.
FF -f lavfi -i "sine=f=600:d=5,volume=0.3" -f lavfi -i "sine=f=900:d=5,volume=0.3" \
   -filter_complex "[0][1]concat=n=2:v=0:a=1" -c:a aac -ac 2 -y "$M/film.fr.mka"
# Un autre codec : AC-3 à 1000 Hz.
FF -f lavfi -i "sine=f=1000:d=10,volume=0.3" -c:a ac3 -ac 2 -y "$M/film.ac3"
# Un fichier sans piste audio (image seule).
FF -f lavfi -i "color=c=black:s=64x64:r=5:d=2" -c:v libx264 -preset ultrafast -y "$M/muet.mkv"
# Un film qui n'a pas de voisins, pour vérifier qu'il n'en invente pas.
FF -f lavfi -i "color=c=0x202020:s=160x90:r=10:d=3" -f lavfi -i "sine=f=300:d=3,volume=0.3" \
   -c:v libx264 -preset ultrafast -c:a aac -shortest -y "$BASE/seul.mkv"

cat > "$CONF" <<'JSON'
{"video": {"hardware_decoding": false}, "audio": {"device": "", "volume_max": 150},
 "subtitles": {"auto_load": false}, "interface": {"show_osd": false}, "playback": {"resume": false}, "playlist": []}
JSON

ANALYSE="python3 tools/analyse-audio.py"
proche() { awk "BEGIN{exit !($1>$2-$3 && $1<$2+$3)}"; }
dump() {  # dump <sortie.wav> <arguments…> : --dump-audio avec arguments éventuels
    local sortie="$1"; shift
    rm -f "$sortie"
    timeout 60 "$KP" --dump-audio "$sortie" "$@" >"$BASE/dump.log" 2>&1
}

echo "=== A. Le son décodé (--dump-audio) ==="
echo "--- 1. témoin : la piste du film"
dump "$BASE/a1.wav" "$M/film.mkv"
f=$($ANALYSE freqfen "$BASE/a1.wav" 1000 8000)
if proche "$f" 300 15; then ok "sans fichier externe : $f Hz"; else ko "piste du film : $f Hz (attendu 300)"; fi

echo "--- 2. un .mka voisin, avec bascule à 5 s"
dump "$BASE/a2.wav" "$M/film.mkv" --audio-file "$M/film.fr.mka"
f1=$($ANALYSE freqfen "$BASE/a2.wav" 500 4500); f2=$($ANALYSE freqfen "$BASE/a2.wav" 5500 9500)
b=$($ANALYSE bascule "$BASE/a2.wav" 600 900)
if proche "$f1" 600 15 && proche "$f2" 900 15; then ok "0,5–4,5 s : $f1 Hz ; 5,5–9,5 s : $f2 Hz"; else ko "fréquences $f1 / $f2 (attendu 600 / 900)"; fi
if [ "$b" != "-1" ] && proche "$b" 5000 150; then ok "bascule 600 → 900 Hz à $b ms (alignement correct)"; else ko "bascule à $b ms (attendu 5000 ± 150)"; fi

echo "--- 3. un autre codec (AC-3)"
dump "$BASE/a3.wav" "$M/film.mkv" --audio-file "$M/film.ac3"
f=$($ANALYSE freqfen "$BASE/a3.wav" 1000 8000)
if proche "$f" 1000 15; then ok "AC-3 : $f Hz"; else ko "AC-3 : $f Hz (attendu 1000)"; fi

echo "--- 4. fichiers inutilisables : on garde la piste du film"
dump "$BASE/a4.wav" "$M/film.mkv" --audio-file "$M/inexistant.mka"; code=$?
f=$($ANALYSE freqfen "$BASE/a4.wav" 1000 8000 2>/dev/null)
if proche "${f:--1}" 300 15; then ok "fichier absent : le film garde sa piste ($f Hz)"; else ko "fichier absent : $f Hz (attendu 300)"; fi
dump "$BASE/a4b.wav" "$M/film.mkv" --audio-file "$M/muet.mkv"
f=$($ANALYSE freqfen "$BASE/a4b.wav" 1000 8000 2>/dev/null)
if proche "${f:--1}" 300 15; then ok "fichier sans audio : le film garde sa piste ($f Hz)"; else ko "fichier sans audio : $f Hz (attendu 300)"; fi

echo "--- 5. les fichiers voisins sont proposés"
liste=$("$KP" --list-tracks "$M/film.mkv" 2>/dev/null)
echo "$liste" | grep -q "flux 1000.*film\.ac3" && echo "$liste" | grep -q "flux 1001.*film\.fr\.mka" \
    && ok "pistes externes listées (1000 : film.ac3, 1001 : film.fr.mka)" || ko "liste : $(echo "$liste" | grep audio | tr '\n' '|')"
echo "$liste" | grep -q "audio\[0\] flux 1 \|audio\[0\] flux 1 :" && ok "la piste du film reste la première" \
    || ko "piste du film absente de la liste : $(echo "$liste" | grep audio | head -1)"
seul=$("$KP" --list-tracks "$BASE/seul.mkv" 2>/dev/null | grep -c "flux 1[0-9][0-9][0-9]")
[ "$seul" = "0" ] && ok "un film sans voisin n'en reçoit pas" || ko "pistes externes inventées : $seul"

echo
echo "=== B. Le son qui sort (sortie PipeWire factice) ==="
for outil in xdotool gdbus pw-record pactl; do
    command -v "$outil" >/dev/null || { echo "  · outil manquant : $outil — partie B ignorée"; echo; echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="; [ "$FAIL" -eq 0 ]; exit; }
done
export DISPLAY="$DISP"
xdpyinfo >/dev/null 2>&1 || { echo "  · écran $DISP indisponible — partie B ignorée"; echo; echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="; [ "$FAIL" -eq 0 ]; exit; }
arreter() {
    for pid in $(pgrep -x rmplayer 2>/dev/null); do
        aff=$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | sed -n 's/^DISPLAY=//p')
        [ "$aff" = "$DISP" ] && kill "$pid" 2>/dev/null
    done
    sleep 0.6
}
MODULE=""
nettoyer() { arreter; [ -n "$MODULE" ] && pactl unload-module "$MODULE" >/dev/null 2>&1; }
trap nettoyer EXIT
MODULE=$(pactl load-module module-null-sink sink_name=kp_test_ext channels=2 \
    "sink_properties=device.description=KP-externe" 2>/dev/null)
[ -n "$MODULE" ] || { echo "  · impossible de créer la sortie factice — partie B ignorée"; exit 2; }
sleep 0.5
cat > "$CONF" <<'JSON'
{"video": {"hardware_decoding": false}, "audio": {"device": "pw:kp_test_ext", "volume_max": 150},
 "subtitles": {"auto_load": false}, "interface": {"show_osd": false}, "playback": {"resume": false}, "playlist": []}
JSON
enregistre() {  # enregistre <fichier.wav> : 1,6 s de la sortie factice
    rm -f "$1"
    timeout 1.6 pw-record -P '{ stream.capture.sink=true }' --target kp_test_ext --rate 48000 --channels 2 \
        --format f32 "$1" >/dev/null 2>&1
}
mpris() { gdbus call --session --dest org.mpris.MediaPlayer2.rmplayer --object-path /org/mpris/MediaPlayer2 "$@" 2>/dev/null; }
piste_mpris() { local p; p=$(mpris --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player Metadata \
    | sed -n "s/.*trackid[^']*objectpath '\([^']*\)'.*/\1/p" | head -1); echo "${p:-/org/rmplayer/track/0}"; }
position_ms() { mpris --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player Position \
    | sed -n 's/.*int64 \([0-9-]*\).*/\1/p' | awk '{printf "%d", $1/1000}'; }
sauter() {  # sauter <ms> : le saut est CONTRÔLÉ (position publiée), sinon tout ce qui suit ne prouve rien
    mpris --method org.mpris.MediaPlayer2.Player.SetPosition "$(piste_mpris)" "$(( $1 * 1000 ))" >/dev/null
    sleep 0.3
    local p; p=$(position_ms)
    if [ -z "$p" ] || [ "$p" -lt $(( $1 - 300 )) ] || [ "$p" -gt $(( $1 + 1500 )) ]; then
        ko "le saut à $1 ms n'a pas eu lieu (position publiée : ${p:-?} ms)"
    fi
}
ecoute() {  # ecoute <fichier.wav> : fréquence dominante du moment
    $ANALYSE freqfen "$1" 200 1400 2>/dev/null
}

echo "--- 6. témoin : sans fichier externe, après deux sauts"
"$KP" --speed 1 "$M/film.mkv" >"$BASE/b6.log" 2>&1 &
K_PID=$!
sleep 3.5
sauter 7000; sleep 0.6; enregistre "$BASE/b6a.wav"; fa=$(ecoute "$BASE/b6a.wav")
sauter 1000; sleep 0.6; enregistre "$BASE/b6b.wav"; fb=$(ecoute "$BASE/b6b.wav")
if proche "${fa:--1}" 300 15 && proche "${fb:--1}" 300 15; then ok "piste du film : $fa Hz puis $fb Hz (l'instrument entend)"; else ko "témoin : $fa / $fb Hz (attendu 300 / 300)"; fi
kill "$K_PID" 2>/dev/null; wait "$K_PID" 2>/dev/null

echo "--- 7. avec --audio-file, le fichier externe suit les sauts"
"$KP" --speed 1 "$M/film.mkv" --audio-file "$M/film.fr.mka" >"$BASE/b7.log" 2>&1 &
K_PID=$!
sleep 3.5
sauter 7000; sleep 0.6; enregistre "$BASE/b7a.wav"; fa=$(ecoute "$BASE/b7a.wav")
sauter 1000; sleep 0.6; enregistre "$BASE/b7b.wav"; fb=$(ecoute "$BASE/b7b.wav")
if proche "${fa:--1}" 900 15; then ok "saut à 7 s : $fa Hz (2e moitié du fichier externe)"; else ko "saut à 7 s : $fa Hz (attendu 900)"; fi
if proche "${fb:--1}" 600 15; then ok "saut à 1 s : $fb Hz (1re moitié du fichier externe)"; else ko "saut à 1 s : $fb Hz (attendu 600)"; fi
kill "$K_PID" 2>/dev/null; wait "$K_PID" 2>/dev/null

echo "--- 8. menu Audio : choisir le fichier voisin, puis revenir à la piste du film"
export RUST_LOG=rmplayer=info,kinterface=debug
"$KP" --speed 1 "$M/film.mkv" >"$BASE/b8.log" 2>&1 &
K_PID=$!
sleep 4
WID=$(xdotool search --pid "$K_PID" --name RMPlayer 2>/dev/null | head -1)
if [ -z "$WID" ]; then ko "fenêtre introuvable"; else
    xdotool windowmove "$WID" 0 0; xdotool windowsize "$WID" 1280 720
    xdotool windowactivate --sync "$WID" 2>/dev/null || true
    sleep 1; xdotool mousemove 640 400; sleep 0.6
    propre() { sed 's/\x1b\[[0-9;]*m//g' "$BASE/b8.log"; }
    titre_menu() { propre | grep -a "menu-titre rect" | tail -6 | sed -n "$(( $1 + 1 ))p" | python3 -c "
import re, sys
m = re.findall(r'-?[\d.]+', sys.stdin.read().split('rect')[1]); x0, y0, x1, y1 = map(float, m[:4]); print(int((x0+x1)/2), int((y0+y1)/2))"; }
    piste_menu() { propre | grep -a "menu audio piste $1 rect" | tail -1 | python3 -c "
import re, sys
l = sys.stdin.read()
if not l.strip(): sys.exit(1)
m = re.findall(r'-?[\d.]+', l.split('rect')[1]); x0, y0, x1, y1 = map(float, m[:4]); print(int((x0+x1)/2), int((y0+y1)/2))"; }
    cliquer() { xdotool mousemove "$1" "$2"; sleep 0.25; xdotool click 1; sleep 0.9; }
    choisir() {  # choisir <rang de piste> : ouvre le menu Audio et clique la piste (deux essais : le premier
        # clic tombe parfois avant que la fenêtre ait le focus quand la machine est chargée)
        for essai in 1 2; do
            read -r cx cy <<<"$(titre_menu 2)"; cliquer "$cx" "$cy"
            if pos=$(piste_menu "$1"); then read -r ex ey <<<"$pos"; cliquer "$ex" "$ey"; return 0; fi
            xdotool key Escape; sleep 0.5
        done
        return 1
    }
    sauter 1000; sleep 0.4
    if choisir 2; then
        sauter 1000; sleep 0.6; enregistre "$BASE/b8a.wav"; fa=$(ecoute "$BASE/b8a.wav")
        if proche "${fa:--1}" 600 15; then ok "clic sur la piste externe film.fr.mka : $fa Hz"; else ko "après le clic : $fa Hz (attendu 600)"; fi
    else ko "entrée de menu pour la piste 2 introuvable"; fi
    xdotool key Escape; sleep 0.4
    if choisir 0; then
        sauter 1000; sleep 0.6; enregistre "$BASE/b8b.wav"; fb=$(ecoute "$BASE/b8b.wav")
        if proche "${fb:--1}" 300 15; then ok "retour à la piste du film : $fb Hz"; else ko "retour : $fb Hz (attendu 300)"; fi
    else ko "entrée de menu pour la piste 0 introuvable"; fi
fi
kill "$K_PID" 2>/dev/null; wait "$K_PID" 2>/dev/null
export RUST_LOG=rmplayer=warn

echo "--- 9. un rechargement du film (changement d'enceintes) garde la piste externe"
export RUST_LOG=rmplayer=info
"$KP" --speed 1 "$M/film.mkv" --audio-file "$M/film.fr.mka" >"$BASE/b9.log" 2>&1 &
K_PID=$!
sleep 3.5
K_ID=$(xdotool search --pid "$K_PID" --name RMPlayer 2>/dev/null | tail -1)
xdotool windowfocus "$K_ID" 2>/dev/null; sleep 0.3
xdotool key --window "$K_ID" ctrl+shift+e 2>/dev/null; sleep 2.5
charges=$(sed 's/\x1b\[[0-9;]*m//g' "$BASE/b9.log" | grep -ac "Chargement du média")
sauter 1000; sleep 0.6; enregistre "$BASE/b9.wav"; f=$(ecoute "$BASE/b9.wav")
if [ "$charges" -ge 2 ]; then
    if proche "${f:--1}" 600 15; then ok "après $charges chargements du film, le son vient toujours du fichier externe ($f Hz)"
    else ko "après le rechargement : $f Hz (attendu 600, le film seul donnerait 300)"; fi
else ko "le film n'a pas été rechargé ($charges chargement) : le contrôle ne prouve rien"; fi
kill "$K_PID" 2>/dev/null; wait "$K_PID" 2>/dev/null
export RUST_LOG=rmplayer=warn

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]

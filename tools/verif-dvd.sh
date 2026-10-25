#!/usr/bin/env bash
# verif-dvd.sh — ouvrir un DVD-Vidéo : dossier VIDEO_TS, fichiers .ifo / .vob, lecteur ; titres, chapitres,
# langues, positions, protection CSS. On mesure des images, du son et des positions, pas des lignes de journal.
#
# Un DVD de test est fabriqué (`tools/fabrique-dvd.py`) : un film de 20 s en trois plans de couleur
# (rouge 0–8 s, vert 8–15 s, bleu 15–20 s), deux pistes AC-3 (440 Hz « fre », 880 Hz « jpn »), trois
# cellules et trois chapitres ; un 2e titre = la dernière cellule seule (bleue, 5 s). Les décalages
# d'IFO sont ceux que `lsdvd` confirme sur un vrai disque (contrôle 8).
#
# Contrôles :
#   1. `--list-dvd` : 2 titres, durées 20 s et 5 s, 3 puis 1 chapitres, 3 cellules, pistes fre/jpn ;
#   2. le même DVD est reconnu par le dossier, son sous-dossier VIDEO_TS, VIDEO_TS.IFO, VTS_01_0.IFO et
#      VTS_01_1.VOB ; un dossier quelconque n'est pas un DVD ;
#   3. images aux positions 4 / 12 / 18 s : rouge, vert, bleu (le film se lit à travers les cellules) ;
#   4. le début d'un titre n'est pas à zéro dans le flux (≈ 0,5 s) : sauter à 7,7 s montre encore du rouge,
#      à 8,3 s déjà du vert, à 14,7 s du vert, à 15,3 s du bleu (les positions sont celles du titre) ;
#   5. le titre 2 (`dvd://…#2`) commence directement dans le bleu et dure 5 s, pas 20 ;
#   6. audio : 440 Hz sur 20 s pour le titre 1 ; la piste 880 Hz se choisit ; les langues du disque
#      (« fre », « jpn ») figurent dans l'inventaire des pistes ;
#   7. fenêtre réelle : durée 20 s, lecture en cours, SetPosition 8,3 s → l'écran est vert, 15,3 s → bleu ;
#   7b. menus : les entrées « Ouvrir un DVD » existent ; avec un DVD ouvert, Lecture › Titres du DVD liste
#      les titres et un clic sur le titre 2 charge ce titre (durée 5 s publiée) ;
#   8. disque chiffré (bits de brouillage posés dans les paquets) : refus clair mentionnant CSS, sans plantage ;
#   9. vrai disque (`/dev/sr0`, `lsdvd`) s'il y en a un : mêmes titres, durées, chapitres, cellules et pistes que
#      lsdvd, et le film chiffré est refusé avec le message CSS (rien n'est déchiffré). Sinon : ignoré.
#
# Usage : tools/verif-dvd.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-dvd-bench
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
ls /usr/lib/x86_64-linux-gnu/libdvdread.so* >/dev/null 2>&1 || { echo "libdvdread absente — banc non exécutable ici"; exit 2; }
export DISPLAY="$DISP"
xdpyinfo >/dev/null 2>&1 || { echo "écran $DISP indisponible — banc non exécutable ici"; exit 2; }
if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ] && [ -S "$XDG_RUNTIME_DIR/bus" ]; then
    export DBUS_SESSION_BUS_ADDRESS="unix:path=$XDG_RUNTIME_DIR/bus"
fi

rm -rf "$BASE"; mkdir -p "$BASE/conf/rmplayer" "$BASE/cache"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/rmplayer/config.json"
arreter() {
    for pid in $(pgrep -x rmplayer 2>/dev/null); do
        aff=$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | sed -n 's/^DISPLAY=//p')
        [ "$aff" = "$DISP" ] && kill "$pid" 2>/dev/null
    done
    sleep 0.6
}
trap arreter EXIT
arreter
python3 - "$CONF" <<'PY'
import json, sys
json.dump({"video": {"hardware_decoding": False}, "audio": {"device": ""},
           "subtitles": {"auto_load": False}, "interface": {"show_osd": False},
           "playback": {"resume": False}, "playlist": []}, open(sys.argv[1], "w"), indent=2)
PY

# --- Fabrication du DVD de test ---------------------------------------------------------------------
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -y \
    -filter_complex "color=c=red:size=720x576:rate=25:duration=8,format=yuv420p[a];color=c=green:size=720x576:rate=25:duration=7,format=yuv420p[b];color=c=blue:size=720x576:rate=25:duration=5,format=yuv420p[c];[a][b][c]concat=n=3:v=1[v]" \
    -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=20" \
    -f lavfi -i "sine=frequency=880:sample_rate=48000:duration=20" \
    -map "[v]" -map 0:a -map 1:a -c:v mpeg2video -g 12 -b:v 3000k -maxrate 8000k -bufsize 1835k \
    -c:a ac3 -b:a 192k -ac 2 -f dvd "$BASE/film.vob"
python3 -I tools/fabrique-dvd.py "$BASE/dvd" "$BASE/film.vob" >/dev/null || { echo "DVD de test non fabriqué"; exit 2; }
DVD="$BASE/dvd"; TS="$DVD/VIDEO_TS"

# pixel central d'une image PNG : « r g b »
centre() {
    python3 - "$1" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert("RGB")
print(*im.getpixel((im.width // 2, im.height // 2)))
PY
}
# classe un pixel : rouge / vert / bleu / ?
couleur() {
    read -r r g b <<<"$1"
    if [ "$r" -gt 180 ] && [ "$g" -lt 90 ] && [ "$b" -lt 90 ]; then echo rouge
    elif [ "$g" -gt 90 ] && [ "$r" -lt 90 ] && [ "$b" -lt 90 ]; then echo vert
    elif [ "$b" -gt 180 ] && [ "$r" -lt 90 ] && [ "$g" -lt 90 ]; then echo bleu
    else echo "?($1)"; fi
}
# image <ms> <média> → couleur dominante
image_a() {
    rm -f "$BASE/i.png"
    timeout 60 "$KP" --dump-frame "$1" "$BASE/i.png" "$2" >/dev/null 2>&1
    [ -s "$BASE/i.png" ] && couleur "$(centre "$BASE/i.png")" || echo "aucune image"
}

echo "=== 1. Titres, chapitres, cellules, pistes ==="
LISTE=$("$KP" --list-dvd "$DVD" 2>/dev/null)
echo "$LISTE" | sed 's/^/    /'
echo "$LISTE" | grep -q "^Title: 01, Length: 00:00:20.000, Chapters: 03, Cells: 03, Audio streams: 02, Subpictures: 00" \
    && ok "titre 1 : 20 s, 3 chapitres, 3 cellules, 2 pistes audio" || ko "titre 1 incorrect"
echo "$LISTE" | grep -q "^Title: 02, Length: 00:00:05.000, Chapters: 01, Cells: 01, Audio streams: 02" \
    && ok "titre 2 : 5 s, 1 chapitre, 1 cellule" || ko "titre 2 incorrect"
echo "$LISTE" | grep -q "Audio: 1, Language: fre, Format: AC-3.*0x80" && echo "$LISTE" | grep -q "Audio: 2, Language: jpn, Format: AC-3.*0x81" \
    && ok "pistes audio : fre (0x80) et jpn (0x81)" || ko "langues des pistes audio"
echo "$LISTE" | grep -q "Chapter: 02, Start: 00:00:08.000" && echo "$LISTE" | grep -q "Chapter: 03, Start: 00:00:15.000" \
    && ok "chapitres à 0 / 8 / 15 s" || ko "début des chapitres"
echo "$LISTE" | grep -q "^Longest track: 01" && ok "titre principal = le plus long (1)" || ko "titre principal"

echo "=== 2. Reconnaissance par dossier et par fichier ==="
reco() { "$KP" --list-dvd "$1" 2>/dev/null | grep -c "^Title:"; }
for chemin in "$DVD" "$TS" "$TS/VIDEO_TS.IFO" "$TS/VTS_01_0.IFO" "$TS/VTS_01_1.VOB" "$TS/VIDEO_TS.BUP"; do
    n=$(reco "$chemin")
    [ "$n" = "2" ] && ok "reconnu : ${chemin#$BASE/}" || ko "non reconnu : ${chemin#$BASE/} ($n titre(s))"
done
mkdir -p "$BASE/vide"
"$KP" --list-dvd "$BASE/vide" >/dev/null 2>&1 && ko "un dossier vide serait un DVD" || ok "un dossier quelconque n'est pas un DVD"
cp "$TS/VTS_01_1.VOB" "$BASE/seul.vob"
"$KP" --list-dvd "$BASE/seul.vob" >/dev/null 2>&1 && ko "un .vob isolé serait un DVD" || ok "un .vob isolé reste un fichier ordinaire"

echo "=== 3. Le film se lit à travers les cellules ==="
for t in 4000:rouge 12000:vert 18000:bleu; do
    c=$(image_a "${t%%:*}" "$DVD")
    [ "$c" = "${t##*:}" ] && ok "à ${t%%:*} ms : $c" || ko "à ${t%%:*} ms : $c (attendu ${t##*:})"
done
c=$(image_a 12000 "$TS/VTS_01_1.VOB")
[ "$c" = "vert" ] && ok "ouvrir par le fichier VTS_01_1.VOB lit le film (vert à 12 s)" || ko "VTS_01_1.VOB : $c"

echo "=== 4. Les positions sont celles du titre (début du flux ≠ 0) ==="
for t in 7700:rouge 8300:vert 14700:vert 15300:bleu; do
    c=$(image_a "${t%%:*}" "$DVD")
    [ "$c" = "${t##*:}" ] && ok "à ${t%%:*} ms : $c" || ko "à ${t%%:*} ms : $c (attendu ${t##*:} — début du flux non retranché ?)"
done

echo "=== 5. Choix du titre ==="
c=$(image_a 500 "dvd://$DVD#2")
[ "$c" = "bleu" ] && ok "titre 2 : il commence dans le bleu" || ko "titre 2 : $c"
c=$(image_a 7000 "dvd://$DVD#2")
[ "$c" = "aucune image" ] && ok "titre 2 : rien au-delà de sa durée (5 s)" || ko "titre 2 à 7 s : $c (attendu : fin du titre)"
"$KP" --dump-frame 500 "$BASE/x.png" "dvd://$DVD#9" >"$BASE/t9.log" 2>&1
grep -q "pas de titre 9" "$BASE/t9.log" && ok "titre inexistant : message clair" || ko "titre 9 : $(tail -2 "$BASE/t9.log")"

echo "=== 6. Audio et langues ==="
timeout 60 "$KP" --dump-audio "$BASE/a1.wav" "$DVD" >"$BASE/a1.log" 2>&1
read -r f d <<<"$(python3 tools/analyse-audio.py freq "$BASE/a1.wav" 2>/dev/null)"
[ "$f" = "440.0" ] && awk "BEGIN{exit !($d>19.5 && $d<20.5)}" && ok "titre 1 : 440 Hz pendant $d s" || ko "titre 1 : $f Hz, $d s"
timeout 60 "$KP" --dump-audio "$BASE/a2.wav" --audio-track 3 "$DVD" >/dev/null 2>&1
read -r f d <<<"$(python3 tools/analyse-audio.py freq "$BASE/a2.wav" 2>/dev/null)"
[ "$f" = "880.0" ] && ok "la 2e piste audio (flux 3) donne 880 Hz" || ko "2e piste : $f Hz"
sed 's/\x1b\[[0-9;]*m//g' "$BASE/a1.log" | grep -q "Piste audio 0 : fre" && sed 's/\x1b\[[0-9;]*m//g' "$BASE/a1.log" | grep -q "Piste audio 1 : jpn" \
    && ok "inventaire des pistes : fre puis jpn (langues lues dans l'IFO)" || ko "langues absentes de l'inventaire"
timeout 60 "$KP" --dump-audio "$BASE/a3.wav" "dvd://$DVD#2" >/dev/null 2>&1
read -r f d <<<"$(python3 tools/analyse-audio.py freq "$BASE/a3.wav" 2>/dev/null)"
awk "BEGIN{exit !($d>4.5 && $d<6.5)}" && ok "titre 2 : $d s d'audio (la dernière cellule seule)" || ko "titre 2 : $d s d'audio"

echo "=== 7. Fenêtre réelle ==="
mpris() { gdbus call --session --dest org.mpris.MediaPlayer2.rmplayer --object-path /org/mpris/MediaPlayer2 "$@" 2>/dev/null; }
meta() { mpris --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player Metadata; }
etat() { mpris --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player PlaybackStatus | sed -n "s/.*'\([A-Za-z]*\)'.*/\1/p"; }
position_ms() { mpris --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player Position | sed -n 's/.*int64 \([0-9-]*\).*/\1/p' | awk '{printf "%d", $1/1000}'; }
"$KP" --fullscreen "$DVD" >"$BASE/fenetre.log" 2>&1 &
K_PID=$!
sleep 3.5
long=$(meta | sed -n "s/.*mpris:length.*int64 \([0-9]*\).*/\1/p" | head -1)
[ -n "$long" ] && awk "BEGIN{exit !($long>19000000 && $long<21000000)}" && ok "durée publiée : $((long/1000)) ms" || ko "durée publiée : $long µs"
[ "$(etat)" = "Playing" ] && ok "lecture en cours" || ko "état : $(etat)"
titre=$(meta | sed -n "s/.*xesam:title'[^']*'\([^']*\)'.*/\1/p" | head -1)
echo "$titre" | grep -q "DVD" && ok "titre affiché : « $titre »" || ko "titre affiché : « $titre »"
PISTE=$(meta | sed -n "s/.*trackid[^']*objectpath '\([^']*\)'.*/\1/p" | head -1)
grab() { env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x720 -i "$DISP+0,0" -frames:v 1 -y "$BASE/ecran.png"; }
for t in 8300:vert 15300:bleu 3000:rouge; do
    mpris --method org.mpris.MediaPlayer2.Player.SetPosition "${PISTE:-/org/rmplayer/track/0}" "$(( ${t%%:*} * 1000 ))" >/dev/null
    sleep 1.2
    grab
    c=$(couleur "$(centre "$BASE/ecran.png")")
    p=$(position_ms)
    if [ "$c" = "${t##*:}" ] && [ -n "$p" ] && [ "$p" -ge $(( ${t%%:*} - 200 )) ] && [ "$p" -le $(( ${t%%:*} + 2500 )) ]; then
        ok "SetPosition ${t%%:*} ms : écran ${c}, position publiée $p ms"
    else ko "SetPosition ${t%%:*} ms : écran $c (attendu ${t##*:}), position $p ms"; fi
done
kill "$K_PID" 2>/dev/null; wait "$K_PID" 2>/dev/null

echo "=== 7b. Menus : ouvrir un DVD, titres du DVD ==="
export RUST_LOG=rmplayer=info,kinterface=debug
"$KP" "$DVD" >"$BASE/menus.log" 2>&1 &
K_PID=$!
sleep 4
WID=$(xdotool search --pid "$K_PID" --name RMPlayer 2>/dev/null | head -1)
if [ -z "$WID" ]; then ko "fenêtre introuvable"; else
    xdotool windowmove "$WID" 0 0; xdotool windowsize "$WID" 1280 720
    xdotool windowactivate --sync "$WID" 2>/dev/null || true
    sleep 1; xdotool mousemove 640 400; sleep 0.6
    propre() { sed 's/\x1b\[[0-9;]*m//g' "$BASE/menus.log"; }
    # centre du n-ième (0 = Fichier, 1 = Lecture) des six derniers titres de menu journalisés
    titre_menu() { propre | grep -a "menu-titre rect" | tail -6 | sed -n "$(( $1 + 1 ))p" | python3 -c "
import re, sys
m = re.findall(r'-?[\d.]+', sys.stdin.read().split('rect')[1]); x0, y0, x1, y1 = map(float, m[:4]); print(int((x0+x1)/2), int((y0+y1)/2))"; }
    # centre de la dernière entrée de menu dont le libellé contient « $1 »
    entree() { propre | grep -a "menu-item « .*$1" | tail -1 | python3 -c "
import re, sys
l = sys.stdin.read()
if not l.strip(): sys.exit(1)
m = re.findall(r'-?[\d.]+', l.split('rect')[1]); x0, y0, x1, y1 = map(float, m[:4]); print(int((x0+x1)/2), int((y0+y1)/2))"; }
    # egui veut voir le pointeur survoler avant le clic
    cliquer() { xdotool mousemove "$1" "$2"; sleep 0.25; xdotool click 1; sleep 0.9; }
    # Fichier
    read -r cx cy <<<"$(titre_menu 0)"; cliquer "$cx" "$cy"
    for lib in "Ouvrir un DVD (dossier)" "Ouvrir un DVD (.ifo, .iso)" "Lire le DVD du lecteur"; do
        propre | grep -a "menu-item « .*$lib" >/dev/null && ok "menu Fichier : « $lib »" || ko "menu Fichier : « $lib » absent"
    done
    cliquer 640 500; xdotool key Escape; sleep 0.4
    # Lecture → Titres du DVD → Titre 2
    read -r cx cy <<<"$(titre_menu 1)"; cliquer "$cx" "$cy"
    if pos=$(entree "Titres du DVD (1/2)"); then
        ok "menu Lecture : « Titres du DVD (1/2) » présent (titre 1 en cours)"
        read -r ex ey <<<"$pos"; cliquer "$ex" "$ey"
        propre | grep -a "menu-item « ● Titre 1" >/dev/null && ok "liste des titres : le titre 1 est marqué actif" || ko "titre 1 non marqué actif"
        if pos2=$(entree "  Titre 2"); then
            read -r ex ey <<<"$pos2"; cliquer "$ex" "$ey"; sleep 2.5
            long=$(meta | sed -n "s/.*mpris:length.*int64 \([0-9]*\).*/\1/p" | head -1)
            titre=$(meta | sed -n "s/.*xesam:title'[^']*'\([^']*\)'.*/\1/p" | head -1)
            if echo "$titre" | grep -q "titre 2" && [ -n "$long" ] && [ "$long" -lt 8000000 ]; then ok "clic sur « Titre 2 » : « $titre », durée $((long/1000)) ms"
            else ko "clic sur « Titre 2 » : titre « $titre », durée $long µs"; fi
        else ko "entrée « Titre 2 » absente de la liste"; fi
    else ko "menu Lecture : « Titres du DVD » absent"; fi
fi
kill "$K_PID" 2>/dev/null; wait "$K_PID" 2>/dev/null
export RUST_LOG=rmplayer=info

echo "=== 8. Disque chiffré (CSS) ==="
rm -rf "$BASE/chiffre"; cp -r "$DVD" "$BASE/chiffre"
python3 - "$BASE/chiffre/VIDEO_TS/VTS_01_1.VOB" <<'PY'
import sys
p = sys.argv[1]
d = bytearray(open(p, "rb").read())
for i in range(0, len(d), 2048):
    q = i + 14 + (d[i + 13] & 7)
    if d[q + 3] >> 4 == 0xE or d[q + 3] == 0xBD:
        d[q + 6] |= 0x30        # pes_scrambling_control = 3
open(p, "wb").write(d)
PY
"$KP" --dump-frame 4000 "$BASE/c.png" "$BASE/chiffre" >"$BASE/css.log" 2>&1
code=$?
sed 's/\x1b\[[0-9;]*m//g' "$BASE/css.log" > "$BASE/css.txt"
if grep -q "CSS" "$BASE/css.txt" && [ ! -s "$BASE/c.png" ] && [ "$code" -lt 128 ]; then ok "refus clair (CSS), aucune image, sortie propre (code $code)"
else ko "chiffré : code $code, image $( [ -s "$BASE/c.png" ] && echo oui || echo non ), message : $(grep -m1 -i 'css\|chiffr' "$BASE/css.txt")"; fi

echo "=== 9. Vrai disque ==="
if [ -b /dev/sr0 ] && command -v lsdvd >/dev/null && lsdvd /dev/sr0 2>/dev/null | grep -q "^Title:"; then
    "$KP" --list-dvd /dev/sr0 2>/dev/null | grep '^Title:' > "$BASE/mien.txt"
    lsdvd /dev/sr0 2>/dev/null | grep '^Title:' | sed 's/ Chapters/, Chapters/' > "$BASE/lsdvd.txt"
    n=$(wc -l < "$BASE/lsdvd.txt")
    if [ "$n" -gt 0 ] && diff -q "$BASE/mien.txt" "$BASE/lsdvd.txt" >/dev/null; then ok "$n titres identiques à lsdvd (durées, chapitres, cellules, pistes audio, sous-images)"
    else ko "écart avec lsdvd : $(diff "$BASE/mien.txt" "$BASE/lsdvd.txt" | head -4)"; fi
    "$KP" --list-dvd /dev/sr0 2>/dev/null | grep -A3 '^Title: 01' | grep -q 'Audio: 1, Language: fre' \
        && "$KP" --list-dvd /dev/sr0 2>/dev/null | grep -q 'Audio: 2, Language: jpn' && ok "langues du vrai disque lues (fre, jpn)" || ko "langues du vrai disque"
    "$KP" --dump-frame 2000 "$BASE/r.png" /dev/sr0 >"$BASE/reel.log" 2>&1
    if [ -s "$BASE/r.png" ]; then ok "le disque se lit (libdvdcss présente ?) : une image a été produite"
    elif sed 's/\x1b\[[0-9;]*m//g' "$BASE/reel.log" | grep -q "CSS"; then ok "film chiffré refusé avec le message CSS (rien n'est déchiffré)"
    else ko "disque réel : ni image ni message CSS : $(tail -2 "$BASE/reel.log")"; fi
else
    echo "  (aucun DVD dans le lecteur ou lsdvd absent : contrôle ignoré)"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]

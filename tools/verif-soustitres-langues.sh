#!/usr/bin/env bash
# verif-soustitres-langues.sh — langues préférées avec priorité, durées mini/maxi
# d'affichage, piste mémorisée par média. Tout se mesure sur les pixels de la vraie fenêtre :
# deux pistes portent des textes de LARGEURS très différentes (« iiiii » / « MMMMMMMMMM »),
# donc la largeur du texte affiché dit quelle piste est à l'écran.
#
# Contrôles :
#   1. fichiers externes : ["fre","eng"] → la piste fre ; ["eng","fre"] → eng ; une langue
#      absente en tête ["xx","fre"] → on passe à la suivante ; liste vide + default_language
#      "eng" → eng (comportement historique) ;
#   2. pistes intégrées d'un MKV : même priorité ;
#   3. durée minimale : un sous-titre de 1 s reste affiché 3 s ; sans minimum il a disparu
#      (témoin) ;
#   4. durée maximale : un sous-titre de 5 s disparaît après 1 s ; sans maximum il est là ;
#   5. piste mémorisée : on passe à l'autre piste avec J, on ferme, on rouvre — la piste choisie
#      revient si la mémoire est activée, pas sinon (témoin).
#   6. alphabet (encodage) : un fichier en windows-1252 (« Éléphant à côté ») s'affiche — il
#      n'était même pas chargé avant (lecture en UTF-8 strict) ; imposer windows-1251 sur un
#      fichier cyrillique change le texte affiché (largeur mesurée) par rapport à l'automatique.
#
# Usage : tools/verif-soustitres-langues.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-soustitres-langues
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
CONF="$XDG_CONFIG_HOME/rmplayer/config.json"
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
    -f lavfi -i "color=c=0x202020:s=640x360:r=10:d=14" -c:v libx264 -preset ultrafast -y "$M/fond.mp4"
# Deux langues, deux largeurs de texte.
printf '1\n00:00:00,000 --> 00:00:14,000\niiiii\n\n' > "$M/fond.eng.srt"
printf '1\n00:00:00,000 --> 00:00:14,000\nMMMMMMMMMM\n\n' > "$M/fond.fre.srt"
# MKV avec les deux mêmes pistes intégrées (eng d'abord, fre ensuite).
cp "$M/fond.eng.srt" "$BASE/eng.srt"; cp "$M/fond.fre.srt" "$BASE/fre.srt"
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -i "$M/fond.mp4" -i "$BASE/eng.srt" -i "$BASE/fre.srt" \
    -map 0:v -map 1 -map 2 -c:v copy -c:s srt \
    -metadata:s:s:0 language=eng -metadata:s:s:1 language=fre -y "$M/integre.mkv"
# Durées : une réplique d'1 s (2 → 3 s), une de 5 s (6 → 11 s).
cp "$M/fond.mp4" "$M/duree.mp4"
printf '1\n00:00:02,000 --> 00:00:03,000\nMMMMMMMMMM\n\n2\n00:00:06,000 --> 00:00:11,000\nMMMMMMMMMM\n\n' > "$M/duree.srt"

# ecrire_config '<json des champs de sous-titres>' : les valeurs absentes gardent le défaut du lecteur.
ecrire_config() {
    python3 - "$CONF" "$1" <<'PY'
import json, sys
st = {"font_size": 40.0, "outline_width": 0.0, "background_alpha": 0, "position_percent": 50.0,
      "color": "FFFFFF", "auto_load": True, "use_embedded": False}
st.update(json.loads(sys.argv[2]))
json.dump({"video": {"hardware_decoding": False}, "audio": {"device": "", "volume_max": 150},
           "subtitles": st, "interface": {"show_osd": False, "screenshots_dir": "/tmp"},
           "playback": {"resume": False}, "playlist": []}, open(sys.argv[1], "w"), indent=2)
PY
}

# lance <json> <média> : démarre le lecteur et attend qu'il joue.
lance() {
    ecrire_config "$1"
    arreter
    "$KP" --fullscreen "$M/$2" >"$BASE/dernier.log" 2>&1 &
    K_PID=$!
    sleep 4.5
}
# position <ms> : pause puis saut à la position (MPRIS), image figée.
position() {
    local piste
    piste=$(gdbus call --session --dest org.mpris.MediaPlayer2.rmplayer --object-path /org/mpris/MediaPlayer2 \
        --method org.freedesktop.DBus.Properties.Get org.mpris.MediaPlayer2.Player Metadata 2>/dev/null \
        | sed -n "s/.*trackid[^']*objectpath '\([^']*\)'.*/\1/p" | head -1)
    gdbus call --session --dest org.mpris.MediaPlayer2.rmplayer --object-path /org/mpris/MediaPlayer2 \
        --method org.mpris.MediaPlayer2.Player.Pause >/dev/null 2>&1
    sleep 0.4
    gdbus call --session --dest org.mpris.MediaPlayer2.rmplayer --object-path /org/mpris/MediaPlayer2 \
        --method org.mpris.MediaPlayer2.Player.SetPosition "${piste:-/org/rmplayer/track/0}" "$(( $1 * 1000 ))" >/dev/null 2>&1
    sleep 1.2
}
# largeur : largeur en pixels du texte clair visible (0 = aucun texte).
largeur() {
    xdotool mousemove 3 3 2>/dev/null; sleep 0.3
    ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x720 -i "$DISP+0,0" -frames:v 1 -y "$BASE/img.png"
    python3 - "$BASE/img.png" <<'PY'
import sys
import numpy as np
from PIL import Image
a = np.asarray(Image.open(sys.argv[1]).convert("L"), dtype=int)
a[:30, :30] = 0
m = a > 200
if m.sum() < 30:
    print(0)
else:
    ys, xs = np.nonzero(m)
    print(int(xs.max() - xs.min()))
PY
}
ferme() { kill "$K_PID" 2>/dev/null; wait "$K_PID" 2>/dev/null; sleep 0.4; }
# etroit / large : seuils entre « iiiii » (≈ 40 px) et « MMMMMMMMMM » (≈ 250 px).
classe() { if [ "$1" -eq 0 ]; then echo aucun; elif [ "$1" -lt 120 ]; then echo etroit; else echo large; fi; }

echo "=== 1. Langues préférées, fichiers externes ==="
for cas in 'fre,eng|["fre","eng"]|"xx"|large' 'eng,fre|["eng","fre"]|"xx"|etroit' \
           'xx,fre|["xx","fre"]|"eng"|large' 'vide+eng|[]|"eng"|etroit'; do
    nom=${cas%%|*}; reste=${cas#*|}; liste=${reste%%|*}; reste=${reste#*|}; defaut=${reste%%|*}; attendu=${reste#*|}
    lance "{\"preferred_languages\": $liste, \"default_language\": $defaut}" fond.mp4
    position 3000
    l=$(largeur); c=$(classe "$l"); ferme
    if [ "$c" = "$attendu" ]; then ok "$nom → piste $attendu à l'écran (largeur $l px)"
    else ko "$nom : attendu $attendu, mesuré $c (largeur $l px)"; fi
done

echo "=== 2. Langues préférées, pistes intégrées d'un MKV ==="
for cas in '["fre","eng"]|large' '["eng","fre"]|etroit'; do
    liste=${cas%%|*}; attendu=${cas#*|}
    lance "{\"preferred_languages\": $liste, \"default_language\": \"xx\", \"auto_load\": false, \"use_embedded\": true}" integre.mkv
    # Pas de saut ici : une réplique intégrée commencée AVANT la position visée n'est pas
    # relue après un saut (limite connue du lecteur, notée dans docs/PROCHAINES-FONCTIONS.md).
    # On mesure donc en lecture continue.
    l=$(largeur); c=$(classe "$l"); ferme
    if [ "$c" = "$attendu" ]; then ok "intégrées $liste → piste $attendu (largeur $l px)"
    else ko "intégrées $liste : attendu $attendu, mesuré $c (largeur $l px)"; fi
done

echo "=== 3. Durée minimale d'affichage ==="
lance '{"min_duration_ms": 3000, "default_language": "xx"}' duree.mp4; position 4500
avec=$(largeur); ferme
lance '{"min_duration_ms": 0, "default_language": "xx"}' duree.mp4; position 4500
sans=$(largeur); ferme
echo "  à 4,5 s : avec minimum $avec px, sans $sans px"
if [ "$avec" -gt 120 ] && [ "$sans" -eq 0 ]; then ok "la réplique d'1 s reste affichée 3 s ; sans minimum elle a disparu"
else ko "durée minimale : avec=$avec sans=$sans"; fi

echo "=== 4. Durée maximale d'affichage ==="
lance '{"max_duration_ms": 1000, "default_language": "xx"}' duree.mp4; position 8000
avec=$(largeur); ferme
lance '{"max_duration_ms": 0, "default_language": "xx"}' duree.mp4; position 8000
sans=$(largeur); ferme
echo "  à 8 s (réplique 6 → 11 s) : avec maximum $avec px, sans $sans px"
if [ "$avec" -eq 0 ] && [ "$sans" -gt 120 ]; then ok "la réplique de 5 s disparaît après 1 s ; sans maximum elle est là"
else ko "durée maximale : avec=$avec sans=$sans"; fi

echo "=== 5. Piste mémorisée ==="
# Départ : fre (large). J passe à eng (étroit). Relance : eng si mémorisé, fre sinon.
tente() {  # tente <remember_track>
    lance "{\"preferred_languages\": [\"fre\",\"eng\"], \"default_language\": \"xx\", \"remember_track\": $1}" fond.mp4
    position 3000
    avant=$(largeur)
    id=$(xdotool search --pid "$K_PID" --name RMPlayer 2>/dev/null | tail -1)
    xdotool windowfocus "$id" 2>/dev/null; sleep 0.3
    xdotool key --window "$id" j 2>/dev/null; sleep 0.8
    apres=$(largeur)
    ferme
    lance "{\"preferred_languages\": [\"fre\",\"eng\"], \"default_language\": \"xx\", \"remember_track\": $1}" fond.mp4
    position 3000
    relance=$(largeur); ferme
    echo "$(classe "$avant") $(classe "$apres") $(classe "$relance")"
}
r=$(tente true); echo "  mémoire activée : départ / après J / relance = $r"
if [ "$r" = "large etroit etroit" ]; then ok "la piste choisie (eng) revient à la réouverture"
else ko "mémoire activée : $r (attendu « large etroit etroit »)"; fi
rm -rf "$BASE/cache"/*
r=$(tente false); echo "  mémoire désactivée : départ / après J / relance = $r"
if [ "$r" = "large etroit large" ]; then ok "sans mémoire, la réouverture revient à la langue préférée (témoin)"
else ko "mémoire désactivée : $r (attendu « large etroit large »)"; fi

echo "=== 6. Alphabet des sous-titres ==="
cp "$M/fond.mp4" "$M/latin.mp4"; cp "$M/fond.mp4" "$M/cyril.mp4"
python3 - "$M" <<'FINPY'
import sys
m = sys.argv[1]
open(f"{m}/latin.srt", "wb").write("1\n00:00:00,000 --> 00:00:14,000\nMMMMMMMMMM Éléphant à côté\n\n".encode("cp1252"))
open(f"{m}/cyril.srt", "wb").write("1\n00:00:00,000 --> 00:00:14,000\nПРИВЕТ МИР ПРИВЕТ\n\n".encode("cp1251"))
FINPY
lance '{"default_language": "xx"}' latin.mp4; position 3000
lat=$(largeur); ferme
echo "  fichier windows-1252, détection automatique : largeur $lat px"
if [ "$lat" -gt 250 ]; then ok "le fichier Latin-1 est chargé et affiché (largeur $lat px)"
else ko "le fichier Latin-1 n'apparaît pas (largeur $lat px)"; fi
lance '{"default_language": "xx"}' cyril.mp4; position 3000
auto=$(largeur); ferme
lance '{"default_language": "xx", "encoding": "windows-1251"}' cyril.mp4; position 3000
force=$(largeur); ferme
echo "  fichier cyrillique : automatique $auto px | windows-1251 imposé $force px"
if python3 -c "import sys; a, f = $auto, $force; sys.exit(0 if a > 0 and f > 0 and abs(a - f) >= 0.04 * max(a, f) else 1)"; then
    ok "imposer l'alphabet change le texte affiché (largeurs différentes)"
else ko "l'alphabet imposé ne change pas le rendu ($auto / $force)"; fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]

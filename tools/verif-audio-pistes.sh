#!/usr/bin/env bash
# verif-audio-pistes.sh — choix de la piste audio : langues par priorité, pistes de commentaire ou
# d'audiodescription écartées, piste « par défaut » du fichier, mémoire par film.
#
# Principe : des MKV dont chaque piste audio a une fréquence propre (300 / 600 / 900 Hz) ; le lecteur
# décode via `--dump-audio` (même chaîne que la lecture) et on lit la fréquence dominante du WAV. La
# piste réellement jouée se mesure au son, pas à un message.
#
# Contrôles :
#   1. deux pistes « fre », la première est une audiodescription (drapeau du conteneur) : c'est la
#      seconde qui est jouée ;
#   2. même chose reconnue par le TITRE seul (« Commentaires du réalisateur », sans drapeau) ;
#   3. si la seule piste de la langue est un commentaire, elle est jouée quand même ;
#   4. langues par priorité : ["eng","fre"] → eng ; ["ger","fre"] → fre ; liste vide + langue par
#      défaut « eng » → eng (comportement historique) ;
#   5. aucune langue ne correspond : la piste marquée « par défaut » du fichier ;
#   6. mémoire par film : la piste choisie à la main (Ctrl+J) revient à la réouverture (si le réglage
#      est activé), pas sinon (témoin).
#
# Usage : tools/verif-audio-pistes.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-audio-pistes
KP=./target/release/rmplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=warn

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
for outil in python3 ffmpeg xdotool; do
    command -v "$outil" >/dev/null || { echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }

rm -rf "$BASE"; mkdir -p "$BASE/conf/rmplayer" "$BASE/cache"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
CONF="$XDG_CONFIG_HOME/rmplayer/config.json"

# mk <nom> <piste>… : une piste = "langue|titre|fréquence|disposition" (disposition : 0 pour aucune)
mk() {
    local nom="$1"; shift
    local entrees=(-f lavfi -i "color=c=0x202020:s=160x90:r=10:d=2")
    local cartes=(-map 0:v) meta=() i=0
    for piste in "$@"; do
        IFS='|' read -r langue titre freq dispo <<<"$piste"
        entrees+=(-f lavfi -i "sine=f=$freq:d=2,volume=0.03")
        cartes+=(-map "$((i+1)):a")
        meta+=(-metadata:s:a:$i language="$langue" -metadata:s:a:$i title="$titre")
        [ "$dispo" != 0 ] && meta+=(-disposition:a:$i "$dispo")
        i=$((i+1))
    done
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error "${entrees[@]}" "${cartes[@]}" \
        -c:v libx264 -preset ultrafast -c:a aac -ac 2 "${meta[@]}" -shortest -y "$BASE/$nom.mkv"
}
mk multi      "fre|Audiodescription|300|visual_impaired" "fre|VF 5.1|600|default" "eng|VO|900|0"
mk titre      "fre|Commentaires du réalisateur|300|0" "fre|VF|600|0"
mk seul       "fre|Commentaires|300|comment" "eng|VO|900|0"
mk defaut     "eng|VO|300|0" "spa|ES|600|default" "jpn|JA|900|0"
# Source MONO (le reste des pistes est en stéréo) : pour vérifier que le WAV dit la vérité.
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -f lavfi -i "color=c=0x202020:s=160x90:r=10:d=2" \
    -f lavfi -i "sine=f=600:d=2,volume=0.03" -ac 1 -c:v libx264 -preset ultrafast -c:a aac -shortest -y "$BASE/mono.mkv"

config() {  # config <json audio>
    python3 - "$CONF" "$1" <<'PY'
import json, sys
audio = {"device": "", "volume_max": 150}
audio.update(json.loads(sys.argv[2]))
json.dump({"video": {"hardware_decoding": False}, "audio": audio, "subtitles": {"auto_load": False},
           "playback": {"resume": False}, "playlist": []}, open(sys.argv[1], "w"), indent=2)
PY
}
freq_jouee() {  # freq_jouee <média> : fréquence dominante (Hz, arrondie à 50) de la piste jouée
    rm -f "$BASE/sortie.wav"
    "$KP" --dump-audio "$BASE/sortie.wav" "$BASE/$1.mkv" >/dev/null 2>&1
    python3 tools/analyse-audio.py freq "$BASE/sortie.wav" 2>/dev/null | cut -d' ' -f1 \
        | python3 -c "import sys; v = sys.stdin.read().strip(); print(int(round(float(v) / 50.0)) * 50 if v else 0)"
}
verdict() {  # verdict <description> <attendu> <obtenu>
    if [ "$2" = "$3" ]; then ok "$1 → ${3} Hz"; else ko "$1 : ${3} Hz au lieu de ${2} Hz"; fi
}

echo "=== 1. Audiodescription (drapeau) écartée ==="
config '{"default_language": "fre"}'; verdict "deux pistes fre, la première en audiodescription" 600 "$(freq_jouee multi)"
echo "=== 2. Commentaire reconnu par le titre ==="
verdict "« Commentaires du réalisateur » écarté" 600 "$(freq_jouee titre)"
echo "=== 3. Seule piste de la langue = commentaire ==="
verdict "à défaut d'autre piste fre, le commentaire est joué" 300 "$(freq_jouee seul)"
echo "=== 4. Langues par priorité ==="
config '{"preferred_languages": ["eng", "fre"], "default_language": "xx"}'; verdict "[eng, fre]" 900 "$(freq_jouee multi)"
config '{"preferred_languages": ["ger", "fre"], "default_language": "xx"}'; verdict "[ger, fre] (ger absent)" 600 "$(freq_jouee multi)"
config '{"preferred_languages": [], "default_language": "eng"}'; verdict "liste vide + langue par défaut eng" 900 "$(freq_jouee multi)"
echo "=== 5. Aucune langue ne correspond ==="
config '{"preferred_languages": ["ger"], "default_language": "ger"}'; verdict "piste marquée « par défaut » du fichier" 600 "$(freq_jouee defaut)"

echo "=== 5b. L'en-tête du WAV de --dump-audio dit la vérité (source mono) ==="
config '{}'
rm -f "$BASE/mono.wav"; "$KP" --dump-audio "$BASE/mono.wav" "$BASE/mono.mkv" >/dev/null 2>&1
mesure_mono=$(python3 - "$BASE/mono.wav" <<'PY'
import struct, sys
d = open(sys.argv[1], "rb").read()
canaux, taux = struct.unpack("<HI", d[22:24] + d[24:28])
octets = len(d) - 44
duree = octets / 4 / canaux / taux      # float32
print(canaux, "%.2f" % duree)
PY
)
set -- $mesure_mono
if [ "$1" = 2 ] && python3 -c "import sys; sys.exit(0 if abs($2 - 2.0) < 0.15 else 1)"; then
    ok "source mono : le WAV déclare $1 canaux et dure $2 s (2 s attendues)"
else ko "source mono : le WAV déclare ${1:-?} canaux et dure ${2:-?} s (attendu 2 canaux, 2 s)"; fi
verdict "source mono : fréquence juste" 600 "$(freq_jouee mono)"

echo "=== 6. Mémoire par film ==="
export DISPLAY="$DISP"
xdpyinfo >/dev/null 2>&1 || { echo "  · écran $DISP indisponible : contrôle de mémoire ignoré"; echo; echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="; [ "$FAIL" -eq 0 ]; exit; }
arreter() {
    for pid in $(pgrep -x rmplayer 2>/dev/null); do
        aff=$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | sed -n 's/^DISPLAY=//p')
        [ "$aff" = "$DISP" ] && kill "$pid" 2>/dev/null
    done
    sleep 0.5
}
trap arreter EXIT
# Une séance : ouvre le film, passe à la piste audio suivante (Ctrl+J), ferme ; puis relit la
# fréquence jouée à la réouverture (--dump-audio passe par la même sélection).
seance() {  # seance <remember: true|false>
    rm -rf "$BASE/cache"/*
    config "{\"preferred_languages\": [\"fre\"], \"default_language\": \"fre\", \"remember_track\": $1}"
    arreter
    "$KP" --fullscreen "$BASE/multi.mkv" >"$BASE/fenetre.log" 2>&1 &
    local pid=$!
    sleep 4.5
    local id; id=$(xdotool search --pid "$pid" --name RMPlayer 2>/dev/null | tail -1)
    xdotool windowfocus "$id" 2>/dev/null; sleep 0.3
    xdotool key --window "$id" ctrl+j 2>/dev/null; sleep 1.2
    kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null; sleep 0.5
    freq_jouee multi
}
verdict "mémoire activée : Ctrl+J (VF → VO) puis réouverture" 900 "$(seance true)"
verdict "mémoire désactivée : la réouverture revient à la VF (témoin)" 600 "$(seance false)"

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]

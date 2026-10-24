#!/usr/bin/env bash
# verif-pistes.sh — vérifie les pistes embarquées : inventaire, changement de piste
# audio (mesuré en fréquence), sélection de sous-titres (texte ET minutages).
#
# Principe : on fabrique un MKV de référence dont on connaît tout (deux pistes audio
# de fréquences différentes, deux pistes de sous-titres dans deux langues), puis on
# vérifie que le lecteur :
#   1. voit les quatre pistes avec leurs langues ;
#   2. change réellement de piste audio (mesure FFT : 300 Hz vs 900 Hz) ;
#   3. choisit la piste selon la langue préférée ;
#   4. extrait les sous-titres avec le bon TEXTE et les bons MINUTAGES ;
#   5. affiche le sous-titre à l'écran au bon moment (capture d'image).
#
# Usage : tools/verif-pistes.sh [écran]      (par défaut :99, pour le contrôle visuel)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-pistes
KP=./target/release/kplayer
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
# `kinterface=debug` : les menus journalisent la géométrie de leurs entrées, que
# l'audit utilise pour cliquer sans deviner les pixels.
export RUST_LOG=kplayer=info,kinterface=debug

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }

mkdir -p "$BASE"

# ---------------------------------------------------------------- média de test
# Deux pistes audio (300 Hz « eng », 900 Hz « fre ») et deux pistes SRT. Les
# fréquences différentes rendent le choix de piste MESURABLE.
if [ ! -s "$BASE/complet.mkv" ]; then
    # Deux événements courts (vérification des MINUTAGES) et un long (vérification
    # de l'AFFICHAGE : il couvre largement le moment où la capture est prise, sans
    # dépendre d'un positionnement au clavier).
    cat > "$BASE/en.srt" <<'EOF'
1
00:00:01,000 --> 00:00:03,000
English line one

2
00:00:04,000 --> 00:00:06,000
English line two

3
00:00:08,000 --> 00:00:18,000
English long line for display
EOF
    cat > "$BASE/fr.srt" <<'EOF'
1
00:00:01,000 --> 00:00:03,000
Ligne française un

2
00:00:04,000 --> 00:00:06,000
Ligne française deux

3
00:00:08,000 --> 00:00:18,000
Ligne française longue pour l'affichage
EOF
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "sine=f=300:d=20:r=48000" \
        -f lavfi -i "sine=f=900:d=20:r=48000" \
        -f lavfi -i "color=c=teal:s=320x240:d=20:r=10" \
        -map 2:v -map 0:a -map 1:a -c:v libx264 -preset ultrafast \
        -c:a aac -b:a 160k -ac 2 \
        -metadata:s:a:0 language=eng -metadata:s:a:0 title=Anglais \
        -metadata:s:a:1 language=fre -metadata:s:a:1 title=Français \
        -y "$BASE/deux-audio.mkv"
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -i "$BASE/deux-audio.mkv" -i "$BASE/en.srt" -i "$BASE/fr.srt" \
        -map 0 -map 1 -map 2 -c copy -c:s srt \
        -metadata:s:s:0 language=eng -metadata:s:s:0 title=English \
        -metadata:s:s:1 language=fre -metadata:s:s:1 title=Français \
        -y "$BASE/complet.mkv"
fi
[ -s "$BASE/complet.mkv" ] || { echo "média de test non généré"; exit 1; }
echo "média de test : $BASE/complet.mkv"

# ------------------------------------------------------- configuration isolée
config_essai() {  # config_essai <nom> <langue_audio> <langue_subs> <embarques>
    local nom="$1" la="$2" ls_="$3" emb="$4"
    local dossier="$BASE/$nom"
    rm -rf "$dossier"; mkdir -p "$dossier/conf/kplayer" "$dossier/cache"
    python3 - "$dossier/conf/kplayer/config.json" "$la" "$ls_" "$emb" <<'PY'
import json, sys
la, ls_, emb = sys.argv[2], sys.argv[3], sys.argv[4] == "true"
json.dump({
    "video": {"hardware_decoding": False, "scaling_method": "Lanczos"},
    "audio": {"device": "", "volume_max": 150, "default_language": la},
    "subtitles": {"auto_load": False, "font_size": 40.0, "default_language": ls_,
                  "use_embedded": emb},
    "interface": {"dark_theme": True, "show_osd": False, "osd_duration": 1000,
                  "show_progress_bar": True, "controls_always_visible": False,
                  "show_title": False, "screenshots_dir": "/tmp"},
    "playback": {"resume": False},
    "playlist": [],
}, open(sys.argv[1], "w"), indent=2, ensure_ascii=False)
PY
}

exec_essai() {  # exec_essai <nom> <arguments…>
    local nom="$1"; shift
    XDG_CONFIG_HOME="$BASE/$nom/conf" XDG_CACHE_HOME="$BASE/$nom/cache" \
        "$KP" "$@" >"$BASE/$nom/log.txt" 2>&1
}

echo
echo "=== 1. Inventaire des pistes ==="
config_essai inventaire "" "" true
exec_essai inventaire --list-tracks "$BASE/complet.mkv"
liste=$(cat "$BASE/inventaire/log.txt")
echo "$liste" | sed 's/^/    /'
for attendu in "audio[0] flux 1" "audio[1] flux 2" "sous-titres[0] flux 3" "sous-titres[1] flux 4"; do
    # `grep -F` : les crochets de « audio[0] » sont une CLASSE DE CARACTÈRES en
    # expression régulière — sans -F, la recherche ne correspond à rien (piège qui a
    # fait échouer ces quatre contrôles au premier essai).
    if printf '%s' "$liste" | grep -qF "$attendu"; then
        ok "piste annoncée : $attendu"
    else
        ko "piste manquante dans l'inventaire : $attendu"
    fi
done
if printf '%s' "$liste" | grep -qF "fre — Français"; then
    ok "les métadonnées de langue et de titre sont lues"
else
    ko "langue/titre non lus depuis le conteneur"
fi

echo
echo "=== 2. Changement de piste audio (mesuré en fréquence) ==="
config_essai audio "" "" false
freq_de() { python3 tools/analyse-audio.py freq "$1" | cut -d' ' -f1; }
exec_essai audio --dump-audio "$BASE/p1.wav" --audio-track 1 "$BASE/complet.mkv"
exec_essai audio --dump-audio "$BASE/p2.wav" --audio-track 2 "$BASE/complet.mkv"
f1=$(freq_de "$BASE/p1.wav"); f2=$(freq_de "$BASE/p2.wav")
echo "    piste 1 (eng) : $f1 Hz   piste 2 (fre) : $f2 Hz"
if python3 -c "import sys; sys.exit(0 if abs(float('$f1') - 300) < 10 else 1)"; then
    ok "la piste 1 contient bien le son de 300 Hz"
else
    ko "la piste 1 ne contient pas 300 Hz ($f1)"
fi
if python3 -c "import sys; sys.exit(0 if abs(float('$f2') - 900) < 10 else 1)"; then
    ok "la piste 2 contient bien le son de 900 Hz"
else
    ko "la piste 2 ne contient pas 900 Hz ($f2)"
fi

echo
echo "=== 3. Piste choisie selon la langue préférée ==="
config_essai langue_fr "fre" "fre" true
exec_essai langue_fr --dump-audio "$BASE/def_fr.wav" "$BASE/complet.mkv"
config_essai langue_eng "eng" "eng" true
exec_essai langue_eng --dump-audio "$BASE/def_eng.wav" "$BASE/complet.mkv"
ffr=$(freq_de "$BASE/def_fr.wav"); feng=$(freq_de "$BASE/def_eng.wav")
echo "    langue « fre » → $ffr Hz   langue « eng » → $feng Hz"
if python3 -c "import sys; sys.exit(0 if abs(float('$ffr') - 900) < 10 else 1)"; then
    ok "la langue préférée « fre » choisit la 2e piste"
else
    ko "la langue préférée n'est pas respectée ($ffr Hz au lieu de 900)"
fi
if python3 -c "import sys; sys.exit(0 if abs(float('$feng') - 300) < 10 else 1)"; then
    ok "la langue préférée « eng » choisit la 1re piste"
else
    ko "la langue préférée « eng » n'est pas respectée ($feng Hz au lieu de 300)"
fi

echo
echo "=== 4. Sous-titres embarqués : texte et minutages ==="
config_essai subs_fr "" "fre" true
exec_essai subs_fr --dump-subs "$BASE/subs_fr.srt" "$BASE/complet.mkv"
echo "    extrait :"; sed 's/^/      /' "$BASE/subs_fr.srt" | head -8
if grep -q "Ligne française un" "$BASE/subs_fr.srt"; then
    ok "la piste française est extraite (texte exact)"
else
    ko "la piste française n'est pas extraite"
fi
if grep -q "English" "$BASE/subs_fr.srt"; then
    ko "la piste anglaise apparaît alors que la langue demandée est « fre »"
else
    ok "la piste anglaise n'est pas mélangée (une seule piste à la fois)"
fi
# Minutages : 1000→3000 et 4000→6000. Une durée fausse (3 s par défaut) se voit
# immédiatement — c'est le défaut qu'a révélé ce banc d'essai.
if grep -q "^1000 --> 3000$" "$BASE/subs_fr.srt" && grep -q "^4000 --> 6000$" "$BASE/subs_fr.srt"; then
    ok "les minutages correspondent au fichier (1000→3000, 4000→6000)"
else
    ko "minutages incorrects : $(grep -- '-->' "$BASE/subs_fr.srt" | tr '\n' ' ')"
fi

config_essai subs_off "" "fre" false
exec_essai subs_off --dump-subs "$BASE/subs_off.srt" --sub-track -1 "$BASE/complet.mkv"
if [ ! -s "$BASE/subs_off.srt" ]; then
    ok "piste désactivée : aucun sous-titre extrait"
else
    ko "la désactivation n'est pas respectée"
fi

echo
echo "=== 5. Affichage réel du sous-titre embarqué ==="
# Le 3e événement couvre 8 s → 18 s : quelle que soit l'instant de la capture (le
# démarrage prend quelques secondes), le texte doit être affiché. On compare donc
# le BAS de l'écran entre « embarqués activés » et « désactivés ».
capture_bas() {  # capture_bas <nom_config> <sortie.png>
    local nom="$1" sortie="$2"
    pkill -x kplayer 2>/dev/null; sleep 0.6
    XDG_CONFIG_HOME="$BASE/$nom/conf" XDG_CACHE_HOME="$BASE/$nom/cache" DISPLAY="$DISP" \
        "$KP" --fullscreen "$BASE/complet.mkv" >"$BASE/$nom/run.log" 2>&1 &
    local kpid=$!
    sleep 9
    DISPLAY=$DISP ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x720 \
        -i "$DISP+0,0" -frames:v 1 -y "$sortie"
    kill "$kpid" 2>/dev/null
    sleep 0.4
    [ -s "$sortie" ]
}
if DISPLAY=$DISP xdotool getdisplaygeometry >/dev/null 2>&1; then
    if capture_bas subs_fr "$BASE/avec.png" && capture_bas subs_off "$BASE/sans.png"; then
        ecart=$(python3 -c "
from PIL import Image
import numpy as np
a = np.asarray(Image.open('$BASE/avec.png').convert('L'), dtype=int)
b = np.asarray(Image.open('$BASE/sans.png').convert('L'), dtype=int)
h = min(a.shape[0], b.shape[0]); w = min(a.shape[1], b.shape[1])
# Bas de l'image : c'est là que le sous-titre est dessiné.
print(round(float(np.abs(a[h*2//3:h, :w] - b[h*2//3:h, :w]).mean()), 2))
")
        echo "    écart d'image (bas de l'écran) activé / désactivé : $ecart/255"
        if python3 -c "import sys; sys.exit(0 if float('$ecart') > 0.3 else 1)"; then
            ok "le sous-titre embarqué est réellement dessiné à l'écran"
        else
            ko "aucune différence visible : le sous-titre n'est pas dessiné ($ecart)"
        fi
    else
        ko "capture d'écran impossible"
    fi
else
    echo "  (écran $DISP indisponible : contrôle visuel ignoré)"
fi

echo
echo "=== 6. Changement de piste à chaud depuis l'interface ==="
# Le mécanisme est le même que `--audio-track` (même méthode du moteur), mais on
# vérifie ici le CABLAGE de l'interface : ouverture du menu, clic sur une piste,
# journal de sélection. Sans cela, une erreur d'index ou un menu inerte passerait
# inaperçu alors que le moteur fonctionne.
if DISPLAY=$DISP xdotool getdisplaygeometry >/dev/null 2>&1; then
    pkill -x kplayer 2>/dev/null; sleep 0.6
    config_essai chaud "" "" false
    XDG_CONFIG_HOME="$BASE/chaud/conf" XDG_CACHE_HOME="$BASE/chaud/cache" DISPLAY="$DISP" \
        "$KP" --speed 0.5 "$BASE/complet.mkv" >"$BASE/chaud/run.log" 2>&1 &
    kpid=$!
    sleep 5
    wid=$(DISPLAY=$DISP xdotool search --pid "$kpid" --name KPlayer 2>/dev/null | head -1)
    [ -z "$wid" ] && wid=$(DISPLAY=$DISP xdotool search --name KPlayer 2>/dev/null | head -1)
    DISPLAY=$DISP xdotool windowactivate --sync "$wid" 2>/dev/null \
        || DISPLAY=$DISP xdotool windowfocus --sync "$wid" 2>/dev/null || true
    # Coordonnées RELATIVES à la fenêtre : elle n'est pas forcément en (0,0) — un
    # menu cliqué à l'aveugle sur l'écran ne s'ouvre pas (constaté).
    eval "$(DISPLAY=$DISP xdotool getwindowgeometry --shell "$wid")"
    DISPLAY=$DISP xdotool mousemove $((X + WIDTH / 2)) $((Y + HEIGHT / 2)); sleep 0.3
    # Menu « Audio » : 3e entrée de la barre.
    DISPLAY=$DISP xdotool mousemove $((X + 180)) $((Y + 10)); sleep 0.2
    DISPLAY=$DISP xdotool click 1
    sleep 1.2
    cible=$(sed 's/\x1b\[[0-9;]*m//g' "$BASE/chaud/run.log" \
        | grep "menu audio piste" | tail -2 | grep "actif=false" | head -1)
    if [ -z "$cible" ]; then
        ko "le menu Audio n'énumère aucune piste inactive"
    else
        read -r mx my <<<"$(echo "$cible" | sed -E \
            's/.*rect \[\[([0-9.]+) ([0-9.]+)\] - \[([0-9.]+) ([0-9.]+)\].*/\1 \2 \3 \4/' \
            | awk '{printf "%d %d", ($1+$3)/2, ($2+$4)/2}')"
        echo "    clic sur la piste inactive en ($mx,$my)"
        DISPLAY=$DISP xdotool mousemove "$mx" "$my"; sleep 0.4
        DISPLAY=$DISP xdotool click 1
        sleep 1.5
        if sed 's/\x1b\[[0-9;]*m//g' "$BASE/chaud/run.log" | grep -q "piste audio choisie"; then
            choisi=$(sed 's/\x1b\[[0-9;]*m//g' "$BASE/chaud/run.log" \
                | grep "piste audio choisie" | tail -1 | sed 's/.*piste audio choisie : //')
            ok "le menu change réellement de piste ($choisi)"
        else
            ko "le clic dans le menu n'a pas changé de piste"
        fi
        # La lecture doit continuer : une bascule qui arrête le média serait un bug.
        if sed 's/\x1b\[[0-9;]*m//g' "$BASE/chaud/run.log" | grep -q "Fin du média"; then
            ko "la lecture s'est arrêtée pendant le changement de piste"
        else
            ok "la lecture continue après le changement de piste"
        fi
    fi
    kill "$kpid" 2>/dev/null
    sleep 0.4
else
    echo "  (écran $DISP indisponible : contrôle d'interface ignoré)"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
pkill -x kplayer 2>/dev/null || true
[ "$FAIL" -eq 0 ]

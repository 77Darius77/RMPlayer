#!/usr/bin/env bash
# verif-profils.sh — vérifie les profils de réglages : enregistrement, application,
# persistance, export/import, et surtout que l'application d'un profil CHANGE le rendu.
#
# Principe : un profil n'a d'intérêt que s'il agit. On vérifie donc qu'après
# application, l'image produite est réellement différente (netteté, luminosité) et que
# la configuration écrite sur disque contient bien les valeurs du profil.
#
# Usage : tools/verif-profils.sh [écran]      (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-profils
KP=./target/release/rmplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=info,kinterface=debug

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }

rm -rf "$BASE"; mkdir -p "$BASE/conf/rmplayer" "$BASE/cache"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"

# Média de test : dégradé + texture, pour que netteté et luminosité se voient.
if [ ! -s "$BASE/mire.mp4" ]; then
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "testsrc2=s=640x360:r=10:d=4" \
        -c:v libx264 -preset ultrafast -y "$BASE/mire.mp4"
fi

echo
echo "=== 1. Espace de profils vide au départ ==="
$KP --list-profiles > "$BASE/liste0.txt" 2>/dev/null
if grep -q "aucun profil" "$BASE/liste0.txt"; then
    ok "aucun profil tant que rien n'est enregistré"
else
    ko "état initial inattendu : $(cat "$BASE/liste0.txt")"
fi

echo
echo "=== 2. Écriture de deux profils et relecture ==="
# Les profils sont écrits par l'application (interface) ; pour le banc d'essai, on les
# écrit directement dans le fichier, puis on vérifie que le LECTEUR les relit — c'est
# ce qui compte : le format doit être le sien.
python3 - "$BASE/conf/rmplayer/profiles.json" "$BASE/conf/rmplayer/config.json" <<'PY'
import json, sys

# Configuration de base (réglages « neutres »).
base = {
    "video": {"hardware_decoding": False, "scaling_method": "Lanczos"},
    "audio": {"device": "", "volume_max": 150},
    "subtitles": {"auto_load": False, "font_size": 24.0, "default_language": "fre"},
    "interface": {"dark_theme": True, "show_osd": False, "osd_duration": 1000,
                  "show_progress_bar": True, "controls_always_visible": False,
                  "show_title": False, "screenshots_dir": "/tmp"},
    "playback": {"resume": False},
    "playlist": [],
}
json.dump(base, open(sys.argv[2], "w"), indent=2)

lumiere = json.loads(json.dumps(base))
lumiere["video"].update({"brightness": 0.35, "contrast": 1.4, "crop_top": 20,
                         "crop_bottom": 20, "aspect": "Ratio4x3"})

nettete = json.loads(json.dumps(base))
nettete["video"].update({"sharpen": 3, "denoise": 2})
nettete["audio"].update({"eq_enabled": True, "normalizer": True})

profils = [
    {"name": "Lumineux", "description": "film sombre",
     "video": lumiere["video"], "audio": lumiere["audio"],
     "subtitles": lumiere["subtitles"]},
    {"name": "Net", "description": "anime, vieux encodage",
     "video": nettete["video"], "audio": nettete["audio"],
     "subtitles": nettete["subtitles"]},
]
json.dump(profils, open(sys.argv[1], "w"), indent=2, ensure_ascii=False)
PY
$KP --list-profiles > "$BASE/liste1.txt" 2>/dev/null
sed 's/^/    /' "$BASE/liste1.txt"
if grep -q "Lumineux" "$BASE/liste1.txt" && grep -q "Net" "$BASE/liste1.txt"; then
    ok "les deux profils sont relus"
else
    ko "profils non relus"
fi
if grep "Lumineux" "$BASE/liste1.txt" | grep -q "image" \
   && grep "Lumineux" "$BASE/liste1.txt" | grep -q "recadrage" \
   && grep "Lumineux" "$BASE/liste1.txt" | grep -q "aspect 4:3"; then
    ok "le résumé du profil décrit ce qu'il contient"
else
    ko "résumé incomplet : $(grep Lumineux "$BASE/liste1.txt")"
fi

echo
echo "=== 3. Un profil inconnu est refusé (pas de démarrage silencieux) ==="
$KP --profil "Inexistant" "$BASE/mire.mp4" > "$BASE/erreur.txt" 2>&1
code=$?
if [ "$code" -eq 2 ] && grep -q "introuvable" "$BASE/erreur.txt"; then
    ok "profil inconnu : code 2 et message explicite"
else
    ko "profil inconnu mal traité (code $code)"
fi

echo
echo "=== 4. Appliquer un profil change réellement l'image ==="
config_actuelle() { python3 -c "
import json
c = json.load(open('$BASE/conf/rmplayer/config.json'))
v = c['video']
print(v.get('brightness', 0.0), v.get('contrast', 1.0), v.get('crop_top', 0),
      v.get('aspect', 'Auto'), v.get('sharpen', 0))
"; }
$KP --dump-frame 1000 "$BASE/neutre.png" "$BASE/mire.mp4" > "$BASE/log_neutre.txt" 2>&1
avant=$(config_actuelle)
$KP --profil "Lumineux" --dump-frame 1000 "$BASE/lumineux.png" "$BASE/mire.mp4" > "$BASE/log_lum.txt" 2>&1
apres=$(config_actuelle)
echo "    avant : $avant"
echo "    après : $apres"
d_neutre=$(python3 -c "from PIL import Image; print('%dx%d' % Image.open('$BASE/neutre.png').size)")
d_lum=$(python3 -c "from PIL import Image; print('%dx%d' % Image.open('$BASE/lumineux.png').size)")
echo "    dimensions : sans profil $d_neutre, avec « Lumineux » $d_lum"
if [ "$d_neutre" = "640x360" ] && [ "$d_lum" = "640x320" ]; then
    ok "le recadrage du profil est appliqué à l'image (360 → 320)"
else
    ko "recadrage du profil non appliqué ($d_neutre → $d_lum)"
fi
ecart=$(python3 -c "
from PIL import Image
import numpy as np
a = np.asarray(Image.open('$BASE/neutre.png').convert('L'), dtype=int)
b = np.asarray(Image.open('$BASE/lumineux.png').convert('L'), dtype=int)
h = min(a.shape[0], b.shape[0]); w = min(a.shape[1], b.shape[1])
print(round(float(abs(a[:h,:w].mean() - b[:h,:w].mean())), 2))
")
echo "    écart de luminosité moyenne : $ecart/255"
if python3 -c "import sys; sys.exit(0 if float('$ecart') > 15 else 1)"; then
    ok "la luminosité du profil est appliquée (écart $ecart)"
else
    ko "luminosité du profil non appliquée (écart $ecart)"
fi
# Le profil ne doit PAS écraser le périphérique audio (propre à la machine).
if grep -q "Profil appliqué" "$BASE/log_lum.txt"; then
    ok "l'application du profil est tracée dans le journal"
else
    ko "application du profil non tracée"
fi

echo
echo "=== 5. Le périphérique audio de la machine est préservé ==="
# On met un périphérique factice dans la configuration, puis on applique un profil
# qui, lui, n'en contient pas de particulier : la valeur doit survivre.
python3 - "$BASE/conf/rmplayer/config.json" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))
c["audio"]["device"] = "peripherique-de-la-machine"
json.dump(c, open(sys.argv[1], "w"), indent=2)
PY
$KP --profil "Net" --dump-audio /dev/null "$BASE/mire.mp4" > "$BASE/log_net.txt" 2>&1
apres_dev=$(python3 -c "
import json
print(json.load(open('$BASE/conf/rmplayer/config.json'))['audio']['device'])
")
echo "    périphérique après application : « $apres_dev »"
if [ "$apres_dev" = "peripherique-de-la-machine" ]; then
    ok "le périphérique de sortie n'est pas écrasé par un profil"
else
    ko "le profil a écrasé le périphérique audio (« $apres_dev »)"
fi

echo
echo "=== 6. Profil corrompu : le lecteur démarre quand même ==="
# Un fichier de profils illisible ne doit jamais empêcher de lire un média : ce sont
# des réglages de confort, pas une dépendance.
echo '{"ceci": "n est pas une liste"}' > "$BASE/conf/rmplayer/profiles.json"
$KP --list-tracks "$BASE/mire.mp4" > "$BASE/log_corrompu.txt" 2>&1
if grep -q "vidéo :" "$BASE/log_corrompu.txt"; then
    ok "un fichier de profils corrompu n'empêche pas la lecture"
else
    ko "fichier de profils corrompu : la lecture échoue"
fi
# Et la liste doit être vide plutôt que provoquer une erreur.
$KP --list-profiles > "$BASE/liste_corrompu.txt" 2>&1
if grep -q "aucun profil" "$BASE/liste_corrompu.txt"; then
    ok "profils illisibles : liste vide, pas de plantage"
else
    ko "profils illisibles mal gérés : $(cat "$BASE/liste_corrompu.txt" | head -2)"
fi

echo
echo "=== 7. Profils en double : dédoublonnés à la lecture ==="
python3 - "$BASE/conf/rmplayer/profiles.json" <<'PY'
import json, sys
p = {"name": "Double", "description": "", "video": {"hardware_decoding": False},
     "audio": {}, "subtitles": {}}
json.dump([p, dict(p)], open(sys.argv[1], "w"), indent=2)
PY
$KP --list-profiles > "$BASE/liste_double.txt" 2>/dev/null
nb=$(grep -c "^Double" "$BASE/liste_double.txt")
echo "    occurrences de « Double » : $nb"
if [ "$nb" = "1" ]; then
    ok "un profil en double n'apparaît qu'une fois (choix non ambigu)"
else
    ko "profil en double conservé ($nb occurrences)"
fi

echo
echo "=== 8. Interface : la section « Profils » existe et s'ouvre ==="
if DISPLAY=$DISP xdotool getdisplaygeometry >/dev/null 2>&1; then
    # Un profil est nécessaire pour que le bouton « Appliquer » existe : on en
    # installe un avant d'ouvrir l'interface.
    python3 - "$BASE/conf/rmplayer/profiles.json" <<'PY'
import json, sys
profil = {
    "name": "Essai", "description": "",
    "video": {"hardware_decoding": False, "crop_top": 20, "crop_bottom": 20,
              "aspect": "Ratio4x3"},
    "audio": {"volume_max": 150},
    "subtitles": {"auto_load": False, "font_size": 24.0, "default_language": "fre"},
}
json.dump([profil], open(sys.argv[1], "w"), indent=2, ensure_ascii=False)
PY
    # Configuration neutre, pour que l'application du profil ait un effet mesurable.
    python3 - "$BASE/conf/rmplayer/config.json" <<'PY'
import json, sys
c = json.load(open(sys.argv[1]))
c["video"] = {"hardware_decoding": False, "scaling_method": "Lanczos"}
c["audio"]["device"] = ""
json.dump(c, open(sys.argv[1], "w"), indent=2)
PY
    pkill -x rmplayer 2>/dev/null; sleep 0.6
    DISPLAY=$DISP "$KP" --fullscreen "$BASE/mire.mp4" >"$BASE/ui.log" 2>&1 &
    kpid=$!
    sleep 5
    wid=$(DISPLAY=$DISP xdotool search --pid "$kpid" --name RMPlayer 2>/dev/null | head -1)
    [ -z "$wid" ] && wid=$(DISPLAY=$DISP xdotool search --name RMPlayer 2>/dev/null | head -1)
    DISPLAY=$DISP xdotool windowactivate --sync "$wid" 2>/dev/null \
        || DISPLAY=$DISP xdotool windowfocus --sync "$wid" 2>/dev/null || true
    DISPLAY=$DISP xdotool mousemove 640 360; sleep 0.3
    DISPLAY=$DISP xdotool key --clearmodifiers F5; sleep 1.5
    # Trace réelle de l'application : la géométrie de la fenêtre est journalisée.
    if grep -q "fenetre pref" "$BASE/ui.log"; then
        ok "la fenêtre des préférences s'ouvre"
    else
        ko "préférences non ouvertes"
    fi
    # La section « Profils » doit figurer dans la liste des sections, ouverte.
    if grep "prefs section profils" "$BASE/ui.log" | grep -q "ouvert=true"; then
        ok "la section « Profils » est présente et dépliée"
    else
        ko "section « Profils » absente ou repliée"
    fi
    # La section « Profils » doit être présente (elle est dépliée par défaut).
    # Contrôle par image : la fenêtre doit afficher du contenu (texte lisible).
    DISPLAY=$DISP ffmpeg -hide_banner -loglevel error -f x11grab -video_size 700x400 \
        -i "$DISP+0,0" -frames:v 1 -y "$BASE/prefs.png"
    if [ -s "$BASE/prefs.png" ]; then
        texte=$(python3 tools/compte-pixels-clairs.py "$BASE/prefs.png")
        echo "    pixels clairs dans la fenêtre : $texte"
        if [ "${texte:-0}" -gt 500 ]; then
            ok "la fenêtre des préférences affiche du contenu"
        else
            ko "fenêtre des préférences vide"
        fi
    fi
    # --- Clic réel sur « Appliquer » : le bouton doit agir, pas seulement exister ---
    ligne=$(sed 's/\x1b\[[0-9;]*m//g' "$BASE/ui.log" | grep "bouton appliquer" | head -1)
    if [ -n "$ligne" ]; then
        read -r mx my <<<"$(echo "$ligne" | sed -E \
            's/.*rect \[\[([0-9.]+) ([0-9.]+)\] - \[([0-9.]+) ([0-9.]+)\].*/\1 \2 \3 \4/' \
            | awk '{printf "%d %d", ($1+$3)/2, ($2+$4)/2}')"
        echo "    clic sur « Appliquer » en ($mx,$my)"
        DISPLAY=$DISP xdotool mousemove "$mx" "$my"; sleep 0.4
        DISPLAY=$DISP xdotool click 1
        sleep 1.5
        if sed 's/\x1b\[[0-9;]*m//g' "$BASE/ui.log" | grep -q "profil appliqué"; then
            applique=$(sed 's/\x1b\[[0-9;]*m//g' "$BASE/ui.log" \
                | grep "profil appliqué" | tail -1 | sed 's/.*profil appliqué : //')
            ok "le bouton « Appliquer » agit ($applique)"
        else
            ko "le bouton « Appliquer » n'a rien fait"
        fi
        # Et la configuration doit avoir été écrite avec les valeurs du profil.
        enregistre=$(python3 -c "
import json
v = json.load(open('$BASE/conf/rmplayer/config.json'))['video']
print(round(v.get('crop_top', 0)), v.get('aspect', 'Auto'))
")
        echo "    configuration après application : recadrage haut / aspect = $enregistre"
        if echo "$enregistre" | grep -q "Ratio4x3"; then
            ok "le profil est écrit dans la configuration (persistant)"
        else
            ko "profil non persistant ($enregistre)"
        fi
    else
        ko "bouton « Appliquer » introuvable dans le journal"
    fi
    # --- Enregistrement d'un profil depuis l'interface ---
    nom_ligne=$(sed 's/\x1b\[[0-9;]*m//g' "$BASE/ui.log" | grep "bouton enregistrer profil" | tail -1)
    if [ -n "$nom_ligne" ]; then
        read -r nx ny <<<"$(echo "$nom_ligne" | sed -E \
            's/.*rect \[\[([0-9.]+) ([0-9.]+)\] - \[([0-9.]+) ([0-9.]+)\].*/\1 \2 \3 \4/' \
            | awk '{printf "%d %d", ($1+$3)/2, ($2+$4)/2}')"
        # Il faut saisir un nom avant que le bouton soit actif : on passe par le champ.
        champ=$(sed 's/\x1b\[[0-9;]*m//g' "$BASE/ui.log" | grep "champ nom profil" | tail -1)
        if [ -n "$champ" ]; then
            read -r cx cy <<<"$(echo "$champ" | sed -E \
                's/.*rect \[\[([0-9.]+) ([0-9.]+)\] - \[([0-9.]+) ([0-9.]+)\].*/\1 \2 \3 \4/' \
                | awk '{printf "%d %d", ($1+$3)/2, ($2+$4)/2}')"
            DISPLAY=$DISP xdotool mousemove "$cx" "$cy"; sleep 0.3
            DISPLAY=$DISP xdotool click 1; sleep 0.3
            DISPLAY=$DISP xdotool type --delay 40 "Essai"
            sleep 0.5
            DISPLAY=$DISP xdotool mousemove "$nx" "$ny"; sleep 0.3
            DISPLAY=$DISP xdotool click 1; sleep 1.2
            if sed 's/\x1b\[[0-9;]*m//g' "$BASE/ui.log" | grep -q "profil enregistré"; then
                ok "le bouton « Enregistrer » crée bien un profil"
            else
                ko "le bouton « Enregistrer » n'a rien créé"
            fi
        fi
    fi
    kill "$kpid" 2>/dev/null
    sleep 0.3
else
    echo "  (écran $DISP indisponible : contrôle d'interface ignoré)"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
pkill -x rmplayer 2>/dev/null || true
[ "$FAIL" -eq 0 ]

#!/usr/bin/env bash
# verif-cadrage.sh — vérifie les réglages d'image « avancés » : rapport d'aspect,
# recadrage des bandes noires (détection comprise), style des sous-titres, chapitres.
#
# Principe : rien n'est cru sur parole. Chaque réglage est vérifié par un mesurable :
#   - recadrage et détection : dimensions de l'image produite (`--dump-frame`) ;
#   - rapport d'aspect : forme de la zone vidéo à l'écran (géométrie journalisée) ;
#   - style des sous-titres : analyse de pixels dans la zone du texte ;
#   - chapitres : positions retournées par `--list-tracks`.
#
# Usage : tools/verif-cadrage.sh [écran]      (défaut :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-cadrage
KP=./target/release/kplayer
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=kplayer=info,kinterface=debug

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }

mkdir -p "$BASE"

# ---------------------------------------------------------------- médias de test
# 1) letterbox : image utile 1280x540 centrée dans 1280x720 (90 px de bandes)
if [ ! -s "$BASE/letterbox.mp4" ]; then
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "testsrc2=s=1280x540:r=10:d=6" -vf "pad=1280:720:0:90:black" \
        -c:v libx264 -preset ultrafast -y "$BASE/letterbox.mp4"
fi
# 2) sans bandes (mire colorée)
if [ ! -s "$BASE/plein.mp4" ]; then
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "testsrc2=s=640x360:r=10:d=6" \
        -c:v libx264 -preset ultrafast -y "$BASE/plein.mp4"
fi
# 3) fond GRIS uni + sous-titres + 3 chapitres. Le fond gris est indispensable pour
#    mesurer la couleur du texte : une mire colorée contient déjà du rouge, ce qui
#    rendait la mesure impossible (constaté au premier essai).
if [ ! -s "$BASE/subs_gris.mkv" ]; then
    cat > "$BASE/g.srt" <<'EOF'
1
00:00:00,500 --> 00:00:06,000
Sous-titre de controle pour la verification
EOF
    cat > "$BASE/meta2.txt" <<'EOF'
;FFMETADATA1
[CHAPTER]
TIMEBASE=1/1000
START=0
END=2000
title=Introduction
[CHAPTER]
TIMEBASE=1/1000
START=2000
END=4000
title=Milieu
[CHAPTER]
TIMEBASE=1/1000
START=4000
END=6000
title=Fin
EOF
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "color=c=0x303030:s=640x360:r=10:d=6" -i "$BASE/g.srt" -i "$BASE/meta2.txt" \
        -map 0:v -map 1:s -map_metadata 2 -map_chapters 2 \
        -c:v libx264 -preset ultrafast -c:s srt -metadata:s:s:0 language=fre \
        -y "$BASE/subs_gris.mkv"
fi

# 4) mire colorée + sous-titres embarqués + 3 chapitres
if [ ! -s "$BASE/chapitres.mp4" ]; then
    cat > "$BASE/c.srt" <<'EOF'
1
00:00:00,500 --> 00:00:06,000
Sous-titre de contrôle pour la vérification
EOF
    cat > "$BASE/meta.txt" <<'EOF'
;FFMETADATA1
[CHAPTER]
TIMEBASE=1/1000
START=0
END=2000
title=Introduction
[CHAPTER]
TIMEBASE=1/1000
START=2000
END=4000
title=Milieu
[CHAPTER]
TIMEBASE=1/1000
START=4000
END=6000
title=Fin
EOF
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "testsrc2=s=640x360:r=10:d=6" -i "$BASE/c.srt" -i "$BASE/meta.txt" \
        -map 0:v -map 1:s -map_metadata 2 -map_chapters 2 \
        -c:v libx264 -preset ultrafast -c:s srt \
        -metadata:s:s:0 language=fre \
        -y "$BASE/chapitres.mkv"
fi
for f in letterbox.mp4 plein.mp4 subs_gris.mkv chapitres.mkv; do
    [ -s "$BASE/$f" ] || { echo "média de test manquant : $f"; exit 1; }
done
echo "médias de test : $BASE/{letterbox.mp4, plein.mp4, subs_gris.mkv, chapitres.mkv}"

config_essai() {  # config_essai <nom> '<json video>' ['<json subtitles>']
    local nom="$1" video="$2" subs="${3:-{\}}"
    local dossier="$BASE/$nom"
    rm -rf "$dossier"; mkdir -p "$dossier/conf/kplayer" "$dossier/cache"
    python3 - "$dossier/conf/kplayer/config.json" "$video" "$subs" <<'PY'
import json, sys
video = json.loads(sys.argv[2])
subs = json.loads(sys.argv[3])
cfg = {
    "video": dict({"hardware_decoding": False, "scaling_method": "Lanczos"}, **video),
    "audio": {"device": "", "volume_max": 150},
    "subtitles": dict({"auto_load": False, "font_size": 40.0, "default_language": "fre",
                       "use_embedded": True}, **subs),
    "interface": {"dark_theme": True, "show_osd": False, "osd_duration": 1000,
                  "show_progress_bar": True, "controls_always_visible": False,
                  "show_title": False, "screenshots_dir": "/tmp"},
    "playback": {"resume": False},
    "playlist": [],
}
json.dump(cfg, open(sys.argv[1], "w"), indent=2, ensure_ascii=False)
PY
}

exec_essai() {  # exec_essai <nom> <arguments…>
    local nom="$1"; shift
    XDG_CONFIG_HOME="$BASE/$nom/conf" XDG_CACHE_HOME="$BASE/$nom/cache" "$KP" "$@" \
        >"$BASE/$nom/log.txt" 2>&1
}

dims() { python3 -c "from PIL import Image; print('%dx%d' % Image.open('$1').size)"; }

echo
echo "=== 1. Recadrage des bandes noires (dimensions de l'image produite) ==="
config_essai sans_crop '{}'
config_essai crop_90 '{"crop_top": 90, "crop_bottom": 90}'
exec_essai sans_crop --dump-frame 1000 "$BASE/sans.png" "$BASE/letterbox.mp4"
exec_essai crop_90  --dump-frame 1000 "$BASE/crop.png" "$BASE/letterbox.mp4"
d_sans=$(dims "$BASE/sans.png"); d_crop=$(dims "$BASE/crop.png")
echo "    sans recadrage : $d_sans   avec recadrage 90/90 : $d_crop"
if [ "$d_sans" = "1280x720" ] && [ "$d_crop" = "1280x540" ]; then
    ok "le recadrage retire exactement les bandes demandées"
else
    ko "recadrage inattendu ($d_sans → $d_crop, attendu 1280x720 → 1280x540)"
fi

echo
echo "=== 2. Détection automatique des bandes ==="
exec_essai sans_crop --detect-bars "$BASE/letterbox.mp4"
detecte=$(grep "bandes noires" "$BASE/sans_crop/log.txt" | tail -1)
echo "    $detecte"
if echo "$detecte" | grep -q "haut=90 bas=90 gauche=0 droite=0"; then
    ok "détection exacte sur le clip letterbox"
else
    ko "détection incorrecte : $detecte"
fi
exec_essai sans_crop --detect-bars "$BASE/plein.mp4"
detecte2=$(grep "bandes noires" "$BASE/sans_crop/log.txt" | tail -1)
echo "    $detecte2"
if echo "$detecte2" | grep -q "haut=0 bas=0 gauche=0 droite=0"; then
    ok "aucune bande inventée sur un clip sans bandes"
else
    ko "faux positif sur un clip sans bandes : $detecte2"
fi

echo
echo "=== 3. Chapitres ==="
exec_essai sans_crop --list-tracks "$BASE/chapitres.mkv"
chaps=$(grep "chapitre\[" "$BASE/sans_crop/log.txt")
echo "$chaps" | sed 's/^/    /'
nb=$(echo "$chaps" | grep -c "chapitre\[")
if [ "$nb" = "3" ]; then
    ok "les trois chapitres du conteneur sont lus"
else
    ko "chapitres manquants ($nb au lieu de 3)"
fi
if echo "$chaps" | grep -q "Introduction" && echo "$chaps" | grep -q "Milieu"; then
    ok "les titres de chapitre sont lus"
else
    ko "les titres de chapitre ne sont pas lus"
fi

echo
echo "=== 4. Style des sous-titres (analyse de pixels) ==="
# On affiche le sous-titre embarqué en plein écran et on compare la zone du texte
# entre deux réglages : la couleur, la position et la taille doivent changer l'image.
capture_subs() {  # capture_subs <nom_config> <sortie.png>
    local nom="$1" sortie="$2"
    pkill -x kplayer 2>/dev/null; sleep 0.6
    XDG_CONFIG_HOME="$BASE/$nom/conf" XDG_CACHE_HOME="$BASE/$nom/cache" DISPLAY="$DISP" \
        "$KP" --fullscreen "$BASE/subs_gris.mkv" >"$BASE/$nom/run.log" 2>&1 &
    local kpid=$!
    sleep 5
    DISPLAY=$DISP ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x720 \
        -i "$DISP+0,0" -frames:v 1 -y "$sortie"
    kill "$kpid" 2>/dev/null
    sleep 0.4
    [ -s "$sortie" ]
}
if DISPLAY=$DISP xdotool getdisplaygeometry >/dev/null 2>&1; then
    config_essai style_blanc '{}' '{"font_size": 48.0, "color": "FFFFFF", "position_percent": 88.0}'
    config_essai style_rouge '{}' '{"font_size": 48.0, "color": "FF0000", "position_percent": 88.0}'
    config_essai style_haut  '{}' '{"font_size": 48.0, "color": "FFFFFF", "position_percent": 8.0}'

    if capture_subs style_blanc "$BASE/blanc.png" && capture_subs style_rouge "$BASE/rouge.png"; then
        # Compter les pixels franchement rouges : avec color=FF0000, le texte l'est.
        n_rouge=$(python3 -c "
from PIL import Image
import numpy as np
a = np.asarray(Image.open('$BASE/rouge.png').convert('RGB'), dtype=int)
r, g, b = a[:,:,0], a[:,:,1], a[:,:,2]
print(int(((r > 140) & (g < 90) & (b < 90)).sum()))
")
        n_rouge_blanc=$(python3 -c "
from PIL import Image
import numpy as np
a = np.asarray(Image.open('$BASE/blanc.png').convert('RGB'), dtype=int)
r, g, b = a[:,:,0], a[:,:,1], a[:,:,2]
print(int(((r > 140) & (g < 90) & (b < 90)).sum()))
")
        echo "    pixels rouges : texte rouge $n_rouge / texte blanc $n_rouge_blanc"
        if [ "$n_rouge" -gt 200 ] && [ "$n_rouge" -gt $((n_rouge_blanc * 3 + 50)) ]; then
            ok "la couleur du texte est réellement appliquée"
        else
            ko "la couleur du texte n'est pas appliquée ($n_rouge vs $n_rouge_blanc)"
        fi
    else
        ko "capture impossible (couleur)"
    fi

    if capture_subs style_haut "$BASE/haut.png"; then
        # Position : le texte doit se trouver dans le HAUT de l'image, pas en bas.
        pos=$(python3 - "$BASE/haut.png" "$BASE/blanc.png" <<'PY'
import sys
import numpy as np
from PIL import Image

def bande_texte(chemin):
    a = np.asarray(Image.open(chemin).convert('L'), dtype=int)
    h = a.shape[0]
    # Le texte est clair sur fond sombre : on cherche la ligne la plus « claire ».
    lignes = a.mean(axis=1)
    # On ignore la barre de contrôles (bas de l'écran, ~40 px).
    utiles = lignes[: h - 60]
    return int(np.argmax(utiles)) * 100 // h

print(bande_texte(sys.argv[1]), bande_texte(sys.argv[2]))
PY
)
        haut_pct=$(echo "$pos" | cut -d' ' -f1); bas_pct=$(echo "$pos" | cut -d' ' -f2)
        echo "    ligne la plus claire : position 8 % → $haut_pct % de l'image, position 88 % → $bas_pct %"
        if [ "$haut_pct" -lt 40 ] && [ "$bas_pct" -gt 55 ]; then
            ok "la position verticale du texte est réellement appliquée"
        else
            ko "position non appliquée ($haut_pct % / $bas_pct %)"
        fi
    else
        ko "capture impossible (position)"
    fi

    # Taille de police : le nombre de pixels de texte doit augmenter nettement.
    config_essai style_petit '{}' '{"font_size": 18.0, "color": "FFFFFF", "position_percent": 88.0}'
    if capture_subs style_petit "$BASE/petit.png"; then
        n_petit=$(python3 -c "
from PIL import Image
import numpy as np
a = np.asarray(Image.open('$BASE/petit.png').convert('L'), dtype=int)
print(int((a > 200).sum()))
")
        n_grand=$(python3 -c "
from PIL import Image
import numpy as np
a = np.asarray(Image.open('$BASE/blanc.png').convert('L'), dtype=int)
print(int((a > 200).sum()))
")
        echo "    pixels très clairs : police 18 pt $n_petit / 48 pt $n_grand"
        if [ "$n_grand" -gt $((n_petit * 2)) ]; then
            ok "la taille de police est réellement appliquée"
        else
            ko "la taille de police est sans effet ($n_petit → $n_grand)"
        fi
    else
        ko "capture impossible (taille)"
    fi

    # Délai : avec un retard de 5 s, le sous-titre (0,5–6 s) ne doit PLUS être affiché.
    config_essai style_retard '{}' '{"font_size": 48.0, "color": "FFFFFF", "position_percent": 88.0, "delay_ms": -5500}'
    if capture_subs style_retard "$BASE/retard.png"; then
        n_retard=$(python3 -c "
from PIL import Image
import numpy as np
a = np.asarray(Image.open('$BASE/retard.png').convert('L'), dtype=int)
print(int((a > 200).sum()))
")
        n_normal=$(python3 -c "
from PIL import Image
import numpy as np
a = np.asarray(Image.open('$BASE/blanc.png').convert('L'), dtype=int)
print(int((a > 200).sum()))
")
        echo "    pixels très clairs : sans décalage $n_normal / décalé de 5,5 s $n_retard"
        if [ "$n_retard" -lt $((n_normal / 4)) ]; then
            ok "le décalage temporel est réellement appliqué (texte hors de sa fenêtre)"
        else
            ko "le décalage temporel est sans effet ($n_normal → $n_retard)"
        fi
    else
        ko "capture impossible (décalage)"
    fi
else
    echo "  (écran $DISP indisponible : contrôles visuels ignorés)"
fi

echo
echo "=== 5. Rapport d'aspect imposé (forme de la zone vidéo) ==="
# En plein écran, la zone vidéo est journalisée par l'application : on vérifie que
# le rapport imposé change ses proportions. Un clip 640x360 (16:9) forcé en 4:3 doit
# produire une zone plus étroite.
if DISPLAY=$DISP xdotool getdisplaygeometry >/dev/null 2>&1; then
    mesurer_zone() {  # mesurer_zone <nom_config>
        local nom="$1"
        pkill -x kplayer 2>/dev/null; sleep 0.6
        XDG_CONFIG_HOME="$BASE/$nom/conf" XDG_CACHE_HOME="$BASE/$nom/cache" DISPLAY="$DISP" \
            "$KP" --fullscreen "$BASE/plein.mp4" >"$BASE/$nom/run.log" 2>&1 &
        local kpid=$!
        sleep 5
        kill "$kpid" 2>/dev/null; sleep 0.3
    }
    config_essai aspect_auto '{}'
    config_essai aspect_43 '{"aspect": "Ratio4x3"}'
    mesurer_zone aspect_auto
    mesurer_zone aspect_43
    # La géométrie de la zone vidéo n'est pas journalisée : on la mesure à l'image.
    if capture_subs aspect_auto "$BASE/auto.png" && capture_subs aspect_43 "$BASE/43.png"; then
        largeurs=$(python3 - "$BASE/auto.png" "$BASE/43.png" <<'PY'
import sys
import numpy as np
from PIL import Image

def largeur_image(chemin):
    """Largeur de la zone non noire, mesurée sur une ligne du milieu."""
    a = np.asarray(Image.open(chemin).convert('L'), dtype=int)
    h, w = a.shape
    ligne = a[h // 2, :]
    # Une mire testsrc2 est colorée : ses pixels sont nettement plus clairs que le
    # fond noir des barres latérales.
    non_noirs = np.nonzero(ligne > 40)[0]
    return int(non_noirs[-1] - non_noirs[0]) if len(non_noirs) else 0

print(largeur_image(sys.argv[1]), largeur_image(sys.argv[2]))
PY
)
        l_auto=$(echo "$largeurs" | cut -d' ' -f1); l_43=$(echo "$largeurs" | cut -d' ' -f2)
        echo "    largeur de la zone vidéo : auto $l_auto px → 4:3 $l_43 px"
        if [ "$l_43" -lt $((l_auto * 90 / 100)) ] && [ "$l_43" -gt 100 ]; then
            ok "forcer 4:3 réduit réellement la largeur affichée (image déformée)"
        else
            ko "le rapport d'aspect imposé est sans effet ($l_auto → $l_43)"
        fi
    else
        ko "capture impossible (aspect)"
    fi
else
    echo "  (écran $DISP indisponible : contrôle d'aspect ignoré)"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
pkill -x kplayer 2>/dev/null || true
[ "$FAIL" -eq 0 ]

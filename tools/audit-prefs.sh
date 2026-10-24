#!/usr/bin/env bash
# audit-prefs.sh — vérifie la fenêtre « Préférences » : bornage à l'écran,
# dépliage/repliage des sections, défilement, et absence de réglage inerte.
#
# Principe : aucune coordonnée devinée. Chaque section journalise la position de
# son bandeau (`prefs section <id> rect [[x1 y1] - [x2 y2]] ouvert=<bool>`, niveau
# debug, cible `kinterface`) : le script clique à cet endroit et relit l'état dans
# le journal. Le bornage se vérifie sur la géométrie de la fenêtre, elle aussi
# journalisée.
#
# Usage : tools/audit-prefs.sh [écran]        (par défaut :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
LOG=/tmp/audit-prefs.log
ETAT=/tmp/audit-prefs-etat
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=kplayer=info,kinterface=debug

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
nettoyer() { sed 's/\x1b\[[0-9;]*m//g' "$LOG"; }

# --- État isolé : l'audit ne doit pas dépendre de la configuration de l'utilisateur ---
rm -rf "$ETAT"; mkdir -p "$ETAT/conf/kplayer" "$ETAT/cache"
python3 - "$ETAT/conf/kplayer/config.json" <<'PY'
import json, sys
json.dump({
    "video": {"hardware_decoding": True, "scaling_method": "Lanczos", "deband": False,
              "denoise": 0, "deinterlace": False, "brightness": 0.0, "contrast": 1.0,
              "saturation": 1.0, "gamma": 1.0, "deband_range": 16,
              "deband_threshold": 0.02, "deinterlace_parity": "Auto"},
    # Égaliseur activé sur le profil « Basses renforcées » : c'est ce qui fait
    # dessiner les 18 curseurs de bande, donc ce que l'audit doit pouvoir vérifier.
    "audio": {"device": "", "volume_max": 150, "eq_enabled": True,
              "eq_gains": [2.0, 2.0, 1.8, 1.5, 1.3, 1.1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]},
    "subtitles": {"auto_load": True, "font_size": 24.0, "default_language": "fr"},
    "interface": {"dark_theme": True, "show_osd": True, "osd_duration": 3000,
                  "show_progress_bar": True, "controls_always_visible": False,
                  "show_title": False, "screenshots_dir": "/tmp/audit-prefs-captures"},
    "playback": {"resume": False},
    "playlist": [],
}, open(sys.argv[1], "w"), indent=2, ensure_ascii=False)
PY
export XDG_CONFIG_HOME="$ETAT/conf" XDG_CACHE_HOME="$ETAT/cache"

pkill -x kplayer 2>/dev/null; sleep 1
DISPLAY=$DISP ./target/release/kplayer /media/blaise/LINUX_DATA/test-60s.mp4 >"$LOG" 2>&1 &
KPID=$!
sleep 6
WID=$(DISPLAY=$DISP xdotool search --pid "$KPID" --name KPlayer 2>/dev/null | head -1)
[ -z "$WID" ] && WID=$(DISPLAY=$DISP xdotool search --name KPlayer | head -1)
if [ -z "$WID" ]; then echo "fenêtre introuvable"; kill "$KPID" 2>/dev/null; exit 1; fi
eval "$(DISPLAY=$DISP xdotool getwindowgeometry --shell "$WID")"
APP_W=$WIDTH; APP_H=$HEIGHT
ORIG_X=$X; ORIG_Y=$Y
echo "fenêtre de l'application : ${APP_W}x${APP_H} en ($ORIG_X,$ORIG_Y)"

focus() {
    DISPLAY=$DISP xdotool windowactivate --sync "$WID" 2>/dev/null \
        || DISPLAY=$DISP xdotool windowfocus --sync "$WID" 2>/dev/null || true
    DISPLAY=$DISP xdotool mousemove $((ORIG_X + APP_W / 2)) $((ORIG_Y + APP_H / 2))
    sleep 0.3
}
ouvrir_prefs() {
    focus
    DISPLAY=$DISP xdotool key --clearmodifiers F5
    sleep 1.5
}

# `rect_en_cours [motif]` : dernière géométrie journalisée pour ce motif.
dernier_rect() { nettoyer | grep "$1" | tail -1; }
# Conversion « [[x1 y1] - [x2 y2]] » → "x1 y1 x2 y2"
plat() { python3 -c "
import re,sys
m=re.search(r'\[\[([0-9.-]+) ([0-9.-]+)\] - \[([0-9.-]+) ([0-9.-]+)\]\]', sys.argv[1])
print(' '.join(m.groups()) if m else '')" "$1"; }

# --- 1. Ouverture et bornage -------------------------------------------------
ouvrir_prefs
ligne=$(dernier_rect "fenetre preférences rect")
if [ -z "$ligne" ]; then
    ko "la fenêtre Préférences ne s'ouvre pas (aucune géométrie journalisée)"
else
    read -r x1 y1 x2 y2 <<<"$(plat "$ligne")"
    echo "  fenêtre Préférences : (${x1%.*},${y1%.*}) → (${x2%.*},${y2%.*}) dans une fenêtre de ${APP_W}x${APP_H}"
    if python3 -c "
import sys
x1,y1,x2,y2 = map(float, '$x1 $y1 $x2 $y2'.split())
assert x1 >= -1 and y1 >= -1, 'déborde en haut/gauche'
assert x2 <= $APP_W + 1, 'déborde à droite'
assert y2 <= $APP_H + 1, 'déborde en bas'
"; then
        ok "la fenêtre tient entièrement dans la fenêtre de l'application"
    else
        ko "la fenêtre déborde de l'écran ($x1,$y1 → $x2,$y2)"
    fi
fi

# --- 2. Déplier toutes les sections puis vérifier le bornage ----------------
# Repérage des deux boutons dans la dernière ligne journalisée :
#   prefs boutons replier [[..] - [..]] deplier [[..] - [..]]
rects=$(nettoyer | grep "prefs boutons replier" | tail -1)
r_replier=$(echo "$rects" | sed -E 's/.*replier \[\[([^]]*)\] - \[([^]]*)\]\].*/\1 \2/')
r_deplier=$(echo "$rects" | sed -E 's/.*deplier \[\[([^]]*)\] - \[([^]]*)\]\].*/\1 \2/')
# Nombre de sections distinctes dans un état donné, parmi les lignes écrites
# depuis la ligne `$1`. On analyse le TEXTE du message et non des numéros de
# champ : la ligne commence par l'horodatage puis « DEBUG kinterface: », ce qui
# décale tous les champs (piège rencontré : le comptage renvoyait 1 au lieu de 6).
nb_sections_etat() {  # nb_sections_etat <ligne_marque> <true|false>
    sed -n "$(($1 + 1)),\$p" "$LOG" | sed 's/\x1b\[[0-9;]*m//g' \
        | python3 -c "
import re, sys
etat = sys.argv[1]
ids = set()
for ligne in sys.stdin:
    m = re.search(r'prefs section (\w+) rect .* ouvert=(true|false)', ligne)
    if m and m.group(2) == etat:
        ids.add(m.group(1))
print(len(ids))
" "$2"
}
marque() { wc -l <"$LOG"; }

clic_centre() {  # clic_centre "x1 y1 x2 y2"
    local a b c d
    read -r a b c d <<<"$(echo "$1" | tr -d '[]')"
    local mx my
    mx=$(python3 -c "print(int(($a+$c)/2))")
    my=$(python3 -c "print(int(($b+$d)/2))")
    DISPLAY=$DISP xdotool mousemove $((ORIG_X + mx)) $((ORIG_Y + my))
    sleep 0.2
    DISPLAY=$DISP xdotool click 1
    sleep 1.2
}
if [ -n "$r_deplier" ]; then
    m=$(marque)
    clic_centre "$r_deplier"
    ouverts=$(nb_sections_etat "$m" true)
    total_sections=$(grep -c 'id: "' src/settings.rs)
    if [ "$ouverts" -ge 2 ]; then
        ok "« tout déplier » ouvre les sections ($ouverts sections ouvertes vues)"
    else
        ko "« tout déplier » ne déplie rien ($ouverts)"
    fi
    sleep 0.5
    ligne=$(dernier_rect "fenetre preférences rect")
    read -r x1 y1 x2 y2 <<<"$(plat "$ligne")"
    echo "  fenêtre tout déplié : (${x1%.*},${y1%.*}) → (${x2%.*},${y2%.*})"
    if python3 -c "
x1,y1,x2,y2 = map(float, '$x1 $y1 $x2 $y2'.split())
raise SystemExit(0 if (x2 <= $APP_W + 1 and y2 <= $APP_H + 1 and y1 >= -1) else 1)
"; then
        ok "tout déplié, la fenêtre reste dans l'écran (le défilement prend le relais)"
    else
        ko "tout déplié, la fenêtre déborde ($x1,$y1 → $x2,$y2)"
    fi
    # --- 3. Le contenu défile réellement
    DISPLAY=$DISP ffmpeg -hide_banner -loglevel error -f x11grab -video_size ${APP_W}x${APP_H} \
        -i "$DISP+$ORIG_X,$ORIG_Y" -frames:v 1 -y /tmp/audit-prefs-avant.png
    DISPLAY=$DISP xdotool mousemove $((ORIG_X + 200)) $((ORIG_Y + APP_H - 200))
    DISPLAY=$DISP xdotool click 5; DISPLAY=$DISP xdotool click 5; DISPLAY=$DISP xdotool click 5
    sleep 1
    DISPLAY=$DISP ffmpeg -hide_banner -loglevel error -f x11grab -video_size ${APP_W}x${APP_H} \
        -i "$DISP+$ORIG_X,$ORIG_Y" -frames:v 1 -y /tmp/audit-prefs-apres.png
    ecart=$(python3 -c "
from PIL import Image
import numpy as np
a=np.asarray(Image.open('/tmp/audit-prefs-avant.png').convert('L'),dtype=int)
b=np.asarray(Image.open('/tmp/audit-prefs-apres.png').convert('L'),dtype=int)
print(round(float(np.abs(a-b).mean()),2))
")
    if python3 -c "raise SystemExit(0 if float('$ecart') > 0.5 else 1)"; then
        ok "la molette fait défiler le contenu (écart d'image $ecart/255)"
    else
        ko "le contenu ne défile pas (écart $ecart/255)"
    fi
    # --- 4. « tout replier » referme les sections
    m=$(marque)
    clic_centre "$r_replier"
    ligne=$(dernier_rect "fenetre preférences rect")
    read -r x1 y1 x2 y2 <<<"$(plat "$ligne")"
    h_replie=$(python3 -c "print(int($y2-$y1))")
    echo "  fenêtre tout replié : hauteur ${h_replie} px"
    fermes=$(nb_sections_etat "$m" false)
    if [ "$fermes" -ge 2 ]; then
        ok "« tout replier » referme les sections ($fermes sections fermées vues)"
    else
        ko "« tout replier » ne referme rien ($fermes)"
    fi
else
    ko "boutons « tout déplier / tout replier » introuvables dans le journal"
fi

# --- 5. Un clic sur un bandeau bascule UNE section --------------------------
# La fenêtre est encore ouverte ici (le « tout replier » ne fait que replier les
# sections) : inutile — et nuisible — de renvoyer F5, qui la refermerait.
ligne=$(nettoyer | grep "prefs section audio rect" | tail -1)
if [ -n "$ligne" ]; then
    etat_avant=$(echo "$ligne" | grep -o "ouvert=[a-z]*")
    r=$(echo "$ligne" | sed -E 's/.*rect \[\[([^]]*)\] - \[([^]]*)\]\].*/\1 \2/')
    clic_centre "$r"
    ligne2=$(nettoyer | grep "prefs section audio rect" | tail -1)
    etat_apres=$(echo "$ligne2" | grep -o "ouvert=[a-z]*")
    if [ -n "$etat_apres" ] && [ "$etat_avant" != "$etat_apres" ]; then
        ok "clic sur le bandeau « Audio » : $etat_avant → $etat_apres"
    else
        ko "le clic sur le bandeau ne bascule pas la section ($etat_avant → $etat_apres)"
    fi
else
    ko "section « audio » absente du journal (bandeau non cliquable ?)"
fi

# --- 6. Aucune section ne journalise d'erreur de filtres --------------------
if nettoyer | grep -q "Chaîne de filtres inapplicable"; then
    ko "une chaîne de filtres a été refusée par FFmpeg"
else
    ok "aucune chaîne de filtres refusée"
fi

# --- 7. Égaliseur : profil, bandes, et absence de dérive des gains ----------
# Le dépliage de la section Audio est nécessaire pour que les curseurs existent.
ligne=$(nettoyer | grep "prefs section audio rect" | tail -1)
if [ -n "$ligne" ] && echo "$ligne" | grep -q "ouvert=false"; then
    r=$(echo "$ligne" | sed -E 's/.*rect \[\[([^]]*)\] - \[([^]]*)\]\].*/\1 \2/')
    clic_centre "$r"
fi
# Faire défiler jusqu'à l'égaliseur : la roue au-dessus de la liste.
# `defiler <nombre>` : molette au-dessus de la LISTE des sections. Le point choisi
# doit être sous le bandeau fixe (« tout déplier ») et au-dessus du bord bas :
# viser le haut de la fenêtre ne fait rien (constaté : curseur d'égaliseur jamais
# atteint, donc test en échec pour une raison sans rapport avec le code).
defiler() {  # defiler <nombre> [sens] : sens = bas (défaut) ou haut
    local n="${1:-12}" sens="${2:-bas}"
    local bouton=5
    [ "$sens" = "haut" ] && bouton=4
    DISPLAY=$DISP xdotool mousemove $((ORIG_X + 250)) $((ORIG_Y + 380))
    for _ in $(seq 1 "$n"); do DISPLAY=$DISP xdotool click $bouton; sleep 0.08; done
    sleep 1
}
defiler_eq() { defiler 14; }
defiler_eq

if nettoyer | grep -q "prefs eq profil"; then
    profil=$(nettoyer | grep "prefs eq profil" | tail -1 | sed 's/.*profil //')
    echo "  profil affiché : $profil"
    case "$profil" in
        Neutre|"Basses renforcées"|"Aigus renforcés"|"Voix (dialogue)"|"Isolation / écoute tardive"|Personnalisé)
            ok "le profil d'égalisation est identifié (« $profil »)" ;;
        *) ko "profil d'égalisation inattendu (« $profil »)" ;;
    esac

    echo "  curseurs de bande dessinés : $(nettoyer | grep -c 'prefs eq bande')"

    # Déplacer la 3e bande (262 Hz) vers la gauche doit changer le gain stocké.
    ligne=$(nettoyer | grep "prefs eq bande 3 rect" | tail -1)
    r=$(echo "$ligne" | sed -E 's/.*rect \[\[([^]]*)\] - \[([^]]*)\]\].*/\1 \2/')
    read -r a b c d <<<"$(echo "$r" | tr -d '[]')"
    mx=$(python3 -c "print(int(($a+$c)/2))"); my=$(python3 -c "print(int(($b+$d)/2))")
    gain_avant=$(echo "$ligne" | sed 's/.*valeur=//')
    # Un curseur hors de la zone visible (y < 90 : sous le bandeau fixe) ne peut pas
    # être saisi : on fait défiler jusqu'à ce qu'il le soit.
    essai=0
    while { [ "$my" -lt 95 ] || [ "$my" -gt $((APP_H - 130)) ]; } && [ "$essai" -lt 8 ]; do
        essai=$((essai + 1))
        if [ "$my" -lt 95 ]; then
            echo "  (curseur à y=$my, sous le bandeau : on remonte)"
            defiler 4 haut
        else
            echo "  (curseur à y=$my, sous la zone visible : on descend)"
            defiler 4 bas
        fi
        ligne=$(nettoyer | grep "prefs eq bande 3 rect" | tail -1)
        r=$(echo "$ligne" | sed -E 's/.*rect \[\[([^]]*)\] - \[([^]]*)\]\].*/\1 \2/')
        read -r a b c d <<<"$(echo "$r" | tr -d '[]')"
        mx=$(python3 -c "print(int(($a+$c)/2))"); my=$(python3 -c "print(int(($b+$d)/2))")
    done
    echo "  curseur à y=$my (zone visible)"
    DISPLAY=$DISP xdotool mousemove $((ORIG_X + mx)) $((ORIG_Y + my))
    sleep 0.2
    DISPLAY=$DISP xdotool mousedown 1; sleep 0.2
    DISPLAY=$DISP xdotool mousemove $((ORIG_X + mx - 40)) $((ORIG_Y + my)); sleep 0.2
    DISPLAY=$DISP xdotool mouseup 1
    sleep 1.2
    gain_apres=$(nettoyer | grep "prefs eq bande 3 rect" | tail -1 | sed 's/.*valeur=//')
    echo "  bande 3 : $gain_avant dB → $gain_apres dB"
    if python3 -c "
import sys
try:
    a, b = float('$gain_avant'), float('$gain_apres')
except ValueError:
    sys.exit(1)
sys.exit(0 if b < a - 1.0 else 1)
"; then
        ok "déplacer un curseur de bande change réellement le réglage"
    else
        ko "le curseur de bande ne change pas le réglage ($gain_avant → $gain_apres)"
    fi

    # Dérive : rouvrir les préférences ne doit pas modifier les gains.
    g1=$(nettoyer | grep "prefs eq bande 1 rect" | tail -1 | sed 's/.*valeur=//')
    DISPLAY=$DISP xdotool key --clearmodifiers Escape; sleep 1
    ouvrir_prefs
    defiler_eq
    g2=$(nettoyer | grep "prefs eq bande 1 rect" | tail -1 | sed 's/.*valeur=//')
    echo "  bande 1 après réouverture : $g1 → $g2 dB"
    if python3 -c "
import sys
try:
    a, b = float('$g1'), float('$g2')
except ValueError:
    sys.exit(1)
sys.exit(0 if abs(a - b) < 0.2 else 1)
"; then
        ok "rouvrir les préférences ne modifie pas les gains (pas de dérive d'arrondi)"
    else
        ko "les gains dérivent à chaque ouverture ($g1 → $g2)"
    fi
else
    ko "l'égaliseur n'est pas dessiné (aucune géométrie de bande journalisée)"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
kill "$KPID" 2>/dev/null || true
pkill -x kplayer 2>/dev/null || true
[ "$FAIL" -eq 0 ]

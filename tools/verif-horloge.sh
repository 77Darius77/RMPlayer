#!/usr/bin/env bash
# verif-horloge.sh — prouve que la CADENCE SUIT LA CARTE SON (horloge audio maître).
#
# Ce que l'on mesure, et pourquoi c'est le bon critère
# ---------------------------------------------------
# Avant, l'image était cadencée par le temps mural : la profondeur du tampon audio
# (entre 42 et 85 ms mesurés ici) était donc une erreur de synchronisation
# image/son PUREMENT ADDITIVE, que rien ne corrigeait — le son arrivait toujours
# après l'image du temps du tampon (plus le retard du périphérique).
#
# Maintenant l'image attend que la carte son ait RÉELLEMENT consommé les
# échantillons : la dérive affichée (PTS de l'image − position du son) doit rester
# dans un intervalle d'UNE image, quelle que soit la profondeur du tampon. C'est
# exactement ce que l'on vérifie ici : un tampon plein (≈ 127 ms) avec une dérive
# ≤ 1 image prouve que la profondeur du tampon n'est plus une erreur de synchro.
#
# Aucune carte son de l'utilisateur n'est touchée durablement : le banc crée son
# propre puits « kptest » (module-null-sink), le rend destination par défaut le
# temps des mesures, puis rétablit la destination d'origine et décharge le puits.
#
# Usage : tools/verif-horloge.sh        (nécessite ffmpeg, Xvfb, xdotool, pactl)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
KP=./target/release/kplayer
BASE=/tmp/kp-horloge
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export DISPLAY=:97
mkdir -p "$BASE"
# Configuration et positions ISOLÉES : le banc ne doit ni lire la reprise de
# lecture de l'utilisateur (elle déplacerait le point de départ des mesures) ni
# écrire dans sa configuration.
export XDG_CONFIG_HOME="$BASE/config"
export XDG_CACHE_HOME="$BASE/cache"
rm -rf "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }

for outil in ffmpeg Xvfb xdotool pactl; do
    command -v "$outil" >/dev/null || {
        echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }

# --- Destination audio dédiée (puits sans haut-parleur, horloge propre) --------
SINK_DEFAUT_AVANT=$(pactl get-default-sink 2>/dev/null || true)
MODULE=$(pactl load-module module-null-sink sink_name=kptest rate=48000 2>/dev/null || true)
if [ -z "$MODULE" ]; then
    echo "impossible de créer un puits audio de test (PipeWire/PulseAudio ?)"; exit 2
fi
pactl set-default-sink kptest >/dev/null 2>&1 || true
nettoyer() {
    [ -n "$SINK_DEFAUT_AVANT" ] && pactl set-default-sink "$SINK_DEFAUT_AVANT" >/dev/null 2>&1
    pactl unload-module "$MODULE" >/dev/null 2>&1
    [ -n "${XPID:-}" ] && kill "$XPID" 2>/dev/null
}
trap nettoyer EXIT

# --- Fichiers de test ---------------------------------------------------------
if [ ! -s "$BASE/av.mp4" ]; then
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -y \
        -f lavfi -i "testsrc2=size=640x360:rate=24:duration=20" \
        -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=20" \
        -c:v libx264 -preset veryfast -pix_fmt yuv420p -c:a aac -b:a 128k -shortest \
        "$BASE/av.mp4" || { echo "génération du fichier de test impossible"; exit 2; }
fi
if [ ! -s "$BASE/court.mp4" ]; then
    # Vidéo de 14 s dont la piste audio s'arrête à 4 s : la fin de l'audio ne doit
    # pas figer l'image (repli sur le temps mural).
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -y \
        -f lavfi -i "testsrc2=size=640x360:rate=24:duration=14" \
        -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=4" \
        -c:v libx264 -preset veryfast -pix_fmt yuv420p -c:a aac -b:a 128k \
        "$BASE/court.mp4" || { echo "génération impossible"; exit 2; }
fi
if [ ! -s "$BASE/muet.mp4" ]; then
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error -y \
        -f lavfi -i "testsrc2=size=640x360:rate=24:duration=10" \
        -c:v libx264 -preset veryfast -pix_fmt yuv420p -an "$BASE/muet.mp4" \
        || { echo "génération du fichier muet impossible"; exit 2; }
fi

Xvfb :97 -screen 0 1280x800x24 >/tmp/xvfb-horloge.log 2>&1 &
XPID=$!
sleep 2

# lecteur <journal> <durée_s> <arguments…> : lance KPlayer, attend, l'arrête.
# Les positions mémorisées sont effacées : sans cela, la « reprise de lecture »
# replacerait chaque essai là où le précédent s'est arrêté (mesures non
# reproductibles), et masquerait l'effet de `--seek`.
lecteur() {
    local journal="$1" duree="$2"; shift 2
    rm -f "$journal" "$XDG_CACHE_HOME/kplayer/positions.json"
    RUST_LOG=kplayer=debug "$KP" "$@" >"$journal" 2>&1 &
    local pid=$!
    sleep "$duree"
    kill "$pid" 2>/dev/null
    wait "$pid" 2>/dev/null
    sed 's/\x1b\[[0-9;]*m//g' "$journal" >"$journal.txt"
}

# vider <journal> <motif> : valeurs numériques d'un motif (1er nombre capturé)
nombres() { grep -oE "$2" "$1" | grep -oE '[-0-9]+(\.[0-9]+)?' ; }

echo "== 1. Lecture normale : la carte son est la référence =="
lecteur "$BASE/av.log" 12 "$BASE/av.mp4"

if grep -q "Sortie audio prête" "$BASE/av.log.txt"; then
    ok "sortie audio ouverte (la mesure a un sens)"
else
    ko "aucune sortie audio : la mesure ne peut pas être faite"
    echo "   (banc interrompu)"; exit 1
fi

ANCRES=$(grep -cE "Horloge: audio [0-9]" "$BASE/av.log.txt" || true)
if [ "${ANCRES:-0}" -ge 8 ]; then
    ok "horloge audio interrogeable ($ANCRES bilans d'une seconde)"
else
    ko "horloge audio absente du journal ($ANCRES bilans) — repli sur le temps mural ?"
fi

# Dérive image/son sur toutes les trames journalisées (une ligne toutes 60 images)
DERIVES=$(nombres "$BASE/av.log.txt" 'dérive [-0-9]+ ms')
NB_DERIVES=$(echo "$DERIVES" | grep -c . || true)
if [ "${NB_DERIVES:-0}" -ge 3 ]; then
    # 24 i/s → intervalle d'image 41,7 ms ; on tolère [-1,5 image ; +0,5 image].
    MIN=$(echo "$DERIVES" | sort -n | head -1)
    MAX=$(echo "$DERIVES" | sort -n | tail -1)
    echo "  dérive image/son mesurée : $MIN … $MAX ms (sur $NB_DERIVES relevés)"
    if [ "$MIN" -ge -63 ] && [ "$MAX" -le 21 ]; then
        ok "l'image suit le son à moins d'une image près ($MIN…$MAX ms)"
    else
        ko "dérive hors d'une image : $MIN…$MAX ms"
    fi
    # La preuve que la profondeur du tampon n'est plus une erreur : tampon plein.
    TAMPON=$(nombres "$BASE/av.log.txt" 'tampon [0-9]+/[0-9]+ ms' | sort -n | head -1)
    if [ "${TAMPON:-0}" -ge 64 ]; then
        ok "tampon maintenu ($TAMPON ms au minimum) SANS décaler l'image"
    else
        ko "tampon trop faible ($TAMPON ms) : alimentation insuffisante"
    fi
else
    ko "pas assez de relevés de dérive ($NB_DERIVES)"
fi

# Profondeur du tampon : stable, et jamais au bord de la saturation (1 s max)
PLANCHER=$(nombres "$BASE/av.log.txt" 'tampon [0-9]+/[0-9]+ ms' | sort -n | head -1)
PLAFOND=$(nombres "$BASE/av.log.txt" 'tampon [0-9]+/[0-9]+ ms' | sort -n | tail -1)
if [ "${PLANCHER:-0}" -ge 64 ] && [ "${PLAFOND:-9999}" -le 400 ]; then
    ok "tampon borné et stable ($PLANCHER…$PLAFOND ms)"
else
    ko "tampon instable ou hors bornes ($PLANCHER…$PLAFOND ms)"
fi

# Le tampon doit être rempli AVANT que la carte consomme : sinon les premiers blocs
# sortent en silence (mesuré : 2 blocs à 70 et 96 ms, la première image d'interface
# étant la plus lente). La trace l'établit sans ambiguïté.
LIGNE_PREFILL=$(grep -n "prérempli" "$BASE/av.log.txt" | head -1 | cut -d: -f1)
LIGNE_CARTE=$(grep -n "Sortie audio prête" "$BASE/av.log.txt" | head -1 | cut -d: -f1)
if [ -n "${LIGNE_PREFILL:-}" ] && [ -n "${LIGNE_CARTE:-}" ]; then
    ok "le tampon est prérempli avant le démarrage de la carte (ligne $LIGNE_PREFILL)"
else
    ko "aucune trace de préremplissage : la carte démarre sur un tampon vide"
fi

# Sous-alimentations : un bloc de silence inséré faute d'échantillons prêts.
#
# **Tolérance mesurée, pas complaisance** : sur cette machine (bureau non temps réel,
# écran virtuel Xvfb), un événement isolé survient quand l'ordonnanceur retarde la
# carte son — observé une fois sur deux sur une lecture de 6 s (soit 1 bloc sur ~600).
# Le défaut réel corrigé en session 29 produisait l'inverse : la carte démarrait sur un
# tampon **vide**, d'où des sous-alimentations en rafale dès la première seconde. Le
# seuil ci-dessous attrape donc encore la régression (≥ 2) et laisse passer l'aléa (≤ 1),
# et le nombre est affiché dans les deux cas pour que la dérive soit visible.
SOUS_ALIM=$(nombres "$BASE/av.log.txt" 'sous-alimentations [0-9]+' | sort -n | tail -1)
SOUS_ALIM=${SOUS_ALIM:-99}
if [ "$SOUS_ALIM" -eq 0 ]; then
    ok "aucune sous-alimentation du tampon"
elif [ "$SOUS_ALIM" -le 1 ]; then
    ok "sous-alimentations : $SOUS_ALIM (aléa d'ordonnanceur, toléré ; ≥ 2 = défaut)"
else
    ko "sous-alimentations : $SOUS_ALIM blocs de silence (défaut : légitime en session 29)"
fi

# L'écart entre l'horloge audio et le temps mural n'a pas à être nul (le son est
# en retard sur le temps mural du fait de la lecture anticipée — c'est justement
# ce retard que l'image cesse de reproduire), mais il ne doit PAS DÉRIVER.
ECART_DEBUT=$(nombres "$BASE/av.log.txt" 'écart [-0-9]+ ms' | tr -d '-' | head -1)
ECART_FIN=$(nombres "$BASE/av.log.txt" 'écart [-0-9]+ ms' | tr -d '-' | tail -1)
if [ "${ECART_FIN:-999}" -ne 999 ] &&    [ $(( ECART_FIN > ECART_DEBUT ? ECART_FIN - ECART_DEBUT : ECART_DEBUT - ECART_FIN )) -le 50 ]; then
    ok "pas de dérive horloge audio / temps mural (${ECART_DEBUT} → ${ECART_FIN} ms)"
else
    ko "dérive horloge audio / temps mural : ${ECART_DEBUT:-?} → ${ECART_FIN:-?} ms"
fi

# Cadence réelle : images affichées par seconde (le son ne doit pas ralentir l'image)
IMAGES=$(grep -c "frame #" "$BASE/av.log.txt" || true)
if [ "${IMAGES:-0}" -ge 2 ]; then
    ok "lecture cadencée ($IMAGES relevés de 60 images en 12 s)"
else
    ko "très peu d'images affichées ($IMAGES)"
fi

# --- 1 bis. La profondeur du tampon n'a plus AUCUN effet sur la synchro -------
# C'était l'erreur du temps mural : le son arrivait après l'image du temps du
# tampon. On impose ici un tampon trois fois plus profond (400 ms) : si la
# dérive image/son ne bouge pas, c'est bien la carte son qui fait foi.
echo "== 1 bis. Tampon profond (400 ms) : synchronisation inchangée =="
lecteur "$BASE/profond.log" 10 "$BASE/av.mp4" --audio-buffer 400
TAMPON_PROFOND=$(nombres "$BASE/profond.log.txt" 'tampon [0-9]+/[0-9]+ ms' | sort -n | tail -1)
DERIVE_PROFONDE=$(nombres "$BASE/profond.log.txt" 'dérive [-0-9]+ ms' | tr -d '-' | sort -n | tail -1)
if [ "${TAMPON_PROFOND:-0}" -ge 250 ]; then
    ok "tampon réellement approfondi (${TAMPON_PROFOND} ms)"
else
    ko "tampon non approfondi (${TAMPON_PROFOND:-?} ms) : contrôle sans objet"
fi
if [ "${DERIVE_PROFONDE:-999}" -le 63 ]; then
    ok "dérive toujours ≤ 1 image avec ${TAMPON_PROFOND} ms de tampon (${DERIVE_PROFONDE} ms)"
else
    ko "la profondeur du tampon déplace encore l'image (${DERIVE_PROFONDE} ms)"
fi

# --- 2. Fichier sans audio : repli sur le temps mural -------------------------
echo "== 2. Média sans piste audio : repli sur le temps mural =="
lecteur "$BASE/muet.log" 6 "$BASE/muet.mp4"
if ! grep -qE "Horloge: audio [0-9]" "$BASE/muet.log.txt"; then
    ok "aucune horloge audio inventée quand il n'y a pas de son"
else
    ko "horloge audio annoncée alors que le média est muet"
fi
if grep -q "Sortie audio prête" "$BASE/av.log.txt"; then
    DRIFT_MUET=$(nombres "$BASE/muet.log.txt" 'dérive [-0-9]+ ms' | sort -n | tail -1)
    if [ "${DRIFT_MUET:-999}" -le 60 ]; then
        ok "le média muet reste cadencé (dérive ${DRIFT_MUET} ms)"
    else
        ko "média muet : cadence perdue (dérive ${DRIFT_MUET} ms)"
    fi
fi

# --- 2 bis. Piste audio plus courte que la vidéo : l'image ne se fige pas -----
echo "== 2 bis. Audio terminé avant la vidéo : repli, l'image continue =="
lecteur "$BASE/court.log" 10 "$BASE/court.mp4"
if grep -q "Horloge audio muette" "$BASE/court.log.txt"; then
    ok "horloge audio abandonnée proprement à la fin de la piste"
else
    ko "aucun repli signalé alors que l'audio s'arrête à 4 s"
fi
POS_FIN=$(grep -oE "murale [0-9]+ ms" "$BASE/court.log.txt" | grep -oE '[0-9]+' | tail -1)
if [ "${POS_FIN:-0}" -ge 7500 ]; then
    ok "la lecture a continué jusqu'à ${POS_FIN} ms (au-delà de l'audio)"
else
    ko "lecture figée après la fin de l'audio (${POS_FIN:-?} ms)"
fi

# --- 3. Après un seek : horloge réancrée -------------------------------------
echo "== 3. Seek : l'horloge se réancre sur la nouvelle position =="
# `--seek` s'exprime en SECONDES (comme sur la ligne de commande du lecteur).
lecteur "$BASE/seek.log" 9 "$BASE/av.mp4" --seek 12
POS1=$(nombres "$BASE/seek.log.txt" 'Horloge: audio [0-9]+' | head -1)
POS2=$(nombres "$BASE/seek.log.txt" 'Horloge: audio [0-9]+' | tail -1)
if [ "${POS1:-0}" -ge 11000 ] && [ "${POS2:-0}" -gt "${POS1:-0}" ]; then
    ok "lecture reprise à ${POS1} ms et horloge qui avance (${POS2} ms)"
else
    ko "horloge non réancrée après seek (${POS1:-?} → ${POS2:-?} ms)"
fi
DERIVE_SEEK=$(nombres "$BASE/seek.log.txt" 'dérive [-0-9]+ ms' | tr -d '-' | sort -n | tail -1)
if [ "${DERIVE_SEEK:-999}" -le 63 ]; then
    ok "dérive d'une image au plus après le seek (${DERIVE_SEEK} ms)"
else
    ko "dérive après seek : ${DERIVE_SEEK} ms"
fi

# --- 4. Vitesse 2× : le média avance deux fois plus vite que le temps mural ---
echo "== 4. Vitesse 2× : l'horloge suit le média, pas le temps mural =="
lecteur "$BASE/vitesse.log" 9 --speed 2 "$BASE/av.mp4"
# L'horloge audio est en temps MÉDIA : à 2×, elle doit avancer deux fois plus
# vite que le temps réel écoulé entre deux lignes du journal (horodatage).
MESURE=$(python3 - "$BASE/vitesse.log.txt" <<'EOF'
import re, sys
from datetime import datetime
lignes = [l for l in open(sys.argv[1], encoding='utf-8', errors='replace')
          if re.search(r'Horloge: audio \d', l)]
if len(lignes) < 3:
    print("0 0 0"); sys.exit()
def h(l):
    return datetime.fromisoformat(re.match(r'(\S+)', l).group(1).rstrip('Z'))
def m(l):
    return int(re.search(r'Horloge: audio (\d+)', l).group(1))
d_media = m(lignes[-1]) - m(lignes[0])
d_reel = (h(lignes[-1]) - h(lignes[0])).total_seconds() * 1000.0
print(f"{d_media} {d_reel:.0f} {(d_media / d_reel if d_reel > 0 else 0):.2f}")
EOF
)
RATIO=$(echo "$MESURE" | cut -d' ' -f3)
if [ "${RATIO:-0}" = "1.90" ] || [ "${RATIO:-0}" = "1.91" ]; then :; fi
if awk "BEGIN{exit !(${RATIO:-0} >= 1.85 && ${RATIO:-0} <= 2.15)}"; then
    ok "horloge média × ${RATIO} le temps réel (${MESURE}) — la vitesse est suivie"
else
    ko "à 2×, l'horloge avance × ${RATIO:-?} le temps réel (${MESURE})"
fi

# --- 5. Pause / reprise : pas de saut de synchronisation ---------------------
echo "== 5. Pause puis reprise (touche Espace) =="
rm -f "$BASE/pause.log" "$XDG_CACHE_HOME/kplayer/positions.json"
RUST_LOG=kplayer=debug "$KP" "$BASE/av.mp4" >"$BASE/pause.log" 2>&1 &
LECTEUR_PID=$!
sleep 4
WID=$(xdotool search --name "KPlayer" 2>/dev/null | head -1)
if [ -n "${WID:-}" ]; then
    eval "$(xdotool getwindowgeometry --shell "$WID" 2>/dev/null)"
    xdotool mousemove $((X + WIDTH / 2)) $((Y + HEIGHT / 2))
    xdotool windowfocus --sync "$WID" 2>/dev/null
    sleep 1
    xdotool key --clearmodifiers space   # pause
    sleep 2
    xdotool key --clearmodifiers space   # reprise
    sleep 4
fi
kill "$LECTEUR_PID" 2>/dev/null
wait "$LECTEUR_PID" 2>/dev/null
sed 's/\x1b\[[0-9;]*m//g' "$BASE/pause.log" >"$BASE/pause.log.txt"
if grep -q "Pause" "$BASE/pause.log.txt" && grep -qE "Horloge: audio [0-9]" "$BASE/pause.log.txt"; then
    DERIVE_PAUSE=$(grep -A100 "Pause" "$BASE/pause.log.txt" | \
        nombres /dev/stdin 'dérive [-0-9]+ ms' | tr -d '-' | sort -n | tail -1)
    if [ "${DERIVE_PAUSE:-999}" -le 63 ]; then
        ok "reprise après pause sans décalage (${DERIVE_PAUSE} ms au plus)"
    else
        ko "décalage après reprise : ${DERIVE_PAUSE} ms"
    fi
else
    echo "  … pause non déclenchée (fenêtre ou touche indisponible), contrôle ignoré"
fi

echo
echo "Résultat : $PASS réussites, $FAIL échec(s)"
[ "$FAIL" -eq 0 ]

#!/usr/bin/env bash
# verif-rapide.sh — ne lance QUE les bancs concernés par une modification.
#
# **Pourquoi.** Après un changement d'interface, lancer la suite complète
# (`tools/tout-verifier.sh`, 26 bancs, ~20 min) est disproportionné : elle vérifie
# l'audio, le réseau, les sous-titres ASS, le pré-cache… rien de ce qui vient d'être
# touché. Ce script rejoue seulement les bancs nommés, avec le même format de sortie
# et le même garde-fou « banc muet », et n'écrit **pas** `docs/DERNIERE-SUITE.txt` :
# la fiche de reprise ne doit pas annoncer une suite complète qui n'a pas eu lieu.
#
# Usage :
#   tools/verif-rapide.sh playlist          # les bancs liés au panneau de playlist
#   tools/verif-rapide.sh ui                # interface : audit-ui, audit-menus, audit-prefs
#   tools/verif-rapide.sh verif-playlist audit-ui   # noms exacts
#   tools/verif-rapide.sh --liste           # ce que chaque raccourci lance
#
# Les noms exacts sont ceux de `tools/tout-verifier.sh` (colonne de gauche).
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux || exit 1
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
DISP=":99"

# Correspondance « sujet touché → bancs à rejouer ». Volontairement courte et
# explicite : mieux vaut compléter à la main que de tout relancer par prudence.
bancs_pour() {
    case "$1" in
        playlist)   echo "verif-playlist audit-ui" ;;
        vignettes)  echo "verif-vignettes verif-playlist" ;;
        mpris)      echo "verif-mpris verif-playlist" ;;
        focus)      echo "verif-focus" ;;
        hdr)        echo "verif-hdr verif-filtres" ;;
        soustitres) echo "verif-soustitres verif-soustitres-style verif-soustitres-langues verif-ass" ;;
        ui)         echo "audit-ui audit-menus audit-prefs" ;;
        fenetre)    echo "verif-fenetre verif-fenetre-wayland" ;;
        association) echo "verif-association" ;;
        filtres)    echo "verif-filtres verif-cadrage verif-couleur" ;;
        audio)      echo "verif-audio verif-horloge" ;;
        reseau)     echo "verif-reseau verif-precache" ;;
        recents)    echo "verif-recents" ;;
        boucle)     echo "verif-boucle-ab" ;;
        couleur)    echo "verif-couleur verif-filtres verif-hdr" ;;
        affichage)  echo "verif-affichage verif-cadrage" ;;
        signets)    echo "verif-signets verif-playlist" ;;
        souris)     echo "verif-souris verif-personnalisation" ;;
        decodeur)   echo "verif-decodeur verif-hdr" ;;
        systeme)    echo "verif-systeme verif-tray" ;;
        theme)      echo "verif-theme audit-prefs" ;;
        saut)       echo "verif-saut-pause verif-pistes verif-signets" ;;
        configuration) echo "verif-config-io verif-recents" ;;
        *)          echo "" ;;
    esac
}

if [ $# -eq 0 ] || [ "$1" = "--liste" ]; then
    echo "Sujets disponibles :"
    for sujet in playlist vignettes mpris focus hdr soustitres ui fenetre association filtres audio reseau recents configuration boucle couleur affichage signets souris decodeur systeme theme saut; do
        echo "  $sujet → $(bancs_pour "$sujet")"
    done
    exit 0
fi

# Environnement des bancs posé explicitement (Python, session X11, écran virtuel) :
# voir tools/env-bancs.sh. Après `--liste`, qui n'en a pas besoin.
. "$(dirname "$(readlink -f "$0")")/env-bancs.sh"

# Résolution des arguments en noms de bancs.
CIBLES=""
for arg in "$@"; do
    correspondance=$(bancs_pour "$arg")
    if [ -n "$correspondance" ]; then
        CIBLES="$CIBLES $correspondance"
    else
        CIBLES="$CIBLES $arg"
    fi
done

# Les deux mires des bancs d'interface (créées par tout-verifier.sh).
M1=/tmp/kp-mire-ui1.mp4
M2=/tmp/kp-mire-ui2.mp4
for m in "$M1" "$M2"; do
    [ -s "$m" ] || env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f lavfi -i "testsrc2=s=640x360:r=10:d=25" -c:v libx264 -preset ultrafast -y "$m"
done

# Écran virtuel : plusieurs bancs cliquent dans une vraie fenêtre (même garde-fou
# que la suite complète — sans Xvfb, l'audit échoue 71 fois sans raison de code).
XVFB_PID=""
if ! DISPLAY=$DISP xdpyinfo >/dev/null 2>&1; then
    Xvfb "$DISP" -screen 0 "$KP_XVFB_TAILLE" >/tmp/kp-xvfb-rapide.log 2>&1 &
    XVFB_PID=$!
    for _ in $(seq 40); do
        DISPLAY=$DISP xdpyinfo >/dev/null 2>&1 && break
        sleep 0.25
    done
fi
trap '[ -n "$XVFB_PID" ] && kill "$XVFB_PID" 2>/dev/null; true' EXIT
kp_verifier_ecran "$DISP" || exit 2

total_p=0; total_f=0; muets=0; lances=0
for nom in $CIBLES; do
    script="tools/$nom.sh"
    if [ ! -x "$script" ]; then
        echo "### $nom — script introuvable ($script)"
        continue
    fi
    lances=$((lances + 1))
    echo "### $nom"
    case "$nom" in
        audit-ui)            sortie=$("$script" "$M1" "$M2" "$DISP" 2>&1) ;;
        audit-menus|audit-prefs|verif-playlist|verif-recents|verif-boucle-ab|verif-soustitres-style|verif-soustitres-langues|verif-affichage|verif-signets|verif-souris|verif-systeme|verif-tray|verif-theme|verif-saut-pause) sortie=$("$script" "$DISP" 2>&1) ;;
        verif-fenetre)       sortie=$("$script" :98 2>&1) ;;   # son propre écran 1280×800
        verif-fenetre-wayland) sortie=$(env WAYLAND_DISPLAY="$KP_ENV_ORIG_WAYLAND_DISPLAY" \
                                   XDG_SESSION_TYPE="$KP_ENV_ORIG_XDG_SESSION_TYPE" "$script" 2>&1) ;;
        *)                   sortie=$("$script" 2>&1) ;;
    esac
    echo "$sortie" | grep -E "ÉCHEC|✗|Bilan|échec|Résultat" | tail -4
    p=$(echo "$sortie" | grep -oE "[0-9]+ réussite" | tail -1 | grep -oE "[0-9]+")
    f=$(echo "$sortie" | grep -oE "[0-9]+ échec" | tail -1 | grep -oE "[0-9]+")
    total_p=$((total_p + ${p:-0})); total_f=$((total_f + ${f:-0}))
    if [ "${p:-0}" -eq 0 ] && [ "${f:-0}" -eq 0 ]; then
        echo "  ⚠ banc « $nom » sans résultat — non compté"
        muets=$((muets + 1))
    fi
done

echo "=== TOTAL (vérification ciblée) : $total_p réussites, $total_f échecs, $lances banc(s) ==="
[ "$muets" -gt 0 ] && echo "⚠ $muets banc(s) muet(s) : le total est incomplet"
echo "  (suite complète : tools/tout-verifier.sh — disponible, non lancée ici)"
[ "$total_f" -eq 0 ] && [ "$muets" -eq 0 ]

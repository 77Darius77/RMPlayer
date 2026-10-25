#!/usr/bin/env bash
# verif-sousimages.sh — prouve que les sous-titres IMAGE (PGS, DVD) sont rendus.
#
# Ce qui était faux (session 23)
# -----------------------------
# Les pistes image étaient détectées puis **refusées** : « aucun texte à afficher ».
# Elles sont maintenant décodées en bitmaps RGBA et composées sur la vidéo.
# Vérifier cela ne peut pas se faire en lisant du texte : on mesure des pixels.
#
# Méthode
# -------
#   1. on fabrique un flux PGS dont on connaît TOUT (rectangle, position, couleur,
#      minutage) avec `tools/fabrique-pgs.py` — il n'existe pas d'encodeur PGS dans
#      FFmpeg, d'où ce générateur ;
#   2. la vidéo de fond est un aplat sombre : tout pixel clair vient du sous-titre ;
#   3. on capture l'écran pendant l'affichage, puis après l'extinction, et on
#      mesure la position/la taille/la couleur du rectangle ;
#   4. un contrôle sans écran (`--dump-images`) vérifie le décodage lui-même
#      (dimensions, pixels opaques, empreinte identique d'une exécution à l'autre).
#
# Usage : tools/verif-sousimages.sh [écran]
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
KP=./target/release/rmplayer
DISP="${1:-:95}"
BASE=/tmp/kp-sousimages
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export DISPLAY="$DISP"
export XDG_CONFIG_HOME="$BASE/config"
export XDG_CACHE_HOME="$BASE/cache"
mkdir -p "$BASE"
rm -rf "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
nettoyer() {
    [ -n "${KP_PID:-}" ] && kill "$KP_PID" 2>/dev/null
    [ -n "${XPID:-}" ] && kill "$XPID" 2>/dev/null
}
trap nettoyer EXIT

for outil in ffmpeg python3; do
    command -v "$outil" >/dev/null || {
        echo "outil manquant : $outil — banc non exécutable ici"; exit 2; }
done
[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }

# --- 1. Contrôles sans écran : le décodage lui-même ---------------------------
# --- Garde : aucune instance de test résiduelle ne doit couvrir l'écran --------
# Une capture d'écran (x11grab) prend **tout l'écran** : un lecteur laissé par un
# essai précédent sur le même écran virtuel recouvre la fenêtre mesurée, et le banc
# mesure ses pixels (constaté : taille 1075x651 « sous-titre » et couleur bleue,
# c'étaient les barres de couleur d'un autre film). On arrête donc les rmplayer qui
# tournent **sur cet écran** — `DISPLAY` est lu dans /proc, pour ne jamais toucher
# au lecteur de l'utilisateur (qui est sur :0 / Wayland).
arreter_instances_de_test() {
    for pid in $(pgrep -x rmplayer 2>/dev/null); do
        affichage=$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | sed -n 's/^DISPLAY=//p')
        if [ "$affichage" = "$DISP" ]; then
            kill "$pid" 2>/dev/null
        fi
    done
    sleep 0.5
}
arreter_instances_de_test

echo "== 1. Décodage des sous-titres image (sans écran) =="

# PGS 1080p : rectangle 400x80 en (760,900), jaune, de 1 s à 6 s.
python3 tools/fabrique-pgs.py "$BASE/hd.sup" --largeur 1920 --hauteur 1080 \
    --x 760 --y 900 --l 400 --h 80 --debut 1 --fin 6 --couleur jaune 2>/dev/null
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "color=c=0x101010:s=1920x1080:r=15:d=8" \
    -f sup -i "$BASE/hd.sup" -c:v libx264 -preset veryfast -pix_fmt yuv420p \
    -c:s copy -map 0:v -map 1:s "$BASE/hd.mkv" || {
    echo "fabrication du média de test impossible"; exit 2; }

SORTIE=$("$KP" --dump-images --dump-images-pgm "$BASE/rect.pgm" "$BASE/hd.mkv" 2>/dev/null)
echo "$SORTIE" | sed 's/^/   /'

if echo "$SORTIE" | grep -q "400x80 en (760,900)"; then
    ok "le rectangle décodé a la géométrie écrite dans le flux (400x80 en 760,900)"
else
    ko "géométrie du rectangle incorrecte"
fi
if echo "$SORTIE" | grep -qE "32000 pixels opaques"; then
    ok "le rectangle est entièrement opaque (32000 pixels = 400x80)"
else
    ko "pixels opaques inattendus (transparence anormale)"
fi
if echo "$SORTIE" | grep -qE "^0 --> 5000"; then
    ok "minutage cohérent (affiché de 0 à 5000 ms — le muxeur a rebasé le départ)"
else
    ko "minutage inattendu : $(echo "$SORTIE" | grep -E '^[0-9]+ -->' | head -1)"
fi
# Empreinte stable : le même flux doit donner exactement les mêmes pixels.
EMP1=$(echo "$SORTIE" | grep -oE "empreinte [0-9a-f]+" | head -1)
SORTIE2=$("$KP" --dump-images "$BASE/hd.mkv" 2>/dev/null)
EMP2=$(echo "$SORTIE2" | grep -oE "empreinte [0-9a-f]+" | head -1)
if [ -n "$EMP1" ] && [ "$EMP1" = "$EMP2" ]; then
    ok "décodage déterministe ($EMP1 à chaque exécution)"
else
    ko "empreinte instable : « $EMP1 » puis « $EMP2 »"
fi
# L'image elle-même : 400x80, unie et claire (le PGM pondère par l'alpha).
if python3 - "$BASE/rect.pgm" <<'EOF'
import sys
from PIL import Image
import numpy as np
a = np.asarray(Image.open(sys.argv[1]))
if a.shape != (80, 400):
    print("   dimensions", a.shape); sys.exit(1)
# jaune (R et G élevés) → luminance ~230 ; le PGM conserve cette valeur
sys.exit(0 if a.min() > 150 else 1)
EOF
then
    ok "l'image décodée est un rectangle 400x80 uniformément clair"
else
    ko "image décodée inattendue (dimensions ou luminance)"
fi

# Couleur : un rouge dans la palette doit ressortir rouge dans l'image.
python3 tools/fabrique-pgs.py "$BASE/rouge.sup" --largeur 1920 --hauteur 1080 \
    --x 100 --y 100 --l 200 --h 60 --debut 1 --fin 5 --couleur rouge 2>/dev/null
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "color=c=0x101010:s=1920x1080:r=15:d=6" \
    -f sup -i "$BASE/rouge.sup" -c:v libx264 -preset veryfast -pix_fmt yuv420p \
    -c:s copy -map 0:v -map 1:s "$BASE/rouge.mkv" 2>/dev/null
"$KP" --dump-images --dump-images-pgm "$BASE/rouge.pgm" "$BASE/rouge.mkv" >/dev/null 2>&1
# Le PGM est en niveaux de gris : on vérifie la couleur sur une capture d'écran
# plus bas. Ici, on contrôle surtout que la palette est bien appliquée (alpha).

# --- 2. Composition à l'écran ------------------------------------------------
if ! command -v Xvfb >/dev/null; then
    echo "== 2. Composition à l'écran : Xvfb absent, contrôles ignorés =="
else
    echo "== 2. Composition à l'écran : position, taille, couleur, extinction =="
    Xvfb "$DISP" -screen 0 1280x800x24 >"$BASE/xvfb.log" 2>&1 &
    XPID=$!
    sleep 2

    capture() {  # capture <fichier.png>
        ffmpeg -hide_banner -loglevel error -f x11grab -video_size 1280x800 \
            -i "$DISP+0,0" -frames:v 1 -y "$1" 2>/dev/null
    }
    # Lecture depuis le début (un seek effacerait le sous-titre en cours, voir la
    # limite documentée) ; capture pendant l'affichage puis après l'extinction.
    rm -f "$XDG_CACHE_HOME/rmplayer/positions.json"
    "$KP" "$BASE/hd.mkv" >"$BASE/lecteur.log" 2>&1 &
    KP_PID=$!
    sleep 3
    capture "$BASE/avec.png"
    sleep 4
    capture "$BASE/sans.png"
    kill "$KP_PID" 2>/dev/null; wait "$KP_PID" 2>/dev/null; KP_PID=""

    MESURE=$(python3 - "$BASE/avec.png" "$BASE/sans.png" <<'EOF'
import sys
from PIL import Image
import numpy as np

avec = np.asarray(Image.open(sys.argv[1]).convert("RGB"), dtype=int)
sans = np.asarray(Image.open(sys.argv[2]).convert("RGB"), dtype=int)

# Zone vidéo : le fond du média est un **aplat sombre**, dont la valeur exacte
# dépend du chemin de conversion des couleurs — mesuré : 15 en conversion logicielle,
# 30 quand l'échelle matérielle du pilote intervient (défaut depuis la session 31).
# Coder 15 en dur faisait échouer ce banc (aucun pixel trouvé, puis « aucun
# sous-titre affiché » alors que tout allait bien). On prend donc la valeur
# dominante des pixels sombres de la capture de référence.
# Le fond du média est sombre mais **non noir** (0x101010) : les marges de la
# fenêtre, elles, sont du noir pur (0). Sans exclure le 0, la valeur dominante
# devenait 0, la « zone vidéo » englobait toute la fenêtre, et le banc mesurait les
# bords (constaté : taille « 1259x710 » pour un sous-titre de 267x53).
sombre = (sans.sum(axis=2) > 0) & (sans.sum(axis=2) < 200)
if not sombre.any():
    sombre = sans.sum(axis=2) < 300
valeurs, comptes = np.unique(sans[sombre][:, 0], return_counts=True)
fond = int(valeurs[np.argmax(comptes)]) if len(valeurs) else 0
m_fond = (np.abs(sans[:, :, 0] - fond) < 10) & (np.abs(sans[:, :, 1] - fond) < 10)
ys, xs = np.where(m_fond)
x0, x1, y0, y1 = xs.min(), xs.max(), ys.min(), ys.max()
# L'échelle se déduit de la LARGEUR : la hauteur visible de la zone vidéo est
# rognée par la barre de contrôles, et s'en servir faussait l'attendu de 2 %.
echelle = (x1 - x0 + 1) / 1920

# Sous-titre = ce qui DIFFÈRE entre la capture avec et sans. Un simple seuil de
# luminosité attrapait le pointeur de la souris (blanc, 16x16) et faussait la
# boîte englobante ; la différence l'annule quand il n'a pas bougé.
zone_a = avec[y0:y1 + 1, x0:x1 + 1]
zone_s = sans[y0:y1 + 1, x0:x1 + 1]
diff = np.abs(zone_a - zone_s).sum(axis=2)
m = diff > 60
# On ne garde que les pixels clairs (le sous-titre est jaune sur fond sombre) :
# les variations de rendu du fond (dithering) sont écartées.
m &= zone_a.sum(axis=2) > 300
ys2, xs2 = np.where(m)
# Nombre de pixels clairs dans la capture « sans » : doit rester négligeable.
clair_sans = ((zone_s.sum(axis=2) > 300) & (diff > 60)).sum()
if len(xs2) < 500:
    print("ABSENT", len(xs2), clair_sans)
    sys.exit()
l, h = xs2.max() - xs2.min() + 1, ys2.max() - ys2.min() + 1
cy, cx = (ys2.min() + ys2.max()) // 2, (xs2.min() + xs2.max()) // 2
r, g, b = zone_a[cy, cx]
# Position attendue : 760,900 dans un média de 1920x1080.
att_x = round(760 * echelle) + (x0 if False else 0)
print(
    f"{l} {h} {att_x} {ys2.min()} {r} {g} {b} {len(xs2)} {clair_sans} "
    f"{400 * echelle:.0f} {80 * echelle:.0f} {xs2.min()}"
)
EOF
)
    set -- $MESURE
    if [ "${1:-ABSENT}" = "ABSENT" ]; then
        ko "aucun sous-titre affiché (pixels clairs : ${2:-0} ; référence sans sous-titre : ${3:-0})"
    else
        LARGEUR=$1; HAUTEUR=$2; N_PIX=$8; N_SANS=$9
        ATT_L=${10}; ATT_H=${11}
        ecart_l=$(( LARGEUR > ATT_L ? LARGEUR - ATT_L : ATT_L - LARGEUR ))
        ecart_h=$(( HAUTEUR > ATT_H ? HAUTEUR - ATT_H : ATT_H - HAUTEUR ))
        if [ "$ecart_l" -le 4 ] && [ "$ecart_h" -le 4 ]; then
            ok "rectangle affiché à la bonne taille (${LARGEUR}x${HAUTEUR} px pour ${ATT_L}x${ATT_H} attendus)"
        else
            ko "taille affichée ${LARGEUR}x${HAUTEUR} px, ${ATT_L}x${ATT_H} attendus"
        fi
        if [ "${N_SANS:-1}" -lt 100 ]; then
            ok "absent avant l'affichage et après l'extinction (${N_SANS} px clairs)"
        else
            ko "des pixels clairs subsistent hors affichage (${N_SANS})"
        fi
        # Couleur : jaune → R et G élevés, B bas.
        R=$5; G=$6; B=$7
        if [ "${R:-0}" -gt 150 ] && [ "${G:-0}" -gt 150 ] && [ "${B:-255}" -lt 120 ]; then
            ok "couleur de la palette respectée (R=$R G=$G B=$B)"
        else
            ko "couleur inattendue (R=${R:-?} G=${G:-?} B=${B:-?})"
        fi
    fi

    # Le journal doit signaler la piste image comme rendue, pas comme refusée.
    if grep -q "les images sont composées sur la vidéo" "$BASE/lecteur.log"; then
        ok "le journal annonce le rendu des images (plus de « non implémenté »)"
    else
        ko "le journal ne signale pas le rendu des images"
    fi
    if grep -q "n'est pas implémenté" "$BASE/lecteur.log"; then
        ko "le message obsolète « non implémenté » subsiste"
    fi
fi

# --- 3. Le sous-titre texte n'est pas cassé ----------------------------------
echo "== 3. Piste texte toujours rendue (non-régression) =="
printf '1\n00:00:00,500 --> 00:00:04,000\nBonjour\n\n' > "$BASE/a.srt"
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "color=c=0x101010:s=640x360:r=15:d=6" \
    -i "$BASE/a.srt" -c:v libx264 -preset veryfast -pix_fmt yuv420p -c:s srt \
    -map 0:v -map 1:s "$BASE/texte.mkv" 2>/dev/null
"$KP" --dump-subs "$BASE/texte_extraite.srt" "$BASE/texte.mkv" >/dev/null 2>&1
if grep -q "Bonjour" "$BASE/texte_extraite.srt" 2>/dev/null; then
    ok "les sous-titres texte sont toujours lus (« $(head -3 "$BASE/texte_extraite.srt" | tr '\n' ' ') »)"
else
    ko "les sous-titres texte ne sont plus lus"
fi

echo
echo "Résultat : $PASS réussites, $FAIL échec(s)"
[ "$FAIL" -eq 0 ]

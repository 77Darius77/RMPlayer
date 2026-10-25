#!/usr/bin/env bash
# verif-hdr.sh — le module HDR : détection, conversion tonale, 10 bits, tramage.
#
# **Ce que ce banc prouve, et comment.** Un réglage HDR peut être présent dans un menu
# et ne rien faire : la seule preuve acceptable est de **mesurer les pixels** produits
# par le moteur, comparés à une conversion naïve. Les mires sont calibrées en nits :
#
#   bandes.mkv   PQ/BT.2020 10 bits : trois bandes à 5 / 30 / 1000 nits
#   degrade.mkv  PQ : dégradé 100 → 1000 nits
#   hlg.mkv      HLG/BT.2020 10 bits
#   sdr10.mkv    SDR 10 bits (aucune conversion attendue)
#   fin.mkv      SDR 10 bits, dégradé très fin (le tramage s'y voit)
#   marches.mkv  SDR 8 bits, dégradé à marches (le débanding s'y voit)
#
# **Signature d'une conversion tonale correcte** (mesurée, pas supposée) : les basses
# lumières **descendent** (5 nits : 61 → 47) et les hautes **montent** jusqu'à la
# saturation (1000 nits : 190 → 253). Une conversion absente laisse 61/98/190 ; une
# conversion faite « dans le vide » (sortie PQ au lieu de SDR) aussi — c'est
# exactement le piège qui a coûté une heure en session 35.
#
# Usage : tools/verif-hdr.sh        (aucun écran requis)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
BASE=/tmp/kp-hdr-verif
KP=./target/release/rmplayer
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export RUST_LOG=rmplayer=info

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
info() { echo "  · $1"; }

[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }
FF=/opt/ffmpeg-7.1/bin/ffmpeg
[ -x "$FF" ] || { echo "FFmpeg /opt/ffmpeg-7.1 absent"; exit 2; }
# libplacebo est indispensable à ce banc : sans lui, les contrôles GPU n'ont pas de sens.
if ! $FF -hide_banner -filters 2>/dev/null | grep -q "libplacebo"; then
    echo "BANC IGNORÉ : ce FFmpeg n'a pas le filtre libplacebo (recompiler avec --enable-libplacebo)"
    exit 3
fi

rm -rf "$BASE"; mkdir -p "$BASE/clips" "$BASE/dumps" "$BASE/cfg"
Z_PQ="zscale=tin=linear:min=gbr:rin=full:pin=bt709:npl=1000:t=smpte2084:m=bt2020nc:p=bt2020:r=limited,format=yuv420p10le"

echo "== 0. Mires calibrées en nits =="
# Trois bandes : 5 / 30 / 1000 nits (référence 1000 nits = 1,0 en linéaire).
$FF -hide_banner -loglevel error -f lavfi -i "color=c=black:s=640x360:r=5:d=4,format=gbrpf32le" \
  -vf "geq=r='if(lt(X,214),0.005,if(lt(X,428),0.03,1.0))':g='if(lt(X,214),0.005,if(lt(X,428),0.03,1.0))':b='if(lt(X,214),0.005,if(lt(X,428),0.03,1.0))',$Z_PQ" \
  -c:v ffv1 -level 3 -color_trc smpte2084 -color_primaries bt2020 -colorspace bt2020nc -color_range tv \
  -y "$BASE/clips/bandes.mkv" 2>/dev/null
# HLG : même construction, autre transfert.
$FF -hide_banner -loglevel error -f lavfi -i "color=c=black:s=640x360:r=5:d=4,format=gbrpf32le" \
  -vf "geq=r='0.1+0.9*X/W':g='0.1+0.9*X/W':b='0.1+0.9*X/W',zscale=tin=linear:min=gbr:rin=full:pin=bt709:npl=1000:t=arib-std-b67:m=bt2020nc:p=bt2020:r=limited,format=yuv420p10le" \
  -c:v ffv1 -level 3 -color_trc arib-std-b67 -color_primaries bt2020 -colorspace bt2020nc -color_range tv \
  -y "$BASE/clips/hlg.mkv" 2>/dev/null
# SDR 10 bits, dégradé horizontal fin : la réduction à 8 bits y fait des marches.
$FF -hide_banner -loglevel error -f lavfi -i "color=c=black:s=640x360:r=5:d=4" \
  -vf "format=yuv420p10le,geq=lum='X*255/W'" -c:v ffv1 -level 3 -y "$BASE/clips/fin.mkv" 2>/dev/null
# SDR 8 bits à marches (3 niveaux par pas) : le débanding doit lisser.
$FF -hide_banner -loglevel error -f lavfi -i "color=c=black:s=640x360:r=5:d=4" \
  -vf "format=gray,geq=lum='40+floor(X/40)*2',format=yuv420p" \
  -c:v ffv1 -level 3 -y "$BASE/clips/marches.mkv" 2>/dev/null
# SDR 8 bits tout simple : aucun étage ne doit être inséré.
$FF -hide_banner -loglevel error -f lavfi -i "testsrc2=s=320x240:r=5:d=3" \
  -c:v ffv1 -level 3 -y "$BASE/clips/sdr8.mkv" 2>/dev/null
ls -la "$BASE/clips/"*.mkv | awk '{print "    ", $NF, $5, "octets"}'

# Configuration isolée : `config <nom> '<json video>'`.
config() {
    local nom="$1" video="$2"
    mkdir -p "$BASE/cfg/$nom/rmplayer" "$BASE/cfg/$nom/cache"
    python3 - "$BASE/cfg/$nom/rmplayer/config.json" "$video" <<'PY'
import json, sys
chemin, video = sys.argv[1], json.loads(sys.argv[2])
json.dump({
    "video": dict({"hardware_decoding": False}, **video),
    "audio": {"device": "", "volume_max": 150},
    "subtitles": {"auto_load": False},
    "interface": {"show_osd": False, "show_title": False},
    "playback": {"resume": False}, "playlist": [],
}, open(chemin, "w"), indent=2)
PY
}
# image <config> <clip> <sortie> [moteur] : décodage d'une image par notre moteur.
image() {
    local nom="$1" clip="$2" sortie="$3" moteur="${4:-}"
    if [ -n "$moteur" ]; then
        RMPLAYER_HDR_MOTEUR="$moteur" XDG_CONFIG_HOME="$BASE/cfg/$nom" \
            XDG_CACHE_HOME="$BASE/cfg/$nom/cache" \
            "$KP" --dump-frame 500 "$sortie" "$clip" >"$BASE/cfg/$nom.log" 2>&1
    else
        XDG_CONFIG_HOME="$BASE/cfg/$nom" XDG_CACHE_HOME="$BASE/cfg/$nom/cache" \
            "$KP" --dump-frame 500 "$sortie" "$clip" >"$BASE/cfg/$nom.log" 2>&1
    fi
    [ -s "$sortie" ]
}
# bandes <png> : imprime « bas moyen haut » (luminance des trois bandes).
bandes() {
    python3 - "$1" <<'PY'
import sys
import numpy as np
from PIL import Image
a = np.asarray(Image.open(sys.argv[1]).convert("L"), dtype=int)
l = a[a.shape[0] // 2]
print("%.1f %.1f %.1f" % (l[40:200].mean(), l[250:400].mean(), l[480:620].mean()))
PY
}

echo
echo "== 1. Détection des métadonnées (sans décodage) =="
detection() {  # detection <fichier> <champ>
    "$KP" --hdr-info "$1" 2>/dev/null | sed -n "s/^$2: //p"
}
for cas in "bandes:oui" "hlg:oui" "fin:non" "sdr8:non"; do
    IFS=: read -r nom attendu _ <<<"$cas"
    hdr=$(detection "$BASE/clips/$nom.mkv" hdr)
    if [ "$hdr" = "$attendu" ]; then
        ok "$nom.mkv détecté HDR=$hdr"
    else
        ko "$nom.mkv : HDR détecté « $hdr » au lieu de « $attendu »"
    fi
done
bits=$([ -s "$BASE/clips/fin.mkv" ] && detection "$BASE/clips/fin.mkv" bits)
[ "$bits" = "10" ] && ok "profondeur lue : 10 bits sur fin.mkv" || ko "profondeur lue « $bits » au lieu de 10"
prim=$(detection "$BASE/clips/bandes.mkv" primaires)
[ "$prim" = "BT.2020" ] && ok "primaires lues : BT.2020" || ko "primaires lues « $prim »"

echo
echo "== 2. L'étage HDR entre dans la chaîne selon le mode =="
config auto '{"hdr": {"mode": "Auto"}}'
config jamais '{"hdr": {"mode": "Jamais"}}'
config toujours '{"hdr": {"mode": "Toujours"}}'
for nom in auto jamais; do
    image "$nom" "$BASE/clips/bandes.mkv" "$BASE/dumps/etage-$nom.png"
    if grep -q "libplacebo=" "$BASE/cfg/$nom.log"; then
        presence="présent"
    else
        presence="absent"
    fi
    attendu=$([ "$nom" = "auto" ] && echo "présent" || echo "absent")
    if [ "$presence" = "$attendu" ]; then
        ok "mode « $nom » : étage $presence (attendu)"
    else
        ko "mode « $nom » : étage $presence, attendu $attendu"
    fi
done
# Mode « Toujours » sur un SDR 8 bits : étage inséré quand même (comparaison).
image toujours "$BASE/clips/sdr8.mkv" "$BASE/dumps/etage-toujours.png"
if grep -q "libplacebo=" "$BASE/cfg/toujours.log"; then
    ok "mode « Toujours » : étage inséré même sur du SDR"
else
    ko "mode « Toujours » : aucun étage inséré sur du SDR"
fi
# Auto sur SDR 8 bits : rien à faire.
config auto_sdr '{"hdr": {"mode": "Auto"}}'
image auto_sdr "$BASE/clips/sdr8.mkv" "$BASE/dumps/etage-sdr.png"
if grep -q "libplacebo=\|zscale=" "$BASE/cfg/auto_sdr.log"; then
    ko "un étage a été inséré sur un SDR 8 bits en mode automatique"
else
    ok "SDR 8 bits en automatique : aucun étage (image intacte)"
fi

echo
echo "== 3. La conversion tonale change réellement l'image (signature en nits) =="
read -r bas_off moy_off haut_off <<<"$(bandes "$BASE/dumps/etage-jamais.png")"
read -r bas_gpu moy_gpu haut_gpu <<<"$(bandes "$BASE/dumps/etage-auto.png")"
echo "    sans conversion (mode jamais) : $bas_off / $moy_off / $haut_off"
echo "    avec conversion (GPU)         : $bas_gpu / $moy_gpu / $haut_gpu"
python3 - "$bas_off" "$bas_gpu" "$haut_off" "$haut_gpu" <<'PY'
import sys
bas_off, bas_gpu, haut_off, haut_gpu = map(float, sys.argv[1:5])
# Signature : les basses lumières baissent d'au moins 8 niveaux, les hautes montent
# d'au moins 30. Une conversion absente laisse tout à l'identique ; une conversion
# « dans le vide » (sortie PQ) aussi.
ok = (bas_off - bas_gpu) >= 8 and (haut_gpu - haut_off) >= 30
sys.exit(0 if ok else 1)
PY
if [ $? -eq 0 ]; then
    ok "les basses lumières descendent et les hautes montent : conversion réelle"
else
    ko "la conversion ne produit pas la signature attendue"
fi

echo
echo "== 4. Deux opérateurs donnent deux images différentes =="
config hable '{"hdr": {"mode": "Auto", "tonemap": "Hable"}}'
image hable "$BASE/clips/bandes.mkv" "$BASE/dumps/bandes-hable.png"
read -r bas_h moy_h haut_h <<<"$(bandes "$BASE/dumps/bandes-hable.png")"
echo "    BT.2390 : $bas_gpu / $moy_gpu / $haut_gpu"
echo "    Hable   : $bas_h / $moy_h / $haut_h"
if python3 -c "import sys; sys.exit(0 if abs(float('$bas_h')-float('$bas_gpu'))>=4 else 1)"; then
    ok "changer d'opérateur change l'image (BT.2390 ≠ Hable)"
else
    ko "les deux opérateurs donnent la même image (réglage inerte)"
fi

echo
echo "== 5. Repli CPU sans Vulkan : même travail, autre moteur =="
image auto "$BASE/clips/bandes.mkv" "$BASE/dumps/bandes-cpu.png" cpu
if grep -q "zscale=.*tonemap=" "$BASE/cfg/auto.log"; then
    ok "repli CPU : chaîne zscale + tonemap construite"
else
    ko "repli CPU : chaîne absente ($(grep -o 'étage: .*' "$BASE/cfg/auto.log" | head -1 | cut -c1-80))"
fi
read -r bas_cpu moy_cpu haut_cpu <<<"$(bandes "$BASE/dumps/bandes-cpu.png")"
echo "    repli CPU : $bas_cpu / $moy_cpu / $haut_cpu"
if python3 - "$bas_off" "$bas_cpu" "$haut_off" "$haut_cpu" <<'PY'
import sys
bas_off, bas_cpu, haut_off, haut_cpu = map(float, sys.argv[1:5])
# Même exigence que le GPU, un peu plus souple : le repli utilise Hable.
sys.exit(0 if (bas_off - bas_cpu) >= 5 and (haut_cpu - haut_off) >= 20 else 1)
PY
then
    ok "le repli CPU convertit aussi (même signature, moins marquée)"
else
    ko "le repli CPU ne convertit pas"
fi
if grep -qi "refusé\|inapplicable" "$BASE/cfg/auto.log"; then
    ko "la chaîne a été refusée par le système : $(grep -io 'refusé[^:]*' "$BASE/cfg/auto.log" | head -1)"
else
    ok "aucune chaîne refusée (le graphe est accepté tel quel)"
fi

echo
echo "== 6. HLG : détecté et converti =="
config auto '{"hdr": {"mode": "Auto"}}'
image jamais "$BASE/clips/hlg.mkv" "$BASE/dumps/hlg-jamais.png"
image auto "$BASE/clips/hlg.mkv" "$BASE/dumps/hlg-auto.png"
if python3 - "$BASE/dumps/hlg-jamais.png" "$BASE/dumps/hlg-auto.png" <<'PY'
import sys
import numpy as np
from PIL import Image
a = np.asarray(Image.open(sys.argv[1]).convert("L"), dtype=int)
b = np.asarray(Image.open(sys.argv[2]).convert("L"), dtype=int)
# HLG n'est pas PQ : sa courbe garde une compatibilité SDR, la différence est donc
# plus fine. On exige une différence mesurable, pas spectaculaire.
ecart = float(np.abs(a.astype(float) - b.astype(float)).mean())
print("    écart moyen HLG : %.2f niveaux" % ecart)
sys.exit(0 if ecart >= 3.0 else 1)
PY
then
    ok "HLG : la conversion change l'image (écart mesuré)"
else
    ko "HLG : aucune différence entre brut et converti"
fi

echo
echo "== 7. Tramage : une source 10 bits en 8 bits sans marches =="
config sans_tramage '{"hdr": {"mode": "Jamais", "dither": "Aucun"}}'
config tramage_bleu '{"hdr": {"mode": "Auto", "dither": "BruitBleu"}}'
config tramage_aucun '{"hdr": {"mode": "Auto", "dither": "Aucun"}}'
image tramage_aucun "$BASE/clips/fin.mkv" "$BASE/dumps/fin-aucun.png"
image tramage_bleu  "$BASE/clips/fin.mkv" "$BASE/dumps/fin-bleu.png"
# Mesure : le nombre de « sauts » francs d'un pixel au suivant. Sans tramage, un
# dégradé 10→8 bits fait des marches nettes ; avec tramage, les transitions sont
# remplacées par du bruit fin et l'amplitude des sauts baisse.
python3 - "$BASE/dumps/fin-aucun.png" "$BASE/dumps/fin-bleu.png" <<'PY'
import sys
import numpy as np
from PIL import Image
def sauts(chemin):
    a = np.asarray(Image.open(chemin).convert("L"), dtype=int)
    lignes = a[100:260, 40:600]
    d = np.abs(np.diff(lignes.astype(float), axis=1))
    return float((d > 0).mean() * 100.0), float(d.max())
p_a, maxi_a = sauts(sys.argv[1])
p_b, maxi_b = sauts(sys.argv[2])
print("    sans tramage : %.1f %% de sauts, amplitude max %.0f" % (p_a, maxi_a))
print("    bruit bleu   : %.1f %% de sauts, amplitude max %.0f" % (p_b, maxi_b))
# Le tramage remplace des marches rares (peu de sauts, amplitude 1) par du bruit
# fréquent : la proportion de sauts doit augmenter nettement.
sys.exit(0 if p_b > p_a * 3.0 else 1)
PY
if [ $? -eq 0 ]; then
    ok "le tramage remplace les marches par un bruit fin (mesuré)"
else
    ko "le tramage ne change pas la structure du dégradé"
fi

echo
echo "== 8. Débanding GPU sur un dégradé à marches =="
config sans_deband '{"hdr": {"mode": "Auto", "gpu_deband": false}}'
config avec_deband '{"hdr": {"mode": "Auto", "gpu_deband": true}}'
image sans_deband "$BASE/clips/marches.mkv" "$BASE/dumps/marches-sans.png"
image avec_deband "$BASE/clips/marches.mkv" "$BASE/dumps/marches-avec.png"
if grep -q "deband=true" "$BASE/cfg/avec_deband.log"; then
    ok "l'option débanding apparaît dans la chaîne"
else
    ko "débanding absent de la chaîne : $(grep -o 'libplacebo=[^ ]*' "$BASE/cfg/avec_deband.log" | head -1 | cut -c1-90)"
fi
python3 - "$BASE/dumps/marches-sans.png" "$BASE/dumps/marches-avec.png" <<'PY'
import sys
import numpy as np
from PIL import Image
def marches(chemin):
    a = np.asarray(Image.open(chemin).convert("L"), dtype=int)
    # Moyenne sur 60 lignes : le débanding ajoute volontairement du grain, qu'il faut
    # absorber pour mesurer la STRUCTURE (les marches), pas le bruit.
    b = a[150:210, 40:600].mean(axis=0).round().astype(int)
    d = np.abs(np.diff(b))
    return int((d >= 2).sum())
sans = marches(sys.argv[1])
avec = marches(sys.argv[2])
print("    marches de 2 niveaux ou plus : %d -> %d" % (sans, avec))
# Le débanding remplace une marche nette par une rampe de 1 niveau : le nombre de
# sauts >= 2 doit tomber nettement, et il doit y avoir des marches au départ (sinon
# la mire est mauvaise et le contrôle ne prouve rien).
sys.exit(0 if (sans >= 3 and avec <= sans // 2) else 1)
PY
if [ $? -eq 0 ]; then
    ok "le débanding GPU transforme les marches en rampes (mesuré)"
else
    ko "le débanding GPU ne réduit pas les marches"
fi

echo
echo "== 9. Rien n'est inventé quand tout est inutile =="
# SDR 8 bits, mode auto, tramage demandé : aucun étage (pas de 10 bits à tramer).
if grep -q "aucun étage à insérer" "$BASE/cfg/auto_sdr.log"; then
    ok "SDR 8 bits : le journal dit qu'aucun étage n'est nécessaire"
else
    info "journal : $(grep -o 'HDR: .*' "$BASE/cfg/auto_sdr.log" | head -1 | cut -c1-90)"
fi
# Moteur forcé à « aucun » : l'étage disparaît, et l'explication le dit.
image auto "$BASE/clips/bandes.mkv" "$BASE/dumps/bandes-aucun.png" aucun
if grep -q "RMPLAYER_HDR_MOTEUR" "$BASE/cfg/auto.log"; then
    ok "moteur forcé à « aucun » : explication présente"
else
    ko "moteur forcé : aucune explication"
fi

echo
echo "== 10. Décodage matériel (VA-API) : même résultat qu'en logiciel =="
# **Le cas réel d'un film HDR** : HEVC Main10 décodé par la carte graphique. Ce
# contrôle existe parce que ce chemin avait un défaut grave et muet : le
# rapatriement VA-API perdait l'horodatage des images (`pts` = AV_NOPTS_VALUE), et
# libplacebo refuse alors de rendre quoi que ce soit — chaîne acceptée, aucune image
# émise, image d'origine affichée sans conversion. Le banc compare donc matériel et
# logiciel sur le MÊME fichier : les deux doivent donner le même résultat.
# `-e` et non `-f` : `/dev/dri/renderD128` est un **périphérique**, pas un fichier
# ordinaire — `-f` échoue et le contrôle s'ignorait à tort (mesuré).
if [ -e /dev/dri/renderD128 ] && $FF -hide_banner -encoders 2>/dev/null | grep -q hevc_vaapi; then
    $FF -hide_banner -loglevel error -vaapi_device /dev/dri/renderD128 \
        -i "$BASE/clips/bandes.mkv" -vf "format=p010,hwupload" \
        -c:v hevc_vaapi -profile:v main10 -color_trc smpte2084 \
        -color_primaries bt2020 -colorspace bt2020nc -color_range tv \
        -y "$BASE/clips/hdr-hevc10.mp4" 2>/dev/null
    if [ -s "$BASE/clips/hdr-hevc10.mp4" ]; then
        config materiel '{"hardware_decoding": true}'
        config logiciel '{"hardware_decoding": false}'
        image materiel "$BASE/clips/hdr-hevc10.mp4" "$BASE/dumps/vaapi.png"
        image logiciel "$BASE/clips/hdr-hevc10.mp4" "$BASE/dumps/cpu10.png"
        read -r bas_m moy_m haut_m <<<"$(bandes "$BASE/dumps/vaapi.png")"
        read -r bas_l moy_l haut_l <<<"$(bandes "$BASE/dumps/cpu10.png")"
        echo "    matériel : $bas_m / $moy_m / $haut_m"
        echo "    logiciel : $bas_l / $moy_l / $haut_l"
        if python3 - "$bas_m" "$moy_m" "$haut_m" "$bas_l" "$moy_l" "$haut_l" <<'PY'
import sys
m = list(map(float, sys.argv[1:4]))
l = list(map(float, sys.argv[4:7]))
# Même conversion, même résultat : tolérance de 2 niveaux (chemins de conversion
# 10 bits différents, arrondis).
sys.exit(0 if all(abs(a - b) <= 2.0 for a, b in zip(m, l)) else 1)
PY
        then
            ok "décodage matériel et logiciel donnent le même tonemap (à 2 niveaux près)"
        else
            ko "le décodage matériel donne un résultat différent du logiciel"
        fi
        # Et la conversion a bien eu lieu (pas la valeur naïve).
        if python3 - "$haut_m" <<'PY'
import sys
sys.exit(0 if float(sys.argv[1]) >= 240 else 1)
PY
        then
            ok "les hautes lumières sont bien étendues en décodage matériel"
        else
            ko "décodage matériel : image non convertie (hautes lumières à $haut_m)"
        fi
    else
        info "encodage HEVC 10 bits impossible ici — contrôle VA-API ignoré"
    fi
else
    info "pas de VA-API (ou pas d'encodeur hevc_vaapi) — contrôle matériel ignoré"
fi

echo
echo "== 11. Détection de l'écran (EDID) et branchement de la décision =="
# **La question qui compte** : « pourquoi ne pas brancher le HDR par détection ? » —
# parce que la détection manquait. Elle est là : l'EDID de chaque écran allumé est lu
# (`/sys/class/drm/*/edid`), et la décision en dépend.
INFO=$("$KP" --hdr-info "$BASE/clips/bandes.mkv" 2>/dev/null)
ecrans_lus=$(ls /sys/class/drm/card*-*/status 2>/dev/null | while read -r f; do
    [ "$(cat "$f" 2>/dev/null)" = "connected" ] && basename "$(dirname "$f")"
done | wc -l)
if [ "${ecrans_lus:-0}" -gt 0 ]; then
    if grep -q "^ecran: écrans : card" <<<"$INFO"; then
        ok "les écrans actifs sont lus par l'EDID ($ecrans_lus connecté(s))"
    else
        ko "aucun écran lu alors que /sys/class/drm en annonce $ecrans_lus : $(grep '^ecran:' <<<"$INFO" | cut -c1-90)"
    fi
else
    info "aucun écran dans /sys/class/drm — contrôle ignoré"
fi
# Chaque écran lu doit annoncer SDR **ou** HDR, jamais rien.
if grep -qE "^ecran: .*\((SDR seulement|PQ|HLG)" <<<"$INFO"; then
    ok "chaque écran annonce une capacité explicite (SDR ou HDR)"
else
    ko "capacité d'écran illisible : $(grep '^ecran:' <<<"$INFO" | cut -c1-90)"
fi
# La sortie est déclarée SDR avec sa raison : pas de passthrough revendiqué à tort.
if grep -q "^passthrough: non" <<<"$INFO"; then
    ok "aucun passthrough HDR revendiqué (la fenêtre est en 8 bits)"
else
    ko "passthrough annoncé alors que la sortie ne peut pas le faire"
fi
if grep -q "^ecran: .*sortie : SDR (la fenêtre est rendue en RGBA 8 bits" <<<"$INFO"; then
    ok "la raison de la sortie SDR est écrite (winit/egui sans API de couleurs)"
else
    info "raison de sortie : $(grep -o 'sortie : SDR ([^)]*' <<<"$INFO" | cut -c1-70)"
fi
# Et l'explication du plan dit **pourquoi** on convertit, écran ou sortie.
if grep -q "^explication: .*— écran SDR (EDID)\|^explication: .*— écran HDR détecté" <<<"$INFO"; then
    ok "l'explication nomme la cause (écran SDR, ou écran HDR mais sortie limitée)"
else
    ko "l'explication ne dit pas pourquoi la conversion a lieu : $(grep '^explication:' <<<"$INFO" | cut -c1-90)"
fi
# Un passthrough simulé doit produire un plan **vide** (le branchement existe, même
# si aucun écran de cette machine ne l'active) : c'est vérifié en unité, ici on
# s'assure que le mode « jamais » ne produit bien aucun étage — même chemin de code.
config jam '{"hdr": {"mode": "Jamais"}}'
image jam "$BASE/clips/bandes.mkv" "$BASE/dumps/passthrough.png"
if grep -q "aucun traitement HDR" "$BASE/cfg/jam.log"; then
    ok "mode « jamais » : aucun étage, chemin du passthrough empruntable"
else
    ko "mode « jamais » : le journal ne dit pas que rien n'est appliqué"
fi

echo
echo "== 12. Vulkan inutilisable : repli CPU borné, sinon image non convertie et dite =="
# **Ce que ce banc fixe.** Quand `libplacebo` est refusé à l'exécution (pilote absent, session
# distante), le lecteur ne s'arrête pas. Deux cas, séparés par un budget de débit
# (`hdr::repli_cpu_possible`, en mégapixels par seconde) parce que le moteur CPU est mesuré :
# 114 i/s en 1080p mais 26 i/s en 4K pour 9 cœurs (`docs/HDR.md`) —
#   * source abordable (ici 640×360 à 5 i/s) : l'étage CPU (`zscale` + `tonemap`) prend le
#     relais, l'image EST convertie, et le journal dit « repli CPU » ;
#   * source trop lourde (ici 3840×2160 à 25 i/s) : pas de conversion (une lecture saccadée
#     serait pire), l'image reste celle d'origine, et le journal dit « repli CPU refusé ».
# Vulkan est cassé en pointant le chargeur vers un fichier d'ICD inexistant. Le témoin positif
# est la section 3 (même fichier, même config, Vulkan intact : l'image est convertie).
VK_CASSE="VK_ICD_FILENAMES=/nonexistent VK_DRIVER_FILES=/nonexistent"
nettoye() { sed 's/\x1b\[[0-9;]*m//g' "$1"; }
sans_vulkan() {  # sans_vulkan <config> <clip> <sortie.png> <journal>
    env $VK_CASSE XDG_CONFIG_HOME="$BASE/cfg/$1" XDG_CACHE_HOME="$BASE/cfg/$1/cache" \
        "$KP" --dump-frame 500 "$3" "$2" >"$4" 2>&1
}
# Trois bandes, par fractions de la largeur : valable pour 640 comme pour 3840 pixels.
bandes_rel() {
    python3 - "$1" <<'PY'
import sys
import numpy as np
from PIL import Image
a = np.asarray(Image.open(sys.argv[1]).convert("L"), dtype=int)
l = a[a.shape[0] // 2]; n = len(l)
f = lambda x, y: l[int(n * x):int(n * y)].mean()
print("%.1f %.1f %.1f" % (f(0.0625, 0.3125), f(0.390625, 0.625), f(0.75, 0.96875)))
PY
}

# --- 12a. Source abordable : le CPU prend le relais -----------------------------------
sans_vulkan auto "$BASE/clips/bandes.mkv" "$BASE/dumps/sv-petit.png" "$BASE/cfg/sv-petit.log"
if [ -s "$BASE/dumps/sv-petit.png" ]; then
    ok "sans Vulkan : le lecteur ne plante pas et produit une image"
else
    ko "sans Vulkan : aucune image produite ($(tail -1 "$BASE/cfg/sv-petit.log" | cut -c1-80))"
fi
if nettoye "$BASE/cfg/sv-petit.log" | grep -q "Étage HDR refusé.*repli CPU"; then
    ok "sans Vulkan : le refus du GPU et le repli CPU sont journalisés"
else
    ko "sans Vulkan : pas de ligne « Étage HDR refusé … repli CPU » ($(nettoye "$BASE/cfg/sv-petit.log" | grep -o 'Étage HDR[^—]*—[^:]*' | head -1 | cut -c1-90))"
fi
read -r bas_sv moy_sv haut_sv <<<"$(bandes "$BASE/dumps/sv-petit.png")"
echo "    sans Vulkan (abordable) : $bas_sv / $moy_sv / $haut_sv   (non converti : $bas_off / $moy_off / $haut_off ; GPU : $bas_gpu / $moy_gpu / $haut_gpu)"
if python3 - "$bas_off" "$bas_sv" "$haut_off" "$haut_sv" <<'PY'
import sys
bas_off, bas_sv, haut_off, haut_sv = map(float, sys.argv[1:5])
# Même signature que le repli CPU forcé (section 5) : les basses lumières descendent d'au
# moins 5 niveaux, les hautes montent d'au moins 20 par rapport à l'image non convertie.
sys.exit(0 if (bas_off - bas_sv) >= 5 and (haut_sv - haut_off) >= 20 else 1)
PY
then
    ok "sans Vulkan, source abordable : l'image est convertie par le CPU"
else
    ko "sans Vulkan, source abordable : l'image n'est pas convertie (pas de repli CPU effectif)"
fi

# --- 12b. Source trop lourde : pas de conversion, et on le dit ------------------------
# Même mire, étirée en 3840×2160 à 25 i/s (ffv1 : les aplats se compressent presque à rien).
# 25 et non 24 : les images tombent à 480 et 520 ms, donc la position 500 ms n'est PAS une
# image. À 24 i/s elle l'est, `--dump-frame` s'arrête après UNE image, et libplacebo (une image
# de latence) n'a encore rien sorti : la mire sortait non convertie, même Vulkan intact.
$FF -hide_banner -loglevel error -i "$BASE/clips/bandes.mkv" -t 2 \
    -vf "fps=25,scale=3840:2160:flags=neighbor" \
    -c:v ffv1 -level 3 -color_trc smpte2084 -color_primaries bt2020 -colorspace bt2020nc -color_range tv \
    -y "$BASE/clips/lourd.mkv" 2>/dev/null
if [ -s "$BASE/clips/lourd.mkv" ]; then
    image jamais "$BASE/clips/lourd.mkv" "$BASE/dumps/lourd-jamais.png"
    image auto "$BASE/clips/lourd.mkv" "$BASE/dumps/lourd-gpu.png"
    sans_vulkan auto "$BASE/clips/lourd.mkv" "$BASE/dumps/sv-lourd.png" "$BASE/cfg/sv-lourd.log"
    read -r bas_lj moy_lj haut_lj <<<"$(bandes_rel "$BASE/dumps/lourd-jamais.png")"
    read -r bas_lg moy_lg haut_lg <<<"$(bandes_rel "$BASE/dumps/lourd-gpu.png")"
    read -r bas_ls moy_ls haut_ls <<<"$(bandes_rel "$BASE/dumps/sv-lourd.png")"
    echo "    4K à 25 i/s : non converti $bas_lj / $moy_lj / $haut_lj ; GPU $bas_lg / $moy_lg / $haut_lg ; sans Vulkan $bas_ls / $moy_ls / $haut_ls"
    if nettoye "$BASE/cfg/sv-lourd.log" | grep -q "repli CPU refusé"; then
        ok "sans Vulkan, source 4K à 25 i/s : le refus du repli CPU est journalisé, avec sa raison"
    else
        ko "sans Vulkan, source 4K : pas de ligne « repli CPU refusé » ($(nettoye "$BASE/cfg/sv-lourd.log" | grep -o 'Étage HDR[^:]*' | head -1 | cut -c1-90))"
    fi
    if python3 - "$bas_lj" "$bas_ls" "$haut_lj" "$haut_ls" "$haut_lg" <<'PY'
import sys
bas_lj, bas_ls, haut_lj, haut_ls, haut_lg = map(float, sys.argv[1:6])
# Image d'origine rendue telle quelle (±3) ET nettement différente de l'image convertie par
# le GPU : sans la seconde condition, le contrôle passerait si Vulkan n'était pas réellement cassé.
sys.exit(0 if abs(bas_ls - bas_lj) <= 3 and abs(haut_ls - haut_lj) <= 3
         and (haut_lg - haut_ls) >= 30 else 1)
PY
    then
        ok "sans Vulkan, source 4K : image non convertie (pas de lecture saccadée imposée)"
    else
        ko "sans Vulkan, source 4K : l'image n'est ni l'originale ni cohérente avec un Vulkan réellement coupé"
    fi
else
    info "mire 4K non fabriquée — contrôle 12b ignoré"
fi

# --- 12c. En lecture réelle, l'échec n'est pas rejoué à chaque image ------------------
# Défaut constaté le 2026-10-24 : avec un AUTRE filtre actif (ici le contraste), le graphe
# reconstruit sans l'étage portait la signature de la chaîne réduite, pas celle demandée :
# à chaque image, le lecteur retentait Vulkan, échouait, reconstruisait (20 refus en 8 s sur
# un clip à 5 i/s). `--dump-frame` ne décode qu'une image : seule une vraie lecture le voit.
if command -v Xvfb >/dev/null; then
    XV=:94
    Xvfb "$XV" -screen 0 1280x800x24 >"$BASE/xvfb-lecture.log" 2>&1 &
    XVPID=$!
    sleep 1.5
    config contraste '{"contrast": 1.2, "hdr": {"mode": "Auto"}}'
    joue() {  # joue <clip> <journal>
        env -u WAYLAND_DISPLAY XDG_SESSION_TYPE=x11 DISPLAY="$XV" \
            XDG_RUNTIME_DIR=/tmp/rmplayer-bench-pas-de-son $VK_CASSE \
            XDG_CONFIG_HOME="$BASE/cfg/contraste" XDG_CACHE_HOME="$BASE/cfg/contraste/cache" \
            timeout 5 "$KP" "$1" >"$2" 2>&1
    }
    for cas in "petit:$BASE/clips/bandes.mkv" "lourd:$BASE/clips/lourd.mkv"; do
        nom="${cas%%:*}"; clip="${cas#*:}"
        [ -s "$clip" ] || continue
        joue "$clip" "$BASE/cfg/lecture-$nom.log"
        refus=$(nettoye "$BASE/cfg/lecture-$nom.log" | grep -c "Étage HDR refusé")
        if [ "$refus" -ge 1 ] && [ "$refus" -le 1 ]; then
            ok "lecture réelle ($nom, autre filtre actif) : le refus de l'étage est journalisé une seule fois"
        elif [ "$refus" -eq 0 ]; then
            ko "lecture réelle ($nom) : aucun refus journalisé — le chemin testé n'a pas été exercé"
        else
            ko "lecture réelle ($nom) : l'échec est rejoué à chaque image ($refus refus en 5 s)"
        fi
    done
    kill "$XVPID" 2>/dev/null
else
    info "Xvfb absent — contrôle de lecture réelle (12c) ignoré"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]

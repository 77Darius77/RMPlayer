#!/usr/bin/env bash
# sante-hdr.sh — le chemin GPU du HDR fonctionne-t-il encore ? (≈ 10 s)
#
# **À lancer après une mise à jour de Mesa, du pilote graphique ou du noyau.** Le HDR passe
# par Vulkan (`libplacebo`) ; une mise à jour qui le casse n'a aucun symptôme évident : le film
# se lit, mais délavé (ou converti par le CPU s'il est petit). Ce script le dit en dix
# secondes, avant que ce soit un film qui le dise.
#
# Ce n'est PAS un banc de la suite (`tout-verifier.sh`) : il ne mesure pas de fonctionnalité,
# il vérifie l'environnement. Pour la preuve complète : `tools/verif-rapide.sh hdr` (~3 min).
#
# Codes de sortie : 0 = tout va bien ; 1 = le GPU ne peut pas servir au HDR (le lecteur
# retombe alors sur le repli CPU borné ou sur une image non convertie, cf. docs/HDR.md) ;
# 2 = outil manquant.
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux || exit 2
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
FF=/opt/ffmpeg-7.1/bin/ffmpeg
KP=./target/release/kplayer
MAUVAIS=0

ok()  { echo "  ✔ $1"; }
ko()  { echo "  ✗ $1"; MAUVAIS=1; }
info() { echo "  · $1"; }

[ -x "$FF" ] || { echo "FFmpeg /opt/ffmpeg-7.1 absent"; exit 2; }

echo "== 1. Vulkan voit-il un vrai GPU ? =="
if command -v vulkaninfo >/dev/null; then
    appareils=$(vulkaninfo --summary 2>/dev/null | sed -n 's/^[[:space:]]*deviceName *= *//p')
    types=$(vulkaninfo --summary 2>/dev/null | sed -n 's/^[[:space:]]*deviceType *= *//p')
    vrai=$(paste -d'|' <(echo "$types") <(echo "$appareils") | grep -v "PHYSICAL_DEVICE_TYPE_CPU" | head -1 | cut -d'|' -f2)
    if [ -n "$vrai" ]; then
        ok "GPU Vulkan : $vrai"
    else
        ko "aucun GPU Vulkan (seulement un Vulkan logiciel : libplacebo le refuse). Vérifier mesa-vulkan-drivers / le pilote, puis relancer"
    fi
else
    info "vulkaninfo absent (paquet vulkan-tools) — contrôle 1 ignoré"
fi

echo "== 2. libplacebo construit-il son graphe sur ce GPU ? =="
SRC="testsrc2=s=1920x1080:r=24:d=1,format=yuv420p10le,setparams=color_trc=smpte2084:colorspace=bt2020nc:color_primaries=bt2020:range=tv"
PLACEBO="libplacebo=color_trc=bt709:color_primaries=bt709:colorspace=bt709:range=tv:format=yuv420p:tonemapping=bt.2390:peak_detect=true:dithering=blue"
sortie=$($FF -hide_banner -loglevel error -f lavfi -i "$SRC,$PLACEBO" -an -f null - 2>&1)
if [ $? -eq 0 ] && ! grep -qiE "Failed|no suitable device" <<<"$sortie"; then
    ok "libplacebo convertit 24 images 1080p PQ → SDR sans erreur"
else
    ko "libplacebo refuse : $(echo "$sortie" | head -2 | tr '\n' ' ' | cut -c1-140)"
fi

echo "== 3. Décodage matériel HEVC 10 bits (VA-API) =="
if command -v vainfo >/dev/null; then
    if vainfo 2>/dev/null | grep -q "VAProfileHEVCMain10.*VAEntrypointVLD"; then
        ok "VA-API décode HEVC Main10 (les films HDR courants)"
    else
        info "VA-API ne déclare pas HEVC Main10 : le décodage sera logiciel (plus lourd, mais le HDR fonctionne)"
    fi
else
    info "vainfo absent — contrôle 3 ignoré"
fi

echo "== 4. Le lecteur choisit-il le moteur GPU ? =="
if [ -x "$KP" ]; then
    mire=/tmp/kp-sante-hdr.mkv
    $FF -hide_banner -loglevel error -f lavfi -i "$SRC" -frames:v 5 -c:v ffv1 -level 3 \
        -color_trc smpte2084 -color_primaries bt2020 -colorspace bt2020nc -color_range tv -y "$mire" 2>/dev/null
    moteur=$($KP --hdr-info "$mire" 2>/dev/null | sed -n 's/^moteur: //p')
    case "$moteur" in
        libplacebo*) ok "moteur retenu : $moteur" ;;
        "")          ko "kplayer --hdr-info ne répond pas" ;;
        *)           info "moteur retenu : $moteur (mode HDR forcé, ou écran HDR : voir docs/HDR.md)" ;;
    esac
    rm -f "$mire"
else
    info "binaire kplayer absent (cargo build --release) — contrôle 4 ignoré"
fi

echo
if [ "$MAUVAIS" -eq 0 ]; then
    echo "HDR : chemin GPU opérationnel."
else
    echo "HDR : le GPU ne peut pas servir. Le lecteur ne s'arrête pas : une source ≤ 1080p30 sera"
    echo "convertie par le CPU, au-delà l'image ne sera pas convertie (docs/HDR.md)."
fi
exit "$MAUVAIS"

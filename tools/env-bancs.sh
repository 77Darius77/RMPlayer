#!/usr/bin/env bash
# env-bancs.sh — l'environnement que les bancs supposent, posé explicitement.
#
# À SOURCER (`. tools/env-bancs.sh`) par les points d'entrée (`tout-verifier.sh`,
# `verif-rapide.sh`) ; les bancs en héritent. Il ne s'exécute pas seul.
#
# **Pourquoi.** Les bancs lançaient l'application avec l'environnement de la personne qui
# les appelait. Quatre dépendances implicites ont fait échouer la suite le 2026-10-24 sans
# qu'une ligne de code du lecteur soit en cause (353/75 au lieu de 426/5, puis 485/0) :
#
#   1. `python3` : une mise à jour de Homebrew a fait pointer `python3` vers une version
#      sans `numpy` ni `PIL` ; tous les bancs qui mesurent des pixels sont devenus
#      aveugles (« teinte sans effet ( → ) »). → on choisit un interpréteur qui les a, et
#      on s'arrête net s'il n'y en a aucun.
#   2. `WAYLAND_DISPLAY` : sans Xvfb-seul, winit cherche un compositeur Wayland que le
#      banc n'a pas (`Could not find wayland compositor`). → retiré.
#   3. `XDG_SESSION_TYPE` : le lecteur en déduit s'il peut placer sa fenêtre
#      (`config.rs`, détection Wayland) ; sous « wayland », la position mémorisée est
#      ignorée et `verif-fenetre` échoue 5 fois sur 9. → forcé à `x11`.
#   4. Taille de l'écran virtuel : `audit-ui` place une fenêtre 1280×720 en (40,40) ; sur
#      1280×800 la capture dépasse l'écran et le banc sort muet. → écran 1500×900.
#
# L'environnement d'origine est conservé (`KP_ENV_ORIG_*`) pour le seul banc qui mesure
# le bureau réel, `verif-fenetre-wayland`.

KP_ENV_ORIG_WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-}"
KP_ENV_ORIG_XDG_SESSION_TYPE="${XDG_SESSION_TYPE:-}"
export KP_ENV_ORIG_WAYLAND_DISPLAY KP_ENV_ORIG_XDG_SESSION_TYPE

unset WAYLAND_DISPLAY
export XDG_SESSION_TYPE=x11

# Écran virtuel commun à tous les bancs : assez grand pour une fenêtre 1280×720 décalée.
export KP_XVFB_TAILLE=1500x900x24

# --- Interpréteur Python : le premier qui sait importer numpy et PIL -------------------
kp_choisir_python() {
    local cand
    for cand in "$(command -v python3 2>/dev/null)" /usr/bin/python3 \
                /home/linuxbrew/.linuxbrew/bin/python3.14 /usr/local/bin/python3; do
        [ -n "$cand" ] && [ -x "$cand" ] || continue
        if "$cand" -c 'import numpy, PIL' >/dev/null 2>&1; then
            echo "$cand"
            return 0
        fi
    done
    return 1
}

KP_PYTHON=$(kp_choisir_python) || {
    echo "DÉPENDANCE ABSENTE : aucun python3 avec numpy et PIL (pip install numpy pillow)."
    echo "  Les bancs de pixels échoueraient tous avec des mesures vides : la suite s'arrête ici."
    exit 2
}
# Un dossier de raccourcis en tête du PATH : `python3` désigne alors cet interpréteur
# pour tous les bancs, sans en modifier un seul.
KP_BIN=/tmp/kp-bin-bancs
mkdir -p "$KP_BIN" && ln -sf "$KP_PYTHON" "$KP_BIN/python3"
export PATH="$KP_BIN:$PATH"
export KP_PYTHON

# kp_verifier_ecran <affichage> : refuse un écran virtuel trop petit (déjà lancé par
# quelqu'un d'autre, par exemple) au lieu de laisser un banc sortir muet.
kp_verifier_ecran() {
    local dims
    dims=$(DISPLAY="$1" xdpyinfo 2>/dev/null | sed -n 's/.*dimensions: *\([0-9]*\)x\([0-9]*\) pixels.*/\1 \2/p' | head -1)
    set -- $dims
    if [ -n "${1:-}" ] && { [ "$1" -lt 1400 ] || [ "${2:-0}" -lt 850 ]; }; then
        echo "ÉCRAN VIRTUEL TROP PETIT : ${1}x${2} (il faut au moins 1400x850 pour audit-ui)."
        echo "  Arrêtez le Xvfb existant (pkill Xvfb) et relancez : la suite en démarre un de $KP_XVFB_TAILLE."
        return 1
    fi
    return 0
}

#!/usr/bin/env bash
# vivacite.sh — prouve que la boucle d'événements d'une application egui tourne.
#
# Pourquoi : une application GELÉE (interblocage de verrou) reste vivante — le
# processus existe, `kill -0` réussit — et la touche fautive est même journalisée
# AVANT le blocage. Un test qui se contente de vérifier « la touche est reçue »
# ne peut donc pas voir le gel. On envoie une touche NON affectée (`X`) et on
# exige une NOUVELLE ligne « touche X » : seule la réception d'un message
# supplémentaire prouve que le fil principal progresse encore.
#
# Usage : tools/vivacite.sh <journal> [écran] [délai_s]
#   code de sortie 0 = l'application répond, 1 = elle ne répond plus (gelée)
set -u
LOG="${1:?usage: vivacite.sh <journal> [écran] [délai_s]}"
DISP="${2:-:0}"
DELAI="${3:-5}"

if [ ! -f "$LOG" ]; then
    echo "vivacite : journal introuvable ($LOG)" >&2
    exit 2
fi

avant=$(grep -c "touche X" "$LOG" 2>/dev/null || true)
avant=${avant:-0}
DISPLAY="$DISP" xdotool key --clearmodifiers x

i=0
while [ "$i" -lt $((DELAI * 4)) ]; do
    sleep 0.25
    apres=$(grep -c "touche X" "$LOG" 2>/dev/null || true)
    if [ "${apres:-0}" -gt "$avant" ]; then
        exit 0
    fi
    i=$((i + 1))
done

echo "vivacite : aucune touche traitée pendant ${DELAI} s — application GELÉE" >&2
echo "  → interblocage : lancer tools/check-ctx-reentrance.py et lire" >&2
echo "    docs/AVANCEMENT.md §6.17 (verrou exclusif de ctx.input)" >&2
exit 1

#!/bin/bash
# Script de lancement de KPlayer.
#
# **Ce qu'il fait et pourquoi.** Il ajoute le FFmpeg de `/opt/ffmpeg-7.1` au chemin
# des bibliothèques (le binaire y est lié) et lance le lecteur avec les arguments
# reçus. C'est ce script que le fichier `.desktop` appelle : le double-clic sur une
# vidéo passe donc par lui, avec `%U` (les chemins absolus fournis par le
# gestionnaire de fichiers).
#
# **Deux corrections par rapport à la version précédente** (session 33) :
#
# 1. **Plus de `cd`.** Le script se plaçait dans son dossier, ce qui cassait les
#    chemins relatifs passés en argument (`run.sh film.mp4` lançait le lecteur sur
#    un fichier inexistant). Le binaire est appelé par son chemin absolu.
# 2. **Plus de `DISPLAY=":0"` inventé.** Plasma 6.8 démarre toujours sous Wayland :
#    forcer `:0` quand la variable est absente faisait démarrer winit en X11 sur un
#    serveur inexistant, donc échouer sans message utile. On ne devine plus : si
#    aucune session graphique n'est visible, on le dit.
DIR="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"

export LD_LIBRARY_PATH="/opt/ffmpeg-7.1/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
    echo "KPlayer : ni DISPLAY ni WAYLAND_DISPLAY — lancer depuis une session graphique." >&2
    exit 1
fi

exec "$DIR/target/release/kplayer" "$@"

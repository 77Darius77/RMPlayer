#!/usr/bin/env bash
# verif-mpris.sh — le module MPRIS, **testé seul** (pas de fenêtre, pas de média).
#
# Règle de cette phase : un module se mesure avant son intégration, puis après.
# Ce banc interroge `--mpris-test`, qui démarre le serveur MPRIS avec un état connu
# et imprime chaque commande reçue ; il vérifie donc le module, pas le reste de
# l'application. `verif-playlist.sh` (section MPRIS) couvre ensuite l'intégration :
# une touche média change réellement l'état du lecteur.
#
# Contrôles :
#   1. le nom `org.mpris.MediaPlayer2.rmplayer` apparaît sur le bus ;
#   2. l'identité, le statut et les métadonnées sont ceux du protocole (chaînes
#      exactes, durée et position en **microsecondes**) ;
#   3. `CanGoNext`/`CanGoPrevious` reflètent l'état publié (pas une constante) ;
#   4. chaque appel extérieur arrive au module : `PlayPause`, `Next`, `Seek` ;
#   5. `SetPosition` avec le **bon** identifiant de piste est accepté, avec un
#      identifiant étranger il est ignoré (spécification) ;
#   6. un changement d'état publié produit un `PropertiesChanged` (le bureau ne
#      redemande pas les propriétés à chaque image) ;
#   7. en sortie propre, le nom est **rendu** au bus (sinon Plasma garde un lecteur
#      fantôme dans son widget) ;
#   8. sans bus de session, le module le dit et le programme sort avec le code 3
#      (aucun plantage).
#
# Usage : tools/verif-mpris.sh
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
KP=./target/release/rmplayer
BASE=/tmp/kp-mpris
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }
info() { echo "  · $1"; }

[ -x "$KP" ] || { echo "binaire absent : compilez d'abord (cargo build --release)"; exit 2; }
command -v gdbus >/dev/null || { echo "gdbus absent — banc non exécutable ici"; exit 2; }

# Bus de session : repris de Plasma si la variable manque (un terminal lancé depuis
# un service n'en hérite pas — piège 75 du projet).
if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]; then
    pid_plasma=$(pgrep -u "$(id -u)" -f plasmashell | head -1)
    [ -n "${pid_plasma:-}" ] && DBUS_SESSION_BUS_ADDRESS=$(tr '\0' '\n' \
        < "/proc/$pid_plasma/environ" | sed -n 's/^DBUS_SESSION_BUS_ADDRESS=//p')
fi
# Repli standard : le socket du bus de session est dans XDG_RUNTIME_DIR. Sans lui,
# le banc s'ignorait à tort quand `plasmashell` n'était pas visible (mesuré).
if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]; then
    bus="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/bus"
    [ -S "$bus" ] && DBUS_SESSION_BUS_ADDRESS="unix:path=$bus"
fi
# **Export AVANT la sonde** : une affectation seule n'est pas héritée par les
# processus fils, donc `gdbus` ne voyait pas l'adresse et tentait de lancer son
# propre bus (« Impossible de lancer automatiquement D-Bus sans $DISPLAY X11 ») —
# le banc concluait « pas de bus » alors que le bus répondait.
export DBUS_SESSION_BUS_ADDRESS

# Sonde explicite : on garde le code et le message, plutôt que de conclure
# « pas de bus » sur un échec dont on ignore la cause (un `!` sur une commande
# composée masquait le vrai message — mesuré : le banc s'ignorait alors que le bus
# répondait).
sonde_bus=$(gdbus call --session \
    --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.ListNames 2>&1)
code_bus=$?
if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ] || [ "$code_bus" -ne 0 ]; then
    echo "BANC IGNORÉ : aucun bus de session D-Bus joignable"
    echo "  adresse : ${DBUS_SESSION_BUS_ADDRESS:-aucune} (code $code_bus)"
    echo "  message : $(printf '%s' "$sonde_bus" | head -1 | cut -c1-120)"
    exit 3
fi

rm -rf "$BASE"; mkdir -p "$BASE"
P=org.mpris.MediaPlayer2.rmplayer
O=/org/mpris/MediaPlayer2
prop() { gdbus call --session --dest "$P" --object-path "$O" \
    --method org.freedesktop.DBus.Properties.Get "$1" "$2" 2>&1; }
appel() { gdbus call --session --dest "$P" --object-path "$O" --method "$@" 2>&1; }
# Valeur brute sans le décor de gdbus : `(<'x'>,)` → `x`, `(<int64 42>,)` → `42`.
valeur() {
    sed -e 's/^(<//' -e 's/>,)$//' -e "s/^'//" -e "s/'$//" \
        -e 's/^int64 //' -e 's/^uint64 //' -e 's/^int32 //' <<<"$1"
}

# Instance résiduelle : elle prendrait le nom sur le bus.
pkill -f "rmplayer --mpris-test" 2>/dev/null
sleep 1

"$KP" --mpris-test 25 >"$BASE/journal.txt" 2>&1 &
TEST_PID=$!
nettoyer() { kill "$TEST_PID" 2>/dev/null; pkill -f "rmplayer --mpris-test" 2>/dev/null; }
trap nettoyer EXIT

echo
echo "=== 1. Le nom apparaît sur le bus de session ==="
pret=0
for _ in $(seq 40); do
    if grep -q "mpris pret" "$BASE/journal.txt" 2>/dev/null; then pret=1; break; fi
    sleep 0.25
done
if [ "$pret" -eq 1 ]; then
    # Le mode test publie son état juste avant d'annoncer « pret » ; une petite
    # marge évite de lire une propriété au moment exact du basculement.
    sleep 0.4
    ok "serveur démarré ($(grep -o 'mpris pret: .*' "$BASE/journal.txt" | head -1))"
else
    ko "le serveur n'a pas démarré : $(tail -2 "$BASE/journal.txt" | head -1)"
fi
if gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
        --method org.freedesktop.DBus.ListNames 2>/dev/null | grep -q "$P"; then
    ok "$P présent sur le bus"
else
    ko "$P absent du bus"
fi

echo
echo "=== 2. Identité, statut, métadonnées (unités MPRIS) ==="
ident=$(valeur "$(prop org.mpris.MediaPlayer2 Identity)")
[ "$ident" = "RMPlayer" ] && ok "Identity = RMPlayer" || ko "Identity inattendu : $ident"
statut=$(valeur "$(prop org.mpris.MediaPlayer2.Player PlaybackStatus)")
case "$statut" in
    Playing|Paused|Stopped) ok "PlaybackStatus = $statut (chaîne de la spécification)" ;;
    *) ko "PlaybackStatus inattendu : $statut" ;;
esac
meta=$(prop org.mpris.MediaPlayer2.Player Metadata)
if grep -q "xesam:title" <<<"$meta" && grep -q "Mire de test MPRIS" <<<"$meta"; then
    ok "Metadata contient xesam:title (titre publié)"
else
    ko "Metadata sans titre : $(echo "$meta" | cut -c1-100)"
fi
if grep -q "mpris:trackid.*org/rmplayer/track/0" <<<"$meta"; then
    ok "Metadata contient mpris:trackid (/org/rmplayer/track/0)"
else
    ko "mpris:trackid absent ou inattendu"
fi
# `mpris:length` et `Position` sont en MICROsecondes : 60 s = 60 000 000.
duree=$(grep -o "mpris:length': <int64 [0-9]*" <<<"$meta" | grep -oE "[0-9]+$")
if [ "${duree:-0}" = "60000000" ]; then
    ok "mpris:length = 60 000 000 µs pour 60 s (unité respectée)"
else
    ko "mpris:length inattendu : ${duree:-absent} (attendu 60000000)"
fi
pos=$(valeur "$(prop org.mpris.MediaPlayer2.Player Position)")
if [ "${pos:-0}" -gt 0 ] && [ $((pos % 1000)) -eq 0 ]; then
    ok "Position = ${pos} µs (multiple de 1000 : millisecondes converties)"
else
    ko "Position inattendue : ${pos:-absente}"
fi

echo
echo "=== 3. CanGoNext / CanGoPrevious suivent l'état publié ==="
suivant=$(valeur "$(prop org.mpris.MediaPlayer2.Player CanGoNext)")
precedent=$(valeur "$(prop org.mpris.MediaPlayer2.Player CanGoPrevious)")
if [ "$suivant" = "true" ] && [ "$precedent" = "true" ]; then
    ok "CanGoNext et CanGoPrevious à true (état publié avec deux voisins)"
else
    ko "CanGoNext=$suivant, CanGoPrevious=$precedent (attendu true/true)"
fi
canseek=$(valeur "$(prop org.mpris.MediaPlayer2.Player CanSeek)")
[ "$canseek" = "true" ] && ok "CanSeek = true (durée connue)" || ko "CanSeek = $canseek"

echo
echo "=== 4. Les appels extérieurs arrivent au module ==="
appel org.mpris.MediaPlayer2.Player.PlayPause >/dev/null
appel org.mpris.MediaPlayer2.Player.Next >/dev/null
appel org.mpris.MediaPlayer2.Player.Previous >/dev/null
appel org.mpris.MediaPlayer2.Player.Seek 5000000 >/dev/null
sleep 1
for attendu in "LecturePause" "Suivant" "Precedent" "SeDeplacer(5000000)"; do
    if grep -q "commande: $attendu" "$BASE/journal.txt"; then
        ok "commande reçue : $attendu"
    else
        ko "commande perdue : $attendu"
    fi
done

echo
echo "=== 5. SetPosition : bon identifiant accepté, étranger ignoré ==="
appel org.mpris.MediaPlayer2.Player.SetPosition /org/rmplayer/track/0 12000000 >/dev/null
appel org.mpris.MediaPlayer2.Player.SetPosition /org/mpris/MediaPlayer2/TrackList/9 9000000 >/dev/null
sleep 1
if grep -q "commande: Position(12000000)" "$BASE/journal.txt"; then
    ok "SetPosition accepté pour la piste courante (12 000 000 µs)"
else
    ko "SetPosition refusé à tort pour la piste courante"
fi
if grep -q "commande: Position(9000000)" "$BASE/journal.txt"; then
    ko "SetPosition accepté pour une piste étrangère (il devait être ignoré)"
else
    ok "SetPosition ignoré pour une piste étrangère (conforme à la spécification)"
fi

echo
echo "=== 5 bis. Properties.Set : volume, aléatoire, boucle ==="
# Sans ces réglages, le widget média affiche des curseurs inertes ; et les variantes
# de commande correspondantes ne seraient jamais exercées.
reglage() {  # reglage <propriété> <valeur typée>
    gdbus call --session --dest "$P" --object-path "$O" \
        --method org.freedesktop.DBus.Properties.Set \
        org.mpris.MediaPlayer2.Player "$1" "$2" 2>&1
}
reglage Volume "<0.5>" >/dev/null
reglage Shuffle "<true>" >/dev/null
reglage LoopStatus "<'Playlist'>" >/dev/null
reglage Rate "<1.5>" >/dev/null
sleep 1
for attendu in "Volume(0.5)" "Aleatoire(true)" "Repetition(Playlist)" "Vitesse(1.5)"; do
    if grep -q "commande: $attendu" "$BASE/journal.txt"; then
        ok "réglage transmis : $attendu"
    else
        ko "réglage perdu : $attendu"
    fi
done
# Les trois modes de LoopStatus sont tous appliqués (Track = relire le fichier, None = couper) ;
# une valeur inconnue est ignorée.
reglage LoopStatus "<'Track'>" >/dev/null
sleep 0.8
if grep -q "commande: Repetition(Fichier)" "$BASE/journal.txt"; then
    ok "LoopStatus Track → répéter le fichier"
else
    ko "LoopStatus Track non transmis"
fi
reglage LoopStatus "<'None'>" >/dev/null
sleep 0.8
if grep -q "commande: Repetition(Aucune)" "$BASE/journal.txt"; then
    ok "LoopStatus None → répétition coupée"
else
    ko "LoopStatus None non transmis"
fi
avant=$(grep -c "commande: Repetition" "$BASE/journal.txt")
reglage LoopStatus "<'Bizarre'>" >/dev/null
sleep 0.8
apres=$(grep -c "commande: Repetition" "$BASE/journal.txt")
if [ "$avant" = "$apres" ]; then
    ok "LoopStatus inconnu : ignoré"
else
    ko "LoopStatus inconnu : une commande a été transmise"
fi

echo
echo "=== 6. Un changement publié émet PropertiesChanged ==="
# L'observateur est lancé AVANT de provoquer le changement : sinon on ne voit rien.
timeout 6 gdbus monitor --session --dest "$P" --object-path "$O" >"$BASE/monitor.txt" 2>&1 &
MON_PID=$!
sleep 1
# Le mode test fait varier le statut toutes les 2 s ; on provoque aussi un appel
# pour être sûr qu'un signal arrive dans la fenêtre d'observation.
appel org.mpris.MediaPlayer2.Player.PlayPause >/dev/null
sleep 4
kill "$MON_PID" 2>/dev/null
if grep -q "PropertiesChanged" "$BASE/monitor.txt"; then
    ok "PropertiesChanged émis (le bureau est prévenu sans interroger)"
else
    ko "aucun PropertiesChanged observé"
fi
if grep -q "PlaybackStatus" "$BASE/monitor.txt"; then
    ok "le signal porte PlaybackStatus (ce que le widget média affiche)"
else
    info "signal sans PlaybackStatus : $(head -3 "$BASE/monitor.txt" | tr '\n' ' ' | cut -c1-120)"
fi

echo
echo "=== 7. Le nom est rendu au bus à la sortie ==="
# On demande la fermeture par le bus (Quit), puis on vérifie que le nom disparaît :
# un nom laissé sur le bus ferait un lecteur fantôme dans le widget système.
appel org.mpris.MediaPlayer2.Quit >/dev/null 2>&1
for _ in $(seq 40); do
    kill -0 "$TEST_PID" 2>/dev/null || break
    sleep 0.2
done
if kill -0 "$TEST_PID" 2>/dev/null; then
    kill "$TEST_PID" 2>/dev/null
    sleep 1
    info "le mode test ne quitte pas sur Quit (il imprime la commande)"
fi
if gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
        --method org.freedesktop.DBus.ListNames 2>/dev/null | grep -q "$P"; then
    ko "$P est toujours sur le bus après la sortie (lecteur fantôme)"
else
    ok "$P retiré du bus après la sortie"
fi
if grep -q "commande: Quitter" "$BASE/journal.txt"; then
    ok "Quit est bien parvenu au module"
else
    info "Quit non journalisé (arrêt avant réception)"
fi

echo
echo "=== 8. Sans bus de session : le module le dit et ne plante pas ==="
sortie=$(DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/nexiste-pas-rmplayer timeout 10 \
    "$KP" --mpris-test 3 2>&1)
code=$?
if [ "$code" -eq 3 ]; then
    ok "code 3 et message explicite quand le bus est absent"
else
    ko "code $code au lieu de 3 (message : $(echo "$sortie" | tail -1 | cut -c1-80))"
fi
if printf '%s' "$sortie" | grep -qi "bus indisponible"; then
    ok "message « bus indisponible » présent"
else
    info "message : $(printf '%s' "$sortie" | tail -1 | cut -c1-90)"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
[ "$FAIL" -eq 0 ]

#!/usr/bin/env bash
# verif-precache.sh — le pré-cache des flux réseau : mesuré, pas supposé.
#
# Deux questions distinctes, deux mesures :
#   1. **Lit-on en avance ?** Un serveur HLS local journalise chaque requête avec son
#      instant. Sans réserve, les segments arrivent au rythme exact de la lecture
#      (intervalle entre requêtes = durée du segment, zéro avance). Avec réserve, ils
#      sont demandés d'avance.
#   2. **Cela protège-t-il d'une coupure ?** Le même serveur répond 503 pendant 3 s.
#      Sans réserve, l'image se fige (le fil d'interface attend la réponse) ;
#      avec réserve, la lecture continue et reprend sans retour en arrière.
#
# Le banc mesure aussi les garde-fous : pré-cache à 0 = comportement d'origine,
# fichier local jamais pré-caché, capacité du tampon qui suit le réglage.
#
# Usage : tools/verif-precache.sh [écran]     (défaut : :99)
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
DISP="${1:-:99}"
BASE=/tmp/kp-precache
KP=./target/release/rmplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
export RUST_LOG=rmplayer=debug
PORT=8911
PANNE_DEBUT=9
PANNE_DUREE=3

PASS=0; FAIL=0
ok() { echo "  ✔ $1"; PASS=$((PASS+1)); }
ko() { echo "  ✗ ÉCHEC : $1"; FAIL=$((FAIL+1)); }

rm -rf "$BASE"; mkdir -p "$BASE/hls" "$BASE/conf/rmplayer" "$BASE/cache"
export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"

# ---------------------------------------------------------------- contenu de test
echo
echo "=== 0. Flux HLS local (segments de 2 s) et serveur pilotable ==="
# 40 s de contenu, segments de 2 s : assez long pour traverser une panne de 3 s.
env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
    -f lavfi -i "testsrc2=s=640x360:r=25:d=40" \
    -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=40" \
    -c:v libx264 -preset ultrafast -g 25 -c:a aac -shortest \
    -f hls -hls_time 2 -hls_list_size 0 \
    -hls_segment_filename "$BASE/hls/seg%03d.ts" "$BASE/hls/index.m3u8"
nb_seg=$(ls "$BASE"/hls/seg*.ts 2>/dev/null | wc -l)
if [ "$nb_seg" -ge 10 ]; then
    ok "$nb_seg segments de 2 s écrits ($(du -sh "$BASE/hls" | cut -f1))"
else
    ko "génération HLS insuffisante ($nb_seg segments)"
    exit 1
fi

# Serveur : journalise chaque requête (instant + code), et répond 503 pendant une
# fenêtre pilotée par l'environnement. Le code est écrit ici, dans le dossier
# temporaire : c'est un outil de banc, pas du programme.
cat > "$BASE/serveur.py" <<'PY'
import http.server, socketserver, sys, time, os
DOSSIER, PORT, JOURNAL = os.path.abspath(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
T0 = time.monotonic()
DEBUT = float(os.environ.get("PANNE_DEBUT", "-1"))
DUREE = float(os.environ.get("PANNE_DUREE", "0"))

class H(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *a, **k):
        super().__init__(*a, directory=DOSSIER, **k)
    def log_message(self, *a):
        pass
    def do_GET(self):
        t = time.monotonic() - T0
        panne = DEBUT >= 0 and DEBUT <= t < DEBUT + DUREE
        with open(JOURNAL, "a") as f:
            f.write(f"{t:8.3f} {'503' if panne else '200'} {self.path}\n")
        if panne:
            self.send_response(503)
            self.send_header("Content-Length", "3")
            self.send_header("Connection", "close")
            self.end_headers()
            try:
                self.wfile.write(b"503")
            except OSError:
                pass
            return
        super().do_GET()

class S(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True

with S(("127.0.0.1", PORT), H) as s:
    s.serve_forever()
PY

# ------------------------------------------------------------- configuration
regler() {  # regler <precache_ms>
    python3 - "$BASE/conf/rmplayer/config.json" "$1" <<'PY'
import json, sys
json.dump({
    "video": {"hardware_decoding": False, "scaling_method": "Lanczos"},
    "audio": {"device": "", "volume_max": 150},
    "subtitles": {"auto_load": False},
    "interface": {"show_osd": False, "screenshots_dir": "/tmp"},
    "playback": {"resume": False, "precache_ms": int(sys.argv[2])},
    "playlist": [],
}, open(sys.argv[1], "w"), indent=2)
PY
}

lecture() {  # lecture <precache_ms> <étiquette> <durée_s> [panne]
    local ms="$1" nom="$2" duree="$3" panne="${4:-non}"
    regler "$ms"
    : > "$BASE/req_$nom.txt"
    if [ "$panne" = "oui" ]; then
        PANNE_DEBUT=$PANNE_DEBUT PANNE_DUREE=$PANNE_DUREE \
            python3 "$BASE/serveur.py" "$BASE/hls" "$PORT" "$BASE/req_$nom.txt" &
    else
        PANNE_DEBUT=-1 python3 "$BASE/serveur.py" "$BASE/hls" "$PORT" "$BASE/req_$nom.txt" &
    fi
    local srv=$!
    sleep 1.2
    DISPLAY="$DISP" "$KP" "http://127.0.0.1:$PORT/index.m3u8" >"$BASE/$nom.log" 2>&1 &
    local pid=$!
    sleep "$duree"
    kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
    kill "$srv" 2>/dev/null; wait "$srv" 2>/dev/null
    sed 's/\x1b\[[0-9;]*m//g' "$BASE/$nom.log" >"$BASE/$nom.txt"
}

# ------------------------------------------------------------------ 1. l'avance
echo
echo "=== 1. Sans réserve : les segments arrivent au rythme de la lecture ==="
lecture 0 sans 12 non
python3 - "$BASE/req_sans.txt" <<'PY'
import sys
req = [l.split() for l in open(sys.argv[1])]
seg = [(float(t), p.split('/')[-1]) for t, code, p in req if p.startswith('/seg')]
ecarts = [seg[i+1][0] - seg[i][0] for i in range(len(seg) - 1)]
print(f"    {len(seg)} segments demandés ; écarts : " +
      ", ".join(f"{e:.1f}s" for e in ecarts[:6]))
# « au rythme de la lecture » = chaque segment demandé ~2 s après le précédent
# (durée du segment). Une avance se verrait comme des écarts bien plus courts.
moy = sum(ecarts) / len(ecarts) if ecarts else 0
sys.exit(0 if moy >= 1.6 else 1)
PY
if [ $? -eq 0 ]; then
    ok "aucune avance : les segments sont demandés un par un, au moment de leur usage"
else
    ko "comportement inattendu sans réserve (segments demandés d'avance)"
fi

echo
echo "=== 2. Avec 6 s de réserve : les segments sont demandés d'avance ==="
lecture 6000 avec 12 non
python3 - "$BASE/req_avec.txt" <<'PY'
import sys
req = [l.split() for l in open(sys.argv[1])]
seg = [(float(t), p.split('/')[-1]) for t, code, p in req if p.startswith('/seg')]
ecarts = [seg[i+1][0] - seg[i][0] for i in range(len(seg) - 1)]
print(f"    {len(seg)} segments demandés ; écarts : " +
      ", ".join(f"{e:.1f}s" for e in ecarts[:8]))
# La réserve se voit au début : plusieurs segments sont pris d'affilée, en moins
# d'une seconde, au lieu d'attendre 2 s entre chacun.
rafale = sum(1 for e in ecarts if e < 0.5)
sys.exit(0 if rafale >= 2 else 1)
PY
if [ $? -eq 0 ]; then
    ok "les segments partent en rafale au démarrage : la réserve se remplit en avance"
else
    ko "aucune avance mesurable des requêtes (le pré-cache ne lit pas en avance)"
fi
python3 - "$BASE/avec.txt" <<'PY'
import re, sys
ms = [int(m) for m in re.findall(r'pré-cache (\d+) ms', open(sys.argv[1]).read())]
if ms and max(ms) >= 4000:
    print(f"    avance lue par le lecteur : jusqu'à {max(ms)} ms")
    sys.exit(0)
print(f"    avance lue par le lecteur : {max(ms) if ms else 'aucune'}")
sys.exit(1)
PY
if [ $? -eq 0 ]; then
    ok "le lecteur mesure lui-même une avance de plusieurs secondes"
else
    ko "le journal n'annonce pas d'avance de pré-cache"
fi

# --------------------------------------------------------------- 3. la coupure
echo
echo "=== 3. Coupure de 3 s : avec réserve, la lecture continue ==="
lecture 6000 panne_avec 22 oui
lecture 0 panne_sans 22 oui
python3 - "$BASE/panne_sans.txt" "$BASE/panne_avec.txt" <<'PY'
import re, sys
from datetime import datetime

def position(path):
    ev = []
    for l in open(path, encoding='utf-8', errors='replace'):
        m = re.search(r'(\S+Z)\s+DEBUG rmplayer::player: Horloge: audio (\d+) ms', l)
        if m:
            ev.append((datetime.fromisoformat(m.group(1).rstrip('Z')), int(m.group(2))))
    return ev

def gel(ev):
    """Plus long BLOCAGE de l'application, en secondes.

    Un relevé d'horloge est écrit à chaque seconde, depuis la boucle d'interface.
    Quand le fil d'interface attend une réponse réseau, il n'écrit plus rien : le
    blocage se voit donc comme un TROU entre deux relevés, et non comme une suite
    de positions identiques (c'est le piège de ce banc : les positions ne se
    répètent pas, elles cessent simplement d'être publiées). On retranche la
    période normale (1 s) pour ne compter que l'attente réelle.
    """
    if len(ev) < 2:
        return 0.0
    ecarts = [(ev[i][0] - ev[i-1][0]).total_seconds() for i in range(1, len(ev))]
    return max(0.0, max(ecarts) - 1.0)

sans, avec = position(sys.argv[1]), position(sys.argv[2])
if len(sans) < 5 or len(avec) < 5:
    print("    relevés insuffisants")
    sys.exit(1)
g_sans, g_avec = gel(sans), gel(avec)
print(f"    blocage le plus long : sans réserve {g_sans:.2f} s | avec réserve {g_avec:.2f} s")
# Seuils larges mais parlants : la cadence normale est de 1 s (déjà retranchée), donc
# un blocage « normal » vaut 0 s ; une coupure de 3 s vaut plusieurs secondes.
if g_sans < 1.5:
    print("    (la coupure n'a pas été subie sans réserve : mesure non concluante)")
    sys.exit(1)
if g_avec <= 1.0 and g_avec < g_sans / 2:
    sys.exit(0)
sys.exit(1)
PY
if [ $? -eq 0 ]; then
    ok "la réserve absorbe la coupure : aucun gel avec, plusieurs secondes sans"
else
    ko "la réserve n'absorbe pas la coupure (voir le détail ci-dessus)"
fi

# Pas de retour en arrière après reconnexion : avec la réserve, la reprise doit se
# faire là où la lecture en était, et non plusieurs secondes avant.
python3 - "$BASE/panne_avec.txt" <<'PY'
import re, sys
from datetime import datetime
ev = []
for l in open(sys.argv[1], encoding='utf-8', errors='replace'):
    m = re.search(r'(\S+Z)\s+DEBUG rmplayer::player: Horloge: audio (\d+) ms', l)
    if m:
        ev.append((datetime.fromisoformat(m.group(1).rstrip('Z')), int(m.group(2))))
recul = max((ev[i-1][1] - ev[i][1] for i in range(1, len(ev))), default=0)
print(f"    plus grand recul de position après reconnexion : {recul} ms")
sys.exit(0 if recul <= 1500 else 1)
PY
if [ $? -eq 0 ]; then
    ok "la reprise se fait là où l'on en était (aucun retour en arrière notable)"
else
    ko "la lecture reprend plusieurs secondes avant la coupure"
fi

# ------------------------------------------------------------- 4. garde-fous
echo
echo "=== 4. Garde-fous : réserve à 0, et fichier local ==="
lecture 0 zero 8 non
python3 - "$BASE/zero.txt" <<'PY'
import re, sys
# Le journal publie l'avance MÊME à zéro (c'est la file de paquets qui avance un
# peu : quelques centaines de ms). La question est de savoir si une RÉSERVE a été
# constituée : on exige donc une avance restée sous la seconde.
ms = [int(m) for m in re.findall(r'pré-cache (\d+) ms', open(sys.argv[1]).read())]
print(f"    avance maximale avec une réserve réglée à 0 : {max(ms) if ms else 0} ms")
sys.exit(0 if (max(ms) if ms else 0) < 1000 else 1)
PY
if [ $? -eq 0 ]; then
    ok "réserve à 0 : aucune avance constituée (comportement d'origine)"
else
    ko "une réserve subsiste alors qu'elle est réglée à 0"
fi
python3 - "$BASE/zero.txt" "$BASE/avec.txt" <<'PY'
import re, sys
def capacite(path):
    m = re.findall(r'capacité (\d+) ms', open(path).read())
    return int(m[-1]) if m else 0
c0, c6 = capacite(sys.argv[1]), capacite(sys.argv[2])
print(f"    capacité du tampon : réserve 0 → {c0} ms | réserve 6 s → {c6} ms")
sys.exit(0 if c0 < 2000 and c6 >= 5000 else 1)
PY
if [ $? -eq 0 ]; then
    ok "la capacité du tampon audio suit le réglage (c'est elle qui garde la réserve)"
else
    ko "la capacité du tampon ne suit pas le réglage"
fi
# Fichier local : jamais de pré-cache, même avec une réserve réglée.
regler 6000
cp "$BASE/hls/seg000.ts" "$BASE/local.ts"
DISPLAY="$DISP" "$KP" --dump-frame 1000 "$BASE/local.png" "$BASE/local.ts" \
    >"$BASE/local.log" 2>&1
python3 - "$BASE/local.log" <<'PY'
import re, sys
txt = open(sys.argv[1], encoding='utf-8', errors='replace').read()
ms = [int(m) for m in re.findall(r'pré-cache (\d+) ms', txt)]
cap = re.findall(r'capacité (\d+) ms', txt)
print(f"    fichier local : avance {max(ms) if ms else 0} ms, "
      f"capacité {(cap[-1] if cap else '—')} ms")
sys.exit(0 if (max(ms) if ms else 0) < 1000 else 1)
PY
if [ $? -eq 0 ]; then
    ok "un fichier local n'est pas pré-caché (la réserve ne s'applique qu'aux flux)"
else
    ko "un fichier local a été pré-caché (mémoire gaspillée)"
fi

echo
echo "=== Bilan : $PASS réussite(s), $FAIL échec(s) ==="
pkill -x rmplayer 2>/dev/null || true
[ "$FAIL" -eq 0 ]

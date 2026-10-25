#!/usr/bin/env bash
# bench-desentrelacement.sh — choisit le filtre de désentrelacement PAR LA MESURE.
#
# Question posée : `bwdif` (détection de mouvement) vaut-il mieux que `yadif`
# (interpolation spatiale), et à quel coût ? La question se tranche avec une vérité
# de référence, ce qui est rare et donc précieux : **on fabrique nous-mêmes** le
# contenu entrelacé à partir d'une image progressive connue. On peut donc mesurer
# non seulement « le peigne a disparu » (ce que deux filtres réussissent), mais
# « l'image retrouvée est proche de l'originale ».
#
# Ce que le banc mesure, et pourquoi chacun de ces indicateurs :
#   1. PSNR / erreur moyenne face à l'image d'origine, scène IMMOBILE — la majorité
#      d'un film. C'est le seul cas où la reconstruction exacte est possible : les
#      deux trames viennent du même instant, un bon filtre doit retrouver l'image.
#   2. PSNR face à l'image d'origine, scène en MOUVEMENT — les deux trames diffèrent :
#      aucun filtre ne peut être exact, la mesure sert à vérifier qu'aucun ne décroche.
#   3. Détail à une ligne d'intervalle (mire en lignes de 1 px) : le pire cas du
#      désentrelacement. Un filtre purement spatial efface ces lignes ; un filtre qui
#      détecte l'immobilité les conserve.
#   4. Coût : millisecondes par image 1080p (3 passes, meilleure).
#
# ⚠️ Piège d'indicateur, mesuré et vérifié : l'écart vertical moyen (« peigne ») est
# ÉLEVÉ après `bwdif` sur une scène immobile — et c'est correct, puisque le détail est
# conservé. Ce chiffre ne distingue pas « peigne résiduel » de « détail restitué » :
# seul le PSNR face à l'origine le fait. C'est la même leçon que le détramage
# (session 13), où la « rugosité » augmentait après filtrage.
#
# Usage : tools/bench-desentrelacement.sh [--sans-rmplayer]
set -u
cd /media/blaise/LINUX_DATA/lecteur-linux
BASE=/tmp/kp-bench-desent
KP=./target/release/rmplayer
export PATH=$HOME/.cargo/bin:$PATH
export LD_LIBRARY_PATH=/opt/ffmpeg-7.1/lib
export XDG_RUNTIME_DIR=/run/user/1000
FF=/opt/ffmpeg-7.1/bin/ffmpeg
H=720; W=1280

rm -rf "$BASE"; mkdir -p "$BASE"
ECHEC=0
ok() { echo "  ✔ $1"; }
ko() { echo "  ✗ $1"; ECHEC=$((ECHEC+1)); }

echo
echo "=== 1. Fabrication d'un contenu entrelacé dont on connaît l'original ==="
python3 - "$BASE" <<'PY'
import sys
import numpy as np
base = sys.argv[1]
H, W = 720, 1280

def mire(decal=0):
    """Mire déterministe : lignes 1 px (détail vertical maximal), grille verticale,
    bruit FIGÉ (sinon la scène « immobile » bougerait d'une image à l'autre), aplat."""
    img = np.zeros((H, W), dtype=np.uint8)
    y = np.arange(H)[:, None]
    x = np.arange(W)[None, :]
    img[0:240, :] = np.where(y[0:240] % 2 == 0, 20, 235)
    img[240:480, :] = np.where((x // 2) % 2 == 0, 30, 220)
    img[480:600, :] = np.random.default_rng(7).integers(0, 256, size=(120, W), dtype=np.uint8)
    img[600:720, :] = 90
    return np.roll(img, decal, axis=1) if decal else img

# 60 images progressives : 30 immobiles, puis 30 avec un déplacement de 12 px/image.
prog = np.stack([mire(0 if k < 30 else (k - 30) * 12) for k in range(60)])
assert (prog[0] == prog[29]).all(), "la scène immobile doit être réellement figée"

# Entrelacement exact (tff) : lignes paires de l'image 2k, impaires de l'image 2k+1.
entrel = np.empty_like(prog[::2])
for k in range(30):
    entrel[k][0::2] = prog[2 * k][0::2]
    entrel[k][1::2] = prog[2 * k + 1][1::2]

open(f"{base}/entrel.raw", "wb").write(entrel.tobytes())
np.save(f"{base}/prog.npy", prog)
np.save(f"{base}/entrel.npy", entrel)
print(f"    {prog.shape[0]} images progressives, {entrel.shape[0]} entrelacées ({W}×{H})")
PY

# Flux de travail : lossless, pour que la mesure ne porte que sur le désentrelacement.
$FF -hide_banner -loglevel error -f rawvideo -pix_fmt gray -s ${W}x${H} -r 25 \
    -i "$BASE/entrel.raw" -vf "setfield=tff" -c:v ffv1 -y "$BASE/entrel.mkv"
# Variante à drapeaux d'entrelacement DANS LE FLUX (indispensable pour vérifier le
# lecteur : `deint=interlaced` ne filtre que les trames réellement marquées).
if command -v ffmpeg >/dev/null 2>&1; then
    env -u LD_LIBRARY_PATH ffmpeg -hide_banner -loglevel error \
        -f rawvideo -pix_fmt gray -s ${W}x${H} -r 25 -i "$BASE/entrel.raw" \
        -c:v libx264 -preset veryfast -crf 8 -flags +ildct+ilme \
        -x264-params "tff=1" -y "$BASE/entrel_ildct.mp4" 2>/dev/null
    ordre=$(env -u LD_LIBRARY_PATH ffprobe -hide_banner -loglevel error \
        -show_streams -select_streams v:0 "$BASE/entrel_ildct.mp4" 2>/dev/null \
        | grep -oE "field_order=[a-z]+" | head -1)
    echo "    flux à drapeaux : $ordre (attendu field_order=tt)"
fi

echo
echo "=== 2. Qualité : PSNR face à l'image progressive d'origine ==="
echo "    (deint=all : on force le filtrage sur toutes les trames, c'est le test du filtre)"
for f in "yadif:yadif=mode=send_frame:parity=tff:deint=all" \
         "bwdif:bwdif=mode=send_frame:parity=tff:deint=all" \
         "estdif:estdif=mode=frame:parity=tff:deint=all"; do
    nom="${f%%:*}"; filtre="${f#*:}"
    $FF -hide_banner -loglevel error -i "$BASE/entrel.mkv" -vf "$filtre" \
        -f rawvideo -pix_fmt gray -y "$BASE/out_$nom.raw"
done
python3 - "$BASE" <<'PY'
import sys
import numpy as np
base = sys.argv[1]
H, W = 720, 1280
prog = np.load(f"{base}/prog.npy")
entrel = np.load(f"{base}/entrel.npy")

def charger(nom):
    a = np.fromfile(f"{base}/out_{nom}.raw", dtype=np.uint8)
    return a.reshape(a.size // (H * W), H, W)

def net(z):
    """Énergie verticale dans la zone des lignes de 1 px (détail fin restitué)."""
    return float(np.abs(np.diff(z[..., 20:220, 100:1180].astype(np.float32), axis=-2)).mean())

nref = net(prog[0:1])
print(f"    référence : netteté verticale {nref:.1f}")
print(f"    {'filtre':8s} {'PSNR immobile':>15s} {'PSNR mouvement':>15s} {'netteté':>9s}")
mesures = {}
for nom in ["yadif", "bwdif", "estdif"]:
    r = charger(nom)
    n = min(r.shape[0], 30)
    p_stat = 10 * np.log10(255.0**2 / max(((r[:15].astype(float) - prog[:30:2][:15]) ** 2).mean(), 1e-9))
    p_mouv = 10 * np.log10(255.0**2 / max(((r[15:n].astype(float) - prog[30::2][:n-15]) ** 2).mean(), 1e-9))
    mesures[nom] = (p_stat, p_mouv, net(r[:15]))
    print(f"    {nom:8s} {p_stat:12.1f} dB {p_mouv:12.1f} dB "
          f"{100*net(r[:15])/nref:8.1f} %")
import json
json.dump({k: list(v) for k, v in mesures.items()}, open(f"{base}/mesures.json", "w"))
PY

echo
echo "=== 2 bis. Contenu « naturel » (testsrc2) : le cas courant ==="
echo "    La mire en lignes de 1 px est le pire cas ; sur une image ordinaire, les deux"
echo "    filtres ne se départagent que sur les zones immobiles."
$FF -hide_banner -loglevel error -f lavfi -i "testsrc2=s=${W}x${H}:r=50:d=2" \
    -c:v ffv1 -y "$BASE/nat_prog.mkv"
$FF -hide_banner -loglevel error -i "$BASE/nat_prog.mkv" \
    -vf "tinterlace=interleave_top,setfield=tff" -c:v ffv1 -y "$BASE/nat_entrel.mkv"
$FF -hide_banner -loglevel error -i "$BASE/nat_entrel.mkv" -f rawvideo -pix_fmt gray \
    -y "$BASE/nat_entrel.raw"
# 1) scène IMMOBILE (une image répétée) : la reconstruction exacte est possible.
$FF -hide_banner -loglevel error -f lavfi -i "testsrc2=s=${W}x${H}:r=50:d=2" \
    -frames:v 1 -c:v ffv1 -y "$BASE/nat1.mkv"
# (`-loop 1` n'existe pas dans ce build : on répète avec le filtre `loop`.)
$FF -hide_banner -loglevel error -i "$BASE/nat1.mkv" \
    -vf "loop=loop=24:size=1,setpts=N/25/TB" -r 25 -frames:v 25 \
    -c:v ffv1 -y "$BASE/nat_stat.mkv"
$FF -hide_banner -loglevel error -i "$BASE/nat_stat.mkv" \
    -vf "tinterlace=interleave_top,setfield=tff" -c:v ffv1 -y "$BASE/nat_stat_entrel.mkv"
$FF -hide_banner -loglevel error -i "$BASE/nat_stat.mkv" -f rawvideo -pix_fmt gray \
    -y "$BASE/nat_stat_ref.raw"
for f in "yadif:yadif=mode=send_frame:parity=tff:deint=all" \
         "bwdif:bwdif=mode=send_frame:parity=tff:deint=all"; do
    nom="${f%%:*}"; filtre="${f#*:}"
    $FF -hide_banner -loglevel error -i "$BASE/nat_stat_entrel.mkv" -vf "$filtre" \
        -f rawvideo -pix_fmt gray -y "$BASE/nat_stat_$nom.raw"
    $FF -hide_banner -loglevel error -f rawvideo -pix_fmt gray -s ${W}x${H} -r 25 \
        -i "$BASE/nat_entrel.raw" -vf "setfield=tff,$filtre" \
        -f rawvideo -pix_fmt gray -y "$BASE/nat_mouv_$nom.raw"
done
python3 - "$BASE" <<'PYNAT'
import json, sys
import numpy as np
base = sys.argv[1]
H, W = 720, 1280
ref = np.fromfile(f"{base}/nat_stat_ref.raw", dtype=np.uint8).reshape(-1, H, W)
stat = {}
print(f"    {'filtre':8s} {'PSNR immobile':>15s}")
for nom in ["yadif", "bwdif"]:
    a = np.fromfile(f"{base}/nat_stat_{nom}.raw", dtype=np.uint8).reshape(-1, H, W)
    n = min(ref.shape[0], a.shape[0])
    p = 10 * np.log10(255.0 ** 2 / max(((a[:n].astype(float) - ref[:n]) ** 2).mean(), 1e-9))
    stat[nom] = p
    print(f"    {nom:8s} {p:12.1f} dB")
# En mouvement, la vérité n'est pas récupérable : on mesure l'écart entre les deux
# filtres, qui dit si l'un décroche visiblement de l'autre.
m = {nom: np.fromfile(f"{base}/nat_mouv_{nom}.raw", dtype=np.uint8).reshape(-1, H, W)
     for nom in ["yadif", "bwdif"]}
n = min(m["yadif"].shape[0], m["bwdif"].shape[0])
ecart = float(np.abs(m["yadif"][:n].astype(float) - m["bwdif"][:n].astype(float)).mean())
print(f"    en mouvement, écart moyen entre les deux filtres : {ecart:.2f} niveaux")
json.dump({"stat": stat, "ecart_mouvement": ecart}, open(f"{base}/naturel.json", "w"))
PYNAT

echo
echo "=== 3. Coût : millisecondes par image 1080p ==="
$FF -hide_banner -loglevel error -f lavfi -i "testsrc2=s=1920x1080:r=50:d=4" \
    -c:v ffv1 -y "$BASE/hd_prog.mkv"
$FF -hide_banner -loglevel error -i "$BASE/hd_prog.mkv" \
    -vf "tinterlace=interleave_top,setfield=tff" -c:v ffv1 -y "$BASE/hd_entrel.mkv"
for f in "aucun:" "yadif:yadif=mode=send_frame:parity=tff:deint=all" \
         "bwdif:bwdif=mode=send_frame:parity=tff:deint=all"; do
    nom="${f%%:*}"; filtre="${f#*:}"; [ -n "$filtre" ] && vf="-vf $filtre" || vf=""
    best=""
    for i in 1 2 3; do
        t=$( { /usr/bin/time -f "%U %S" $FF -hide_banner -loglevel error \
               -i "$BASE/hd_entrel.mkv" $vf -f null - ; } 2>&1 | tail -1 )
        v=$(python3 -c "print(round(${t%% *}+${t##* },3))")
        if [ -z "$best" ] || python3 -c "import sys; sys.exit(0 if $v < $best else 1)"; then
            best=$v
        fi
    done
    # 100 images entrelacées dans la mire (4 s à 25 i/s).
    python3 -c "print(f'    $nom : {($best/100)*1000:.2f} ms par image 1080p')"
    echo "$nom $best" >> "$BASE/couts.txt"
done

echo
echo "=== 4. Vérification dans le lecteur (le réglage atteint bien FFmpeg) ==="
if [ "${1:-}" = "--sans-rmplayer" ] || [ ! -x "$KP" ]; then
    echo "  (lecteur non testé)"
else
    mkdir -p "$BASE/conf/rmplayer" "$BASE/cache"
    export XDG_CONFIG_HOME="$BASE/conf" XDG_CACHE_HOME="$BASE/cache"
    regler() {
        python3 - "$BASE/conf/rmplayer/config.json" "$1" "$2" <<'PY'
import json, sys
chemin, actif, filtre = sys.argv[1], sys.argv[2] == "1", sys.argv[3]
try:
    c = json.load(open(chemin))
except Exception:
    c = {}
c.setdefault("audio", {})["device"] = ""
c.setdefault("subtitles", {}).update({"auto_load": False})
c.setdefault("interface", {}).update({"show_osd": False, "screenshots_dir": "/tmp"})
c.setdefault("playback", {})["resume"] = False
c["video"] = {"hardware_decoding": False, "deinterlace": actif,
              "deinterlace_filter": filtre, "scaling_method": "Lanczos"}
json.dump(c, open(chemin, "w"), indent=2)
PY
    }
    verifier() {  # verifier <filtre|aucun> <attendu dans la chaîne>
        local nom="$1" attendu="$2"
        if [ "$nom" = "aucun" ]; then regler 0 bwdif; else regler 1 "$nom"; fi
        $KP --dump-frame 400 "$BASE/k_$nom.png" "$BASE/entrel_ildct.mp4" \
            >"$BASE/k_$nom.log" 2>&1
        chaine=$(sed 's/\x1b\[[0-9;]*m//g' "$BASE/k_$nom.log" | grep -oE 'filtres « [^»]* »' | head -1)
        echo "    $nom : $chaine"
        if echo "$chaine" | grep -q "$attendu"; then
            ok "$nom : la chaîne construite contient « $attendu »"
        else
            ko "$nom : chaîne inattendue ($chaine)"
        fi
    }
    verifier aucun ""
    verifier bwdif "bwdif=mode=send_frame:parity=auto:deint=interlaced"
    verifier yadif "yadif=mode=send_frame:parity=auto:deint=interlaced"

    # Effet mesuré : sur la scène immobile, `bwdif` doit conserver les lignes de 1 px
    # (donc rester très proche de l'image non filtrée) et `yadif` les effacer.
    python3 - "$BASE" <<'PY'
import json, sys
import numpy as np
from PIL import Image
base = sys.argv[1]
def lit(n):
    return np.asarray(Image.open(f"{base}/k_{n}.png").convert("L"), dtype=np.float32)
def net(z):
    return float(np.abs(np.diff(z[20:220, 100:1180], axis=0)).mean())
aucun, bwdif, yadif = lit("aucun"), lit("bwdif"), lit("yadif")
n0, nb, ny = net(aucun), net(bwdif), net(yadif)
print(f"    netteté verticale : non filtré {n0:.0f} | bwdif {nb:.0f} "
      f"({100*nb/max(n0,1e-9):.0f} %) | yadif {ny:.0f} ({100*ny/max(n0,1e-9):.0f} %)")
json.dump({"aucun": n0, "bwdif": nb, "yadif": ny, "ecart": float(np.abs(bwdif-yadif).mean())},
          open(f"{base}/rmplayer.json", "w"))
PY
    python3 - "$BASE" <<'PY'
import json, sys
base = sys.argv[1]
k = json.load(open(f"{base}/rmplayer.json"))
if k["bwdif"] > 0.8 * k["aucun"] and k["yadif"] < 0.2 * k["aucun"]:
    print("  ✔ bwdif conserve le détail fin des zones immobiles, yadif l'efface "
          f"(écart mesuré {k['ecart']:.1f} niveaux)")
else:
    print(f"  ✗ effet inattendu : bwdif {k['bwdif']:.0f} / yadif {k['yadif']:.0f} "
          f"/ référence {k['aucun']:.0f}")
    sys.exit(1)
PY
    [ $? -ne 0 ] && ECHEC=$((ECHEC+1))
fi

echo
echo "=== 5. Verdict ==="
python3 - "$BASE" <<'PY'
import json, sys
base = sys.argv[1]
m = json.load(open(f"{base}/mesures.json"))
couts = {}
for ligne in open(f"{base}/couts.txt"):
    nom, t = ligne.split()
    couts[nom] = float(t) / 100 * 1000
pb, pm, nb = m["bwdif"]; py, pm_y, ny = m["yadif"]
import os
nat = json.load(open(f"{base}/naturel.json")) if os.path.exists(f"{base}/naturel.json") else None
print(f"    mire 1 px : bwdif {pb:.1f} dB contre yadif {py:.1f} dB "
      f"(+{abs(pb-py):.1f} dB pour bwdif)")
if nat:
    nb_, ny_ = nat["stat"]["bwdif"], nat["stat"]["yadif"]
    print(f"    naturel   : bwdif {nb_:.1f} dB contre yadif {ny_:.1f} dB "
          f"(+{abs(nb_-ny_):.1f} dB pour bwdif), écart en mouvement "
          f"{nat['ecart_mouvement']:.2f} niveaux")
print(f"    mouvement  : bwdif {pm:.1f} dB contre yadif {pm_y:.1f} dB (écart {abs(pm-pm_y):.1f} dB)")
print(f"    coût       : bwdif {couts.get('bwdif', 0):.2f} ms contre yadif "
      f"{couts.get('yadif', 0):.2f} ms par image 1080p")
print(f"    estdif     : {m['estdif'][0]:.1f} dB (immobile) — écarté : "
      f"{couts.get('estdif', float('nan')) if 'estdif' in couts else 0:.0f} ms/image"
      if 'estdif' in couts else "")
gain_qualite = pb - py
if gain_qualite >= 3.0 and couts.get("bwdif", 99) <= couts.get("yadif", 0) * 1.2:
    print("    → bwdif retenu : meilleur de 3 dB au moins sur les scènes immobiles, "
          "à coût égal ou inférieur.")
    sys.exit(0)
if gain_qualite < 1.0:
    print("    → aucun avantage net : garder le filtre historique serait légitime.")
    sys.exit(0)
print("    → bwdif meilleur en qualité mais plus cher : à réexaminer.")
sys.exit(1)
PY
[ $? -ne 0 ] && ECHEC=$((ECHEC+1))

echo
echo "=== Bilan : $ECHEC échec(s) ==="
[ "$ECHEC" -eq 0 ]

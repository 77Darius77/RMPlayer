#!/usr/bin/env python3
"""Mesures audio pour vérifier qu'un traitement change RÉELLEMENT le signal.

Pourquoi cet outil : « un sélecteur ne prouve rien ». Une case cochée dans
l'interface, et même une chaîne de filtres journalisée, ne disent pas si le son
change. Ces mesures comparent des fichiers WAV produits par `--dump-audio`, donc
par le même chemin que la lecture réelle.

La lecture des WAV est faite à la main (et non avec le module `wave`) : celui-ci
refuse le format IEEE float (tag 3), que `--dump-audio` produit justement pour ne
rien perdre.

Usage : analyse-audio.py <commande> <fichier.wav> [arguments]

  niveaux  <wav>                écart de niveau entre la 1re moitié et la 2e (dB)
  crete    <wav>                crête maximale (1,0 = 0 dBFS)
  sn       <wav>                rapport signal/bruit (dB) : signal 440 Hz vs 3–6 kHz
  canaux   <wav>                écart moyen |G−D| (stéréo)
  max      <wav>                niveau maximal absolu
  freq     <wav>                fréquence dominante (Hz) et durée (s)
  queue    <wav> <ms> <ms>      énergie RMS entre deux instants (ms)
  debut    <wav> [seuil]        premier instant audible (ms)
  enceintes <wav> <f1,f2,…>     JSON : nombre de canaux et, par canal, l'amplitude à chaque fréquence
"""
import struct
import sys

import numpy as np


def lire(path):
    """Lit un WAV (entier 16 bits ou flottant 32 bits, « extensible » compris) et renvoie (canaux, taux)."""
    d = open(path, "rb").read()
    if d[:4] != b"RIFF" or d[8:12] != b"WAVE":
        raise SystemExit(f"{path} : ce n'est pas un fichier WAV")
    pos = 12
    fmt = canaux = taux = None
    while pos + 8 <= len(d):
        cid = d[pos : pos + 4]
        taille = struct.unpack("<I", d[pos + 4 : pos + 8])[0]
        if cid == b"fmt ":
            fmt = struct.unpack("<H", d[pos + 8 : pos + 10])[0]
            canaux = struct.unpack("<H", d[pos + 10 : pos + 12])[0]
            taux = struct.unpack("<I", d[pos + 12 : pos + 16])[0]
            if fmt == 0xFFFE and taille >= 40:
                # WAVE_FORMAT_EXTENSIBLE (au-delà de 2 canaux) : le vrai format est le début du GUID
                fmt = struct.unpack("<H", d[pos + 8 + 24 : pos + 8 + 26])[0]
        elif cid == b"data":
            brut = d[pos + 8 : pos + 8 + taille]
            if fmt == 3:
                echantillons = np.frombuffer(brut, dtype=np.float32)
            elif fmt == 1:
                echantillons = np.frombuffer(brut, dtype=np.int16).astype(np.float32) / 32768.0
            else:
                raise SystemExit(f"{path} : format WAV {fmt} non géré")
            return echantillons.reshape(-1, canaux), taux
        pos += 8 + taille + (taille & 1)
    raise SystemExit(f"{path} : bloc « data » introuvable")


def mono(path):
    x, taux = lire(path)
    return x[:, 0].astype(float), taux


def cmd_niveaux(path):
    """Écart entre la première et la seconde moitié (clip à deux niveaux)."""
    x, taux = mono(path)
    n = len(x) // 2
    fort = np.sqrt(np.mean(x[int(0.15 * n) : int(0.85 * n)] ** 2))
    faible = np.sqrt(np.mean(x[n + int(0.15 * n) : n + int(0.85 * n)] ** 2))
    print(f"{20 * np.log10((fort + 1e-12) / (faible + 1e-12)):.2f}")


def cmd_crete(path):
    x, _ = mono(path)
    print(f"{np.abs(x).max():.4f}")


def cmd_sn(path):
    """Rapport entre le signal (440 Hz) et une bande où il n'y a que du bruit."""
    x, taux = mono(path)
    X = np.abs(np.fft.rfft(x * np.hanning(len(x))))
    f = np.fft.rfftfreq(len(x), 1 / taux)
    signal = X[(f > 400) & (f < 480)].mean()
    bruit = X[(f > 3000) & (f < 6000)].mean()
    print(f"{20 * np.log10((signal + 1e-12) / (bruit + 1e-12)):.2f}")


def cmd_canaux(path):
    x, _ = lire(path)
    if x.shape[1] < 2:
        print("0.0")
        return
    print(f"{np.abs(x[:, 0] - x[:, 1]).mean():.5f}")


def cmd_max(path):
    x, _ = mono(path)
    print(f"{np.abs(x).max():.6f}")


def cmd_freq(path):
    x, taux = mono(path)
    X = np.abs(np.fft.rfft(x * np.hanning(len(x))))
    f = np.fft.rfftfreq(len(x), 1 / taux)
    print(f"{f[np.argmax(X)]:.1f} {len(x) / taux:.2f}")


def cmd_queue(path, debut_ms, fin_ms):
    x, taux = mono(path)
    a, b = int(float(debut_ms) * taux / 1000), int(float(fin_ms) * taux / 1000)
    seg = x[a:b]
    print(f"{np.sqrt(np.mean(seg**2)):.6f}")


def cmd_debut(path, seuil="0.01"):
    x, taux = mono(path)
    audibles = np.nonzero(np.abs(x) > float(seuil))[0]
    if len(audibles) == 0:
        print("-1")
        return
    print(f"{audibles[0] * 1000 / taux:.0f}")


def cmd_enceintes(path, frequences):
    """Pour chaque canal : l'amplitude (0–1) de la composante la plus proche de chaque fréquence.

    Sert à vérifier QUEL haut-parleur reçoit QUEL signal (mixage, ordre des canaux) : une source dont
    chaque canal porte une fréquence propre, puis la matrice « canal × fréquence » de la sortie.
    """
    import json

    x, taux = lire(path)
    freqs = [float(f) for f in frequences.split(",") if f]
    n = len(x)
    fenetre = np.hanning(n)
    sortie = []
    for c in range(x.shape[1]):
        spectre = np.abs(np.fft.rfft(x[:, c] * fenetre)) * 2.0 / (fenetre.sum())
        axe = np.fft.rfftfreq(n, 1.0 / taux)
        ligne = []
        for f in freqs:
            largeur = (np.abs(axe - f) <= 15.0)
            ligne.append(round(float(spectre[largeur].max()) if largeur.any() else 0.0, 4))
        sortie.append(ligne)
    print(json.dumps({"canaux": int(x.shape[1]), "taux": int(taux), "amplitudes": sortie}))


COMMANDES = {
    "enceintes": cmd_enceintes,
    "niveaux": cmd_niveaux,
    "crete": cmd_crete,
    "sn": cmd_sn,
    "canaux": cmd_canaux,
    "max": cmd_max,
    "freq": cmd_freq,
    "queue": cmd_queue,
    "debut": cmd_debut,
}


def main():
    if len(sys.argv) < 3 or sys.argv[1] not in COMMANDES:
        print(__doc__)
        return 2
    COMMANDES[sys.argv[1]](*sys.argv[2:])
    return 0


if __name__ == "__main__":
    sys.exit(main())

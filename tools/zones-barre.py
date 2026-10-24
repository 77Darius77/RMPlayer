#!/usr/bin/env python3
"""Détecte les zones cliquables de la barre de contrôles dans une capture KPlayer.

La barre occupe les dernières lignes de la fenêtre ; les commandes se distinguent du
fond par un écart de niveau. Sortie : une ligne par zone (centre horizontal, en pixels).

Usage : zones-barre.py <capture.png> <largeur> <hauteur>
"""
import sys
import numpy as np
from PIL import Image

chemin, largeur, hauteur = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
im = np.asarray(Image.open(chemin).convert("RGB"), dtype=int)
bande = im[hauteur - 22:hauteur - 6, :largeur]
fond = np.bincount(bande.reshape(-1, 3)[:, 0]).argmax()
masque = (np.abs(bande[:, :, 0] - fond) > 8).sum(axis=0) > 6

zones, debut = [], None
for x, actif in enumerate(masque):
    if actif and debut is None:
        debut = x
    elif not actif and debut is not None:
        if x - debut >= 8:
            zones.append((debut + x - 1) // 2)
        debut = None
if debut is not None and len(masque) - debut >= 8:
    zones.append((debut + len(masque) - 1) // 2)
print("\n".join(str(z) for z in zones))

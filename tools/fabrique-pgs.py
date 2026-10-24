#!/usr/bin/env python3
"""Fabrique un flux de sous-titres PGS (.sup) minimal mais VALIDE.

Pourquoi ce générateur : il n'existe pas d'encodeur PGS dans FFmpeg (seulement un
décodeur), et les sous-titres image sont précisément ce que ce banc doit vérifier.
On écrit donc les segments à la main — le format est simple et entièrement décrit
par la spécification HDMV :

    segment = "PG"(2) pts(4, 90 kHz) dts(4) type(1) taille(3) charge

Un « display set » complet = PCS (composition) + WDS (fenêtre) + PDS (palette)
+ ODS (objet), soit l'image à afficher et où. Une composition sans objet efface.

Les images produites sont volontairement reconnaissables : un rectangle plein dont
on connaît la position et la couleur exactes (voir `--motif`).

Usage :
  fabrique-pgs.py sortie.sup [options]
    --largeur / --hauteur        dimensions vidéo (défaut 640x360)
    --x / --y / --l / --h        rectangle du sous-titre (défaut 100,250,200,40)
    --couleur blanche|jaune|rouge
    --debut / --fin              instants en secondes (défaut 1.0 et 4.0)
    --numero                     numéro de composition (défaut 0)
"""
import argparse
import struct
import sys

PCS, WDS, PDS, ODS, END = 0x16, 0x17, 0x14, 0x15, 0x80

# Couleurs en YCbCr (BT.601, plage limitée) telles que PGS les transporte.
COULEURS = {
    "blanche": (235, 128, 128),
    # (Y, Cr, Cb) — BT.601, plage limitée : jaune = Cb bas, Cr élevé.
    "jaune": (219, 149, 16),
    "rouge": (81, 240, 90),
}


def segment(type_segment, charge, pts_ms=0):
    """Un segment PGS, en-tête compris.

    En-tête de 13 octets : « PG », PTS (4), DTS (4), type (1) et **taille sur
    2 octets**. C'est la forme attendue par le démultiplexeur `sup` de FFmpeg
    (mesuré : une taille sur 3 octets, conforme à la définition du segment mais
    pas au conteneur `.sup`, fait échouer la lecture avec « Picture size 0x0 »).
    """
    pts = int(pts_ms * 90)  # millisecondes → 90 kHz
    return (
        b"PG"
        + struct.pack(">II", pts, pts)
        + bytes([type_segment])
        + len(charge).to_bytes(2, "big")
        + charge
    )


def rle_run(index, longueur):
    """Un bloc RLE PGS, tel que le décodeur de FFmpeg le relit.

    Règle (lue dans libavcodec/pgssubdec.c, `decode_rle`) :
      - un octet non nul C                      → 1 pixel de couleur C ;
      - 0x00 puis un octet de drapeaux F        → bloc :
            longueur = F & 0x3f, et si F & 0x40 la longueur devient
            (longueur << 8) + octet suivant ;
            couleur  = F & 0x80 ? octet suivant : 0 (transparent) ;
      - 0x00 0x00                               → fin de ligne.
    """
    if index != 0 and longueur == 1:
        return bytes([index])
    if longueur <= 0x3F:
        drapeaux = longueur | (0x80 if index else 0x00)
        return bytes([0x00, drapeaux]) + (bytes([index]) if index else b"")
    drapeaux = 0x40 | ((longueur >> 8) & 0x3F) | (0x80 if index else 0x00)
    return (
        bytes([0x00, drapeaux, longueur & 0xFF])
        + (bytes([index]) if index else b"")
    )


def rle_plein(largeur, hauteur, index):
    """RLE PGS d'un rectangle uniforme : un bloc par ligne, puis fin de ligne."""
    ligne = rle_run(index, largeur) + b"\x00\x00"
    return ligne * hauteur


def display_set(args, composition, avec_objet, pts_ms):
    """Assemble un display set complet (ou son effacement)."""
    vid_w, vid_h = args.largeur, args.hauteur
    objets = 1 if avec_objet else 0

    # --- PCS : ce qui est affiché, et où ---
    pcs = struct.pack(">HH", vid_w, vid_h)
    pcs += bytes([0x03])                     # 25 i/s (sans effet sur le rendu)
    pcs += struct.pack(">H", composition)
    # État de composition : deux bits de poids fort (0x80 = début d'époque,
    # 0x40 = point d'acquisition, 0x00 = normal).
    pcs += bytes([0x80 if composition == 0 else 0x00])
    pcs += bytes([0x00])                     # pas de mise à jour de palette seule
    pcs += bytes([0x00])                     # identifiant de palette
    pcs += bytes([objets])
    if avec_objet:
        pcs += struct.pack(">H", 0)          # identifiant d'objet
        pcs += bytes([0x00])                 # fenêtre 0
        pcs += bytes([0x00])                 # drapeaux (non rogné)
        pcs += struct.pack(">HH", args.x, args.y)

    # --- WDS : la fenêtre qui contient l'objet ---
    wds = bytes([1])
    wds += bytes([0x00])                     # identifiant
    wds += struct.pack(">HHHH", args.x, args.y, args.l, args.h)

    # --- PDS : la palette (index 0 transparent, index 1 la couleur) ---
    pds = bytes([0x00, 0x00])                # palette 0, version 0
    y, cr, cb = COULEURS[args.couleur]
    pds += bytes([0x00, 16, 128, 128, 0])    # index 0 : noir transparent
    pds += bytes([0x01, y, cr, cb, 255])     # index 1 : la couleur, opaque

    # --- ODS : l'image elle-même ---
    donnees = rle_plein(args.l, args.h, 1)
    ods = struct.pack(">H", 0)               # identifiant d'objet
    ods += bytes([0x00])                     # version
    ods += bytes([0xC0])                     # premier et dernier fragment
    corps = struct.pack(">HH", args.l, args.h) + donnees
    ods += len(corps).to_bytes(3, "big")
    ods += corps

    sortie = segment(PCS, pcs, pts_ms) + segment(WDS, wds, pts_ms)
    if avec_objet:
        sortie += segment(PDS, pds, pts_ms) + segment(ODS, ods, pts_ms)
    sortie += segment(END, b"", pts_ms)
    return sortie


def main():
    p = argparse.ArgumentParser(description="Flux PGS minimal pour vérification")
    p.add_argument("sortie")
    p.add_argument("--largeur", type=int, default=640)
    p.add_argument("--hauteur", type=int, default=360)
    p.add_argument("--x", type=int, default=100)
    p.add_argument("--y", type=int, default=250)
    p.add_argument("--l", type=int, default=200)
    p.add_argument("--h", type=int, default=40)
    p.add_argument("--couleur", default="blanche", choices=sorted(COULEURS))
    p.add_argument("--debut", type=float, default=1.0)
    p.add_argument("--fin", type=float, default=4.0)
    p.add_argument("--numero", type=int, default=0)
    args = p.parse_args()

    flux = display_set(args, args.numero, True, args.debut * 1000)
    # Effacement : une composition sans objet.
    flux += display_set(args, args.numero + 1, False, args.fin * 1000)

    with open(args.sortie, "wb") as f:
        f.write(flux)
    sys.stderr.write(
        "écrit %s : %d octets, rectangle %dx%d en (%d,%d), %s, %.2f→%.2f s\n"
        % (
            args.sortie, len(flux), args.l, args.h, args.x, args.y,
            args.couleur, args.debut, args.fin,
        )
    )


if __name__ == "__main__":
    main()

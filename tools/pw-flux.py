#!/usr/bin/env python3
"""Interroge PipeWire pour les bancs : où va le flux audio d'un processus ?

Pourquoi : « le menu propose le bon périphérique » ne prouve rien ; ce qui compte, c'est la sortie
sur laquelle le flux du lecteur est RÉELLEMENT relié. Ces mesures lisent `pw-dump`, c'est-à-dire le
graphe tel que PipeWire le tient, pas ce que le lecteur croit avoir demandé.

  sink <pid>      nom (node.name) de la sortie sur laquelle le flux du processus est relié
  etat <pid>      état du nœud du flux (running / idle / suspended…) ; « aucun » s'il n'existe pas
  defaut          nom de la sortie par défaut du système
"""
import json
import subprocess
import sys


def dump():
    return json.loads(subprocess.run(["pw-dump"], capture_output=True, check=True).stdout)


def flux_du_processus(objets, pid):
    clients = {
        o["id"]
        for o in objets
        if o["type"].endswith("Client")
        and str(o["info"]["props"].get("application.process.id")) == str(pid)
    }
    for o in objets:
        p = o.get("info", {}).get("props", {})
        if p.get("media.class") == "Stream/Output/Audio" and p.get("client.id") in clients:
            return o
    return None


def sink_du_flux(objets, flux):
    noms = {
        o["id"]: o["info"]["props"]["node.name"]
        for o in objets
        if o.get("info", {}).get("props", {}).get("media.class") == "Audio/Sink"
    }
    for o in objets:
        if o["type"].endswith("Link") and o["info"].get("output-node-id") == flux["id"]:
            return noms.get(o["info"].get("input-node-id"))
    return None


def defaut(objets):
    for o in objets:
        if o["type"].endswith("Metadata"):
            for m in o.get("metadata", []):
                if m.get("key") == "default.audio.sink":
                    return m["value"]["name"]
    return None


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    objets = dump()
    cmd = sys.argv[1]
    if cmd == "defaut":
        print(defaut(objets) or "inconnu")
        return
    flux = flux_du_processus(objets, sys.argv[2])
    if cmd == "sink":
        print((sink_du_flux(objets, flux) if flux else None) or "aucun")
    elif cmd == "etat":
        print(flux["info"]["state"] if flux else "aucun")
    else:
        raise SystemExit(__doc__)


main()

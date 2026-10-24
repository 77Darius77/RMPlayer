#!/usr/bin/env python3
"""Détecte les appels `ctx.*` faits depuis l'intérieur d'une fermeture qui
détient déjà le verrou du contexte egui — c'est-à-dire les interblocages
garantis.

Pourquoi cet outil existe
-------------------------
Dans egui 0.33, `Context::input()` et `Context::viewport()` prennent le verrou
**exclusif** du contexte (`Context::write`). Le verrou (parking_lot::RwLock,
via epaint::mutex) n'est PAS réentrant : tout appel `ctx.*` fait depuis la
fermeture redemande le verrou et fige définitivement l'application.
Constaté le 2026-10-16 avec `Q` (`send_viewport_cmd` → `viewport_id` → verrou
partagé) : fenêtre gelée, KWin la force à quitter (SIGABRT), ce qui ressemble
à un plantage alors que c'est un interblocage.

Règle : dans ces fermetures, on ne fait que *lire* le paramètre fourni (`i`)
et on note la demande dans un champ de `App` (par ex. `pending_viewport_cmds`),
vidé ensuite hors de la fermeture.

Usage : tools/check-ctx-reentrance.py [src/]   (code de sortie 1 si problème)
"""
import re
import sys
from pathlib import Path

# Fermetures qui détiennent le verrou exclusif du contexte egui.
CLOSURES = (
    "input",
    "viewport",
    "input_for",
    "viewport_for",
)
CALL = re.compile(r"\bctx\.([a-z_][a-z0-9_]*)\s*\(")


def strip_noise(src: str) -> str:
    """Remplace commentaires et chaînes par des espaces (mêmes positions)."""
    out = list(src)
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            while i < n and src[i] != "\n":
                out[i] = " "
                i += 1
        elif c == "/" and i + 1 < n and src[i + 1] == "*":
            while i < n and not (src[i] == "*" and i + 1 < n and src[i + 1] == "/"):
                if src[i] != "\n":
                    out[i] = " "
                i += 1
            if i < n:
                out[i] = out[min(i + 1, n - 1)] = " "
                i += 2
        elif c == '"':
            i += 1
            while i < n and src[i] != '"':
                if src[i] == "\\":
                    out[i] = " "
                    i += 1
                if i < n and src[i] != "\n":
                    out[i] = " "
                i += 1
            i += 1
        else:
            i += 1
    return "".join(out)


def closure_body(src: str, open_paren: int) -> int:
    """Index de fin de la fermeture dont la parenthèse ouvrante est `open_paren`."""
    depth = 0
    i = open_paren
    while i < len(src):
        if src[i] == "(":
            depth += 1
        elif src[i] == ")":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return len(src)


def main() -> int:
    root = Path(sys.argv[1] if len(sys.argv) > 1 else "src")
    problems = []
    checked = 0
    for path in sorted(root.rglob("*.rs")):
        raw = path.read_text(encoding="utf-8", errors="replace")
        src = strip_noise(raw)
        for name in CLOSURES:
            for m in re.finditer(r"\bctx\." + name + r"\s*\(", src):
                para = m.end() - 1
                end = closure_body(src, para)
                body = src[para:end]
                checked += 1
                for call in CALL.finditer(body):
                    line = raw[: para + call.start()].count("\n") + 1
                    col = para + call.start() - raw.rfind("\n", 0, para + call.start())
                    problems.append(
                        f"{path}:{line}:{col}: ctx.{call.group(1)}() appelé depuis "
                        f"la fermeture ctx.{name}(...) — interblocage garanti"
                    )
    print(f"fermetures de saisie examinées : {checked}")
    if problems:
        print("\n".join(problems))
        print(f"\n{len(problems)} problème(s).")
        return 1
    print("aucun appel ctx.* depuis une fermeture de saisie ✔")
    return 0


if __name__ == "__main__":
    sys.exit(main())

# ADR-002 — `core/` synchrone, sans GLib

Statut : accepté · 2026-10-19

## Contexte

`core` sert la CLI (sans GTK) et la GUI. Transcode, le projet frère, pilote ses processus avec
`gio::Subprocess`, ce qui lierait `core` à GLib.

## Décision

- `core::helper::Helper` utilise `std::process` en **bloquant** : `open` et `render` attendent
  la réponse. Pas de timeout (un gros PSD est légitimement lent) ; un assistant mort est
  détecté par la fin de son stdout.
- stderr de l'assistant lu en continu par un thread (une sortie bavarde ne bloque jamais le
  tube) ; les 16 derniers Ko sont gardés pour les messages d'erreur et le rapport (T10).
- Erreurs typées : `PythonMissing` (python du venv absent), `PsdToolsMissing`
  (`No module named 'psd_tools'` sur stderr), `HelperCrashed(stderr)`, `Psd(message)`.
- Le venv est trouvé comme dans `scripts/setup-venv.sh` : `DDSNAP_VENV`, sinon
  `$XDG_DATA_HOME/ddsnap/venv`, sinon `~/.local/share/ddsnap/venv`.
- La GUI appellera `core` depuis un thread de travail et reviendra sur la boucle GLib
  (`glib::MainContext` / `spawn_future_local`).

## Conséquences

- `core` se teste sans GTK ni boucle d'événements.
- Écart assumé avec Transcode ; l'annulation d'un rendu en cours se fait en tuant l'assistant
  (pas d'annulation fine).

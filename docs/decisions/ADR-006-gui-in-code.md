# ADR-006 — GUI construite en code, hors de la bibliothèque, travail sur un thread

Statut : accepté · 2026-10-19 · met en œuvre ADR-002 et ADR-005

## Contexte

Le plan prévoyait des fichiers Blueprint (`data/ui/*.blp`) compilés en GResource par
`build.rs`. Ddsnap se compile sur le portable de l'utilisateur (`install.sh`). Blueprint y
ajouterait `blueprint-compiler` aux paquets requis et une étape de build de plus, pour une
seule fenêtre. La CLI, elle, ne doit jamais dépendre de GTK.

## Décisions

1. **Widgets construits en Rust** (`gtk::…::builder()`), sans Blueprint ni GResource, sans
   sous-classe GObject. `build.rs` ne fait que lier libstdc++ (ADR-003).
2. **`src/ui/` hors de la bibliothèque.** `src/bin/ddsnap-gui.rs` l'inclut avec
   `#[path = "../ui/mod.rs"] mod ui;`. `lib.rs` n'expose que `core`, donc la CLI et les tests
   ne lient pas GTK. Les deux binaires se compilent quand même ensemble : `cargo build` exige
   `libgtk-4-dev` et `libadwaita-1-dev`.
3. **Travail lourd sur un thread** (`ui/worker.rs`) : `std::thread` + `mpsc`, relevé toutes les
   50 ms par `glib::timeout_add_local` (pas d'async, pas de nouvelle dépendance). Messages :
   étape (barre de progression et journal), fraction, calque extrait.
4. **Un seul assistant psd-tools par fenêtre** (`Session` : assistant + PSD ouvert). Une tâche
   le prend, le rend à la fin ; s'il est mort (`Helper::is_alive`), il est jeté et la tâche
   suivante en relance un et rouvre le PSD (`Session::ensure`). Les demandes faites pendant une
   tâche attendent dans une file ; une extraction d'aperçu en cours est interrompue au calque
   suivant puis reprise, un export va toujours au bout.
5. **Aperçu** (ADR-005) : nœuds GSK assemblés directement (`TextureNode`, `BlendNode`,
   `OpacityNode`, `MaskNode` pour l'écrêtage, `ColorMatrixNode` pour les vues RVB / Alpha),
   rendus par `renderer.render_texture` dans un `gtk::Picture`. L'assistant extrait un calque
   par requête (`layer`), pas tous d'un coup comme écrit dans ADR-005, pour montrer la
   progression et pouvoir s'arrêter entre deux calques.

## Pièges

- Les gestionnaires de signaux tiennent des références **faibles** à `Window`. Sans une
  référence forte, tout est libéré après `present()` et plus aucun bouton ne répond (bug vécu,
  `21d3822`). La fenêtre garde son `Rc` jusqu'à `close-request`.
- `AdwApplicationWindow` possède déjà le préfixe d'actions `win.` : les actions s'ajoutent à
  la fenêtre (`add_action`), pas dans un second groupe `win`.
- Le code qui modifie lui-même des widgets lève `updating` pour que leurs gestionnaires ne
  réécrivent pas la recette.

## Conséquences

- Paquets requis : `libgtk-4-dev libadwaita-1-dev`, sans `blueprint-compiler`.
- `ui/window.rs` est long (mise en page comprise) ; à découper par panneau seulement s'il
  devient difficile à modifier.
- Aucun test automatique de l'interface. Contrôle minimal : lancer `ddsnap-gui` sous
  `gtk4-broadwayd` avec un PSD généré par `tests/fixtures/make_psd.py` et lire le journal.

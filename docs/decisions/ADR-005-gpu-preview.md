# ADR-005 — Aperçu fusionné sur le GPU, composite Photoshop pour l'export « tel quel »

Statut : accepté par l'utilisateur · 2026-10-19

## Contexte

Mesures (ADR-001) : psd-tools compose ≈ 1 s par calque en 4096² ; le vrai PSD de l'utilisateur
(`EXT_Main.psd`, 8192×2956, 36 calques) s'exporte en **51,6 s** avec un pic de **4,3 Go**
sur le Surface Book (15 Go). L'aperçu en direct < 2 s (critère 2) est impossible avec
psd-tools à chaque case cochée.

## Décisions

1. **Aperçu en direct sur le GPU, via GTK** (aucune dépendance ajoutée).
   - À l'ouverture, l'assistant extrait **une fois** chaque calque à l'échelle de l'aperçu
     (commande `layers`) : pixels du calque seul (effets, masque et smart object appliqués par
     psd-tools), position, opacité, mode de fusion.
   - La GUI empile ces textures dans un `GtkSnapshot` (`GskBlendNode`, opacité, groupes
     imbriqués) : cocher un calque ne refait que la fusion GPU (quelques ms).
   - Approximation assumée : masques d'écrêtage et calques de réglage peuvent différer de
     Photoshop. L'« Aperçu compressé » et l'export restent sur psd-tools / composite Photoshop,
     donc **le fichier écrit est toujours exact**.
   - Option « Sans aperçu » (demandée) : n'extrait rien ; arbre + export seulement.
   - `core::render_queue` du plan initial n'est plus nécessaire : la fusion GPU est
     instantanée et le rendu exact n'est lancé que par un bouton.
2. **Composite Photoshop pour l'export « tel quel ».** Quand une sortie demande exactement la
   visibilité enregistrée dans le PSD, l'export lit l'image aplatie que Photoshop a écrite
   (`Maximiser la compatibilité`) : quelques secondes, identique à Photoshop. Sans cette image,
   ou si la visibilité diffère, psd-tools compose (plus lent, signalé dans la progression).
3. **Plugin PhotoCraft** : toujours hors MVP, la CLI reste la frontière (revu plus tard).

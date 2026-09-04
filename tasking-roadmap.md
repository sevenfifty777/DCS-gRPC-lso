# Tasking & roadmap — DCS-gRPC-lso

> Historique synthétique des chantiers, décisions ouvertes et bugs connus. Complète
> [AGENTS.md](AGENTS.md) (état courant du code) et [primer.md](primer.md) (vulgarisation). Mis à
> jour le 4 septembre 2026 en fusionnant `.ignore/tasking-v3.md`, `.ignore/roadmap-post-test-20260904.md`
> et les autres notes de session, avant leur suppression du dépôt.

## À faire en priorité (P0)

Aucun bug P0 confirmé ouvert actuellement. Le seul point P0 identifié (remise de gaz en survol
classée `Bolter`) a été corrigé — voir "Décisions déjà prises" ci-dessous et l'historique des
sessions.

## À investiguer — pas encore un bug confirmé (P1)

- **Déclin de l'AoA corrigée dans le groove** (~1,5–3° entre ¾ NM et ¼ NM, systématique sur 3-4
  F-14 dans un test avec vent nul). Le vent nul dans cette mission exclut un artefact de la
  correction vent introduite par le commit `108ffa1` ; reste à savoir si c'est un comportement de
  pilotage IA réel ou un effet de la décomposition en repère avion. Nécessite un outil de
  comparaison ancienne/nouvelle formule sur les mêmes données.
- **Délai fixe touchdown → fichiers complets** (~10,2 s, ±0,03 s observé) — trop régulier pour être
  du temps de calcul variable ; sent le délai intentionnel dans le pipeline (confirmation finale du
  brin ? cadence de lecture du buffer côté fork ?). Source exacte non identifiée.

## Optimisations à considérer (P2)

- **Boucle de redémarrage pendant une pause mission** (observé : 6 redémarrages de génération en
  ~30 s, un par timeout de session ID). Sans gravité mais bruyant ; un backoff progressif
  réduirait le bruit de logs lors d'une pause prolongée.
- **Vent nul non exercé en test** : la mécanique de capture (2 requêtes `AtmosphereService.GetWind`
  à l'entrée du groove) a été validée mécaniquement, mais le calcul de correction lui-même (la
  soustraction du vecteur vent) n'a jamais été vraiment exercé avec du vent réel. À refaire avec du
  vent configuré dans la mission de test.
- **Purger le ring Lua sur `after_sequence` acquitté** : déjà implémenté (voir AGENTS.md,
  section DCS-gRPC), à revalider en usage prolongé.
- **`telemetryObservationErrors` bornée à 128 entrées** : déjà implémenté, à revalider.

## Hors-scope confirmé (rappel volontaire)

- **Robustesse multi-recoveries simultanées** : la dernière mission de test était trop
  minimaliste (8 unités, jamais deux avions en approche en même temps) pour être testée.
- **Cadence adaptative pré-groove (100/200 ms)** : toujours en attente d'une décision explicite,
  indépendante de tout test. `lso.exe cadence-ab` (voir AGENTS.md) est l'outil de mesure prévu pour
  instruire cette décision, pas la décision elle-même. Exécuté une fois sur un corpus de 9 rapports
  live : aucun changement de grade, mais 2 occurrences où la porte ¾ NM devient invalide (strides 2
  et 4) sur l'enregistrement le plus récent, déjà sous-échantillonné côté capture — confirme
  empiriquement le risque déjà identifié (un sous-échantillonnage supplémentaire cumulé casse la
  porte la plus proche du groove).
- **Nouveau message `RecoveryTelemetry` compact** : changement de protocole additif, deux dépôts,
  régénération des stubs — chantier séparé, non entamé.

## Note opérationnelle (procédure de test, pas un bug LSO)

Arrêter une tâche de surveillance qui encapsule `lso.exe` dans son propre pipeline tue aussi
`lso.exe` — a coûté l'enregistrement d'un trap en cours d'approche lors d'un test. À éviter dans une
future session : lancer le process et le monitoring séparément.

## Décisions déjà prises (ne pas rouvrir sans nouvelle preuve)

Ces points étaient listés "ouverts" dans les versions antérieures de la roadmap (`tasking-v3.md`) ;
ils sont désormais tranchés et implémentés :

- **Batch vs stream source** : batch incrémental `after_sequence`, implémenté et actif par défaut
  (`--position-source buffered`).
- **Hook groupé ou indépendant** : resté totalement indépendant du snapshot avion/carrier.
- **Partage carrier en multi-recovery** : clé `(session, generation, carrier_id, sequence)`, jamais
  de cache périmé dans une preuve de gate.
- **Format de causes multiples** : `cause` reste l'alias primaire, `causes: { primary, secondary[] }`
  ajouté en JSON, migration SQLite 6 additive.
- **AoA réelle via draw argument (`aircraft_draw_argument`/`DrawArgumentObservation`)** : abandonné
  après recherche sérieuse infructueuse sur les vraies valeurs de draw argument par module (sources
  web bloquées/ambiguës). Remplacé par une correction du vent sur l'approximation géométrique
  existante (commit `108ffa1`), documentée `PROJECT-DERIVED`. Ne pas relancer cette piste sans accès
  DCS live pour un balayage empirique de `UnitService.GetDrawArgumentValue`.
- **Remise de gaz en survol classée `Bolter` au lieu de `WO?`** : `crossed_deck_threshold` dans
  [src/track.rs](src/track.rs) ne marque désormais le franchissement du seuil de pont (`x` passant
  de positif à négatif) que si l'avion est réellement proche du niveau du pont au moment du
  franchissement (`DECK_CROSSING_ALT_CAP_FT = 50 ft`, relatif au pont, crosse comprise), et non
  simplement dans les 500/300 ft déjà utilisés ailleurs (`in_approach`, entrée en groove) — ces
  seuils-là restent trop hauts pour exclure le cas confirmé en live (franchissement à ~460 ft). Deux
  tests de régression couvrent le franchissement haute altitude (→ `WaveoffUnknown`) et le
  franchissement bas niveau (→ toujours `Bolter`, garde-fou contre une sur-correction). Non encore
  revalidé en mission live — voir "Décisions encore ouvertes" et "Scénarios de validation Phase 6".

## Décisions encore ouvertes (mission/serveur nécessaires)

- Le collecteur `--positions-only` atteint-il réellement p99 <300 ms en usage prolongé et
  multi-recovery ?
- Cadence de lecture LSO adaptative 100/200 ms hors zone de notation : à valider par A/B
  (`lso.exe cadence-ab`) avant toute promotion — voir ci-dessus, jamais décidée à ce jour.
- Validation live DCS-gRPC de la ligne de version serveur réellement déployée avant tout repin des
  stubs vers une révision Git immuable.
- Refaire les scénarios de validation Phase 6 (voir plus bas) sur une mission moins minimaliste :
  câbles 1–4, pattern puis finale, longue passe, simultané, respawn, reconnect/session, gaps, sans
  ACMI et V/STOL.
- Revalider en mission live le correctif de la remise de gaz en survol (`DECK_CROSSING_ALT_CAP_FT`) :
  corrigé et testé unitairement, mais jamais encore rejoué sur un vrai enregistrement CVN-72 pour
  confirmer que les 8 tentatives de F18-4-1 du test du 4 septembre 2026 seraient bien reclassées.
- Revalider sur données live les nouveaux seuils de notation (`PERSISTENCE_MIN_CONSECUTIVE_SAMPLES`,
  `OSCILLATION_MIN_SWING_DEG`/`OSCILLATION_MIN_REVERSALS`) : corrigés et testés unitairement
  seulement, jamais rejoués sur un corpus de rapports live pour vérifier qu'ils ne masquent pas de
  vrais écarts ni ne déclenchent de faux positifs sur des approches réelles.
- `cargo audit` reste à exécuter dès qu'un outil autorisé est disponible localement (la CI
  l'exécute déjà).

## Pistes d'amélioration de la notation (état après la session du 4 septembre 2026, notation II)

Comparaison de la méthode actuelle ("trois gates + pire écart") à la doctrine LSO américaine des
années 2000 (NATOPS 00-80T-104). Conclusion : le principe fonctionne mais reste plus pauvre que ce
que les données déjà enregistrées permettraient, sans changement de DCS ni du fork. Déjà codé :
trajectoire continue, facteur de tendance, pondération temporelle des 150 derniers mètres, vent
persisté en contexte (voir AGENTS.md, grading v4), et depuis cette session : garde de persistance
(A.1), détection de surcorrection (A.4/OC), taux de descente et angle de gîte en contexte. Détail
technique dans [docs/GRADING_REFERENCE.md](docs/GRADING_REFERENCE.md).

1. **NC vs statut neutre dédié — pas de changement de code, ré-examiné et jugé déjà couvert.**
   `grading_availability` (`available` / `unavailable_technical` / `unavailable_event_outcome`)
   distingue déjà, en JSON, une `NC` d'origine télémétrique d'une `NC` d'origine événementielle ;
   introduire un nouveau statut *pass_grade* dédié à "neutre par construction" serait une décision
   de nommage/UX (quel libellé, quel impact sur le greenie board existant) plutôt qu'un correctif de
   clarté à faible risque — hors scope d'une implémentation automatique sans validation produit.
   Laissé ouvert pour une session dédiée si le besoin se confirme.
2. **AoA réellement lue depuis le cockpit** (`aircraft_draw_argument`) : voir "décisions déjà
   prises" ci-dessus — abandonné pour l'instant faute de source fiable, pas un refus définitif.
3. **Taux de descente (sink rate) — codé, contexte uniquement.** `TrajectoryDeviation` porte
   désormais `sink_rate_mps` (m/s, positif = descend, calculé en `d(alt)/dt` entre échantillons
   continus consécutifs) et `alt_m`. Jamais noté (voir AGENTS.md : le taux de descente reste
   explicitement hors notation) — NATOPS `TMRD` reste un signal affiché, pas un code appliqué.
4. **Détection de surcorrection (`OC` — overcontrolled) — codée.** Un compteur d'inversions de
   signe sur GS et lineup (`OSCILLATION_MIN_REVERSALS = 2` inversions d'au moins
   `OSCILLATION_MIN_SWING_DEG = 0.3°` chacune, sur la même fenêtre de 4 s que le facteur de
   tendance A.2) plafonne désormais une passe par ailleurs `Ok` à `(OK)`, exactement comme le
   facteur de tendance mais sensible à une pente nette proche de zéro. Seuils `PROJECT-DERIVED`,
   pas encore éprouvés sur données live.
5. **Angle de gîte (bank) aux corrections — codé, contexte uniquement.** `TrajectoryDeviation`
   porte désormais `bank_deg` (roll brut de la télémétrie) ; `datums[].roll_deg` porte la même
   donnée sur toute la trajectoire pour que le rejeu `cadence-ab` reste fidèle. Jamais noté — pas de
   seuil NATOPS "attitude/wing" appliqué, affichage/diagnostic seulement pour l'instant.
6. **Filtre de durée minimale pour l'amplitude continue (A.1) — codé.** Un échantillon isolé
   au-dessus du seuil (`*_SLIGHT`) ne compte plus seul : il faut au moins
   `PERSISTENCE_MIN_CONSECUTIVE_SAMPLES = 2` échantillons consécutifs dans la même direction.
   Jamais appliqué au Cut (`GS_CUT_LOW_DEG`) ni à la pondération de fin d'approche (A.3), qui
   restent volontairement sensibles à un seul échantillon dangereux. Toujours non confirmé par des
   données live (aucun faux positif n'avait été observé le 4 septembre, ce correctif reste donc une
   robustesse préventive) — à revalider en mission.

Points 3 à 6 sont couverts par de nouveaux tests unitaires dans `src/grading.rs` et `src/track.rs`
(`cargo test --locked` : 180 réussis au moment du correctif) ; aucun n'a de preuve DCS live. Le
point 1 n'a entraîné aucune modification de code — voir ci-dessus.

Explicitement écarté (pas une piste à reprendre sans nouvelle donnée) :

- Reconstruire une "fenêtre de remise de gaz" dynamique dépendante de la puissance moteur réelle :
  DCS n'expose pas les données nécessaires.
- Entraîner un modèle statistique sur d'anciens vols : aucun corpus de grades LSO humains alignés
  sur les traces DCS du projet n'existe ou n'est raisonnablement constituable.

## Nettoyage technique déjà réalisé (pour référence, ne pas re-proposer)

Les lots suivants, un temps envisagés dans des notes de travail antérieures, sont déjà en place
dans le code actuel : tests autonomes compilables sans fixtures externes, parsing borné du câble
DCS (`1..=4` strict), calculs AoA/vitesse protégés contre les vecteurs dégénérés, registre de
tâches par session/génération avec annulation propre au respawn, migrations SQLite idempotentes
avec propagation des erreurs réelles, rendu PNG déporté en `spawn_blocking`, observabilité
(gaps/skew/latences/percentiles bornés) sans impact sur les décisions métier, `cargo fmt`/`clippy
-D warnings` propres. Toute nouvelle relecture doit repartir du code actuel, pas de cette liste.

## Scénarios de validation Phase 6 (checklist de non-régression avant toute promotion majeure)

- CATOBAR nominal avec câbles DCS 1 à 4 ;
- passage pattern puis finale, sans verrouillage du câble ancien ;
- gaps artificiels de 300 ms, 1 s et livraison retardée ;
- hook timeout/error/stale et passe longue >128 s ;
- simultanéité de plusieurs recoveries et partage éventuel du carrier ;
- reconnect, changement de session et génération ;
- mode sans ACMI ;
- V/STOL séparé sans régression ;
- charge avec détecteurs actifs puis suspendus pendant groove.

## Historique des sessions (résumé, du plus ancien au plus récent)

- **1er septembre 2026** — Refonte v3 : `PositionCollector`/`EventCorrelator`/`ReportPipeline`
  extraits, `--positions-only`, causes multiples, migration SQLite 6, provenance Git/build. Deux
  relectures ("phase 1", "phase 2") ont trouvé puis corrigé : suspension des détecteurs bloquant
  une seconde recovery simultanée, `UnconfirmedArrest` écrasant les causes télémétriques, codes
  gRPC hook mal formatés, `--positions-only` encore couplé à SQLite/Discord, vocabulaire SQLite
  divergent du JSON, indicateur Git dirty incomplet, panne d'événements invalidant à tort la
  télémétrie positionnelle, respawn avec nouvel ID non nettoyé, câbles DCS non bornés à 1–4,
  publication non atomique sur Unix. Toutes ces corrections sont dans le code actuel.
- **4 septembre 2026 (buffer Lua)** — Analyse d'un test local (trois traps `complete/green`, 20 Hz,
  zéro perte) : purge du ring Lua sur `after_sequence` acquitté, `diagnostics` limité à 1×/s,
  `telemetryObservationErrors` bornée à 128 entrées. Côté LSO : sous-échantillonnage des `datums`
  JSON hors fenêtre `scoring_relevant` (1/4), sans toucher à la zone de notation.
- **4 septembre 2026 (notation et cadence)** — Chantier A (A.1 trajectoire continue, A.2 facteur de
  tendance, A.3 pondération temporelle des 150 derniers mètres, A.4 vérification que cause/causes
  séparaient déjà "télémétrie dégradée" du reste, A.5 vent persisté en JSON) et B.2 (commande
  `cadence-ab`) implémentés et validés (fmt/clippy/test/build verts à chaque commit). A.6 (AoA
  réelle via draw argument) non traité à ce stade faute de source fiable.
- **4 septembre 2026 (AoA/vent, commit `108ffa1`)** — A.6 tranché autrement : au lieu du draw
  argument introuvable, l'AoA de `datums`/`pattern_datums` est corrigée du vecteur vent (deux
  appels `AtmosphereService.GetWind` à l'entrée du groove, interpolation par altitude), avec
  `wind_reference_established` comme évidence de repli.
- **4 septembre 2026 (test live CVN-72, 4×F-14 + 4×F-18 IA)** — 14 rapports produits, 0 crash, 0
  erreur télémétrie. Résultats détaillés en tête de ce document (P0/P1/P2 ci-dessus).
- **4 septembre 2026 (ménage documentaire)** — Consolidation de tous les `.md` épars
  (`.agents/agents.md`, `.ignore/*.md`, documents `docs/*.md` obsolètes explicitement marqués comme
  superseded) en trois documents racine : `AGENTS.md`, `tasking-roadmap.md` (ce document) et
  `primer.md`. Les fichiers sources ont été supprimés pour éviter toute divergence future ; leur
  contenu utile est repris ci-dessus.
- **4 septembre 2026 (nouvelles pistes de notation post-test)** — Suite au test live CVN-72,
  identification de quatre pistes supplémentaires non codées pour le calcul de la note (taux de
  descente, détection de surcorrection, angle de gîte, filtre de durée minimale sur l'amplitude
  continue d'A.1) — voir "Pistes d'amélioration de la notation" ci-dessus, points 3 à 6.
- **4 septembre 2026 (correctif remise de gaz en survol)** — Le seul bug P0 confirmé de la session
  précédente (franchissement du seuil de pont sans plafond d'altitude, classant à tort une remise de
  gaz haute en `Bolter`) est corrigé : ajout de `DECK_CROSSING_ALT_CAP_FT` (50 ft, relatif au pont)
  dans [src/track.rs](src/track.rs) et de deux tests de régression (franchissement haut → `WaveoffUnknown`,
  franchissement bas → toujours `Bolter`). `cargo test --locked` (175 réussis), `cargo fmt --check`
  et `cargo clippy --locked --all-targets -- -D warnings` propres au moment du correctif. Non encore
  revalidé sur un enregistrement live — voir "Décisions encore ouvertes".
- **4 septembre 2026 (pistes de notation post-test, notation II)** — Points 3 à 6 de la liste
  "Pistes d'amélioration de la notation" implémentés : garde de persistance A.1
  (`PERSISTENCE_MIN_CONSECUTIVE_SAMPLES`), détection de surcorrection A.4/OC
  (`OSCILLATION_MIN_SWING_DEG`/`OSCILLATION_MIN_REVERSALS`), taux de descente et angle de gîte
  portés en contexte sur `TrajectoryDeviation`/`datums` (jamais notés). Point 1 (NC vs statut
  neutre) réexaminé et jugé déjà couvert par `grading_availability` — aucun changement de code.
  Point 2 (AoA cockpit) reste abandonné, inchangé. `src/grading.rs`, `src/track.rs` et
  `src/commands/cadence_ab.rs` modifiés ; `cargo test --locked` (180 réussis), `cargo fmt --check`
  et `cargo clippy --locked --all-targets -- -D warnings` propres. Plusieurs tests existants de
  `src/grading.rs` construits sur un échantillon de trajectoire unique ont dû être adaptés à deux
  échantillons consécutifs pour rester cohérents avec le nouveau garde de persistance. Aucun de ces
  seuils n'a de preuve DCS live — voir "Décisions encore ouvertes".

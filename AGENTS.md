# AGENTS.md — Contexte machine-first pour DCS-gRPC-lso

> Document de continuité, à tenir à jour à chaque changement significatif de code ou de contrat.
> Dépôt `E:\DCS stuffs\Initiative ESG\DCS-gRPC-lso`, branche `feature/refonte-v3-lua-buffer`.
> Dernier commit : HEAD `6917223` ("Nouvelle détection de groove-entry"). Working tree
> actuellement **non propre** : achèvement des tranches développables du P0 sur la réduction des
> `Grading unavailable` et mise à jour documentaire associée. La restitution graduée, la
> couverture des observations source invalides et des gates, le flux événementiel de session et
> `ApproachOnly` sont implémentés ; le rendu du pattern sépare les circuits antérieurs de la branche
> finale. La correction de segmentation des waveoffs issue du
> second corpus humain F-14B(U) du 7 septembre 2026 est incluse dans HEAD. L'entrée en groove Case I
> CATOBAR reconnaît le dernier virage depuis bâbord sous 600 ft puis confirme le roll-out sur une
> gîte absolue au plus égale à 10° et une progression inbound maintenues 0,75 s ; lineup, route et
> tendance restent des diagnostics non bloquants. Les observations unité
> invalides conservent séquence, horodatage source, côté, statut et réception afin d'être attribuées
> au vrai segment temporel ; la santé sépare cadence de capture, âge de livraison et pertes de
> séquence ; une preuve cinématique d'arrestation structurée est exposée à titre diagnostique sans
> lever `unconfirmed_arrest` ; la timeline de crosse est un ring récent de 2 048 entrées ; et
> `lso.exe groove-ab` compare en lecture seule l'ancien et le nouveau détecteur sur des JSON v3 ;
> un LQM DCS `GRADE:WO` établit désormais l'issue de la tentative courante afin que son départ ferme
> la track avant le circuit suivant. La sentinelle vent `180°/0,0 m/s` intermittente de `GetWind`
> (confirmée venir du moteur DCS, pas du fork) déclenche désormais un retry borné puis un repli sur
> la probe haute cohérente, pour la référence AoA comme pour la requête de fin de tentative. Le
> grading CASE I CATOBAR `project-derived-v7` classe les épisodes GS/lineup/AoA par zone,
> persistance et qualité de correction ; V/STOL et les Cuts de sécurité restent séparés. Crate
> `lso` 0.5.0 (lignée d'intégration, postérieure à la fois à `astra-review` 0.4.0 et à la refonte
> 0.2.0), Rust 2021 ; les changements postérieurs au tag `0.2.0` sont sous `Unreleased` dans
> [CHANGES.md](CHANGES.md). Le dashboard web embarqué (`src/web.rs`, `--web-port`) a été retiré :
> le greenie board est la page LSO du DCS Web Dashboard, qui lit `lso.db` directement (WAL).

Pour un résumé humain, vulgarisé, du fonctionnement du module : voir [primer.md](primer.md).
Pour la roadmap, les décisions ouvertes et les bugs connus non résolus : voir
[tasking-roadmap.md](tasking-roadmap.md). Pour l'historique synthétique de toutes les versions : voir
[CHANGES.md](CHANGES.md).

## Règles de maintenance des documents markdown racine

Ce dépôt maintient quatre documents markdown à la racine selon des règles strictes, à appliquer
automatiquement par le modèle à la fin de toute session de travail ayant modifié le code, le
contrat de données ou la conception du projet de façon significative — sans attendre une demande
explicite de l'utilisateur :

- **[AGENTS.md](AGENTS.md)** (ce fichier) : machine-first, technique, exhaustif sur l'état courant.
  Reconstruit/édité pour ne refléter que l'état factuel actuel du code et des contrats — jamais de
  trace d'anciennes versions, de formulation "auparavant X, maintenant Y" une fois la transition
  terminée, ni de journal de session. En cas de changement significatif : mettre à jour ce fichier
  en fin de session, supprimer toute information devenue obsolète plutôt que l'annoter comme
  périmée.
- **[tasking-roadmap.md](tasking-roadmap.md)** : roadmap technique et humaine — idées, choix
  ouverts, arbitrages à faire, bugs confirmés mais non corrigés, corrections faites mais non encore
  revalidées en mission live. Dès qu'un chantier qui y était listé est terminé **et** revalidé (ou
  dès qu'un point est explicitement tranché sans besoin de revalidation), son entrée est supprimée
  d'ici ; le fait qu'il a été fait, et l'essentiel technique du "pourquoi", migre dans
  [CHANGES.md](CHANGES.md) (et dans AGENTS.md si ça touche l'état durable du système). Ne doit
  contenir, à tout instant, que des points **encore en suspens** — jamais un historique de ce qui a
  déjà été résolu.
- **[primer.md](primer.md)** : human-first, vulgarisé pour un public non-développeur, présente le
  projet dans son ensemble et la logique de notation en détail mais sans jargon de programmation.
  Mis à jour après tout changement de comportement observable par un pilote/LSO humain (nouvelle
  règle de notation, nouveau champ de rapport visible, correctif changeant un verdict). Purgé des
  informations obsolètes à chaque mise à jour : un bug corrigé n'y reste documenté que si comprendre
  qu'il a existé aide à comprendre la règle actuelle (sinon, le retirer) ; jamais deux versions
  contradictoires d'une même règle.
- **[CHANGES.md](CHANGES.md)** : technique, historique, format changelog (Added/Changed/Fixed/
  Security, section `Unreleased` depuis le tag `0.2.0`). Mis à jour à chaque fois que les documents
  ci-dessus perdent une information devenue obsolète suite à un changement significatif : la version
  synthétique de ce qui a été fait (quoi, pourquoi en une ligne, quel fichier de code) y est ajoutée
  au même moment, jamais reconstruite plus tard de mémoire. Ne pas y remettre le détail narratif de
  session (dates de test, discussions de conception) qui appartient à l'historique Git lui-même — un
  `git log`/`git show` sur le commit correspondant fait foi pour ce niveau de détail.

Ordre d'application typique en fin de session : coder + tester -> mettre à jour AGENTS.md avec le
nouvel état factuel -> ajouter l'entrée correspondante dans CHANGES.md -> retirer de
tasking-roadmap.md ce qui est désormais résolu (ou ajouter ce qui reste ouvert) -> mettre à jour
primer.md si le comportement observable par un utilisateur a changé.

Aucun document technique sous `docs/` ne subsiste indépendamment de ce document : le déploiement/
rollback, la spécification de notation détaillée et les contrats de données sont documentés
directement dans ce fichier plus bas. La vulgarisation sans jargon de la logique de notation vit
dans [primer.md](primer.md).

## Règles de vérité

Ordre des sources : résultat fraîchement exécuté sur le worktree > code courant > artefact live
identifié > décision/contrat > cible de refonte > documentation historique/hypothèse. Ne jamais
conclure depuis une checklist ou un ancien résultat.

Préserver les modifications utilisateur. Ne jamais annoncer de validation DCS live, de
compatibilité fonctionnelle DCS-gRPC hors de la ligne mineure déjà validée à l'exécution, ou d'une
amélioration du p99 sans preuve nouvelle. Ne pas modifier le fork DCS-gRPC/Lua sans demande
explicite. Ne pas relever le seuil de 300 ms, interpoler une coupure proche de 900 ms, fabriquer
une trajectoire, traiter l'ACMI LSO comme source indépendante, exposer un UCID hors SQLite/API
privée, ou déplacer silencieusement le métier dans Lua.

## État exécutable vérifié

Baseline exécutée avant modification sur le HEAD propre `8e1228a` :

- `cargo test --locked --no-fail-fast` : **230 réussis, 0 échec** (228 tests du binaire + 2 tests
  de provenance de build) ;
- `cargo fmt --check` et `cargo clippy --locked --all-targets -- -D warnings` propres.

Sur le HEAD `e62506d`, qui inclut la correction de segmentation des waveoffs :

- `cargo test --locked --no-fail-fast` : **246 réussis, 0 échec** (244 tests du binaire + 2 tests
  de provenance) ;
- `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings` et `git diff --check`
  propres ;
- `lso.exe groove-ab .ignore/tests-20260907-human` traite les 12 rapports sans les modifier et
  retrouve une entrée avec le nouveau détecteur sur les 12 ; deux durées restent `N/A` faute de
  touchdown DCS exploitable.

Sur le working tree courant :

- `cargo test --locked --no-fail-fast` : **292 réussis, 0 échec** (290 tests du binaire + 2 tests
  de provenance) ;
- `cargo fmt --check` et `cargo clippy --locked --all-targets -- -D warnings` propres ;
- les tests du grading v7 couvrent les quatre zones et leurs délais post-pic, corrections
  bonne/moyenne/mauvaise, aggravation initiale puis correction rapide récompensée,
  stagnation/aggravation/oscillation, bruit isolé, axes multiples
  sans cumul, AoA F/A-18C/F-14/T-45, AoA non fiable sans pénalité et priorité du Cut ;
- les tests P0 couvrent gate ponctuelle absente mais bracketée par la trajectoire continue à
  `<=300 ms`, refus au-delà, finale `ApproachOnly` avec points conservés, rejeu filtré du journal
  événementiel, LQM tardif et reconnexion explicitement comptée ;
- les tests déterministes du détecteur Case I couvrent sortie nominale, undershoot, overshoot,
  lineup/route/corrections imparfaits, durée de roll-out insuffisante ou suffisante, outbound, gap
  `>300 ms`, retour du temps source, faux initial/break/vent arrière/straight-in, waveoff puis
  nouvelle branche, V/STOL inchangé et parité du détecteur pur avec `groove-ab` ;
- la géométrie lineup près du pont couvre une sévérité latérale constante dans les 150 derniers
  mètres, l'exposition de l'écart brut et le maintien de la sanction d'un vrai écart de 5 m ;
- la segmentation pure du pattern couvre un circuit simple, deux survols suivis de la finale et une
  discontinuité temporelle ; la revue visuelle d’un PNG nominal est propre ; le corpus multi-circuits
  historique ne peut pas être rerendu depuis ses JSON, qui ne sérialisent pas `pattern_datums` ;
- un test dédié couvre le repli de la référence AoA sur la probe haute quand la probe basse reste la
  sentinelle vent `180°/0,0 m/s` après retry.

Le corpus humain `.ignore/tests-20260908-1-humans`, capturé avec le binaire propre au commit
`42ebdecd499526ef9cbc42d7e7d139d37c388bd4` et les traces `-vv`, contient 28 rapports F-14B(U) de
deux pilotes : 17 T&G, un bolter, un `GRADE:WO` et neuf traps ou signatures d'arrêt. Les 28 captures
sont continues à 20 Hz (gap maximum 50 ms), sans observation source invalide ni perte de séquence
lecteur malgré 81 196 évictions internes du ring. La livraison reste tardive (p95 650-880 ms,
maximum 1 030 ms), mais 23 rapports restent `available/full`; les cinq `partial` ont exclusivement
`unconfirmed_arrest`. Le corpus valide la séparation live `GRADE:WO -> trois T&G -> WIRE# 2`, sans
contamination de LQM entre tracks. Les 28 PNG de pattern comptent 8 cas à deux branches, 18 à trois
et 2 à cinq ; la revue visuelle des deux cas à cinq branches, du WO et de traps confirme la
séparation et l'atténuation des circuits antérieurs pour CATOBAR/F-14B(U). Deux WO consécutifs,
V/STOL, pattern compacté à sa limite, overhead et waveoff sans LQM restent à couvrir. Plusieurs
paires de tracks des deux pilotes se chevauchent jusqu'à publication, dont deux T&G simultanés et
un bolter concurrent d'un T&G, sans collision d'artefact ni contamination inter-track observée.
La reconstruction de l'écart latéral brut depuis les trajectoires sérialisées identifie 320
échantillons répartis sur 12 rapports qui franchissaient `LATE_WINDOW_LU_DEG = 1,5°` uniquement
parce que le dénominateur tombait de 150 à 75 m ; ils restent sous le seuil lorsque la même
sévérité de 3,93 m est conservée sur toute la fenêtre. Cette analyse calibre le correctif logiciel,
mais sa comparaison à une nouvelle vérité LSO en mission reste ouverte.

Le rejeu hors ligne `groove-ab` du détecteur Case I courant sur les 12 + 4 + 28 JSON humains des
7 et 8 septembre retrouve une entrée sur les 44 rapports ; 20 de ces roll-outs restent à bâbord du
corridor `-0,75°`, ce qui confirme que ce diagnostic n'est pas traité comme une obligation. Les
`datums` JSON sont sous-échantillonnés avant la zone notée et ne permettent de reconstruire ni
RPC/événement/UTC/vitesse absente ; ce résultat ne constitue aucune validation mission live du
comportement courant.

La première session humaine du 7 septembre a été capturée avec un binaire issu d'un working tree dirty au
commit `b6308bc`, sans `-vv`. Elle fournit le corpus de calibration, mais ne constitue une
revalidation live ni des changements courants ni de la dernière corrélation de brin par onset. La
seconde session (`.ignore/tests-20260907-2-human`, `-vv`) a exercé le code post-corrections via un
binaire encore déclaré dirty au commit `8e1228a` : six approches réelles ont été reconstruites (deux
T&G crosse haute, un bolter, deux `GRADE:WO`, puis un trap DCS `WIRE# 2`). Les deux waveoffs et le
trap ont été fusionnés dans une seule track parce que le premier LQM n'établissait pas une issue ;
le correctif inclus dans HEAD traite désormais un `GRADE:WO` matching comme l'issue terminale de
la tentative et attend le départ géométrique normal pour la fermer. Le corpus propre du 8 septembre
valide cette séparation pour un WO suivi de trois T&G puis d'un trap `WIRE# 2`; deux WO consécutifs
restent à exercer. Voir [tasking-roadmap.md](tasking-roadmap.md) pour les autres limites.

## Produit et périmètre métier

DCS-gRPC-lso est un client Rust/Tokio externe à DCS World (`lso.exe`). Il détecte les recoveries,
collecte les transforms avion/navire, construit une trajectoire relative, capture trois gates,
corrèle les événements, estime le câble, calcule un score de projet et produit les artefacts de
débrief (JSON, PNG, ACMI, SQLite, Discord, board HTTP).

- CATOBAR : F/A-18C, F-14A, F-14B, F-14B(U), VNAO T-45 sur Nimitz/Forrestal.
- V/STOL expérimental : AV-8B NA sur LHA Tarawa uniquement (voir [VSTOL.md](VSTOL.md)).
- Humains ; IA seulement avec `--ki`.
- Multi-avions/navires/recoveries isolé par session et génération.
- `lso run` live ; `lso file` rejoue seulement un ACMI créé par LSO ; `lso cadence-ab` et
  `lso groove-ab` sont des diagnostics hors-ligne en lecture seule, jamais des rejeux live.

Le grade est un score **PROJECT-DERIVED** `project-derived-v7`, jamais une certification
USN/USMC. Puissance moteur, mouvement du pont, et auteur réel du waveoff ne sont pas notés. L'AoA
entre dans le classificateur CASE I CATOBAR uniquement quand la référence de vent est établie ; le
vent reste contextuel. Sink rate (`sink_rate_mps`)
et gîte (`bank_deg`), calculés depuis la télémétrie continue, restent contexte uniquement sur leur
amplitude/tendance générale, mais alimentent chacun un Cut dédié en cas d'excès soutenu près du pont
(voir "Gates, outcomes et câble" plus bas, qui fait foi pour la spécification exacte). Ne pas
modifier les règles CATOBAR/V/STOL sans demande dédiée.

## DCS-gRPC et dépendances

- Stubs : **pin Git immuable** sur le tag `v0.9.2` du fork `sevenfifty777/rust-server` (`Cargo.toml`
  `[dependencies.stubs] git = "https://github.com/sevenfifty777/rust-server.git"`, `tag = "v0.9.2"`,
  commit `16291fb`). Aucun checkout frère n'est requis : Cargo récupère le tag directement. Le fork
  contient `RecoveryService` (start/read/stop telemetry), et LSO en dépend directement pour la
  source bufferisée par défaut.
- `tonic = 0.13` (plus de dépendance Axum directe depuis le retrait du dashboard embarqué).
  Contrainte durable : les clients
  générés par les stubs sont paramétrés par les types de transport de **leur propre** version de
  `tonic` (`tonic::transport::Channel` passé à `MissionServiceClient`/`UnitServiceClient`/
  `WorldServiceClient`, etc.) — la version `tonic` directe de LSO doit donc rester alignée
  major/minor avec celle déclarée par `dcs-grpc-stubs`, jamais choisie indépendamment.
- Le champ protobuf `dcs.common.v0.Unit.type` est optionnel (`Option<String>`) : une unité DCS sans
  type n'est jamais retenue comme candidate recovery (`check_candidate`, `src/commands/run.rs`,
  via `unit.r#type.as_deref().and_then(AirplaneInfo::by_type)` — `None` ne matche jamais), aussi
  bien à la découverte initiale qu'aux événements `Birth` ultérieurs. Nécessaire : sans type, LSO ne
  peut pas choisir en sécurité l'offset de crosse, les positions de brins, le glideslope ou les
  dimensions du porte-avions.
- URI par défaut `http://127.0.0.1:50051`, deadline/connect timeout 2 s, retry exponentiel sans
  limite totale, intervalle max 30 s.
- Compatibilité serveur : `dcs_grpc_compatibility` est calculé à l'exécution contre le serveur
  réellement connecté ; classification `compatible_same_api_line` avec avertissement pour une
  autre version mineure de la même ligne, `incompatible` pour une autre ligne. Aucune compatibilité
  fonctionnelle au-delà de cette classification n'a été validée live avec la version actuelle des
  stubs.
- Commit serveur épinglé : `16291fbab9585e6fd8c223e21b0da12fd1a954cd` (tag `v0.9.2`). **À vérifier** :
  cette branche visait auparavant le commit non publié `c6fb3f7737f48c82601866f696d7df66ac727414`
  (workspace `dcs-grpc v0.10.0`) via le chemin local. Le passage à `v0.9.2` aligne LSO sur le tag
  publié ; si un service ou champ protobuf présent uniquement dans `c6fb3f7` est requis, il faut
  publier un nouveau tag du fork et remonter le `tag =` plutôt que revenir à un chemin local.

**Procédure pour un futur repin du fork** (quand le chemin local sera remplacé par un tag/rev Git
immuable, ou lors d'une mise à jour ultérieure de ce pin) :

1. Revoir l'historique du fork, le tag/release visé et le SHA de commit exact.
2. Comparer `Cargo.toml`, `stubs/Cargo.toml`, les changements protobuf, les dépendances de build et
   le changelog du dépôt serveur.
3. Ne changer que la valeur `tag`/`rev` de `[dependencies.stubs]`, sauf si le fork change aussi sa
   version `tonic` requise (voir contrainte ci-dessus) — auquel cas aligner la version `tonic`
   directe de LSO en même temps.
4. `cargo update -p dcs-grpc-stubs` pour régénérer le lockfile.
5. `cargo test --locked --no-fail-fast`, puis `cargo audit`.
6. Vérifier que `Cargo.lock` enregistre bien le SHA de commit complet visé.
7. Mettre à jour les références de version/commit dans [README.md](README.md) et ce document.
8. Test de fumée live contre un DCS World réel avant tout déploiement du nouveau binaire LSO —
   aucune promotion sur la seule base des tests unitaires/`cargo audit`.

## Architecture courante

```text
DCS / Mission Scripting Environment
  -> DCS-gRPC Lua + DLL (buffer circulaire RecoveryTelemetry)
  -> superviseur session/génération + inventaire initial/Birth + hub StreamEvents reconnectable
  -> registre de tâches par noms, IDs, session et génération
  -> détecteur par paire compatible
  -> record_recovery
       -> PositionCollector prioritaire
       -> EventCorrelator indépendant
       -> hook indépendant ou désactivé
       -> Track / gates / trajectoire continue / santé / grading
  -> ReportPipeline
       -> JSON create-if-absent
       -> ACMI / SQLite / PNG / Discord du producteur gagnant
```

Frontières implémentées (fichiers vérifiés présents) :

- [src/tasks/position_collector.rs](src/tasks/position_collector.rs) : deux transforms
  prioritaires, alignement et métriques ; aucune dépendance événements/hook/sorties.
- [src/tasks/event_correlator.rs](src/tasks/event_correlator.rs) : identité plane/carrier, LQM,
  touchdown, disparition et état du stream ; aucune modification de la complétude positionnelle.
- [src/tasks/event_hub.rs](src/tasks/event_hub.rs) : flux `StreamEvents` partagé par session,
  journal récent borné à 512 événements, abonnements filtrés par temps DCS et signalement des
  coupures/reconnexions.
- [src/tasks/report_pipeline.rs](src/tasks/report_pipeline.rs) : claim par `recovery_id`,
  publication atomique JSON/ACMI/PNG, rendu temporaire nettoyé et refus de remplacement.
- [src/tasks/record_recovery.rs](src/tasks/record_recovery.rs) : orchestration restante ;
  volumineux (>1700 lignes), mais les responsabilités critiques précédentes sont testables
  séparément. Le découplage complet vers `EventCorrelator`/`ReportPipeline` prévu par la refonte
  v3 n'est pas terminé — voir [tasking-roadmap.md](tasking-roadmap.md).
- [src/track.rs](src/track.rs) : géométrie, gates, crossings, hook evidence, complétude, santé,
  outcomes, trajectoire continue (`trajectory_deviations`) et grading.
- [src/commands/run.rs](src/commands/run.rs) : connexion, inventaire, respawns, registre de
  tâches, positions-only et supervision.
- [src/commands/cadence_ab.rs](src/commands/cadence_ab.rs) : commande `lso.exe cadence-ab`,
  diagnostic hors-ligne rejouant des `datums` déjà enregistrés avec un sous-échantillonnage
  artificiel, sans jamais modifier la capture live ni les fichiers d'entrée.
- [src/commands/groove_ab.rs](src/commands/groove_ab.rs) : commande `lso.exe groove-ab`, comparaison
  hors-ligne en lecture seule entre l'entrée/durée enregistrée et le détecteur Case I courant
  sur les `datums` JSON v3. Elle rejoue exactement la géométrie disponible, mais ne peut pas
  reconstruire un UTC de capture, les RPC/événements ni les vitesses absentes de `datums`.
- [src/tasks/detect_recovery_attempt.rs](src/tasks/detect_recovery_attempt.rs) : détecteur par
  paire, vérifié toutes les 2 s. Enveloppe de repérage d'un début d'approche (`is_recovery_attempt`)
  : altitude avion `<= 1100 ft`, distance au porte-avions `<= 3.5 NM` et `> 200 m` (exclut un avion
  déjà posé/en train de décoller). Volontairement sans vérification de cap pointé vers le
  porte-avions ni d'hémisphère arrière : pendant le break, l'avion est abeam avec le nez perpendiculaire
  à la BRC, et le circuit "overhead" place l'avion devant le porte-avions (initial/break) — ces deux
  vérifications auraient exclu de vraies approches légitimes.
- [src/telemetry.rs](src/telemetry.rs) : politique de fraîcheur monotone, skew, extrapolation et
  reset après coupure — voir "Contrat de télémétrie" plus bas pour les seuils exacts.
- [src/grading.rs](src/grading.rs) : score projet CATOBAR et modèle V/STOL expérimental — voir
  "Gates, outcomes et câble" plus bas.
- [src/db.rs](src/db.rs) : migrations additives et persistance privée idempotente — voir
  "Persistance et atomicité" plus bas.
- [src/metrics.rs](src/metrics.rs) : instrumentation RPC/stream/queue/IO/rendu — voir
  "Observabilité runtime" plus bas.
- [src/draw.rs](src/draw.rs) : rendu PNG (approche + pattern), déporté en `spawn_blocking`. Le
  pattern est segmenté aux retournements approche/départ confirmés (>150 m) et aux discontinuités ;
  la branche contenant l’entrée en groove, ou le touchdown/la plus récente en fallback, conserve
  les couleurs AoA, tandis que les branches antérieures sont atténuées et jamais reliées entre elles.

Le collecteur source bufferisé Lua/DCS-gRPC **est implémenté et actif par défaut**
(`--position-source buffered`) : `PositionCollector` consomme
`RecoveryService.ReadRecoveryTelemetry` par lots incrémentaux (`after_sequence`), plutôt que deux
`GetTransform` concurrents. Le polling unary (`--position-source unary`) reste disponible comme
rollback explicite. Côté fork, `Read` purge du ring les séquences acquittées par le
`after_sequence` de la lecture suivante (jamais le lot en cours) ; le bloc `diagnostics` est
renvoyé au maximum 1×/s (`recoveryTelemetry.diagnosticsIntervalSeconds`, défaut 1.0) ; la table
`telemetryObservationErrors` est bornée à 128 entrées (FIFO) — voir `docs/recovery_telemetry.md`
du fork pour le détail à jour.

## Contrat de télémétrie

Contrat `telemetry-contract-v1`, PROJECT-DERIVED :

- cadence cible 10-20 Hz selon la source, `MissedTickBehavior::Skip` ;
- skew <=100 ms : direct ;
- 100<skew<=300 ms : extrapolation de position seulement avec historique valide/frais ;
- skew >300 ms : invalide ;
- gap/source age >300 ms : warning et bracket gate invalide ;
- gap de capture >1 000 ms **à l'intérieur d'une gate/du groove noté** : `TelemetryGap`, télémétrie
  de notation incomplète, aucun point (`telemetry_gap_only_invalidates_the_scored_segment`,
  `src/track.rs`) ;
- le même gap >1 000 ms **hors du segment noté** (pattern/break avant le groove) reste un
  diagnostic conservé, sans invalider la note à lui seul ;
- watchdog sans progression source : 2 s en unary ; en bufferisé, récupération jusqu'à une seconde
  avant la rétention annoncée par le ring, bornée entre 2 et 30 s ; une reprise contiguë ne crée
  aucune perte, tandis qu'une perte explicitement rapportée reste bloquante si elle touche le segment ;
- reset de l'aligneur après erreur ;
- timestamps DCS, réception Unix et horloge monotone distincts.

La santé distingue désormais explicitement :

- `capture_gap_*` : espacement du temps de capture source ;
- `delivery_age_*` : âge du snapshot lorsqu'il est livré/lu ;
- `observed_reader_sequence_losses`/`reader_sequence_contiguous` : pertes réellement observées par
  le curseur lecteur ;
- `source_ring_capacity_evictions`/`source_ring_retention_evictions` : churn/évictions internes du
  ring, jamais assimilés seuls à des snapshots perdus.

Pour la source bufferisée, `delivery_age_ms` reste toujours une métrique de santé et un avertissement,
mais n'est jamais à lui seul une cause d'invalidité : seuls le temps de capture, le skew et les pertes
lecteur prouvées décident de la couverture. La politique unary conserve son contrôle d'âge source.

La restitution schema-v3 ajoute `assessment_scope` (`full`, `partial`, `outcome_only`, `none`),
`observed_from_distance_m`, `missing_coverage`, `points_eligible` et `fallback_source` (`project`,
`dcs_lqm`, `geometry`, `none`). Une évaluation partielle conserve le grade d'approche mesuré mais
n'accorde aucun point ; l'issue et le `WIRE#` DCS restent indépendants. Les mêmes informations sont
persistées par migrations SQLite additives et exposées par le board.

Les anciens champs `sample_gap_ms`, `gap_*`, `overflow_count`, `capacity_overflow_count` et
`lost_snapshots` restent sérialisés pour compatibilité. `sample_gap_ms`/`gap_*` gardent la valeur la
plus défavorable entre capture et livraison ; les nouveaux champs sont la source à utiliser pour
les distinguer. Les observations unité invalides du buffer conservent individuellement séquence,
`capture_tick`, `capture_time_dcs` si fini, côté avion/navire, code+nom de statut source,
`source_read_time_dcs` et `received_unix_ms`. L'attribution et l'effet sur le verdict sont décidés
dans `Track::finish()`, jamais à la réception. Avant groove et après touchdown restent diagnostiques.
Dans le segment noté, une seule séquence invalide peut rester diagnostique uniquement si les deux
séquences valides immédiatement adjacentes, leurs `capture_tick` strictement ordonnés et leurs temps
source prouvent un intervalle total `<=300 ms`, sans intersection avec le bracket réel d'une gate.
Les séries, intervalles plus longs, bornes incohérentes/manquantes et pertes touchant une gate
produisent `InvalidTelemetry`. Si `capture_time_dcs` manque, les voisins séquence/tick/temps peuvent
borner prudemment le segment sans fabriquer d'instant exact ; sinon l'attribution reste
`indeterminate_missing_source_time` et bloquante. Le temps Unix de réception n'est jamais substitué.
Chaque observation sérialise `attribution_basis`, les bornes/les séquences voisines disponibles,
`coverage_gap_ms`, `verdict_effect` et `affects_scoring`; les compteurs agrégés distinguent hors
segment, trou court couvert et trou bloquant. Cette tolérance ne crée ni position ni échantillon de
trajectoire.

Les corpus humains F-14B(U) des 7 et 8 septembre 2026 confirment une capture source à 20 Hz
(`capture_gap_max_ms = 50`), sans perte de séquence ni intervalle source manqué sur les quatre
rapports exportés. Les évictions internes du ring (2 210 à 7 224) n'y correspondent donc à aucune
perte lecteur. La livraison reste en revanche tardive (p95 690 à 830 ms, maximum 870 à 1 020 ms),
sans observation invalide dans les segments notés. Le corpus du 8 septembre confirme à nouveau ce
découplage sur 28 rapports : aucune perte lecteur malgré 81 196 évictions, p95 de livraison entre
650 et 880 ms et maximum à 1 030 ms ; la cause et l'effet d'un run sans `-vv` restent à isoler dans
[tasking-roadmap.md](tasking-roadmap.md).
Des rotations répétées du watchdog indiquent un producteur silencieux ou un canal gRPC dégradé, pas
une raison d'augmenter ce délai de 2 s sans mesure préalable.

## Gates, outcomes et câble

Sources `OFFICIAL` utilisées pour le vocabulaire/les symboles (jamais pour les formules/seuils
géométriques ni le bonus V/STOL A/B/C/D, tous `PROJECT-DERIVED`) : NAVAIR 00-80T-104 (1 mai 2009)
§6.3.2 (terminologie touch-and-go), §6.6.4 (contexte foul-deck waveoff) et §11.4.1 (symboles de
note) ; NAVAIR 00-80T-105 §6.2.4.2/6.2.4.3 (groove Case I) ; NAVAIR 00-80T-111 (15 décembre 2004)
chapitre 23 et fiches A-5/A-9 (phases V/STOL, évaluation humaine hover/cross/VL/puissance/
assiette/spot/cap relatif).

Copies PDF locales de ces manuels, déposées sous `docs/` : `docs/LSO-NATOPS-MAY09.pdf` (NAVAIR
00-80T-104), `docs/CV-NATOPS-JUL09.pdf` (NAVAIR 00-80T-105), `docs/AV8-CASE-I-II-III.pdf` (extrait
chapitre 6 de NAVAIR 00-80T-111, procédures de recovery Case I/II/III V/STOL), `docs/AV8-CVN-SPOT.pdf`
(diagramme de référence des spots de pont AV-8B — image, sans texte extractible). **Ces quatre PDF
sont des documents doctrinaux de référence, pas une spécification du projet** : ils ne servent qu'à
vérifier, ponctuellement et manuellement, qu'une règle `OFFICIAL` citée dans ce fichier correspond
bien à la doctrine réelle — jamais à en extraire de nouveaux seuils numériques pour le grading sans
une décision explicite de l'utilisateur, et jamais à les traiter comme une source de vérité sur
l'état du code (voir "Règles de vérité" plus haut).

- Gates : 3/4 NM 1 389 m, 1/2 926 m, 1/4 463 m.
- États : `Missing`, `Late`, `Invalid`, `Valid`.
- Validité : deux samples inbound encadrants, temps croissant, bracket <=300 ms, skew <=300 ms,
  phase/altitude admissibles. `GateQuality` conserve aussi les temps source des deux extrémités :
  une observation source invalide située dans ce bracket reste bloquante même si le bracket vaut
  au plus 300 ms. Si l'objet gate ponctuel manque mais que deux échantillons adjacents de la
  trajectoire continue encadrent réellement sa distance, sont valides/inbound/chronologiques et
  séparés de `<=300 ms`, leur bracket établit la couverture sans inventer de position ;
  `GateQuality.coverage_source = "continuous_trajectory_bracket"` conserve cette provenance et
  l'amplitude reste celle des échantillons continus mesurés.
- Trois gates valides et ordonnées sont obligatoires pour une note favorable **CATOBAR uniquement
  lorsque la porte 3/4 NM a été capturée après l'entrée en groove confirmée** (roll-out, voir
  ci-dessous). Quand la porte 3/4 NM (présente ou non, valide ou non) précède l'entrée en groove,
  elle ne compte ni pour l'éligibilité ni pour l'amplitude GS/lineup retenue
  (`GateDeviations::three_quarter_counts`/`all_valid`, `src/track.rs` ; `grade_from_gates`,
  `src/grading.rs`) : seules les portes 1/2 NM et 1/4 NM (plus la trajectoire continue) restent
  exigées. Sur un pattern Case I humain, la porte 3/4 NM tombe structurellement dans le virage
  base-à-finale plutôt que dans le groove (confirmé live 5 septembre 2026 soir : 7 passes sur 8,
  lineup jusqu'à -10,5° à cette porte), ce qui imposait `--` indépendamment du groove réellement
  volé ensuite. Aucune source NATOPS ne fait d'un franchissement de porte fixe une condition de
  qualification. **V/STOL garde la règle historique inconditionnelle** (pas d'entrée en groove
  confirmée par roll-out à laquelle ancrer cette relaxation) ; `lso.exe cadence-ab` aussi (ne
  rejoue jamais la détection d'entrée en groove). `PROJECT-DERIVED` : le corpus humain CATOBAR du
  8 septembre confirme que 24 entrées sur 26 surviennent après cette porte et restent évaluables,
  dont trois `(OK)` ; la revue humaine de sécurité des écarts de base/finale reste ouverte dans
  [tasking-roadmap.md](tasking-roadmap.md).
- Démarrage à l'intérieur : `Late`, jamais de donnée inventée.
- Formule au seuil `x` : `ideal_alt = base_alt + x * tan(pente_avion)` ;
  `gs_deg = atan2(observed_alt - ideal_alt, x)` ; `lineup = atan2(écart_latéral, x)` (voir
  "Near-touchdown geometry" plus bas pour le cas `x` proche de zéro).
- Entrée en groove Case I CATOBAR (`entered_groove`) : une machine à états pure
  (`CaseIGrooveDetector`, `src/track.rs`) observe d'abord le côté bâbord du pattern, arme le dernier
  virage seulement sur l'approach side (`x > 0`), sous `600 ft` relatifs, pendant une progression
  inbound et avec `|bank| > 10°`, puis confirme le roll-out lorsque `|bank| <= 10°` et l'inbound
  persistent pendant `0,75 s` de temps source continu. Un gap de capture `>300 ms` remet la
  confirmation à zéro ; un retour/non-progrès de temps ou une branche repartie de plus de 150 m
  réinitialise l'état de branche. La distance de 3/4 NM n'est plus un prérequis CATOBAR et ne peut
  pas retarder un roll-out réel détecté plus tôt. L'entrée ou le franchissement du corridor bâbord
  `-0,75°` est conservé comme indice, jamais comme obligation : undershoot restant à bâbord et
  overshoot traversant l'axe restent admissibles. Lineup `±2°`, route `±10°` et tendance lineup
  `±0,5°/s` restent dans les diagnostics legacy mais leurs booléens `*_blocks_entry` valent faux.
  La trajectoire continue commence au sample de confirmation et sérialise aussi la route sol ; ces
  écarts ne changent aucune règle de grading mais deviennent visibles dès le roll-out physique.
  `groove_entry` sérialise additivement instant de confirmation, début du roll-out, distance,
  altitude relative, lineup, bank, route, progression inbound, côté d'approche, durée/nombre de
  samples, franchissement éventuel du corridor, motif d'armement, trigger et seuils effectifs. Les
  champs legacy `stability_duration_s`/`stability_sample_count` désignent désormais la durée et le
  nombre de samples de confirmation du roll-out, pas une stabilité lineup/route/tendance. Le
  contrat RPC ne fournit pas d'ancre exacte DCS→UTC : `utc_mapping_status` l'indique et le temps
  Unix de réception reste distinct. **Case I CATOBAR uniquement** : le straight-in sans dernier
  virage observé n'active pas implicitement Case II/III ; V/STOL conserve exactement sa boîte
  historique `x <= 3/4 NM`, altitude `<=300 ft`, lineup `<=±10°`.
- Franchissement du seuil de pont (`crossed_deck_threshold`, distingue `Bolter` de `WO?`) : ne se
  déclenche que si l'avion est proche du niveau du pont au moment du franchissement
  (`DECK_CROSSING_ALT_CAP_FT = 50 ft`, relatif au pont, crosse comprise) — sinon `WaveoffUnknown`.
  Corrige un bug confirmé live où une remise de gaz haute (~460 ft au franchissement) était classée
  `Bolter`. **Insuffisant seul** : un franchissement sans jamais aucun événement DCS corrélé (`Land`/
  `RunwayTouch`) doit en plus prouver un contact réel avant de conclure `Bolter`
  (`deck_crossing_confirmed_contact`, hauteur de crosse au franchissement
  `<= DECK_CONTACT_CONFIRMATION_ALT_M = 1,0 m`, nettement plus strict que les 50 ft ci-dessus qui
  n'excluent qu'un survol *haut*) ; sinon `WaveoffUnknown`. Corrige un second bug confirmé live où
  un survol bas mais sans contact (8,7 m, largement sous les 50 ft) était classé `Bolter`. Séparément,
  un `Bolter` déjà retenu (par cette voie géométrique ou par un contact événementiel réel) est
  toujours refusé dans `Track::finish()` si le LQM DCS ouvre lui-même sur `GRADE:WO`
  (`dcs_grade_is_waveoff`) — refuser un bolter que DCS contredit directement n'invente pas un
  auteur de remise de gaz. Les deux correctifs (5 septembre 2026, soir) restent non revalidés en
  mission live — voir [tasking-roadmap.md](tasking-roadmap.md).
- Un LQM matching dont le commentaire ouvre sur `GRADE:WO` établit immédiatement
  `WaveoffUnknown` pour la tentative courante, sans prétendre connaître son initiateur. La collecte
  conserve la fin de trajectoire puis se ferme par le garde géométrique normal lorsque la distance
  au point de toucher regrossit de plus de 150 m. Le détecteur peut alors créer une nouvelle `Track`
  pour le circuit suivant : LQM, groove, gates, touchdown et franchissements de brins ne traversent
  jamais deux passes. `event_correlation.outcome_confirmed` vaut vrai pour ce waveoff explicitement
  attesté par DCS, mais reste faux pour une remise de gaz déduite de la seule géométrie. Correctif
  testé sur la séquence déterministe `GRADE:WO -> GRADE:WO -> WIRE# 2`, non encore revalidé live.
- Déclencheur commun touch-and-go/Bolter : la distance au point de toucher atteint un minimum puis
  regrossit de plus de 150 m (`src/track.rs`) — l'avion a touché puis est reparti sans s'arrêter.
  Seule la position de crosse à cet instant distingue ensuite les deux issues.
- Touch-and-go (CQ, crosse relevée volontairement) vs `Bolter` (`Track::calibrated_hook_state`,
  `src/track.rs`) : distingués par la position de crosse lue depuis DCS
  (`UnitService.GetDrawArgumentValue`, `AirplaneInfo::hook_draw_argument`), jamais déduits du
  comportement seul. Index par type : F/A-18C et VNAO T-45 = 25, F-14A/F-14B/F-14B(U) = 1305
  (`F14_HOOK_DRAW_ARGUMENT`, `src/data.rs`) ; AV-8B = aucun (pas de crosse pour ce workflow). Seuils
  d'interprétation `<= 0.2` = up, `>= 0.8` = down, avec stabilité exigée (3 échantillons/0,4 s pour
  "up", 2/0,2 s pour "down" — barre plus haute pour "up" car c'est cette conclusion qui transforme
  le verdict par défaut `Bolter` en `TouchAndGo`), uniquement sur des échantillons pris dans le
  dernier 1/4 NM et **avant le premier contact** — désormais figé sur le premier contact
  *géométrique* de la crosse (`first_hook_ground_contact_time`, hauteur de crosse `<= 0`), et non
  plus sur l'horodatage de l'événement DCS corrélé (`landing_time`), en retard de ~0,2 à 1 s sur le
  contact physique confirmé live : un échantillon pris dans cette fenêtre reflète la crosse écrasée
  sur le pont (`0,0`, soit "up") et aurait pu inventer un `TouchAndGo` sur un vrai trap sans le
  filet de sécurité du LQM DCS (5 septembre 2026, soir ; correctif non revalidé en mission live).
  **La polarité 0,2/0,8 du F/A-18C a été confirmée empiriquement en premier**
  (`HookObservation::polarity = "fa18c_zero_up_one_down_test_corpus"`) ; celle du T-45 et du
  F-14B(U) est désormais **confirmée en live dans les deux sens** (5 septembre 2026, soir,
  `"zero_up_one_down_confirmed_live_20260905"`) ; celle du F-14A/F-14B (même index de draw
  argument que le F-14B(U), mais variante non testée) reste une extension non vérifiée
  (`"assumed_zero_up_one_down_pending_live_validation"`). Sans index connu pour un type (ou lecture
  ambiguë/instable), le résultat reste `Unknown` → `Bolter` par défaut, jamais `TouchAndGo` inventé.
  L'échantillonnage de crosse est un sampler indépendant, hors du chemin critique 10-20 Hz de
  position : défaut 4 Hz / timeout 300 ms, configurable dans 2-4 Hz / 250-300 ms
  (`--hook-sampling-hz`, `--hook-timeout-ms`) ; `--legacy-inline-hook-sampling` restaure l'ancien
  échantillonnage bloquant inline comme rollback A/B (voir "Observabilité runtime").
- Géométrie 3D du point de crosse F-14 (`F14_HOOK`, `src/data.rs`, toutes variantes) : la position
  extraite via ModelViewer2 plaçait le point de crosse ~0,8-1,1 m sous le niveau réel du pont
  pendant qu'un F-14B(U) y roulait (le T-45 ne présente pas ce biais, ~0,0 m au toucher). Reçoit une
  correction verticale empirique `F14_HOOK_VERTICAL_CORRECTION_M = +1,0 m`, `PROJECT-DERIVED` et
  non une réextraction ModelViewer2 — confirmée live pour le F-14B(U) uniquement (5 septembre 2026,
  soir), à vérifier aussi sur F-14A/F-14B dès qu'un enregistrement humain existe pour ces variantes,
  et à remplacer par une vraie remesure ModelViewer2 dès que possible. Alimentait un faux positif
  de contact (voir le franchissement de seuil de pont ci-dessus) et est la cause probable la plus
  vraisemblable du biais de -1 brin observé sur l'estimation Rust du câble
  (`Track::wire_estimate_at`, voir [tasking-roadmap.md](tasking-roadmap.md)).
- Grading v7 (`project-derived-v7`), CASE I CATOBAR uniquement : la trajectoire continue du groove
  au touchdown est classée séparément sur les axes glideslope, lineup et AoA. Les zones
  `START` (>1/2 NM), `MIDDLE` (1/2–1/4 NM), `IN_CLOSE` (1/4 NM–150 m) et `RAMP` (<=150 m) portent
  respectivement les poids abstraits 1,0/1,2/1,5/2,0. Les classes brutes sont GS
  `<0,5/0,5–1,0/1,0–2,5/>=2,5°`, lineup `<1,0/1,0–2,0/2,0–3,0/>=3,0°` et AoA
  `OnSpeed/SlightlyFast|SlightlySlow/Fast|Slow`, exclusivement via `AirplaneInfo::aoa_rating` de
  `src/data.rs`; aucun niveau AoA « extrême » n'existe et l'AoA seule ne produit jamais un Cut.
  Une anomalie ordinaire isolée est ignorée ; les runs consécutifs non nuls deviennent des
  `GradingEpisode`. Le pic est le maximum de gravité puis, à égalité, l'erreur normalisée la plus
  éloignée de la cible ; une égalité complète retient le premier sample. L'analyse commence au pic.
  Une correction bonne retire un niveau, moyenne conserve le niveau, mauvaise en ajoute un, borné
  à 0..3. Elle emploie une fenêtre de tendance de 4 s à `0,075°/s`, le franchissement durable d'un
  niveau dans le délai de la zone du pic (START 3,0 s, MIDDLE 2,5 s, IN CLOSE 1,5 s, RAMP 0,75 s),
  au moins deux samples stabilisés, et l'oscillation à deux inversions d'au moins 0,3°. Au RAMP,
  une amélioration non stabilisée avant la fin exploitable est mauvaise. Pour l'AoA, l'erreur signée
  vaut zéro dans OnSpeed, négative côté Fast et positive côté Slow, avec pour amplitude la distance
  à la limite OnSpeed la plus proche, exclusivement dérivée de `AirplaneInfo::aoa_rating`. La
  gravité effective vaut niveau corrigé × poids de la zone du pic ; seul le pire épisode décide, sans
  somme ni double sanction : `<1,5 => OK`, `<3,0 => (OK)`, sinon `--`. Sans référence AoA fiable,
  les épisodes AoA restent sérialisés avec `affects_grade=false`, diagnostic explicite et gravité
  effective nulle. Toutes ces zones, coefficients, bandes et règles de correction sont
  **PROJECT-DERIVED** et attendent une validation face à des appréciations de LSO humains.
  La géométrie proche du pont garde `NEAR_TOUCHDOWN_LINEUP_REFERENCE_M = 150 m` et
  `NEAR_TOUCHDOWN_ANGLE_REFERENCE_M = 75 m`; `trajectory_deviations[].lineup_deviation_m` conserve
  l'écart signé brut. Indépendamment du classificateur, un Cut dédié (seuils
  `PROJECT-DERIVED`, **non chiffrés par NATOPS** — les deux NATOPS de référence ne codifient
  `TMRD`/`W`/`TMA`/`DLW`/`DRW` que comme codes de commentaire qualitatifs, jamais un nombre)
  sanctionne un sink rate (`SINK_RATE_CUT_MPS = 8.0 m/s`, environ le double du régime nominal de
  poser CATOBAR sans flare ~600-800 ft/min) ou une gîte (`BANK_ANGLE_CUT_DEG = 30°`, le double du
  seuil de roll-out `GROOVE_ROLLOUT_MAX_BANK_DEG`) soutenus (au moins 3 échantillons consécutifs,
  garde renforcée par rapport aux 2 échantillons habituels) à l'intérieur du 1/4 NM — même zone que
  le Cut GS. `sink_rate_mps` = `d(altitude)/dt` entre deux échantillons continus consécutifs
  (positif = descend, `0.0` pour le premier échantillon d'une passe). `trajectory_deviations` porte
  aussi `alt_m`/`bank_deg`/`sink_rate_mps` en contexte sur
  le reste de leur amplitude/tendance (non notée en dehors de ce Cut). Un échantillon n'est plus
  poussé sous
  `TRAJECTORY_MIN_DISTANCE_M` (3 m, `src/track.rs`) : les angles sont issus d'un `atan2` entre
  l'écart et une distance de référence bornée, et sous ce plancher un flare réaliste produisait un
  angle de plusieurs dizaines de degrés sans signification géométrique (bug confirmé live le 5
  septembre 2026, corrigé le même jour). Au-dessus de ce plancher mais en dessous de
  `NEAR_TOUCHDOWN_ANGLE_REFERENCE_M` (75 m, ajouté le 5 septembre 2026 suite à un second test live
  le même jour), le même effet existait sous une forme plus modérée mais tout aussi trompeuse : un
  écart quasi constant de quelques décimètres (confirmé présent dès 50 m sur les cinq appontages du
  second test, appontages propres comme Cut) grossissait mécaniquement jusqu'à des dizaines de
  degrés dans les derniers mètres, faisant retomber à `NoGrade` deux passes par ailleurs
  irréprochables — dont une notée `_OK_` par DCS lui-même. Corrigé en substituant cette distance de
  référence fixe à `x` dans le calcul vertical une fois `x` sous ce seuil
  (`trajectory_deviation_angles_deg`, partagée par `Track::next` et `replay_gate_and_trajectory`) :
  Le lineup utilise séparément 150 m, calibré sur les 28 rapports humains F-14B(U) du 8 septembre
  2026 : un écart réel et important continue de dégrader la note, seul l'arrondi normal et
  attendu près du pont n'explose plus artificiellement. Voir
  [tasking-roadmap.md](tasking-roadmap.md) pour le détail des deux bugs.

**Table de note CATOBAR** (`project-derived-v7`, classification d'épisodes `PROJECT-DERIVED`) :

| Résultat | Règle | Points |
|---|---|---:|
| `_OK_` | candidat `OK`, aucun épisode significatif, trajectoire stable, amplitudes strictes `GS +0,4/-0,3°` et `LU 0,5°`, groove 15-18 s ; jamais pour un touch-and-go | 5.0 |
| `OK` | pire gravité effective `<1,5` | 4.0 |
| `(OK)` | pire gravité effective de `1,5` inclus à `<3,0` | 3.0 |
| `--` | pire gravité effective `>=3,0` | 2.0 |
| `C` | GS strictement sous `-2,5°` à la gate 1/4 NM, ou n'importe où dans la trajectoire continue à 463 m ou en dessous ; ou sink rate soutenu (>=3 échantillons) `>= 8,0 m/s` ou gîte `>= 30°` à l'intérieur de 463 m | 0.0 |
| `B` | bolter confirmé et gates comptées valides | 2.5 |
| `WO?` | remise de gaz/go-around neutre, initiateur inconnu | aucun |
| `NC` | télémétrie insuffisante/invalide ou trap non confirmé | aucun |

**Ce que `NC` recouvre réellement** — `NC` est un seul symbole d'affichage, jamais une seule raison
interne ; `cause`/`causes` (voir "Contrats de données") distinguent déjà :

| Valeur `cause` | Signification | Catégorie |
|---|---|---|
| `telemetry_gap` | gap dans le segment noté au-delà des limites de gap/extrapolation | Télémétrie trop dégradée pour mesurer |
| `invalid_telemetry` | un sample noté a échoué à la validation (skew, temps non croissant...) | Télémétrie trop dégradée pour mesurer |
| `position_buffer_limit` | buffer de position débordé/perdu dans le segment noté | Télémétrie trop dégradée pour mesurer |
| `insufficient_gates` | télémétrie correcte, mais couverture valide/ordonnée incomplète aux gates requises (deux ou trois selon l'éligibilité CATOBAR 3/4 NM) | Structurel — rien à noter |
| `unconfirmed_arrest` | contact observé mais aucun brin DCS/LQM ne confirme un arrêt | Preuve manquante, pas une mesure |
| `unknown` (`Grading::Unknown`) | avion suivi mais jamais devenu une approche notée | Pas un problème de télémétrie du tout |

`cause` est la valeur de plus haute priorité (`Completeness::priority`, `src/track.rs`) ;
`causes.secondary` liste le reste. Un consommateur qui a besoin de cette distinction lit déjà
`cause`/`causes`, jamais le symbole `NC` seul.

CATOBAR conserve les symboles projet (`OK`, `(OK)`, `--`, `C`, `B`, `WO?`, `NC`) et `_OK_`
automatique (`is_amplitude_perfect`/`compute_catobar_assessment`, `src/grading.rs`) : uniquement
depuis une passe déjà `Ok`, sans épisode significatif, si en
plus (1) chaque porte et chaque échantillon continu reste dans `OK_PERFECT_GS_HIGH_DEG`/
`OK_PERFECT_GS_LOW_DEG` (+0,4°/-0,3°) et `OK_PERFECT_LU_ABS_DEG` (0,5°) — gardes sans pardon sur les
portes (preuves déjà validées bracket/skew), avec le même pardon anti-bruit qu'ailleurs
(`PERSISTENCE_MIN_CONSECUTIVE_SAMPLES`, 2) sur la trajectoire continue — et (2) le temps de groove
(`groove_time_secs`) est connu et tombe dans `15,0..=18,0` s (`OK_PERFECT_GROOVE_TIME_MIN_S`/
`_MAX_S`). Seuil de temps `OFFICIAL` (NAVAIR 00-80T-105 §6.2.4.3, "a 15 - 18 second groove"),
appliqué identiquement à tous les CATOBAR (F-14/F-18/T-45) malgré la pente différente du T-45,
faute de référence de vitesse d'approche par type — limite connue, non résolue. Bande d'amplitude
`PROJECT-DERIVED`, empruntée telle quelle au mod open-source MOOSE `Ops.Airboss`
(`AIRBOSS.GLE`/`AIRBOSS.LUE` `_max`/`_min`), même filiation historique que `GS_SLIGHT_*`/
`LU_SLIGHT`. Jamais lié au brin (l'ancien couplage MOOSE "brin 3 + 15-18,99 s" reste désactivé, aucun
NATOPS ne relie brin et note) ; un touch-and-go plafonne systématiquement un tier sous
`grade_from_gates`, jamais `_OK_`. `lso.exe cadence-ab` ne rejoue pas la détection de toucher, donc
`groove_time_secs` y vaut toujours `None` : `_OK_` n'apparaît jamais dans une note rejouée, seul
l'usage live peut l'émettre. Non revalidé en mission live.

Contact sans arrest confirmé : `UnconfirmedArrest`, aucun point. Le
câble DCS/LQM confirme seulement une valeur strictement comprise entre 1 et 4 ; 0, >4, overflow,
négatif ou format mal formé sont rejetés. Câble Rust et DCS restent séparés avec
provenance/divergence/confiance ; les surfaces pilote (Discord, PNG, SQLite/board) n'affichent
jamais l'estimation Rust si elle diverge du câble DCS/LQM affiché — seul le JSON complet garde les
deux valeurs pour diagnostic. `cable_estimated`/`wire_estimated` (grading) est désormais toujours
dérivé de `wire_estimation` (diagnostic JSON), calculé une seule fois dans `Track::finish()` contre
l'historique complet des franchissements de brin, plutôt que capturé séparément au moment du
toucher : les deux champs pouvaient diverger sur un même rapport selon que la corrélation
événementielle du `Land` tombait avant ou après le tick positionnel ayant ajouté le franchissement
correspondant (bug confirmé live le 5 septembre 2026, corrigé le même jour).
`wire_estimate_at` (`src/track.rs`) ne retourne plus jamais `confidence: "high"` sans un brin
confirmé par DCS (`WIRE#` du LQM parsé) : une fenêtre de corrélation serrée prouve seulement que le
franchissement a été mesuré précisément, jamais que l'avion s'est réellement arrêté à ce brin, ce qui
produisait une confiance « high » sémantiquement fausse sur un survol/bolter sans aucun accrochage
(confirmé live 5 septembre 2026, corrigé le 6 septembre 2026). Le biais de fond distinct (brin retenu
systématiquement trop haut quand l'événement DCS est en retard, la crosse étant entraînée au-delà de
son brin réel par l'élongation du câble) est désormais traité par un second correctif indépendant
(6 septembre 2026) : `wire_estimate_at` préfère le premier franchissement de brin survenu au moment
ou après le début d'une décélération horizontale soutenue détectée en continu
(`observe_horizontal_deceleration`, `WIRE_ARREST_DECELERATION_MPS2 = 5,0 m/s²` sur
`WIRE_ARREST_DECELERATION_MIN_CONSECUTIVE_SAMPLES = 2` échantillons consécutifs,
`PROJECT-DERIVED`, non chiffré NATOPS) plutôt que le dernier franchissement avant l'événement DCS ;
retombe sur ce dernier comportement dès qu'aucune décélération n'a été observée (bolter/touch-and-go/
waveoff, ou arrêt dont la signature de décélération a été perdue dans un gap). Dépend de
`plane.velocity`, que seul le chemin gRPC live remplit : `lso.exe file` (rejeu ACMI/Tacview) laisse
`velocity` à zéro comme `touchdown_horizontal_speed_mps`, donc ce proxy n'est jamais exercé par un
rejeu hors-ligne. Nouveau champ diagnostic additif `wire_estimation.arrest_deceleration_onset_time`
(JSON, jamais noté) expose l'instant détecté.

La porte de corrélation temporelle événement/brin, en amont de la sélection ci-dessus, s'appuie
désormais sur cet onset quand il est disponible, plutôt que sur le dernier franchissement brut
uniquement (correctif du 7 septembre 2026, suite à l'analyse d'un log `-vv` live) : confirmé sur une
session humaine du 6 septembre (6 recoveries, F-14B(U)) que le dernier franchissement brut se situe
systématiquement 505-1953 ms avant l'événement DCS (au-delà de `SAMPLE_GAP_WARNING_MS = 300 ms`),
bloquant toute estimation cette soirée-là y compris sur les deux arrestations confirmées `WIRE# 1` —
alors que l'onset, lui, ne se situait qu'à 180-280 ms de ce même événement. Repli inchangé sur
l'ancienne vérification (dernier franchissement vs événement) si aucun onset n'est détecté. La même
session a aussi montré que l'avion reste à vitesse quasi constante 0,9-1,0 s après le franchissement
géométrique du brin réellement accroché (le premier des quatre franchis, pas le dernier) avant
qu'une décélération mesurable n'apparaisse ; `WIRE_ARREST_ONSET_TOLERANCE_S` est donc élargi de 0,5
à 1,2 s pour continuer à sélectionner ce premier franchissement plutôt qu'un suivant. Les deux
valeurs restent calibrées sur seulement 2 échantillons (même pilote/avion) — non revalidées sur un
nouvel enregistrement live (ce correctif a été dérivé d'un log déjà capturé, pas d'un nouveau test).
Voir [tasking-roadmap.md](tasking-roadmap.md), P1, pour l'historique complet et ce qui reste à
confirmer en mission live.

`arrest_confirmation` fournit en plus une preuve cinématique structurée **diagnostique seulement**.
Elle exige simultanément : contact `Land`/`RunwayTouch` corrélé ; onset de décélération à `<=1,2 s`
du contact ; vitesse horizontale avion-navire `<=5 m/s` maintenue au moins `2,0 s` et 3 samples ;
gaps de capture `<=300 ms` ; crosse `<=3 m` au-dessus du pont ; aucun rebond, départ vers l'avant
(`>10 m/s`) ni verdict Bolter/T&G/WO contradictoire. Le JSON expose mesures, seuils, sources et motif
d'acceptation/rejet. Sans `WIRE#`, une signature acceptée est `source:
"kinematic_diagnostic"`, confiance `medium`, `verdict_effect:
"diagnostic_only_no_grading_change"` : elle ne lève jamais `UnconfirmedArrest`, ne crée aucun brin
et n'accorde aucun point. Seul un brin DCS/LQM produit `source: "dcs_lqm"`, confiance `high` et une
confirmation autoritative. La Phase B reste ouverte dans [tasking-roadmap.md](tasking-roadmap.md).

V/STOL reste AV-8B/Tarawa, spot intentionnel 7.5, formule locale expérimentale décrite dans
[VSTOL.md](VSTOL.md). Intended spot, nearest active spot et distance sont séparés. Jamais de note
favorable si incomplet. Le catalogue géométrique actif ne contient que le spot 7.5 calibré ; les
spots 7 et 8 sont des candidats explicites pour le futur, ni actifs ni notés tant qu'aucune
calibration live n'existe pour eux. La vitesse horizontale au premier contact est conservée (`first-contact
horizontal speed`) pour permettre à une future évidence de distinguer VL et RVL sans fabriquer de
seuil aujourd'hui. Un contact suivi d'un départ est normalisé en un go-around/touch-and-go neutre,
jamais en `Bolter` (concept CATOBAR non transposé tel quel). Des contacts dupliqués sont conservés
comme preuve de robustesse plutôt qu'écrasés. Aucune de ces observations ne prouve l'ordre ou la
fiabilité des événements DCS réels côté Tarawa — voir [tasking-roadmap.md](tasking-roadmap.md).

AoA dans `datums`/`pattern_datums` est corrigé du vent une fois une référence de vent établie
(deux appels `AtmosphereService.GetWind` à l'entrée du groove, interpolés par altitude), sinon
retombe sur l'approximation brute (jamais une valeur fabriquée) ; `wind_reference_established`
enregistre lequel des deux cas s'est produit. En CASE I CATOBAR, seuls les échantillons issus
d'une référence établie peuvent affecter le grade via les tables avion de `src/data.rs`. Sans cette
référence, les épisodes restent diagnostiques (`affects_grade=false`) et n'infligent aucune
pénalité. V/STOL n'utilise jamais l'AoA pour sa note.

`GetWind` peut renvoyer de façon intermittente un vecteur vent nul (`180°/0,0 m/s`) : confirmé
provenir du moteur DCS lui-même, pas d'un bug de calcul côté LSO ni du fork (`../DCS-gRPC`,
`src/rpc/atmosphere.rs`, produit fidèlement cette valeur exacte pour un vecteur DCS `(x=0, z=0)`,
hors du contrôle de ce dépôt). Cette sentinelle est mathématiquement indiscernable d'un vrai vent
calme, donc jamais rejetée à l'aveugle : chaque appel `GetWind` (probe basse d'entrée en groove et
requête de fin de tentative) est retenté une seule fois — jamais en boucle — s'il la reproduit
exactement (`query_wind_with_sentinel_retry`, `src/tasks/record_recovery.rs`). Si la probe basse
d'entrée en groove reste suspecte après ce retry, la référence AoA réutilise la valeur de la probe
haute (restée cohérente sur tout le corpus revu) pour les deux points d'interpolation ;
`wind_reference_probes.low` continue toujours d'exposer la lecture brute réellement observée,
jamais falsifiée, et `wind_reference_probes.low_reading_overridden_by_high` signale ce repli. La
requête de fin de tentative retombe sur cette même probe haute si elle est encore suspecte après
son propre retry, avec `wind_reading_is_groove_entry_fallback` pour le signaler dans le JSON ;
sans référence de groove disponible, `wind_heading_deg`/`wind_speed_mps` restent la valeur brute
(même suspecte) plutôt que d'inventer une correction. Correctif du 10 septembre 2026, non revalidé
en mission live — voir [tasking-roadmap.md](tasking-roadmap.md), P1. Les
tables `aoa_rating` par type (`src/data.rs`) viennent de documentation publique : bracket indexeur
VRS pour le F/A-18C, manuel Heatblur pour le F-14 (conversion `degrees=((units/1.0989)-3.01)`),
DisplayElectronicsUnit décompilé pour le VNAO T-45 v1.0.2 — ces tables classent la valeur AoA déjà
calculée, elles ne la mesurent pas elles-mêmes.

## Événements et complétude

`StreamEvents` est ouvert une seule fois au niveau de la session et partagé avec toutes les tracks.
Une fin propre ou une erreur déclenche une reconnexion exponentielle de 500 ms à 30 s sans arrêter
le collecteur de positions. Le hub conserve les 512 événements les plus récents avec séquence et
temps DCS ; un nouvel abonnement ne rejoue que ceux dont le temps DCS est postérieur au démarrage
de la tentative, puis `EventCorrelator` exige toujours les IDs exacts avion/navire. Après une
fermeture géométrique, une grâce bornée de 2 s récupère dans ce journal un LQM ou contact tardif
avant publication. `event_correlation` expose les nombres de coupures et de reconnexions.

Une panne ou fermeture propre de `StreamEvents` :

- ajoute `event_stream_unavailable` comme diagnostic secondaire ;
- conserve gates et métriques positionnelles ;
- n'appelle jamais `mark_telemetry_gap` ;
- laisse la collecte de positions continuer ;
- expose `event_correlation.stream_status`, détail, preuve antérieure et `outcome_confirmed` ;
- rend confiance/availability insuffisantes si l'outcome dépendant des événements n'est pas
  confirmé ;
- ne retire pas un touchdown/LQM déjà confirmé ;
- ne bloque pas un outcome confirmé indépendamment par les positions, par exemple un bolter.

Les overflows hook/event sont diagnostiques. Seule la perte du buffer positions peut produire
`BufferLimit`.

Pour un LQM matching, le premier événement appartient uniquement à la tentative courante. Un
`GRADE:WO` est une preuve de l'issue waveoff mais pas de son auteur ; il arme la clôture de la track
sur le départ géométrique. Les LQM ultérieurs ne sont des doublons que tant que cette même track
n'est pas encore fermée, jamais à travers plusieurs circuits.

Une finale avec entrée en groove ou gate significative 1/2-1/4 NM mais sans issue prouvée se termine
en `Grading::ApproachOnly`, jamais en waveoff implicite. Si sa couverture est complète, la note
d'approche et ses points restent acquis (`grading_availability = "available_approach_only"`) tandis
que l'issue et la confiance événementielle restent inconnues. Une tentative sans groove, gate
significative ni événement d'issue conserve `Grading::Unknown` et n'est publiée sur aucune surface
pilote.

## Observabilité runtime

Le mode live (`lso run`) journalise un instantané cumulatif toutes les 10 s (`src/metrics.rs`),
purement observationnel (jamais utilisé pour une décision métier) :

- appels RPC unary et RPC/s, avec latence moyenne/p50/p95/p99/max séparée par catégorie (transform
  avion, transform carrier, autres transforms, hook) et compteurs succès/erreurs/timeouts propres à
  chacune ;
- durée complète de la boucle de recovery et retard de tick Tokio (moyenne/p50/p95/p99/max) ;
- nombre de streams d'événements et de recoveries actifs ;
- `queue_high_watermark` de la file du superviseur (capacité 16) ;
- octets écrits par la publication atomique ACMI/JSON ;
- nombre de rendus PNG et temps de rendu moyen (rendu déporté en `spawn_blocking`, hors boucle
  d'échantillonnage 10-20 Hz).

Les rapports de track ajoutent par-dessus le gap/skew maximum, les compteurs invalid/warning/dropped
et la complétude des buffers bornés.

### Protocole de benchmark

Une optimisation n'est acceptée qu'après une baseline reproductible et seulement si la qualité de
la télémétrie ne régresse pas (voir "Invariants d'optimisation" ci-dessous). Le rejeu hors-ligne
valide la mécanique CPU/RAM/rendu/IO ; RPC, streams, skew et FPS/tick DCS nécessitent le corpus
serveur live.

**Baseline hors-ligne** : builder une fois, puis lancer au moins 20 exécutions propres de la même
fixture depuis un dossier de sortie vide, et relever médiane/p95/max :

```powershell
cargo build --release --locked
$samples = 1..20 | ForEach-Object {
  $process = Start-Process -FilePath .\target\release\lso.exe `
    -ArgumentList @('file','tests\recordings\wire_3_01_T45.zip.acmi') `
    -PassThru -NoNewWindow
  $process.WaitForExit()
  $process.Refresh()
  [pscustomobject]@{
    ExitCode = $process.ExitCode
    CPU_s = $process.TotalProcessorTime.TotalSeconds
    PeakWorkingSet_MB = $process.PeakWorkingSet64 / 1MB
  }
}
$samples | Measure-Object CPU_s,PeakWorkingSet_MB -Average -Minimum -Maximum
```

Consigner aussi octets d'entrée/sortie et temps mur, ainsi que le diff Git exact, la version Rust,
le profil de build, le modèle CPU, la RAM et l'OS avec chaque résultat.

**Matrice live** : rejouer des missions identiques en baseline et en candidat — (1) une recovery
Hornet/CVN, (2) une recovery AV-8B/Tarawa, (3) Hornet/CVN et AV-8B/Tarawa simultanés, (4) 40 joueurs
sur deux navires, (5) cas de stress à trois porte-avions, (6) délai gRPC délibéré, gaps 300 ms/1 s,
reconnect et rotation de mission. Capturer à la résolution de la seconde : CPU/working-set/private
bytes du process, les métriques runtime ci-dessus (RPC/s, nombre de streams), octets disque/s et
high-water marks de file/buffer, percentiles p50/p95/p99 de latence transform/skew/gap, durée de
rendu PNG, et FPS/tick de simulation DCS via l'outil serveur convenu. Au moins dix minutes de
régime stable par run, plus toutes les recoveries ; conserver logs bruts anonymisés et hash de
mission.

**A/B du hook indépendant** : rejouer deux fois la même mission (mêmes hash serveur/mission) :

```powershell
# Candidat : le hook ne peut jamais retarder les transforms
.\lso.exe -v run -o C:\LSO\ab-independent --no-acmi --hook-sampling-hz 4 --hook-timeout-ms 300

# Rollback/contrôle : ancien comportement bloquant
.\lso.exe -v run -o C:\LSO\ab-inline --no-acmi --legacy-inline-hook-sampling
```

Comparer percentiles RPC transform, percentiles de lag boucle/tick, fréquence d'échantillonnage
réelle, source age, gaps et gates valides. `--no-acmi` est volontaire : la disponibilité ACMI ne
fait pas partie de la notation nominale. Le mode indépendant n'est acceptable que s'il préserve ou
améliore la cadence positionnelle sans jamais transformer une observation de hook périmée/inconnue
en certitude.

**Déjà sûr par invariant, pas la peine de re-benchmarker** : la matrice de compatibilité stricte
(paires Cartésiennes invalides exclues) est une correctness requise, pas une réduction de cadence
issue d'un benchmark ; les buffers bornés empêchent une croissance mémoire illimitée ; l'écriture
atomique, l'insertion SQLite idempotente et le rendu en `spawn_blocking` protègent la correction
sous passes concurrentes. Tout travail supplémentaire de cache de transform, déduplication RPC ou
allocation reste conditionné à une mesure live — voir les invariants ci-dessous.

**Aucune preuve n'existe encore** pour la charge 40 joueurs/trois porte-avions ni pour l'impact sur
le FPS/tick de simulation DCS : ne jamais l'affirmer sans une mesure live nouvelle (voir "Règles de
vérité").

### Protocole de capture de preuve live

Pour qu'une session live compte comme preuve (et pas seulement comme test manuel) : un dossier par
test, nommé heure UTC + ID de scénario, contenant build/versions DCS et hash de mission, SHA-256 du
DLL déployé et de chaque fichier Lua DCS-gRPC, log DCS, log DCS-gRPC, trace LSO, JSON, ACMI optionnel
et log de l'observateur d'événements indépendant, début/fin UTC synchronisés plus session/génération
DCS, IDs d'unité avion/carrier et jeton pilote anonymisé (**jamais l'UCID**), et les actions de
scénario attendues écrites **avant** de regarder les résultats. Une correspondance jeton↔participant
ne doit être conservée que si nécessaire, jamais partagée ; aucun UCID dans fixtures, documentation,
PNG, ACMI ou tickets.

Une preuve live n'est suffisante que si des runs répétés s'accordent et que les versions/hash
capturés sont identiques d'un run à l'autre. Ne promouvoir en fixture déterministe anonymisée
qu'à cette condition, en consignant explicitement ce que la preuve démontre et ce qu'elle ne
démontre pas. Un test de robustesse logiciel (absence/duplication/délai/réordonnancement simulés)
reste une simulation conservative, jamais une preuve de comportement DCS réel.

### Invariants d'optimisation (à respecter avant toute promotion de performance)

Une optimisation n'est acceptable que si elle ne dégrade, sur aucun scénario testé, aucun des
éléments suivants : échantillons manquants/invalides/périmés, skew/gap maximum ou p95, erreurs de
corrélation ou sorties dupliquées, interaction entre recoveries, débordement de file, redémarrages
de tâche de recovery. La cadence active 10-20 Hz de la zone de notation est fixe ; un remplacement
de la découverte/pré-filtrage par `StreamUnits` (côté fork) n'est acceptable qu'après une expérience
mesurée, jamais par défaut. Un éventuel cache de transform partagé devrait être indexé par unité,
imposer sa propre fraîcheur, et ne jamais réutiliser une donnée à travers une coupure de session/
génération ou de RPC.

## Respawns et isolation

Le registre stocke session, génération, IDs et noms. Un avion ou carrier de même nom avec nouvel ID
annule toutes les tâches de l'ancienne incarnation dans la même session/génération. Les autres
unités et générations restent isolées. Les guards prioritaires sont relâchés à l'abort. La
suspension des détecteurs reste limitée au même avion (JSON : `detector_suspension_scope:
same_aircraft`), donc une autre recovery simultanée peut être découverte.

Clé de corrélation d'une recovery (supervision/registre, distincte du `recovery_id` de nommage de
fichier plus bas) : `session + génération + ID unité avion + identité interne pilote + ID unité
carrier + mode de recovery`. Identité interne : humain résolu = UCID du slot réseau occupé (jamais
le nom d'affichage) ; humain non résolu = clé locale session/unité ; IA = `ai:<session>:<unit_id>`
(`src/commands/run.rs`). Ces clés privées ne sont jamais journalisées ni placées dans un artefact
public.

Matrice de compatibilité stricte (paire sans entrée = aucun détecteur créé) : AV-8B NA + LHA Tarawa
= V/STOL ; avion à crosse supporté + géométrie Nimitz/Forrestal = arrêté ; AV-8B NA + porte-avions
arrêté, ou avion à crosse + Tarawa = incompatible. Découverte initiale et événements `Birth`
ultérieurs utilisent la même matrice, quel que soit l'ordre d'apparition avion/navire.

## Mode `--positions-only`

Conserve seulement le collecteur et le JSON diagnostic :

- aucune lecture/validation de `--discord-users`, webhook forcé à `None` ;
- pas de SQLite, dashboard, session board ou Discord ;
- pas de hook, canal hook ou client legacy ;
- pas de stream d'événements recovery ni métrique de stream correspondante ;
- pas de writer/metadata/unit RPC ACMI ;
- pas de World/theatre ni mission-time output-only ;
- pas de PNG/rendu ;
- JSON, provenance, cadence, gaps, source age, skew et latences positions conservés.

Le stream superviseur Birth/session reste nécessaire à la découverte et l'isolation des unités.

## Persistance et atomicité

`recovery_id = s<session>-g<generation>-p<plane>-c<carrier>-t<dcs_ms>`.

- Nom de fichier de base réellement écrit sur disque (`src/tasks/record_recovery.rs`) :
  `LSO-<horodatage wall-clock>-<nom pilote assaini ou "unknown">-<...>`, distinct du `recovery_id`
  logique ci-dessus — évite toute collision entre deux passes simultanées y compris à l'affichage.
- Aucun `try_exists + rename`.
- Écriture dans un temporaire du même dossier, flush + `sync_all`, puis `hard_link(temp,
  destination)`.
- Création atomique sans remplacement sur Windows et Unix.
- Temporaires et répertoires de rendu nettoyés.
- Claim process-scoped `(out_dir, recovery_id)` contre deux noms concurrents.
- Le JSON identifie le producteur gagnant ; seul lui poursuit ACMI, SQLite, rendu, session log et
  Discord.
- Un artefact existant n'est jamais remplacé.

Capacités des buffers bornés en mémoire (`src/track.rs`, `src/tasks/event_hub.rs`) :
`datums`/`pattern_datums` et ancres
valides séquence/tick/temps utilisées pour la couverture, 72 000
échantillons chacun (`MAX_TRACK_SAMPLES`), `trajectory_deviations` 4 000 (`MAX_TRAJECTORY_SAMPLES`),
preuves d'événements 256 (`MAX_EVENT_EVIDENCE`), timeline d'observations source invalides 512,
observations de hook 2 048 (`MAX_HOOK_EVIDENCE`, ~8,5 min à 4 Hz) ; file du superviseur 16
(`queue_high_watermark`, voir "Observabilité runtime") ; journal événementiel de session 512. Les
timelines hook/source-invalide sont des
rings/diagnostics explicites avec booléen et compteur de troncature ; le hook évince le plus ancien
afin de préserver prioritairement le dernier quart de nautique et le contact. Leur dépassement ne
modifie pas la complétude positionnelle. Seule une perte/débordement de positions dans le segment
noté produit `BufferLimit`.

SQLite : migrations additives 2–6 (`schema_migrations`), index unique partiel `recovery_id`,
`INSERT OR IGNORE`, base ouverte en mode WAL avec `busy_timeout` 2 s pour qu'un lecteur externe
(page LSO du DCS Web Dashboard, qui ouvre `lso.db` directement) puisse interroger le board pendant
une insertion. Discord seulement pour une nouvelle ligne. UCID uniquement SQLite, jamais
JSON/PNG/ACMI/Discord/log public. Le dashboard loopback embarqué a été retiré ; `--web-port` et
`--web-expose-ucid` restent analysés une version et arrêtent LSO avec un message explicite.

Contenu des migrations (`src/db.rs`) : 1 = table `passes` historique ; 2 = champs
recovery/session/carrier/complétude/provenance du brin + index unique de recovery ; 3 =
`points_awarded` (sépare un vrai zéro d'une absence de points) ; 4 = spot visé, spot actif le plus
proche et distance au spot visé, séparés ; 5 = gap du segment noté, santé télémétrie, confiance de
l'estimation de brin et disponibilité de la note ; 6 = causes secondaires encodées JSON (`cause`
legacy reste la colonne primaire). Au démarrage, chaque `ALTER TABLE` est précédé d'une inspection
`PRAGMA table_info(passes)` ; une erreur de migration inattendue est retournée, jamais avalée comme
une simple "colonne déjà existante". Les lignes existantes sont préservées. `points_awarded` vaut
`true` par défaut pour les lignes historiques (une note numérique existait toujours avant ce champ)
; une nouvelle ligne incomplète stocke `0` dans la colonne legacy non-nullable `grade_points` (pour
compatibilité SQL) mais `points_awarded = false` — l'API sérialise ce dernier champ, jamais un zéro
fabriqué à partir de `grade_points` seul.

Avant tout futur changement de schéma ou nettoyage destructeur : conserver des fixtures de la base
et du JSON les plus anciens réellement rencontrés en production, et tester la migration forward et
l'affichage dashboard dessus avant de merger.

## Provenance Git et baseline

`build.rs` injecte commit et dirty. Dirty = `git status --porcelain=v1 --untracked-files=no` :
modifications/suppressions/staging suivis participent ; fichiers non suivis et `target/` ne
participent pas. Tous les chemins suivis, index, HEAD et ref active déclenchent Cargo ; `build.rs`
et `build_support.rs` ont des triggers explicites. `tests/build_provenance.rs` teste la logique
déterministe.

`--baseline-manifest` refuse objet vide, clés inconnues, valeurs vides et SHA-256 mal formés. Les
erreurs affichent chemin, ligne, colonne et cause système. Manifeste typé optionnel fourni par
l'opérateur : build DCS, versions mission/module, et SHA-256 de la mission et du DLL/Lua DCS-gRPC
déployé. Une valeur inconnue reste absente plutôt que déduite.

## Diagnostics d'erreur

[src/error.rs](src/error.rs) préserve `source` et affiche message système IO, chemin
contextualisé, ligne/colonne JSON, détail SQLite, rendu, ACMI et Discord. Le point de terminaison
journalise display et chaîne debug. Les échecs SQLite/PNG/ACMI/Discord arrivent après
`Track::finish` et ne modifient jamais rétroactivement les preuves positionnelles.

## Contrats de données

JSON porte `schema_version: 9` (premier numéro de la lignée fusionnée, après le schéma 8 d'`astra-review` et le schéma 3 de la refonte ; la disposition des champs est celle de la refonte), évolution additive : aucun ancien champ supprimé/renommé ; `cause`
reste l'alias primaire ; `causes` contient primaire/secondaires ; `event_correlation`,
`wind_heading_deg`/`wind_speed_mps`, `wind_reference_established` et `trajectory_deviations` sont
des ajouts récents ; `trajectory_deviations[].lineup_deviation_m`/`alt_m`/`bank_deg`/
`sink_rate_mps` et
`datums[].roll_deg` sont des ajouts additifs plus récents encore (contexte sur leur
amplitude/tendance générale, mais `bank_deg`/`sink_rate_mps` alimentent chacun le Cut dédié
sink-rate/bank — voir "Gates, outcomes et câble") ; diagnostics possibles
`event_stream_unavailable` ; `grading_availability` peut valoir
`unavailable_event_outcome` ou `available_approach_only` ; `groove_time_secs` (ajout du 5 septembre 2026) sérialise désormais
dans le JSON la donnée déjà utilisée pour `_OK_` automatique, auparavant calculée mais visible
seulement dans l'embed Discord — un rapport live sans Discord configuré ne permettait alors aucune
vérification a posteriori de l'éligibilité `_OK_`. `wind_reference_probes` (ajout du 6 septembre
2026, absent si la référence de vent n'a jamais été établie) : les deux réponses brutes
`GetWind` (altitude/heading/speed) derrière `wind_reference_established`, purement diagnostique.
`wind_reference_probes.low_reading_overridden_by_high` et `wind_reading_is_groove_entry_fallback`
(ajouts du 10 septembre 2026, ce dernier toujours présent, `false` par défaut) signalent
respectivement quand la référence AoA et la requête de fin de tentative ont dû réutiliser la probe
haute face à la sentinelle vent `180°/0,0 m/s` — voir "Gates, outcomes et câble" plus haut.
`wire_estimation.arrest_deceleration_onset_time` (ajout du 6 septembre 2026, absent si aucune
décélération soutenue n'a été détectée) : instant de l'estimateur de brin par décélération, lui
aussi purement diagnostique. Les ajouts du 7 septembre sont `groove_entry`,
`arrest_confirmation`, les champs `capture_gap_*`/`delivery_age_*`/continuité lecteur,
`telemetry_quality.invalid_source_observations` (attribution, base de bornage, intervalle couvert et
effet explicite sur le verdict), les bornes source des brackets de gate et la sémantique de troncature de
`hook_observation`. `groove_entry` conserve ses champs schema-v3 et ajoute le début du roll-out,
l'altitude relative, la progression inbound, le côté d'approche, l'indice de corridor bâbord et le
motif d'armement ; `trajectory_deviations[].track_angle_deg` conserve additivement la route sol
post-roll-out sans entrer dans le grading. `pattern_rendering` ajoute le nombre de branches de circuit, l’index primaire
compté à partir de zéro, son motif de sélection et le nombre de branches atténuées ; ce diagnostic décrit le
rendu uniquement et n’affecte jamais le grading. Voir "Gates, outcomes et câble" et "Contrat de
télémétrie" ci-dessus.

Les ajouts P0 courants sont `Grading::ApproachOnly`,
`gate_deviations.*_quality.coverage_source = "continuous_trajectory_bracket"`,
`grading_availability = "available_approach_only"` et les compteurs
`event_correlation.unavailability_count`/`reconnection_count`. Ils restent additifs au schema-v3.
`grading_episodes` ajoute au même schema-v3 l'axe, les temps/durée, la zone et son poids, les
gravités brute/corrigée/effective, le pic (instant, zone, valeur brute, erreur normalisée et
direction/classe), les délais vers l'amélioration durable et AUCUN, le niveau et le nombre de
samples stabilisés, l'aggravation postérieure, les inversions, la qualité et la justification
déterministe de correction, `affects_grade` et le diagnostic éventuel d'AoA non fiable.
Le tableau est vide pour V/STOL.

SQLite utilise le vocabulaire snake_case du JSON. L'absence d'un nouveau champ signifie
legacy/unknown, jamais favorable. `points_awarded` (`src/db.rs`, booléen) distingue explicitement
"aucun point attribué" (outcome non éligible) d'un vrai zéro de points — ne pas confondre les deux
côté API/dashboard.

## Build

Prérequis : toolchain Rust stable (édition 2021, pas de `rust-toolchain.toml` figé) et un accès
réseau à GitHub au premier build — `Cargo.toml` résout `dcs-grpc-stubs` via un pin Git sur le tag
`v0.9.2` du fork `sevenfifty777/rust-server` (voir "DCS-gRPC et dépendances" plus haut). Aucun
checkout frère n'est nécessaire.

- Build direct : `cargo build --release` (ou sans `--release` pour un binaire debug non optimisé)
  depuis la racine du dépôt ; produit `target/release/lso.exe` (ou `target/debug/lso.exe`).
- Script Windows fourni, [build.ps1](build.ps1) (non suivi par git, local) :
  `./build.ps1` (release par défaut) ou `./build.ps1 -Configuration debug` — exécute
  `cargo build --locked` (+ `--release` sauf en debug) et affiche le chemin du binaire produit en
  cas de succès ; utile pour toujours builder avec `Cargo.lock` figé (`--locked`) sans y penser.
- Vérification complète avant commit (celle que la CI exécute, voir plus bas) :
  `cargo test --locked --no-fail-fast`, `cargo fmt --check`, `cargo clippy --locked --all-targets
  -- -D warnings`, puis `git diff --check` (espaces/fins de ligne en conflit).
- Lancer le binaire construit : voir [README.md](README.md), "Quick start", pour les options CLI
  (`lso run -o <dossier>`, variable `DCS_GRPC_API_KEY`, etc.) — non dupliqué ici.
- Rejeu hors-ligne d'un ACMI déjà produit par LSO (pas un fichier Tacview quelconque) :
  `lso.exe file <chemin.zip.acmi>`, ex. `lso.exe file tests\recordings\wire_3_01_T45.zip.acmi`
  (fixtures de test réellement présentes dans `tests/recordings/`). Une invariance live/replay est
  couverte par un test, mais le replay ne peut pas reproduire le timing réseau, l'UCID, la livraison
  d'événements DCS ni la performance serveur.
- Comparaison Case I roll-out sur un rapport ou dossier JSON v3 : `lso.exe groove-ab <chemin>`.
  Sortie TSV sur stdout avec anciennes/nouvelles entrée et durée, diagnostics au nouvel instant et
  amplitudes ajoutées avant l'ancienne entrée ; entrées jamais modifiées. La note
  `new_geometric_grade` est la note géométrique avant application des causes techniques/
  événementielles du rapport original.

## Déploiement et rollback

Runbook opérationnel (jamais exécuté en conditions réelles chronométrées à ce jour — voir
[tasking-roadmap.md](tasking-roadmap.md)). La phase 1 ne déploie ni ne modifie DCS-gRPC, le
protobuf, le DLL ou le Lua.

**Préparer un candidat** : exécuter les vérifications obligatoires (voir "Build"), builder avec
`cargo build --release --locked`, noter le SHA-256 de `lso.exe` et la révision/diff Git, copier le
candidat dans un dossier versionné sans jamais écraser le binaire actif, sauvegarder `lso.db`
(sauvegarde SQLite ou copie fichier process arrêté), conserver le binaire précédent et sa
configuration à côté du candidat. Layout suggéré :

```text
C:\LSO\releases\<revision>\lso.exe
C:\LSO\releases\previous\lso.exe
C:\LSO\data\lso.db
C:\LSO\active.txt
```

Le dossier de sortie reste partagé car les migrations sont additives — mais tester l'ancien binaire
contre une copie de la base migrée avant tout changement en production ; s'il ne sait pas lire les
colonnes additives, le pointer vers la sauvegarde pré-changement lors d'un rollback.

**Bascule** : arrêter uniquement le process/service LSO et attendre sa sortie, mettre à jour le
chemin de l'exécutable ou le pointeur de version, démarrer et vérifier sous deux minutes : connexion,
version/session serveur rapportée, aucune erreur de migration, nombre de paires strictes attendu,
page LSO du DCS Web Dashboard qui lit bien `lso.db`, log de métriques à 10 s. Rollback immédiat d'acquisition de position sans
changer de binaire : redémarrer avec `--position-source unary` ; `--legacy-inline-hook-sampling`
restaure indépendamment l'ancien chemin hook bloquant. Conserver les logs bufferisé et indépendant
avant la bascule pour garder les percentiles A/B comparables. Le candidat bufferisé exige le
DLL/Lua/protobuf DCS-gRPC exactement de la même ligne d'API que celle validée à l'exécution (voir
"DCS-gRPC et dépendances", `dcs_grpc_compatibility`) — ne jamais mélanger avec un DLL/Lua d'une autre
ligne, ni déployer seulement une moitié du paquet serveur.

**Rollback en moins de cinq minutes** : arrêter LSO, pointer vers le binaire précédent préservé, si
un test de compatibilité l'a exigé écarter la base de la tentative ratée et restaurer la sauvegarde
pré-changement (sans jamais écraser la copie ratée avant diagnostic), redémarrer l'ancien binaire,
confirmer connexion gRPC/session ID/dossier de sortie/lecture de `lso.db` par le dashboard, consigner heures UTC,
hashs de binaire et raison. Un rollback réussi se mesure à la reprise de l'enregistrement local ;
un échec Discord/PNG est secondaire et ne doit jamais retarder la restauration de la persistance
locale.

Non validé à ce jour : cette procédure n'a jamais été chronométrée sur une copie de
staging avec le vrai wrapper de service et les permissions filesystème réelles — un runbook écrit
seul ne valide pas l'objectif des cinq minutes.

## CI et sécurité

`.github/workflows/ci.yml` exécute build/test avec `--locked`, Clippy `--locked --all-targets --
-D warnings`, rustfmt, installation épinglée de `cargo-audit 0.21.2 --locked`, puis `cargo audit`.
Ne jamais modifier silencieusement `.cargo/audit.toml`.

## Fichiers de contexte associés

- [primer.md](primer.md) : explication human-first, vulgarisée, du fonctionnement complet du
  module (DCS / DCS-gRPC / DCS-gRPC-lso, les 8 étapes d'un appontage noté), et de la logique de
  notation en détail vulgarisé.
- [tasking-roadmap.md](tasking-roadmap.md) : roadmap technique, décisions ouvertes et bugs connus
  non résolus ou non revalidés — voir "Règles de maintenance des documents markdown racine"
  ci-dessus.
- [CHANGES.md](CHANGES.md) : historique changelog synthétique de toutes les versions.
- [README.md](README.md) : usage utilisateur (installation, options CLI).
- [VSTOL.md](VSTOL.md) : spécification V/STOL AV-8B/Tarawa.

Le dossier `docs/` ne contient plus de documentation markdown maintenue : son contenu utile a été
consolidé ici au fil du ménage documentaire (voir [CHANGES.md](CHANGES.md) pour la trace de ce qui a
été fusionné et retiré).

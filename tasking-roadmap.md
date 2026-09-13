# Tasking & roadmap — DCS-gRPC-lso

> Ce document ne contient que les arbitrages ouverts, les développements restant à faire, les bugs
> non corrigés et les corrections qui attendent encore une preuve live. L’état courant et les
> contrats détaillés vivent dans [AGENTS.md](AGENTS.md) ; les changements terminés vivent dans
> [CHANGES.md](CHANGES.md). Les récits complets des anciennes sessions de test restent consultables
> avec `git log`/`git show` et ne sont pas reproduits ici. Dernière purge : 10 septembre 2026.

## P0 — disponibilité de la note et restitution pilote

### Achever la réduction des `Grading unavailable`

Les tranches locales sur la restitution graduée, la couverture des observations invalides, les
gates et la résilience événementielle sont implémentées : capture et livraison bufferisées sont
distinguées, le watchdog tient compte de la rétention du ring, le vieux pattern peut être compacté
sans rendre la note indisponible, et le contrat additif expose périmètre, couverture, points et
fallback. Une séquence source invalide
isolée n'est plus bloquante si des voisins valides adjacents prouvent un trou `<=300 ms` hors de tout
bracket de gate ; séries, trous plus longs, gates touchées et bornes indéterminées restent
bloquants. Le corpus humain propre du 8 septembre confirme que la livraison tardive seule ne rend
plus la note indisponible : 23 rapports sur 28 restent `available/full` malgré un p95 de livraison
de 650 à 880 ms et un maximum de 1 030 ms, avec capture continue à 20 Hz et zéro perte lecteur.
Les cinq autres rapports sont `partial` uniquement faute de confirmation DCS de l'arrêt, jamais à
cause de la livraison. Une gate ponctuelle absente peut désormais être remplacée comme preuve de
couverture par deux échantillons continus valides, inbound, chronologiques et espacés d'au plus
300 ms ; sa provenance reste visible. `StreamEvents` est partagé au niveau de la session, se
reconnecte, journalise 512 événements et laisse deux secondes de grâce aux LQM/contacts tardifs.
Une finale reconnaissable sans issue prouvée devient `approach_only` et conserve sa note et ses
points si sa couverture est complète ; un faux départ sans groove, gate 1/2 ou 1/4 NM ni événement
d'issue reste absent des surfaces pilote. Ces changements et les observations source invalides
doivent encore être revalidés en mission DCS avant clôture globale du chantier.

Fait le 13 septembre 2026 (lignée d'intégration, porté depuis `astra-review`) :

- **Phase B de la preuve d'arrêt sans `WIRE#`.** Politique arrêtée : *une arrestation confirmée par
  une transitoire de crosse complète corrélée à un franchissement de câble, ou par la cinématique
  pont (arrêt relatif au bateau tenu deux secondes), donne un `Recovered` notable à confiance
  `medium`, sans jamais inventer de numéro de brin que la preuve ne porte pas.* Le corpus exigé
  existe désormais : les quatorze passes réelles de `tests/recordings/live_2026-09/` (T-45 et
  F-14B(U), sept traps étiquetés `WIRE#`, un bolter crosse basse, six touch-and-go crosse haute)
  sont rejouées trois fois chacune — telles quelles, sans le message DCS, puis sans le message ni
  les échantillons de crosse. Les sept traps ressortent avec le bon brin par la transitoire de
  crosse, puis `kinematic` sans brin ; aucun bolter ni touch-and-go n'est promu. La signature
  cinématique par vitesse instantanée (`arrest_confirmation.kinematic`) reste diagnostique. Un
  grade DCS sans `WIRE#` reste un fallback d'affichage, jamais des points.

Reste à développer :

1. **Revalider la transitoire de crosse en direct bufferisé.** Les timestamps de crosse drainés par
   lots arrivent avec la latence de livraison (p95 ~700 ms observé), ce qui peut faire échouer la
   corrélation à 200 ms avec le franchissement de câble ; la cinématique pont, fondée sur les
   positions, prend alors le relais. Mesurer sur une session humaine la part de traps confirmés par
   chaque source (`arrest_evidence`), et faire passer les échantillons de crosse par le moteur
   bufferisé si la transitoire échoue systématiquement.
2. **Arrêt sans aucun événement DCS.** Le `Recovered` établi par cinématique seule (ni `Land` ni
   `RunwayTouch`) et sa fenêtre de preuve bornée de 10 s (`POST_ARREST_EVIDENCE_WINDOW_S`) n'ont
   pas encore de fixture live : les quatorze passes portent toutes un `Land`. À couvrir en session.

Contraintes communes : ne pas relever les seuils 300/1 000 ms, ne pas interpoler une longue
coupure, ne pas assimiler les évictions internes du ring à une perte lecteur, et n’accorder aucun
point à un fallback incomplet.

Validation minimale : observation invalide avant/dans/après groove, gate absente mais zone
bracketée, capture commencée tard, LQM absent/tardif/réordonné, fermeture/reconnexion du stream,
faux départ pur et vraie finale `approach_only`. Vérifier JSON, SQLite, board, Discord et PNG.

### Revalider le PNG de pattern des tracks à plusieurs circuits

Le renderer segmente désormais la timeline sur les retournements approche/départ confirmés à
150 m et sur les discontinuités, choisit la branche contenant l’entrée en groove (puis le touchdown
ou la dernière branche en fallback), conserve ses couleurs AoA et atténue les autres sans les
relier. Le JSON expose le nombre de branches, leur sélection et le nombre atténué. Cette logique de
rendu ne ferme aucune track et ne modifie ni télémétrie, ni issue, ni grading.

La revalidation CATOBAR humaine du 8 septembre est acquise sur 28 captures F-14B(U) propres : huit
rendus à deux branches, dix-huit à trois branches et deux à cinq branches. La revue visuelle des
deux cas à cinq branches, ainsi que d'un `GRADE:WO` et de traps avec rollout, confirme que la branche
finale garde ses couleurs, que les circuits antérieurs sont atténués et qu'aucune liaison parasite
n'est tracée. Le défaut historique du cas `g8` est donc clos pour CATOBAR/F-14B(U).

Restent à revalider : pattern compacté jusqu'à sa limite, V/STOL, overhead et waveoff sans LQM. La
suppression complète des faux départs sans preuve reste le chantier P0 distinct ci-dessus.

### Expliquer le retard de livraison bufferisée

La capture source à 20 Hz et la continuité du lecteur ont été confirmées sur le corpus humain
`-vv` du 7 septembre, alors que la livraison restait tardive (p95 690–830 ms, max 870–1 020 ms).
Les évictions du ring ne constituent pas une perte lecteur. La cause du retard reste inconnue et a
déjà coïncidé, avant la séparation de ces métriques, avec un gap agrégé supérieur à 1 000 ms qui a
rendu une note indisponible sur un autre run.

À faire :

- mesurer CPU, mémoire, tick DCS, débit disque et files côté serveur dédié pendant un run humain ;
- comparer le même scénario avec et sans traces `-vv` ;
- isoler la lecture bufferisée dans une tâche de drain continu vers une queue locale bornée si la
  mesure montre que les autres RPC ou traitements LSO retardent encore l’acquisition ;
- ne modifier ni la politique unary ni les seuils de fraîcheur sans nouvelle preuve.

## P1 — bugs et arbitrages fonctionnels

### Bugs confirmés restant à corriger

- **Fin de génération sans jointure des recorders.** Depuis le 13 septembre, Ctrl-C ferme le
  flux fusionné du recorder, finalise la passe en cours et `lso run` attend jusqu'à 30 s les
  tâches actives. En revanche une fin de génération sur erreur fatale (perte du stream, retry de
  `run`) appelle toujours `abort_all` sur le registre sans attendre : une passe en cours à cet
  instant est perdue. Remplacer l'abort par une fin de flux + jointure bornée comme pour Ctrl-C
  (finding F01, seconde moitié).
- **Roll-out Case I armé sur un passage bas à grande vitesse.** Sur la fixture live
  `f14bu_hookup_3`, le détecteur de groove a latché une entrée à 1 095 m, lineup 18°, pendant un
  passage au-dessus du bateau à ~185 m/s (360 kt) et 215 ft, 150 s avant la vraie approche ; la
  tentative s'est fermée en `WaveoffUnknown` au franchissement de l'axe. En direct un nouveau
  recorder reprend l'approche réelle (le rejeu fait de même depuis le 13 septembre), mais ce faux
  rapport de tentative est publié. Ajouter au roll-out une garde de vitesse sol / d'altitude et
  vérifier les treize autres fixtures, sans toucher aux seuils de notation.
- **`baseline_manifest` perdu.** Un manifeste valide est accepté au démarrage mais ressort
  entièrement `null` dans `RecoveryReport`. Corriger sa propagation depuis `run.rs`. Décider aussi
  s’il doit être horodaté ou rechargé à chaque nouvelle session DCS pour suivre un changement de
  mission. Ajouter un test de bout en bout jusqu’au JSON.
- **Observabilité des opérations.** Ajouter au niveau INFO la fin de génération, la première
  reconnexion échouée, puis une ligne synthétique par insertion SQLite et publication Discord.
- **Contrat JSON incohérent.** Uniformiser `datums[].alt` actuellement clampé à zéro et
  `trajectory_deviations[].alt_m` non clampé. Renommer ou documenter additivement `aircraft_id`, qui
  est un index de type et non un ID d’unité, sans casser le schema-v3.
- **Scripts de déploiement locaux.** Rendre `run-live-buffered.ps1` autonome hors du poste de
  développement : manifeste optionnel ou livré, `$lsoRoot` réellement utilisé, webhook Discord
  sorti des scripts et lu depuis l’environnement. Vérifier l’écriture UTF-8 sur Windows PowerShell
  5.1 et PowerShell 7. Inspecter avec `sqlite3` la base `Records` qui mélange deux lignées de schéma
  avant de faire confiance au board.

### Externaliser les seuils `PROJECT-DERIVED`

Le brouillon [lso.toml](lso.toml) existe, mais le binaire ne le charge pas encore.

Développement attendu :

- `--config <chemin>` prioritaire, sinon recherche à côté de `lso.exe`, sans hot reload initial ;
- externaliser détection de groove, grading CATOBAR/V/STOL, Cut, tendance/correction et, dans des
  sections avancées, outcomes/estimation de brin ;
- garder compilés les invariants de télémétrie, distances de gates, géométries, capacités mémoire,
  isolation et règles de sécurité ;
- appliquer les défauts aux clés absentes, refuser clés inconnues, valeurs non finies et ordres
  incohérents, et échouer clairement au démarrage ;
- figer par `Track` la configuration effective complète et la sérialiser avec un SHA-256 sémantique
  canonique, indépendant des commentaires, espaces et ordre TOML ;
- faire accepter à `groove-ab` une ou plusieurs configurations pour comparer un corpus en lecture
  seule.

Tests requis : absence de fichier = comportement compilé courant bit-à-bit ; configuration partielle ;
rejet de chaque incohérence ; priorité et résolution des chemins ; hash stable et sensible à toute
valeur effective ; snapshot immuable pendant la track ; séparation CATOBAR/V/STOL. Mettre ensuite à
jour README, primer, AGENTS et CHANGES. Toute modification réelle de seuil devra être revalidée sur
corpus puis en mission selon sa portée.

### Calibration du grading et de la géométrie

- **Lineup près du pont — correction à revalider live.** Le corpus humain F-14B(U) du 8 septembre
  confirme que le plancher angulaire commun de 75 m resserrait artificiellement le seuil lineup
  tardif : `1,5°` représentait 3,93 m à 150 m mais seulement 1,96 m en close. Le code courant garde
  75 m pour le glide, normalise séparément le lineup sur 150 m dans toute la fenêtre tardive et
  sérialise l’écart brut `lineup_deviation_m`. Revalider ce candidat sur une nouvelle mission avec
  vérité LSO et vent nettement différent ; vérifier qu’un vrai écart bref supérieur à environ
  3,93 m reste bien sanctionné et comparer les verdicts `project-derived-v7` aux commentaires LQM.
- **Classificateur CASE I CATOBAR v7.** Confronter les zones START/MIDDLE/IN_CLOSE/RAMP, leurs
  poids 1,0/1,2/1,5/2,0, les bandes GS/lineup/AoA, les délais post-pic 3,0/2,5/1,5/0,75 s, la
  stabilisation sur deux samples, l'erreur AoA normalisée à OnSpeed, la tendance 4 s à 0,075°/s
  et l'oscillation 2×0,3° à des
  appréciations synchronisées de LSO humains sur F/A-18C, F-14 et T-45. Chercher explicitement les
  corrections récompensées à tort, les épisodes séparés par un bref retour dans la bande neutre,
  les surcorrections passant par zéro et les égalités entre axes/zones. Aucun coefficient ni seuil
  de cette version, y compris la remise de gravité -1/0/+1, n'est validé opérationnellement.
- **Temps de groove et `_OK_`.** Les corpus disponibles donnent des durées tantôt sous 15 s, tantôt
  au-dessus de 18 s, et aucun `_OK_` live. Déterminer si l’entrée physique par roll-out, la géométrie du
  pattern, le type avion ou la fenêtre 15–18 s expliquent cet échec. La fenêtre est `OFFICIAL`, la
  bande d’amplitude est `PROJECT-DERIVED` ; ne pas les confondre. Vérifier séparément T-45 et
  F-14/F-18 avant promotion.
- **Cut sink-rate/bank.** Quatre observations humaines `TMRDAR` suggèrent que 8,0 m/s et/ou trois
  samples consécutifs sont trop permissifs. Arbitrer seulement après un corpus avec vérité LSO,
  vents forts et corrections tardives. Vérifier que `dangerous_sink_rate_or_bank` ne s’applique
  jamais à un waveoff/survol sans toucher. La gîte de 30° reste elle aussi à valider.
- **AoA géométrique.** Construire un comparateur ancienne/nouvelle formule pour distinguer le déclin
  progressif observé dans le groove de l’artefact ponctuel à hauteur de pont. Le v6 ne pénalise
  l'AoA que lorsque la référence de vent est établie, mais cette condition ne transforme pas
  l'estimation géométrique en mesure cockpit : vérifier les faux épisodes par type avant promotion.

### Corrections implémentées qui attendent encore une preuve live suffisante

- **Référence de vent près du niveau mer.** Diagnostiqué côté `../DCS-gRPC` (`src/rpc/atmosphere.rs`) :
  `180°/0,0 m/s` est la traduction fidèle d'un vecteur vent DCS `(0,0)`, pas un bug de calcul du
  fork ni de LSO — le moteur DCS lui-même renvoie intermittemment un vecteur nul près du niveau de
  la mer, et cette sortie est mathématiquement indiscernable d'un vrai vent calme (aucun rejet
  possible à l'aveugle sur la seule valeur). Chaque appel `GetWind` (probe basse d'entrée en
  groove, requête de fin de tentative) est désormais retenté une seule fois — jamais en boucle —
  s'il reproduit exactement cette sentinelle. Si la probe basse reste suspecte après retry, la
  référence AoA réutilise la valeur de la probe haute (restée cohérente sur tout le corpus revu)
  pour les deux points d'interpolation ; la requête de fin de tentative retombe sur cette même
  probe haute si elle est encore suspecte après son propre retry. `wind_reference_probes.low`
  continue toujours d'exposer la lecture brute réellement observée (jamais falsifiée) et
  `wind_reference_probes.low_reading_overridden_by_high`/`wind_reading_is_groove_entry_fallback`
  signalent chaque repli dans le JSON. Reste à revalider en mission live qu'une nouvelle occurrence
  de la sentinelle déclenche bien ce repli (le corpus du 8 septembre, utilisé pour concevoir ce
  correctif, ne peut pas servir de revalidation puisqu'il l'a précédé).
- **Segmentation `GRADE:WO`, cas consécutif restant.** Le corpus propre du 8 septembre valide la
  fermeture et l'absence de contamination pour `WO -> trois T&G -> WIRE# 2` : chaque tentative a
  son rapport, le WO et le trap n'acceptent chacun qu'un LQM et les passes intermédiaires sont
  conservées. Il reste à exercer deux `GRADE:WO` consécutifs avant un trap (`WO -> WO -> WIRE# 2`),
  seul cas de la séquence déterministe qui n'est pas présent dans ce corpus.
- **Attribution des observations invalides.** Reproduire une livraison tardive avant/dans/après le
  segment noté et vérifier statut source, séquence, tick, bornes, effet de verdict et absence de faux
  `TimeWentBackwards`. Couvrir en particulier l'erreur isolée dans/hors groove, une série, une gate
  touchée et un temps source manquant bornable/non bornable.
- **Cut et couverture incomplète.** Le contrat gradué conserve déjà un `C` mesuré comme évaluation
  partielle au lieu de l'écraser par `NC`, mais n'attribue aucun point et marque la disponibilité
  technique incomplète. Ne promouvoir ce Cut en verdict techniquement disponible qu'après avoir
  conservé assez d'identité source dans la trajectoire pour prouver qu'une observation invalide ne
  coupe pas la série de samples qui établit le Cut et qu'aucune donnée manquante ne peut changer
  l'issue ; aucun changement de règle de grading n'est inclus dans la tranche courante.
- **Estimation Rust du brin.** Sur traps isolés et vérité indépendante, vérifier
  `arrest_deceleration_onset_time`, la corrélation via onset et la fenêtre de 1,2 s. Couvrir les
  câbles 1–4, plusieurs pilotes/types, bolter/T&G et absence de LQM ; le replay ACMI ne peuple pas
  `plane.velocity` et ne peut pas valider ce chemin.
- **Porte 3/4 NM conditionnelle, revue humaine restante.** Sur le corpus du 8 septembre, 24 des 26
  entrées en groove surviennent après la porte 3/4 NM ; ces passes restent évaluables et trois
  obtiennent `(OK)`, ce qui confirme quantitativement que la relaxation évite le blocage automatique.
  Faire encore revoir humainement les écarts de base/finale pour vérifier qu'elle ne masque pas une
  situation dangereuse. V/STOL reste inchangé.
- **Bolter et survol.** Valider le plafond 50 ft, la preuve de contact géométrique à 1,0 m et le
  refus d’un bolter contredit par `GRADE:WO`, notamment sur bolter léger, survol bas et rebond.
- **Crosse F-14.** Vérifier l’offset vertical `+1,0 m` et le gel de lecture au premier contact sur
  F-14A/F-14B ; effectuer une nouvelle mesure ModelViewer2 pour remplacer l’offset empirique.
- **Entrée Case I par roll-out.** La machine à états et le rejeu hors ligne sont implémentés, mais
  aucune mission live ne les a encore exercés. Revalider avec plusieurs pilotes et F/A-18C,
  F-14A/B/B(U), T-45, avec sorties nominales, undershoots restant à bâbord, overshoots traversant
  rapidement l'axe, route imparfaite, oscillation de gîte autour de 10°, corrections tardives,
  vent de travers, waveoff puis nouveau circuit et tracks multi-circuits. Vérifier explicitement
  absence de déclenchement sur initial, break, vent arrière, survol, départ catapulte, ailes à plat
  outbound et straight-in Case II/III. Comparer l'instant observé par un LSO humain au
  `rollout_started_at_dcs`/`timestamp_dcs`, sans desserrer 300 ms/0,75 s pour forcer une durée de
  15–18 s. V/STOL doit rester inchangé.
- **Tranche graduée du P0.** Revalider les nouveaux périmètres/fallbacks sur toutes les causes de
  complétude et sur les quatre surfaces pilote avant de considérer le contrat stabilisé.

## P2 — robustesse, performance et travaux différables

- **Armer moins de faux départs.** L’enveloppe 3,5 NM/1 100 ft capture initial, break et vent arrière
  chez humains comme IA. Étudier une tendance d’altitude décroissante et un cooldown après présence
  sur le pont, sans ajouter de filtre de cap/hémisphère qui exclurait un vrai overhead. Mesurer le
  coût et conserver le motif d’abandon en INFO.
- **Reconnexion de session.** Ajouter un backoff progressif pendant pause/rechargement de mission,
  tout en conservant une première erreur visible et une reprise rapide.
- **Sampler de crosse.** Expliquer les nombreuses annulations RPC sans `deadline_exceeded`. Ne
  l’activer seulement pendant groove/dernier quart qu’après mesure du warm-up et preuve que les
  transitions utiles ne sont pas perdues. Conserver son indépendance du collecteur position.
- **Taille des rapports.** Mesurer `datums` et le ring hook de 2 048 entrées sur de longues tracks.
  Toute compaction pré-groove doit garder transitions, erreurs et warm-up.
- **Délai touchdown → publication.** Localiser dans `record_recovery.rs` le délai d’environ 10 s du
  chemin `Arrested` contre environ 2 s pour T&G/bolter, puis réduire seulement la part qui n’est pas
  requise pour confirmer l’issue.
- **Mémoire de `DCS_server`.** Refaire une mesure longue sur mission et charge constantes dès le
  démarrage du serveur ; la hausse observée sur une mission dynamique n’est pas attribuable à LSO.
- **Cadence adaptative pré-groove.** Décision toujours ouverte. Utiliser `cadence-ab` sur un corpus
  non déjà sous-échantillonné ; une première expérience a invalidé deux portes 3/4 NM. Ne promouvoir
  aucun 100/200 ms sans A/B live.
- **Message protobuf compact `RecoveryTelemetry`.** Changement additif à mener séparément dans les
  deux dépôts avec régénération des stubs ; non entamé.
- **Charge cible.** Aucune preuve pour 40 joueurs/deux navires, trois porte-avions ou l’impact sur le
  tick/FPS DCS. Mesurer avant toute optimisation de cache, déduplication RPC ou `StreamUnits`.

## V/STOL et périmètres encore non validés

- Vérifier en direct la sémantique `Land`/`RunwayTouch`/LQM sur AV-8B/Tarawa.
- Capturer VL, RVL, rebond/double contact, T&G et go-around réels ; définir ensuite, sans transposer
  automatiquement CATOBAR, les règles encore manquantes.
- Calibrer en mission l’occupation exacte du spot 7.5. Les spots 7 et 8 restent inactifs jusqu’à une
  calibration dédiée.
- Exercer la politique de skew 100/300 ms avec un porte-avions en virage et en accélération.

## Décisions produit encore ouvertes

- **`NC` ou statut neutre dédié.** Choisir le libellé pilote et l’impact sur le greenie board pour
  un cas neutre par construction. `grading_availability`, `assessment_scope` et `cause(s)` apportent
  déjà la distinction machine ; ne pas changer `pass_grade` sans décision produit.
- **AoA cockpit par draw argument.** Ne rouvrir cette piste qu’avec un balayage empirique DCS ou une
  preuve ModelViewer pour les types concernés ; l’argument est absent du T-45 et aucune source
  primaire n’a été trouvée pour les autres.
- **Puissance moteur et auteur du waveoff.** Maintenir hors périmètre tant que DCS n’expose pas les
  données nécessaires. Ne pas inventer OWO/WOP à partir de la seule trajectoire.
- **Modèle statistique de notation.** Écarté faute de corpus aligné de notes LSO humaines et traces
  DCS ; ne reprendre qu’avec une nouvelle source de vérité exploitable.

## Préparation release et exploitation

- ~~Remplacer la dépendance locale `../DCS-gRPC/stubs` par un pin Git immuable~~ — fait : pin sur le
  tag `v0.9.2` du fork `sevenfifty777/rust-server`. Reste à valider live la ligne serveur réellement
  déployée selon le runbook d’AGENTS.md.
- Exécuter `cargo audit` localement dès que l’outil est disponible ; la CI le fait déjà.
- Chronométrer en staging la bascule et le rollback avec le vrai wrapper de service, les vraies
  permissions et une copie de la base. L’objectif de cinq minutes n’est pas encore prouvé.
- Vérifier `--positions-only` en usage prolongé et multi-recovery, notamment son p99 <300 ms.
- Pendant une preuve live, lancer LSO et le monitoring dans des processus séparés afin que l’arrêt
  du monitoring ne tue pas une recovery en cours.

## Matrice de validation avant promotion majeure

Chaque preuve doit conserver versions/hash, mission, horodatage UTC, logs bruts et vérité attendue
écrite avant analyse, selon le protocole d’AGENTS.md.

1. CATOBAR nominal, câbles DCS 1–4 et vérité indépendante.
2. Pattern puis finale ; waveoff, bolter et nouveau circuit sans contamination inter-track.
3. Événements absents, dupliqués, tardifs et réordonnés ; fermeture/reconnexion du stream.
4. Hook complet sur F/A-18C, T-45, F-14A/B/B(U), y compris timeout/error/stale et passe >128 s.
5. AV-8B/Tarawa : VL, RVL, rebond, double contact, T&G et go-around.
6. Navire en ligne droite, virage et accélération autour des seuils de skew.
7. Respawn, rotation de mission, changement de slot, homonymes et changement d’epoch.
8. Hornet/CVN et AV-8B/Tarawa simultanés ; deux recoveries complètes simultanées sur un navire.
9. 40 joueurs/deux navires, puis stress trois porte-avions.
10. Hook indépendant contre `--legacy-inline-hook-sampling`, avec `--no-acmi` et RPC hook retardé.
11. Source unary contre buffered ; livraison à 300 ms/1 s, gel producteur, capacité/rétention,
    retry du même `after_sequence` et continuité des snapshots intermédiaires.
12. Détecteurs actifs puis suspendus pendant le groove.

Pour chaque événement, corréler charge utile et ordre d’arrivée entre DCS, DCS-gRPC et LSO. L’ACMI
LSO peut compléter le diagnostic, jamais servir de source indépendante pour les événements live.

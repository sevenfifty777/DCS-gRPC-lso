# Contexte de DCS-gRPC-lso

> Réanalyse consolidée du 30 août 2026 sur `95f1d27ff273c93b547c963514d26c8d77b31d7f` (`feature/developpement-post-analyse-3082026`, également `main` local au moment du contrôle), intégrant les décisions de `docs/todoanalyse-GPT-5.6-a25303e.md`. Le binaire serveur déclaré correspond à `bc5da20f83bdd98932e5e1b2da17ee69159cbbd9`, et non au HEAD analysé.
> Ce document décrit le comportement du dépôt local. Il ne certifie ni la conformité NATOPS, ni la compatibilité avec une version de DCS World qui n'a pas été testée.

## 1. Objet du projet

DCS-gRPC-lso est un client Rust externe à DCS World. Il découvre les avions joueurs et les navires supportés via DCS-gRPC, détecte une tentative de recovery, échantillonne la télémétrie, interprète les événements de toucher et produit un grade, des rapports et des graphiques.

Le HEAD ajoute deux familles : **CATOBAR** (F/A-18C, F-14, T-45 sur CVN/Forrestal) et **V/STOL** (AV-8B NA sur LHA Tarawa). Sorties : PNG groove/pattern, JSON, SQLite, Discord et ACMI facultatifs, journal et dashboard HTTP.

Le grade automatique reste **expérimental**. Le CATOBAR résume surtout trois gates GS/LU. En V/STOL, la grille A/B/C/D et le bonus de spot sont des conventions du projet non retrouvées dans les NATOPS consultés.

Catégories : **Confirmé par le code**, **Décidé par l'utilisateur**, **Confirmé par la documentation**, **Confirmé par test ou corpus**, **Déduction raisonnable**, **Hypothèse à vérifier**, **Information manquante**.

## 2. Périmètre et limites de l'analyse

### Révision et delta

- HEAD local analysé : `95f1d27`, 19 commits après la photographie `a25303e`; version Cargo toujours `0.2.0`, donc `lso.exe --version` ne permet pas d'identifier un commit (`Cargo.toml:1-4`).
- Binaire déclaré en production : `bc5da20`, antérieur au correctif graphique `6330349`; le comportement local et le comportement déployé doivent rester distingués.
- Migration vers le fork `sevenfifty777/rust-server`, `dcs-grpc-stubs 0.9.0`, tag officiel `v0.9.0` résolu par `Cargo.lock` au commit `5bd6d6e42491c8697a5c5a95e80a2e689923bd3b` (`Cargo.toml:37-41`; `Cargo.lock:647-649`). Cela prouve la version du client compilé, pas celle de la DLL/Lua réellement installée sur le serveur.
- Les cinq fixtures ACMI ont été restaurées dans `tests/recordings/` par `bc5da20`.
- Le commit `6330349` sépare les panneaux, extrait des séquences finales continues et sélectionne la dernière branche CATOBAR, ce qui corrige localement le grand raccord graphique entre pattern et groove (`draw.rs:34-79,137-283,660-680,945-964,1181-1212`).

### Éléments inspectés

Ensemble de `src/`, manifests, tests/fixtures, CI, docs, historique Git local, distribution/protobuf/Lua DCS-gRPC 0.9.0, géométrie Tarawa locale, fichiers serveur fournis sous `.ignore/docs-prompt/`, NAVAIR 00-80T-104 (2001 et contrôle 2009), NAVAIR 00-80T-105, NAVAIR 00-80T-111 (2004), AV-8B NATOPS (2008), étude NPS (1995) et NAVMC 3500.51B Ch.1 officiel USMC (2014).

### Contrôles exécutés

| Contrôle | Résultat |
|---|---|
| `cargo test --locked --no-fail-fast` | Réussi : 59 tests passés, 0 échec |
| `cargo fmt --all -- --check` | Réussi |
| `cargo clippy --locked --all-targets -- -D warnings` | Échec : 13 emplacements `result_large_err` liés à `tonic::Status`, plus `items_after_test_module`; 14 erreurs lors de la cible test |
| `git diff --check` | Réussi |
| état Git initial | `docs/todoanalyse-GPT-5.6-a25303e.md` déjà modifié par l'utilisateur; aucune source modifiée |

La restauration des fixtures et le formatage ont levé les anciens blocages. La commande Clippy exigée par la CI (`.github/workflows/ci.yml:44-50`) échoue toutefois avec le toolchain stable local du 30 août 2026; une CI relancée avec ce toolchain est donc susceptible d'être rouge. Cette conclusion remplace l'ancienne affirmation erronée d'une suite non compilable faute de fixtures.

### Limites

- **Décidé par l'utilisateur** : DCS Dedicated Server `2.9.29.27278`, Windows Server 2019, LSO sur la même machine, écoute DCS-gRPC locale `127.0.0.1:50051`, jusqu'à 40 joueurs, un CVN Nimitz et un Tarawa usuels.
- **Confirmé par les fichiers fournis** : `throughputLimit=600`, 18 appels traités toutes les 0,03 s, écoute locale et authentification DCS-gRPC désactivée (`.ignore/docs-prompt/gRPC.log:1-6`; `.ignore/docs-prompt/dcs.log:1052-1057`).
- **Information manquante** : `version.lua`, hash de la DLL/Lua installée, résultat `Metadata.GetVersion`, configuration source `dcs-grpc.lua`, logs propres à `lso.exe`, mission `.miz` effective et capture brute AV-8B/Tarawa. La version serveur 0.9.0/0.9.1 ne peut donc pas être authentifiée à partir du corpus fourni.
- **Décidé par l'utilisateur** : la première phase de développement sera réalisée sans captures live AV-8B/Tarawa. Les données de la soirée de tests seront fournies ensuite et constitueront un jalon de validation obligatoire avant de considérer comme fiables les hypothèses relatives à `RunwayTouch`, LQM, VL/RVL, rebonds et contacts multiples.
- Aucun exemple V/STOL dans les 33 JSON de `trap sample/`.
- Le corpus `trap sample/` est déclaré obsolète par l'utilisateur; il reste utile pour démontrer l'ancien défaut de gates dupliqués, mais pas pour accepter la future version.
- 00-80T-111 est officiel, mais la copie accessible est hébergée par un tiers et porte une restriction de diffusion ; aucune copie publique NAVAIR officielle trouvée.
- 00-80T-111 ne démontre pas que plusieurs AV-8B peuvent rester sur 7/7½/8 pendant d'autres recoveries : SOP, Air Boss, deck handling et 00-80T-106 sont aussi requis.
- La fiabilité `RunwayTouch` du Tarawa reste à reproduire sur la build déployée.

### Évolution depuis l'analyse précédente

| Élément | Conclusion mise à jour | Motif |
|---|---|---|
| Fixtures et tests | Fixtures présentes; 59 tests réussissent | `bc5da20`, `tests/recordings/`, contrôle local |
| Formatage | `cargo fmt --check` vert | `a5c6f20`, contrôle local |
| Clippy | Toujours rouge sur le stable actuel, mais pour de nouveaux constats précis | contrôle local; ne plus reprendre les anciens « 24 erreurs » |
| PNG groove erroné | Corrigé dans le HEAD par sélection d'une branche continue; non corrigé dans le binaire `bc5da20` déclaré déployé | `6330349`, `draw.rs:137-283` |
| Gates | Aucun correctif de franchissement/interpolation/qualité n'a été ajouté | `track.rs:436-506`, `GateDatum:129-148` |
| `_OK_` / Unicorn | `_OK_` est un symbole officiel de perfect pass; « Unicorn » et câble 3 + 15–18,99 s ne le sont pas | NAVAIR 00-80T-104 §11.4.1; code `grading.rs:160-203` |
| Barème V/STOL | Le 00-80T-111 publie bien `_OK_=5`, `OK=4`, `(OK)=3`, `--=2`, `WO=1`, `C=0`; il ne valide pas la moyenne de gates ni le bonus A/B/C/D du code | 00-80T-111 §16.7 p.16-5 |
| Environnement serveur | Build DCS, limite 600, écoute et désandboxage sont désormais observables | `.ignore/docs-prompt/` |
| Décisions | E/H/K/R/V notamment sont décidés mais non implémentés; W et Y restent partiels | todo complété, comparé au code |

### Matrice consolidée des décisions du todo

Une case cochée dans une phase ou un critère d'acceptation exprime une cible; elle ne prouve pas que le code la satisfait.

| Sujet | Décision ou réponse | Statut | Conséquence technique | Code concerné | Validation encore nécessaire |
|---|---|---|---|---|---|
| A — périmètre | F/A-18C, F-14A/B/B(U), T-45, AV-8B; Nimitz + Tarawa; humains et IA via l'option existante `--ki` | Décidé, compatible sous condition de déploiement | conserver l'opt-in actuel : humains par défaut, IA si `--ki`; AV-8B uniquement Tarawa | `data.rs:137-334,469-478`; `run.rs:46-48,388-400` | ajouter `--ki` à la commande de production et vérifier sa présence au démarrage |
| B — autorité | grade Rust autorité dès la prochaine version | Décidé comme cible | barème dérivé de la doctrine, déterministe, versionné et transparent; pas une certification USN/USMC | `grading.rs` | satisfaire E/K/R/X et publier le manuel explicatif du barème |
| C — doctrine | critères/catégories officiels prioritaires; formules projet autorisées lorsque les manuels ne donnent pas de conversion calculable | Décidé sous condition de transparence | séparer règles officielles, métriques dérivées et score projet dans le code, les rapports et la documentation | `grading.rs:3-20,83-163` | commentaire obligatoire pour toute formule/valeur non directement sourcée; référence document/édition/section pour toute règle officielle |
| D — portée | groove noté; pattern visualisé/non pondéré | Décidé, déjà cohérent dans son principe | conserver `PATT`/observations séparés | `draw.rs`; `grading.rs` | origine explicite requise avant `WOP`; future pondération non arbitrée |
| E — gates | trois gates valides obligatoires; franchissement, interpolation et qualité persistée | Décidé, non implémenté | créer un état `Incomplete/TelemetryGap`, aucun point | `track.rs:129-148,436-506`; `grading.rs:214-351` | tests 0/1/2/3 gates, trous et démarrage tardif |
| F — observation | gates de résumé + analyse continue du groove | Décidé, partiel | la trajectoire est enregistrée, mais seul le trio de gates note | `track.rs:243-249,519-525,650-673` | définir les tendances/excursions et leur effet métier |
| G — AoA/énergie | AoA à intégrer après validation par appareil; puissance indisponible explicitement | Accepté sous condition | ne pas noter l'AoA avant tables validées | `data.rs`; `draw.rs:1092-1101` | corpus/type NATOPS; le code actuel colore seulement |
| H — waveoff | outcome neutre tant que l'initiateur n'est pas prouvé | Décidé, non implémenté | remplacer `WaveoffPilot`; causes séparées si source explicite | `track.rs:150-164,360-367`; `record_recovery.rs:71-84` | source OWO/LSO/FD/WOP ou maintien « unknown initiator » |
| I — touchdown/bolter | corréler événements et télémétrie; crosse observée durant le groove | Décidé, non implémenté | conserver preuves brutes et confiance | `record_recovery.rs:277-415`; `track.rs:294-350` | valider `RunwayTouch`, `Land`, LQM, argument 25 par module |
| J — câble | conserver DCS et estimé; afficher estimé en principal | Décidé, non implémenté | divergence visible et deux champs persistés | `track.rs:600-639`; `record_recovery.rs:512-573`; `db.rs:18-40` | tests de précision par appareil/carrier; corriger rotation degrés/radians `track.rs:698-704` |
| K — Unicorn | désactiver la règle câble 3 + 15–18,99 s | Décidé, contradictoire avec le code | conserver `_OK_` officiel sans le surnom/règle « Unicorn » | `grading.rs:32-42,137-163,170-203` | tests et migration d'enum; empêcher aussi `_OK_` V/STOL artificiel |
| L — identité | UCID clé interne; absent des fichiers JSON de rapport, mais autorisé dans le JSON `/api/passes` du dashboard | Décidé, partiellement conforme | registre par session/slot; conserver `pilot_ucid` dans `StoredPass`, sans l'ajouter aux rapports JSON de passe | `run.rs:183-198`; `record_recovery.rs:94-117,545-559`; `db.rs:42-66`; `web.rs:32-37` | tests homonymes/slot/leave; politique d'accès et d'authentification du dashboard distincte |
| M — compatibilité | ajouts compatibles et migrations non destructives | Décidé, partiel | versionner schémas et préserver site externe/Discord/dashboard | `db.rs:68-185` | migrations avalent toutes les erreurs; tester anciennes DB/JSON |
| N — rétention | JSON/DB systématiques; PNG/ACMI supprimés manuellement | Décidé | ACMI reste optionnel; pas de rotation automatique | `record_recovery.rs:489-595` | espace disque/sauvegarde et politique sur données conservées éternellement |
| O — livraison | déploiement direct principal par admins | Décidé | rollback prétesté en moins de 5 min indispensable | procédure externe | test réel du rollback; risque supérieur à un déploiement progressif accepté par l'utilisateur |
| P — erreurs | isoler l'échec à une passe | Décidé, non implémenté | ne pas relancer toutes les paires pour erreur locale | `run.rs:238-278,374-377,97-124` | annulation/génération des tâches et idempotence des sorties |
| Q — validation | aucune validation humaine requise; barème dérivé accepté | Décidé sous condition | score autonome du projet fondé sur la doctrine, jamais présenté comme certification officielle | ensemble grading | formules traçables, tests reproductibles, version du barème et manuel explicatif accessibles aux pilotes |
| R — trous | warning >300 ms; incomplet >1 000 ms; canal actif détecté <2 s | Décidé, non implémenté | watchdog, fraîcheur, continuité et fragmentation explicites | clients; `record_recovery.rs:186-275` | ne pas traiter le silence normal d'un stream d'événements comme panne |
| S — ressources | priorité 1 fiabilité, priorité 2 optimisation; budgets chiffrés après benchmark | Décidé comme hiérarchie | aucune optimisation ne doit dégrader fraîcheur, complétude, isolation ou traçabilité; centraliser/cache avant réduire la cadence | run/detect/record/web/draw | benchmark 40 joueurs, 2 navires usuels et 3 carriers de stress; établir baselines et seuils de non-régression |
| T — T&G/CQ | outcome distinct si crosse relevée près du pont; pas de `_OK_` | Décidé, non implémenté | enregistrer crosse tout le groove, décider dans une fenêtre finale | `track.rs:294-350`; `grading.rs:179-203` | lever l'ambiguïté entre observation complète et latch permanent |
| U — carrier | brut+filtré; skew ≤100 ms direct, 100–300 ms extrapolation courte, >300 ms invalide | Décidé, non implémenté, `PROJECT-DERIVED` | alignement temporel avion/navire obligatoire; conserver brut, corrigé, skew et méthode | `track.rs:252-273`; `record_recovery.rs:192-262` | valider les seuils et l'erreur d'extrapolation avec captures live, navire en ligne droite et en virage |
| V — compatibilité | matrice stricte AV-8B↔Tarawa, appareils à crosse↔Arrested | Décidé, non implémenté | filtrer avant création des tâches | `run.rs:280-353`; `commands/file.rs:109-143` | simultanéité et enveloppes chevauchantes |
| W — spots | phase 1 : seul le spot 7½ est attribué et doit être libéré rapidement; occupation/libération détectées géométriquement à titre informatif; autres spots différés | Décidé pour la phase 1 | `intended_spot=7½`, `actual_nearest_spot` et état d'occupation séparés; aucune pénalité ni déclaration automatique de `foul deck` | `data.rs:286-334`; `record_recovery.rs:489-538` | calibrer et tester la zone géométrique; définir ultérieurement autres spots, délais normatifs et autorité foul deck |
| X — V/STOL | score actuel conservé comme expérimental, incomplet interdit; futur barème dérivé documenté autorisé | Accepté sous condition, non implémenté | employer l'échelle officielle 0–5 tout en identifiant la formule de conversion comme règle projet | `grading.rs:83-153,208-268` | remplacer ou justifier moyenne+bonus; publier sources, hypothèses, seuils et limites |
| Y — DCS-gRPC | mise à niveau autorisée en test; identification exacte du serveur reportée après la première phase | Accepté sous condition | première refonte compatible avec l'interface actuelle, sans changement de protocole ni nouvelle garantie supposée | `Cargo.toml:37-41`; protos Metadata/Mission | non bloquant en phase 1; `Metadata.GetVersion`, hashes DLL/Lua et pin production redeviennent obligatoires avant upgrade ou diagnostic de compatibilité |
| Z — StreamUnits | conserver `GetTransform` actif; StreamUnits seulement préfiltrage éventuel | Décidé, conforme au principe | cache partagé prioritaire; ne pas descendre le groove à 1 Hz | `record_recovery.rs:186-204`; proto StreamUnits | benchmark cache partagé; aucune implémentation actuelle |

### Correction méthodologique issue de l'audit ciblé des gates

L'analyse initiale avait correctement identifié qu'un `Option<GateDatum>` absent n'était pas pénalisé par le grading, mais elle n'avait pas estimé sa fréquence ni vérifié si un gate présent avait réellement été mesuré à sa distance. Elle examinait surtout les branches d'erreur et la conséquence d'un `None`, sans confronter les valeurs du corpus ni suivre la sémantique exacte de `x <= gate`. Cela a laissé subsister une confusion entre **complétude syntaxique** (champ `Some`) et **validité métrologique** (mesure prise au bon endroit, au bon moment et indépendamment des autres gates).

L'audit ciblé corrige cette lacune : toute future conclusion de fiabilité devra vérifier séparément (1) présence, (2) condition et provenance de capture, (3) distance/temps réels, (4) indépendance des observations, (5) fraîcheur/skew et (6) distribution sur le corpus. Les commentaires, noms de fonctions et types `Option` ne seront plus considérés comme preuve que la mesure correspond à l'événement métier annoncé.

## 3. Cartographie du dépôt

| Zone | Rôle |
|---|---|
| `src/main.rs` | CLI, Tokio, logs, Ctrl-C |
| `src/commands/run.rs` | connexion, full-sync, tâches, reconnexion, Birth |
| `src/tasks/detect_recovery_attempt.rs` | détection de l'enveloppe |
| `src/tasks/record_recovery.rs` | suivi 10 Hz, événements, finalisation/sorties |
| `src/data.rs` | modèles et géométries avion/navire |
| `src/track.rs` | état, repères, gates, touchdown, outcome |
| `src/grading.rs` | grades CATOBAR/V/STOL |
| `src/transform.rs`, `src/draw.rs` | transformations et PNG |
| `src/db.rs`, `src/web.rs` | SQLite/dashboard |
| `src/client/` | wrappers tonic |
| `docs/DCS-gRPC-0.9.0/` | serveur/Lua/protobuf attendus |
| `docs/Carrier info/` | géométries DCS, dont Tarawa |
| `tests/recordings/` | cinq fixtures ACMI CATOBAR restaurées et utilisées par `src/tests.rs` |
| `.ignore/docs-prompt/` | artefacts du serveur fourni : journaux DCS/DCS-gRPC et scripts d'installation |
| `VSTOL.md` | calibration V/STOL |
| `releases/lso.exe` | binaire 0.2.0, version insuffisante pour identifier le commit |

README, `docs/LSO_ANALYSIS.md` et `docs/GRADING_REFERENCE.md` sont partiellement obsolètes : omission/déclaration d'absence AV-8B/Tarawa et parfois 3,5° pour tous les avions, contre 3° dans le code AV-8B.

## 4. Architecture technique

```text
DCS/MSE Lua → DCS-gRPC v0.9.0 (5bd6d6e) → tonic/Tokio
  → full-sync → une tâche par avion × navire
  → Track CATOBAR ou V/STOL → grade → JSON/PNG/DB/Discord/ACMI
```

Le mode live se reconnecte avec backoff jusqu'à 30 s (`run.rs:74-124`) et keepalive HTTP/2 (`run.rs:131-134`). Il crée le **produit cartésien** avions×navires (`run.rs:280-293`) puis écoute les `Birth` (`run.rs:315-353`). Les cartes construites au full-sync ne sont pas enrichies par ces `Birth`; deux unités apparues après le démarrage peuvent donc ne jamais être appariées.

La clé `(plane_id, carrier_id)` sépare les trackers (`run.rs:202-206`), mais `Arrested`/`Vstol` appartient uniquement au navire (`data.rs:286-314`) : aucune compatibilité du couple n'est validée.

Une paire inactive fait deux `GetTransform` toutes les 2 s. Une active ouvre son propre `StreamEvents` et poll à 10 Hz : environ 30 RPC/s CATOBAR, 20 RPC/s V/STOL. À 40 avions et 2 navires, les seules paires inactives représentent environ 80 RPC/s. Le full-sync est non borné, les buffers de passe/session ne sont pas plafonnés et le rendu PNG reste synchrone dans Tokio (`record_recovery.rs:192-204,593-594`).

**Décidé par l'utilisateur — hiérarchie de développement** : (1) fiabiliser le module, puis (2) l'optimiser. En l'absence de budgets absolus préalables, la première phase doit mesurer CPU, RAM, RPC/s, latence et effet sur les FPS/tick DCS afin d'établir une baseline et des seuils de non-régression. Une optimisation ne peut être acceptée si elle augmente les données manquantes/périmées, le skew, les erreurs de corrélation ou les interactions entre recoveries. Privilégier les suppressions de travail redondant — matrice avion/navire, cache partagé, files bornées, IO hors runtime — avant toute réduction de la cadence active de 10 Hz.

Une erreur locale est envoyée au superviseur global (`run.rs:238-261`); la première erreur termine la génération courante (`run.rs:374-377`) puis `execute()` relance l'ensemble (`run.rs:97-124`). Les `JoinHandle` restants ne sont pas explicitement annulés : une ancienne génération peut coexister avec la nouvelle, produire des doublons et supprimer une tâche remplaçante par `map.remove(key)` (`run.rs:263-265`).

## 5. Intégration avec DCS-gRPC

Le client compile le fork DCS-gRPC au tag `v0.9.0`, résolu au commit `5bd6d6e` (`Cargo.toml`, `Cargo.lock`). Le serveur doit être authentifié séparément : les artefacts fournis prouvent DCS Dedicated `2.9.29.27278`, l'écoute locale `127.0.0.1:50051`, `throughputLimit=600` et l'absence d'authentification, mais ni `version.lua`, ni hashes DLL, ni réponse `Metadata.GetVersion` ne sont disponibles (`.ignore/docs-prompt/dcs.log:5,1052-1057`; `.ignore/docs-prompt/gRPC.log:1-6`). La compatibilité effective reste donc **à confirmer**.

`GetTransform` exporte `timer.getTime()`, `getPosition()` et `getVelocity()` (`methods/unit.lua:74-84`; `exporters/object.lua:37-48`). La crosse utilise draw argument 25 (`unit.lua:49-58`). Les événements DCS sont relayés sans garantie visible de déduplication (`methods/mission.lua:113-130,443-451`; `grpc.lua:237-256`).

| Interface | Usage |
|---|---|
| Coalition/GetGroups, Group/GetUnits | full-sync |
| Unit/GetDescriptor | identification carrier |
| Unit/GetTransform | avion/navire |
| Unit/GetDrawArgumentValue(25) | crosse CATOBAR |
| Mission/StreamEvents | Birth et événements de passe |
| Hook/GetMissionName, Net/GetPlayers | mission/identité |
| World/GetTheatre, Atmosphere/GetWind | métadonnées |

Le touchdown dépend uniquement de `RunwayTouch`, pas `Land` (`record_recovery.rs:340-415`), alors que Lua 0.9 expose les deux. Un VL AV-8B sans `RunwayTouch` ou sans `place.unit == Tarawa` sera manqué.

### `StreamUnits`

`StreamUnits` est inutilisé. La documentation le réserve aux cartes en ligne à faible cadence, « not as a Tacview replacement »; `poll_rate` est en secondes (`mission.proto:19-23,607-640`). À 65–75 m/s, 1 Hz laisse 65–75 m entre points : insuffisant pour le groove.

**Conclusion** : ne pas remplacer le polling actif 10 Hz. Évaluer `StreamUnits` seulement pour découverte/préfiltrage. L'optimisation prioritaire est un cache partagé : un transform par unité et par tick, distribué aux trackers.

### Désynchronisation et reconnexion

Avion/navire sont deux commandes MSE avec timestamps propres. Le code les combine sans limite de skew (`record_recovery.rs:240-262`), ne rejette ni donnée périmée ni temps non croissant et n'appelle pas `Mission.GetSessionId` (`mission.proto:80-83`).

Une coupure peut perdre un événement sans replay, bloquer un RPC sans deadline ou raccorder deux sessions. Symptômes : trajectoire cassée, gate sautée, GS/LU faux, toucher manquant ou rapport double/absent. Le transform du `RunwayTouch` est plus cohérent car avion et `place` sont exportés dans le même callback.

**Décidé par l'utilisateur, non implémenté** : avertissement si un sample actif dépasse 300 ms; passe `Incomplete`/`TelemetryGap` à partir de 1 000 ms; watchdog d'un canal actif en moins de 2 s. Les échantillons doivent porter timestamps, skew et fraîcheur; aucune reconnexion ne doit raccorder silencieusement des fragments ou sessions. `Metadata.GetVersion` et `Mission.GetSessionId`, exposés par les protos locaux, doivent compléter le diagnostic (`metadata.proto:7-25`; `mission.proto:80-83,823-829`).

## 6. Modèle de données

`AirplaneInfo` porte hook/landing reference, glide slope et AoA. `AV8BNA` est ajouté avec ID 4, GS 3° et AoA visuel 10–12° (`data.rs:251-284,426-448`).

`CarrierInfo` porte `CarrierRecovery::{Arrested,Vstol}` (`data.rs:286-314`). `LHA_Tarawa` : deck angle 0°, deck 19,98 m, point 7½ `(-3.10,19.95,-64.81)`, axe 27,24 m à bâbord et cible 120 ft (`data.rs:317-340,381-389`). Ce sont des calibrations DCS, pas des constantes NATOPS.

`Track` conserve datums, gates, temps, minimum et outcome. V/STOL ajoute distance/grade de spot. SQLite/JSON ont `spot`, `spot_grade`, `spot_distance_m`, `outcome`, mais pas l'identifiant/type du navire, le recovery mode, l'identifiant de session, la complétude, les gaps ni la confiance des données.

La taxonomie commune contient `Bolter`; un V/STOL qui repart peut donc recevoir ce terme CATOBAR inadapté.

## 7. Cycle de vie complet d'un recovery

1. Connexion et full-sync.
2. Classification avion/navire.
3. Tâche par couple cartésien.
4. Détection toutes les 2 s : <1 100 ft MSL, ≤3,5 NM, >200 m (`detect_recovery_attempt.rs:52-89`).
5. `Track`, `StreamEvents`, échantillonnage 10 Hz.
6. Projection repère navire, gates et datums. Un gate est actuellement rempli au premier échantillon admissible vérifiant `x <= seuil`, pas nécessairement lors d'un franchissement encadré du seuil (`track.rs:430-483`).
7. `RunwayTouch` aux IDs exacts → `Track::landed`.
8. Surveillance dix secondes; départ >150 m peut modifier l'outcome.
9. `finish`, grade et sorties.

CATOBAR suit angled deck, crosse et LQM/câble. Le grade Rust combine trois gates GS/LU; l'AoA reste hors note.

V/STOL suit un axe parallèle au BRC décalé bâbord. Une droite de 3° rejoint 120 ft abeam du 7½; gates ¾, ½, ¼ NM. Au toucher, la référence avion-sol est comparée au seul 7½ dans le repère Tarawa (`track.rs:396-483,509-575`). Moyenne des gates présentes puis bonus spot.

## 8. Machines à états et règles de déclenchement

```text
détection → Unknown
  ├─ RunwayTouch exact → Recovered → délai 10 s
  ├─ éloignement → Waveoff/Bolter selon état
  ├─ crash/dead/leave → abandon
  └─ sortie enveloppe → finish/rejet
```

Risques confirmés :

- `min_distance_state` ne prouve pas un deck crossing : WO CATOBAR possiblement Bolter;
- `hook_was_up` reste vrai après un ancien échantillon crosse relevée;
- V/STOL `Recovered` s'éloignant >150 m devient aussi `Bolter` (`track.rs:306-328`);
- plusieurs `RunwayTouch` remplacent heure/distance et relancent le délai;
- **zéro gate V/STOL** → `grade_from_gates(empty) == OK`, puis spot A → `_OK_`/5 possible (`grading.rs:202-243,282-333`);
- une/deux gates absentes ne sont pas pénalisées.

### Complétude et validité des gates

**Mesure sur le corpus local** : les 33 JSON de `trap sample/` ont tous trois champs de gate présents, soit 0/33 rapport avec gate absent. Ce corpus ne contient aucun exemple V/STOL et ne mesure pas les pertes réseau du serveur déployé. Il permet donc seulement d'estimer **faible** le risque d'un `None` dans un rapport CATOBAR nominal déjà finalisé, avec un niveau de confiance modéré.

Cette présence est trompeuse : 6/33 rapports ont au moins deux `GateDatum` strictement identiques et 5/33 ont les trois identiques. Certains portent des valeurs incompatibles avec un prélèvement dans le groove (par exemple lineup proche de ±90°). La cause est **confirmée par le code** : les trois tests utilisent `x <= gate`; si le premier échantillon admissible est déjà à l'intérieur de plusieurs seuils, plusieurs gates reçoivent le même état (`track.rs:435-483`). Le garde `in_approach <= 500 ft` ne suffit pas à exclure toute portion basse du pattern et `gate_lined_up` n'est appliqué qu'au V/STOL.

Estimation corrigée :

| Risque | Estimation | Justification |
|---|---|---|
| gate absent dans un rapport nominal terminé | **Faible** | seuil cumulatif `x <=`; 0/33 dans le corpus CATOBAR |
| gate présent mais tardif, dupliqué ou hors phase | **Modérée**, déjà observée | 6/33 dupliqués; aucune distance/heure de capture persistée |
| passe entière perdue sur erreur transitoire | **Modérée à mesurer** | `GetTransform(...).await?` fait sortir `record_recovery`, puis la tâche de paire (`record_recovery.rs:192-196`; `detect_recovery_attempt.rs:33-45`) |
| fiabilité AV-8B/Tarawa | **Indéterminée à modérée** | aucun JSON V/STOL réel; garde lineup spécifique susceptible de différer/retarder la capture |

Une cadence nominale de 10 Hz rend improbable le saut géométrique d'un gate lors d'un fonctionnement sain, mais elle n'est pas garantie : chaque tick attend les RPC et `MissedTickBehavior::Delay` décale les ticks suivants (`record_recovery.rs:186-204`; `utils/interval.rs:8-12`). En outre, la détection à 2 s et les appels de métadonnées exécutés avant la boucle peuvent commencer le suivi tardivement (`detect_recovery_attempt.rs:21-36`; `record_recovery.rs:94-175`). Les transforms avion/navire peuvent aussi avoir des timestamps différents sans limite de skew.

**Conclusion d'architecture** : DCS, DCS-gRPC, le réseau et la charge empêchent une garantie absolue et une longue coupure ne peut pas être reconstruite sans source enregistrée. Ils n'empêchent pas une récupération robuste au sens logiciel. Le module peut encadrer les franchissements, interpoler les trous courts, qualifier chaque mesure et refuser de noter les données insuffisantes. Le défaut actuel est donc principalement partagé entre temporalité externe et validation Rust insuffisante, non une fatalité imposée par DCS.

## 9. Calculs, unités et repères

- DCS `(x,y,z)` devient est/haut/nord `(z,y,x)` (`transform.rs:76-80`).
- Mètres en interne, NM/ft à certains affichages, radians puis degrés.
- Repère relatif au navire; EMA position (`track.rs:54,249-270`) sans conserver brut+filtré.
- Skew avion/navire non contrôlé : virage/accélération biaisent GS/LU.
- AoA par `acos(forward·normalize(velocity))` sans garde vitesse nulle/clamp : NaN possible (`transform.rs:34-36`).
- Axe Tarawa 27,24 m = interprétation plausible de « one plane width from edge », non chiffre NATOPS.
- Hauteur V/STOL fondée sur MSL/cible 120 ft, pas sur hauteur instantanée du pont.
- Rendu Tarawa remappé (`draw.rs:635-703,927-966`) : schématique, non métrique brut.

**Décidé par l'utilisateur — politique initiale de skew/extrapolation (`SOURCE: PROJECT-DERIVED`)** :

- skew `≤100 ms` : paire de mesures utilisable directement;
- skew `>100 ms` et `≤300 ms` : alignement sur un même instant par extrapolation courte de la position et, si l'historique le permet, de l'orientation du navire;
- skew `>300 ms` : échantillon invalide pour gates, touchdown et notation;
- conserver systématiquement les deux mesures brutes, leurs timestamps, le skew, la valeur corrigée et la méthode appliquée;
- ne jamais extrapoler sans historique valide, au-delà de 300 ms ou à travers une coupure/session;
- les invalidations répétées alimentent les règles déjà décidées de warning à 300 ms de sample age et `Incomplete/TelemetryGap` à 1 000 ms.

Ces seuils ne proviennent pas d'un NATOPS. Ils correspondent à la cadence active visée de 10 Hz et devront être confirmés ou révisés à partir des captures live, notamment lorsque le navire tourne ou accélère.

## 10. Stockage et sorties

SQLite est mutexé et l'insertion passe par `spawn_blocking`. Les migrations ignorent toutes les erreurs `ALTER TABLE` (`db.rs:93-106`). JSON/DB/dashboard/Discord divergent.

Risques : filenames à la seconde sans navire, collision possible; navire/session/complétude absents; buffers non bornés; dashboard `0.0.0.0` sans auth/TLS; erreurs DB web masquées; tous V/STOL étiquetés `Spot 7.5` (`record_recovery.rs:71-84,493-503,659-672`).

Le rapport conserve un câble principal et `estimated_cable`, mais la DB et l'interface privilégient le câble principal et perdent la divergence. **Décidé par l'utilisateur, non implémenté** : conserver câble estimé et câble DCS séparément, afficher leur provenance/confiance et faire de l'estimation géométrique la valeur principale sans la présenter comme confirmée. Le calcul actuel mérite en outre un test ciblé : `estimate_cable()` transmet un angle de pont exprimé en degrés à une rotation qui attend des radians (`track.rs:698-704`).

JSON et DB doivent rester les sorties toujours conservées; l'ACMI et le PNG pourront être supprimés manuellement. Le schéma doit évoluer par migrations additives et compatibilité descendante si possible (décisions M/N).

**Décidé par l'utilisateur — dashboard colocalisé** : le dashboard sera systématiquement installé sur la même machine que DCS-gRPC-lso. Pour que cette colocalisation supprime effectivement l'exposition réseau, le serveur web de phase 1 doit écouter sur `127.0.0.1` et non sur l'actuel `0.0.0.0` (`web.rs:11-25`); le port ne doit pas être publié par le pare-feu ou une redirection. Dans cette configuration strictement locale, OAuth2 et HTTPS internes ne sont pas requis pour la première phase. Si un accès depuis une autre machine devient nécessaire, cette décision devra être réouverte et un reverse proxy HTTPS avec authentification sera requis.

**Décidé par l'utilisateur — UCID** : les fichiers JSON individuels de rapport ne doivent pas contenir l'UCID. En revanche, son exposition dans la réponse JSON de l'API dashboard `GET /api/passes` est acceptée; le champ actuel `StoredPass::pilot_ucid` peut donc être conservé (`db.rs:42-62`; `web.rs:32-37`). Cette décision porte sur le contenu de l'API, pas sur son niveau d'exposition réseau : la politique OAuth2/HTTPS et les droits d'accès restent à définir séparément.

## 11. Analyse de l'ingénieur Rust senior

Points favorables : séparation recovery/grading, pas d'appel crosse en V/STOL, erreur crosse → `None`, événements filtrés par deux IDs, repère mobile Tarawa, DB hors worker, renderer V/STOL distinct.

Défauts prioritaires :

1. produit cartésien incompatible (`run.rs:272-284,315-345`);
2. données incomplètes sur-notées;
3. course de nettoyage d'une ancienne tâche supprimant une remplaçante (`run.rs:231-269`);
4. maps Birth non enrichies (`run.rs:307-355`);
5. erreur locale susceptible de relancer globalement `run()`;
6. aucune session/deadline/fraîcheur/skew/replay;
7. parsing câble `&w[0..1]` fragile (`track.rs:596-613`);
8. UCID tardif par nom et fallbacks V/STOL ambigus;
9. charge P×C, streams multiples, rendu sync, buffers non bornés;
10. V/STOL peu testé et Clippy incompatible avec le toolchain stable local actuel;
11. persistance avant PNG/Discord : une erreur postérieure peut provoquer une relance et un doublon;
12. calcul AoA sans garde vitesse nulle/clamp et estimation câble probablement affectée par degrés/radians.

Le correctif `6330349` sélectionne désormais la dernière branche entrante continue pour éviter l'ancienne ligne rouge CATOBAR reliant deux segments (`draw.rs`). Le binaire serveur déclaré (`bc5da20`) le précède : le correctif est **confirmé dans le dépôt actuel, mais probablement absent du déploiement**.

## 12. Analyse LSO US Navy — début des années 2000

### Doctrine CATOBAR vérifiée

Le NAVAIR 00-80T-104 du 15 décembre 2001 place explicitement sous la responsabilité du LSO la détermination de la performance acceptable pendant la **final approach** (§6.4, p.6-7). En Case I/II, le contrôle LSO commence toutefois dès la position 180° (§6.4.3.1, p.6-10) : le LSO surveille l'approach turn et doit notamment déclencher un waveoff si la trajectoire produira un groove trop court. Il ne se limite donc pas à regarder l'appareil une fois wings-level.

La consignation officielle reste principalement structurée autour de l'approche finale. La figure 11-2 (p.11-3) sépare le grade, les erreurs de glideslope/speed aux phases `AW`, `X`, `IM`, `IC`, `AR`, les erreurs de contrôle, lineup/wing, autres commentaires et le câble. Les suffixes (§11.4.3, p.11-7) définissent `X` comme le premier tiers du glideslope, `IM` le tiers central, `IC` le dernier tiers et `AR` la rampe.

Le pattern n'est cependant pas ignoré dans la pratique LSO : les symboles officiels comprennent `PATT` (pattern), `WOP` (waveoff pattern), `OT` (out of turn), `TWA`/`TCA` (too wide/close abeam) et `TTS`/`TTL` (turned too soon/late), pp.11-4 à 11-7. Le manuel ne fournit en revanche ni formule ni pondération permettant de convertir automatiquement ces écarts en points ajoutés ou retranchés au grade global. **Déduction raisonnable** : un défaut de pattern peut être consigné, débriefé ou provoquer un `WOP`, mais une sous-note automatique du Case I complet serait une convention locale à spécifier et valider.

Le CATOBAR du projet représente une partie géométrique du groove, pas une observation LSO complète. Il capture gates GS/LU, AoA visuel, toucher/câble et LQM DCS, mais ignore/simplifie tendances, corrections, power, deck motion, start/middle/in-close détaillé, qualité du touchdown, OWO/LSO WO/foul deck, l'approach turn et les annotations de pattern prévues par le 00-80T-104.

`_OK_` est le symbole officiel d'une *perfect pass* (NAVAIR 00-80T-104, 2001, §11.4.1). « Unicorn » n'est pas un grade officiel trouvé dans les éditions vérifiées. Un 3-wire ne justifie pas seul `_OK_`; l'édition 2009 (§4.2.5.1, p.4-5) admet qu'une perfect pass peut, avec deck motion, accrocher n'importe lequel des quatre câbles, aller à la rampe ou bolter. **Décidé par l'utilisateur, non implémenté** : désactiver la règle locale câble 3 + groove 15–18,99 s et le libellé « Unicorn », tout en conservant `_OK_` selon une règle documentée.

Le 00-80T-104 ne publie pas dans les éditions vérifiées un barème numérique complet transformant automatiquement toutes les observations en score. Les points historiques trouvés dans une étude gouvernementale NPS de 1995 ne constituent pas à eux seuls une politique NATOPS applicable. En conséquence, le score autonome Rust est un **score interne du projet dérivé de la doctrine**, pas un grade officiel.

**Décidé par l'utilisateur — méthode de formalisation** : le futur barème appliquera en priorité les outcomes, catégories, phases et seuils explicitement documentés. Lorsque les manuels décrivent une appréciation qualitative sans formule calculable, le projet pourra définir une formule déterministe combinant amplitude, durée, phase/proximité du pont, persistance et qualité de la correction. Les hypothèses et seuils dérivés devront être versionnés, testables et publiés; ils ne devront jamais être attribués au NAVAIR.

Convention documentaire requise dans le futur code de grading :

- `SOURCE: OFFICIAL` pour une règle directement issue d'un document, avec numéro, édition/date et section/page;
- `SOURCE: PROJECT-DERIVED` pour toute formule, pondération, interpolation ou seuil déduit, avec justification et identifiant de version du barème;
- aucune valeur métier « magique » sans constante nommée, unité, source et test de frontière;
- une recherche textuelle sur ces marqueurs doit permettre d'inventorier toutes les règles et de générer ou contrôler un manuel explicatif destiné aux pilotes.

Conclusion : outil de débrief utile, grade autonome non certifié. Pour une première version, limiter le calcul automatique au groove est cohérent avec la structure du suivi NATOPS, à condition de conserver séparément les observations de l'approach turn/pattern et les outcomes tels que `WOP`. Les intégrer ultérieurement au score exige une règle métier approuvée, car le 00-80T-104 ne donne pas de pondération numérique.

## 13. Analyse LSO USMC — AV-8B/LHA, contexte 2004

### Doctrine vérifiée

NAVAIR 00-80T-111 du 1er juillet 2004 :

- fig. 5-2 p.5-3 : spot **7½** = *primary landing spot* LHA, distinct des 7/8;
- §§6.3.2 pp.6-4 à 6-7 : break 800 ft, downwind 600 ft, AoA 10–12°, groove 0,5–0,75 NM/300–350 ft, environ 3°, approche bâbord, hover stop, 120 ft abeam, cross 50 ft au-dessus du pont, tête au-dessus du spot;
- Paddles annonce « Expect spot ___ », puis « Spot ___ »/clearance;
- §§6.3.3/6.5.2 : 7½ primaire de nuit; spots alternatifs possibles pour urgence/sécurité;
- chap. 23/fiches A-5/A-9 : note humaine sur phases/tendances, cross, hover, VL, power, attitude, spot, cap relatif; exemples de spots 7.5, 4, 5, 2.
- §16.7 p.16-5 : échelle V/STOL officielle `C=0`, `WO=1`, `--=2`, `(OK)=3`, `OK=4`, `_OK_=5`.

AV-8B NATOPS 2008 (§§7.6.2–7.6.4) confirme 3°, AoA 10–12°, hover 50–60 ft AGL et contrôle dérive/cap/attitude/taux de descente. Il ne valide pas la formule du code.

Le même principe s'applique au V/STOL : l'échelle officielle `C/WO/--/(OK)/OK/_OK_` reste la taxonomie de sortie, tandis que la conversion des mesures de trajectoire, décélération, hover, dérive, descente et spot vers cette échelle constitue une formule `PROJECT-DERIVED` dès qu'elle n'est pas explicitement donnée par le 00-80T-111.

### Écarts

- A/B/C/D `<1/<3/<5/≥5 m`, moyenne des gates CATOBAR et bonus `+1/.75/.5/0` sans source NATOPS (`grading.rs:83-153`). L'échelle finale 0–5 correspond aux valeurs officielles, mais la formule qui y mène ne l'est pas;
- aucune note de closure, hover stop, cross, drift, sink, power/nozzle, attitude, heading, heavy WO/foul deck;
- AoA seulement visuel;
- `RunwayTouch` n'est pas une preuve de VL stabilisé;
- gates CATOBAR réutilisées et incomplétude sur-notée.
- le code ne représente pas les phases `X/IM/IC`, turn/cross, hover, décélération, dérive, VL/RVL/FIRM/FD ni l'autorité du waveoff décrites par le chapitre 23.

### Spots successifs/occupation

Plusieurs AV-8B peuvent générer des passes séparées, mais le module ne considère correctement que 7½. Aucun catalogue 7/7½/8, spot assigné, nearest spot, occupation/libération ou foul deck. Un posé correct au 7/8 est déclaré 7.5 et mesuré comme erreur au 7½.

`RunwayTouchEvent` fournit initiator/place, pas le spot assigné. Le spot touché peut être inféré après calibration; le spot demandé exige une entrée externe. Nearest spot seul récompenserait potentiellement le mauvais spot.

La doctrine ne prouve pas la sécurité de plusieurs AV-8B stationnés sur spots adjacents pendant recoveries : arbitrage SOP/Air Boss/LSO/00-80T-106 requis.

**Décidé par l'utilisateur pour la phase 1** : le seul spot attribué est 7½. L'AV-8B doit le libérer aussi rapidement que possible afin de permettre la recovery suivante sur ce même spot. Le futur modèle doit néanmoins séparer dès sa conception :

- `intended_spot`, fixé à 7½ par la règle de phase 1;
- `actual_nearest_spot`, déterminé géométriquement au toucher;
- l'état d'occupation et de libération du spot;
- le catalogue extensible des spots et leur géométrie.

La méthode d'attribution d'un autre `intended_spot` est explicitement différée. Cette extensibilité ne signifie pas que les spots 7 et 8 doivent être activés ou notés durant la phase 1.

**Décidé par l'utilisateur pour la phase 1 — occupation/libération** : le module détectera géométriquement l'entrée, la présence et la sortie de l'AV-8B dans une zone calibrée autour du spot 7½. Ces états et leurs timestamps seront conservés comme informations de débrief et d'observabilité. Ils ne modifieront ni l'outcome, ni le grade, ni les points et ne déclencheront pas automatiquement un `foul deck`. La géométrie exacte de la zone doit être calibrée et testée dans DCS; elle ne doit pas être présentée comme une limite NATOPS. Une politique normative de délai, de pénalité ou de `foul deck` nécessitera ultérieurement une SOP et une autorité explicitement définies.

## 14. Analyse DCS World et DCS-gRPC

- Tarawa exige groupe `Ship`, type exact `LHA_Tarawa` et attribut carrier (`run.rs:381-409`); à vérifier live.
- `RunwayTouch` est dépendant de version; tester VL, rolling landing, bounce, touch-and-go et `place.unit`.
- Installation partielle : MSE peut fournir transforms/events tandis que Hook/Net manque identité/mission.
- `throughputLimit=600` est un plafond, pas une fraîcheur garantie (`grpc-mission.lua:31-33`; `grpc.lua:203-235`).
- Pause, accélération et mission restart ne sont pas structurés par session.
- names, slots, player names et UCID ne sont pas équivalents/uniques.
- update/repair peut écraser l'installation Lua; sandbox/exposition réseau à auditer.
- le `MissionScripting.lua` fourni désactive la sanitization de `os`, `io`, `lfs`, `require`, `loadlib` et `package` avant de charger DCS-gRPC : dépendance de déploiement et surface de sécurité/maintenance à documenter (`.ignore/docs-prompt/MissionScripting.lua`).
- les journaux fournis contiennent de nombreux événements ignorés (`1062`, `1066`) et erreurs de catégories exporteur. Cela confirme des objets/événements non pris en charge, mais ne prouve pas une perte des événements `RunwayTouch` ou LQM nécessaires au module.
- les joueurs IA sont ignorés sans `--ki` (`run.rs:46-48,388-400`). Ce comportement opt-in est désormais confirmé; la présence du paramètre devient une exigence vérifiable de la commande de production.

**Décidé par l'utilisateur — IA en production** : conserver l'option et la sémantique actuelles de `--ki`; ne pas ajouter `--human-only` et ne pas inclure les IA par défaut. La commande de production devra donc fournir explicitement `--ki`. Les IA doivent être identifiées par session DCS, ID et nom d'unité, sans UCID inventé; les données futures devront distinguer `Human` et `AI`. La matrice avion/navire doit filtrer les unités avant la création des trackers afin de limiter la charge supplémentaire. Prévoir des tests de respawn, réutilisation de nom et plusieurs IA simultanées.

## 15. Registre des risques propres à DCS

| Risque | Version | Code | Symptôme/impact | Confiance | Reproduction |
|---|---|---|---|---|---|
| RunwayTouch AV-8B absent/sans place | dépendante | `record_recovery.rs:340-415` | touchdown/passe perdu | À reproduire | log Land+RunwayTouch DS |
| doublons/rebonds | dépendante | idem | dernier contact faux | Élevée | bounce/T&G |
| transforms non contemporains | toutes | `record_recovery.rs:240-262` | GS/LU/graph faux | Très élevée | skew en virage |
| mission sans session | toutes | run/record | fragments raccordés | Très élevée | restart en groove |
| Tarawa non classé | build | `run.rs:381-409` | aucun suivi | À reproduire | GetDescriptor |
| Birth incomplet | toutes | `run.rs:287-355` | paire absente | Très élevée | spawn séquentiel |
| MSE saturé | charge | polling P×C | trous/latence | Élevée | benchmark |
| Hook/Net absent | déploiement | identité | pilote faux/vide | Élevée | install partielle |
| pause/temps accéléré | toutes | intervalle mural | sous-échantillonnage | Élevée | pause/accéléré |
| update écrase Lua | update | installation | service absent | Élevée | checklist post-update |
| serveur gRPC non authentifié | déploiement actuel | `Cargo.lock`; absence d'appel Metadata | incompatibilité/proto subtil | Élevée | GetVersion + hashes |
| événements 1062/1066 ignorés | DCS 2.9.29.27278 observé | frontière Lua | bruit, objet incomplet; impact LSO non prouvé | Confirmé dans les logs | corréler IDs et passe live |
| sandbox Lua désactivé | déploiement | `MissionScripting.lua` fourni | sécurité et réparation/update fragiles | Très élevée | audit post-repair |
| option IA omise au déploiement | toutes | `run.rs:46-48,388-400` | aucune passe IA malgré le périmètre attendu | Très élevée | vérifier la commande puis lancer avec/sans `--ki` |

Lecture causale détaillée des risques prioritaires :

| Risque | Supposition du module | Garantie réelle DCS/DCS-gRPC | Symptôme et impact | Code | Confiance | Vérification reproductible |
|---|---|---|---|---|---|---|
| `RunwayTouch` V/STOL | l'événement arrive une fois, avec `place` | à valider sur la build et le type de posé | passe absente, touchdown tardif ou écrasé | `record_recovery.rs:340-415` | Hypothèse à vérifier | journaliser Land/RunwayTouch/LQM pour VL, RVL, rebond, T&G |
| ordre/doublons | événements uniques et ordonnés | aucun mécanisme local de déduplication/replay n'est visible | outcome/câble/touchdown incohérent | `record_recovery.rs:308-415`; `track.rs:596-639` | Déduction élevée | injecter doublons et réordonnancement |
| deux transforms | avion et navire contemporains | deux RPC/MSE distincts avec timestamps propres | gate, GS/LU et dessin faux | `record_recovery.rs:192-262` | Confirmé par le code | virage navire avec skew forcé 0–500 ms |
| coupure/restart | flux continu et même mission | aucun replay; mission/session peut changer | fragment recollé, rapport absent/double | `run.rs:74-124`; `record_recovery.rs:94-175` | Confirmé par le code | couper 0,2/1/3/10 s puis changer mission |
| identité joueur | nom suffisant puis UCID récupérable | slot, déconnexion, homonymie et IA varient | attribution erronée | `record_recovery.rs:545-559` | Déduction élevée | deux noms identiques, slot change, disconnect |
| carrier/type | descripteur et nom stables | dépend du module/build DCS | aucun tracker ou mauvais mode | `run.rs:381-409`; `data.rs:286-340` | À reproduire | capturer GetDescriptor CVN/Tarawa |
| charge | plafond gRPC implique cadence | `throughputLimit` ne garantit ni latence ni fraîcheur | trous, gates manquées, FPS serveur | produit P×C et polling | Déduction élevée | benchmark 1/5/10/20 joueurs × 2 navires |
| Birth | full-sync puis Birth suffisent | fenêtre startup et apparitions séquentielles possibles | couple jamais créé | `run.rs:183-205,315-353` | Confirmé par le code | avion puis navire et ordre inverse |
| sandbox/update | fichiers Lua restent inchangés | repair/update peut restaurer le fichier; sanitization désactivée localement | service absent ou exposition accrue | `.ignore/docs-prompt/MissionScripting.lua` | Confirmé par artefact | hash/checklist avant et après repair |

## 16. Matrice de traçabilité de la télémétrie

| Donnée | Source/API | DCS-gRPC | Rust | Usage LSO | Fragilité |
|---|---|---|---|---|---|
| avion transform | Unit position/velocity | GetTransform | Transform/Track | GS/LU/trace | optionnel/skew |
| navire transform | Unit position | GetTransform | EMA/repère | deck/spot | skew/rotation |
| vitesse | Unit | GetTransform | AoA | couleur | zéro/NaN |
| crosse | draw arg 25 | Unit RPC | hook state | qualif bolter | module/polarité |
| toucher | world event | RunwayTouch | IDs + landed | recovery | version/doublons |
| LQM/câble | world event | LQM | parsing | note DCS/wire | ordre/format |
| joueur | Unit/Net | Unit+Net | fallbacks | attribution | homonymes |
| spot assigné | LSO/SOP | absent | absent | conformité | entrée externe |
| occupation | opérations pont | absent | absent | foul deck | modèle à créer |

## 17. Matrice des règles métier

| Règle | Source/transmission | Rust | Seuils | Résultat | Lecture LSO | Risque |
|---|---|---|---|---|---|---|
| début suivi | transforms | detector | 3,5 NM/1100 ft/>200 m | Track | technique | direction ignorée |
| gates | 10 Hz | Track | ¾/½/¼, ±10° | GS/LU | résumé | absents favorables |
| touchdown CATOBAR | RunwayTouch | landed | IDs exacts | recovered/wire | indice | version |
| touchdown V/STOL | RunwayTouch | distance au 7½ | mètres | A-D | non NATOPS | cible unique |
| grade V/STOL | gates+spot | moyenne+bonus | 1/.75/.5/0 | 0–5 | convention | très élevée |
| spot | place sans label | fixe 7.5 | point unique | label | faux si autre | très élevée |
| bolter/WO | transforms/events | state machine | départ >150 m | outcome | insuffisant | élevée |
| donnée périmée | timestamps disponibles mais ignorés | absent | 300 ms warning; 1 000 ms incomplete; watchdog 2 s décidés | futur statut/confiance | sécurité de notation | non implémenté |
| Unicorn CATOBAR | câble + gates | règle locale | wire 3 + 15–18,99 s | `_OK_`/Unicorn | non officiel | à désactiver |
| Unicorn V/STOL | spot+gates | bonus local | score ≥5 | Unicorn | non officiel | à désactiver |
| T&G crosse sortie | événement + état sticky | `hook_was_up` | observé n'importe quand | possible `_OK_` | contradictoire à T | non implémenté |

## 18. Cas multijoueur et scénarios limites

Les événements de toucher sont filtrés par `plane.id`+`carrier.id` et SQLite sérialise les écritures. Cela fournit une isolation partielle, mais **ne garantit pas** deux recoveries simultanées : le produit cartésien crée aussi les deux couples incompatibles, une erreur locale peut relancer la génération globale, les tâches anciennes peuvent survivre et les sorties ne portent pas toutes le navire/session.

Le conflit critique est cartésien :

| Couple | Traitement actuel | Effet |
|---|---|---|
| AV-8B×Tarawa | V/STOL attendu | vrai rapport |
| CATOBAR×CVN | arrêté attendu | vrai rapport |
| AV-8B×CVN | faux arrêté | hook/câble absurdes |
| CATOBAR×Tarawa | faux V/STOL | faux Spot 7.5 |

Si les enveloppes 3,5 NM se recouvrent, faux trackers et WO/bolters/artefacts parasites. Même éloignées, surcharge RPC. La décision V impose une matrice stricte `AV-8B↔Tarawa` et `appareils à crosse↔Arrested`, ainsi qu'une clé de recovery comprenant appareil, joueur/UCID interne, navire cible, recovery mode et session. Cette isolation est **décidée mais non implémentée**.

Autres limites : UCID/homonymes, filename simultané, slot changes, unités tardives, rebonds, hover long, coupure, virage navire, restart mission.

## 19. Écarts, anomalies et zones de risque

| Anomalie | Origine probable | Confiance |
|---|---|---|
| couples incompatibles | implémentation Rust | Très élevée |
| grade positif incomplet | Rust + modèle métier | Très élevée |
| gates présents mais dupliqués/tardifs | implémentation Rust + temporalité | Très élevée, observé dans 6/33 JSON |
| Bolter V/STOL | implémentation Rust | Élevée |
| Birth non enrichi | implémentation Rust | Très élevée |
| session/skew/deadline absents | réseau/temporalité + Rust | Très élevée |
| collision fichiers | implémentation Rust | Très élevée |
| CI non reproductible | dépôt/configuration | Très élevée |
| ancienne trajectoire CATOBAR erronée | visualisation | corrigée au HEAD par `6330349`; binaire déployé antérieur |
| version serveur DCS-gRPC inconnue | configuration/déploiement | Élevée |
| règle Unicorn/_OK_ locale | modèle métier LSO + Rust | Très élevée |
| points V/STOL corrects mais formule non officielle | modèle métier LSO | Très élevée |
| T&G avec hook sorti pouvant rester favorable | Rust + modèle métier | Très élevée |

Écarts métier : grille V/STOL non NATOPS; 7½ forcé; pas d'occupation; modèle hover/cross incomplet; CATOBAR réduit aux gates; AoA/énergie/tendances hors note.

Le défaut de trajectoire CATOBAR a été corrigé par `6330349` dans le dépôt actuel; il demeure probable sur le binaire déployé `bc5da20`. À reproduire : RunwayTouch Tarawa, descriptor Tarawa, géométrie 7/7½/8/axe, règles foul deck et autorité LQM/algorithme/LSO.

## 20. Tests existants et stratégie future

Les cinq fixtures ACMI attendues sont présentes. `cargo test --locked --no-fail-fast` réussit **59 tests sur 59**. Les tests couvrent plusieurs replays CATOBAR, quelques points/limites de spot, outcomes et DB, mais pas le chemin DCS live, les franchissements encadrés/interpolés de gates, repères complets, incomplétude, couples croisés, multi-spots, simultanéité, reconnexion ni rendu. Clippy échoue avec le stable local sur 13 `result_large_err` liés à `tonic::Status` et un `items_after_test_module`; ce résultat doit être distingué des tests fonctionnels réussis.

Priorités :

1. CVN+Tarawa éloignés puis <3,5 NM; Hornet+AV-8B simultanés; exactement 2 rapports.
2. CATOBAR et AV-8B avec 0/1/2/3 gates; franchissements encadrés, démarrage à l'intérieur de ¾/½/¼ NM, plusieurs seuils franchis pendant un trou et aucun grade favorable incomplet.
3. 7/7½/8 avec spot assigné distinct du nearest.
4. 2/3 AV-8B successifs restant au pont, taxi/takeoff/libération.
5. VL, rolling, bounce, double touch, touch-and-go, taxi >150 m.
6. skew 0/50/100/101/200/300/301/500 ms; navire en ligne droite, virage et accélération; vérifier données brutes/corrigées, interdiction d'extrapoler et bascule incomplete.
7. coupure gRPC 0,2/1/3/10 s pendant deux recoveries; restart.
8. Birth avion puis navire et inverse.
9. collision filename à la seconde.
10. benchmark 1/5/10/20/40 joueurs×2 navires, puis stress 3 carriers : baseline CPU, RAM, RPC/s, latence et FPS/tick DCS; comparer chaque optimisation sans relâcher les critères de fiabilité.
11. calibration spots et point avion-sol sous attitude navire.
12. validation contre doctrine et corpus reproductible USN/USMC; la validation humaine n'est pas une autorité de production, mais les règles dérivées doivent être transparentes, versionnées et confrontables à des cas explicables.
13. contrôle de corpus interdisant des gates identiques sans interpolation explicite et vérifiant distance, timestamp, ordre, sample gap et skew de chaque observation.

Pour la première phase, les scénarios DCS live indisponibles devront être couverts par des événements simulés et des tests conservateurs : absence, duplication, réordonnancement et répétition de `RunwayTouch`/LQM; VL, RVL, rebond et remise de gaz. Ces tests vérifieront la robustesse du code, mais ne prouveront pas le comportement réel de DCS. Après la soirée de tests, les captures devront être corrélées par timestamps entre logs DCS, DCS-gRPC, LSO, ACMI et JSON, puis transformées en fixtures de non-régression anonymisées.

## 21. Questions ouvertes

**Décision levée — version serveur DCS-gRPC** : l'identification du commit et des hashes DLL/Lua réellement déployés est reportée et ne bloque pas la première phase de refonte. Condition : cette phase doit conserver l'interface DCS-gRPC actuelle et ne doit supposer ni méthode, ni champ, ni garantie absente de la version effectivement observée. La vérification redeviendra obligatoire avant toute mise à niveau DCS-gRPC, modification des protobuf ou conclusion attribuant un défaut à une incompatibilité de version.

**Décision levée — spot V/STOL de phase 1** : `intended_spot` est implicitement 7½ pour toutes les recoveries AV-8B/Tarawa. L'architecture doit permettre d'autres spots ultérieurement, sans les activer ni définir maintenant leur mécanisme d'attribution.

**Décision levée — occupation/libération en phase 1** : détection géométrique informative autour du spot 7½, sans effet sur la note et sans déclaration automatique de `foul deck`. Les limites normatives, délais et autorités sont différés à une phase ultérieure.

**Décision levée — barème autonome** : utiliser les règles officielles lorsqu'elles sont calculables et compléter les appréciations qualitatives par des formules déterministes dérivées de la doctrine. Chaque règle doit déclarer son origine `OFFICIAL` ou `PROJECT-DERIVED`; les formules dérivées seront documentées dans un manuel transparent et publiable. Le résultat reste un score du projet, non une certification officielle USN/USMC.

**Décision levée — captures live AV-8B/Tarawa** : elles ne seront pas disponibles pour la première phase. Développer défensivement avec simulations et hypothèses explicitement marquées, puis valider et ajuster avec les données fournies après la soirée de tests. Aucune conclusion dépendante des événements DCS live ne doit être présentée comme confirmée avant ce jalon.

**Décision levée — skew avion/navire** : `≤100 ms` direct, `>100 ms` et `≤300 ms` extrapolation courte si l'historique est valide, `>300 ms` invalide. Conserver brut et corrigé; aucune extrapolation à travers une coupure ou session. Seuils `PROJECT-DERIVED` à réévaluer avec le corpus live.

**Décision levée — ressources** : ordre impératif « fiabiliser, puis optimiser ». Les budgets absolus ne bloquent pas la première phase; celle-ci doit produire les mesures de référence. Les optimisations seront ensuite classées par gain mesuré et ne devront jamais affaiblir les garanties de données ou l'isolation des passes.

**Décision levée — UCID du dashboard** : l'UCID est autorisé dans le JSON dynamique de `/api/passes`, mais reste exclu des fichiers JSON individuels de rapport. La sécurisation et l'audience du dashboard constituent une question séparée.

**Décision levée — IA en production** : conserver l'option actuelle `--ki` et son fonctionnement opt-in. La production devra être lancée avec cette option pour suivre les IA; aucun `--human-only` ni changement du comportement par défaut. Ajouter une identité IA adaptée, un indicateur `Human/AI`, la matrice de compatibilité et les tests associés.

**Décision levée — sécurité du dashboard en phase 1** : dashboard colocalisé avec DCS-gRPC-lso et accès strictement local. Lier le serveur web à `127.0.0.1` et ne pas exposer son port; OAuth2/HTTPS différés. Toute ouverture à distance imposera de réévaluer la sécurité et de placer préférentiellement l'application derrière un reverse proxy authentifié en HTTPS.

- Pin de production DCS-gRPC retenu et stratégie de test de la montée de version Y, après la première phase ?

## 22. Glossaire et index

**BRC** Base Recovery Course; **CATOBAR** recovery arrêtée; **V/STOL/VL** décollage/atterrissage court ou vertical; **Gate** point ¾/½/¼ NM; **GS/LU** glideslope/lineup; **Hover stop/cross** stabilisation bâbord puis translation; **LQM** LandingQualityMark; **Skew** écart temporel; **MSE** Mission Scripting Environment.

| Sujet | Référence |
|---|---|
| pairing/tâches | `src/commands/run.rs:202-284,315-355` |
| détection | `src/tasks/detect_recovery_attempt.rs:52-89` |
| AV-8B/Tarawa | `src/data.rs:251-340,381-448` |
| boucle/events/sorties | `src/tasks/record_recovery.rs:94-204,340-415,489-672` |
| état/gates/toucher | `src/track.rs:249-328,396-575,596-613` |
| grades V/STOL | `src/grading.rs:83-153,202-333` |
| rendu | `src/draw.rs:114-319,330-1044` |
| DB | `src/db.rs:17-106` |
| proto | `docs/DCS-gRPC-0.9.0/Docs/DCS-gRPC/protos/dcs/mission/v0/mission.proto:19-23,80-83,138-144,607-640` |

Sources doctrinales :

- [NAVAIR 00-80T-104, 15 Dec 2001 — miroir public](https://www.yumpu.com/en/document/view/62004951/lso-natops-manual)
- [NAVAIR 00-80T-104, 1 May 2009 — miroir public](https://info.publicintelligence.net/LSO-NATOPS-MAY09.pdf)
- [NAVAIR 00-80T-105, 1 Jul 2009 — miroir public](https://info.publicintelligence.net/CV-NATOPS-JUL09.pdf)
- [NAVAIR 00-80T-111, 1 Jul 2004 — miroir public](https://feral-hogs.com/Downloads/NATOPS%2000-80T-111%20VSTOL%20Shipboard%20%26%20LSO%20Manual%20Jul%202004%20pp148.pdf)
- [NAVAIR 00-80T-111, 1 Jul 2004 — miroir public alternatif](https://server.3rd-wing.net/public/Bureau_4thMEG/VMA-214/00-80T-111%20-%20VSTOL%20Shipboard%20and%20LSO%20Manual%20Jul2004%201Mb.pdf)
- [A1-AV8BB-NFM-000 AV-8B/TAV-8B, 15 Mar 2008 — miroir public](https://info.publicintelligence.net/AV-8B-000.pdf)
- [NAVMC 3500.51B Ch.1 — source officielle USMC](https://www.marines.mil/Portals/1/NAVMAC%203500.51B%20W%20CH%201.pdf)
- [NPS, *Analysis of LSO Grades*, 1995 — dépôt gouvernemental](https://calhoun.nps.edu/server/api/core/bitstreams/97ad4f6d-126e-49b1-8638-2cc975639778/content) — source historique, non assimilée à une politique NATOPS.

## Synthèse — dix points avant toute modification

1. Le code est au commit `95f1d27`; le binaire déclaré `bc5da20` n'inclut probablement pas le correctif graphique `6330349`.
2. Le client est verrouillé sur DCS-gRPC `v0.9.0`/`5bd6d6e`, mais la version serveur n'est pas authentifiée.
3. La matrice stricte avion↔navire est décidée et prioritaire; le produit cartésien actuel compromet charge et simultanéité.
4. L'isolation par IDs ne suffit pas : session, navire cible, recovery mode, génération de tâche et sorties doivent être explicitement corrélés.
5. AV-8B/Tarawa reste expérimental : en phase 1, `intended_spot=7½` et libération rapide sont décidés, mais la détection d'occupation/libération et les phases V/STOL complètes restent à développer.
6. Le 00-80T-111 confirme l'échelle 0–5 et le spot 7½ primaire, pas la formule moyenne+bonus du code.
7. `_OK_` est officiel; « Unicorn » et la règle câble 3 + 15–18,99 s ne le sont pas et doivent être désactivés.
8. Zéro gate peut encore donner un grade favorable : les seuils décidés 300/1 000/2 000 ms et la preuve de validité restent à implémenter.
9. `StreamUnits` ne remplace pas le polling 10 Hz; cache partagé, deadlines, skew, session, watchdog et instrumentation sont prioritaires.
10. Les 59 tests passent et les fixtures sont présentes; Clippy échoue encore, tandis que les scénarios live, simultanés, V/STOL et de coupure restent non couverts.

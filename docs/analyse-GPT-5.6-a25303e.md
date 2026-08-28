# Contexte de DCS-gRPC-lso

> Réanalyse complète du 28 août 2026 sur `a25303e757b587019f7be1f446c2927f16ebab47` (`main`), comparée à l'analyse précédente de `30f3f4d`.  
> Ce document décrit le comportement du dépôt local. Il ne certifie ni la conformité NATOPS, ni la compatibilité avec une version de DCS World qui n'a pas été testée.

## 1. Objet du projet

DCS-gRPC-lso est un client Rust externe à DCS World. Il découvre les avions joueurs et les navires supportés via DCS-gRPC, détecte une tentative de recovery, échantillonne la télémétrie, interprète les événements de toucher et produit un grade, des rapports et des graphiques.

Le HEAD ajoute deux familles : **CATOBAR** (F/A-18C, F-14, T-45 sur CVN/Forrestal) et **V/STOL** (AV-8B NA sur LHA Tarawa). Sorties : PNG groove/pattern, JSON, SQLite, Discord et ACMI facultatifs, journal et dashboard HTTP.

Le grade automatique reste **expérimental**. Le CATOBAR résume surtout trois gates GS/LU. En V/STOL, la grille A/B/C/D et le bonus de spot sont des conventions du projet non retrouvées dans les NATOPS consultés.

Catégories : **Confirmé par le code**, **Confirmé par la documentation**, **Déduction raisonnable**, **Hypothèse à vérifier**, **Information manquante**.

## 2. Périmètre et limites de l'analyse

### Révision et delta

- HEAD `a25303e`; ancienne base `30f3f4d`; 21 commits nouveaux.
- Migration vers le fork `sevenfifty777/rust-server`, `dcs-grpc-stubs 0.9.0`, commit exact `11aea3484099c2dd21d41a53db2e510f6e5e84c5` (`Cargo.toml:31,37-41`; `Cargo.lock:647-649`).
- Ajout V/STOL, séparation des grades, nouveaux champs de persistance et rendu Tarawa.

### Éléments inspectés

Ensemble de `src/`, manifests, tests, CI, docs, historique Git local, distribution/protobuf/Lua DCS-gRPC 0.9.0, géométrie Tarawa locale, NAVAIR 00-80T-111 (2004), AV-8B NATOPS (2008) et NAVMC 3500.51B Ch.1 officiel USMC (2014).

### Contrôles exécutés

| Contrôle | Résultat |
|---|---|
| `cargo check --bin lso` | Réussi |
| `cargo test --all-targets` | Échec : cinq fixtures ACMI absentes, dix `include_bytes!` en erreur |
| `cargo fmt --all -- --check` | Échec : écarts de formatage |
| `cargo clippy --bin lso -- -D warnings` | Échec : 24 erreurs |
| `git diff --check` | Réussi |
| état Git initial | Propre |

La CI exécute ces contrôles (`.github/workflows/ci.yml:40-50`) et ne peut donc pas être verte au HEAD.

### Limites

- **Information manquante** : build DCS/Dedicated Server, logs DCS/gRPC/LSO, configuration Lua/miz effective et capture brute AV-8B/Tarawa.
- Aucun exemple V/STOL dans les 33 JSON de `trap sample/`.
- 00-80T-111 est officiel, mais la copie accessible est hébergée par un tiers et porte une restriction de diffusion ; aucune copie publique NAVAIR officielle trouvée.
- 00-80T-111 ne démontre pas que plusieurs AV-8B peuvent rester sur 7/7½/8 pendant d'autres recoveries : SOP, Air Boss, deck handling et 00-80T-106 sont aussi requis.
- La fiabilité `RunwayTouch` du Tarawa reste à reproduire sur la build déployée.

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
| `VSTOL.md` | calibration V/STOL |
| `releases/lso.exe` | binaire 0.2.0, version insuffisante pour identifier le commit |

README, `docs/LSO_ANALYSIS.md` et `docs/GRADING_REFERENCE.md` sont partiellement obsolètes : omission/déclaration d'absence AV-8B/Tarawa et parfois 3,5° pour tous les avions, contre 3° dans le code AV-8B.

## 4. Architecture technique

```text
DCS/MSE Lua → DCS-gRPC 0.9.0 (11aea348) → tonic/Tokio
  → full-sync → une tâche par avion × navire
  → Track CATOBAR ou V/STOL → grade → JSON/PNG/DB/Discord/ACMI
```

Le mode live se reconnecte avec backoff jusqu'à 30 s (`run.rs:74-116`) et keepalive HTTP/2 (`run.rs:131-134`). Il crée le **produit cartésien** avions×navires (`run.rs:272-284`) puis écoute les `Birth` (`run.rs:287-355`).

La clé `(plane_id, carrier_id)` sépare les trackers (`run.rs:202-206`), mais `Arrested`/`Vstol` appartient uniquement au navire (`data.rs:286-314`) : aucune compatibilité du couple n'est validée.

Une paire inactive fait deux `GetTransform` toutes les 2 s. Une active ouvre son `StreamEvents` et poll à 10 Hz : environ 30 RPC/s CATOBAR, 20 RPC/s V/STOL. Le rendu PNG reste synchrone dans Tokio (`record_recovery.rs:591-593`).

## 5. Intégration avec DCS-gRPC

Le client et le serveur doivent correspondre au commit `11aea348`, pas seulement au label 0.9.0. Le dépôt embarque DLL/PDB/Lua/protobuf ; leurs hashes doivent être tracés au déploiement.

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

## 6. Modèle de données

`AirplaneInfo` porte hook/landing reference, glide slope et AoA. `AV8BNA` est ajouté avec ID 4, GS 3° et AoA visuel 10–12° (`data.rs:251-284,426-448`).

`CarrierInfo` porte `CarrierRecovery::{Arrested,Vstol}` (`data.rs:286-314`). `LHA_Tarawa` : deck angle 0°, deck 19,98 m, point 7½ `(-3.10,19.95,-64.81)`, axe 27,24 m à bâbord et cible 120 ft (`data.rs:317-340,381-389`). Ce sont des calibrations DCS, pas des constantes NATOPS.

`Track` conserve datums, gates, temps, minimum et outcome. V/STOL ajoute distance/grade de spot. SQLite/JSON ont `spot`, `spot_grade`, `spot_distance_m`, `outcome`, mais pas le navire ni un identifiant de session.

La taxonomie commune contient `Bolter`; un V/STOL qui repart peut donc recevoir ce terme CATOBAR inadapté.

## 7. Cycle de vie complet d'un recovery

1. Connexion et full-sync.
2. Classification avion/navire.
3. Tâche par couple cartésien.
4. Détection toutes les 2 s : <1 100 ft MSL, ≤3,5 NM, >200 m (`detect_recovery_attempt.rs:52-89`).
5. `Track`, `StreamEvents`, échantillonnage 10 Hz.
6. Projection repère navire, gates et datums.
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

## 9. Calculs, unités et repères

- DCS `(x,y,z)` devient est/haut/nord `(z,y,x)` (`transform.rs:76-80`).
- Mètres en interne, NM/ft à certains affichages, radians puis degrés.
- Repère relatif au navire; EMA position (`track.rs:54,249-270`) sans conserver brut+filtré.
- Skew avion/navire non contrôlé : virage/accélération biaisent GS/LU.
- AoA par `acos(forward·normalize(velocity))` sans garde vitesse nulle/clamp : NaN possible (`transform.rs:34-36`).
- Axe Tarawa 27,24 m = interprétation plausible de « one plane width from edge », non chiffre NATOPS.
- Hauteur V/STOL fondée sur MSL/cible 120 ft, pas sur hauteur instantanée du pont.
- Rendu Tarawa remappé (`draw.rs:635-703,927-966`) : schématique, non métrique brut.

## 10. Stockage et sorties

SQLite est mutexé et l'insertion passe par `spawn_blocking`. Les migrations ignorent toutes les erreurs `ALTER TABLE` (`db.rs:93-106`). JSON/DB/dashboard/Discord divergent.

Risques : filenames à la seconde sans navire, collision possible; navire/session/complétude absents; buffers non bornés; dashboard `0.0.0.0` sans auth/TLS; erreurs DB web masquées; tous V/STOL étiquetés `Spot 7.5` (`record_recovery.rs:71-84,493-503,659-672`).

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
10. CI rouge et V/STOL peu testé.

## 12. Analyse LSO US Navy — début des années 2000

Le CATOBAR représente une partie géométrique du groove, pas une observation LSO complète. Il capture gates GS/LU, AoA visuel, toucher/câble et LQM DCS, mais ignore/simplifie tendances, corrections, power, deck motion, start/middle/in-close détaillé, qualité du touchdown, OWO/LSO WO/foul deck et pattern.

Un 3-wire ne justifie pas seul `_OK_`. La règle locale câble 3 + groove 15–18,99 s doit être approuvée comme convention. Seuils et points doivent être versionnés dans une spécification métier signée.

Conclusion : outil de débrief utile, grade autonome non certifié.

## 13. Analyse LSO USMC — AV-8B/LHA, contexte 2004

### Doctrine vérifiée

NAVAIR 00-80T-111 du 1er juillet 2004 :

- fig. 5-2 p.5-3 : spot **7½** = *primary landing spot* LHA, distinct des 7/8;
- §§6.3.2 pp.6-4 à 6-7 : break 800 ft, downwind 600 ft, AoA 10–12°, groove 0,5–0,75 NM/300–350 ft, environ 3°, approche bâbord, hover stop, 120 ft abeam, cross 50 ft au-dessus du pont, tête au-dessus du spot;
- Paddles annonce « Expect spot ___ », puis « Spot ___ »/clearance;
- §§6.3.3/6.5.2 : 7½ primaire de nuit; spots alternatifs possibles pour urgence/sécurité;
- chap. 23/fiches A-5/A-9 : note humaine sur phases/tendances, cross, hover, VL, power, attitude, spot, cap relatif; exemples de spots 7.5, 4, 5, 2.

AV-8B NATOPS 2008 (§§7.6.2–7.6.4) confirme 3°, AoA 10–12°, hover 50–60 ft AGL et contrôle dérive/cap/attitude/taux de descente. Il ne valide pas la formule du code.

### Écarts

- A/B/C/D `<1/<3/<5/≥5 m` et bonus `+1/.75/.5/0` sans source NATOPS (`grading.rs:83-153`);
- aucune note de closure, hover stop, cross, drift, sink, power/nozzle, attitude, heading, heavy WO/foul deck;
- AoA seulement visuel;
- `RunwayTouch` n'est pas une preuve de VL stabilisé;
- gates CATOBAR réutilisées et incomplétude sur-notée.

### Spots successifs/occupation

Plusieurs AV-8B peuvent générer des passes séparées, mais le module ne considère correctement que 7½. Aucun catalogue 7/7½/8, spot assigné, nearest spot, occupation/libération ou foul deck. Un posé correct au 7/8 est déclaré 7.5 et mesuré comme erreur au 7½.

`RunwayTouchEvent` fournit initiator/place, pas le spot assigné. Le spot touché peut être inféré après calibration; le spot demandé exige une entrée externe. Nearest spot seul récompenserait potentiellement le mauvais spot.

La doctrine ne prouve pas la sécurité de plusieurs AV-8B stationnés sur spots adjacents pendant recoveries : arbitrage SOP/Air Boss/LSO/00-80T-106 requis.

## 14. Analyse DCS World et DCS-gRPC

- Tarawa exige groupe `Ship`, type exact `LHA_Tarawa` et attribut carrier (`run.rs:381-409`); à vérifier live.
- `RunwayTouch` est dépendant de version; tester VL, rolling landing, bounce, touch-and-go et `place.unit`.
- Installation partielle : MSE peut fournir transforms/events tandis que Hook/Net manque identité/mission.
- `throughputLimit=600` est un plafond, pas une fraîcheur garantie (`grpc-mission.lua:31-33`; `grpc.lua:203-235`).
- Pause, accélération et mission restart ne sont pas structurés par session.
- names, slots, player names et UCID ne sont pas équivalents/uniques.
- update/repair peut écraser l'installation Lua; sandbox/exposition réseau à auditer.

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

## 18. Cas multijoueur et scénarios limites

Les bonnes paires sont indépendantes et `RunwayTouch` filtre `plane.id`+`carrier.id`; SQLite sérialise. Deux posés simultanés sur navires éloignés sont **probablement supportés**.

Le conflit critique est cartésien :

| Couple | Traitement actuel | Effet |
|---|---|---|
| AV-8B×Tarawa | V/STOL attendu | vrai rapport |
| CATOBAR×CVN | arrêté attendu | vrai rapport |
| AV-8B×CVN | faux arrêté | hook/câble absurdes |
| CATOBAR×Tarawa | faux V/STOL | faux Spot 7.5 |

Si les enveloppes 3,5 NM se recouvrent, faux trackers et WO/bolters/artefacts parasites. Même éloignées, surcharge RPC. Une erreur de tâche peut relancer globalement et interrompre l'autre recovery.

Autres limites : UCID/homonymes, filename simultané, slot changes, unités tardives, rebonds, hover long, coupure, virage navire, restart mission.

## 19. Écarts, anomalies et zones de risque

| Anomalie | Origine probable | Confiance |
|---|---|---|
| couples incompatibles | implémentation Rust | Très élevée |
| grade positif incomplet | Rust + modèle métier | Très élevée |
| Bolter V/STOL | implémentation Rust | Élevée |
| Birth non enrichi | implémentation Rust | Très élevée |
| session/skew/deadline absents | réseau/temporalité + Rust | Très élevée |
| collision fichiers | implémentation Rust | Très élevée |
| CI non reproductible | dépôt/configuration | Très élevée |
| ancienne trajectoire CATOBAR erronée | visualisation | Élevée, filtre x inchangé |

Écarts métier : grille V/STOL non NATOPS; 7½ forcé; pas d'occupation; modèle hover/cross incomplet; CATOBAR réduit aux gates; AoA/énergie/tendances hors note.

À reproduire : RunwayTouch Tarawa, descriptor Tarawa, géométrie 7/7½/8/axe, règles foul deck, seuils de fraîcheur, autorité LQM/algorithme/LSO.

## 20. Tests existants et stratégie future

Les tests couvrent quelques points/limites de spot, outcomes et DB, pas le chemin DCS, repères complets, incomplétude, couples croisés, multi-spots, simultanéité, reconnexion ou rendu. Fixtures absentes = suite non compilable.

Priorités :

1. CVN+Tarawa éloignés puis <3,5 NM; Hornet+AV-8B simultanés; exactement 2 rapports.
2. AV-8B avec 0/1/2/3 gates; aucun grade favorable incomplet.
3. 7/7½/8 avec spot assigné distinct du nearest.
4. 2/3 AV-8B successifs restant au pont, taxi/takeoff/libération.
5. VL, rolling, bounce, double touch, touch-and-go, taxi >150 m.
6. skew 0/50/200/500 ms; Tarawa en virage.
7. coupure gRPC 0,2/1/3/10 s pendant deux recoveries; restart.
8. Birth avion puis navire et inverse.
9. collision filename à la seconde.
10. benchmark 1/5/10/20 joueurs×2 navires.
11. calibration spots et point avion-sol sous attitude navire.
12. validation aveugle par LSO USN/USMC.

## 21. Questions ouvertes

1. Build DCS, hash gRPC 0.9.0 et config Lua déployés ?
2. AV-8B uniquement Tarawa ? AV-8B V/STOL sur CVN exclu ou futur ?
3. Spots à supporter et source de leur affectation ?
4. Politique occupation/foul deck selon quelle SOP ?
5. Quelle édition 00-80T-106 et adaptations communautaires ?
6. LSO humain autorité et grade logiciel expérimental ?
7. Captures live AV-8B/Tarawa disponibles ?
8. Seuil `Incomplete/TelemetryGap` ?
9. Budgets CPU/RAM/RPC/FPS ?
10. Contraintes compatibilité JSON/DB/dashboard ?

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

- [NAVAIR 00-80T-111, 1 Jul 2004 — miroir public](https://feral-hogs.com/Downloads/NATOPS%2000-80T-111%20VSTOL%20Shipboard%20%26%20LSO%20Manual%20Jul%202004%20pp148.pdf)
- [A1-AV8BB-NFM-000 AV-8B/TAV-8B, 15 Mar 2008 — miroir public](https://info.publicintelligence.net/AV-8B-000.pdf)
- [NAVMC 3500.51B Ch.1 — source officielle USMC](https://www.marines.mil/Portals/1/NAVMAC%203500.51B%20W%20CH%201.pdf)

## Synthèse — dix points avant toute modification

1. Exiger DCS-gRPC fork 0.9.0 commit `11aea348`, pas l'ancien 0.8.1.
2. AV-8B/Tarawa existe mais reste expérimental et sans corpus live local.
3. Corriger d'abord le produit cartésien sans compatibilité.
4. Deux touchers simultanés sont isolés par IDs, mais faux trackers/charge/erreur globale peuvent interférer.
5. Un seul 7½ existe : aucun vrai support 7/8, affectation ou occupation.
6. NATOPS confirme 7½ primaire et spots assignés/alternatifs, pas la grille A-D ni stationnement simultané adjacent.
7. Zéro gate peut donner `_OK_`/5 : rendre les données incomplètes explicitement insuffisantes.
8. Valider RunwayTouch, doubles contacts et départ après toucher sur Dedicated Server; éviter Bolter V/STOL.
9. StreamUnits est trop lent pour le 10 Hz; cache partagé et instrumentation sont prioritaires.
10. CI rouge, session/skew/reconnexion non maîtrisés et sorties sans navire/complétude.

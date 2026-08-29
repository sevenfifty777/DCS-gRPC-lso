# Préparation et arbitrages avant développement

> Document utilisateur associé à `.ignore/analyse-details.md`, révisé le 28 août 2026 pour le HEAD `a25303e` et le support mixte CVN/CATOBAR + Tarawa/AV-8B.  
> Hypothèse de travail : Codex prend intégralement en charge l'analyse technique détaillée, l'implémentation, les tests automatisés, la documentation et la préparation du déploiement. Les actions ci-dessous sont celles qui nécessitent une information, une décision, un accès ou une validation de votre part.

## Mode d'emploi

- Les éléments marqués **Bloquant** doivent être réglés avant de modifier les règles métier ou l'intégration DCS.
- Les éléments **Fortement recommandés** peuvent être menés en parallèle des premiers correctifs purement techniques.
- Les éléments **Optionnels** améliorent la qualité du résultat mais ne doivent pas retarder les corrections urgentes.
- Pour chaque arbitrage, une recommandation est proposée. Vous pouvez simplement l'accepter ou indiquer une autre décision.
- Ne transmettez aucun secret dans ce dépôt : token Discord, mot de passe DCS-gRPC, adresse publique, UCID non anonymisé ou donnée personnelle.

## 1. Préserver l'état actuellement déployé — Bloquant

- [ ] Identifier le binaire réellement exécuté sur le serveur : chemin, nom, taille, date et version affichée par `lso.exe --version`.
- [ ] Calculer son SHA-256 et le conserver avec la date du relevé.
- [ ] Noter le commit Git supposé correspondre au binaire. Le dépôt local analysé est désormais sur `a25303e`, mais il ne faut pas présumer que le serveur exécute ce build.
- [ ] Sauvegarder de façon récupérable :
  - le binaire déployé ;
  - sa commande ou son script de lancement ;
  - les paramètres de service éventuels ;
  - `lso.db` ;
  - les fichiers JSON/PNG/ACMI représentatifs ;
  - la configuration DCS-gRPC ;
  - les journaux utiles.
- [ ] Définir une procédure de retour arrière : qui l'exécute, quel binaire restaurer, où se trouve la sauvegarde et combien de temps l'opération prend.
- [ ] Choisir une fenêtre de maintenance ou un environnement de préproduction qui n'affecte pas les missions normales.

Livrable attendu : un court fichier texte ou message avec version, hash, commande de lancement, emplacement des données, responsable et procédure de rollback.

## 2. Documenter précisément l'environnement — Bloquant

Fournir les valeurs suivantes :

- [ ] version exacte de DCS World Dedicated Server, avec numéro de build ;
- [ ] branche DCS utilisée, si plusieurs canaux existent dans votre installation ;
- [ ] version exacte de DCS-gRPC réellement installée, avec hash des DLL/Lua ; le code attend le tag officiel du fork `v0.9.0`, verrouillé au commit `5bd6d6e42491c8697a5c5a95e80a2e689923bd3b` ;
- [ ] version de Windows Server et architecture ;
- [ ] type d'exécution de LSO : console, tâche planifiée, service, wrapper ou autre ;
- [ ] commande complète de lancement de LSO, en masquant les secrets ;
- [ ] répertoire de travail et répertoire de sortie ;
- [ ] paramètres DCS-gRPC non secrets : adresse d'écoute, port, `throughputLimit`, autostart et mode d'installation ;
- [ ] nombre habituel et maximal de joueurs simultanés ;
- [ ] nombre habituel et maximal de CVN, Forrestal et LHA Tarawa présents dans une mission ;
- [ ] rotation/rechargement automatique des missions et sa fréquence ;
- [ ] autres scripts ou frameworks actifs : MOOSE, MIST, Skynet, AIRBOSS, SLmod, hooks maison, etc.
- [ ] baseline actuelle pendant mission calme et chargée : CPU/RAM de DCS, DCS-gRPC et LSO, FPS serveur, disque et réseau ;
- [ ] indiquer si LSO tourne sur la machine DCS ou sur une machine séparée ;
- [ ] indiquer les budgets maximaux acceptables de CPU, RAM, disque et latence.

À joindre si possible, après suppression des secrets :

- [ ] `dcs-grpc.lua` ;
- [ ] les lignes modifiées de `MissionScripting.lua` ;
- [ ] script/service de lancement de LSO ;
- [ ] extrait pertinent de `dcs.log` et `grpc.log` couvrant démarrage, mission et au moins un trap.

## 3. Définir le périmètre matériel et logiciel — Bloquant

- [ ] Lister les appareils à supporter lors de la première version corrigée : F/A-18C, F-14A/B/B(U), T-45, AV-8B NA ou autres.
- [ ] Lister les carriers réellement employés, avec type DCS exact : CVN-71/72/73/74/75, Forrestal, `LHA_Tarawa` ou autres.
- [ ] Indiquer les mods communautaires, leur version et leur caractère obligatoire ou facultatif.
- [ ] Préciser si les appareils IA doivent être notés ou seulement les joueurs humains.
- [ ] Préciser si plusieurs carriers peuvent être actifs et suffisamment proches pour rendre le carrier visé ambigu.
- [ ] Préciser si un CVN et le Tarawa peuvent conduire des recoveries simultanées et à quelle distance minimale ils opèrent.
- [ ] Préciser si l'AV-8B doit être accepté uniquement sur Tarawa, ou aussi en V/STOL sur CVN ; il ne doit jamais être traité implicitement comme CATOBAR.
- [ ] Préciser si le mode replay ACMI est utilisé opérationnellement ou seulement pour le développement.

### Arbitrage A — Périmètre de la première livraison

**Recommandation :** limiter la première livraison aux appareils et carriers déjà réellement utilisés sur le serveur, puis élargir après stabilisation.

- [ ] Accepté
- [ ] Refusé — périmètre demandé : `______________________________`

## 4. Constituer un corpus de cas réels — Bloquant

Fournir au minimum :

- [ ] une passe jugée correcte et correctement traitée ;
- [ ] une passe pour chaque comportement actuellement jugé non conforme ;
- [ ] si possible, un trap sur chaque fil ;
- [ ] un bolter ;
- [ ] un touch-and-go si ce cas existe dans vos missions ;
- [ ] un waveoff initié par le pilote ;
- [ ] un waveoff ordonné par le LSO, si distinguable ;
- [ ] un changement de slot ou respawn suivi d'un trap ;
- [ ] une rotation/recharge de mission suivie d'un trap ;
- [ ] deux approches simultanées si ce scénario est réaliste ;
- [ ] si disponible, une passe affectée par une coupure ou un ralentissement DCS-gRPC ;
- [ ] un test contrôlé où la connexion client ↔ DCS-gRPC est interrompue brièvement pendant le groove.
- [ ] un VL AV-8B nominal sur Tarawa, avec capture simultanée des événements `Land` et `RunwayTouch` et de leur champ `place` ;
- [ ] un rolling landing, un rebond/double contact et un touch-and-go AV-8B ;
- [ ] un posé AV-8B connu sur chacun des spots 7, 7½ et 8, avec le spot réellement assigné noté séparément ;
- [ ] deux ou trois AV-8B successifs restant au pont, puis taxi/décollage, afin d'étudier occupation et libération des spots ;
- [ ] Hornet/CVN et AV-8B/Tarawa simultanés, d'abord navires éloignés puis enveloppes de 3,5 NM chevauchantes ;
- [ ] apparition tardive avion puis Tarawa, puis ordre inverse, après démarrage de LSO ;
- [ ] dump `GetDescriptor` du Tarawa sur le Dedicated Server réellement utilisé ;
- [ ] pour chaque essai Tarawa : build DCS, hash DCS-gRPC, temps DCS, logs bruts et outcome attendu par le LSO USMC.
- [ ] un waveoff avant passage au-dessus du pont avec la nouvelle version ;
- [ ] un bolter crosse baissée ;
- [ ] un touch-and-go/qualification crosse relevée près du pont ;
- [ ] une passe avec crosse relevée dans le pattern puis baissée dans le groove ;
- [ ] un essai de l'argument 25 en erreur ou indisponible pour chaque module.

Pour chaque cas, fournir autant que possible :

- heure et date précises ;
- nom anonymisé mais stable du pilote ;
- type avion et carrier ;
- résultat humain attendu ;
- résultat produit par LSO ;
- PNG, JSON et ACMI correspondants ;
- extrait synchronisé de `dcs.log`, `grpc.log` et log LSO ;
- timestamps DCS et muraux avant, pendant et après tout trou de connexion ;
- courte explication : « observé », « attendu » et raison.

Les cinq fixtures ACMI attendues par `src/tests.rs` sont absentes. Si vous les possédez :

- [ ] les restaurer ou les fournir hors Git pour anonymisation ;
- [ ] confirmer que leur licence et leur contenu permettent de les conserver comme tests ;
- [ ] indiquer si les noms/joueurs/coordonnées doivent être anonymisés.

Le dossier `trap sample/` ajouté par l'upstream constitue un corpus exploratoire. Il faut encore :

- [ ] confirmer que ces données peuvent être utilisées et conservées ;
- [ ] anonymiser les noms si nécessaire ;
- [ ] fournir l'outcome humain attendu pour chaque passe ;
- [ ] compléter par événements/transforms/logs bruts, absents des JSON actuels.

### Format conseillé pour signaler une anomalie

```text
Identifiant : CASE-001
Date/heure :
Version DCS / DCS-gRPC / LSO :
Mission :
Appareil / carrier :
Pilote anonymisé :
Événement : trap / bolter / WO / autre
Résultat observé :
Résultat attendu :
Pourquoi il est attendu :
Fichiers joints :
Reproductible : toujours / parfois / une fois
```

## 5. Choisir l'autorité de référence pour la notation — Bloquant

Le code calcule actuellement un grade Rust distinct du texte `LandingQualityMark` fourni par DCS. Aucun des deux ne doit devenir implicitement l'autorité sans décision.

### Arbitrage B — Source du grade officiel affiché

**Recommandation :** conserver séparément trois champs : résultat factuel (`wire/bolter/WO`), notation DCS brute et grade calculé par l'application. Tant que l'algorithme n'est pas validé, afficher le grade calculé comme « expérimental ».

- [ ] Recommandation acceptée
- [ ] DCS fait autorité
- [ ] L'algorithme Rust fait autorité dès la prochaine version
- [ ] Un LSO humain fait autorité, l'application ne propose qu'une aide
- [ ] Autre : `______________________________`

### Arbitrage C — Doctrine de référence

Fournir ou désigner explicitement :

- [ ] document de référence, titre et édition/date ;
- [ ] appareils auxquels il s'applique ;
- [ ] adaptations propres à votre escadrille/communauté ;
- [ ] personne habilitée à trancher une ambiguïté LSO ;
- [ ] autorisation d'utiliser des sources communautaires lorsque la source primaire ne couvre pas DCS.

**Recommandation :** versionner une courte spécification métier interne approuvée par votre LSO, même si elle dérive d'un document plus vaste. Le code et les tests viseront cette spécification, pas une formule vaguement qualifiée de « NAVAIR ».

- [ ] Accepté
- [ ] Une autre source doit faire foi : `______________________________`

## 6. Définir ce qui doit être évalué — Bloquant

### Arbitrage D — Portée de la notation

**Recommandation :** première étape = noter uniquement le groove et présenter le Case I pattern comme visualisation non notée. Ajouter la notation du pattern dans une phase distincte après définition de critères vérifiables.

- [ ] Groove uniquement pour la première livraison
- [ ] Groove + Case I complet immédiatement
- [ ] Aucune note automatique, visualisation seulement
- [ ] Autre : `______________________________`

### Arbitrage E — Données absentes ou incomplètes

Le comportement actuel peut assimiler des gates absents à zéro écart et produire `OK`.

**Recommandation :** ne jamais attribuer `OK`, `(OK)` ou `_OK_` si le minimum de données requis n'est pas présent. Produire un état distinct `Incomplete/Insufficient data`, sans points.

- [ ] Accepté
- [ ] Conserver `--` pour les données incomplètes
- [ ] Conserver le comportement actuel
- [ ] Autre : `______________________________`

### Arbitrage F — Observation continue ou gates seulement

**Recommandation :** conserver les gates pour le résumé, mais analyser toute la trajectoire du groove afin de détecter excursions, tendances et corrections entre les gates.

- [ ] Accepté
- [ ] Gates seulement
- [ ] Observation continue sans notion de gates
- [ ] À décider après prototype comparatif

### Arbitrage G — AoA et énergie dans le grade

**Recommandation :** intégrer l'AoA après validation par appareil et ne pas inférer la puissance tant qu'aucune donnée fiable n'est disponible. Afficher une confiance ou un état « unavailable » plutôt que fabriquer une mesure.

- [ ] Accepté
- [ ] AoA reste uniquement visuelle
- [ ] AoA et estimation de puissance doivent entrer immédiatement dans le grade
- [ ] Autre : `______________________________`

## 7. Définir les outcomes et cas spéciaux — Bloquant

### Arbitrage H — Waveoffs

Le code ne peut actuellement pas prouver qui a initié la remise de gaz.

**Recommandation :** utiliser temporairement un outcome neutre `Waveoff/Go-around — initiateur inconnu`. Ne distinguer OWO, WO LSO, foul-deck ou pattern waveoff que si une donnée explicite et testable est ajoutée.

- [ ] Accepté
- [ ] Toute remise de gaz doit rester `WaveoffPilot`
- [ ] Une saisie LSO externe sera disponible pour distinguer les catégories
- [ ] Une intégration DCS spécifique doit être développée
- [ ] Autre : `______________________________`

Préciser les catégories et points souhaités :

| Outcome | Doit être distingué ? | Label souhaité | Points | Autorité de détection |
|---|---|---|---|---|
| Own waveoff |  |  |  |  |
| LSO waveoff |  |  |  |  |
| Foul-deck waveoff |  |  |  |  |
| Pattern waveoff |  |  |  |  |
| Bolter |  |  |  |  |
| Touch-and-go |  |  |  |  |
| Qualif Bolter |  |  |  |  |
| Trap |  |  |  |  |
| Données insuffisantes |  |  |  |  |

### Arbitrage I — Détection du touchdown et du bolter

**Recommandation :** traiter les événements DCS comme indices corrélés avec la télémétrie, et non dépendre exclusivement d'un seul `RunwayTouch`. Conserver l'événement brut et un niveau de confiance.

- [ ] Accepté
- [ ] `RunwayTouch` reste l'unique autorité
- [ ] `LandingQualityMark` reste l'unique autorité
- [ ] Autre : `______________________________`

- [ ] Valider l'argument DCS 25 et sa polarité pour F/A-18C, F-14A, F-14B, F-14B(U) et T-45.
- [ ] Décider si la crosse doit être évaluée au deck crossing/touchdown, sur une fenêtre finale ou pendant tout le groove.
- [ ] Refuser qu'un simple minimum de distance soit considéré comme preuve de survol du pont.

### Arbitrage J — Autorité du numéro de câble

**Recommandation :** conserver `wire_dcs` et `wire_estimated` séparément, signaler les divergences, et utiliser DCS pour l'affichage principal seulement après validation de sa fiabilité par module/carrier.

- [ ] Accepté
- [ ] DCS fait toujours autorité
- [ ] Géométrie fait toujours autorité
- [ ] Le LSO humain corrige le fil
- [ ] Autre : `______________________________`

### Arbitrage K — `_OK_` / « Unicorn »

**Recommandation :** désactiver ce bonus tant que la règle fil 3 + 15–18,99 s n'est pas formellement approuvée dans votre spécification métier.

- [ ] Désactiver temporairement
- [ ] Conserver la règle actuelle
- [ ] Remplacer par la règle suivante : `______________________________`

## 8. Identité joueur et données personnelles — Bloquant pour le multijoueur

### Arbitrage L — Identifiant persistant

**Recommandation :** employer l'UCID comme clé interne, le capturer au Birth/PlayerEnterUnit/changement de slot dans un registre de session et conserver le display name comme attribut modifiable. La recherche post-passe par égalité du nom ajoutée par l'upstream ne suffit pas. Ne pas exposer l'UCID dans JSON public, dashboard ou Discord.

- [ ] UCID disponible et usage autorisé
- [ ] UCID indisponible ; utiliser player name avec limites acceptées
- [ ] Un identifiant interne pseudonymisé sera fourni
- [ ] Autre : `______________________________`

- [ ] Définir qui a accès aux identifiants, logs et historiques.
- [ ] Fixer la durée de conservation des données.
- [ ] Définir si un pilote peut demander correction/suppression.
- [ ] Confirmer les règles d'anonymisation des fixtures de test.
- [ ] Confirmer si le dashboard est privé, authentifié ou exposé sur le réseau.

## 9. Compatibilité des données et interfaces — Bloquant avant changement de schéma

### Arbitrage M — Compatibilité descendante

Les consommateurs connus sont SQLite, JSON, dashboard, Discord et éventuellement des outils externes.

**Recommandation :** préserver les champs existants, ajouter des champs versionnés et fournir une migration SQLite non destructive. Ne changer le sens d'un champ existant qu'avec version de schéma explicite.

La mise à jour ajoute `pilot_ucid`, `aircraft_id`, `mission_datetime` et `outcome` en SQLite/dashboard, retire `esf_pilot_name` des requêtes courantes et n'expose pas tous ces champs dans le JSON. Il faut confirmer la compatibilité des consommateurs et décider si F-14A/B doivent partager `aircraft_id = 2` tandis que F-14B(U) utilise 3.

- [ ] Compatibilité stricte requise
- [ ] Ajouts compatibles et migration autorisés — recommandé
- [ ] Rupture acceptée avec migration/reconstruction
- [ ] Aucun consommateur externe à préserver

Lister les consommateurs externes et responsables :

| Consommateur | Format/API utilisé | Compatibilité exigée | Contact |
|---|---|---|---|
| Dashboard actuel | SQLite/API web |  |  |
| Discord | webhook |  |  |
| Outil externe |  |  |  |

### Arbitrage N — Conservation des artefacts

**Recommandation :** conserver JSON et DB systématiquement ; rendre ACMI configurable ; conserver les PNG pour le débrief ; définir une politique de rotation par âge/taille.

- [ ] Accepté
- [ ] Politique différente : `______________________________`
- [ ] Durée de rétention : `______________________________`
- [ ] Taille disque maximale : `______________________________`

## 10. Déploiement, sécurité et exploitation — Bloquant avant mise en production

- [ ] Désigner un environnement de test ou une mission dédiée.
- [ ] Fournir un moyen sûr d'obtenir les logs après essai.
- [ ] Définir qui peut redémarrer LSO, DCS-gRPC et DCS.
- [ ] Définir les créneaux où une rotation de mission ou un restart est acceptable.
- [ ] Vérifier que le port DCS-gRPC n'est pas exposé publiquement sans authentification/filtrage.
- [ ] Vérifier que le dashboard n'est pas exposé par défaut au-delà du réseau prévu.
- [ ] Conserver les secrets hors Git et hors fichiers joints.
- [ ] Définir la supervision minimale : processus vivant, connexion gRPC, fin de stream, nombre de trackers, erreurs de DB et espace disque.

### Arbitrage O — Stratégie de livraison

**Recommandation :** déploiement progressif : replay/tests → mission de test → serveur en mode observation parallèle → activation officielle avec rollback prêt.

- [ ] Accepté
- [ ] Déploiement direct sur serveur principal accepté
- [ ] Autre processus : `______________________________`

### Arbitrage P — Politique en cas d'erreur d'une passe

Aujourd'hui, certaines erreurs locales peuvent provoquer une reconnexion globale.

**Recommandation :** isoler l'échec à la passe concernée lorsque possible, journaliser clairement, garder le service disponible et réserver le restart global aux erreurs de session/connexion.

- [ ] Accepté
- [ ] Toute erreur doit redémarrer le client
- [ ] Autre : `______________________________`

## 11. Critères d'acceptation — Bloquant avant validation finale

Définir les objectifs mesurables. Valeurs recommandées à accepter ou modifier :

- [ ] aucune attribution de grade positif avec données insuffisantes ;
- [ ] aucun doublon de trap après respawn, reconnexion ou rotation de mission ;
- [ ] attribution au bon pilote après changement de slot ;
- [ ] aucune perte sur le corpus nominal fourni ;
- [ ] résultat live et replay identique pour les informations disponibles dans les deux modes ;
- [ ] câble DCS/estimé tous deux conservés et divergence visible ;
- [ ] explication machine-readable de chaque grade : données utilisées, seuils et raison ;
- [ ] suite `cargo test` entièrement verte ;
- [ ] `cargo fmt --check` vert ;
- [ ] Clippy sans avertissement nouveau pertinent ;
- [ ] aucune régression de lecture des anciennes lignes SQLite/JSON selon l'arbitrage M ;
- [ ] CPU/RAM/disque/réseau respectent les budgets de l'arbitrage S ;
- [ ] nombre de streams et appels gRPC documenté avant/après optimisation ;
- [ ] aucun impact mesurable inacceptable sur les FPS/temps de simulation DCS ;
- [ ] waveoff avant le pont non classé Bolter ;
- [ ] Qualif Bolter impossible sur la seule base d'une crosse relevée plus tôt dans le pattern ;
- [ ] erreur de lecture de crosse visible et jamais remplacée silencieusement par un état valide ;
- [ ] skew avion/carrier mesuré et soumis au seuil choisi ;
- [ ] EMA validée sur carrier rectiligne, en virage et en accélération ;
- [ ] UCID correct avec homonymes, leave, respawn et changement de slot ;
- [ ] charge acceptable avec `____` joueurs et `____` carriers ;
- [ ] reconnexion après changement de mission en moins de `____` secondes ;
- [ ] détection d'un canal silencieux ou d'une donnée périmée en moins de `____` secondes ;
- [ ] aucune note positive lorsqu'un trou dépasse le seuil accepté pendant le groove ;
- [ ] aucun raccord silencieux de deux fragments après reconnexion ;
- [ ] chaque rapport expose la complétude, le plus grand intervalle entre samples et les événements manquants ;
- [ ] aucune tâche n'est créée pour une paire avion–navire incompatible ;
- [ ] deux recoveries simultanées Hornet/CVN et AV-8B/Tarawa produisent exactement deux rapports, même si les enveloppes se chevauchent ;
- [ ] chaque rapport persiste le navire, son type, le mode de recovery et l'identifiant de session DCS ;
- [ ] un AV-8B avec zéro, une ou deux gates manquantes ne peut pas recevoir un grade favorable ;
- [ ] un V/STOL touch-and-go, rebond ou taxi rapide n'est jamais nommé `Bolter` par défaut ;
- [ ] `intended_spot`, `actual_nearest_spot` et la distance au spot attendu sont distingués lorsque le multi-spots est activé ;
- [ ] les spots 7, 7½ et 8 sont validés par mesures live avant notation ;
- [ ] occupation/libération/foul deck suit la décision W et ne repose pas sur une référence DCS devenue invalide ;
- [ ] le client détecte ou journalise la version DCS-gRPC et les changements de session ;
- [ ] retour arrière exécutable en moins de `____` minutes.

### Arbitrage Q — Validation métier finale

**Recommandation :** un LSO désigné compare en aveugle un corpus de passes et signe la spécification/résultat avant que le grade calculé ne soit présenté comme officiel.

- [ ] Accepté — validateur : `______________________________`
- [ ] Validation communautaire collective
- [ ] Pas de validation humaine requise
- [ ] Autre : `______________________________`

### Arbitrage R — Trous de télémétrie et reprise après coupure

Une coupure momentanée peut perdre des transforms ou des événements sans qu'il soit possible de reconstruire fidèlement la portion manquante.

**Recommandation :** mesurer la fraîcheur et la continuité de chaque passe ; au-delà d'un seuil configurable, conserver la trace et le diagnostic mais classer la passe `Incomplete/TelemetryGap`, sans points. Ne jamais raccorder silencieusement deux fragments comme si la télémétrie avait été continue.

- [ ] Recommandation acceptée
- [ ] Rejeter entièrement la passe et ne rien conserver
- [ ] Continuer et calculer le grade malgré le trou
- [ ] Autoriser une validation/correction manuelle par le LSO
- [ ] Autre : `______________________________`

Seuil initial proposé pour expérimentation :

- avertissement si intervalle entre samples > `300 ms` (trois périodes nominales) ;
- grade automatiquement incomplet si intervalle > `1 000 ms` pendant le groove ;
- toute perte de `RunwayTouch`/LQM reste signalée séparément ;
- seuils définitifs à fixer après mesure sur le serveur.

- [ ] Seuils expérimentaux acceptés
- [ ] Seuils demandés : warning `____ ms`, incomplete `____ ms`
- [ ] Aucun seuil avant campagne de mesure

### Arbitrage S — Budget de ressources et lieu d'exécution

**Recommandation :** mesurer d'abord, centraliser les flux et réduire les RPC avant de diminuer la précision. Si possible, exécuter LSO sur une machine séparée du serveur DCS, sur un réseau local protégé.

- [ ] Recommandation acceptée
- [ ] LSO doit rester sur la machine DCS
- [ ] Une machine séparée est disponible
- [ ] À décider après benchmark

Budgets à compléter : CPU LSO `____ %`, RAM `____ MiB`, disque/jour `____ GiB`, latence p95 `____ ms`, joueurs `____`, carriers `____`.

Le benchmark doit intégrer le coût actuel : environ `P×C` RPC/s hors passe, **30 RPC/s par paire CATOBAR active**, **20 RPC/s par paire V/STOL active**, plus un `StreamEvents` par paire active. Les paires incompatibles consomment aussi des ressources tant qu'elles ne sont pas filtrées.

### Arbitrage T — Qualif Bolter et touch-and-go

**Recommandation :** conserver un outcome distinct seulement si la crosse est confirmée relevée dans une fenêtre proche du pont. Autoriser un grade d'approche, mais interdire `_OK_` faute de trap confirmé ; une donnée de crosse absente doit produire `Unknown`, jamais une valeur par défaut favorable.

- [ ] Recommandation acceptée
- [ ] Qualif Bolter reçoit les mêmes grades qu'un trap, `_OK_` compris
- [ ] Qualif Bolter reçoit toujours B/2,5
- [ ] Ne pas distinguer Qualif Bolter et touch-and-go
- [ ] Désactiver cette détection en attendant sa validation
- [ ] Autre : `______________________________`

### Arbitrage U — Lissage et position carrier

**Recommandation :** conserver position brute et filtrée dans le diagnostic, ne pas considérer l'EMA actuelle comme vérité, puis comparer EMA et extrapolation position+vitesse+timestamp. Refuser un grade si le skew avion/carrier dépasse le seuil de l'arbitrage R.

- [ ] Recommandation acceptée
- [ ] Conserver l'EMA 0,15 sans changement
- [ ] Revenir immédiatement à la position brute
- [ ] À décider après benchmark sur carrier en ligne droite, virage et accélération

### Arbitrage V — Matrice de compatibilité avion–navire–recovery

Le code crée actuellement toutes les paires avion×navire : AV-8B/CVN est traité CATOBAR et Hornet/Tomcat/T-45/Tarawa comme V/STOL.

**Recommandation :** filtrage strict avant création des tâches : `AV8BNA ↔ LHA_Tarawa/VSTOL` et avions à crosse ↔ carriers `Arrested`. Tout autre couple doit être explicitement configuré et testé.

- [ ] Recommandation acceptée
- [ ] Prévoir aussi AV-8B V/STOL sur CVN : `______________________________`
- [ ] Autre matrice : `______________________________`

### Arbitrage W — Spots Tarawa, affectation et occupation

**Recommandation :** distinguer au minimum `intended_spot`, `actual_nearest_spot` et erreur au spot assigné. Calibrer les spots réellement employés, et ne pas déduire l'affectation depuis le seul nearest spot.

- [ ] Spots à supporter : `7 / 7½ / 8 / autres : __________`
- [ ] Source de l'affectation : interface LSO / mission flag / configuration / autre `__________`
- [ ] Une passe au bon nearest spot mais au mauvais intended spot doit être signalée comme erreur
- [ ] Une politique d'occupation/libération/foul deck est requise
- [ ] La gestion d'occupation est hors périmètre de la première livraison
- [ ] Autorité SOP/Air Boss/LSO désignée : `______________________________`

### Arbitrage X — Autorité et grille du grade V/STOL

La doctrine 2004 décrit un jugement humain par phases/tendances, hover, cross, VL, power, attitude, spot et cap relatif. Elle ne justifie pas les seuils A/B/C/D ni le bonus actuel.

**Recommandation :** conserver temporairement le score actuel comme métrique expérimentale séparée, interdire tout grade positif incomplet et rédiger une spécification USMC validée avant de l'appeler note LSO.

- [ ] Recommandation acceptée
- [ ] Le score actuel A/B/C/D devient officiel malgré l'absence de source NATOPS
- [ ] Visualisation V/STOL sans note automatique dans un premier temps
- [ ] Autre grille fournie/validée : `______________________________`

### Arbitrage Y — Version DCS-gRPC de référence

**Recommandation :** figer client, DLL et Lua sur le tag `v0.9.0` et le commit verrouillé `5bd6d6e…`, enregistrer leurs hashes et refuser le démarrage ou avertir fortement si version/session attendue non vérifiable.

- [ ] Recommandation acceptée
- [ ] Une autre version doit être supportée : `______________________________`
- [ ] Mise à niveau serveur autorisée dans l'environnement de test

### Arbitrage Z — Usage de `MissionService.StreamUnits`

`StreamUnits` est documenté à faible fréquence, avec `poll_rate` entier en secondes. Il ne remplace pas la télémétrie active 10 Hz.

**Recommandation :** conserver `GetTransform` pour une recovery active; prototyper éventuellement `StreamUnits` pour découverte/préfiltrage et comparer surtout un cache partagé par unité.

- [ ] Recommandation acceptée
- [ ] Prototype comparatif StreamUnits/cache partagé demandé
- [ ] Ne pas travailler sur StreamUnits à ce stade

## 12. Ordre conseillé des actions utilisateur

### Phase 0 — À faire avant tout développement fonctionnel

- [ ] sauvegarde, hash du binaire et rollback ;
- [ ] inventaire des versions/configurations ;
- [ ] choix du périmètre appareils/carriers ;
- [ ] fourniture d'au moins un cas nominal et un cas non conforme ;
- [ ] arbitrages B à K et R à Z sur la notation, les outcomes, la télémétrie, les ressources, le carrier, la compatibilité et le V/STOL ;
- [ ] choix de compatibilité des données ;
- [ ] environnement et méthode de test.

### Phase 1 — Pendant les correctifs techniques indépendants du métier

- [ ] compléter le corpus de captures ;
- [ ] récupérer/anonymiser les fixtures ACMI ;
- [ ] fournir la doctrine et faire approuver la spécification LSO ;
- [ ] décider l'identité persistante et la politique de données ;
- [ ] fixer les critères de charge et de reconnexion.
- [ ] valider les nouveaux contrats SQLite/JSON/dashboard et la taxonomie F-14.
- [ ] valider les contrats multi-carrier (`carrier_id/type`, `recovery_mode`, `session_id`) et multi-spots.

### Phase 2 — Avant préproduction

- [ ] approuver les migrations de données ;
- [ ] préparer supervision et collecte de logs ;
- [ ] confirmer la fenêtre de test ;
- [ ] exécuter la matrice multijoueur/DCS ;
- [ ] valider le rapport de comparaison ancien/nouveau.

### Phase 3 — Avant production

- [ ] validation LSO ;
- [ ] validation des critères d'acceptation ;
- [ ] sauvegarde immédiatement avant déploiement ;
- [ ] test de rollback ;
- [ ] décision explicite Go/No-Go et responsables présents.

## 13. Ce que vous n'avez pas besoin de préparer

Sauf choix contraire, Codex prendra en charge :

- la conception détaillée et le découpage des modifications ;
- l'écriture du code Rust ;
- la remise en état et l'extension des tests automatisés à partir des données fournies ;
- les migrations SQLite et l'évolution des schémas JSON ;
- l'instrumentation et les journaux nécessaires ;
- la documentation technique et d'exploitation ;
- les scripts de vérification reproductibles dans le dépôt ;
- l'analyse des performances et de la concurrence côté application ;
- la préparation du paquet candidat et des notes de version ;
- le diagnostic des résultats de test et les itérations correctives.

Les opérations directes sur le serveur de production, les secrets, l'approbation doctrinale et la décision de mise en service restent sous votre contrôle explicite.

## 14. Réponse synthétique à compléter

Vous pouvez répondre en copiant seulement ce bloc :

```text
ENVIRONNEMENT
DCS build :
DCS-gRPC version :
LSO version/hash :
Mode de lancement :
Joueurs max / carriers max :
Mods et frameworks :

PÉRIMÈTRE
Appareils :
Carriers :
IA incluse : oui/non
Replay ACMI requis : oui/non

ARBITRAGES
A Périmètre :
B Autorité du grade :
C Doctrine :
D Groove ou Case I :
E Données incomplètes :
F Continu ou gates :
G AoA/énergie :
H Waveoffs :
I Touchdown/bolter :
J Autorité du wire :
K Unicorn :
L Identité joueur :
M Compatibilité :
N Rétention :
O Livraison :
P Erreurs locales :
Q Validateur métier :
R Trous télémétriques/reprise :
S Budget de ressources/lieu d'exécution :
T Qualif Bolter/touch-and-go :
U Lissage/position carrier :
V Compatibilité avion/navire/recovery :
W Spots/affectation/occupation :
X Grade V/STOL :
Y Version DCS-gRPC :
Z StreamUnits/cache partagé :

DONNÉES DISPONIBLES
Cas nominaux :
Cas non conformes :
Fixtures ACMI :
Logs DCS/gRPC/LSO :

EXPLOITATION
Environnement de test :
Fenêtre de maintenance :
Rollback :
Critères de charge/reconnexion :
```

## 15. Seuil minimal pour commencer utilement

Je peux commencer les correctifs techniques non controversés lorsque les points suivants sont disponibles :

1. sauvegarde et rollback confirmés ;
2. versions DCS, DCS-gRPC et binaire LSO identifiées ;
3. périmètre appareils/carriers connu ;
4. au moins un cas nominal et un cas incorrect documentés ;
5. décision sur données incomplètes, trous télémétriques, autorité du grade et compatibilité ;
6. matrice avion–navire validée ;
7. pour le Tarawa : spots visés, source de l'affectation et périmètre de l'occupation décidés ;
8. environnement de test défini.

Sans ces éléments, seules des améliorations internes génériques seraient possibles, avec un risque de corriger le mauvais comportement ou de casser les usages actuels.

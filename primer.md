# Comment le programme note un appontage, expliqué simplement

> Ce document s'adresse à quelqu'un qui joue à DCS mais ne lit pas le code du projet. Aucune
> notion de programmation n'est nécessaire. Toutes les valeurs numériques utilisées en exemple
> (positions, distances, timestamps...) sont **inventées** pour illustrer le principe — ce ne sont
> pas des vraies données extraites d'un vol.
>
> Pour le détail technique exact et à jour, voir [AGENTS.md](AGENTS.md), "Gates, outcomes et
> câble". Pour la liste des chantiers en cours ou à venir, voir
> [tasking-roadmap.md](tasking-roadmap.md).

## L'idée en une phrase

Le programme tourne à côté de DCS, pendant que vous jouez. Il ne pilote rien et ne modifie rien
dans le jeu : il **regarde** votre avion et le porte-avions plusieurs fois par seconde, comme une
caméra de surveillance, et à la fin de votre appontage il compare ce qu'il a vu à une grille de
notation, pour vous sortir un petit rapport avec une note, un graphique, et un fichier rejouable.

## Trois briques bien distinctes : DCS, DCS-gRPC et DCS-gRPC-lso

Le nom du projet ("DCS-gRPC-lso") mélange trois choses différentes, et c'est une source de
confusion fréquente. Ce sont en réalité **trois logiciels séparés**, qui ne font pas du tout le
même métier :

- **DCS** : le simulateur lui-même. C'est lui qui fait vraiment voler l'avion, qui simule le
  porte-avions, la physique, et qui sait, en interne, "cet avion vient de toucher le pont". Sans
  DCS, il n'y a tout simplement rien à observer.
- **DCS-gRPC** : un petit module technique qui tourne **à l'intérieur** de DCS (une extension
  installée par l'administrateur du serveur). Il ne connaît rien à l'appontage, ne juge jamais
  rien, et ne prend aucune décision : c'est un simple "guichet". Il sait répondre à des demandes
  très basiques du style *"quelle est la position de l'avion X ?"* ou *"préviens-moi dès qu'un
  événement se produit"*, et les transmettre en dehors de DCS via le réseau (la technologie
  "gRPC", d'où le nom). Une petite partie de ce guichet (la capture "buffered" citée à l'étape 2)
  a été ajoutée sur mesure pour ce projet, mais reste purement mécanique : capturer, stocker
  brièvement, puis servir tel quel — jamais de calcul de note ou d'écart.
- **DCS-gRPC-lso** ("LSO", "le programme" dans tout ce document) : c'est le vrai sujet de ce
  document. Un programme à part, qui tourne à côté de DCS (pas dedans), qui pose toutes les
  questions au guichet DCS-gRPC, et qui fait **toute la réflexion** : repérer une approche,
  convertir la trajectoire, calculer les écarts, décider du verdict, calculer la note, écrire les
  fichiers. Toute la logique "LSO" — tout ce qui ressemble à un jugement — vit uniquement ici,
  jamais dans DCS ni dans DCS-gRPC.

Une image simple : **DCS** est le porte-avions et l'avion eux-mêmes, **DCS-gRPC** est une vitre
sans tain posée entre le pont et l'extérieur (on peut regarder au travers, poser des questions
simples, mais elle ne pense pas), et **DCS-gRPC-lso** est l'observateur humain — ici, un
programme — installé derrière cette vitre, qui prend des notes et juge ce qu'il voit.

Chaque étape ci-dessous précise, entre parenthèses après son titre, laquelle de ces trois briques
fait le travail.

Tout se passe en 8 étapes, dans l'ordre. On va les prendre une par une.

```
1. Repérage         "tiens, cet avion a l'air de rentrer se poser"
2. Enregistrement    "je note où sont l'avion et le porte-avions, encore, encore, encore..."
3. Écoute des events "le jeu me dit qu'il y a eu un contact avec le pont"
4. Calcul trajectoire "par rapport au pont, l'avion est un peu haut et un peu à gauche"
5. Trois photos      "à 3/4 NM, 1/2 NM et 1/4 NM, voici l'écart exact"
6. Verdict           "posé, brin n°3" / "bolter" / "remise de gaz"
7. Notation          "avec cette trajectoire-là, ça vaut... (OK)"
8. Rapports          fichier JSON + rejeu ACMI + image + ligne dans la base + message Discord
```

---

## Étape 1 — Repérage : "ça ressemble à une approche" *(DCS-gRPC lit, DCS-gRPC-lso décide)*

Le programme surveille en permanence, toutes les 2 secondes, la position de chaque avion par
rapport à chaque porte-avions présent dans la mission. Dès qu'un avion coche ces trois cases en
même temps, le programme se dit "c'est probablement un appontage qui commence" et bascule en mode
enregistrement pour cet avion précis :

- il vole en dessous d'environ 1100 ft (pas en croisière) ;
- il est à moins de 3,5 NM (environ 6,5 km) du porte-avions ;
- il est à plus de 200 m du porte-avions (donc pas déjà posé dessus).

**Exemple fictif :** votre F/A-18C, `Wolf 1-1`, redescend du "break" à 800 ft, à 2,1 NM du
Theodore Roosevelt. Les trois conditions sont remplies → le programme démarre l'enregistrement
pour la paire *(Wolf 1-1, CVN-71)*.

Ici, **DCS** est la seule source de vérité (positions réelles) ; **DCS-gRPC** se contente de lire
ces positions dans DCS et de les transmettre sans les interpréter ; c'est **DCS-gRPC-lso** qui
compare ces chiffres aux trois seuils et décide "oui, ça ressemble à un début d'approche".

Notez qu'il ne regarde pas si vous êtes "pointé vers" le porte-avions : pendant le virage du
break, vous êtes souvent perpendiculaire au bateau, donc ce critère aurait raté beaucoup
d'approches légitimes. Il ne regarde que distance + altitude.

> **Cas particulier : le catapultage.** Un avion tout juste catapulté redescend forcément par
> les mêmes trois conditions (il quitte la zone d'exclusion des 200 m, reste sous 1100 ft, reste
> à moins de 3,5 NM pendant quelques secondes) — le programme démarre donc bien un
> enregistrement, même si ce n'est pas une approche. Ce n'est pas un bug ignoré : plus loin
> (étape 6), tant que l'avion s'éloigne au lieu de se rapprocher, il ne "rentre" jamais
> officiellement dans le circuit d'approche, et l'enregistrement se termine tout seul dès qu'il
> sort de la zone (> 3,5 NM ou > 1100 ft) sans jamais produire de verdict. Un verdict manquant
> est automatiquement jeté à la fin de l'étape 8 : aucun rapport, aucune note, aucun fichier
> n'est produit pour un catapultage. Le seul effet de bord possible est un léger délai de
> détection si le même avion revenait se poser dans la minute suivant son catapultage.

---

## Étape 2 — Enregistrement : la caméra tourne *(DCS mesure, DCS-gRPC capture/sert, DCS-gRPC-lso collecte)*

Une fois l'enregistrement démarré, le programme demande en boucle, environ 10 à 20 fois par
seconde, "où est l'avion, où est le porte-avions, à quelle vitesse, dans quelle orientation" —
pour les deux en même temps, au même instant. C'est la matière première de tout le reste :
sans cette trajectoire brute, rien d'autre n'est possible.

**Exemple fictif d'un tout petit extrait de ce flux brut** (positions très simplifiées, en
mètres, par rapport à un repère fixe de la carte) :

| Heure (temps mission) | Avion — position | Avion — vitesse | Porte-avions — position | Cap du bateau |
|---|---|---|---|---|
| 12:03:41.10 | (1 240 m, 88 m, 340 m) | 68 m/s | (0 m, 62 m, 0 m) | 264° |
| 12:03:41.15 | (1 202 m, 89 m, 341 m) | 68 m/s | (0.6 m, 62 m, 0 m) | 264° |
| 12:03:41.20 | (1 164 m, 90 m, 341 m) | 69 m/s | (1.2 m, 62 m, 0 m) | 264° |
| … | … | … | … | … |

Chaque ligne de ce tableau, c'est un "instantané". Le programme en produit des centaines par
appontage. C'est du brut, sans jugement : à ce stade, il n'y a aucune notion de "trop haut" ou
"trop bas", juste des coordonnées.

**Une subtilité importante :** il existe deux façons de récupérer ces instantanés — mais **une
seule des deux est active à la fois**, ce n'est pas un choix fait vol par vol ni les deux en
parallèle. Le choix se fait une bonne fois pour toutes au lancement du programme (option
`--position-source`, avec "buffered" comme réglage par défaut) et s'applique ensuite à tous les
appontages de la session. La méthode "unary" n'est donc pas un filet de sécurité qui se
déclenche automatiquement en cas de souci : c'est une option de secours qu'il faut choisir
explicitement au démarrage, utile surtout pour revenir en arrière si la méthode par défaut pose
un problème, ou pour comparer les deux méthodes lors de tests (jamais sur le même vol).

- **La méthode "buffered" (utilisée par défaut)** : c'est DCS lui-même qui capture avion et
  porte-avions au même instant précis, les met de côté dans une petite boîte aux lettres, et le
  programme vient périodiquement relever le paquet accumulé. Avantage : même si le programme est
  occupé une fraction de seconde, rien n'est perdu, tout est dans la boîte à son prochain passage.
  Cette boîte aux lettres se vide elle-même au fur et à mesure de ce que le programme a déjà
  relevé, pour ne jamais grossir indéfiniment.
- **La méthode "unary" (une alternative de secours)** : le programme demande "position de
  l'avion ?" puis "position du bateau ?" séparément, à chaque tick. Comme les deux réponses
  n'arrivent jamais exactement en même temps, il doit ensuite "recaler" les deux mesures dans le
  temps pour qu'elles redeviennent comparables — un peu comme reconstituer une photo de groupe à
  partir de deux photos prises à quelques centièmes de seconde d'écart.

Le rapport distingue maintenant quatre problèmes qui se ressemblaient auparavant : la boîte aux
lettres qui évince d'anciens éléments, une séquence réellement manquée par le lecteur, une capture
DCS qui s'interrompt, et un paquet complet simplement livré en retard. Un grand compteur interne ne
signifie donc plus à lui seul « positions perdues ». Les seuils de sécurité restent inchangés : le
programme n'interpole jamais une longue coupure et ne fabrique aucune trajectoire.

Si DCS signale qu'une lecture d'avion ou de porte-avions était invalide, le JSON garde l'instant de
capture, le côté concerné, le motif exact et l'instant séparé où l'erreur a été reçue. Une erreur
prouvée avant le groove ou après le toucher reste un diagnostic. Dans le groove, une erreur isolée
ne retire plus automatiquement la note lorsque les deux vraies captures qui l'encadrent prouvent
qu'au plus 300 ms se sont écoulées et qu'aucune porte de mesure ne tombe dans ce trou. Le programme
ne recrée aucune position manquante : il constate seulement que la couverture réelle reste assez
serrée. Plusieurs erreurs de suite, un intervalle plus long ou une porte touchée restent bloquants.
Si l'heure de capture manque, la séquence, le tick DCS et les captures voisines peuvent seulement
servir de bornes prudentes ; l'heure de réception n'est jamais utilisée à sa place. Sans bornes
fiables, la note reste indisponible et le rapport l'indique explicitement.

### Une fréquence identique du début à la fin — est-ce pertinent ?

Aujourd'hui, cette fréquence de 10-20 fois par seconde est **la même partout** : dès le repérage
initial (le "break", le circuit d'approche large) jusqu'au posé, sans distinction entre ce moment
où l'avion vole encore loin du porte-avions et le "groove" (la ligne droite finale, juste avant le
pont), où la précision compte vraiment.

C'est une vraie question d'ingénierie, pas juste un détail : un LSO humain ne "calcule" évidemment
pas 10 fois par seconde — mais ce n'est pas la bonne comparaison à faire. La bonne question, c'est
plutôt : à quelle vitesse la trajectoire de l'avion change-t-elle, et à quelle fréquence faut-il
"prendre des photos" pour pouvoir la reconstituer fidèlement après coup ? Un avion en approche
avance à environ 70-75 m/s, et une correction de trajectoire prend environ 1 à 2 secondes à
s'exécuter : pour voir la *forme* de cette correction (pas seulement son résultat), il faut
plusieurs points par seconde. Dans le groove, 10-20 Hz n'est donc pas un chiffre arbitraire.

En revanche, pendant le grand circuit qui précède (où le programme ne fait que surveiller une
distance grossière pour savoir s'il doit continuer à suivre l'avion), rien n'exige une telle
précision. Réduire la fréquence de *lecture* uniquement à ce moment-là (sans toucher à la capture
côté DCS, déjà à cadence fixe) reste une piste à l'étude, pas encore promue : un outil de mesure
hors-ligne dédié (`lso.exe cadence-ab`, voir `tasking-roadmap.md`) existe déjà pour comparer les
gates avec et sans cette réduction sur des vols déjà enregistrés, mais la dernière mesure a montré
qu'un sous-échantillonnage supplémentaire peut casser la porte la plus proche du groove sur un
enregistrement déjà peu dense — la décision reste donc explicitement en suspens.

Le rapport JSON, lui, ne conserve déjà plus tous les instantanés bruts en dehors de la zone de
notation : la partie "pattern" (avant l'entrée en groove) est sous-échantillonnée pour alléger le
fichier, sans jamais toucher aux instantanés qui servent au calcul des gates ou de la note.

### Pourquoi calculer en temps réel plutôt qu'attendre la fin du trap ?

Bonne question qui revient souvent : puisque tout le calcul "intelligent" (conversion de
trajectoire, gates, verdict, note) est fait par DCS-gRPC-lso et pas par DCS, pourquoi ne pas se
contenter d'enregistrer les positions brutes pendant l'appontage, et ne faire tout le calcul
qu'une fois le trap terminé (posé, bolter ou remise de gaz) ? Ça semblerait plus économe.

En pratique, le programme fait bien les deux à la fois dès le départ : à chaque instantané reçu,
il met immédiatement à jour la trajectoire convertie, vérifie si une des trois portes vient d'être
franchie, et réévalue si un verdict (posé/bolter/remise de gaz) vient de se produire. Ce n'est pas
un choix arbitraire, pour deux raisons concrètes :

- **Le verdict n'existe nulle part avant d'être calculé.** Contrairement à ce qu'on pourrait
  imaginer, rien dans DCS ne dit au programme "le trap est terminé, tu peux traiter les données
  maintenant". Le programme **découvre** que le trap est fini précisément *en observant* la
  trajectoire en continu (par exemple : "la distance recommence à augmenter après avoir touché le
  pont → c'est un bolter"). Sans ce calcul en continu, il n'aurait tout simplement aucun moyen de
  savoir quand arrêter d'enregistrer.
- **Le calcul lui-même ne coûte presque rien.** À chaque instantané, ce que fait le programme
  revient à quelques opérations de trigonométrie — de l'ordre du millionième de seconde de calcul
  processeur. La vraie "dépense" de ressources, c'est la récupération des données depuis DCS
  (réseau, capture côté serveur), qui a lieu de toute façon, que le calcul soit fait tout de suite
  ou plus tard.

C'est la seule étape où **les trois briques** travaillent vraiment ensemble à chaque instant :
**DCS** simule les positions réelles, **DCS-gRPC** les lit dans DCS et les met à disposition
(directement, ou via sa petite boîte aux lettres en mode "buffered"), et **DCS-gRPC-lso** vient
les récupérer et les empiler dans sa propre mémoire pour tout le reste du traitement.

---

## Étape 3 — Écoute des événements : ce que DCS raconte en plus *(DCS génère, DCS-gRPC relaie, DCS-gRPC-lso interprète)*

En parallèle de la caméra qui tourne en continu, le programme écoute aussi un deuxième canal :
les "événements" que DCS envoie lui-même, un peu comme des notifications. Les principaux qui
comptent ici :

- **"Landing Quality Mark"** : DCS envoie parfois un petit texte, écrit par le jeu, du style
  `"LSO: GRADE:OK : WIRE# 3"`. C'est la note et le câble **selon DCS lui-même**, indépendamment
  de ce que le programme calcule de son côté.
- **"Land" / contact avec la piste ou le pont** : le moment précis où le train touche le pont.
- **Disparition de l'avion ou du bateau** (crash, déconnexion...) : sert à arrêter proprement
  l'enregistrement.

Ce canal-là ne dit jamais "c'est un bon ou un mauvais posé" tout seul — il fournit des indices
bruts que les étapes suivantes vont recouper avec la trajectoire. C'est volontairement tenu
séparé de la caméra de l'étape 2 : **si ce canal d'événements a un problème (coupure, fermeture
propre du flux), la trajectoire positionnelle continue d'être enregistrée sans interruption, et le
rapport le dit explicitement** (un diagnostic distinct, jamais confondu avec un vrai trou de
positions). Un seul canal est partagé par toute la session et tente de se reconnecter tout seul. Un
petit journal garde les événements récents ; à la fermeture d'une vraie approche, le programme
laisse encore deux secondes à un contact ou un texte LSO retardataire pour arriver, en vérifiant
toujours l'heure DCS et les identités exactes de l'avion et du bateau.

**Exemple fictif :** à 12:04:02.30, DCS envoie l'événement "Land" pour Wolf 1-1. Deux secondes
plus tard, un message `"LSO: GRADE:OK : WIRE# 3"` arrive.

Encore une fois : **DCS** décide seul quand un événement se produit et quel texte il contient ;
**DCS-gRPC** ne fait que transmettre ces événements bruts, sans les trier ni les comprendre ;
c'est **DCS-gRPC-lso** qui leur donne un sens (par exemple, "ce texte contient WIRE# 3, donc DCS
annonce le brin n°3").

---

## Étape 4 — Calcul de la trajectoire : "où est l'avion par rapport au pont, pas par rapport à la Terre" *(entièrement DCS-gRPC-lso)*

Les positions brutes de l'étape 2 sont exprimées par rapport à la carte (comme des coordonnées
GPS). Ça n'a aucun intérêt pour juger un appontage : ce qui compte, c'est où est l'avion **par
rapport à l'axe d'approche du pont**, qui lui-même bouge et tourne avec le porte-avions.

Le programme fait donc, à chaque instantané, une conversion : il prend l'axe de piste angulaire
du porte-avions (le pont est légèrement décalé par rapport à l'axe du bateau, typiquement
quelques degrés), et recalcule la position de l'avion dans ce repère-là. Le résultat, ce sont
deux nombres qui parlent enfin :

- **la distance restante jusqu'au seuil de piste**, le long de l'axe d'approche ;
- **l'écart latéral** par rapport à cet axe (à gauche ou à droite) ;
- **l'écart vertical** par rapport à la trajectoire idéale de descente (le glideslope).

**Exemple fictif, même instant que tout à l'heure, converti dans le repère du pont :**

| Distance restante (le long de l'axe) | Écart latéral | Altitude par rapport au pont |
|---|---|---|
| 1 450 m | -6 m (un peu à gauche de l'axe) | 92 m (un peu haut) |

C'est cette conversion, refaite à chaque instantané, qui donne la vraie trajectoire d'approche —
celle qu'un LSO humain regarderait "de dos", debout sur le pont, en train de suivre la boule. Le
programme conserve aussi, en plus de ces trois chiffres, une estimation de l'incidence (AoA) de
l'avion à chaque instant, corrigée du vent une fois celui-ci mesuré — voir l'étape 7 pour ce à quoi
elle sert (et ne sert pas).

À partir d'ici et jusqu'à la fin (étapes 4 à 8), **DCS et DCS-gRPC ne travaillent plus** : toutes
les positions brutes nécessaires ont déjà été récupérées à l'étape 2-3. Tout ce qui suit — calcul,
verdict, note, fichiers — se déroule entièrement à l'intérieur de **DCS-gRPC-lso**, sans nouvelle
question posée à DCS.

> **Un bug corrigé à cette étape** (voir `tasking-roadmap.md`) : l'écart vertical et l'écart
> latéral sont calculés à partir de la distance restante jusqu'au pont — un calcul qui devient
> instable quand cette distance approche de zéro, juste avant le toucher. Un flare tout à fait
> normal de quelques dizaines de centimètres, à un mètre du pont, pouvait alors ressortir comme un
> écart de plusieurs dizaines de degrés, sans aucun sens réel. Le programme n'enregistre désormais
> plus d'écart continu à moins de 3 mètres du pont — largement avant que ce calcul ne devienne
> instable, et bien après que l'essentiel de l'approche ait déjà été mesuré. Corrigé et testé, pas
> encore revalidé sur un enregistrement live.

---

## Étape 5 — Trois repères précis dans cette trajectoire déjà continue : ¾ NM, ½ NM et ¼ NM *(entièrement DCS-gRPC-lso)*

Depuis l'étape 4, le programme dispose déjà, à chaque instant du groove, de l'écart vertical et de
l'écart latéral de l'avion — ce n'est pas une donnée qui apparaît seulement à cette étape. En plus
de cette lecture continue, il retient trois instants particuliers, quand l'avion franchit des
distances précises et traditionnellement utilisées en doctrine LSO :

> **Comment le programme sait-il que le groove commence ?** Dans la vraie doctrine (CASE I), le
> groove commence à la sortie du dernier virage, quand le pilote remet les ailes presque à plat.
> C'est un **événement du circuit**, pas le franchissement d'une distance fixe et pas la preuve que
> l'approche est déjà belle. Le programme reconnaît donc d'abord le circuit à gauche du navire,
> observe le dernier virage sous 600 ft, puis attend que l'inclinaison revienne à 10° ou moins
> pendant trois quarts de seconde tandis que l'avion continue vers la zone d'appontage. Un trou de
> télémétrie supérieur à 300 ms recommence cette confirmation. Le passage près de `-0,75°` à gauche
> de l'axe aide au diagnostic, mais n'est pas obligatoire : un undershoot qui reste à gauche ou un
> overshoot qui traverse vite l'axe ont quand même un groove. Leur mauvais lineup, leur route
> imparfaite et leurs corrections sont conservés dès le roll-out au lieu d'être cachés par une
> entrée tardive. Le repère ¾ NM reste une photo de trajectoire, mais ne définit plus le départ du
> groove. Cette logique ne concerne que le Case I CATOBAR ; un straight-in Case II/III n'est pas
> activé implicitement et l'AV-8B/Tarawa garde sa boîte historique.

| Repère | Distance | Équivalent |
|---|---:|---|
| ¾ NM | 1 389 m | premier repère fixe ; il peut tomber avant ou après le roll-out réel |
| ½ NM | 926 m | milieu de la finale |
| ¼ NM | 463 m | juste avant le pont |

À l'instant exact où l'avion franchit chacune de ces trois distances, le programme "prend une
photo" de la valeur déjà mesurée en continu, et la convertit en degrés (l'unité que la doctrine
LSO utilise habituellement, plutôt que des mètres).

**Exemple fictif des trois photos pour l'appontage de Wolf 1-1 :**

| Gate | Écart vertical (glideslope) | Écart latéral (lineup) |
|---|---:|---:|
| ¾ NM | +0.4° (un peu haut) | -1.2° (un peu à gauche) |
| ½ NM | -0.1° (quasi parfait) | +0.3° |
| ¼ NM | 0.0° (parfait) | +0.1° |

Le programme ne retient cette photo que si les mesures autour de ce moment sont fiables (pas de
trou de données, pas de décalage temporel suspect entre avion et bateau, avion bien en
approche et pas trop désaxé). Si les conditions ne sont pas réunies, la photo est marquée
"invalide" plutôt que d'inventer un chiffre approximatif.

Si l'objet « photo » manque alors que deux mesures continues réelles encadrent exactement la même
distance, sont toutes deux valides et ne sont séparées que de 300 ms au maximum, cette paire peut
prouver la couverture de la porte. Le rapport et les affichages disent alors explicitement
« couvert par la trajectoire continue » : aucune position n'est recréée et les écarts réellement
mesurés dans la trajectoire restent ceux qui déterminent la note. Une vraie capture tardive ou un
trou plus long reste incomplet et n'accorde aucun point.

**Pourquoi ces trois photos précises comptent encore, alors que la trajectoire est déjà
enregistrée en continu ?** Parce qu'elles apportent une vérification de fiabilité renforcée. En
CATOBAR, si la photo ¾ NM a été prise avant la vraie entrée en groove — donc encore dans le virage
final — elle reste visible mais ne décide ni de l'éligibilité ni de la note ; les photos ½ et ¼ NM
doivent alors être valides et ordonnées. Si ¾ NM tombe après l'entrée en groove, les trois restent
obligatoires. L'AV-8B/Tarawa exige toujours les trois. Une fois cette condition remplie, le
programme combine les photos utiles avec toute la trajectoire continue — voir l'étape 7.

---

## Étape 6 — Le verdict : qu'est-ce qui s'est réellement passé ? *(entièrement DCS-gRPC-lso)*

En observant en continu la distance entre l'avion et le point d'atterrissage (est-ce que ça
diminue toujours, ou est-ce que ça recommence à augmenter ?), combiné aux événements DCS de
l'étape 3, le programme détermine l'issue :

- **Approche seule (`ApproachOnly`)** : la finale est clairement reconnue (groove ou porte ½/¼ NM),
  mais aucun contact, waveoff ou arrêt certain n'a pu être établi. Ce n'est pas renommé en remise
  de gaz par défaut. Si toute la trajectoire nécessaire est couverte, sa note d'approche et ses
  points restent visibles ; seule l'issue demeure inconnue. Un simple passage dans la grande zone
  de détection, sans groove, porte significative ni événement d'issue, ne produit aucun rapport
  pilote.

- **Posé/arrêté ("Recovered")** : un contact `Land`/`RunwayTouch` prouve le toucher, pas
  l'arrestation. Trois preuves peuvent confirmer le trap, dans cet ordre de priorité :
  1. le message DCS `WIRE#` (confiance « haute », le brin est celui de DCS) ;
  2. la **transitoire de la crosse** : la crosse animée, stable en position basse, se déforme
     brusquement au moment du contact puis revient en position basse dans les huit secondes,
     et ce mouvement coïncide (à 200 ms près) avec le passage géométrique de la crosse sur un
     câble — c'est alors ce câble qui est estimé (confiance « moyenne ») ;
  3. la **cinématique pont** : l'avion, mesuré par rapport à la position brute du bateau, passe
     sous 6 m/s dans les huit secondes qui suivent le contact et y reste deux secondes sans
     rebond ni redécollage (confiance « moyenne », **aucun numéro de brin inventé**).

  **Politique LSO humain (13 septembre 2026)** : quand un LSO humain tient le pattern et que DCS
  n'émet pas de `WIRE#`, une arrestation confirmée par la transitoire de crosse ou par la
  cinématique pont donne un `Recovered` notable à confiance « moyenne », sans jamais inventer de
  numéro de brin que la preuve ne porte pas. Cette politique a été validée en rejeu sur les
  quatorze passes réelles de septembre 2026 (`tests/recordings/live_2026-09/`) : les sept traps
  étiquetés par DCS sont retrouvés avec le bon brin par la transitoire de crosse, puis à nouveau
  sans aucune donnée de crosse par la cinématique seule ; aucun bolter ni touch-and-go crosse
  haute n'est promu en trap. Sans aucune de ces trois preuves, le verdict reste
  `unconfirmed_arrest`, sans points, plutôt que de transformer un simple ralentissement en trap
  certain. Le programme calcule aussi, de façon indépendante, **quel brin** en regardant
  géométriquement où passe la crosse par rapport aux quatre câbles — un peu comme s'il
  chronométrait lui-même à quel endroit exact la crosse a "accroché". Cette estimation est
  ensuite comparée au texte que DCS a envoyé (`WIRE# 3` dans notre exemple), mais **c'est
  toujours le brin annoncé par DCS qui l'emporte** dès qu'il existe : ni la note, ni le message
  Discord, ni l'image ne montreront jamais une estimation Rust qui contredirait ce que vous avez
  vu s'afficher dans le jeu. Si les deux sont d'accord, tant mieux ; s'ils divergent, seul le
  fichier JSON complet (destiné à l'analyse technique, pas à la lecture rapide) garde une trace
  des deux valeurs côte à côte, pour du diagnostic — jamais affiché au premier coup d'œil comme
  s'il fallait choisir entre deux versions. L'estimation Rust ne devient "affichable seule" que
  dans le cas contraire, quand DCS n'a lui-même rien annoncé.

  Le JSON garde aussi, dans `arrest_confirmation.kinematic`, l'ancienne enquête cinématique fondée
  sur la vitesse instantanée (contact corrélé, début de décélération, vitesse relative qui tombe
  près de zéro, absence de rebond). Elle n'existe qu'en direct (le rejeu ACMI n'a pas de vitesse)
  et reste volontairement diagnostique : c'est `arrest_confirmation.deck_kinematics`, la mesure par
  déplacement décrite au point 3, qui confirme.
- **Bolter** : le train a touché le pont mais l'avion a continué et a redécollé sans s'arrêter
  (aucun brin accroché).
- **Touch-and-go** : comme un bolter en apparence, mais la crosse était en position "up" par
  intention (un touch-and-go volontaire d'entraînement, pas un vrai bolter). Le programme ne
  devine jamais cette intention à partir du comportement de vol : il lit directement, depuis DCS,
  la position réelle de la crosse au moment où l'avion repart, et exige que cette lecture reste
  stable sur plusieurs instants d'affilée avant de conclure "crosse levée" (une lecture isolée ou
  ambiguë ne suffit pas). Cette lecture n'est possible que pour les avions dont le programme
  connaît le bon canal d'information dans DCS — actuellement le F/A-18C, le T-45 et le F-14 (dans
  ses variantes) ; pour tout autre type, ou si la lecture est ambiguë, le programme retombe sur
  `Bolter` par défaut, jamais l'inverse.

> **Une extension apportée à cette étape (5 septembre 2026)** : cette lecture de la crosse n'était
> jusqu'ici fiable que pour le F/A-18C — le T-45 et le F-14 obtenaient toujours `Bolter`, même
> pour un vrai touch-and-go volontaire, faute de connaître le bon canal DCS à interroger pour ces
> deux types. Les deux canaux ont été fournis et ajoutés (le T-45 partage celui du F/A-18C ; le
> F-14, dans ses trois variantes, en utilise un différent). La polarité a depuis été confirmée en
> vol pour le T-45 et le F-14B(U), dans les deux sens ; les F-14A et F-14B restent à vérifier
> séparément — voir `tasking-roadmap.md`.
- **Remise de gaz ("Wave off")** : l'avion s'est écarté sans jamais toucher le pont. Le
  programme ne sait pas dire, à partir des données brutes, si c'est le pilote qui a décidé de
  remettre les gaz ou un ordre du LSO/de sécurité — donc il l'affiche comme "remise de gaz",
  sans jamais inventer une cause qu'il ne peut pas prouver. Quand DCS émet explicitement une note
  `GRADE:WO`, cette note termine la tentative dès que l'avion repart : le circuit suivant commence
  dans un rapport neuf, afin qu'un second waveoff ou un trap ultérieur ne soit jamais absorbé comme
  un simple doublon du premier.

**Exemple fictif :** Wolf 1-1 → `Recovered`, brin estimé par géométrie = 3, brin annoncé par DCS
= 3 → les deux concordent, confiance "haute".

> **Pourquoi ce choix ?** Si le module affichait "DCS dit 3, Rust estime 4" bien en évidence dans
> le message Discord juste après votre appontage, vous auriez de quoi douter de l'outil : vous
> avez vu et vécu le brin 3 dans DCS, pas de raison de lire autre chose en premier. Le programme
> garde donc son calcul géométrique — utile en coulisses pour repérer ses propres erreurs de
> calibration, ou pour proposer une estimation quand DCS ne dit rien du tout — mais ne le met
> jamais en avant à côté d'un chiffre DCS qu'il contredirait. DCS reste la seule autorité pour
> tout ce que vous voyez sans creuser.

> **Un bug corrigé à cette étape** (voir `tasking-roadmap.md`) : le calcul géométrique du brin
> pouvait, sur de rares appontages, être fait deux fois avec un historique différent — une fois au
> moment précis où DCS confirme le toucher, une deuxième fois à la toute fin du rapport, une fois
> toute la trajectoire connue. Si le franchissement du brin n'était pas encore enregistré au tout
> premier calcul, le champ principal du rapport (celui que vous voyez en premier) pouvait afficher
> "estimation indisponible" alors que le champ de diagnostic, plus bas dans le même fichier JSON,
> contenait bien une valeur fiable — deux nombres qui auraient dû être identiques et ne l'étaient
> pas toujours. Confirmé sur 2 appontages sur 6 lors d'un test live. Le programme ne calcule
> désormais plus qu'une seule fois, à la toute fin, avec l'historique complet — les deux champs
> sont donc garantis identiques. N'a jamais affecté la note elle-même, seulement l'affichage du
> brin et le niveau de confiance associé. Corrigé et testé, pas encore revalidé sur les
> enregistrements live où le problème avait été observé.

> **Un bug corrigé à cette étape** (voir `tasking-roadmap.md`) : une remise de gaz "en survol", où
> l'avion passe très haut au-dessus du pont sans y toucher puis remonte, était auparavant mal
> classée `Bolter` au lieu de `WO?` — le franchissement du seuil du pont ne vérifiait que la
> position horizontale, pas l'altitude. Le programme vérifie désormais que l'avion est réellement
> proche du niveau du pont au moment du franchissement avant de considérer un vrai contact.
> Corrigé et testé, mais pas encore revalidé sur un enregistrement live au moment de la rédaction
> de ce document.

---

## Étape 7 — La notation : de la trajectoire complète à une lettre *(entièrement DCS-gRPC-lso)*

C'est ici que la trajectoire continue de l'étape 4 et les trois photos de l'étape 5 deviennent une
note lisible. Les trois photos ont déjà tranché une question préalable — l'appontage est-il
notable du tout (étape 5) ? — mais une fois cette question réglée, le calcul ne se limite pas à
"regarder le pire écart parmi les trois photos".

1. **Le programme découpe le groove en quatre zones.** `START` va de la sortie du dernier virage à
   ½ NM, `MIDDLE` de ½ à ¼ NM, `IN CLOSE` de ¼ NM à 150 m et `RAMP` couvre les 150 derniers mètres.
   La même erreur compte davantage lorsqu'elle survient tard : les poids sont respectivement
   1,0, 1,2, 1,5 et 2,0. Ces nombres pondèrent une gravité abstraite ; ils ne multiplient jamais
   directement les angles mesurés.
2. **Chaque axe est observé séparément.** La pente, le lineup et l'incidence propre au type d'avion
   sont rangés dans des bandes « aucune », « petite », « moyenne » ou « grosse ». Une seule mesure
   anormale isolée est ignorée. Plusieurs mesures consécutives forment un épisode dont le rapport
   conserve le début, la durée, la zone, le pic, l'évolution et les éventuelles oscillations.
3. **La correction change la gravité d'un épisode.** Le programme part du pire instant de
   l'épisode : une aggravation initiale n'annule donc plus une réaction franche qui suit ce pic.
   Une correction qui franchit durablement une bande dans le délai propre à l'endroit du pic
   (3 s au START, 2,5 s au MIDDLE, 1,5 s en IN CLOSE, 0,75 s au RAMP) puis se stabilise sur au
   moins deux mesures retire un niveau. Une correction réelle mais tardive, lente ou incomplète ne
   change rien. Une stagnation, une aggravation ou des corrections répétées dans les deux sens
   ajoutent un niveau. Le programme regarde notamment la tendance sur quatre secondes et compte
   les inversions suffisamment grandes pour ne pas confondre une oscillation avec du petit bruit.
4. **Seul le pire épisode décide.** La gravité corrigée est multipliée par le poids de sa zone. Les
   erreurs de plusieurs axes ne sont jamais additionnées, ce qui évite de punir deux fois un même
   moment de pilotage. Une valeur finale sous 1,5 donne un candidat `OK`, de 1,5 à moins de 3 donne
   `(OK)`, et 3 ou davantage donne `--`.
5. **`_OK_`, la passe parfaite.** Elle reste accessible uniquement depuis un candidat `OK` sans
   épisode significatif, avec une trajectoire stable, les amplitudes strictes déjà exigées et un
   groove de 15 à 18 secondes. Elle ne dépend jamais du brin accroché et un touch-and-go reste
   plafonné à `OK`.

| Le résultat de cette analyse | Note | Points |
|---|---|---:|
| Approche parfaite (voir point 4) et temps de groove 15-18 s | `_OK_` | 5.0 |
| Pire épisode pondéré sous 1,5 | `OK` | 4.0 |
| Pire épisode pondéré entre 1,5 et moins de 3 | `(OK)` | 3.0 |
| Pire épisode pondéré à 3 ou davantage | `--` | 2.0 |
| Très bas et dangereux à la toute dernière photo (¼ NM), ou taux de descente/gîte franchement dangereux et soutenu à ce même endroit | `C` (Cut) | 0.0 |
| Bolter confirmé (voir étape 6) | `B` | 2.5 |
| Remise de gaz | `WO?` | pas de points |
| Preuve insuffisante pour juger | évaluation partielle, issue seule, ou `NC` si rien d'utile ne subsiste | pas de points |

Une limite technique n'efface plus ce qui est certain. Le rapport sépare désormais l'issue de la
tentative, l'évaluation de la portion réellement observée et l'éligibilité aux points. Une approche
partielle peut donc conserver son appréciation avec la mention « sans points » ; un câble annoncé
par DCS, un bolter, un touch-and-go ou une remise de gaz restent visibles. Le grade DCS brut peut
servir de recours clairement étiqueté, mais n'est jamais converti en note ou points du projet.

**Exemple fictif :** Wolf 1-1 présente un écart moyen de lineup en `IN CLOSE`, puis revient vers
l'axe mais trop tard pour stabiliser complètement la finale. Cet épisode est déterminant et la
raison affichée est courte : **`(OK): lineup moyen en IN CLOSE, corrigé tardivement ou
incomplètement.`**

> Important à savoir : cette grille de seuils reste une règle **du projet**
> (`PROJECT-DERIVED`, version `project-derived-v7`), pas une reconstruction certifiée de la
> doctrine officielle de l'US Navy — le rapport et la documentation technique le rappellent
> systématiquement. L'AoA est mesurée comme une distance signée à la bande OnSpeed propre à
> l'avion. Les zones, délais, coefficients, bandes d'écart, stabilisation minimale, seuils de
> tendance et d'oscillation et remises de gravité sont des choix
> du projet et doivent encore être confrontés à des appréciations de LSO humains. Puissance,
> assiette et mouvement du pont ne sont toujours pas notés.

> **Sur `_OK_` précisément :** le symbole `_OK_` et son sens ("passe parfaite") sont bien officiels
> (manuel LSO américain, section 11.4.1). La durée de groove 15-18 s citée au point 4 est aussi un
> vrai texte officiel (manuel des procédures porte-avions, section 6.2.4.3) — mais aucun des deux
> manuels ne dit comment transformer ces deux informations en une règle de calcul automatique. La
> fenêtre d'amplitude resserrée, elle, vient d'ailleurs : un mod de notation LSO open-source
> existant, utilisé par d'autres dans la communauté de simulation, qui a d'ailleurs déjà inspiré
> plusieurs autres seuils de ce document. Une ancienne version locale de cette règle liait aussi
> `_OK_` à un brin précis (le brin 3) — ce lien a été délibérément écarté : rien dans les manuels ne
> justifie qu'un brin précis mérite une meilleure note qu'un autre.

### Le vent et l'incidence (AoA)

Le rapport contient désormais le vent au moment de l'appontage (direction et vitesse) — purement
informatif, pour donner du contexte à une déviation (une dérive par vent de travers fort n'a pas le
même sens qu'une dérive par ciel calme), sans jamais changer la note automatiquement.

DCS renvoie parfois, de façon intermittente et sans lien avec ce programme, un vent "à zéro" absurde
(180°, 0 nœud) près du niveau de la mer, indiscernable d'un vrai jour sans vent. Le programme
réessaie une fois avant de faire confiance à cette valeur, et si elle persiste, affiche à la place la
mesure prise un peu plus haut en altitude au début du groove — qui, elle, reste fiable dans tous les
cas observés jusqu'ici.

L'incidence (AoA) affichée sur les graphiques est une estimation géométrique (à partir de la
vitesse et de l'orientation de l'avion), désormais corrigée du vent une fois celui-ci mesuré en
début de groove. Ce n'est toujours pas la vraie valeur lue sur l'instrument de bord du cockpit :
une piste explorée pour la lire directement depuis le modèle 3D de l'avion (le "draw argument") a
été abandonnée faute de source fiable pour identifier la bonne valeur par type d'avion — voir
`tasking-roadmap.md`. Pour une approche CASE I CATOBAR, elle entre maintenant dans les épisodes en
réutilisant uniquement les bandes déjà connues pour le F/A-18C, le F-14 ou le T-45. Si la référence
de vent n'a pas pu être établie, l'épisode reste visible dans le JSON mais n'enlève aucun point.
Il n'existe pas de bande AoA « extrême » dans cette version et l'AoA seule ne peut jamais provoquer
un Cut. L'AV-8B/Tarawa conserve strictement sa notation antérieure sans AoA.

Le rapport garde aussi, pour chaque instant de la trajectoire continue (étape 4), le taux de
descente et l'angle de gîte (bank) de l'avion — deux éléments qu'un vrai LSO commente à l'oral (un
fort taux de chute près du pont, une gîte excessive en corrigeant). Sur leur amplitude générale,
tout le long de l'approche, ils restent purement informatifs, comme le vent et l'AoA : aucune règle
validée n'existe pour les traduire en points de la même façon que l'écart de pente ou d'alignement.

**Une exception, ajoutée le 5 septembre 2026** : si le taux de descente ou la gîte deviennent
franchement dangereux — un taux de chute environ le double de celui d'un poser normal, ou une
inclinaison de plus de 30°, l'un ou l'autre soutenu sur plusieurs mesures d'affilée (pas un seul
pic isolé) — et que ça se produit tout près du pont (même zone que le `Cut` de pente), la note
tombe directement à `C`, exactement comme pour un écart de pente dangereux. Contrairement à
presque tous les autres seuils de ce document, celui-ci n'a **aucune base chiffrée** dans la
doctrine officielle : les documents LSO officiels mentionnent bien un taux de descente ou une
gîte excessifs comme des dangers réels, mais ne donnent jamais de nombre précis — c'est un choix
du projet, assumé comme tel, pas encore éprouvé sur un vrai cas dangereux en mission (voir
`tasking-roadmap.md`).

### Pistes encore envisagées, non codées

- Séparer plus nettement, dans les libellés produits, "je n'ai pas confiance dans mes données" de
  "je n'ai pas pu vous noter" — actuellement les deux sont fusionnés sous l'étiquette `NC`. Le
  rapport JSON distingue déjà les deux en coulisses (champ technique `grading_availability`), mais
  rien ne les sépare encore dans les libellés que vous voyez (Discord, PNG, tableau de bord).
- Une vraie mesure d'incidence lue sur l'instrument de bord plutôt qu'une estimation géométrique
  corrigée du vent, si une source fiable de calibration par avion devient disponible.

### Ce qui n'est volontairement pas envisagé

- **Reconstruire une "fenêtre de remise de gaz" dynamique** comme celle des vrais LSO
  (dépendante de la performance moteur, du taux de descente, du mouvement du pont...) : DCS
  n'expose pas les données nécessaires (puissance moteur réelle, par exemple), donc ce serait
  inventer une doctrine invérifiable plutôt que la reconstruire.
- **Entraîner un modèle statistique** sur d'anciens vols pour "apprendre" à noter comme un LSO
  humain : séduisant en théorie, mais ça demanderait un grand nombre de vols déjà notés par de
  vrais LSO en conditions comparables — un corpus qui n'existe pas pour ce projet.

---

## Étape 8 — Les rapports : tout ce que le programme écrit à la fin *(entièrement DCS-gRPC-lso)*

Une fois le verdict et la note calculés, le programme produit plusieurs fichiers, toujours dans
le même ordre, et seulement s'il est sûr d'être le seul à écrire ce dossier précis (pour éviter
deux rapports en double si jamais deux processus tournaient en même temps) :

1. **Un fichier JSON** — le rapport complet, lisible par une machine : note, écarts aux trois
   portes et sur toute la trajectoire, décision détaillée d'entrée en groove, erreurs de source
   horodatées, preuve d'arrestation, brin, vent et niveau de confiance. L'historique de crosse garde
   les observations les plus récentes dans une capacité couvrant largement un circuit de trois
   minutes ; s'il devait quand même être tronqué, le rapport indique pourquoi, combien d'éléments
   ont été évincés et quelle période reste conservée.
2. **Un fichier ACMI** — un rejeu du passage, ouvrable dans Tacview, pour revoir l'approche en
   3D.
3. **Une image PNG** — un petit graphique visuel de l'écart glideslope/lineup pendant
   l'approche, et un schéma du circuit d'approche (le "pattern"). Si un même suivi contient
   plusieurs circuits avant la finale, la branche réellement évaluée garde ses couleurs ; les
   circuits antérieurs sont tracés séparément en gris fin pour rester visibles sans masquer la
   finale ni créer de faux raccord entre deux tours.
4. **Une ligne dans une base de données locale** — pour garder un historique de tous vos
   passages et calculer des moyennes dans le temps (le "greenie board").
5. **Un message Discord** (si configuré) — avec la note, le graphique et le fichier de rejeu en
   pièce jointe.

**Exemple fictif du cœur du rapport JSON produit pour Wolf 1-1** (simplifié, valeurs
inventées) :

```json
{
  "recovery_id": "s17-g1-p101-c1-t184230000",
  "pilot_name": "Wolf",
  "aircraft_type": "FA-18C_hornet",
  "carrier_name": "CVN-71 Theodore Roosevelt",
  "outcome": "Arrested — wire 3 (DCS/LQM + Rust)",
  "pass_grade": "OK",
  "grade_points": 4.0,
  "wire_estimated": 3,
  "wire_dcs": 3,
  "wire_divergent": false,
  "confidence": "high",
  "wind_heading_deg": 310.0,
  "wind_speed_mps": 6.2,
  "gate_deviations": {
    "at_three_quarter_nm": { "gs_deviation_deg": 0.4, "lineup_deg": -1.2 },
    "at_half_nm":          { "gs_deviation_deg": -0.1, "lineup_deg": 0.3 },
    "at_quarter_nm":       { "gs_deviation_deg": 0.0, "lineup_deg": 0.1 }
  }
}
```

Tout ce que vous voyez dans le rapport final (le fichier, le graphique, le message Discord)
provient uniquement de ce document — rien n'est recalculé ou réinterprété ailleurs.

---

## Le trajet complet, en une image

```
Vous approchez du porte-avions
        │
        ▼
[1] "Ça ressemble à un appontage" (distance + altitude)
        │
        ▼
[2] Enregistrement continu de la position avion + bateau (10-20×/s) ──┐
        │                                                              │
        ▼                                                              │
[3] Écoute des événements DCS (contact, note LSO du jeu...)  ◄─────────┘
        │
        ▼
[4] Conversion en "écart par rapport à l'axe du pont", en continu, à chaque instant du groove
        │
        ▼
[5] Trois photos figées dans ce flux continu (¾ NM, ½ NM, ¼ NM) : décident si c'est notable du tout
        │
        ▼
[6] Verdict : posé / bolter / touch-and-go / remise de gaz + quel brin
        │
        ▼
[7] Note calculée sur trois photos + trajectoire continue (amplitude + tendance + proximité du
    pont + sécurité sink-rate/gîte près du pont + _OK_ si passe parfaite et 15-18 s de groove)
        │
        ▼
[8] Rapport JSON + rejeu ACMI + image + base de données + Discord
```

## Ce qu'il faut garder en tête

- **Qui fait quoi, en une ligne :** DCS simule et mesure, DCS-gRPC lit/relaie sans juger,
  DCS-gRPC-lso réfléchit et note. Si un jour la note vous semble bizarre, le "coupable" possible
  est presque toujours DCS-gRPC-lso (sa logique de calcul) ou la qualité des données transmises
  par DCS-gRPC (étape 2) — jamais DCS lui-même, qui ne fait que simuler le vol.
- Le programme ne **filme** jamais que ce qui existe réellement dans DCS : positions, vitesses,
  orientations, et quelques textes envoyés par le jeu lui-même. Il n'invente jamais une position
  ou un écart qu'il n'a pas pu mesurer — s'il n'est pas sûr, il préfère dire "je ne sais pas"
  (`NC`) plutôt que de deviner.
- La note produite est un outil d'entraînement du projet, **pas** une certification officielle
  US Navy/USMC — le rapport le rappelle systématiquement.
- Toutes les étapes ci-dessus tournent automatiquement, sans aucune action de votre part une
  fois que vous êtes repéré en approche ; vous n'avez rien à démarrer ni arrêter manuellement.

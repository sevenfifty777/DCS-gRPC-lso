# Synchroniser une branche avec le fork de LennyKruger

Procédure pour mettre à jour une branche locale (ex. `feature/refonte-v3-lua-buffer`)
depuis le dépôt d'origine de Lenny, puis la publier sur notre fork.

## Les dépôts distants

| Remote     | URL                                                | Rôle                                   |
| ---------- | -------------------------------------------------- | -------------------------------------- |
| `origin`   | `https://github.com/sevenfifty777/DCS-gRPC-lso.git` | Notre fork (push)                      |
| `lenny`    | `https://github.com/LennyKruger/DCS-gRPC-lso.git`   | Fork de Lenny — source des mises à jour |
| `upstream` | `https://github.com/DCS-gRPC/lso.git`               | Projet amont original                  |

Vérifier la configuration :

```bash
git remote -v
```

Si `lenny` est absent (à faire une seule fois) :

```bash
git remote add lenny https://github.com/LennyKruger/DCS-gRPC-lso.git
```

## Mise à jour — procédure courante

```bash
# 1. Se placer sur la branche et vérifier que rien n'est en cours
git checkout feature/refonte-v3-lua-buffer
git status                  # doit être "clean" — sinon commit ou git stash

# 2. Récupérer les nouveautés de Lenny (ne modifie pas le répertoire de travail)
git fetch lenny feature/refonte-v3-lua-buffer

# 3. Voir ce qui va arriver AVANT de fusionner
git log --oneline HEAD..lenny/feature/refonte-v3-lua-buffer
git diff --stat HEAD lenny/feature/refonte-v3-lua-buffer

# 4. Fusionner
git merge --ff-only lenny/feature/refonte-v3-lua-buffer

# 5. Publier sur notre fork
git push origin feature/refonte-v3-lua-buffer
```

`--ff-only` est le garde-fou : la commande **échoue** au lieu de créer un commit de
fusion si des commits locaux existent. Un échec n'est pas un problème — il signale
simplement qu'il faut passer à la procédure ci-dessous.

## Si `--ff-only` échoue (des commits locaux existent)

D'abord, identifier ses propres commits :

```bash
git log --oneline lenny/feature/refonte-v3-lua-buffer..HEAD
```
****
Puis choisir :

```bash
# Option A — rejouer nos commits par-dessus ceux de Lenny (historique linéaire, préféré)
git rebase lenny/feature/refonte-v3-lua-buffer

# Option B — fusion classique, crée un commit de merge
git merge lenny/feature/refonte-v3-lua-buffer
```

En cas de conflit :

```bash
git status                  # liste les fichiers en conflit
# … éditer les fichiers, retirer les marqueurs <<<<<<< ======= >>>>>>>
git add <fichier>
git rebase --continue       # ou: git commit   (si Option B)

# pour tout annuler et revenir à l'état d'avant
git rebase --abort          # ou: git merge --abort
```

Après un rebase, l'historique local est réécrit ; le push nécessite alors :

```bash
git push --force-with-lease origin feature/refonte-v3-lua-buffer
```

`--force-with-lease` (et non `--force`) refuse d'écraser des commits apparus
entre-temps sur `origin`.

## Vérifier que tout est aligné

```bash
git fetch lenny feature/refonte-v3-lua-buffer

# LA vérification : "0	0" = branches identiques
# (gauche = nos commits en avance, droite = commits en retard)
git rev-list --left-right --count HEAD...lenny/feature/refonte-v3-lua-buffer

# Comparer les empreintes — deux hashes identiques = même contenu exact
git rev-parse HEAD lenny/feature/refonte-v3-lua-buffer

# Aucune différence de fichiers — une sortie vide confirme
git diff --stat HEAD lenny/feature/refonte-v3-lua-buffer

# Où en est notre fork ? (à faire après le push : doit afficher "0	0")
git fetch origin feature/refonte-v3-lua-buffer
git rev-list --left-right --count HEAD...origin/feature/refonte-v3-lua-buffer

# Vue d'ensemble des trois branches
git log --oneline --graph --decorate -15 HEAD lenny/feature/refonte-v3-lua-buffer origin/feature/refonte-v3-lua-buffer
```

Note : `...` (trois points) pour `rev-list --left-right --count`,
`..` (deux points) pour lister les commits d'un côté seulement.

## Points d'attention

- **`lso.db`** — base SQLite locale, suivie par git et supprimée par certains
  commits de Lenny. En faire une copie avant la fusion si les données comptent :
  `cp lso.db ../lso.db.backup`
- **`lso.toml`** — fichier de configuration introduit par Lenny. Relire les valeurs
  après une mise à jour ; un réglage local y serait écrasé.
- **`git fetch` ne modifie jamais le répertoire de travail** — sans risque, à lancer
  librement pour inspecter avant de décider.
- Vérifier que la compilation passe après une mise à jour : `cargo check`

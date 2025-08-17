# Versioning by cubes

A simple tool to manage your project versioning.

## Prérequis

- Rust and Cargo installed
- Variables environment:
    - AK_USERNAME: your identifiant
    - AK_EMAIL: your email
    - EDITOR: your favorite editor for commit message

Examples:

```shell script
# bash
export AK_USERNAME="seido"
export AK_EMAIL="seidogitan@gmail.com"
export EDITOR="vim"     # ou "nano", "code -w", etc.
```

## Installation

```shell script
# bash
cargo install eikyu
```

## Quick starter

In the directory of your project:

```shell script
# bash
ak init                           # initialize .eikyu/ & month's cube
ak inscribe                       # take a snapshot of files
ak seal -t feat -s "ma feature"   # create a commit
ak timeline                       # see history
ak view                           # display the last commit
```

## Commandes

- init: initialize the .eikyu/ directory and the cube for the current month

```shell script
# bash
ak init
```

- inscribe: scan a directory and take a snapshot of its files

```shell script
# bash
ak inscribe            # current directory
ak inscribe path/to/dir
```

- seal: create a commit with a message

```shell script
# bash
ak seal                           # interactif (type, summary, body via $EDITOR)
ak seal -t feat -s "title" -b "body of the commit"
```

- timeline: affiche les commits (ordre chronologique)

```shell script
# bash
ak timeline                       # heure locale de l’utilisateur
ak timeline --utc                 # affichage en UTC
ak timeline --iso                 # format ISO 8601 avec décalage
ak timeline --utc --iso
```

Output example:

```
#12 [feat] ajoute la timeline @ 2025-08-16 11:39:11
```

- view: affiche le dernier commit

```shell script
# bash
ak view
```

## Comment ça marche

- Stockage
    - Les données sont enregistrées dans .eikyu/
        - .eikyu/cubes/YYYY-MM/<AK_USERNAME>.cube
        - .eikyu/tree/<AK_USERNAME> (état du répertoire, réservé/évolutif)
        - .eikyu/branches (réservé)
- Commits
    - Chaque commit est un événement avec:
        - id: entier croissant
        - parent: id du commit précédent (ou null)
        - ty: type (feat, fix, refactor, docs, test, chore, …)
        - summary: court résumé
        - body: détails
        - author, author_email
        - timestamp: millisecondes depuis l’epoch (UTC) au moment du commit
- Timeline
    - Lit les événements “commit” du cube courant (mois/AK_USERNAME) et affiche type, summary, date/heure.
    - Conversion de temps robuste: passe par UTC et formate en Local (ou en UTC si demandé).

## Bonnes pratiques

- Définis AK_USERNAME de manière stable pour retrouver tes commits du mois en cours.
- Utilise ak inscribe avant ak seal pour que l’état des fichiers soit pris en compte.
- Utilise --iso lorsque tu partages des dates (non ambigu).

## Dépannage

- “No commits.”: il faut d’abord ak inscribe puis ak seal.
- Erreur d’éditeur dans seal: vérifie $EDITOR (ex: EDITOR="vim" ou "code -w").
- Problèmes d’heure:
    - ak timeline --utc --iso pour vérifier que l’instant est correct.
    - Les anciens commits pouvaient stocker un timestamp en nanosecondes; la timeline sait les détecter et les
      convertir. Les commits récents utilisent des millisecondes.
- Recompiler après des changements:

```shell script
# bash
cargo clean && cargo build --release
./target/release/ak timeline
```

## Exemples

Créer un commit complet en no interactivement (type, summary, body):

```shell script
# bash
ak inscribe
ak seal -t fix -s "corrige l’erreur X" -b "détails de la correction"
```

Display the history in ISO 8601:

```shell script
# bash
ak timeline --iso
# or in UTC + ISO:
ak timeline --utc --iso
```

## Licence

[AGPL-3.0](https://raw.githubusercontent.com/hackiado/eikyu/refs/heads/main/LICENSE)

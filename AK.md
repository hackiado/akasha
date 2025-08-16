Voici une proposition de README pour ton projet. Tu peux la copier-coller dans un fichier README.md à la racine.

# ak — un mini VCS par “cubes”

ak est un outil en ligne de commande qui enregistre des “événements” (commits) dans des fichiers appelés “cubes”. Chaque cube correspond à un mois donné et à un utilisateur. L’objectif est d’offrir un flux simple:
- insérer un état du répertoire (inscribe),
- créer un commit avec message (seal),
- visualiser l’historique (timeline) et le dernier commit (view).

## Prérequis

- Rust et Cargo installés
- Variables d’environnement:
    - AK_USERNAME: ton identifiant
    - AK_EMAIL: ton email
    - EDITOR: éditeur pour le message de commit (si édition interactive)

Exemple:
```shell script
# bash
export AK_USERNAME="ton_nom"
export AK_EMAIL="ton.email@example.com"
export EDITOR="nano"     # ou "vim", "code -w", etc.
```


## Installation

```shell script
# bash
cargo build --release
# binaire: ./target/release/ak
```


Optionnel: ajoute-le à ton PATH:
```shell script
# bash
sudo cp ./target/release/ak /usr/local/bin/
```


## Démarrage rapide

Dans le répertoire de ton projet:

```shell script
# bash
ak init                           # initialise .eikyu/ et le cube du mois
ak inscribe                       # prend un snapshot des fichiers
ak seal -t feat -s "ma feature"   # crée un commit (ouvre $EDITOR pour le body si -b manquant)
ak timeline                       # affiche l’historique
ak view                           # affiche le dernier commit
```


## Commandes

- init: initialise la structure
```shell script
# bash
ak init
```


- inscribe: scanne un répertoire et enregistre son état dans le cube
```shell script
# bash
ak inscribe            # dossier courant
ak inscribe path/to/dir
```


- seal: crée un commit avec message
```shell script
# bash
ak seal                           # interactif (type, summary, body via $EDITOR)
ak seal -t feat -s "titre" -b "corps du message"
```


- timeline: affiche les commits (ordre chronologique)
```shell script
# bash
ak timeline                       # heure locale de l’utilisateur
ak timeline --utc                 # affichage en UTC
ak timeline --iso                 # format ISO 8601 avec décalage
ak timeline --utc --iso
```


Exemple de sortie:
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
    - Les anciens commits pouvaient stocker un timestamp en nanosecondes; la timeline sait les détecter et les convertir. Les commits récents utilisent des millisecondes.
- Recompiler après des changements:
```shell script
# bash
cargo clean && cargo build --release
./target/release/ak timeline
```


## Exemples

Créer un commit complet en non-interactif:
```shell script
# bash
ak inscribe
ak seal -t fix -s "corrige l’erreur X" -b "détails de la correction"
```


Afficher l’historique en ISO 8601:
```shell script
# bash
ak timeline --iso
# ou en UTC + ISO:
ak timeline --utc --iso
```


## Roadmap (idées)

- Branches et références symboliques
- Diff natif entre snapshots
- Filtres timeline (par type, texte, auteur)
- Export/Import de cubes
- Intégration CI

## Licence

AGPL-3.0

---

Besoin d’un badge, d’exemples supplémentaires ou d’un GIF de démo dans le README ? Dis-moi ce que tu préfères.
# Akasha (Eikyu) — un système vivant qui transforme l’information en sagesse

Eikyu est Akasha. Ce n’est pas une simple base de données: c’est un écosystème vivant, local-first, conçu pour comprendre l’histoire, le contexte et l’intention des informations afin de générer de la sagesse actionnable.

## Vision

- Comprendre plutôt que stocker: chaque donnée est replacée dans un récit (le Métarécit) pour révéler sens, causalité et finalité.
- Mémoire immuable et événementielle: rien n’est écrasé ni supprimé; l’histoire s’écrit par ajouts successifs.
- Système durable et portable: les “cubes” sont des fichiers append-only autonomes, faciles à archiver, synchroniser et auditer.

## Principes clés

- Dualité Phénomène/Noumène
  - Phénomène: le fait concret (quoi/comment).
  - Noumène: le sens et l’intention (pourquoi/finalité).
  - Toute information porte ces deux faces, considérées conjointement.

- Immutabilité et traçabilité
  - Le passé est fondation. Chaque action est un nouvel événement qui enrichit la trame chronologique.

- Récursivité fractale
  - Tout point d’information peut contenir un sous-univers complet (emboîtements infinis et maîtrisés).

- Topologie métamorphique
  - La structure interne s’adapte (cube/réseau/arbre) pour accélérer les usages réels, sans index manuels fragiles.

## Composants

- Cubes (append-only)
  - Fichiers horodatés par période (ex. mois) et auteur. Autonomes, auditables, transférables.

- Événements
  - Enregistrements immuables: id croissant, type (phenomenon), contenu (noumenon), timestamp, intégrité (CRC).

- Timeline
  - Rejeu chronologique des événements pertinents (commits, snapshots…) pour comprendre l’évolution et reconstruire un état.

- Arbre de travail (tree)
  - Espace dédié aux snapshots d’un répertoire (comparaison, préparation de commit).

- Perspectives et sonar (vision)
  - Agents/points de vue thématiques et recherche “résonante” dans l’espace sémantique (concept évolutif).

## Ce que permet Akasha

- Capturer des états (snapshots) et créer des commits contextualisés (type, résumé, auteur, intention).
- Rejouer l’histoire pour auditer, expliquer, comparer, apprendre.
- Transporter/archiver la connaissance de manière robuste et locale (sans serveur obligatoire).
- Servir de base à des outils narratifs: diff, rapports, vues métier, agents (“perspectives”).

## Flux type

1) Initialiser le dépôt Akasha dans un projet (création de la structure et du cube courant).
2) Inscrire l’état courant (snapshot des fichiers utiles).
3) Sceller (commit) avec métadonnées et message (sens/intention).
4) Consulter la timeline ou le dernier commit pour naviguer dans l’histoire.

Pour la CLI et les commandes (init, inscribe, seal, timeline, view), voir AK.md.

## Valeurs et garanties

- Local-first: fonctionne hors-ligne; contrôle complet par l’utilisateur.
- Durabilité: append-only avec contrôle d’intégrité; index reconstruisible depuis le fichier.
- Portabilité: un cube = un artefact autoportant.
- Lisibilité/outillage: charges utiles en texte/JSON, facilement scriptables et analytiques.

## Cas d’usage

- Journalisation durable d’un projet (fichiers, décisions, intentions).
- “Boîte noire” locale: tracer étapes et transformations clés.
- Archivage périodique du travail par auteur avec rejouabilité.
- Base simple pour construire des vues narratives et des analyses.

## Roadmap (évolutif)

- Références/branches légères et filtres de timeline (type, texte, plage, auteur).
- Diff intégré entre snapshots successifs.
- Fusion/import/export de cubes et “univers-bulles” (clones éphémères).
- Perspectives actives (agents) et sonar sémantique.
- Adaptation d’interface par résonance (vue développeur/manager, etc.).

## Licence

AGPL-3.0

Ressources
- Guide CLI: voir AK.md 
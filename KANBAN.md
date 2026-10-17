# DeskCheck · Tableau Kanban

Ce fichier suit toutes les tâches jusqu'à la version 0.1.0.
On le met à jour à la fin de chaque tâche, dans la même branche que la tâche.
Le même tableau existe en ligne : [Kanban DeskCheck](https://claude.ai/artifact/SEKGtbp6ACVXhpff8rK89i) (privé, mis à jour en même temps).

**Avancement : 1 / 37 tâches terminées**

| Colonne | Tâches |
|---|---|
| 🚧 En cours | aucune |
| 🔜 À faire (prochaines) | DC-02, DC-03, DC-04 |
| 📋 Backlog | DC-05 à DC-37 |
| 🖥️ À tester sur ton PC | aucune |
| ✅ Terminé | DC-01 |

---

## Règles de travail

**Branches**
- `main` : versions publiées seulement.
- `develop` : branche d'intégration.
- Une tâche = une branche locale `feat/dc-XX-nom-court`, créée depuis `develop`.
- Fin de tâche : merge en avance rapide (`git merge --ff-only`) dans `develop`.

**Commits**
- Format [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/#specification) :
  `type(portée optionnelle): description`.
- Types utilisés : `feat`, `fix`, `docs`, `chore`, `ci`, `build`, `refactor`, `test`, `perf`, `style`.
- Changement cassant : `!` avant les deux-points, ou pied de page `BREAKING CHANGE: ...`.
- Auteur : `Godwin <koffigodwin96@gmail.com>`.

**Documentation d'abord**
1. Rust et Cargo : seulement la documentation officielle
   ([The Rust Book](https://doc.rust-lang.org/book/), [Cargo Book](https://doc.rust-lang.org/cargo/),
   [Reference](https://doc.rust-lang.org/reference/), [std](https://doc.rust-lang.org/std/)).
2. Bibliothèques : lire leur documentation (docs.rs, livre officiel) avant tout code.
3. Problème non résolu après la doc : chercher dans les issues GitHub du projet, puis les forums.
4. Chaque tâche liste les documents à lire.

**Définition de « terminé »**
- `cargo fmt --check`, `cargo clippy` (sans avertissement) et `cargo test` passent.
- La CI passe dans un conteneur Ubuntu 22.04.
- Les critères d'acceptation de la tâche sont tous cochés.
- Ce fichier est mis à jour.

**Cible technique**
- Ubuntu 22.04 : GTK 4.6, libadwaita 1.1, GNOME 42, PulseAudio (ou PipeWire via `pipewire-pulse`).
- Interface : `gtk4` (option `v4_6`) et `libadwaita` (option `v1_1`).
- Son : `libpulse-binding` (API PulseAudio, compatible PipeWire).
- Écrans : D-Bus `org.gnome.Mutter.DisplayConfig` avec `zbus`.

**Colonne « À tester sur ton PC »**
- Ce qui ne peut pas être testé dans le nuage (son, écrans, barre du haut) attend là
  que Godwin le teste sur sa machine. La tâche passe en « Terminé » après ce test.

**Légende** : 🧪 spike (recherche, risque à lever) · ❓ décision de Godwin

---

## 🚧 En cours

_Aucune tâche._

---

## 🔜 À faire (prochaines)

### DC-02 · Licence du projet ❓
- **But** : choisir la licence (ex. MIT, Apache-2.0, GPL-3.0) et l'ajouter.
- **Critères**
  - [ ] Choix fait par Godwin.
  - [ ] Fichier `LICENSE` et champ `license` du workspace.

### DC-03 · Modèle de domaine
- **But** : décrire en Rust pur les appareils son, les flux des applications et les écrans.
  Aucune dépendance système, tout est testable.
- **Docs** : The Rust Book (structs, enums, traits), API Guidelines.
- **Critères**
  - [ ] Types : sortie, micro, « retour » (monitor), flux de lecture, flux d'enregistrement, écran.
  - [ ] Identifiant stable pour chaque appareil (nom PulseAudio, triple EDID fabricant/modèle/série).
  - [ ] Tests unitaires.

### DC-04 · Connexion PulseAudio 🧪
- **But** : se connecter au serveur son et lire les sorties, micros, retours et flux.
- **Docs** : `libpulse-binding` (context, mainloop threaded, introspect), doc PulseAudio.
- **Critères**
  - [ ] Connexion avec le mainloop « threaded ».
  - [ ] Liste des sinks, sources, sink-inputs, source-outputs et valeurs par défaut.
  - [ ] Conversion vers le modèle de domaine.
  - [ ] Marche aussi avec PipeWire (`pipewire-pulse`).

---

## 📋 Backlog

### Phase 1 · Lire l'état du son

#### DC-05 · Classer les appareils son
- **But** : reconnaître un « retour » (monitor), un casque, un HDMI, un USB, le micro interne.
- **Docs** : propriétés PulseAudio (`device.form_factor`, `device.bus`, `monitor_of_sink`).
- **Critères**
  - [ ] Un retour n'est jamais classé comme micro.
  - [ ] Type d'appareil et icône déduits des propriétés.
  - [ ] Tests avec des données enregistrées.

#### DC-06 · Commande `deskcheck audio`
- **But** : premier outil utile, en terminal : qui écoute quoi.
- **Docs** : `clap` (derive).
- **Critères**
  - [ ] Affiche sortie et micro par défaut, et les applications avec leur appareil.
  - [ ] Signale une application qui écoute un retour.

### Phase 2 · Lire l'état des écrans

#### DC-07 · Lecture des écrans via Mutter 🧪
- **But** : lire les écrans avec `org.gnome.Mutter.DisplayConfig.GetCurrentState`.
- **Docs** : `zbus` (proxy, blocking), XML de l'interface DisplayConfig dans le dépôt Mutter.
- **Critères**
  - [ ] Connecteur, fabricant, modèle, série, modes, écran intégré, écran principal, disposition.
  - [ ] Message clair si la session n'est pas GNOME.

#### DC-08 · Commande `deskcheck displays`
- **Critères**
  - [ ] Liste les écrans avec numéro, nom, résolution, échelle, principal.

### Phase 3 · Noms et configuration

#### DC-09 · Fichier de configuration
- **But** : garder les noms donnés aux appareils et les couleurs des écrans.
- **Docs** : `serde`, `toml`, `directories` (dossier XDG).
- **Critères**
  - [ ] Fichier TOML dans `~/.config/deskcheck/`.
  - [ ] Écriture sûre (fichier temporaire puis renommage).
  - [ ] Version du format pour les migrations futures.

#### DC-10 · Renommer un appareil
- **Critères**
  - [ ] Commandes CLI pour nommer un appareil son ou un écran.
  - [ ] Le nom donné s'affiche partout, le nom technique reste disponible.

### Phase 4 · Diagnostic

#### DC-11 · Moteur de diagnostic
- **But** : des règles pures qui trouvent les problèmes courants.
- **Critères**
  - [ ] Règles : application qui écoute un retour, micro coupé, aucun micro, micro par défaut = retour, volume à 0.
  - [ ] Chaque problème propose une correction.
  - [ ] Tests unitaires pour chaque règle.

#### DC-12 · Commande `deskcheck doctor`
- **Critères**
  - [ ] Affiche les problèmes trouvés.
  - [ ] Option `--fix` pour appliquer la correction proposée.

### Phase 5 · Agir sur le son

#### DC-13 · Changer les appareils et les volumes
- **Critères**
  - [ ] Choisir la sortie et le micro par défaut.
  - [ ] Volume et coupure du son.
  - [ ] Déplacer le flux d'une application vers un autre appareil, sans le couper.

#### DC-14 · Barres de niveau en direct 🧪
- **Docs** : flux de mesure de crête PulseAudio (`PEAK_DETECT`).
- **Critères**
  - [ ] Niveau de chaque micro, mis à jour environ 20 fois par seconde.
  - [ ] Faible charge processeur.

#### DC-15 · Tester le son et le micro
- **Critères**
  - [ ] Jouer un son court sur une sortie.
  - [ ] Enregistrer 3 secondes sur un micro, puis les réécouter.

#### DC-16 · Événements en direct
- **Critères**
  - [ ] Abonnement aux changements PulseAudio (appareil branché, flux nouveau).
  - [ ] Abonnement au signal `MonitorsChanged` de Mutter.
  - [ ] Un seul canal d'événements pour l'interface.

### Phase 6 · Interface graphique

#### DC-17 · Fenêtre GTK compatible Ubuntu 22.04 🧪
- **But** : vérifier tôt que GTK 4.6 et libadwaita 1.1 compilent et tournent.
- **Docs** : livre « GUI development with Rust and GTK 4 », docs `gtk4` et `libadwaita`.
- **Critères**
  - [ ] Fenêtre vide avec barre d'en-tête, construite dans la CI Ubuntu 22.04.

#### DC-18 · Structure de l'application
- **Critères**
  - [ ] `AdwApplication`, barre latérale à 5 sections, badge du lieu.
  - [ ] Le travail système tourne hors du fil de l'interface, résultats envoyés à l'interface.

#### DC-19 · Page « Maintenant »
- **Critères**
  - [ ] Cartes sortie, micro, applications, écrans, conformes à la maquette.

#### DC-20 · Page « Audio »
- **Critères**
  - [ ] Onglets Sorties, Micros, Applications ; retours rangés à part.
  - [ ] Bouton Tester et bouton Renommer.

#### DC-21 · Page « Écrans »
- **Critères**
  - [ ] Carte des écrans avec numéro et couleur ; réglages de l'écran choisi.

#### DC-22 · Page « Diagnostic »
- **Critères**
  - [ ] Liste des vérifications avec état, correction en un clic.

#### DC-23 · Alerte en direct
- **Critères**
  - [ ] Bandeau sur « Maintenant » quand une application écoute un retour.
  - [ ] Bouton « Basculer » qui corrige sans couper l'appel.

### Phase 7 · Agir sur les écrans

#### DC-24 · Identifier les écrans
- **Docs** : `gtk::Window::fullscreen_on_monitor`, `gdk::Display::monitors`.
- **Critères**
  - [ ] Un grand numéro coloré sur chaque vrai écran pendant 5 secondes.
  - [ ] Échap ferme tout.

#### DC-25 · Appliquer une disposition d'écrans
- **Docs** : `ApplyMonitorsConfig` (méthode temporaire, puis persistante).
- **Critères**
  - [ ] Écran principal, résolution, échelle, position.
  - [ ] Retour arrière automatique si l'utilisateur ne confirme pas.

#### DC-26 · Sur quel écran est la souris ? 🧪
- **But** : sous Wayland, une application ne voit pas la position globale du pointeur.
  Chercher une solution (petite extension GNOME Shell, ou autre).
- **Critères**
  - [ ] Solution choisie et documentée, ou fonction retirée de la v0.1.0.

### Phase 8 · Lieux

#### DC-27 · Modèle des lieux et détection
- **Docs** : D-Bus NetworkManager (nom du Wi-Fi actif).
- **Critères**
  - [ ] Un lieu = conditions (écran branché, Wi-Fi) + préférences son, micro, écrans.
  - [ ] Détection du lieu actuel, tests unitaires.

#### DC-28 · Appliquer un lieu avec Annuler
- **Docs** : `gio::Notification`.
- **Critères**
  - [ ] Application automatique, notification avec bouton Annuler.
  - [ ] Modes : appliquer, demander, ne rien faire.

#### DC-29 · Page « Lieux »
- **Critères**
  - [ ] Liste des lieux, conditions, listes de préférence réordonnables.

### Phase 9 · Au quotidien

#### DC-30 · Icône dans la barre du haut 🧪
- **But** : panneau rapide. GNOME n'a pas de zone de notification par défaut ;
  Ubuntu fournit l'extension AppIndicator.
- **Docs** : spécification StatusNotifierItem, caisse `ksni`.
- **Critères**
  - [ ] Icône et menu rapide (sortie, micro, couper le micro, ouvrir DeskCheck).

#### DC-31 · Notifications au branchement
- **Critères**
  - [ ] « Casque branché » avec Annuler et OK.

#### DC-32 · Assistant de premier lancement
- **Critères**
  - [ ] Nommer les sorties, les micros (en parlant dedans), les écrans, puis les lieux.

#### DC-33 · Démarrage avec la session
- **Critères**
  - [ ] Fichier autostart XDG, option dans les préférences.

### Phase 10 · Livraison

#### DC-34 · Paquet .deb
- **Docs** : `cargo-deb`, spécification Desktop Entry, AppStream.
- **Critères**
  - [ ] Paquet installable sur Ubuntu 22.04, fichier `.desktop`, icône, métadonnées.

#### DC-35 · Documentation utilisateur
- **Critères**
  - [ ] README avec captures, installation, limites connues.

#### DC-36 · Test sur la vraie machine
- **Critères**
  - [ ] Parcours A à D validés au bureau et à la maison (voir le croquis UX).

#### DC-37 · Version 0.1.0
- **Critères**
  - [ ] Journal des changements tiré des commits, tag `v0.1.0`, merge sur `main`.

---

## 🖥️ À tester sur ton PC

_Aucune tâche._

---

## ✅ Terminé

### DC-01 · Socle du projet
- **But** : un workspace Cargo propre, des outils de qualité et une CI.
- **Docs lues** : Cargo Book (workspaces, manifest, lints, resolver, rust-version, guide CI),
  Edition Guide 2024, rustup (installation), rustfmt (options stables),
  Clippy (configuration), GLib (`g_application_id_is_valid`), GitHub Actions (jobs en conteneur).
- **Critères**
  - [x] Workspace Cargo avec les caisses `deskcheck-core` (lib) et `deskcheck` (binaire CLI).
  - [x] Champs communs hérités du workspace (édition 2024, resolver 3, Rust 1.85, lints).
  - [x] `rustfmt.toml`, `clippy.toml` et lints Clippy partagés (`all` + `pedantic`).
  - [x] CI GitHub Actions : fmt, clippy, tests, et vérification Rust 1.85, dans un conteneur `ubuntu:22.04`.
  - [x] `CONTRIBUTING.md` : branches, commits, règle « documentation d'abord ».
  - [x] `cargo build`, `cargo test`, `cargo clippy -D warnings` passent en local.
  - [x] La CI passe sur GitHub (17 oct. 2026, Clippy 1.99 et Rust 1.85).
- **Notes** : Clippy 1.99 a ajouté la règle `assert_is_empty`, corrigée avec `assert_ne!`.
  Les messages de Clippy remontent en annotations GitHub, lisibles même sans le journal.
- **Décision** : identifiant d'application `io.github.geekabel.DeskCheck`.

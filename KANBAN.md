# DeskCheck · Tableau Kanban

Ce fichier suit toutes les tâches jusqu'à la version 0.1.0.
On le met à jour à la fin de chaque tâche, dans la même branche que la tâche.
Le même tableau existe en ligne : [Kanban DeskCheck](https://claude.ai/artifact/SEKGtbp6ACVXhpff8rK89i) (privé, mis à jour en même temps).

**Avancement : 26 / 38 tâches terminées**

| Colonne | Tâches |
|---|---|
| 🚧 En cours | aucune |
| 🔜 À faire (prochaines) | DC-30, DC-31 |
| 📋 Backlog | DC-32 à DC-38 |
| 🖥️ À tester sur ton PC | DC-27, DC-28, DC-29 |
| ✅ Terminé | DC-01 à DC-26 |

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

### DC-30 · Icône dans la barre du haut 🧪
- **But** : panneau rapide. GNOME n'a pas de zone de notification par défaut ;
  Ubuntu fournit l'extension AppIndicator.
- **Docs** : spécification StatusNotifierItem, caisse `ksni`.
- **Critères**
  - [ ] Icône et menu rapide (sortie, micro, couper le micro, ouvrir DeskCheck).

### DC-31 · Notifications au branchement
- **Critères**
  - [ ] « Casque branché » avec Annuler et OK.

---

## 📋 Backlog

### Phase 5 · Agir sur le son

### Phase 6 · Interface graphique

### Phase 7 · Agir sur les écrans

### Phase 8 · Lieux

### Phase 9 · Au quotidien

#### DC-32 · Assistant de premier lancement
- **Critères**
  - [ ] Nommer les sorties, les micros (en parlant dedans), les écrans, puis les lieux.

#### DC-33 · Démarrage avec la session
- **Critères**
  - [ ] Fichier autostart XDG, option dans les préférences.

### Phase 10 · Livraison

#### DC-38 · Traduction française
- **But** : l'interface suit la langue d'Ubuntu. Textes en anglais dans le code, traduction
  française à côté (choix de Godwin, 17 oct. 2026).
- **Docs** : caisse `gettext-rs`, manuel GNU gettext (`xgettext`, fichiers `.po` et `.mo`).
- **Critères**
  - [ ] Textes de la fenêtre et de la CLI passés par gettext.
  - [ ] Fichier `po/fr.po` complet : DeskCheck est en français sur un Ubuntu en français.
  - [ ] Les fichiers de traduction sont installés avec le paquet (DC-34).

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
  - [ ] Classement du casque, de la webcam et de l'écran du bureau (reste de DC-05).
  - [ ] Lecture des écrans au bureau avec l'écran externe (reste de DC-07).

#### DC-37 · Version 0.1.0
- **Critères**
  - [ ] Journal des changements tiré des commits, tag `v0.1.0`, merge sur `main`.

---

## 🖥️ À tester sur ton PC

### DC-29 · Page « Lieux » 🧪
- **But** : créer et régler les lieux dans la fenêtre, comme la maquette « Lieux ».
- **Docs lues** : `gtk4` 0.10 (`DragSource`, `DropTarget`, `gdk::ContentProvider::for_value`,
  `WidgetPaintable`, `MenuButton::set_child` depuis GTK 4.6, `CheckButton::set_group`),
  maquette « Lieux ».
- **Critères**
  - [x] Liste des lieux à gauche, tag « Now » sur le lieu reconnu, « Add a place ».
  - [x] Un nouveau lieu prend ce qui est en place : sorties et micros (ceux par défaut en
    premier), disposition des écrans, et l'indice le plus parlant (écran externe, sinon Wi-Fi).
  - [x] « Recognized when » : conditions avec « or », ajout (écrans branchés, Wi-Fi actuel,
    aucun écran externe) et retrait.
  - [x] Sorties et micros par préférence : glisser une ligne pour changer l'ordre, menu de
    ligne (« Never use it here », retirer), « Add an output », « Add a microphone ».
  - [x] Écrans : petit plan de la disposition gardée, « Take the current layout ».
  - [x] « When this place is recognized » : appliquer, demander, ne rien faire.
  - [x] Renommer et supprimer un lieu (formulaire « Rename »).
  - [x] Vu ici : ajouter, renommer, glisser, ajouter une condition, changer le mode, page vide.
- **Décisions**
  - Chaque changement est enregistré tout de suite, sans bouton « Save ».
  - Renommer le lieu reconnu ne compte pas comme une nouvelle arrivée.
  - Changer les réglages du lieu où tu es ne les applique pas tout de suite : ils servent à la
    prochaine arrivée.
  - Ajouts à la maquette, nécessaires : boutons « Add an output » et « Add a microphone »,
    menu « ⋯ » sur chaque ligne, croix pour retirer une condition, « Delete this place ».
- **À tester** : `cargo run -p deskcheck-app`, page Places : crée « Office » au bureau et
  « Home » à la maison, puis débranche et rebranche l'écran pour voir le lieu reconnu.

### DC-28 · Appliquer un lieu avec Annuler 🧪
- **But** : quand DeskCheck reconnaît un lieu, mettre ses choix en place, et pouvoir revenir en
  arrière.
- **Docs lues** : `gio::Notification` et `Application::send_notification` (fichier `.desktop`
  au nom de l'id de l'app, `DBusActivatable`, boutons sur des actions `app.`), interface
  D-Bus `org.gnome.Mutter.DisplayConfig` (méthode « temporary »), maquette « Maintenant »
  (ligne du lieu en bas), maquette « Lieux ».
- **Critères**
  - [x] À l'arrivée : sortie son et micro par défaut (le premier branché des listes), puis
    disposition des écrans gardée. Seul ce qui diffère est changé.
  - [x] Notification « Place recognized: Office » qui dit ce qui a changé, bouton Undo.
  - [x] Modes : appliquer (et prévenir), demander avant (bouton Apply), ne rien faire.
  - [x] Badge « Place: Office » en haut, ligne en bas de Now comme la maquette :
    « Place recognized: Office, because ... Place settings applied at 8:31. [Undo] ».
  - [x] Undo remet la sortie, le micro et la disposition d'avant.
  - [x] Nom du Wi-Fi relu toutes les 5 s.
  - [x] Vu ici avec un faux GNOME et un faux serveur de notifications : appliquer, Undo par la
    notification, demander puis Apply dans la page.
  - [x] `data/` : fichier `.desktop`, service D-Bus et `install-dev.sh`.
- **Décisions**
  - Les écrans d'un lieu sont appliqués avec la méthode « temporary » de GNOME : pas de fenêtre
    « garder ces réglages ? », puisque DeskCheck offre son propre Undo. GNOME garde ses
    dispositions enregistrées telles quelles.
  - Une disposition dont un écran manque n'est pas appliquée (le son l'est quand même).
  - DeskCheck agit une fois par arrivée, aussi au démarrage. Après Undo, il ne réapplique pas
    tant que tu restes au même lieu.
  - PulseAudio 15 déplace les applications qui suivent la sortie par défaut ; celles que tu as
    mises ailleurs à la main restent où elles sont.
- **À tester** : `data/install-dev.sh`, puis lance DeskCheck depuis les Activités et crée
  un lieu dans la page Places (DC-29). Vérifier : la notification et son Undo, la ligne en bas de Now, et qu'aucune fenêtre de
  GNOME ne demande de garder les écrans.

### DC-27 · Modèle des lieux et détection 🧪
- **But** : décrire un lieu (bureau, maison...) et reconnaître celui où est l'ordinateur.
- **Docs lues** : interface D-Bus de NetworkManager (`ActiveConnections`,
  `Connection.Active` : `Type`, `Devices` ; `Device.Wireless.ActiveAccessPoint` ;
  `AccessPoint.Ssid`), `zbus` 5 (proxy bloquant, `CacheProperties::No`, connexion pair à pair
  pour les tests), maquette « Lieux ».
- **Critères**
  - [x] Un lieu = indices (un écran branché, le nom du Wi-Fi, aucun écran externe) + sorties
    son préférées, sorties à éviter, micros préférés, disposition des écrans gardée, et quoi
    faire en arrivant (appliquer, demander, rien).
  - [x] Détection : l'indice le plus parlant gagne (écran, puis Wi-Fi, puis « aucun écran
    externe ») ; à égalité, le premier lieu de la liste. Tests unitaires.
  - [x] Les lieux sont gardés dans le fichier de réglages (`[[places]]`), testé aller-retour.
  - [x] Nouvelle caisse `deskcheck-network` : nom du Wi-Fi lu à NetworkManager, testée avec un
    faux NetworkManager.
  - [x] `deskcheck place` : « You are at Office, because the Wi-Fi is UTBM-Office. »
- **Décisions**
  - Un écran sans numéro de série utile correspond à tout écran du même modèle.
  - Si le Wi-Fi ou les écrans ne se lisent pas, la détection continue avec ce qui se lit.
- **À tester** : `cargo run -p deskcheck -- place` doit montrer le nom de ton Wi-Fi et tes
  écrans (aucun lieu n'est encore enregistré : la page Lieux vient avec DC-29).

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

### DC-02 · Licence du projet
- **But** : choisir la licence et l'ajouter.
- **Docs lues** : Cargo Book (champs `license` et `license-file`), liste SPDX (texte officiel MIT).
- **Critères**
  - [x] Choix fait par Godwin : MIT.
  - [x] Fichier `LICENSE` (texte SPDX, « Copyright (c) 2026 Godwin »).
  - [x] `license = "MIT"` dans le workspace, hérité par les deux caisses.
  - [x] Mention de la licence dans le README.

### DC-03 · Modèle de domaine
- **But** : décrire en Rust pur les appareils son, les flux des applications et les écrans.
  Aucune dépendance système, tout est testable.
- **Docs lues** : The Rust Book (newtype), Rust API Guidelines (checklist, C-COMMON-TRAITS),
  API PulseAudio (`pa_source_info`, `pa_source_output_info`, propriétés), interface
  D-Bus `org.gnome.Mutter.DisplayConfig` (`GetCurrentState`).
- **Critères**
  - [x] Types : `Output` (sortie), `Input` avec `InputKind::Capture` (micro) ou
    `InputKind::Monitor` (retour), `Playback`, `Recording`, `Screen` ; états `AudioState` et `DisplayState`.
  - [x] Identifiants stables : `DeviceId` (nom PulseAudio), `ScreenId` (fabricant, modèle, série).
  - [x] Tests unitaires (17) et exemples de documentation (5).
- **Décisions**
  - Le connecteur (`DP-1`) ne fait pas partie de l'identité d'un écran : il change avec le port.
  - Un retour garde l'identifiant de la sortie qu'il copie (`InputKind::Monitor { of }`).
  - Un écran éteint reste dans la liste, sans placement.
  - Le volume est un pourcentage du niveau normal (100 = normal, plus = amplifié).

### DC-04 · Connexion PulseAudio
- **But** : se connecter au serveur son et lire les sorties, micros, retours et flux.
- **Docs lues** : `libpulse-binding` (mainloop standard et threaded, introspect, proplist,
  volume, COMPATIBILITY.md, options `pa_vXX`), API C de PulseAudio (`pa_source_info`,
  `pa_source_output_info`).
- **Critères**
  - [x] Connexion au serveur, sans lancer de serveur s'il n'y en a pas (`NOAUTOSPAWN`).
  - [x] Liste des sinks, sources, sink-inputs, source-outputs et valeurs par défaut.
  - [x] Conversion vers le modèle de domaine (9 tests sans serveur).
  - [x] Testé ici contre un vrai serveur PulseAudio 16.1 : sorties, retours, micro coupé à 62 %,
    lecture, et une application qui enregistre un retour.
  - [x] Testé sur le PC de Godwin (Ubuntu 22.04) : `audio_dump` et les tests `--ignored` passent.
  - [x] Compatible PipeWire par conception (`pipewire-pulse` parle le même protocole).
    À revérifier sur une machine qui utilise PipeWire.
- **Comment tester** (sur Ubuntu 22.04) :
  ```sh
  sudo apt install build-essential pkg-config libpulse-dev
  cargo run -p deskcheck-pulse --example audio_dump
  cargo test -p deskcheck-pulse -- --ignored
  ```
  Le mieux : lancer `audio_dump` pendant un appel Teams, avec l'écran du bureau branché.
- **Décisions**
  - Mainloop **standard** au lieu de « threaded » : les exemples du mode threaded demandent du
    code `unsafe` (interdit dans le workspace). Le mode standard suffit pour une lecture, sans fil
    d'exécution. Les événements en direct (DC-16) tourneront dans un fil dédié.
  - Options par défaut de `libpulse-binding` (`pa_v8`) : marche avec PulseAudio 8 et plus,
    donc avec Ubuntu 22.04 (15.99) et `pipewire-pulse`.
  - Un flux dont l'appareil a disparu pendant la lecture est ignoré.
- **Remarque** : sans vrai micro, PulseAudio choisit un retour comme entrée par défaut.
  Le diagnostic (DC-11) devra le signaler.

### DC-05 · Classer les appareils son
- **But** : reconnaître un « retour » (monitor), un casque, un HDMI, un USB, le micro interne.
- **Docs lues** : propriétés PulseAudio (`device.form_factor`, `device.bus`), noms des ports
  (fichiers `alsa-mixer/paths` livrés avec PulseAudio), champ `type` des ports dans
  `libpulse-binding` (demande `pa_v14`, donc pas utilisé), icônes du thème Adwaita.
- **Critères**
  - [x] Un retour n'est jamais classé comme micro (`InputKind::Monitor`), et il prend le type
    de la sortie qu'il copie.
  - [x] Type (`Form`), branchement (`Connection`) et icône déduits des propriétés (`classify`).
  - [x] Tests sur 10 cas au format PulseAudio d'Ubuntu 22.04 (haut-parleurs et micro du portable,
    casque jack, HDMI et son retour, casque USB, webcam USB, Bluetooth, appareil virtuel).
  - [x] Testé ici contre un vrai serveur PulseAudio : « screen (HDMI) », « headset (USB) ».
  - [x] Vérifié sur le PC de Godwin (17 oct. 2026) : haut-parleurs, micro et retour intégrés bien classés.
    Casque, webcam et écran du bureau : à revérifier pendant le test final (DC-36).
- **Décisions**
  - Ordre des règles : HDMI ou DisplayPort d'abord, puis `device.form_factor`, puis le port actif.
  - Les haut-parleurs et le micro intégrés prennent l'icône « computer », comme sur la maquette.
  - Les icônes sont des noms freedesktop du thème Adwaita ; GTK ajoutera `-symbolic`.

### DC-06 · Commande `deskcheck audio`
- **But** : premier outil utile, en terminal : qui écoute quoi.
- **Docs lues** : `clap` 4.6 (tutoriel derive : `Parser`, `Subcommand`, `version`, options,
  test avec `Command::debug_assert` et `try_parse_from`).
- **Critères**
  - [x] Affiche la sortie et le micro par défaut, et les applications avec leur appareil
    (regroupées : « Firefox (3 streams) »).
  - [x] Signale une application qui écoute un retour, avec la correction proposée.
  - [x] Signale aussi un retour choisi comme entrée par défaut.
  - [x] `--verbose` : tous les appareils avec leur nom technique.
  - [x] Tests du rapport avec les données relevées sur le PC de Godwin (maison), et du cas
    Teams du bureau sur un vrai serveur PulseAudio de test.
  - [x] Lancé sur le PC de Godwin (17 oct. 2026) : sortie identique au rapport attendu par les tests.
- **Décisions**
  - Le rapport est une fonction pure (`render`), testée sans serveur son.
  - Deux premières règles dans le cœur (`recordings_from_monitors`, `default_input_monitor`),
    reprises par le moteur de diagnostic (DC-11).
  - Code de sortie : 0 si le serveur son répond, 1 sinon.

### DC-07 · Lecture des écrans via Mutter
- **But** : lire les écrans avec `org.gnome.Mutter.DisplayConfig.GetCurrentState`.
- **Docs lues** : `zbus` 5.13 (API bloquante, macro `proxy`, macro `interface` et `out_args`,
  connexions pair à pair, `method_timeout`, types d'erreur), `zvariant` (`OwnedValue`),
  XML de l'interface DisplayConfig de Mutter (signature de `GetCurrentState`).
- **Critères**
  - [x] Connecteur, fabricant, modèle, série, mode actuel, écran intégré, nom affiché,
    position, échelle, écran principal (nouvelle caisse `deskcheck-mutter`).
  - [x] Message clair si la session n'est pas GNOME, testé ici sur un vrai bus de session.
  - [x] Appel arrêté après 5 secondes si GNOME ne répond pas.
  - [x] Tests sans GNOME : 12 tests unitaires et 3 tests de bout en bout avec un faux Mutter
    (connexion D-Bus pair à pair, comme le test `iface_and_proxy_unix_p2p` de zbus).
  - [x] Testé sur le PC de Godwin à la maison (17 oct. 2026) : portable LGD sur eDP-1 (principal),
    ASUS XG27ACS 27" sur HDMI-1 ; le test `--ignored` passe. Bureau : à faire pendant DC-36.
- **Décisions**
  - zbus 5.13 (et pas 6) : choisi par le résolveur de Cargo, car compatible avec Rust 1.85.
  - API bloquante de zbus : une lecture ponctuelle n'a pas besoin d'async.
  - Un écran qu'aucun « logical monitor » ne liste est branché mais éteint.
  - Les écrans en miroir partagent la même position.

### DC-08 · Commande `deskcheck displays`
- **But** : savoir quel écran est lequel, avec un numéro par écran.
- **Docs lues** : `std::fmt` (largeur `{:w$}`, alignement `<` et `>`, précision `{:.0}`),
  `clap` 4.6 derive (sous-commandes et options).
- **Critères**
  - [x] Liste les écrans allumés avec numéro, nom, résolution, échelle, principal.
  - [x] Numéros de 1 à n, de gauche à droite : les mêmes serviront pour « Identifier » (DC-24).
  - [x] Nom de GNOME, sinon le modèle de l'écran.
  - [x] Écrans branchés mais éteints listés à part.
  - [x] `--verbose` ajoute l'identité (fabricant, modèle, série), le connecteur et la position.
  - [x] Message clair et code de sortie 1 hors GNOME (testé ici).
  - [x] Tests avec tes écrans de la maison relevés par `display_dump`.
  - [x] Testé sur le PC de Godwin à la maison (17 oct. 2026) : sortie identique au test. Bureau : pendant DC-36.
- **Décisions**
  - Le rapport est une fonction pure (`render`), testée sans GNOME.
  - Échelle affichée en pourcentage (1.25 donne 125%).
  - Code de sortie : 0 si GNOME répond, 1 sinon.

### DC-09 · Fichier de configuration
- **But** : garder les noms donnés aux appareils et les couleurs des écrans.
- **Docs lues** : `serde` (attributs `default`, `skip_serializing_if`, `deny_unknown_fields`),
  `toml` 1.1 (`from_str`, `to_string_pretty`), `directories` 6 (`ProjectDirs`, code Linux),
  doc Rust `std::fs` (`rename`, `File::create_new`, `sync_all`, `read_link`).
- **Critères**
  - [x] Fichier TOML dans `~/.config/deskcheck/config.toml` (ou sous `$XDG_CONFIG_HOME`),
    nouvelle caisse `deskcheck-config`.
  - [x] Écriture sûre : fichier temporaire caché dans le même dossier, vidé sur le disque,
    puis renommé sur l'ancien. En cas d'échec, l'ancien fichier reste intact.
  - [x] Version du format (`version = 1`) pour les migrations futures.
  - [x] Noms des appareils son, noms et couleurs (`#rrggbb`) des écrans.
  - [x] 24 tests : format, versions, erreurs, vrais fichiers (dossier créé, remplacement,
    lien symbolique, échec d'écriture).
- **Décisions**
  - Le cœur reste sans `serde` : le format du fichier a ses propres structures, une par version.
  - Un fichier d'une version plus récente est refusé, pour ne pas le lire de travers.
  - Un champ inconnu est une erreur : une faute de frappe n'est jamais perdue en silence.
  - Si le même appareil apparaît deux fois, la dernière entrée gagne.
  - Un lien symbolique vers le fichier (dossier dotfiles) est gardé, la cible est remplacée.
  - Les commentaires écrits à la main sont perdus quand DeskCheck réécrit le fichier
    (le fichier le dit dans son en-tête).
  - Deux écrans du même modèle sans numéro de série ont la même identité.
- **Pour voir** : `cargo run -p deskcheck-config --example config_dump` (lecture seule).

### DC-10 · Renommer un appareil
- **But** : donner un nom parlant à un appareil son ou à un écran.
- **Docs lues** : `clap` 4.6 derive (sous-commandes imbriquées, `Args` et `flatten`,
  `group(required, multiple = false)`, `override_usage`).
- **Critères**
  - [x] Commandes `deskcheck rename output|input|screen <QUOI> <NOM>`, et `--reset` pour enlever le nom.
  - [x] `<QUOI>` : `default`, numéro d'écran, connecteur (HDMI-1), nom technique,
    ou un morceau de nom qui ne désigne qu'un seul appareil. Sinon, la liste des choix.
  - [x] Le nom donné s'affiche partout (`audio`, `displays`, problèmes et corrections).
    Un retour suit le nom de sa sortie : « Monitor of Laptop speakers ».
  - [x] `--verbose` garde le nom du système et le nom technique.
  - [x] Testé ici de bout en bout : vrai serveur PulseAudio de test, et faux GNOME sur un bus D-Bus de test.
  - [x] Testé sur le PC de Godwin (17 oct. 2026) : sortie et écran renommés, puis remis.
- **Décisions**
  - Seuls les micros se renomment : un retour suit toujours sa sortie.
  - Un nombre seul désigne toujours un numéro d'écran.
  - Un fichier de réglages cassé n'est jamais écrasé : `rename` s'arrête, les rapports
    préviennent et continuent sans les noms.
  - Le fichier n'est pas réécrit quand rien ne change.

### DC-11 · Moteur de diagnostic
- **But** : des règles pures qui trouvent les problèmes courants.
- **Docs lues** : aucune bibliothèque, Rust standard seulement (le modèle de DC-03).
- **Critères**
  - [x] Règles : application qui écoute un retour, micro par défaut = retour, aucun micro,
    micro utilisé coupé ou à 0 %, sortie utilisée coupée ou à 0 %.
  - [x] Chaque problème propose une correction : déplacer le flux, changer le micro par défaut,
    remettre le son, remettre le volume à 100 %, ou brancher un micro (conseil).
  - [x] 11 tests unitaires, au moins un par règle, et un exemple de documentation.
  - [x] Le rapport `deskcheck audio` affiche tout ce que le diagnostic trouve, avec les noms donnés.
- **Décisions**
  - Le diagnostic est dans le cœur (`deskcheck_core::audio::diagnose`) : la CLI et l'interface
    graphique utiliseront les mêmes règles.
  - Meilleur micro proposé : celui par défaut, sinon un casque, puis un micro, puis une webcam.
  - Seuls les appareils utilisés comptent (par défaut, ou utilisés par une application).
  - Un appareil coupé et à 0 % est signalé deux fois, avec les deux corrections
    (changé pendant DC-12 : sinon il fallait lancer `--fix` deux fois).
  - « Brancher un micro » n'est pas une correction automatique (`Fix::is_automatic`).

### DC-12 · Commande `deskcheck doctor`
- **But** : afficher les problèmes du son et les corriger.
- **Docs lues** : `libpulse-binding` 2.30 (`set_default_source`, `set_default_sink`,
  `move_source_output_by_name`, `move_sink_input_by_name`, `set_*_mute_by_name`,
  `set_*_volume_by_name`, `get_*_info_by_name`, `ChannelVolumes::scale`, `Operation`).
- **Critères**
  - [x] Affiche les problèmes trouvés, avec leur correction.
  - [x] Option `--fix` : applique les corrections, chacune une fois, dit ce qui a marché,
    puis vérifie à nouveau et dit ce qui reste.
  - [x] Code de sortie : 0 sans problème, 1 si le serveur son ne répond pas, 2 s'il reste des problèmes.
  - [x] Actions dans `deskcheck-pulse` : sortie et entrée par défaut, déplacer un flux,
    couper ou remettre le son, volume (l'équilibre entre canaux est gardé).
  - [x] Testé ici sur un vrai serveur PulseAudio de test : application qui écoute un retour,
    retour par défaut, micro à 0 %, sortie coupée. Un seul `--fix` corrige les quatre.
  - [x] Retour de Godwin (17 oct. 2026) : la barre de niveau de GNOME Settings écoute le retour
    des haut-parleurs. Elle était prise pour un appel, et `--fix` ne pouvait pas la déplacer
    (flux créé avec `DONT_MOVE`). Corrigé : les mesureurs de niveau sont reconnus et ignorés.
  - [x] Vérifié ici avec un vrai flux de mesure (`PEAK_DETECT`) sur un serveur de test.
  - [x] Retour de Godwin : il fallait lancer `--fix` deux fois. GNOME coupe la sortie et la met
    à 0 % quand le curseur va tout en bas ; seule la coupure était signalée. Corrigé : les deux
    problèmes sont signalés, un seul `--fix` suffit (reproduit puis vérifié sur un serveur de test).
  - [x] Testé sur le PC de Godwin (17 oct. 2026) : coupé et à 0 % (2 corrections), seulement
    à 0 % (1 correction), tout est réglé en un seul `--fix`.
- **Docs lues (correction)** : code de GNOME Settings 41 (`panels/sound/cc-level-bar.c`),
  de pavucontrol (`createMonitorStreamForSource`) et de PulseAudio 15 (`record_stream_new`
  dans `protocol-native.c`).
- **Décisions**
  - Le micro vers lequel les corrections basculent est vérifié lui aussi (trouvé pendant le test :
    sinon son volume à 0 % n'apparaissait qu'après la première correction).
  - « Brancher un micro » s'affiche comme « À faire toi-même ».
  - Un mesureur de niveau est reconnu par la méthode de rééchantillonnage `peaks` (PulseAudio),
    par son application (GNOME Settings, pavucontrol, DeskCheck) ou par le nom « Peak detect ».
    Il s'affiche « (level meter) » et n'est jamais un problème.
  - Le curseur « System Sounds » de GNOME n'est pas un appareil : c'est le volume des sons
    d'alerte. Choix de Godwin : DeskCheck ne gère pas les sons d'alerte pour le moment.

### DC-13 · Changer les appareils et les volumes
- **But** : agir sur le son à la main, depuis le terminal.
- **Docs lues** : `clap` 4.6 (`ValueEnum`, `value_parser` avec une fonction),
  `libpulse-binding` (déjà lue pour DC-12).
- **Critères**
  - [x] Choisir la sortie et le micro par défaut : `deskcheck use output|input <QUOI>`.
  - [x] Volume et coupure du son : `deskcheck volume output|input <QUOI> 80%`,
    `deskcheck mute ...` et `deskcheck unmute ...`.
  - [x] Déplacer le flux d'une application sans le couper :
    `deskcheck move playback|recording <APPLI> <QUOI>`, tous les flux de l'application à la fois.
  - [x] Testé ici de bout en bout sur un vrai serveur PulseAudio de test (deux sorties, un micro,
    une application qui joue et enregistre).
  - [x] Testé sur le PC de Godwin (17 oct. 2026) : volume, coupure et remise du son.
- **Décisions**
  - Les actions sur le serveur son viennent de DC-12 (`deskcheck_pulse::apply`).
  - Seuls les micros peuvent être choisis comme entrée : jamais un retour.
  - Volume de 0 % à 150 %, comme GNOME avec l'amplification.
  - `use` ne déplace pas les applications : PulseAudio 15 et plus les fait suivre la sortie par
    défaut, sauf celles déplacées à la main avec `move` (vérifié sur le serveur de test).
  - Les mesureurs de niveau ne sont jamais déplacés.
  - Chaque commande dit quand il n'y a rien à faire (« already the default », « already muted »).

### DC-14 · Barres de niveau en direct 🧪
- **But** : voir en direct quel micro entend ta voix.
- **Docs lues** : `libpulse-binding` 2.30 (`Stream`, `connect_record`, `FlagSet::PEAK_DETECT`,
  `peek`, `discard`, `readable_size`, `BufferAttr`, `Mainloop::prepare/poll/dispatch`),
  code de GNOME Settings 41 (`cc-level-bar.c` : 25 valeurs par seconde, fragment d'un float),
  doc Rust (`std::sync::mpsc`, `std::thread`, `std::io::IsTerminal`).
- **Critères**
  - [x] Niveau de chaque micro, mis à jour 25 fois par seconde (mesuré : 25,0 par seconde).
  - [x] Faible charge processeur (mesuré : 0,4 % d'un cœur, en version de débogage).
  - [x] `Meters` dans `deskcheck-pulse` : un fil d'exécution à part, des événements sur un canal,
    arrêt en moins de 50 ms quand on le lâche. Prêt pour l'interface graphique.
  - [x] Commande `deskcheck levels` : une barre par micro, jusqu'à Ctrl+C ou `--seconds N`.
  - [x] Les barres de DeskCheck elles-mêmes sont des mesureurs de niveau : jamais un problème.
  - [x] Testé sur le PC de Godwin (17 oct. 2026) : la barre suit la voix, comme prévu.
- **Décisions**
  - Échelle en décibels, de -60 dB (vide) à 0 dB (plein), comme l'oreille entend.
  - La barre monte tout de suite et descend doucement (vide en 0,8 s), comme dans GNOME.
  - Le fil d'exécution a sa propre connexion : la boucle de la bibliothèque ne peut pas changer de fil.
  - Une source qui envoie le son par gros paquets donne une barre par à-coups (vu avec la source
    de test `module-sine-source`). Les vrais micros envoient de petits paquets.

### DC-15 · Tester le son et le micro
- **But** : vérifier en quelques secondes qu'une sortie joue et qu'un micro entend.
- **Docs lues** : `libpulse-binding` 2.30 (`connect_playback`, `write_copy`, `writable_size`,
  `drain`, `BufferAttr` : `tlength` et `prebuf`), doc Rust (`f32::to_le_bytes`,
  `std::io::Write` et l'erreur de tuyau fermé).
- **Critères**
  - [x] Jouer un son court sur une sortie : `deskcheck test output [QUOI]` (un carillon de 0,6 s).
  - [x] Enregistrer 3 secondes sur un micro, puis les réécouter : `deskcheck test input [QUOI]`
    (`--seconds N` jusqu'à 10). L'enregistrement reste en mémoire, jamais sur le disque.
  - [x] Dit le niveau le plus fort entendu ; presque rien (sous -50 dB) : le dit et propose `levels`.
  - [x] Prévient avant si l'appareil est coupé ou à 0 %.
  - [x] Testé ici sur un serveur PulseAudio de test : le carillon arrive bien sur la sortie,
    l'enregistrement est bien rejoué.
  - [x] Testé sur le PC de Godwin (17 oct. 2026) : carillon entendu, voix réentendue.
- **Décisions**
  - Latence de lecture de 100 ms : par défaut le serveur attend 2 s de son avant de jouer.
  - Sur le serveur de test, la commande dure 2 s : la sortie de test (`module-null-sink`) a
    2 s de latence. Une vraie carte son en a quelques dizaines de millisecondes.
  - Correction au passage : `deskcheck audio | head` ne plante plus (tuyau fermé ignoré).

### DC-16 · Événements en direct
- **But** : savoir tout de suite qu'un casque, une application ou un écran change.
- **Docs lues** : `libpulse-binding` 2.30 (`context::subscribe` : `InterestMaskSet`, `Facility`,
  `Operation`, `set_subscribe_callback`), `zbus` 5.13 (signaux d'un proxy : `#[zbus(signal)]`,
  `receive_*`, `SignalIterator` bloquant, `Connection::close`, `emit_signal`),
  XML de Mutter (signal `MonitorsChanged`), doc Rust (`std::sync::mpsc`).
- **Critères**
  - [x] Abonnement aux changements PulseAudio (appareil branché, flux nouveau, défauts) :
    `deskcheck_pulse::SoundWatcher`.
  - [x] Abonnement au signal `MonitorsChanged` de Mutter : `deskcheck_mutter::ScreenWatcher`,
    testé avec le faux Mutter (deux signaux, deux changements, arrêt en moins d'une seconde).
  - [x] Un seul canal d'événements pour l'interface : le type `Change` du cœur, envoyé par les deux.
  - [x] Commande `deskcheck watch` : une ligne par changement, avec le nouvel état.
  - [x] Testé ici de bout en bout : serveur PulseAudio de test et faux GNOME sur un bus de test.
  - [x] Testé sur le PC de Godwin (17 oct. 2026) : volume, vidéo et écran ASUS vus en direct.
- **Décisions**
  - Un changement dit *quoi* a changé, pas le nouvel état : l'interface relit l'état (une seule vérité).
  - Les rafales (un curseur de volume envoie beaucoup d'événements) sont regroupées sur 150 ms.
  - Un flux qui démarre change aussi l'état de son appareil : `watch` saute une ligne identique
    à la précédente du même genre.
  - Sans GNOME, `watch` surveille le son seul et le dit.

### DC-17 · Fenêtre GTK compatible Ubuntu 22.04 🧪
- **But** : vérifier tôt que GTK 4.6 et libadwaita 1.1 compilent et tournent.
- **Docs lues** : doc de la caisse `libadwaita` 0.8 (exemple `Application`, `ApplicationWindow`,
  `HeaderBar`), `Cargo.toml` de `gtk4-sys` 0.10 et `libadwaita-sys` 0.8 (versions système
  minimales), index crates.io (`rust-version` de chaque version).
- **Critères**
  - [x] Fenêtre vide avec barre d'en-tête (titre et sous-titre) et page d'accueil :
    nouvelle caisse `deskcheck-app`.
  - [x] Construite dans la CI Ubuntu 22.04 : GTK 4.6.9 et libadwaita 1.1.7 (lus dans la CI),
    et avec Rust 1.85.
  - [x] Vue ici sur un écran virtuel (Xvfb) : barre d'en-tête, icône Adwaita et texte bien placés.
  - [x] Testé sur le PC de Godwin (17 oct. 2026) : la fenêtre s'ouvre sur Ubuntu 22.04.
- **Décisions**
  - `gtk4` 0.10 et `libadwaita` 0.8 : `gtk4` 0.11 demande Rust 1.92, au-dessus du 1.85 du projet.
  - Options `v4_6` et `v1_1` : un appel plus récent que Ubuntu 22.04 ne compile pas.
  - libadwaita 1.1 n'a pas `ToolbarView` (1.4) : la barre d'en-tête est dans une boîte verticale.
  - Il faut les paquets `libgtk-4-dev` et `libadwaita-1-dev` pour construire.

### DC-18 · Structure de l'application
- **But** : la charpente de la fenêtre, comme la maquette, et le travail système à part.
- **Docs lues** : caisse `libadwaita` 0.8 et `gtk4` 0.10 (`ListBox` et style `navigation-sidebar`,
  `Stack`, `MenuButton`, `gio::SimpleAction`, `CssProvider::load_from_data`, `AboutDialog`),
  `glib` 0.21 (`spawn_future_local`), caisse `async-channel` 2.5 (`unbounded`, `send_blocking`, `recv`),
  maquette DeskCheck (artboard « Maintenant »).
- **Critères**
  - [x] `AdwApplication`, barre latérale à 5 sections (Now, Audio, Screens, Places, Diagnostic),
    badge du lieu (« No place yet » jusqu'à DC-27), menu (Refresh, About, Quit).
  - [x] Le travail système tourne hors du fil de l'interface, résultats envoyés à l'interface :
    un fil « backend » lit et surveille le son et les écrans, et envoie chaque nouvel état sur un
    canal `async-channel`, attendu par l'interface avec `glib::spawn_future_local`.
  - [x] En bas de la barre latérale : « Sound: PulseAudio » (ou PipeWire), « Screens: GNOME, Wayland ».
  - [x] La page Now montre déjà l'état en direct (sortie, micro, nombre d'écrans), en attendant ses cartes.
  - [x] Vu ici sur un écran virtuel avec un serveur son de test et un faux GNOME : clair et sombre,
    clic sur une section, mise à jour toute seule quand un casque arrive.
  - [x] Testé sur le PC de Godwin (17 oct. 2026) : sections, menu et mise à jour en direct.
- **Décisions**
  - Interface en anglais, traduction française plus tard (nouvelle tâche DC-38, choix de Godwin).
  - libadwaita 1.1 n'a pas `NavigationSplitView` (1.4) : `ListBox` + `Stack` côte à côte.
  - libadwaita 1.1 n'a pas `AboutWindow` (1.2) : `gtk::AboutDialog`.
  - Une seule fenêtre : relancer DeskCheck montre celle qui est déjà ouverte.

### DC-19 · Page « Maintenant »
- **But** : en un coup d'œil, par où sort le son, quel micro t'entend, quelles applications
  utilisent le son, et tes écrans.
- **Docs lues** : `gtk4` 0.10 (`Scale`, `LevelBar` en mode discret, `Fixed`, `Popover`,
  `MenuButton`, `Grid`, `CssProvider`), `libadwaita` 0.8 (`Clamp`, `ToastOverlay`, `Toast`),
  `glib` (`timeout_add_local_once`), feuille de style Adwaita (taille des blocs de `levelbar`),
  `libpulse-binding` (`client` et `owner_module` des flux), maquette (artboard « Maintenant »).
- **Critères**
  - [x] Carte sortie : nom, nom technique, curseur de volume (jusqu'à 150 %), mesure en direct
    de ce qui sort, « Test the sound », « Change » (liste des sorties).
  - [x] Carte micro : nom, « Not muted » ou « Muted », barre de niveau en direct
    (« Speak: the bar moves »), « Record 3 s and play back », « Change ».
  - [x] Carte applications : une ligne par application, « Not a microphone » si elle écoute un retour.
  - [x] Carte écrans : plan avec numéro et couleur de chaque écran, « Extended · Primary: 1 ».
  - [x] Vu ici sur un écran virtuel avec un serveur son de test et un faux GNOME : mise en page,
    barres qui bougent, liste « Change » qui bascule vraiment la sortie, message du test.
  - [x] Reprise du design (17 oct. 2026), à la demande de Godwin : la page suit maintenant la
    maquette. Couleurs de la maquette, en-tête et barre latérale gris, section choisie en bleu,
    boutons blancs bordés avec icône, barres de niveau en petits blocs verts, étiquettes, écrans
    posés côte à côte comme sur un bureau. Mode sombre vérifié aussi.
  - [x] Testé sur le PC de Godwin (17 oct. 2026) : page fidèle à la maquette, volume, barres, tests du son.
- **Décisions**
  - Bandeau d'alerte (DC-23), « Identify » (DC-24, bouton grisé), « Souris ici » (DC-26) et
    ligne du lieu (DC-28) viendront avec leurs tâches.
  - « Change » ouvre l'onglet Sorties ou Micros de la page Audio, comme dans la maquette
    (avant la reprise, c'était une liste dans la carte).
  - Les icônes sont celles de la maquette (dessinées en traits). `icons/build.sh` les
    transforme en icônes symboliques pleines, que GTK sait colorer ; `build.rs` les met dans
    l'application avec les styles (`resources/style.css`, `style-dark.css`).
  - Le curseur de volume attend 120 ms après le dernier mouvement avant d'envoyer, et ne se
    laisse pas déplacer par l'état pendant que tu le bouges.
  - Couleurs des écrans : celle choisie dans les réglages, sinon le bleu et l'orange de la maquette.
  - Correction trouvée en route : les flux créés par un module du serveur son (loopback,
    echo cancel) n'ont pas de client et ne sont plus pris pour des applications.
  - Pièges notés : `css_classes` remplace aussi les classes que GTK pose lui-même (les blocs
    d'une `LevelBar` disparaissaient) ; Adwaita donne 26 px à chaque bloc de `LevelBar`.

### DC-20 · Page « Audio »
- **But** : voir et régler chaque sortie, chaque micro et chaque application, avec les
  retours rangés à part pour ne pas les prendre pour des micros.
- **Docs lues** : `gtk4` 0.10 (`ToggleButton` en groupe, `CheckButton` en radio, `DropDown`,
  `Popover`, `FlowBox`, `Entry`, `Stack`, `Overflow`), `libadwaita` 0.8 (feuilles de style
  chargées depuis les ressources), `glib-build-tools` 0.21 et `gio::resources_register_include`,
  GTK (icônes dans `icons/` sous le chemin des ressources), maquettes « Audio » et « Nommage ».
- **Critères**
  - [x] Onglets Outputs, Microphones, Apps dans la barre du haut, avec leur nombre.
  - [x] Sorties et micros : radio pour choisir celui par défaut, barre en direct, étiquette
    (« Hears your voice », « Faint, far away », « Silent », « Playing », « Quiet »).
  - [x] Retours rangés à part : « Output monitors (n): these are not microphones ». Celui
    qu'une application écoute est en orange, avec « Switch to <micro> ».
  - [x] Applications : l'appareil de chaque flux, à changer dans une liste, sans couper.
  - [x] Bouton « Test » et bouton Renommer (petit formulaire, noms proposés, « Use the system
    name »). Le nom est enregistré dans le fichier de réglages.
  - [x] Vu ici sur un écran virtuel avec un serveur son de test : radio qui change la sortie
    par défaut, « Switch » qui déplace vraiment Firefox sur le micro, nom enregistré et affiché.
  - [x] Testé sur le PC de Godwin (17 oct. 2026) : onglets, barres, tests, choix par défaut, renommage.
- **Décisions**
  - Les barres de tous les micros et retours ne tournent que pendant que la page Audio est
    affichée (sinon, seulement le micro et la sortie par défaut).
  - L'étiquette garde le niveau le plus fort un court moment, pour ne pas changer entre deux mots.
  - « Name a new device », « Advanced options » et « Hidden devices » sont présents comme dans
    la maquette, grisés : ils viendront plus tard (assistant DC-32).

### DC-21 · Page « Écrans »
- **But** : voir chaque écran à sa vraie place, avec son numéro et sa couleur, et régler
  celui qu'on choisit.
- **Docs lues** : `gtk4` 0.10 (`Fixed`, `GestureClick`, `EventControllerFocus`, `Entry`,
  `DropDown` et `StringList`, `CheckButton`, `Overflow`), CSS de GTK 4 (`radial-gradient`,
  `background-size`, `box-shadow` à plusieurs anneaux), `hwdata` (format de `pnp.ids`),
  maquette « Écrans ».
- **Critères**
  - [x] Plan des écrans à leur vraie place, sur un bureau à points : numéro, nom, ligne
    technique, étiquettes « Primary », « Top bar here », « Built-in ».
  - [x] Un clic choisit l'écran (anneau noir) et montre ses réglages.
  - [x] Nom et couleur repère (les quatre de la maquette) enregistrés aussitôt.
  - [x] « Sound of this screen » : les haut-parleurs de l'écran, avec « Use ».
  - [x] « How DeskCheck recognizes it » : fabricant, modèle, n° de série, branchement.
  - [x] Vu ici sur un écran virtuel : couleur et nom enregistrés et affichés, « Use » qui
    change vraiment la sortie, mode sombre.
  - [x] Testé sur le PC de Godwin (17 oct. 2026) : plan, clic, couleur et nom, anneau fin.
- **Décisions**
  - Onglets Extend, Mirror, One screen, case « Primary screen », résolution et échelle : ils
    montrent l'état actuel, grisés. Les changer vient avec DC-25.
  - « Identify screens » vient avec DC-24, « Votre souris est ici » avec DC-26.
    « Bring back lost windows » est grisé (pas encore de tâche).
  - Haut-parleurs d'un écran : la sortie HDMI dont le nom contient le modèle de l'écran, ou la
    seule sortie HDMI quand un seul écran externe est allumé. Sinon, DeskCheck le dit.
  - Le nom du fabricant vient de `/usr/share/hwdata/pnp.ids` s'il est là, sinon d'une petite liste.
  - Les quatre couleurs de la maquette sont maintenant les couleurs par défaut des écrans.
  - Retour de Godwin : l'anneau de l'écran choisi était trop épais. GTK dessine les ombres
    (`box-shadow`) dans l'ordre inverse du CSS : l'anneau noir de 6 px cachait l'écart de 4 px.
    Corrigé : 4 px d'écart puis 2 px de trait, comme la maquette.

### DC-22 · Page « Diagnostic »
- **But** : pour le problème que tu as, une liste de vérifications avec leur état, et une
  correction en un clic quand DeskCheck peut la faire.
- **Docs lues** : `gtk4` 0.10 (`ProgressBar` et ses nœuds CSS `trough` et `progress`,
  `CheckButton`, `Box` homogène), `glib` 0.21 (`timeout_add_seconds_local`, `ControlFlow`),
  moteur de diagnostic de DC-11, maquette « Diagnostic ».
- **Critères**
  - [x] « They can't hear me » : micro non coupé, micro qui entend ta voix (barre en direct,
    « You spoke 2 s ago »), application qui écoute un vrai micro, droit du site (tu réponds),
    test final.
  - [x] « I hear nothing » : sortie non coupée, applications sur la bonne sortie, son de test.
  - [x] « A screen is not detected » : GNOME voit les écrans, ils sont allumés, câble et entrée.
  - [x] Barre de progression « 3 of 5 · 1 problem found », boutons pour passer d'un cas à l'autre.
  - [x] Correction en un clic (bouton orange), sans couper l'appel.
  - [x] Vu ici sur un écran virtuel : « Switch Firefox to Jabra-Mic » déplace vraiment Firefox
    sur le micro, la vérification passe au vert.
  - [x] Testé sur le PC de Godwin (17 oct. 2026) : les trois cas et la correction en un clic.
- **Décisions**
  - Les vérifications viennent des règles de DC-11 : même résultat que `deskcheck doctor`.
  - Ce que DeskCheck ne peut pas voir (droit du site, son entendu, câble) est une question.
  - Le test final reste grisé tant qu'une vérification au-dessus n'est pas bonne, comme la maquette.
  - « Remember the fix for this place » est grisé : il vient avec les lieux (DC-28).

### DC-23 · Alerte en direct
- **But** : prévenir tout de suite, pendant un appel, quand une application écoute un retour.
- **Docs lues** : `gtk4` 0.10 (`Button::set_label`, `Widget::set_visible`), règles de DC-11
  (`RecordsMonitor`), maquette « Maintenant » (bandeau).
- **Critères**
  - [x] Bandeau orange en haut de « Now » dès qu'une application écoute un retour : le nom de
    l'application, du retour et du micro à utiliser.
  - [x] « hears your voice well » quand ce micro vient d'entendre une voix (moins de 10 s).
  - [x] « Switch Firefox to … » corrige sans couper l'appel ; le bandeau disparaît.
  - [x] « Run the diagnostic » ouvre la page Diagnostic ; « Ignore » cache le bandeau.
  - [x] Vu ici sur un écran virtuel : Firefox déplacé sur le micro, bandeau caché, lien vers
    le Diagnostic.
  - [x] Testé sur le PC de Godwin (17 oct. 2026) : bandeau, Switch et Ignore.
- **Décisions**
  - « Ignore » vaut pour les flux présents. Une nouvelle écoute d'un retour fait revenir le bandeau.
  - Le nom de l'onglet (« Teams ») n'est pas connu : Firefox ne le donne pas au serveur son.
    Le bandeau dit « In Firefox ».

### DC-24 · Identifier les écrans
- **But** : voir sur chaque vrai écran son numéro, sa couleur et son nom, les mêmes que dans
  DeskCheck.
- **Docs lues** : `gtk4` 0.10 (`Window::fullscreen_on_monitor`, `EventControllerKey`,
  `GestureClick` et son nombre de clics, `Overlay`, `Actionable`), `gdk4` 0.10
  (`Display::monitors`, `Monitor::connector`, `Monitor::geometry`), `gio::SimpleAction` avec
  un paramètre, maquette « Identifier ».
- **Critères**
  - [x] Sur chaque écran, pendant 5 secondes : le grand numéro dans sa couleur, le nom,
    « eDP-1 · 1920 × 1080 », « Primary », et « The numbers disappear in 5 s · Esc to close ».
  - [x] Échap, un clic ou la fin du compte à rebours ferme tout.
  - [x] « Identify » (Now), « Identify screens » (Screens), et double-clic sur un écran du
    plan pour l'identifier seul.
  - [x] Vu ici sur un écran virtuel avec un gestionnaire de fenêtres : plein écran, fenêtre
    claire autour du numéro, Échap, double-clic.
  - [x] Testé sur le PC de Godwin (17 oct. 2026) : 1 sur le portable, 2 sur l'ASUS, Échap, double-clic.
- **Décisions**
  - Une fenêtre sans bord par écran, en plein écran sur lui, au fond transparent : le bureau
    reste visible, comme la maquette.
  - Le numéro, le nom et la ligne sont sur un panneau sombre : la maquette les montre sur un
    bureau sombre, et un bureau clair les rendrait illisibles.
  - L'écran de GTK et celui de GNOME sont reliés par le nom du branchement (`DP-1`), sinon par
    la place sur le bureau, chaque écran une seule fois.
  - Le plan de la page Screens ne se reconstruit plus à chaque clic : GTK peut ainsi voir le
    double-clic.

### DC-25 · Appliquer une disposition d'écrans
- **But** : changer les écrans depuis DeskCheck : disposition, écran principal, résolution,
  échelle et place.
- **Docs lues** : interface D-Bus `org.gnome.Mutter.DisplayConfig` de Mutter 42
  (`GetCurrentState`, `ApplyMonitorsConfig` et ses méthodes verify, temporary, persistent),
  code de Mutter 42 (`meta_verify_monitors_config`, `meta_rectangle_is_adjacent_to`,
  confirmation après 20 s), `zbus` 5 (méthode de proxy, interface de test qui garde les
  appels), `gtk4` 0.10 (`GestureDrag`, `Fixed::move_`, `DropDown::connect_selected_notify`,
  `ToggleButton::set_group`).
- **Critères**
  - [x] Onglets Extend, Mirror, One screen dans la barre du haut.
  - [x] Écran principal, résolution (« recommended » pour la meilleure), échelle.
  - [x] Glisser un écran dans le plan : il se colle à l'écran le plus proche, sans en couvrir un.
  - [x] Chaque changement est vérifié par DeskCheck avec les règles de Mutter, puis par GNOME
    (méthode « verify »), puis appliqué (« persistent »).
  - [x] Retour arrière automatique : GNOME montre sa fenêtre « garder ces réglages ? » et
    revient à l'ancienne disposition après 20 s sans réponse.
  - [x] Vu ici avec un faux GNOME qui garde les changements : glisser, principal, miroir, retour
    en étendu.
  - [x] Testé sur le PC de Godwin (18 oct. 2026), sous X11 : principal, résolution, retour après 20 s, glisser, Mirror et Extend.
- **Décisions**
  - Le retour arrière est celui de GNOME (méthode « persistent ») : il marche même si DeskCheck
    se ferme, et c'est la fenêtre que tu connais déjà.
  - Sortie du miroir : chaque écran reprend sa meilleure résolution.
  - Sur X11, GNOME impose la même échelle partout : DeskCheck change donc toutes les échelles.

### DC-26 · Sur quel écran est la souris ? 🧪
- **But** : montrer sur quel écran est la souris, sous X11 (le cas de Godwin) comme sous Wayland.
- **Docs lues** : `x11rb` 0.14 (`connect`, `query_pointer`, `QueryPointerReply`), guide GJS
  (`Gio.DBusExportedObject.wrapJSObject`, `Gio.bus_own_name`), guide des extensions GNOME
  Shell (anatomie, `metadata.json`, forme ancienne pour 42 à 44, modules ES depuis 45),
  `zbus` 5 (proxy), maquettes « Maintenant », « Écrans », « Identifier ».
- **Recherche** : sous Wayland, une application ne voit la souris que sur ses propres fenêtres.
  `org.gnome.Shell.Eval` est fermé depuis GNOME 41. Solution choisie : une petite extension
  de GNOME Shell qui répond à un seul appel D-Bus, `GetPosition`, avec `global.get_pointer()`.
  Sous X11, aucune extension : DeskCheck demande au serveur X (`QueryPointer`).
- **Critères**
  - [x] « Mouse here » sur le plan de « Now », « Your mouse is here » sur la page Screens et sur
    l'écran identifié, comme les maquettes.
  - [x] X11 : position lue au serveur X ; Wayland : par l'extension, sinon la page Screens le dit.
  - [x] L'écran sous la souris : taille du mode sous X11, taille divisée par l'échelle sous Wayland.
  - [x] Extension en deux versions (GNOME 42 à 44, et 45 et plus) avec `install.sh`.
  - [x] `deskcheck mouse` : « The mouse is on screen 2, ASUS (at 2400, 300). »
  - [x] Vu ici sous X11 (écran virtuel) : tags au bon endroit, `deskcheck mouse` juste.
  - [x] Testé sur le PC de Godwin (18 oct. 2026), sous X11. Wayland : à voir plus tard, avec l'extension.
- **Décisions**
  - La souris est lue toutes les 300 ms ; l'interface ne change que quand elle passe à un
    autre écran.
  - Sans extension sous Wayland, DeskCheck réessaie toutes les 10 s (tu peux l'activer après).

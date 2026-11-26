# Flair

Launcher clavier façon Spotlight / Raycast / PowerToys Run, en Rust, pour **Windows 10/11** et **Linux** (X11 et Wayland). Un raccourci global (`Alt+Espace`) fait apparaître une barre de recherche ; on tape, on choisit au clavier, `Entrée`.

| Sombre, interface en anglais | Calcul | Clair, interface en français |
|---|---|---|
| ![](docs/screenshots/dark-en-apps.png) | ![](docs/screenshots/dark-en-calc.png) | ![](docs/screenshots/light-fr-apps.png) |

> **État : jalon 1 (squelette).** Fenêtre, thèmes, configuration, traductions et navigation au clavier sont en place, avec des **résultats factices**. Les vraies sources (applications, calcul, fichiers, web, système) arrivent aux jalons 4 à 7. Le cahier des charges complet est dans [`PROMPT.md`](PROMPT.md).

## Compiler et lancer

Prérequis : Rust stable (édition 2024), installé avec [rustup](https://rustup.rs).

### Linux (et WSL2)

```bash
cargo build                 # debug
cargo build --release       # optimisé (LTO fat, binaire strippé)
cargo run -p flair-app -- --show
```

`--show` affiche la fenêtre immédiatement. Au jalon 1, Flair n'est pas encore résident : `Échap` sur un champ vide, la perte du focus ou le lancement d'un résultat **ferment** l'application. Le masquage et le réaffichage via le raccourci global arrivent au jalon 2.

**Sous WSL2 (WSLg)** :
- le raccourci global ne fonctionne pas (Windows intercepte le clavier) : utiliser `--show` ;
- WSLg n'expose pas de pilote Vulkan matériel, et `wgpu` y reste bloqué à la création de la surface. Avec `renderer = "auto"`, Flair détecte WSL et choisit le rendu logiciel (`tiny-skia`) : la fenêtre s'affiche en ~75 ms (build debug) ;
- si la fenêtre ne s'ouvre pas (« NoCompositor »), lancer avec `XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir`.

### Windows, compilé depuis WSL

```bash
cargo install cargo-xwin
rustup target add x86_64-pc-windows-msvc
cargo xwin build --release --target x86_64-pc-windows-msvc -p flair-app
# → target/x86_64-pc-windows-msvc/release/flair.exe
```

Puis lancer le `.exe` depuis WSL ou après l'avoir copié sous `C:\`. La CRT est liée statiquement (`.cargo/config.toml`) : aucun Visual C++ Redistributable n'est nécessaire. En release, aucune console ne s'ouvre (`windows_subsystem = "windows"`).

### Windows, compilé nativement

```powershell
cargo build --release -p flair-app
```

## Qualité

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

La CI (GitHub Actions, `.github/workflows/ci.yml`) lance `fmt`, `clippy` et `test` sur `ubuntu-latest` et `windows-latest`.

## Organisation

```
crates/
├── flair-core/       types, trait Provider, registre des réglages, config, thèmes, i18n (sans UI)
├── flair-providers/  sources de résultats (jalons 4 à 7)
├── flair-platform/   code propre à l'OS : chemins, choix du moteur de rendu (seul crate avec `unsafe`)
└── flair-app/        binaire `flair` : interface Iced, logs, lancement
assets/
├── fonts/            Inter et JetBrains Mono (OFL)
├── icons/            logo Flair (flair.svg, flair-tiny.svg)
└── ui-icons/         icônes Lucide (ISC)
```

- **Registre des réglages** (`flair-core/src/settings/registry.rs`) : chaque réglage est décrit une seule fois (type, bornes, noms, descriptions et alias FR/EN). Le fichier `config.toml` par défaut, la validation et la table ci-dessous en sont générés. Un test vérifie que chaque champ de la config a son entrée, et inversement.
- **Thèmes** (`flair-core/src/theme/`) : 3 couleurs → jetons, avec mélanges en Oklab, ajustements en OKLCH et contrastes WCAG 2 garantis. Un test de propriétés vérifie les minimums de contraste sur des milliers de thèmes aléatoires.
- **Traductions** : `flair-core/i18n/{fr,en}.ftl` (Fluent). Un test vérifie que les deux fichiers ont les mêmes messages.
- **Dimensions** : toutes dans `flair-app/src/design.rs` (grille de 4 px).

## Configuration

Fichier créé au premier lancement, avec des commentaires, dans la langue du système :

| | Windows | Linux |
|---|---|---|
| config | `%AppData%\Flair\config.toml` | `~/.config/flair/config.toml` |
| données, logs | `%LocalAppData%\Flair\` | `~/.local/share/flair/` |
| cache | `%LocalAppData%\Flair\cache\` | `~/.cache/flair/` |

Une erreur dans le fichier n'empêche jamais le démarrage. La valeur fautive est remplacée par sa valeur par défaut, et l'erreur s'affiche dans la fenêtre quand le champ est vide.

Logs : un fichier par jour dans `logs/` du dossier de données (7 conservés). Le niveau se règle avec `log_level` ou la variable `FLAIR_LOG` (prioritaire, syntaxe `tracing`, par exemple `FLAIR_LOG=debug`).

Réglages (table générée depuis le registre, `UPDATE_README=1 cargo test -p flair-core --test readme` pour la mettre à jour) :

<!-- settings:start -->
| Clé | Défaut | Description |
|---|---|---|
| `hotkey` | `"Alt+Space"` | Raccourci qui affiche ou cache Flair |
| `ui_language` | `"system"` | Langue de l'interface et des résultats (system \| fr \| en) |
| `input_languages` | `["fr", "en"]` | Langues de saisie actives, dans l'ordre de préférence (> langue / > language) |
| `decimal_separator` | `"auto"` | Virgule ou point pour les nombres décimaux (auto \| , \| .) |
| `date_format` | `"auto"` | Ordre jour, mois, année des dates numériques (auto \| dmy \| mdy \| ymd) |
| `max_results` | `8` | Nombre maximal de résultats affichés (1 à 20) |
| `keep_last_query` | `false` | Garder la dernière recherche quand Flair réapparaît |
| `autostart` | `false` | Lancer Flair à l'ouverture de la session |
| `log_level` | `"info"` | Détail des journaux (FLAIR_LOG est prioritaire) (error \| warn \| info \| debug \| trace) |
| `calc.angle_unit` | `"deg"` | Unité par défaut des fonctions trigonométriques (deg \| rad) |
| `calc.significant_digits` | `10` | Chiffres significatifs affichés (1 à 100) |
| `calc.binary_prefixes` | `false` | Ko/Mo/Go en 1024 (comme l'Explorateur Windows) au lieu de 1000 |
| `calc.currency` | `false` | Conversions de devises (taux téléchargés et mis en cache) |
| `calc.history_size` | `50` | Nombre de calculs gardés dans l'historique (0 à 1000) |
| `theme.mode` | `"system"` | Suivre le système, ou forcer le thème clair ou sombre (system \| light \| dark) |
| `theme.light` | `"Flair clair"` | Thème utilisé en mode clair |
| `theme.dark` | `"Flair sombre"` | Thème utilisé en mode sombre |
| `theme.opacity` | `1.0` | Transparence du fond, si la plateforme le permet (0.85 à 1) |
| `theme.icons` | `"color"` | Icônes d'applications en couleur ou monochromes (color \| mono) |
| `theme.font` | `"inter"` | inter, system, ou le nom d'une police installée |
| `theme.tray_icon` | `"auto"` | Couleur de l'icône : suit la barre des tâches, ou couleur d'accent (auto \| accent) |
| `theme.animations` | `true` | Fondu à l'apparition de la fenêtre |
| `files.enabled` | `true` | Indexer et rechercher les fichiers |
| `files.roots` | `["~"]` | Dossiers racines de l'index de fichiers |
| `files.exclude` | `["node_modules", ".git", "target", ".cache", "__pycache__"]` | Noms de dossiers ou motifs ignorés par l'index |
| `files.include_hidden` | `false` | Indexer aussi les fichiers et dossiers cachés |
| `files.other_drives` | `false` | Windows : indexer aussi les autres disques (D:\, E:\…) |
| `resources.renderer` | `"auto"` | Rendu GPU, ou logiciel (cpu) sans mémoire vidéo (auto \| gpu \| cpu) |
| `resources.pause_on_battery` | `true` | Suspendre la surveillance et l'indexation sur batterie |
| `resources.trim_memory_after` | `5` | Secondes après le masquage avant de libérer la mémoire (0 = jamais) (0 à 3600) |
| `resources.efficiency_mode` | `true` | Windows : mode efficacité (EcoQoS) quand Flair est caché |
| `providers.apps` | `true` | Rechercher les applications installées |
| `providers.calculator` | `true` | Calculs et conversions |
| `providers.web` | `true` | Mots-clés de recherche web |
| `providers.system` | `true` | Verrouiller, éteindre, redémarrer… |
| `update.channel` | `"main"` | main : chaque push sur main ; stable : versions taguées (main \| stable) |
| `update.auto_check` | `true` | Vérifier au plus une fois par jour, à l'ouverture de Flair |
| `web` | `g, ddg, wiki, yt` | Mots-clés et adresses des moteurs de recherche ({query} = la recherche) |
<!-- settings:end -->

## Limitations connues (jalon 1)

- **Pas encore résident** : cacher = quitter, jusqu'au jalon 2 (icône de notification, IPC, raccourci global).
- **Position** : centrée à 25 % de la hauteur de l'écran *principal*. L'écran sous la souris viendra avec le code plateforme du jalon 2. Sous Wayland, le positionnement libre est impossible sans layer-shell (jalon 8).
- **Wayland, hauteur de la fenêtre** : winit applique un redimensionnement demandé par l'application sans envoyer d'événement `Resized`, et Iced ne refait alors pas sa mise en page. Sous Wayland, la fenêtre transparente garde donc sa hauteur maximale et seul le cadre visible change de taille. Sous Windows et X11, la fenêtre est redimensionnée normalement.
- **Ombre** : sous Windows, l'ombre et les coins arrondis sont ceux de DWM (`undecorated_shadow`). Sous Linux, l'ombre est dessinée dans la partie transparente de la fenêtre, donc seulement sous Wayland pour l'instant.
- **Curseur du champ** : Iced 0.14 dessine le curseur avec la couleur du texte. La spec le veut en couleur d'accent, ce qui demande un petit widget dédié (à faire).
- **Recherche factice** : la correspondance floue du jalon 1 est volontairement simple. `nucleo-matcher` et la frecency arrivent au jalon 4.

## Développement

Options cachées utiles pour vérifier l'interface sans clavier :

```bash
flair --show --dev-query "vsc" --dev-screenshot capture.png
```

Elles tapent la requête, enregistrent une capture PNG de la fenêtre, puis quittent (les captures du haut de cette page sont faites ainsi).

## Licences des ressources

- Inter, JetBrains Mono : SIL Open Font License 1.1 (`assets/fonts/*-LICENSE.txt`).
- Icônes Lucide : ISC (`assets/ui-icons/LICENSE.txt`).

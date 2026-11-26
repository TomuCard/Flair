# Flair — Launcher universel façon Spotlight, en Rust

## Rôle

Tu es un développeur Rust senior, expérimenté en applications desktop multiplateformes (Windows et Linux). Tu dois concevoir et implémenter **Flair**, un launcher clavier inspiré de Spotlight (macOS), Raycast et PowerToys Run.

Travaille par jalons (voir plus bas). À chaque jalon, le projet doit compiler, passer `cargo clippy -- -D warnings` et `cargo test`, et être utilisable.

---

## 1. Le produit

L'utilisateur appuie sur un **raccourci global** (par défaut `Alt+Espace`). Une barre de recherche apparaît instantanément au centre de l'écran, au-dessus de toutes les fenêtres. Il tape quelque chose, les résultats s'affichent en temps réel, il choisit avec le clavier, appuie sur `Entrée`, et la barre disparaît.

Flair doit pouvoir chercher :
- **des applications** installées, et les lancer ;
- **des fichiers et dossiers**, pour les ouvrir ou afficher leur emplacement ;
- **des calculs de toutes sortes**, aussi complets que possible : arithmétique en précision arbitraire et notation scientifique (`1e524 * 1.5`), fonctions mathématiques, octets et débits (`4 Go à 100 Mbit/s`), durées et heures (`5h36 + 65min`), dates, bases numériques, unités physiques, devises… (détail en 5.3) ;
- **des recherches web** (`g rust iced` ouvre une recherche Google, `ddg ...` une recherche DuckDuckGo, etc.) ;
- **des commandes système** : verrouiller, mettre en veille, éteindre, redémarrer, vider la corbeille (avec confirmation pour les actions destructrices).

L'architecture doit permettre d'ajouter facilement d'autres sources plus tard : émojis, historique du presse-papier, fenêtres ouvertes, snippets, plugins.

Flair est **bilingue français / anglais** : toute saisie est comprise dans les deux langues en même temps, sans réglage à changer (voir la section 2, « Langues »).

### Plateformes cibles
- **Windows 10 et 11** (x86_64) : cible prioritaire.
- **Linux** : X11, et Wayland (GNOME, KDE, bureaux basés sur wlroots).
- macOS n'est **pas** une cible.

### Hors périmètre (pour l'instant)
- Système de plugins dynamiques (`.dll`/`.so`, WASM).
- Synchronisation cloud, comptes utilisateur.
- Recherche dans le contenu des fichiers (full-text) : seulement les noms et chemins.
- Installeur et signature de code.

---

## 2. Expérience utilisateur

### Fenêtre
- Sans bordure système ni barre de titre. Tout le design (dimensions, couleurs, thèmes) est détaillé en section 7.
- Centrée horizontalement, positionnée à environ 25 % de la hauteur de l'écran **où se trouve la souris**.
- Toujours au premier plan, absente de la barre des tâches et du Alt+Tab.
- Largeur 680 px. Hauteur : le champ de saisie seul quand il est vide, puis il s'agrandit jusqu'à 8 résultats visibles au maximum, avec défilement au-delà.
- Elle **se cache** quand elle perd le focus, quand on appuie sur `Échap`, ou après avoir lancé un résultat.
- Quand elle réapparaît, le champ est vide et sélectionné (option de config : garder la dernière requête).
- Thème clair et thème sombre par défaut, qui suivent le thème du système en direct. Thèmes personnalisables à partir de 3 couleurs (section 7).

### Clavier
| Touche | Action |
|---|---|
| `Alt+Espace` (configurable) | Afficher ou cacher Flair |
| `↑` / `↓`, `Ctrl+P` / `Ctrl+N` | Résultat précédent / suivant |
| `Entrée` | Action principale (lancer, ouvrir, copier le résultat du calcul) |
| `Ctrl+Entrée` | Action secondaire (ouvrir le dossier contenant le fichier, lancer en administrateur…) |
| `Tab` | Compléter le champ avec le résultat sélectionné (utile pour naviguer dans les chemins) |
| `Ctrl+C` | Copier le chemin ou la valeur du résultat sélectionné |
| `Ctrl+1` … `Ctrl+9` | Lancer directement le résultat n |
| `Échap` | Vider le champ s'il contient du texte, sinon cacher la fenêtre |

### Affichage d'un résultat
Chaque ligne affiche :
- une icône (l'icône système réelle pour les applications et fichiers, ou une version monochrome selon la config) ;
- un titre, avec les caractères qui correspondent à la recherche en couleur d'accent et en semi-gras ;
- un sous-titre (chemin, description, type) en texte secondaire ;
- sur la ligne sélectionnée uniquement, l'action de `Entrée` à droite (`↵ Ouvrir`, `↵ Copier`) ;
- les numéros `1` à `9` des raccourcis `Ctrl+n` n'apparaissent **que lorsqu'on maintient `Ctrl`**, pour ne pas charger l'affichage.

Le résultat d'un calcul s'affiche en premier, en grand.

### Préfixes optionnels pour filtrer
- `=` : calcul uniquement
- `/` ou `~` : fichiers et chemins uniquement
- `>` : commandes système
- `?` : recherche web
- sans préfixe : toutes les sources, mélangées et classées

### Langues : français et anglais en même temps

Flair est **bilingue en permanence**. L'utilisateur peut taper en français ou en anglais, et même **mélanger les deux** dans une même saisie, sans changer de réglage. Il n'y a pas de « mode langue » pour la saisie : les deux langues sont toujours reconnues.

#### Équivalences attendues (exemples)

| Français | Anglais |
|---|---|
| `maintenant + 3h` | `now + 3h` |
| `4 Go à 100 Mbit/s` | `4 GB at 100 Mbit/s` |
| `4j`, `4 jours` | `4d`, `4 days` |
| `2 sem`, `2 semaines` | `2w`, `2 wk`, `2 weeks` |
| `3 ans`, `3 années` | `3y`, `3 yr`, `3 years` |
| `6 mois` | `6 months` |
| `5 km en miles`, `5 km vers miles` | `5 km to miles`, `5 km in miles`, `5 km as miles` |
| `15% de 240` | `15% of 240` |
| `3 fois 4`, `10 divisé par 4` | `3 times 4`, `10 divided by 4` |
| `dans 90 min` | `in 90 min` |
| `il y a 2h` | `2h ago` |
| `jours jusqu'au 25 décembre` | `days until December 25` |
| `semaines depuis le 1er janvier` | `weeks since January 1` |
| `quel jour 14 juillet 1789` | `what day July 14 1789` |
| `heure à Tokyo` | `time in Tokyo` |
| `pgcd(12; 18)`, `ppcm(4; 6)` | `gcd(12, 18)`, `lcm(4, 6)` |
| `1 Kio`, `1 Go`, `1 octet` | `1 KiB`, `1 GB`, `1 byte` |
| `> éteindre`, `> verrouiller`, `> veille` | `> shutdown`, `> lock`, `> sleep` |

Les saisies mélangées doivent aussi marcher : `now + 3 jours`, `4 Go at 100 Mbit/s`, `maintenant + 2 days`.

#### Lexique bilingue
- Les mots-clés et unités de chaque langue sont dans des **fichiers de données intégrés au binaire** (par exemple `lexicon/fr.toml` et `lexicon/en.toml`), pas codés en dur. Chaque mot y est associé à un **jeton canonique** (`to`, `of`, `now`, `ago`, `day`, `byte`…).
- Tous les lexiques sont chargés **en même temps**. Ajouter une langue (espagnol, allemand…) se fait en ajoutant un fichier, sans toucher au code.
- La reconnaissance se fait **sur des jetons entiers** après découpage de la saisie, jamais par simple remplacement de texte (sinon `de` serait remplacé à l'intérieur de `dernier`, et `in` à l'intérieur de `minutes`).
- Pas de différence entre majuscules et minuscules, **sauf pour les unités où ça compte** (`Mo` mégaoctet, `mo` jamais ; `T` tesla, `t` tonne, `To` téraoctet ; `mB`, `MB` et `Mb` sont différents).
- Les accents sont optionnels, pour les claviers qui n'en ont pas : `a` = `à`, `annee` = `année`, `deg` = `°`.

#### Ambiguïtés : règles imposées
| Cas ambigu | Règle |
|---|---|
| `m` : minute ou mètre | **Minute** dans une durée qui contient aussi `h` ou `s` (`1h30m`, `5m30s`) ; **mètre** sinon (`5 m en ft`). `5m` tout seul = mètre, et on propose `5 min` en variante. |
| `in` : conversion, « dans », ou pouce (inch) | En début de saisie suivi d'une durée = temps relatif (`in 90 min`) ; juste après un nombre et suivi d'une unité ou d'une fin de saisie = pouce (`12 in en cm`) ; entre deux quantités = conversion (`5 km in miles`). |
| `an` : année ou article anglais | Après un nombre = année (`3 ans`, `1 an`). Sinon ignoré (`an hour` = `1 hour`). |
| `,` : virgule décimale ou séparateur de milliers | Réglage `decimal_separator` (`auto` par défaut, d'après la langue du système). `1,500.25` est sans ambiguïté (format anglais). En mode `auto`, si c'est ambigu (`1,500`), on suit la langue du système **et on propose l'autre lecture en variante**. |
| Date `01/02/2026` : 1er février ou 2 janvier | Réglage `date_format` (`auto` d'après la langue du système, ou `dmy` / `mdy` / `ymd`). Le format ISO `2026-02-01` et les dates en lettres (`1 feb`, `1er février`) sont toujours sans ambiguïté. Le résultat affiche **toujours le mois en lettres et le jour de la semaine**, pour que la lecture choisie se voie. |
| `à` sans accent (`a`) | Accepté comme « à / at » seulement entre une quantité et un débit (`4 Go a 100 Mbit/s`). |

#### Langue de l'interface et des résultats
- L'interface suit la **langue du système** : texte du champ vide, libellés, erreurs, menu de l'icône de notification, confirmations. On peut la forcer avec `language = "system" | "fr" | "en"`. Pour toute autre langue, l'interface est en anglais.
- Aucun texte visible n'est codé en dur : utiliser une crate d'internationalisation (`fluent` / `fluent-bundle`, ou `rust-i18n`) avec un fichier de traductions par langue.
- Les résultats sont **mis en forme dans la langue de l'interface**, quelle que soit la langue de la saisie :
  - FR : `6h41`, `1,5`, `10 janvier 2027 (dimanche)`, `2h (lendemain)` ;
  - EN : `6h 41m`, `1.5`, `January 10, 2027 (Sunday)`, `2:00 (next day)`.
- La copie avec `Alt+Entrée` (valeur brute) utilise **toujours `.`** comme séparateur décimal, pour pouvoir la coller dans du code ou un tableur.

---

## 3. Performances et sobriété en ressources (non négociables)

### Réactivité
- **Affichage de la fenêtre en moins de 50 ms** après le raccourci. L'app tourne en permanence en arrière-plan : on cache et réaffiche la fenêtre, on ne relance jamais le processus.
- **Résultats en moins de 16 ms** après chaque frappe pour les applications et le calcul. Moins de 50 ms pour les fichiers, même avec un index de plus d'un million d'entrées.
- **L'interface ne se bloque jamais.** Toutes les recherches coûteuses tournent hors du thread de l'UI, et une requête obsolète est annulée dès que l'utilisateur tape un nouveau caractère.

### Sobriété quand Flair est caché
Flair passe 99 % de son temps caché. Il doit alors être **invisible pour le PC** : aucun ralentissement, aucun impact sur la batterie, le moins de mémoire possible.

**Objectifs chiffrés (fenêtre cachée, usage typique) :**

| Mesure | Objectif |
|---|---|
| CPU au repos | **0 %**, et **aucun réveil** de l'app sans événement extérieur (raccourci, fichier modifié, IPC) |
| RAM sans index de fichiers | **< 40 Mo** |
| RAM avec un index d'un million de fichiers | **< 80 Mo** |
| Disque au repos | aucune lecture ni écriture |
| Indexation initiale | ne ralentit pas visiblement le PC (priorité arrière-plan) |

Ces objectifs **ne doivent jamais dégrader la réactivité** : l'affichage en moins de 50 ms reste prioritaire. Si une économie (libérer la mémoire, rendu logiciel…) fait dépasser les 50 ms, on l'allège ou on la rend optionnelle, et on le documente.

#### 1. Zéro réveil inutile
- **Aucun minuteur, aucun polling**, nulle part. Tout est piloté par des événements.
- Fenêtre cachée : aucun abonnement Iced actif (pas d'animation, pas de `time::every`, pas de redessin).
- Taux de change : mis à jour **seulement** quand l'utilisateur tape un calcul de devise et que le cache est trop vieux. Jamais en tâche de fond.
- Les threads de travail (indexation, recherche) sont **endormis** quand ils n'ont rien à faire. Pas d'attente active.

#### 2. Index de fichiers compact et chargé à la demande
- Stockage compact : tous les noms dans **un seul tampon d'octets**, et chaque entrée sous la forme `(index du parent, début et longueur du nom, métadonnées minimales)`. Jamais un `PathBuf` ou un `String` par fichier. Les chemins complets sont reconstruits seulement pour les résultats affichés.
- **Index ouvert directement depuis le fichier sur disque** (`memmap2` + `rkyv`, lecture sans copie). Le système charge en RAM seulement les pages utilisées pendant une recherche, et peut les libérer quand Flair dort.
- Les modifications (ajouts et suppressions entre deux sauvegardes) vont dans un petit index séparé en mémoire. Il est fusionné dans le fichier de temps en temps, pendant une période d'inactivité.

#### 3. Surveillance des fichiers économe
- Événements **regroupés** avec `notify-debouncer-full` (1 à 2 s) : une compilation qui touche 10 000 fichiers = un seul réveil, pas 10 000.
- Dossiers bruyants exclus par défaut : `node_modules`, `target`, `.git`, `.cache`, `__pycache__`, `AppData\Local`, `AppData\Roaming\*\Cache`, `~/.cache`, `~/.local/share/Trash`, et les caches de navigateur.
- Sous Linux, inotify demande une surveillance par dossier : surveiller le nombre utilisé, rester bien en dessous de `fs.inotify.max_user_watches`, et passer en rescan périodique des dossiers non surveillés si la limite approche (avec un message dans les logs).
- **Mode batterie** (`pause_on_battery = true` par défaut) : sur batterie, la surveillance et l'indexation sont mises en pause, puis rattrapées au retour sur secteur. La recherche continue de marcher avec l'index existant.

#### 4. Indexation en basse priorité
- Le thread d'indexation tourne en **priorité arrière-plan, pour le CPU et le disque** : `SetThreadPriority(THREAD_MODE_BACKGROUND_BEGIN)` sous Windows, `nice` + `ioprio_set` (classe *idle*) sous Linux.
- Parallélisme limité pendant l'indexation (par exemple la moitié des cœurs au maximum).
- Indexation complète **une seule fois** (premier lancement ou réindexation demandée), puis uniquement les changements.

#### 5. Rendre la mémoire quand Flair est caché
Quelques secondes après le masquage (délai réglable) :
- vider les caches temporaires : résultats de la dernière recherche, icônes non vues récemment (le cache LRU est réduit à une petite taille) ;
- **Windows** : activer le **mode efficacité** (EcoQoS, `SetProcessInformation` avec `ProcessPowerThrottling`), puis réduire la mémoire de travail (`EmptyWorkingSet` / `SetProcessWorkingSetSize`). On désactive EcoQoS dès que la fenêtre s'affiche ;
- **Linux** : `malloc_trim(0)` (glibc) pour rendre au système la mémoire libérée ;
- **vérifier** que l'affichage suivant reste sous les 50 ms. Sinon, alléger ces étapes.

#### 6. Rendu léger
- Réglage `renderer = "auto" | "gpu" | "cpu"`. `cpu` utilise le rendu logiciel d'Iced (`tiny-skia`), sans contexte GPU ni mémoire vidéo occupée.
- **Mesurer les deux** sous Windows et Linux : RAM et mémoire vidéo au repos, temps d'affichage, fluidité de la frappe. `auto` choisit le meilleur compromis selon ces mesures, et les résultats sont notés dans le `README`.

#### 7. Tout charger à la demande
- Icônes chargées seulement quand elles s'affichent, jamais toutes au démarrage.
- Chaque source de résultats s'initialise à son premier usage quand c'est possible. Exception : les applications, qui doivent être prêtes immédiatement.
- Une seule police intégrée pour l'interface (plus la police mono des calculs si nécessaire), sous-ensemble de caractères si la taille compte.
- Profil `release` : `lto = "fat"`, `codegen-units = 1`, `strip = true`, `opt-level = 3`.

#### 8. Mesurer plutôt que deviner
- Commande `flair --stats` et entrée **Statistiques** dans le menu de l'icône de notification. On y voit : RAM utilisée, taille de l'index (fichier et RAM), nombre d'entrées, dossiers surveillés, nombre de réveils depuis le démarrage, temps du dernier affichage, moteur de rendu actif.
- Compteur interne de réveils (chaque événement traité est compté), pour vérifier l'objectif « aucun réveil ».
- Vérification manuelle documentée dans le `README` : `powertop` et `pidstat` sous Linux, Process Explorer (« Context Switch Delta », « Private Bytes ») sous Windows.

---

## 4. Architecture

Organise le code en **workspace Cargo** :

```
flair/
├── Cargo.toml                # workspace
├── crates/
│   ├── flair-core/           # types, trait Provider, classement, sans dépendance UI
│   ├── flair-providers/      # implémentations : apps, fichiers, calcul, web, système
│   ├── flair-platform/       # code propre à l'OS : raccourci global, liste des apps, icônes, IPC
│   └── flair-app/            # binaire : UI, boucle d'événements, icône de notification, config
└── assets/                   # icônes, polices
```

### Contrat central

```rust
pub struct Query {
    pub raw: String,
    pub text: String,          // texte sans le préfixe
    pub filter: Option<ProviderKind>,
}

pub struct ResultItem {
    pub id: String,            // stable, pour l'historique et le frecency
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: IconSource,      // chemin, données d'image, nom d'icône système ou icône intégrée
    pub score: f32,
    pub match_indices: Vec<u32>,
    pub actions: Vec<Action>,  // [0] = action principale
    pub provider: ProviderKind,
}

pub enum Action {
    Launch { target: LaunchTarget },
    Open { path: PathBuf },
    RevealInFolder { path: PathBuf },
    CopyToClipboard { text: String },
    OpenUrl { url: String },
    SystemCommand(SystemCommand),
}

pub trait Provider: Send + Sync {
    fn kind(&self) -> ProviderKind;
    /// Doit être rapide ; vérifier régulièrement `cancel` sur les longues boucles.
    fn query(&self, q: &Query, cancel: &CancellationToken) -> Vec<ResultItem>;
    /// Indexation ou rafraîchissement en arrière-plan ; ne bloque pas l'appelant.
    fn refresh(&self) {}
}
```

### Classement des résultats
- Score de base : correspondance floue avec **`nucleo-matcher`**.
- Bonus selon le type de résultat : applications > dossiers > fichiers à score égal.
- Bonus **frecency** : on enregistre chaque lancement (`id`, nombre, date du dernier usage) et on remonte ce que l'utilisateur utilise souvent et récemment. Sauvegarde sur disque.
- Bonus si le texte tapé est le début d'un mot du titre (`vsc` doit trouver « **V**isual **S**tudio **C**ode »).
- Les fonctions de score sont pures et testées unitairement.

### Instance unique et IPC
- Une seule instance tourne à la fois. Si on lance `flair` alors qu'une instance existe, ou `flair --toggle`, on envoie un message à l'instance existante via la crate **`interprocess`** (named pipe sous Windows, socket Unix sous Linux), puis on quitte.
- C'est indispensable sous Wayland, où le raccourci global peut devoir être déclaré dans les réglages du bureau pour lancer `flair --toggle`.

---

## 5. Détails par source

### 5.1 Applications
**Windows**
- Parcourir les `.lnk` de `%ProgramData%\Microsoft\Windows\Start Menu\Programs` et `%AppData%\Microsoft\Windows\Start Menu\Programs` (crate `lnk` pour lire la cible).
- Lister aussi les applications du Store (UWP/MSIX) via l'espace `shell:AppsFolder` (crate `windows` : `IShellItem`, `SHGetKnownFolderItem` avec `FOLDERID_AppsFolder`), et les lancer par leur AppUserModelID.
- Icônes : `SHGetFileInfoW` ou `IShellItemImageFactory`, avec un cache sur disque.
- Action secondaire : lancer en administrateur (`ShellExecuteW` avec le verbe `runas`).

**Linux**
- Lire les fichiers `.desktop` de `$XDG_DATA_DIRS/applications` et `$XDG_DATA_HOME/applications`, Flatpak compris (`/var/lib/flatpak/exports/share`, `~/.local/share/flatpak/exports/share`) et Snap (`/var/lib/snapd/desktop`). Crate `freedesktop-desktop-entry`.
- Respecter `NoDisplay`, `Hidden`, `OnlyShowIn`/`NotShowIn`.
- Indexer **à la fois** le nom traduit (`Name[fr]`) et le nom anglais (`Name`), ainsi que `GenericName` et `Keywords` dans les deux langues : `Fichiers` et `Files` doivent tous les deux trouver le gestionnaire de fichiers.
- Gérer les arguments `Exec` (`%f`, `%u`, etc.) correctement.
- Icônes : thème d'icônes courant via `freedesktop-icons`.

**Les deux** :
- Surveiller les dossiers sources avec `notify` pour mettre la liste à jour à chaud.
- Les noms d'applications sont recherchables **en français et en anglais**. Sous Windows, les raccourcis du menu Démarrer sont traduits (`Paramètres`, `Explorateur de fichiers`, `Calculatrice`) : indexer aussi le nom du fichier cible (`explorer.exe`) et une petite table d'alias anglais pour les applications intégrées de Windows (`Settings`, `File Explorer`, `Calculator`, `Task Manager`, `Control Panel`…).

### 5.2 Fichiers
- Construire un **index en mémoire** des chemins à partir de racines configurables (par défaut : dossier personnel, Bureau, Documents, Téléchargements, Images, plus les autres disques sous Windows, à activer dans la config).
- Parcours parallèle avec `jwalk` ou `ignore` (`WalkParallel`), en excluant par défaut : `node_modules`, `.git`, `target`, `AppData\Local\Temp`, `$Recycle.Bin`, `/proc`, `/sys`, les dossiers cachés (configurable), plus les motifs de la config.
- Mise à jour incrémentale avec `notify`.
- **Index sauvegardé sur disque et ouvert directement depuis le fichier** (`memmap2` + `rkyv`), pour que Flair soit utilisable immédiatement après un redémarrage, puis resynchronisation en arrière-plan.
- Stockage compact (tampon de noms unique + arbre parent/enfant), pour tenir l'objectif mémoire. Détails, surveillance et priorité d'indexation : section 3, « Sobriété quand Flair est caché ».
- Si la requête ressemble à un chemin (`C:\Us`, `~/Doc`, `/etc/`), proposer le contenu du dossier avec complétion par `Tab`.
- Plus tard, en option sous Windows : lecture de la MFT/USN Journal NTFS pour une indexation façon *Everything*. **À ne pas faire dans les premiers jalons**, mais à garder possible dans l'architecture.

### 5.3 Calculatrice et conversions

La calculatrice est une **fonctionnalité phare**. L'objectif est de comprendre le plus de choses possible, comme on les écrirait naturellement en français. Une saisie que l'utilisateur pense être un calcul doit presque toujours donner un résultat.

#### Architecture du moteur de calcul

C'est un pipeline en 4 étapes, dans un module `calc` dédié et testable sans l'UI :

1. **Détection** : décider rapidement si la saisie ressemble à un calcul (chiffres, opérateurs, unités, mots-clés). Si ce n'est pas le cas, on ne l'évalue même pas.
2. **Normalisation bilingue (français et anglais)** : découper la saisie en jetons, puis la réécrire avec le **lexique bilingue** (section 2, « Langues ») en syntaxe que le moteur comprend, quelle que soit la langue utilisée :
   - mots-clés : `en`, `vers`, `to`, `in`, `as` → conversion ; `de`, `of` dans les pourcentages → `of` ; `fois`, `times` → `*` ; `divisé par`, `divided by` → `/` ; `plus`, `moins`, `minus` ; `maintenant`, `now` ; `il y a`, `ago` ; `dans` / `in` en début de saisie ; `à`, `at` pour les débits… ;
   - virgule décimale : `1,5` → `1.5`, selon le réglage `decimal_separator` et les règles d'ambiguïté de la section 2. Si `,` est la virgule décimale, les arguments des fonctions se séparent par `;` (`;` est toujours accepté ; `,` l'est quand ce n'est pas ambigu) ;
   - séparateurs de milliers : `1 000 000`, `1_000_000` et `1,000,000` (format anglais) → `1000000`. On ne fusionne que des groupes d'exactement 3 chiffres, pour ne pas casser `5 km` ;
   - unités dans les deux langues : `o`, `Ko`, `Mo`, `Go`, `To`, `Po` et `B`, `KB`/`kB`, `MB`, `GB`, `TB`, `PB` → octets décimaux ; `Kio`, `Mio`, `Gio`, `Tio` et `KiB`, `MiB`, `GiB`, `TiB` → binaires ; `octet(s)`, `byte(s)`, `bit(s)` ; `j`, `jour(s)`, `d`, `day(s)` ; `sem`, `semaine(s)`, `w`, `wk`, `week(s)` ; `an(s)`, `année(s)`, `y`, `yr`, `year(s)` ; `mois`, `month(s)` ; noms des mois et des jours en français et en anglais, abrégés ou non ;
   - **syntaxe compacte des heures** : `5h36` → `5h + 36min` ; `1h05m30s` ; `2:45:10` ; `14h30` (voir les durées plus bas).
3. **Évaluateurs** :
   - **`fend-core`** est le moteur principal : précision arbitraire, fractions exactes, unités, nombres complexes, bases.
   - Des **évaluateurs spécialisés** gèrent ce que fend ne fait pas ou fait mal : heures de la journée, dates en format français, horodatages Unix, devises, opérations bit à bit si besoin.
   - On essaie l'évaluateur spécialisé correspondant, sinon fend. Le premier résultat valide gagne.
4. **Mise en forme** : afficher le résultat principal en grand, et des **variantes** en sous-résultats (voir plus bas).

Avant de coder, **vérifie précisément ce que `fend-core` supporte** (fonctions, unités, opérateurs bit à bit, fuseaux horaires). Ne réimplémente que ce qui manque, et liste ces manques dans le résumé du jalon.

#### Ce qui doit fonctionner

Les exemples ci-dessous sont écrits en français, mais **chacun doit aussi fonctionner avec son équivalent anglais** (`maintenant + 3h` / `now + 3h`, `4 Go à 100 Mbit/s` / `4 GB at 100 Mbit/s`, `26/11/2026 + 45 jours` / `2026-11-26 + 45 days`…).

**Nombres et notation scientifique**
- Précision arbitraire, sans dépassement : `1e524 * 1.5` = `1.5e524` ; `2^1000` exact ; `100!` exact.
- Notation scientifique en entrée (`6.022e23`, `1E-9`, `3×10^8`) et en sortie automatique pour les très grands et très petits nombres. Le nombre de chiffres significatifs est réglable.
- Fractions exactes : `1/3 + 1/6` = `1/2` (avec la variante `0.5`).
- Pourcentages : `15% de 240` = `36` ; `240 + 15%` = `276` ; `240 - 15%` = `204` ; `36 sur 240 en %` = `15%`.
- Opérateurs : `+ - * / ^ ** % mod !`, parenthèses, multiplication implicite (`2pi`, `3(4+1)`).

**Fonctions et constantes**
- `sqrt`, `cbrt`, racine n-ième, `abs`, `floor`, `ceil`, `round`, `ln`, `log`, `log2`, `log10`, `exp`.
- Trigonométrie : `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, versions hyperboliques. Degrés ou radians : `sin(30°)` = `0.5`, avec une unité par défaut réglable dans la config.
- Combinatoire et entiers : `n!`, `nCr` / `nPr` (ou `C(n,k)`), `pgcd`/`gcd`, `ppcm`/`lcm`, nombre premier ou non.
- Nombres complexes : `sqrt(-1)` = `i`, `(1+2i)*(3-i)`.
- Constantes : `pi`, `e`, `phi`, `c`, `G`, `h`, `k_B`, `N_A`, la charge et la masse de l'électron, etc.

**Octets, bits et débits**
- Conversions décimales et binaires : `1 Gio en Mo` = `1073.741824 Mo` ; `500 Go en Gio` ≈ `465.66 Gio` (pour comprendre la taille réelle affichée d'un disque).
- Bits et octets : `100 Mbit/s en Mo/s` = `12.5 Mo/s`.
- **Temps de transfert** : `4 Go à 100 Mbit/s` = `5 min 20 s` ; `1.5 To à 250 Mo/s`.
- Unités anglaises acceptées partout (`KB`, `MB`, `GiB`, `B`, `bit`, `byte`).
- Option dans la config : interpréter `Ko`/`Mo`/`Go` en binaire (comme l'Explorateur Windows) plutôt qu'en décimal (SI, par défaut).

**Durées et heures**
- Additionner et soustraire des durées : `5h36 + 65min` = `6h41` ; `1h45 * 3` = `5h15` ; `3h - 47min` = `2h13`.
- Conversions : `3h45 en min` = `225 min` ; `2.5 h en h:min` = `2h30` ; `10000 s en h:min:s` = `2h46m40s` ; `90 min en h` = `1.5 h`.
- **Heures de la journée** : `14h30 + 2h45` = `17h15` (et non `16h75`) ; `23h + 3h` = `2h (lendemain)` ; `18h - 9h15` = `8h45` (durée entre deux heures).
- Relatif à maintenant : `maintenant + 3h`, `dans 90 min`, `il y a 2h`.
- Affichage : format `6h41` par défaut, avec les variantes `6:41`, `401 min` et `6.683 h`.

**Dates**
- Format français `jj/mm/aaaa` (et ISO `aaaa-mm-jj`) : `26/11/2026 + 45 jours` = `10/01/2027 (dimanche)`.
- Différence entre deux dates : `25/12/2026 - 26/11/2026` = `29 jours`.
- `jours jusqu'au 25/12`, `semaines depuis le 01/01/2026`, `quel jour 14/07/1789`.
- Horodatage Unix : `@1700000000` = `14/11/2023 22:13:20 UTC` (plus l'heure locale) ; `maintenant en unix`.
- Fuseaux horaires (si c'est réaliste) : `15h Tokyo en Paris`, `heure à New York`.
- **L'heure courante est injectée** (trait `Clock`) pour que les tests soient déterministes.

**Bases numériques et opérations bit à bit**
- Entrée en `0x`, `0b`, `0o` ; conversions : `255 en hex` = `0xff`, `0xff en bin`, `42 en base 36`.
- Bit à bit : `and`/`&`, `or`/`|`, `xor`, `not`/`~`, `<<`, `>>`.
- Variantes automatiques : si le résultat est entier, on affiche aussi hexadécimal, binaire et octal.

**Unités physiques et vie courante**
- Longueur, masse, volume, surface, vitesse, accélération, énergie, puissance, pression, angle, fréquence : `90 km/h en m/s` = `25 m/s` ; `1 année-lumière en km` ; `5 pieds 11 en cm`.
- Températures : `30 °C en °F`, `300 K en °C`.
- Cuisine : `2 tasses en ml`, `1 cuillère à soupe en g` (approximation pour l'eau).
- Analyse dimensionnelle : `10 kW * 3h` = `30 kWh` ; `100 km / 1h30` = `66.67 km/h`.

**Devises** (fonction optionnelle, `currency = true` dans la config)
- `100 € en $`, `50 usd en eur`, `1 btc en €` (crypto en option).
- Taux récupérés en arrière-plan depuis une API gratuite sans clé (ex. Frankfurter / BCE), **mis en cache sur disque**, rafraîchis au maximum une fois toutes les quelques heures. On affiche la date des taux. Hors ligne, on utilise le cache **sans jamais bloquer la saisie**.

#### Affichage et actions
- Le résultat est mis à jour **en direct** à chaque frappe. Les erreurs de syntaxe pendant la frappe sont ignorées en silence : on garde le dernier résultat valide en grisé, pas de message d'erreur à chaque caractère.
- Le résultat principal s'affiche en grand, suivi de **variantes** utiles sous forme de résultats secondaires sélectionnables : fraction et décimale, notation scientifique et nombre complet, `6h41` / `401 min` / `6.683 h`, hex / bin / oct, Mo et Mio…
- `Entrée` copie le résultat **tel qu'il est affiché**. `Ctrl+Entrée` copie l'expression complète (`5h36 + 65min = 6h41`). `Alt+Entrée` copie la valeur brute sans unité.
- Ne proposer un résultat que si l'expression s'évalue **et** n'est pas triviale (taper `42` ou `pi` seul ne doit pas afficher de calcul, mais `pi*2` oui).
- Historique des derniers calculs, consultable avec le préfixe `=` et une requête vide.
- Presse-papier : crate `arboard`.

#### Robustesse
- **Délai maximal d'évaluation** (environ 50 ms) et limites de taille : `10^10^10^10`, `1000000!` ou une exponentiation énorme ne doivent jamais bloquer ni faire exploser la mémoire. Au-delà de la limite, afficher « trop grand pour être calculé ».
- L'évaluation tourne hors du thread de l'UI, et chaque nouvelle frappe annule l'évaluation précédente.
- Aucune exécution de code arbitraire : c'est un évaluateur d'expressions, pas un interpréteur.

#### Tests
- Un **tableau de tests** (`calc_cases.rs` ou fichier TOML/JSON), avec **au moins 150 cas** couvrant chaque catégorie ci-dessus. Pour chaque cas : `entrée → résultat affiché attendu`. Inclure les exemples de cette section, des cas limites (division par zéro, unités incompatibles, saisies ambiguës) et des saisies qui **ne doivent pas** être traitées comme des calculs (`firefox`, `rapport 2024.pdf`, `c++`).
- **Bilinguisme** : chaque cas français du tableau a son **jumeau anglais**, qui doit donner le même résultat. On ajoute aussi des cas mélangés (`now + 3 jours`) et un test par règle d'ambiguïté de la section 2 (`5m`, `12 in en cm`, `1,500`, `01/02/2026` en `dmy` et en `mdy`).
- Les résultats affichés sont vérifiés dans les deux langues d'interface (`language = "fr"` et `"en"`).
- Tests de propriétés (`proptest`) sur la normalisation : elle ne doit jamais paniquer, quelle que soit la saisie.

### 5.4 Recherche web
- Mots-clés configurables dans la config, par exemple :
  ```toml
  [[web]]
  keyword = "g"
  name = "Google"
  url = "https://www.google.com/search?q={query}"
  ```
- Encoder correctement la requête dans l'URL.
- Proposer « Rechercher sur le web » en dernier résultat quand rien d'autre ne correspond bien.

### 5.5 Commandes système
- Verrouiller, veille, éteindre, redémarrer, se déconnecter, vider la corbeille. Chaque commande est reconnue en français **et** en anglais (`verrouiller` / `lock`, `veille` / `sleep`, `éteindre` / `shutdown` / `power off`, `redémarrer` / `restart` / `reboot`, `se déconnecter` / `log out` / `sign out`, `vider la corbeille` / `empty trash` / `empty recycle bin`).
- Windows : API Win32 (`LockWorkStation`, `ExitWindowsEx`, `SetSuspendState`, `SHEmptyRecycleBinW`).
- Linux : `loginctl` / D-Bus `org.freedesktop.login1` (crate `zbus`), corbeille via la crate `trash`.
- **Confirmation obligatoire** (deuxième `Entrée`, ou dialogue dans la fenêtre) pour éteindre, redémarrer, se déconnecter et vider la corbeille.

---

## 6. Spécificités des plateformes

### Raccourci global
- **Windows et X11** : crate `global-hotkey`.
- **Wayland** : `global-hotkey` ne fonctionne pas. Ordre de tentative :
  1. le portail XDG **GlobalShortcuts** via `ashpd` (KDE Plasma 6, GNOME récent) ;
  2. sinon, afficher au premier lancement une notification expliquant comment lier un raccourci du bureau à `flair --toggle`.
- Si le raccourci est déjà pris (par exemple `Alt+Espace` par une autre app), afficher une erreur claire avec un lien vers la config, sans planter.
- **Ne pas utiliser `Win+Espace`** par défaut : Windows le réserve au changement de langue du clavier.

### Fenêtre sous Wayland
- Le positionnement libre est impossible sous Wayland standard. Sur les compositeurs qui supportent **layer-shell** (KDE, bureaux basés sur wlroots), l'utiliser (`iced_layershell` ou équivalent). Sous GNOME, se contenter d'une fenêtre normale et documenter la limitation.

### Icône de notification
- Crate `tray-icon` avec un menu : Afficher, Réindexer, Ouvrir la config, Lancer au démarrage (case à cocher), Quitter.
- Lancement au démarrage : clé `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` sous Windows, `~/.config/autostart/flair.desktop` sous Linux.

---

## 7. Interface et design

### Technique
- Framework : **Iced** (dernière version stable). Il est 100 % Rust, gère les fenêtres transparentes, et cosmic-launcher et onagre l'utilisent déjà pour des launchers.
- Architecture Elm : `State`, `Message`, `update`, `view`. Les recherches sont lancées comme `Task` asynchrones. Chaque requête porte un numéro ; on ignore les réponses dont le numéro n'est plus le dernier.
- Rendu des icônes avec un cache (LRU) ; ne jamais charger une icône sur le thread de l'UI.
- Si les contraintes de Wayland ou de transparence rendent Iced bloquant, **arrête-toi et explique le problème** avant de changer de framework (alternatives possibles : Slint, ou Tauri v2).

### Principes de design
Flair doit être **ultra minimaliste et sobre**. Rien n'est affiché qui ne serve pas directement à trouver ou lancer quelque chose.
- **3 couleurs, pas une de plus** : un fond, un texte, un accent. Toutes les autres teintes de l'interface (texte secondaire, sélection, bordure…) sont **calculées** à partir de ces 3 couleurs. Aucune couleur n'est écrite en dur dans un widget.
- **Pas de rouge ni de vert** pour les erreurs, avertissements ou confirmations : on utilise l'accent et un texte clair (« Entrée à nouveau pour éteindre »).
- Pas de dégradés, pas de motifs, pas de logo dans la fenêtre, pas de barre de statut, pas de bouton visible.
- Seules exceptions aux 3 couleurs : les icônes d'applications et de fichiers en mode `icons = "color"` (c'est du contenu, pas de l'interface) et l'ombre de la fenêtre (noir transparent).

### Maquettes

Barre vide : seulement le champ de saisie.
```
╭────────────────────────────────────────────────────────────╮
│   ⌕   Rechercher…                                          │
╰────────────────────────────────────────────────────────────╯
```

Recherche d'application (les lettres trouvées `V`, `S`, `C` sont en couleur d'accent) :
```
╭────────────────────────────────────────────────────────────╮
│   ⌕   vsc▏                                                 │
├────────────────────────────────────────────────────────────┤
│ ░░ ▣  Visual Studio Code                       ↵ Ouvrir ░░ │  ← ligne sélectionnée
│ ░░    Application                                       ░░ │
│    ▣  vsc-backup.zip                                       │
│       ~/Documents/archives                                 │
│    ⌕  Rechercher « vsc » sur le web                        │
╰────────────────────────────────────────────────────────────╯
```

Calcul (le `=` est en couleur d'accent, le résultat principal est plus grand) :
```
╭────────────────────────────────────────────────────────────╮
│   ⌕   5h36 + 65min▏                                        │
├────────────────────────────────────────────────────────────┤
│ ░░ =  6h41                                     ↵ Copier ░░ │
│ ░░    5h36 + 65min                                      ░░ │
│    =  401 min                                              │
│    =  6.683 h                                              │
╰────────────────────────────────────────────────────────────╯
```

### Dimensions et typographie
Grille de 4 px. Toutes ces valeurs sont des constantes dans un seul module (`design.rs`), jamais répétées dans le code.

| Élément | Valeur |
|---|---|
| Largeur de la fenêtre | 680 px |
| Rayon des coins | 12 px (fenêtre), 8 px (ligne sélectionnée) |
| Bordure de la fenêtre | 1 px, couleur `bordure` |
| Ombre | noir à environ 25 % d'opacité, flou de 24 px, décalage vertical de 8 px (si la plateforme le permet) |
| Champ de saisie | hauteur 56 px, texte 20 px, marges latérales 16 px |
| Séparateur champ / résultats | 1 px, couleur `bordure`, affiché seulement s'il y a des résultats |
| Ligne de résultat | hauteur 48 px (titre et sous-titre) ou 40 px (titre seul), marges 8 px |
| Icône | 24 px |
| Titre / sous-titre | 14 px (poids moyen) / 12 px (normal) |
| Résultat de calcul principal | 22 px, chiffres à chasse fixe |
| Barre de défilement | 4 px, couleur `texte discret`, visible seulement pendant le défilement |

- **Police** : **Inter** intégrée au binaire (licence OFL), pour un rendu identique sous Windows et Linux. Avec `font = "system"`, on utilise la police système (Segoe UI Variable sous Windows 11, la police d'interface du bureau sous Linux), ou le nom d'une police installée.
- **Chiffres des calculs** : chiffres tabulaires d'Inter si Iced le permet, sinon une police mono intégrée (JetBrains Mono, licence OFL) pour les résultats de calcul uniquement.
- **Icônes de l'interface** (loupe, `=`, web, commandes système, types de fichiers génériques) : jeu d'icônes au trait **Lucide** (licence ISC), en SVG, teintées en `texte secondaire`.

### Animations
- Apparition : fondu de 80 ms maximum. **L'animation ne doit jamais retarder l'affichage** : la fenêtre doit être visible et utilisable dans les 50 ms, l'animation est purement décorative.
- Disparition : instantanée.
- Pas d'animation sur la liste de résultats ni sur la sélection, qui se déplace instantanément.
- Si le système demande de réduire les animations, ou avec `animations = false`, aucune animation.

### Système de thèmes : 3 couleurs → un thème complet

Un thème, c'est **exactement 3 couleurs** :

| Couleur | Rôle |
|---|---|
| `background` | Fond de la fenêtre |
| `foreground` | Texte principal |
| `accent` | Lettres trouvées, `=` des calculs, confirmations, curseur du champ de saisie |

Toutes les autres couleurs de l'interface (les **jetons**) sont dérivées automatiquement :

| Jeton | Calcul de départ | Contraste minimum garanti |
|---|---|---|
| `fond` | `background` | — |
| `texte` | `foreground` | ≥ 7:1 sur `fond` (≥ 4.5:1 au minimum absolu) |
| `texte secondaire` (sous-titres, actions) | mélange `foreground` → `background` à 35 % | ≥ 4.5:1 sur `fond` |
| `texte discret` (texte du champ vide, numéros `Ctrl`, barre de défilement) | mélange `foreground` → `background` à 50 % | ≥ 3:1 sur `fond` |
| `survol` | mélange `background` → `foreground` à 4 % | — |
| `sélection` (fond de la ligne choisie) | mélange `background` → `accent` à 10 % | `texte` et `accent` restent lisibles dessus (≥ 4.5:1) |
| `bordure` | mélange `background` → `foreground` à 10 % | — |
| `accent` | `accent` | ≥ 4.5:1 sur `fond` **et** sur `sélection` |
| `texte sur accent` (rare, si un élément a un fond d'accent) | `background` ou `foreground`, celui qui contraste le plus | ≥ 4.5:1 |

Règles de calcul :
- Les mélanges se font dans l'espace de couleur **Oklab**, et les ajustements dans **OKLCH**, pour un résultat perceptuellement régulier. Crate `palette`.
- Le contraste est mesuré avec la formule **WCAG 2** (rapport de luminance).
- Si un minimum n'est pas atteint, on **ajuste automatiquement** la luminosité (L en OKLCH) de la couleur concernée en l'éloignant du fond, par petits pas, en gardant sa teinte. On réduit la saturation si la couleur sort de la gamme sRGB. On ne refuse **jamais** un thème : on l'ajuste et on signale l'ajustement (« contraste ajusté ») dans l'aperçu et dans les logs.
- Un thème est classé **clair ou sombre** automatiquement selon la luminosité du fond (par exemple L Oklab > 0.6 = clair). Ça sert à choisir les icônes et à ranger le thème dans le bon emplacement (`light` ou `dark`).
- Les jetons sont calculés **une seule fois** à chaque changement de thème, pas à chaque image.
- Tests :
  - chaque thème intégré respecte tous les minimums sans ajustement ;
  - test de propriétés (`proptest`) : pour **3 couleurs aléatoires**, les jetons dérivés respectent toujours tous les minimums ;
  - le calcul est déterministe : mêmes couleurs, mêmes jetons.

### Thèmes intégrés
Les contrastes de ces 7 thèmes ont été vérifiés : texte principal ≥ 12.8:1, accent ≥ 5.8:1 sur le fond et ≥ 5:1 sur la sélection, texte secondaire ≥ 4.5:1, texte discret ≥ 3:1.

| Nom | Type | `background` | `foreground` | `accent` |
|---|---|---|---|---|
| **Flair clair** *(défaut clair)* | clair | `#FAFAF9` | `#1C1B1A` | `#B03A0A` |
| **Flair sombre** *(défaut sombre)* | sombre | `#161615` | `#ECEBE8` | `#FB923C` |
| Papier | clair | `#F4EFE6` | `#2B2620` | `#7A4A1F` |
| Mono | clair | `#FFFFFF` | `#111111` | `#111111` |
| Encre | sombre | `#0F1115` | `#D7DAE0` | `#7AA2F7` |
| Forêt | sombre | `#1A1F1A` | `#DDE3D6` | `#8FBF7F` |
| Océan | sombre | `#0E1A24` | `#D6E4EE` | `#5CC8D6` |

Les thèmes par défaut ont des tons neutres, très légèrement chauds, avec un accent orange braise (le « flair », l'étincelle). Dans le thème Mono, l'accent est égal au texte : les lettres trouvées se distinguent seulement par le semi-gras.

### Thèmes personnalisés
**Par fichier.** Chaque thème est un petit fichier TOML dans le dossier `themes/` de la config (`%AppData%\Flair\themes\`, `~/.config/flair/themes/`), rechargé à chaud :
```toml
# themes/lavande.toml
name = "Lavande"
background = "#1E1B2E"
foreground = "#E6E1F5"
accent = "#B79CFF"
```
- Couleurs acceptées : `#RGB`, `#RRGGBB`, avec ou sans `#`.
- Avec seulement 2 couleurs (pas d'`accent`), l'accent est égal au texte, comme le thème Mono.
- Un fichier invalide est ignoré avec un message clair. Il ne casse jamais le thème actif.

**Directement depuis Flair**, dans l'esprit d'un launcher :
- `> thème` ou `> theme` liste tous les thèmes. **Naviguer dans la liste applique le thème en aperçu en direct** ; `Entrée` le garde, `Échap` revient au thème précédent.
- `> thème 1E1B2E E6E1F5 B79CFF lavande` crée un thème à partir de 3 couleurs (fond, texte, accent, dans cet ordre) et l'affiche **en aperçu immédiatement**, avec un petit échantillon des 3 couleurs et l'indication « contraste ajusté » si besoin. `Entrée` enregistre le fichier `themes/lavande.toml` et l'applique. Si on ne donne pas de nom, il s'appelle `perso-1`, `perso-2`…
- Le nouveau thème va automatiquement à l'emplacement `light` ou `dark` selon son fond.

### Suivi du thème système
- En mode `system`, Flair bascule entre ses thèmes `light` et `dark` **en direct** quand le système change, sans redémarrage.
- Windows : valeur de registre `AppsUseLightTheme`, surveillée. Linux : portail XDG `org.freedesktop.appearance` / `color-scheme` (via `ashpd`), avec la crate `dark-light` en secours.

---

## 8. Configuration

- Fichier TOML :
  - Windows : `%AppData%\Flair\config.toml`
  - Linux : `~/.config/flair/config.toml`
  - Chemins obtenus avec la crate `directories`.
- Créé avec des valeurs par défaut commentées au premier lancement.
- Rechargement à chaud quand le fichier change.
- Une erreur de syntaxe dans la config n'empêche **jamais** le démarrage : on garde la dernière config valide (ou les valeurs par défaut) et on affiche l'erreur.

```toml
hotkey = "Alt+Space"
language = "system"         # system | fr | en — langue de l'interface et des résultats (la saisie accepte toujours FR et EN)
decimal_separator = "auto"  # auto | "," | "."
date_format = "auto"        # auto | dmy | mdy | ymd
max_results = 8
keep_last_query = false

[theme]
mode = "system"             # system | light | dark
light = "Flair clair"       # thème utilisé en mode clair
dark = "Flair sombre"       # thème utilisé en mode sombre
opacity = 1.0               # 0.85 à 1.0 : transparence du fond, si la plateforme le permet
icons = "color"             # color | mono (icônes d'applications en couleur ou monochromes)
font = "inter"              # inter | system | nom d'une police installée
animations = true

[files]
enabled = true
roots = ["~"]
exclude = ["node_modules", ".git", "target", ".cache", "__pycache__"]
include_hidden = false

[resources]
renderer = "auto"           # auto | gpu | cpu
pause_on_battery = true     # suspendre la surveillance et l'indexation sur batterie
trim_memory_after = 5       # secondes après le masquage avant de libérer la mémoire (0 = jamais)
efficiency_mode = true      # Windows : mode efficacité (EcoQoS) quand Flair est caché

[providers]
apps = true
calculator = true
web = true
system = true
```

- Données (index, frecency, cache d'icônes) : `%LocalAppData%\Flair\` sous Windows, `~/.local/share/flair/` et `~/.cache/flair/` sous Linux.

---

## 9. Qualité du code

- Rust stable, édition 2024. Pas de `unsafe` hors de `flair-platform` ; dans ce crate, chaque bloc `unsafe` a un commentaire `// SAFETY:`.
- Erreurs : `thiserror` dans les bibliothèques, `anyhow` dans le binaire. **Aucun `unwrap()` ni `expect()`** sur une donnée venant de l'extérieur (fichiers, OS, config).
- Logs avec `tracing`, dans un fichier qui tourne (`tracing-appender`) placé dans le dossier de données. Niveau réglable via `FLAIR_LOG`.
- Code propre à une plateforme isolé avec `#[cfg(windows)]` / `#[cfg(target_os = "linux")]` derrière des traits communs. Pas de `cfg` éparpillés dans le reste du code.
- Tests :
  - unitaires pour le classement, l'analyse des préfixes, la lecture des `.desktop`, la détection des expressions de calcul ;
  - benchmarks (`criterion`) pour la recherche dans l'index de fichiers (1 million de chemins générés) ;
  - les tests d'intégration propres à l'OS sont marqués `#[ignore]` s'ils demandent un vrai bureau.
- Intégration continue (GitHub Actions) : `fmt`, `clippy`, `test` sur `windows-latest` et `ubuntu-latest`.

---

## 10. Environnement de développement

Le développeur travaille **sous WSL2** (Ubuntu) sur une machine Windows 11.
- Compiler la version Linux : `cargo build`. Elle peut se lancer via WSLg, mais **le raccourci global n'y fonctionne pas**, car Windows intercepte le clavier. Prévoir un flag `--show` qui affiche la fenêtre directement pour tester l'UI.
- Compiler la version Windows depuis WSL :
  ```bash
  cargo install cargo-xwin
  rustup target add x86_64-pc-windows-msvc
  cargo xwin build --release --target x86_64-pc-windows-msvc -p flair-app
  ```
  puis lancer le `.exe` (depuis WSL ou en le copiant sous `C:\`).
- Le binaire Windows doit utiliser `#![windows_subsystem = "windows"]` en release, pour qu'aucune console ne s'ouvre.
- Documente dans le `README.md` toutes les commandes de compilation et de lancement pour les deux plateformes.

---

## 11. Jalons

Chaque jalon se termine par : compilation OK sur les deux cibles, clippy propre, tests verts, `README` à jour, et un court résumé de ce qui a été fait et de ce qui reste.

1. **Squelette** : workspace, crates, trait `Provider`, config TOML, logs, traductions de l'interface FR/EN, **système de thèmes** (3 couleurs → jetons, ajustement du contraste, thèmes intégrés avec leurs tests), fenêtre Iced fidèle aux maquettes et dimensions de la section 7 avec des résultats factices, navigation au clavier.
2. **Afficher et cacher** : app résidente, icône de notification, raccourci global (Windows et X11), instance unique et `--toggle` via IPC, se cache quand elle perd le focus. Dès ce jalon : **aucun réveil quand la fenêtre est cachée**, compteur de réveils et première version de `flair --stats`.
3. **Applications et calcul de base** : provider d'applications Windows et Linux avec icônes ; moteur de calcul (pipeline complet, lexique et normalisation bilingues FR/EN, règles d'ambiguïté, `fend-core`, nombres, fonctions, octets, unités, variantes) avec son tableau de tests ; classement avec `nucleo` et frecency.
4. **Calculs avancés** : durées et heures de la journée, dates, horodatages Unix, bases et opérations bit à bit, temps de transfert, devises avec cache, historique des calculs, limites de robustesse.
5. **Fichiers** : index parallèle **compact et ouvert depuis le disque** (`memmap2` + `rkyv`), indexation en priorité arrière-plan, mise à jour avec `notify` regroupée, exclusions par défaut, mode batterie, complétion de chemins, action « ouvrir le dossier ».
6. **Web et système** : mots-clés de recherche web, commandes système avec confirmation, préfixes de filtrage.
7. **Wayland et finitions** : portail GlobalShortcuts, layer-shell quand il est disponible, suivi du thème système en direct, thèmes personnalisés (fichiers, `> thème` avec aperçu en direct, création à partir de 3 couleurs), lancement au démarrage, rechargement à chaud de la config, **libération de la mémoire quand l'app est cachée** (EcoQoS, `EmptyWorkingSet`, `malloc_trim`), comparaison et choix du moteur de rendu, benchmarks, CI.

---

## 12. Critères d'acceptation globaux

- [ ] `Alt+Espace` affiche la barre en moins de 50 ms sous Windows 11 ; un second appui la cache.
- [ ] Taper `vsc` puis `Entrée` lance Visual Studio Code s'il est installé.
- [ ] Taper `= 15% de 240` affiche `36` ; `Entrée` le copie dans le presse-papier.
- [ ] Taper `5 km en miles` affiche environ `3.107 miles`.
- [ ] `1e524 * 1.5` affiche `1.5e524` (aucun dépassement, aucune perte de précision).
- [ ] `5h36 + 65min` affiche `6h41` ; `14h30 + 2h45` affiche `17h15`.
- [ ] `4 Go à 100 Mbit/s` affiche `5 min 20 s` ; `500 Go en Gio` affiche environ `465.66 Gio`.
- [ ] `26/11/2026 + 45 jours` affiche `10/01/2027`.
- [ ] `255 en hex` affiche `0xff` ; `1,5 * 2` affiche `3` (virgule décimale française).
- [ ] `1000000!` ou `10^10^10^10` répond « trop grand » en moins de 100 ms, sans bloquer l'interface.
- [ ] Le tableau de tests de calcul (au moins 150 cas) passe entièrement.
- [ ] `now + 3h` donne le même résultat que `maintenant + 3h` ; `4 GB at 100 Mbit/s` le même que `4 Go à 100 Mbit/s` ; `4d` le même que `4j`.
- [ ] Les saisies mélangées fonctionnent : `now + 3 jours`, `4 Go at 100 Mbit/s`.
- [ ] `> lock` et `> verrouiller` verrouillent tous les deux la session.
- [ ] Avec `language = "en"`, l'interface et les résultats sont en anglais (`January 10, 2027 (Sunday)`) ; avec `"fr"`, en français (`10 janvier 2027 (dimanche)`).
- [ ] Taper le nom d'un fichier de `~/Documents` le trouve en moins de 50 ms ; `Ctrl+Entrée` ouvre son dossier.
- [ ] Taper `g rust iced` puis `Entrée` ouvre la recherche Google dans le navigateur par défaut.
- [ ] `> éteindre` demande une confirmation avant d'agir.
- [ ] Une config TOML invalide n'empêche pas le démarrage et un message d'erreur clair s'affiche.
- [ ] Taper très vite ne bloque jamais l'interface et n'affiche jamais les résultats d'une ancienne requête.
- [ ] Au repos, l'utilisation du CPU est de 0 % : après 1 h fenêtre cachée sans modification de fichiers, le compteur de réveils de `flair --stats` n'a pas bougé.
- [ ] Fenêtre cachée : moins de 40 Mo de RAM sans index de fichiers, moins de 80 Mo avec un index d'un million de fichiers.
- [ ] Après la libération de la mémoire, l'affichage suivant reste sous les 50 ms.
- [ ] Une compilation qui modifie 10 000 fichiers dans un dossier surveillé ne provoque qu'une poignée de réveils (événements regroupés).
- [ ] Pendant l'indexation initiale, le PC reste fluide (thread en priorité arrière-plan pour le CPU et le disque).
- [ ] Sur batterie avec `pause_on_battery = true`, aucune activité d'indexation n'a lieu.
- [ ] Aucune couleur n'est écrite en dur hors du module de thèmes : toute l'interface utilise les jetons dérivés des 3 couleurs.
- [ ] Passer le système de clair à sombre change le thème de Flair en direct.
- [ ] `> thème 1E1B2E E6E1F5 B79CFF lavande` affiche l'aperçu immédiatement ; `Entrée` crée `themes/lavande.toml` et l'applique.
- [ ] Un thème aux couleurs illisibles (ex. `#777777 #888888 #808080`) est ajusté automatiquement et reste lisible.

---

## Instructions de travail

- Commence par le **jalon 1** uniquement. Montre l'arborescence et les choix de crates (avec leurs versions) avant d'écrire beaucoup de code.
- Si une contrainte de ce document est irréalisable ou contradictoire sur une plateforme, **signale-le et propose une alternative** au lieu de la contourner en silence.
- Préfère des crates maintenues et largement utilisées ; justifie chaque dépendance qui n'est pas listée ici.
- Écris des commentaires et des messages de commit clairs, en français ou en anglais (choisis une langue et garde-la).

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

Flair est **bilingue français / anglais** : par défaut, toute saisie est comprise dans les deux langues en même temps. L'architecture permet d'ajouter d'autres langues avec un simple fichier, et d'activer ou désactiver chaque langue (voir la section 2, « Langues »).

### Plateformes cibles
- **Windows 10 et 11** (x86_64) : cible prioritaire.
- **Linux** : X11, et Wayland (GNOME, KDE, bureaux basés sur wlroots).
- macOS n'est **pas** une cible.

### Hors périmètre (pour l'instant)
- Système de plugins dynamiques (`.dll`/`.so`, WASM).
- Synchronisation cloud, comptes utilisateur.
- Recherche dans le contenu des fichiers (full-text) : seulement les noms et chemins.
- Installeur classique (MSI, Inno Setup) : Flair s'installe lui-même (section 11).
- Signature de code Windows (certificat Authenticode) : à prévoir plus tard. Les mises à jour, elles, sont signées dès le début (section 11).

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
- `>` : commandes (réglages, thèmes, langues, actions de Flair, commandes système). `> help` / `> aide` les liste toutes (section 8).
- `?` : recherche web
- sans préfixe : toutes les sources, mélangées et classées

### Langues : français et anglais en même temps

Par défaut, Flair est **bilingue**. L'utilisateur peut taper en français ou en anglais, et même **mélanger les deux** dans une même saisie, sans changer de réglage. Il n'y a pas de « mode langue » à basculer : **toutes les langues actives sont reconnues en même temps**. On peut désactiver une langue, ou changer l'ordre de préférence, pour réduire les ambiguïtés (voir « Activer et désactiver des langues »).

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

#### Fichiers de langue (lexiques)
- Les mots de chaque langue sont dans des **fichiers de données intégrés au binaire**, jamais dans le code :
  - `lexicon/common.toml` : partie **neutre, toujours active et non désactivable**. Opérateurs et symboles, unités internationales (`km`, `kg`, `MB`, `GiB`, `°C`, `h`, `min`, `s`), préfixes `0x` / `0b` / `0o`, constantes, et les mots-clés de la commande `> langue` / `> language` ;
  - `lexicon/fr.toml`, `lexicon/en.toml`… : **uniquement les mots** propres à chaque langue (`maintenant`, `jours`, `fois`, `ago`, `days`…), ainsi que ses conventions (séparateur décimal, ordre des dates, noms des mois et des jours).
- Ajouter une langue (espagnol, allemand…) se fait en **ajoutant un fichier**, sans toucher au code.
- L'utilisateur peut ajouter ou surcharger des lexiques dans le dossier `lexicon/` de sa config. Ils sont rechargés à chaud, et un fichier invalide est ignoré avec un message clair.
- Chaque entrée associe une ou plusieurs **formes** (les mots tapés) à un **sens canonique** (`convert_to`, `of`, `now`, `ago`, `unit:day`, `unit:inch`…), avec des **conditions** optionnelles et une **priorité**.
- La reconnaissance se fait **sur des jetons entiers** après découpage de la saisie, jamais par simple remplacement de texte (sinon `de` serait remplacé à l'intérieur de `dernier`, et `in` à l'intérieur de `minutes`). Les formes de plusieurs mots (`il y a`, `divided by`, `jours jusqu'au`) sont reconnues en priorité, la plus longue d'abord.
- Pas de différence entre majuscules et minuscules, **sauf pour les entrées marquées `case_sensitive`** : les unités où ça compte (`Mo` mégaoctet, `mo` jamais ; `T` tesla, `t` tonne, `To` téraoctet ; `mB`, `MB` et `Mb` sont différents).
- Les accents sont optionnels, pour les claviers qui n'en ont pas : `a` = `à`, `annee` = `année`, `deg` = `°`.

**Format d'un fichier de langue :**
```toml
# lexicon/fr.toml
[meta]
code = "fr"
name = "Français"
decimal_separator = ","
date_order = "dmy"

[[word]]
forms = ["en", "vers"]
sense = "convert_to"
when = { between = ["quantity", "unit"] }

[[word]]
forms = ["an", "ans", "année", "années", "annee", "annees"]
sense = "unit:year"
when = { after = "number" }

[[word]]
forms = ["de"]
sense = "of"
when = { after = "percent" }

[[word]]
forms = ["il y a"]
sense = "ago"
when = { position = "start", before = "duration" }
```
```toml
# lexicon/en.toml — le même mot « in » a trois sens possibles
[[word]]
forms = ["in", "to", "as"]
sense = "convert_to"
when = { between = ["quantity", "unit"] }
priority = 30

[[word]]
forms = ["in"]
sense = "relative_in"            # « in 90 min » = dans 90 min
when = { position = "start", before = "duration" }
priority = 20

[[word]]
forms = ["in"]
sense = "unit:inch"              # « 12 in en cm »
when = { after = "number" }
priority = 10

[[word]]
forms = ["an", "a"]
sense = "one"                    # « an hour » = 1 hour
when = { before = "unit" }
```

**Vocabulaire des conditions `when`.** C'est une liste fermée, documentée dans `lexicon/README.md`. Pas de code ni d'expression régulière dans les fichiers de langue. Si un cas ne peut pas être exprimé, on ajoute une condition au vocabulaire (dans le code, avec des tests), on ne bricole pas un cas particulier.

| Condition | Signification |
|---|---|
| `after = "<classe>"` | le jeton précédent est de cette classe |
| `before = "<classe>"` | le jeton suivant est de cette classe |
| `between = ["<classe>", "<classe>"]` | les deux à la fois |
| `position = "start"` / `"end"` | le jeton est au début / à la fin de la saisie |
| `in_duration = true` | le jeton fait partie d'une durée composée (`1h30m`, `5m30s`) |
| `case_sensitive = true` | la casse doit correspondre exactement |

Les classes possibles sont : `number`, `quantity` (nombre + unité), `unit`, `duration`, `time` (heure de la journée), `date`, `percent`, `start`, `end`, ou `sense:<nom>` (un jeton qui a ce sens, par exemple `sense:convert_to`). Une condition peut accepter une liste (`before = ["end", "sense:convert_to"]`).

#### Résolution des ambiguïtés
Quand un mot a plusieurs sens possibles (dans une même langue ou entre plusieurs langues actives), on ne choisit **pas** au hasard :

1. **Candidats** : pour chaque jeton, on récupère **tous les sens** du lexique commun et des langues actives dont les conditions sont remplies. Un jeton inconnu reste tel quel (c'est peut-être un nom d'application ou de fichier).
2. **Lectures** : si au moins un jeton a plusieurs candidats, on construit les lectures possibles (combinaisons), **16 au maximum**, en commençant par les plus prioritaires.
3. **L'évaluation sert d'arbitre** : on évalue chaque lecture et on **élimine celles qui échouent ou n'ont pas de sens** (unités incompatibles, comme `5 km pouce miles`). La plupart des ambiguïtés disparaissent ici, parce qu'une seule lecture donne un calcul valide.
4. **Classement** des lectures restantes, dans cet ordre :
   1. la `priority` des sens utilisés ;
   2. la précision des conditions (une entrée qui a rempli plus de conditions l'emporte) ;
   3. l'**ordre des langues** dans `input_languages` (la première est la préférée) ;
   4. le moins de réinterprétations possible.
5. **Affichage** : la meilleure lecture donne le résultat principal. Jusqu'à 2 autres lectures valides sont proposées en variantes, avec une petite mention discrète : « lu comme : 5 minutes » / « read as: 5 minutes ».
6. **Déterministe** : même saisie et même config = même résultat. Le tout tient dans le délai maximal d'évaluation de la calculatrice.

#### Détection des conflits entre langues
- Au chargement, on repère les **conflits** : une même forme qui a des sens différents dans plusieurs fichiers actifs (par exemple `in` en anglais et en allemand, `a` en anglais, espagnol et italien, `de` en français et en espagnol).
- Un conflit est **résolu** si les conditions sont mutuellement exclusives ou si les priorités diffèrent. Sinon, il est **non résolu** et signalé dans les logs.
- Commande de développement `flair lexicon check [--langs fr,en,es]` : liste tous les conflits (résolus ou non) avec un exemple de saisie pour chacun. Elle sort avec un code d'erreur s'il reste des conflits non résolus. **La CI la lance pour chaque combinaison des langues intégrées.**
- Chaque langue a son fichier de tests `lexicon/tests/<code>.toml` (`saisie → résultat attendu`), exécuté en CI avec cette langue seule **et** combinée aux autres.

#### Activer et désactiver des langues
- Config : `input_languages = ["fr", "en"]`. C'est la liste des langues de saisie actives, **dans l'ordre de préférence** (utilisé pour départager les ambiguïtés).
- Commandes dans Flair (elles-mêmes reconnues dans toutes les langues) :

| Commande | Effet |
|---|---|
| `> langue` / `> language` | Liste les langues disponibles avec leur état (active ou non), leur ordre et la langue de l'interface. Naviguer puis `Entrée` active ou désactive la langue sélectionnée. |
| `> langue fr désactiver` / `> language fr disable` | Désactive le français pour la saisie |
| `> langue fr activer` / `> language fr enable` | Le réactive |
| `> langue en préférer` / `> language en prefer` | Place l'anglais en premier dans l'ordre de préférence |

- Le changement est **appliqué immédiatement** et **écrit dans le fichier de config** en gardant ses commentaires (crate `toml_edit`).
- Garde-fous :
  - au moins **une langue de saisie reste active**. Désactiver la dernière est refusé avec un message clair ;
  - le lexique commun et la commande `> langue` / `> language` restent **toujours** reconnus, donc on peut toujours réactiver une langue ;
  - désactiver une langue de saisie **ne change pas la langue de l'interface** (`ui_language`, réglée séparément) ;
  - les **noms d'applications** traduits (`Name[fr]`, raccourcis Windows en français) restent recherchables : ce sont des noms, pas des mots du lexique ;
  - si `decimal_separator` ou `date_format` est sur `auto` et que la langue du système est désactivée, on suit la convention de la **première langue active**.

#### Ambiguïtés : règles imposées
Ces règles **ne sont pas codées en dur**. Elles s'écrivent dans les fichiers de langue avec des conditions `when` et des priorités, et sont départagées par la résolution décrite plus haut. Le tableau sert de spécification et de base aux tests.

| Cas ambigu | Règle |
|---|---|
| `m` : minute ou mètre | **Minute** dans une durée qui contient aussi `h` ou `s` (`1h30m`, `5m30s`) ; **mètre** sinon (`5 m en ft`). `5m` tout seul = mètre, et on propose `5 min` en variante. |
| `in` : conversion, « dans », ou pouce (inch) | En début de saisie suivi d'une durée = temps relatif (`in 90 min`) ; juste après un nombre et suivi d'une unité ou d'une fin de saisie = pouce (`12 in en cm`) ; entre deux quantités = conversion (`5 km in miles`). |
| `an` : année ou article anglais | Après un nombre = année (`3 ans`, `1 an`). Sinon ignoré (`an hour` = `1 hour`). |
| `,` : virgule décimale ou séparateur de milliers | Réglage `decimal_separator` (`auto` par défaut, d'après la langue du système). `1,500.25` est sans ambiguïté (format anglais). En mode `auto`, si c'est ambigu (`1,500`), on suit la langue du système **et on propose l'autre lecture en variante**. |
| Espace dans un nombre : `1 500` | Toujours un séparateur de milliers s'il est suivi d'exactement 3 chiffres (`1 500` = `1500`, `2 500 000` = `2500000`), quelle que soit la langue. Espaces normales, insécables et fines acceptées. Sinon, ce n'est pas un nombre (`15 00` ne donne aucun résultat). |
| Date `01/02/2026` : 1er février ou 2 janvier | Réglage `date_format` (`auto` d'après la langue du système, ou `dmy` / `mdy` / `ymd`). Le format ISO `2026-02-01` et les dates en lettres (`1 feb`, `1er février`) sont toujours sans ambiguïté. Le résultat affiche **toujours le mois en lettres et le jour de la semaine**, pour que la lecture choisie se voie. |
| `à` sans accent (`a`) | Accepté comme « à / at » seulement entre une quantité et un débit (`4 Go a 100 Mbit/s`). |

#### Langue de l'interface et des résultats
- L'interface suit la **langue du système** : texte du champ vide, libellés, erreurs, menu de l'icône de notification, confirmations. On peut la forcer avec `ui_language = "system" | "fr" | "en"`. Pour toute autre langue, l'interface est en anglais. La langue de l'interface est **indépendante** des langues de saisie (`input_languages`).
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
└── assets/                   # polices ; icons/ contient le logo (flair.svg, flair-tiny.svg)
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
2. **Normalisation multilingue** : découper la saisie en jetons, puis la réécrire avec les **fichiers de langue actifs** et la résolution des ambiguïtés (section 2, « Langues ») en syntaxe que le moteur comprend, quelle que soit la langue utilisée. Exemples pour le français et l'anglais :
   - mots-clés : `en`, `vers`, `to`, `in`, `as` → conversion ; `de`, `of` dans les pourcentages → `of` ; `fois`, `times` → `*` ; `divisé par`, `divided by` → `/` ; `plus`, `moins`, `minus` ; `maintenant`, `now` ; `il y a`, `ago` ; `dans` / `in` en début de saisie ; `à`, `at` pour les débits… ;
   - virgule décimale : `1,5` → `1.5`, selon le réglage `decimal_separator` et les règles d'ambiguïté de la section 2. Si `,` est la virgule décimale, les arguments des fonctions se séparent par `;` (`;` est toujours accepté ; `,` l'est quand ce n'est pas ambigu) ;
   - séparateurs de milliers : `1 500`, `1 000 000`, `1_000_000`, `1'000'000` (format suisse) et `1,000,000` (format anglais) → `1000000`. Règle : un premier groupe de 1 à 3 chiffres, puis des groupes d'**exactement 3 chiffres**, ce qui ne casse pas `5 km`. Les espaces insécables (U+00A0 et l'espace fine U+202F, qu'on obtient en copiant un nombre depuis Word, Excel ou une page web en français) comptent comme des espaces. Ça se combine avec la virgule décimale : `1 500,75` = `1500.75`. Deux nombres séparés par un espace qui ne forment pas un groupement valide (`15 00`, `1 5000`) ne sont **pas** multipliés implicitement : pas de résultat ;
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
- Les résultats affichés sont vérifiés dans les deux langues d'interface (`ui_language = "fr"` et `"en"`).
- Le tableau est aussi exécuté avec **une seule langue active** (`input_languages = ["fr"]`, puis `["en"]`). Les cas de l'autre langue ne doivent alors plus être reconnus comme calculs, et les ambiguïtés qu'elle apportait disparaissent (avec `["fr"]` seul, `in` n'est plus jamais une conversion).
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
- Crate `tray-icon` avec un menu : Afficher, Réindexer, Ouvrir la config, Rechercher une mise à jour, Lancer au démarrage (case à cocher), Quitter.
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
Flair n'intègre que **deux thèmes**, un clair et un sombre. Tous les autres sont créés par l'utilisateur (voir « Thèmes personnalisés »). Leurs contrastes ont été vérifiés : texte principal ≥ 15.2:1, accent ≥ 5.8:1 sur le fond et ≥ 5:1 sur la sélection, texte secondaire ≥ 5.2:1, texte discret ≥ 3.3:1.

| Nom | Type | `background` | `foreground` | `accent` |
|---|---|---|---|---|
| **Flair clair** *(défaut clair)* | clair | `#FAFAF9` | `#1C1B1A` | `#B03A0A` |
| **Flair sombre** *(défaut sombre)* | sombre | `#161615` | `#ECEBE8` | `#FB923C` |

Ils ont des tons neutres, très légèrement chauds, avec un accent orange braise (le « flair », l'étincelle).

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
- Avec seulement 2 couleurs (pas d'`accent`), l'accent est égal au texte : les lettres trouvées se distinguent alors seulement par le semi-gras.
- Un fichier invalide est ignoré avec un message clair. Il ne casse jamais le thème actif.

**Directement depuis Flair**, dans l'esprit d'un launcher :
- `> thème` ou `> theme` liste tous les thèmes. **Naviguer dans la liste applique le thème en aperçu en direct** ; `Entrée` le garde, `Échap` revient au thème précédent.
- `> thème 1E1B2E E6E1F5 B79CFF lavande` crée un thème à partir de 3 couleurs (fond, texte, accent, dans cet ordre) et l'affiche **en aperçu immédiatement**, avec un petit échantillon des 3 couleurs et l'indication « contraste ajusté » si besoin. `Entrée` enregistre le fichier `themes/lavande.toml` et l'applique. Si on ne donne pas de nom, il s'appelle `perso-1`, `perso-2`…
- Le nouveau thème va automatiquement à l'emplacement `light` ou `dark` selon son fond.

### Suivi du thème système
- En mode `system`, Flair bascule entre ses thèmes `light` et `dark` **en direct** quand le système change, sans redémarrage.
- Windows : valeur de registre `AppsUseLightTheme`, surveillée. Linux : portail XDG `org.freedesktop.appearance` / `color-scheme` (via `ashpd`), avec la crate `dark-light` en secours.

### Icône de Flair
Le logo évoque un **reflet d'objectif** (lens flare) : deux anneaux concentriques et un petit cercle qui passe devant eux, en haut à gauche. Le dessin est **au trait, sur fond transparent**. Le petit cercle *masque* les anneaux : il ne les recouvre pas de blanc, donc l'icône marche sur n'importe quel fond.

Les fichiers existent déjà dans `assets/icons/`. Ce sont des SVG en **une seule couleur**, `currentColor`, pour pouvoir les teinter avec une couleur du thème :

| Fichier | Taille d'affichage | Détail |
|---|---|---|
| `flair.svg` | 20 px et plus | Version principale, traits épais (lisible de 20 à 256 px) |
| `flair-tiny.svg` | 16 px | Simplifiée : un seul anneau et le petit cercle (à 16 px, trois cercles se mélangent) |

Ne pas modifier le dessin. Utiliser toujours la version adaptée à la taille.

**Couleurs** (toujours une seule couleur, fidèle à la règle des 3 couleurs) :
- **Dans l'interface** : la loupe `⌕` des maquettes **est l'icône Flair** (`flair.svg`, 20 px), en `texte discret`. Elle passe en `accent` quand le champ contient du texte.
- **Icône de notification** : couleur `foreground` du thème Flair clair (`#1C1B1A`) si la barre des tâches est claire, celle du thème Flair sombre (`#ECEBE8`) si elle est sombre. On suit le thème de la barre des tâches : `SystemUsesLightTheme` sous Windows (différent de `AppsUseLightTheme`), le thème du système sous Linux. Réglage `theme.tray_icon = "auto" | "accent"` (`accent` = couleur d'accent du thème actif).
- **Icône de l'application** (`.exe`, menu Démarrer, `.desktop`, fichier `.ico`) : couleur fixe **orange braise `#E06A28`**. Elle a été choisie parce qu'elle reste lisible sur fond blanc (3.35:1) comme sur une barre des tâches sombre (4.86:1), au-dessus du minimum de 3:1 recommandé pour les éléments graphiques.

**Génération des formats** (dans `build.rs` ou un petit outil `xtask`, jamais à la main) :
- Remplacer `currentColor` par la couleur voulue dans le texte du SVG, puis faire le rendu avec **`resvg`**.
- **Windows** : `flair.ico` avec les tailles 16 (`flair-tiny.svg`), puis 20, 24, 32, 48, 64, 128 et 256 (`flair.svg`), crate `ico`. L'icône est intégrée à l'exécutable avec **`winresource`**.
- **Linux** : `flair.svg` installé comme icône vectorielle (`hicolor/scalable/apps/flair.svg`, avec la couleur fixe), plus des PNG 16 à 256 px. Le `.desktop` utilise `Icon=flair`.
- **Icône de notification** : rendue à l'exécution depuis le SVG (`resvg` → RGBA → `tray-icon`) dans la bonne couleur, et re-rendue quand le thème de la barre des tâches change.

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
config_version = 1          # géré par Flair pour les migrations, ne pas modifier
hotkey = "Alt+Space"
ui_language = "system"      # system | fr | en — langue de l'interface et des résultats
input_languages = ["fr", "en"]  # langues de saisie actives, dans l'ordre de préférence (voir « > langue »)
decimal_separator = "auto"  # auto | "," | "."
date_format = "auto"        # auto | dmy | mdy | ymd
max_results = 8             # 1 à 20
keep_last_query = false
autostart = false           # lancer Flair au démarrage de la session
log_level = "info"          # error | warn | info | debug | trace (FLAIR_LOG est prioritaire)

[calc]
angle_unit = "deg"          # deg | rad
significant_digits = 10     # chiffres significatifs affichés
binary_prefixes = false     # true : Ko/Mo/Go = 1024 (comme l'Explorateur Windows), false : 1000 (SI)
currency = false            # conversions de devises (taux téléchargés et mis en cache)
history_size = 50           # nombre de calculs gardés dans l'historique

[theme]
mode = "system"             # system | light | dark
light = "Flair clair"       # thème utilisé en mode clair
dark = "Flair sombre"       # thème utilisé en mode sombre
opacity = 1.0               # 0.85 à 1.0 : transparence du fond, si la plateforme le permet
icons = "color"             # color | mono (icônes d'applications en couleur ou monochromes)
font = "inter"              # inter | system | nom d'une police installée
tray_icon = "auto"          # auto (suit la barre des tâches) | accent
animations = true

[files]
enabled = true
roots = ["~"]
exclude = ["node_modules", ".git", "target", ".cache", "__pycache__"]
include_hidden = false
other_drives = false        # Windows : indexer aussi les autres disques (D:\, E:\…)

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

[update]
channel = "main"            # main (chaque push sur main) | stable (versions taguées vX.Y.Z)
auto_check = true           # vérifier au plus une fois par jour, seulement quand on ouvre Flair
```

- `config_version` est le seul champ absent du registre des réglages et des commandes `>` : il est géré par Flair (voir les migrations, section 11).

- Les moteurs de recherche web s'ajoutent avec des blocs `[[web]]` (voir 5.4).
- Données (index, frecency, cache d'icônes) : `%LocalAppData%\Flair\` sous Windows, `~/.local/share/flair/` et `~/.cache/flair/` sous Linux.

### Tout configurer depuis Flair avec des commandes `>`

**Règle : tout ce qui est réglable dans le fichier de config l'est aussi depuis Flair**, avec une commande `>`, sans jamais avoir à ouvrir le fichier. Toutes les mentions « configurable », « réglable » ou « option » de ce document sont concernées. Le fichier TOML reste disponible pour ceux qui préfèrent.

#### Un registre unique des réglages
- Chaque réglage est décrit **une seule fois**, dans un registre (`SettingDescriptor`) qui contient :
  - la clé TOML (`theme.opacity`) ;
  - le type (booléen, nombre avec bornes, texte, choix dans une liste, liste de valeurs, raccourci clavier, couleur, chemin) ;
  - la valeur par défaut ;
  - les valeurs permises ;
  - un **nom et une description en français et en anglais** ;
  - des **alias** dans chaque langue (`transparence`, `opacity`) ;
  - s'il faut redémarrer pour l'appliquer.
- Depuis ce registre, on génère : la commande générique `> config`, l'aide `> help`, la validation des valeurs, le fichier de config par défaut avec ses commentaires, et la doc des réglages dans le `README`. **Ajouter un réglage = ajouter une entrée au registre**, rien d'autre.
- Un test vérifie que **chaque champ de la structure de config a son entrée dans le registre**, et inversement, avec une description dans les deux langues. Un réglage ne peut donc pas être oublié.

#### La commande générique `> config`
| Commande | Effet |
|---|---|
| `> config` / `> réglages` / `> settings` | Liste tous les réglages avec leur valeur actuelle. On filtre en tapant, par recherche floue sur la clé, le nom et les alias **dans les deux langues** (`> config transp` trouve `theme.opacity`). |
| `> config <réglage> <valeur>` | Change directement la valeur : `> config max_results 10`, `> config transparence 0.9`, `> config calc.angle_unit rad` |
| `> config <réglage> défaut` / `reset` | Remet la valeur par défaut |
| `> config ouvrir` / `open` | Ouvre le fichier de config dans l'éditeur par défaut |
| `> config recharger` / `reload` | Relit le fichier de config |
| `> annuler` / `> undo` | Annule le dernier changement de réglage fait par commande |

Comportement dans la liste `> config` :
- **Booléen** : `Entrée` bascule entre activé et désactivé.
- **Choix dans une liste** : `Entrée` affiche les valeurs possibles comme résultats ; on choisit avec les flèches.
- **Nombre, texte, couleur, chemin** : `Entrée` remplit le champ avec `> config <réglage> <valeur actuelle>`, prêt à être modifié.
- **Liste** (`files.roots`, `files.exclude`, `input_languages`…) : sous-commandes `ajouter` / `add` et `retirer` / `remove`. Exemple : `> config files.exclude ajouter .venv`.
- Le sous-titre de chaque réglage indique sa valeur actuelle, sa valeur par défaut si elle est différente, et les valeurs permises.

#### Raccourcis pour les réglages les plus courants
Chacun est un simple alias vers le registre, sans logique dupliquée :

| Commande (FR / EN) | Exemples |
|---|---|
| `> thème` / `> theme` | voir section 7 (aperçu en direct, création à partir de 3 couleurs) |
| `> langue` / `> language` | voir section 2 (activer, désactiver, préférer) |
| `> raccourci` / `> hotkey` | `> raccourci Ctrl+Espace` |
| `> fichiers` / `> files` | `> fichiers ajouter D:\Projets`, `> fichiers exclure .venv`, `> fichiers cachés on` |
| `> web` | `> web` liste les moteurs ; `> web ajouter yt YouTube https://www.youtube.com/results?search_query={query}` ; `> web retirer yt` |
| `> calcul` / `> calc` | `> calc angle rad`, `> calc chiffres 15`, `> calc devises on`, `> calc octets binaire` |
| `> démarrage` / `> autostart` | `> démarrage on` / `off` |
| `> transparence` / `> opacity` | `> transparence 0.9` |
| `> animations` | `> animations off` |
| `> rendu` / `> renderer` | `> rendu cpu` |
| `> batterie` / `> battery` | `> batterie pause on` |

`on` / `off`, `oui` / `non`, `yes` / `no`, `activer` / `désactiver`, `enable` / `disable` et `true` / `false` sont tous acceptés pour les booléens.

#### Actions de Flair
| Commande (FR / EN) | Effet |
|---|---|
| `> réindexer` / `> reindex` | Relance l'indexation des fichiers |
| `> stats` | Statistiques (voir section 3) |
| `> historique effacer` / `> history clear` | Efface l'historique des calculs et les données d'usage (frecency), avec confirmation |
| `> logs` | Ouvre le dossier des logs |
| `> exporter` / `> export` | Sauvegarde tout dans **un seul fichier** `.flair`, à l'endroit choisi (voir « Exporter et importer ») |
| `> importer` / `> import` | Restaure un fichier `.flair` choisi dans l'explorateur de fichiers |
| `> quitter` / `> quit` | Ferme complètement Flair |
| Commandes système | voir 5.5 (`> verrouiller`, `> éteindre`…) |

#### Règles communes à toutes les commandes `>`
- **Aperçu en direct** pour tout ce qui se voit (thème, transparence, police, icônes, nombre de résultats, animations) : la valeur s'applique pendant qu'on la tape ou qu'on navigue, `Entrée` la garde, `Échap` revient à l'ancienne.
- **Validation avant écriture** avec les mêmes règles que le fichier TOML. Une valeur invalide affiche un message clair dans la ligne de résultat (« doit être entre 0.85 et 1.0 »), en couleur d'accent (pas de rouge), et **rien n'est écrit**.
- **Application immédiate** et **écriture dans le fichier de config** avec `toml_edit`, en gardant les commentaires et la mise en forme. Si un réglage demande un redémarrage, l'indiquer (« appliqué au prochain démarrage ») et proposer `> redémarrer flair` / `> restart flair`. Viser un minimum de réglages dans ce cas.
- **Raccourci global** : on enregistre d'abord le nouveau raccourci. S'il est refusé (déjà pris par une autre app, ou Wayland), on garde l'ancien et on explique pourquoi. On ne se retrouve jamais sans raccourci.
- **Autocomplétion** : en tapant `> con`, `> config the`, `> config theme.mode `, les résultats proposent les commandes, les réglages puis les valeurs possibles. `Tab` complète.
- Tous les mots des commandes (noms, sous-commandes, valeurs comme `on` / `activer`) viennent des **fichiers de langue** : ils suivent les langues actives, sauf `> help` / `> aide`, `> config` et `> langue` / `> language`, qui sont dans `common.toml` et restent toujours disponibles.

#### `> help` / `> aide`
- `>` seul affiche les commandes les plus utilisées, et `> help` en dernière ligne.
- `> help` liste **toutes** les commandes, regroupées par catégorie : Réglages, Apparence, Langues, Recherche et fichiers, Web, Calcul, Système, Flair. Chaque ligne affiche la commande, une description courte et un exemple, dans la langue de l'interface.
- `> help <mot>` filtre par recherche floue (`> help fich`, `> help transparence`). Sélectionner une commande et appuyer sur `Entrée` affiche son détail (syntaxe, valeurs possibles, exemples). `Tab` insère la commande dans le champ, prête à l'emploi.
- Le contenu de l'aide est **généré automatiquement** à partir du registre des réglages et de la liste des commandes. Il ne peut donc pas être en retard sur le code. Un test vérifie que chaque commande a une description et au moins un exemple, en français et en anglais.

### Exporter et importer : un seul fichier `.flair`

Tout ce que l'utilisateur a personnalisé tient dans **un seul fichier** `.flair`. Il sert à sauvegarder, à passer d'un PC à un autre, et à partager ses réglages.

#### Le fichier `.flair`
C'est une archive ZIP (crate `zip`) avec une extension propre à Flair. Elle ne contient que des fichiers texte lisibles :
```
flair-export-2026-11-26.flair
├── manifest.json      # format, version de Flair, config_version, date, nom du PC, OS, contenu
├── config.toml        # tous les réglages (moteurs web compris)
├── themes/*.toml      # thèmes personnalisés
├── lexicon/*.toml     # fichiers de langue personnels
└── data/
    ├── history.json   # historique des calculs
    └── frecency.json  # données d'usage (ce qu'on lance souvent)
```
- **Jamais inclus**, car ça se reconstruit ou ça ne sert à rien ailleurs : index des fichiers, cache d'icônes, logs, fichiers de mise à jour.
- `manifest.json` contient un numéro de format (`"format": 1`) pour pouvoir faire évoluer le fichier sans casser les anciens exports.

#### `> exporter` / `> export`
1. Flair propose deux lignes :
   - **Tout** : réglages, thèmes, langues, historique et données d'usage ;
   - **Réglages seulement** : sans historique ni données d'usage. C'est le choix à faire pour partager ses réglages : les données d'usage révèlent des noms de fichiers et d'applications.
2. `Entrée` ouvre la **fenêtre native « Enregistrer sous »** du système (crate `rfd` : boîte de dialogue Windows, portail XDG sous Linux, donc compatible Wayland). Nom proposé : `flair-export-<date>.flair`, dans le dossier Documents.
3. Flair confirme dans sa fenêtre : « Exporté : D:\Sauvegardes\flair-export-2026-11-26.flair (12 Ko) ». `Entrée` ouvre le dossier.

#### `> importer` / `> import`
1. Flair affiche :
   - **« Choisir un fichier… »**, qui ouvre la **fenêtre native « Ouvrir »** filtrée sur les fichiers `*.flair` ;
   - en dessous, les **sauvegardes automatiques** récentes (faites avant une mise à jour, une réinitialisation ou un import), pour les restaurer en un `Entrée`.
2. Une fois le fichier choisi, Flair affiche un **résumé avant de toucher à quoi que ce soit** : « Exporté le 26/11/2026 depuis Flair 0.4.0 sur PC-BUREAU (Windows) : réglages, 3 thèmes, 1 lexique, historique (50 calculs) ». Il signale aussi ce qui ne s'appliquera pas tel quel, par exemple des dossiers de `files.roots` qui n'existent pas sur ce PC.
3. Deux choix :
   - **Tout remplacer** : la config, les thèmes, les lexiques et les données deviennent ceux du fichier ;
   - **Ajouter les thèmes et lexiques seulement** : les réglages actuels ne changent pas. Si un nom existe déjà, l'élément importé est renommé (`lavande-2`).
4. **Avant d'importer, Flair fait automatiquement une sauvegarde `.flair` de l'état actuel** dans `backups/`. `> annuler` / `> undo` juste après un import revient à l'état d'avant.
5. Tout s'applique immédiatement, sans redémarrage, sauf exception signalée (voir les réglages qui demandent un redémarrage).

#### Compatibilité entre versions
- Fichier exporté par une **version plus ancienne** : la config passe par les **mêmes migrations** que lors d'une mise à jour (section 11).
- Fichier exporté par une **version plus récente** que celle installée : import refusé, avec « Ce fichier vient d'une version plus récente de Flair. Faites d'abord > update. »
- Un fichier de Windows s'importe sous Linux et inversement. Les chemins utilisent `~` pour le dossier personnel quand c'est possible, pour rester valables d'un PC à l'autre.

#### Sécurité : un fichier `.flair` n'est pas fiable par défaut
Il peut venir de n'importe où :
- on n'extrait **que les chemins attendus** (liste fixe ci-dessus) ; tout chemin avec `..`, absolu, ou inconnu est ignoré (protection contre le « zip slip ») ;
- limites de taille : 10 Mo par fichier et 50 Mo au total une fois décompressé, sinon refus (protection contre les « bombes ZIP ») ;
- chaque fichier est validé avec les mêmes règles que s'il était écrit à la main (TOML de config, thèmes, lexiques). Un élément invalide est ignoré et signalé, il ne bloque pas le reste ;
- un `.flair` ne contient **jamais** de code ni d'exécutable.

#### Détails
- **Fenêtres de dialogue** : Flair se cache quand il perd le focus. L'ouverture d'une boîte de dialogue ne doit **ni** annuler l'opération **ni** la perdre : Flair se met en attente, la boîte s'affiche au premier plan, puis Flair revient avec le résultat. La boîte ne bloque jamais le thread de l'interface.
- **Ligne de commande**, pour les scripts : `flair --export <chemin> [--settings-only]` et `flair --import <chemin> [--merge]`.
- **Double-clic** sur un `.flair` : à l'installation (section 11), Flair associe l'extension `.flair` (`HKCU\Software\Classes\.flair` sous Windows, type MIME `application/x-flair` sous Linux). Un double-clic ouvre Flair directement sur le résumé d'import.
- **Sauvegardes automatiques** : celles faites avant une mise à jour, une réinitialisation ou un import utilisent **le même format `.flair`**, dans `backups/` du dossier de données. On en garde 10 ; les plus anciennes sont supprimées.
- Tests :
  - export puis import donne exactement le même état ;
  - export Windows importé sous Linux ;
  - fichier venant d'une version plus ancienne (migration) et plus récente (refus) ;
  - archive piégée (`../`, chemin absolu, bombe ZIP) refusée sans rien écrire ;
  - « Réglages seulement » ne contient ni historique ni frecency.

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

## 11. Distribution et mises à jour

Objectif : l'utilisateur télécharge **un seul `flair.exe`**. Ensuite, chaque push sur la branche `main` du dépôt GitHub produit une nouvelle version, que `> update` installe **sans réinstaller l'app et sans perdre les réglages**. Même un gros changement doit passer par `> update`, grâce aux migrations.

### Un seul exécutable, qui s'installe lui-même
- **Windows** : un seul fichier `flair.exe`, avec tout intégré (polices, icônes, lexiques, traductions). Aucune DLL ni Visual C++ Redistributable à installer : CRT liée statiquement (`-C target-feature=+crt-static` pour la cible MSVC, dans `.cargo/config.toml`).
- **Installation au premier lancement** : si `flair.exe` n'est pas lancé depuis son dossier d'installation, Flair affiche un écran minimal (dans le style de l'app) qui propose de s'installer :
  - copie dans `%LocalAppData%\Programs\Flair\flair.exe`, **sans droits administrateur** ;
  - raccourci dans le menu Démarrer (crate `mslnk`) ;
  - lancement au démarrage (case cochée par défaut) ;
  - entrée dans « Applications installées » de Windows (clé `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Flair`), pour pouvoir désinstaller proprement ;
  - association de l'extension `.flair` (double-clic sur un export = import, voir section 8).

  Un choix « Utiliser sans installer » lance Flair là où il est (mode portable). Les mises à jour y marchent aussi, si le dossier est accessible en écriture.
- **Désinstallation** : `> désinstaller` / `> uninstall`, ou depuis les Paramètres Windows. On retire le raccourci, le lancement au démarrage, l'entrée de registre et l'exécutable. Un exe en cours d'exécution ne peut pas se supprimer lui-même : on passe par un petit processus détaché qui attend la fin de Flair, ou par `MoveFileExW` avec `MOVEFILE_DELAY_UNTIL_REBOOT`. Flair demande si on garde la config et les données.
- **Linux** : un seul binaire `flair`, installé de la même façon dans `~/.local/bin/`, avec le `.desktop` et l'icône dans `~/.local/share/`. Si Flair a été installé par un gestionnaire de paquets (chemin sous `/usr`), la mise à jour intégrée est désactivée et `> update` l'explique.
- **SmartScreen** : l'exe n'étant pas signé, Windows affiche « Windows a protégé votre ordinateur » au **premier** lancement d'un exe téléchargé avec un navigateur (« Informations complémentaires » → « Exécuter quand même »). À documenter dans le `README`. Les mises à jour téléchargées par Flair lui-même ne déclenchent pas cet avertissement.

### Publication automatique à chaque push sur `main`
Workflow GitHub Actions `.github/workflows/release.yml` :
- **Déclencheurs** : push sur `main` (canal `main`) et tags `v*` (canal `stable`).
- **Compilation native** : `windows-latest` pour Windows (pas besoin de `cargo-xwin` en CI) et `ubuntu-latest` pour Linux. Les tests tournent avant : **s'ils échouent, aucune version n'est publiée**.
- **Numéro de version** : la `version` de `Cargo.toml`, plus le numéro de build pour le canal `main`. Exemples : `0.4.0-main.57` (57 = `github.run_number`) pour un push sur `main`, `0.4.0` pour un tag `v0.4.0`. La version, le commit et la date sont intégrés au binaire à la compilation (`build.rs` ou crate `vergen`), et affichés par `flair --version` et `> version`.
- **Release GitHub** (pré-release pour le canal `main`) avec : `flair-windows-x86_64.exe`, `flair-linux-x86_64`, leurs signatures et `manifest.json`. Les notes de version sont générées à partir des messages de commit depuis la version précédente.
- **Nettoyage** : ne garder que les 20 dernières pré-releases `main`.

```json
{
  "schema": 1,
  "version": "0.4.0-main.57",
  "channel": "main",
  "commit": "a1b2c3d",
  "date": "2026-12-03T14:12:00Z",
  "notes": ["Calcul : fuseaux horaires", "Thème sombre : contraste de la sélection"],
  "min_updater_version": "0.2.0",
  "config_version": 3,
  "requires_reset": false,
  "assets": {
    "windows-x86_64": { "file": "flair-windows-x86_64.exe", "sha256": "…", "signature": "flair-windows-x86_64.exe.minisig" },
    "linux-x86_64":   { "file": "flair-linux-x86_64",       "sha256": "…", "signature": "flair-linux-x86_64.minisig" }
  }
}
```

### Sécurité des mises à jour (non négociable)
- Chaque binaire est **signé en CI** avec une clé Ed25519 au format minisign. La clé privée est **uniquement** dans un secret GitHub (`FLAIR_SIGNING_KEY`). La **clé publique est intégrée** au binaire de Flair.
- Flair vérifie la **signature** (crate `minisign-verify`) **et** le **SHA-256** **avant** de toucher à quoi que ce soit. En cas d'échec, la mise à jour est refusée, rien n'est modifié, et un message clair s'affiche.
- HTTPS uniquement (`rustls`). L'adresse du dépôt est fixée **à la compilation** (`FLAIR_UPDATE_REPO = "<propriétaire>/flair"`) et n'est **pas** modifiable par la config, pour qu'une config modifiée ne puisse pas rediriger les mises à jour.
- Jamais de retour à une version plus ancienne, sauf `> update annuler` explicite.
- Les releases doivent être **publiques** pour être téléchargées sans jeton. Si le dépôt du code est privé, publier les releases dans un second dépôt public.

### La commande `> update` / `> mise à jour`
| Commande (FR / EN) | Effet |
|---|---|
| `> mise à jour` / `> update` | Cherche la dernière version du canal. Affiche « Flair 0.4.0-main.57 disponible (actuelle : 0.4.0-main.52) » avec les notes de version en lignes secondaires ; `Entrée` l'installe. Si Flair est à jour : « Flair est à jour (0.4.0-main.57) ». |
| `> mise à jour annuler` / `> update rollback` | Revient à la version précédente (conservée sur le disque) |
| `> version` | Version, commit, date de compilation et canal |
| `> mise à jour canal stable` / `> update channel main` | Change de canal (raccourci vers le réglage `update.channel`) |

Déroulement après `Entrée` :
1. **Téléchargement** dans un fichier temporaire du dossier de données (`updates/`), avec la progression dans la ligne de résultat (`42 %`). L'interface reste fluide. `Échap` cache la fenêtre mais le téléchargement continue ; l'infobulle de l'icône de notification indique la progression.
2. **Vérification** de la signature et du SHA-256.
3. **Remplacement** : sous Windows, un exe en cours d'exécution ne peut pas être écrasé mais peut être renommé. `flair.exe` devient `flair.previous.exe`, puis la nouvelle version prend sa place (crate `self-replace`). Sous Linux, renommage atomique. Un téléchargement interrompu ne laisse **jamais** un exe cassé : rien n'est remplacé tant que le fichier complet n'est pas vérifié.
4. **Redémarrage** : l'ancienne instance lance `flair.exe --after-update --wait-pid <pid>` puis se ferme. La nouvelle attend la fin de l'ancienne (pour récupérer le verrou d'instance unique), applique les migrations, puis affiche brièvement « Flair mis à jour vers 0.4.0-main.57 ».
5. **Vérification de santé** : la nouvelle version écrit un marqueur « démarrage réussi » une fois la fenêtre prête et le raccourci enregistré. Si elle plante avant **deux fois de suite**, le lancement suivant restaure automatiquement `flair.previous.exe` et affiche « La mise à jour a échoué, retour à la version X ». Cette logique est la toute première chose exécutée dans `main`, avant tout le reste.

Config, thèmes, lexiques personnels, historique et index sont **conservés**.

**Vérification automatique**, compatible avec la sobriété de la section 3 : avec `update.auto_check = true`, Flair vérifie **au plus une fois toutes les 24 h**, et **seulement au moment où l'utilisateur ouvre Flair**. Jamais de minuteur, jamais de réveil en arrière-plan. Si une version est disponible, une seule ligne discrète s'affiche dans la liste des commandes `>` (« Mise à jour disponible : > update »). Flair n'installe **jamais** rien sans qu'on le lui demande.

### Gros changements : des migrations plutôt qu'une réinstallation
Le but : ne **jamais** avoir à réinstaller Flair.
- **Config** : le champ `config_version` donne la version du format. Au démarrage, si la config est plus ancienne, Flair applique les migrations en chaîne (v1 → v2 → v3), chacune écrite dans le code et testée. Elles passent par `toml_edit` pour garder les commentaires. Avant, une sauvegarde automatique `.flair` (même format que `> export`, section 8) est faite dans `backups/`.
- **Données** (index, frecency, cache d'icônes, historique) : chaque fichier commence par un numéro de format. S'il n'est plus compatible, la donnée est **reconstruite automatiquement** (réindexation en arrière-plan, cache vidé) sans action de l'utilisateur. La frecency et l'historique sont migrés si possible, sinon remis à zéro avec un message.
- **`requires_reset = true`** dans le manifest (rare, pour un changement énorme) : avant d'installer, `> update` prévient « Cette mise à jour réinitialise les réglages. Une sauvegarde sera faite. » Après confirmation, Flair crée une sauvegarde complète `.flair` dans `backups/` (et propose de l'enregistrer aussi ailleurs avec la boîte « Enregistrer sous »), réinitialise, puis installe la mise à jour. On peut ensuite récupérer ses thèmes et lexiques avec `> importer` → « Ajouter les thèmes et lexiques seulement ». **C'est toujours une mise à jour, pas une réinstallation.**
- **`min_updater_version`** : si la version installée est trop ancienne pour installer la nouvelle, Flair ne tente rien. Il affiche « Cette version doit être téléchargée manuellement », avec un lien vers la page des releases. C'est le **seul** cas qui demande de retélécharger l'exe, et il doit rester exceptionnel :
  - le manifest est lu de façon tolérante (champs inconnus ignorés, valeurs par défaut) ;
  - un test en CI vérifie que le module de mise à jour de la **dernière version publiée** sait lire le nouveau manifest.

### Tests
L'adresse du serveur de mises à jour est un paramètre interne. Les tests d'intégration utilisent un faux serveur HTTP local (`wiremock` ou `httpmock`) et couvrent :
- version à jour ;
- version disponible ;
- signature invalide refusée ;
- SHA-256 invalide refusé ;
- téléchargement interrompu (rien n'est remplacé) ;
- migration de config v1 → v3 ;
- `requires_reset` ;
- retour automatique après deux démarrages ratés ;
- `min_updater_version`.

---

## 12. Jalons

Chaque jalon se termine par : compilation OK sur les deux cibles, clippy propre, tests verts, `README` à jour, et un court résumé de ce qui a été fait et de ce qui reste.

1. **Squelette** : workspace, crates, trait `Provider`, config TOML **décrite par le registre des réglages** (section 8, avec le test de complétude), logs, traductions de l'interface FR/EN, **système de thèmes** (3 couleurs → jetons, ajustement du contraste, thèmes intégrés avec leurs tests), fenêtre Iced fidèle aux maquettes et dimensions de la section 7 avec des résultats factices, navigation au clavier.
2. **Afficher et cacher** : app résidente, icône de notification, raccourci global (Windows et X11), instance unique et `--toggle` via IPC, se cache quand elle perd le focus. Dès ce jalon : **aucun réveil quand la fenêtre est cachée**, compteur de réveils et première version de `flair --stats`.
3. **Distribution et mises à jour** (section 11) : `flair.exe` autonome, installation au premier lancement et désinstallation, workflow `release.yml` qui publie une version signée à chaque push sur `main`, `manifest.json`, mise à jour par `flair --update` et `> update` (première commande `>` du projet, intégrée ensuite au système de commandes du jalon 7), vérification de la signature, remplacement de l'exe, redémarrage, retour arrière automatique, migrations de config versionnées, **export / import `.flair`** (`> export`, `> import`, boîtes de dialogue natives, sauvegardes automatiques, `--export` / `--import`), tests avec un faux serveur. **À partir de ce jalon, chaque jalon suivant s'installe avec `> update`.**
4. **Applications et calcul de base** : provider d'applications Windows et Linux avec icônes ; moteur de calcul (pipeline complet, fichiers de langue `common` / `fr` / `en` avec leurs conditions, résolution des ambiguïtés par évaluation, `flair lexicon check`, `fend-core`, nombres, fonctions, octets, unités, variantes) avec son tableau de tests ; classement avec `nucleo` et frecency.
5. **Calculs avancés** : durées et heures de la journée, dates, horodatages Unix, bases et opérations bit à bit, temps de transfert, devises avec cache, historique des calculs, limites de robustesse.
6. **Fichiers** : index parallèle **compact et ouvert depuis le disque** (`memmap2` + `rkyv`), indexation en priorité arrière-plan, mise à jour avec `notify` regroupée, exclusions par défaut, mode batterie, complétion de chemins, action « ouvrir le dossier ».
7. **Web et système** : mots-clés de recherche web, commandes système avec confirmation, préfixes de filtrage, **système de commandes `>`** : `> config` générique (aperçu en direct, validation, écriture avec `toml_edit`, `> annuler`), raccourcis de réglages, `> langue`, actions de Flair, autocomplétion, et `> help` généré automatiquement.
8. **Wayland et finitions** : portail GlobalShortcuts, layer-shell quand il est disponible, suivi du thème système en direct, thèmes personnalisés (fichiers, `> thème` avec aperçu en direct, création à partir de 3 couleurs), lancement au démarrage, rechargement à chaud de la config, **libération de la mémoire quand l'app est cachée** (EcoQoS, `EmptyWorkingSet`, `malloc_trim`), comparaison et choix du moteur de rendu, benchmarks, CI.

---

## 13. Critères d'acceptation globaux

- [ ] `Alt+Espace` affiche la barre en moins de 50 ms sous Windows 11 ; un second appui la cache.
- [ ] Taper `vsc` puis `Entrée` lance Visual Studio Code s'il est installé.
- [ ] Taper `= 15% de 240` affiche `36` ; `Entrée` le copie dans le presse-papier.
- [ ] Taper `5 km en miles` affiche environ `3.107 miles`.
- [ ] `1e524 * 1.5` affiche `1.5e524` (aucun dépassement, aucune perte de précision).
- [ ] `5h36 + 65min` affiche `6h41` ; `14h30 + 2h45` affiche `17h15`.
- [ ] `4 Go à 100 Mbit/s` affiche `5 min 20 s` ; `500 Go en Gio` affiche environ `465.66 Gio`.
- [ ] `26/11/2026 + 45 jours` affiche `10/01/2027`.
- [ ] `255 en hex` affiche `0xff` ; `1,5 * 2` affiche `3` (virgule décimale française).
- [ ] `1 500 + 1 500` affiche `3000` ; `1 500,75 * 2` affiche `3001,5` ; un nombre copié depuis Excel avec une espace insécable (`1 500`) marche aussi.
- [ ] `1000000!` ou `10^10^10^10` répond « trop grand » en moins de 100 ms, sans bloquer l'interface.
- [ ] Le tableau de tests de calcul (au moins 150 cas) passe entièrement.
- [ ] `now + 3h` donne le même résultat que `maintenant + 3h` ; `4 GB at 100 Mbit/s` le même que `4 Go à 100 Mbit/s` ; `4d` le même que `4j`.
- [ ] Les saisies mélangées fonctionnent : `now + 3 jours`, `4 Go at 100 Mbit/s`.
- [ ] `> lock` et `> verrouiller` verrouillent tous les deux la session.
- [ ] Avec `ui_language = "en"`, l'interface et les résultats sont en anglais (`January 10, 2027 (Sunday)`) ; avec `"fr"`, en français (`10 janvier 2027 (dimanche)`).
- [ ] `> language fr disable` désactive le français immédiatement et l'écrit dans la config ; `maintenant + 3h` n'est plus reconnu, `now + 3h` l'est toujours ; `> langue fr activer` le réactive.
- [ ] Désactiver la dernière langue active est refusé avec un message clair.
- [ ] `5 km in miles` donne la conversion, et non une erreur avec « in » lu comme pouce : l'évaluation élimine les lectures invalides.
- [ ] `5m` affiche `5 m` (mètres) avec la variante « lu comme : 5 minutes ».
- [ ] `flair lexicon check` ne signale aucun conflit non résolu pour les langues intégrées.
- [ ] Chaque réglage du fichier de config est modifiable par une commande `>` (vérifié par le test de complétude du registre).
- [ ] `> config transparence 0.9` applique la transparence immédiatement et l'écrit dans `config.toml` sans effacer les commentaires ; `> annuler` revient à la valeur précédente.
- [ ] `> config max_results 99` est refusé avec un message clair, et rien n'est écrit.
- [ ] `> raccourci` vers une combinaison déjà prise garde l'ancien raccourci et explique pourquoi.
- [ ] `> help` liste toutes les commandes par catégorie, dans la langue de l'interface ; `> help fich` ne montre que les commandes liées aux fichiers.
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
- [ ] `flair.exe`, téléchargé depuis la page des releases, se lance sur un Windows neuf sans rien installer d'autre, et propose de s'installer dans `%LocalAppData%\Programs\Flair` sans droits administrateur.
- [ ] Un push sur `main` dont les tests passent publie automatiquement une version `X.Y.Z-main.N` signée, avec son `manifest.json`. Si les tests échouent, rien n'est publié.
- [ ] `> update` installe la nouvelle version sans réinstallation : Flair redémarre en quelques secondes, avec la config, les thèmes, l'historique et l'index conservés.
- [ ] Un binaire ou une signature modifiés sont refusés, et rien n'est remplacé.
- [ ] Une version qui plante au démarrage est automatiquement remplacée par la précédente.
- [ ] Une config d'une ancienne version est migrée automatiquement, avec une sauvegarde.
- [ ] Les mises à jour ne réveillent jamais Flair quand il est caché : la vérification n'a lieu qu'à l'ouverture, au plus une fois par 24 h.
- [ ] `> export` ouvre la boîte native « Enregistrer sous » et crée **un seul** fichier `.flair` ; Flair ne se cache pas et ne perd pas l'opération pendant que la boîte est ouverte.
- [ ] `> import` ouvre la boîte native « Ouvrir », affiche un résumé, et n'applique rien avant confirmation ; `> annuler` juste après revient à l'état d'avant.
- [ ] Un export fait sous Windows s'importe sous Linux, et un export d'une ancienne version est migré automatiquement.
- [ ] Un `.flair` piégé (`../`, chemin absolu, bombe ZIP) est refusé sans rien écrire sur le disque.

---

## Instructions de travail

- Commence par le **jalon 1** uniquement. Montre l'arborescence et les choix de crates (avec leurs versions) avant d'écrire beaucoup de code.
- Avant le jalon 3, **demande à l'utilisateur** l'adresse de son dépôt GitHub (`<propriétaire>/flair`) et s'il est public ou privé. Explique-lui comment générer la clé de signature (`minisign -G` ou équivalent en Rust) et l'ajouter en secret GitHub : **ne génère ni ne stocke jamais toi-même la clé privée dans le dépôt**.
- Si une contrainte de ce document est irréalisable ou contradictoire sur une plateforme, **signale-le et propose une alternative** au lieu de la contourner en silence.
- Préfère des crates maintenues et largement utilisées ; justifie chaque dépendance qui n'est pas listée ici.
- Écris des commentaires et des messages de commit clairs, en français ou en anglais (choisis une langue et garde-la).

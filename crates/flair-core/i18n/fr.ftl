# Interface de Flair — français.
# Les identifiants doivent exister aussi dans en.ftl (vérifié par un test).

window-title = Flair
search-placeholder = Rechercher…

action-open = Ouvrir
action-copy = Copier
action-reveal = Afficher le dossier
action-run = Exécuter

kind-application = Application
kind-folder = Dossier
kind-file = Fichier
kind-command = Commande
kind-system = Système

web-search = Rechercher « { $query } » sur le web
web-search-with = Rechercher « { $query } » avec { $engine }
read-as = lu comme : { $reading }
contrast-adjusted = contraste ajusté
confirm-again = Entrée à nouveau pour confirmer

config-error-title = Erreur dans la configuration
config-error-hint = Les valeurs fautives sont remplacées par leurs valeurs par défaut.
config-error-invalid = { $key } : valeur invalide ({ $error })
config-error-unknown = réglage inconnu : { $key }
config-error-syntax = erreur de syntaxe : { $error }
config-error-io = lecture impossible : { $error }

copied = Copié : { $text }

value-not-bool = doit être activé ou désactivé (true / false)
value-not-integer = doit être un nombre entier
value-not-number = doit être un nombre
value-out-of-range = doit être entre { $min } et { $max }
value-not-text = doit être un texte
value-not-choice = valeurs possibles : { $allowed }
value-not-list = doit être une liste
value-empty-list = la liste ne doit pas être vide
value-invalid-language = code de langue invalide : { $value }
value-invalid-hotkey = raccourci invalide : { $value }
value-invalid-color = couleur invalide : { $value } (attendu #RGB ou #RRGGBB)
value-invalid-web-engine = moteur de recherche invalide : { $value } (il faut keyword, name et une url contenant {"{query}"})

tray-tooltip = Flair ({ $hotkey })
tray-show = Afficher
tray-reindex = Réindexer
tray-open-config = Ouvrir la configuration
tray-check-update = Rechercher une mise à jour
tray-autostart = Lancer au démarrage
tray-stats = Statistiques
tray-quit = Quitter

hotkey-taken-title = Raccourci { $hotkey } indisponible
hotkey-taken-detail = Une autre application l'utilise déjà. Entrée : ouvrir la configuration pour changer « hotkey ».
hotkey-failed-title = Raccourci { $hotkey } non enregistré
hotkey-failed-detail = { $error }. Entrée : ouvrir la configuration.
hotkey-wayland-title = Wayland : liez un raccourci du bureau à « flair --toggle »
hotkey-wayland-detail = Les applications ne peuvent pas capturer de raccourci global sous Wayland. Entrée : copier la commande.

stats-title = Statistiques de Flair
stats-version = Version
stats-uptime = Lancé depuis
stats-ram = Mémoire utilisée
stats-index-size = Taille de l'index (fichier / RAM)
stats-index-entries = Entrées de l'index
stats-watched = Dossiers surveillés
stats-wakeups = Réveils depuis le démarrage
stats-last-show = Dernier affichage
stats-renderer = Moteur de rendu
stats-hotkey = Raccourci global
stats-tray = Icône de notification
stats-none = —
stats-yes = oui
stats-no = non
not-running = Flair n'est pas lancé.

# Interface de Flair — français.
# Les identifiants doivent exister aussi dans en.ftl (vérifié par un test).

window-title = Flair
search-placeholder = Rechercher…

action-open = Ouvrir
action-copy = Copier
action-reveal = Afficher le dossier
action-run-admin = Exécuter en administrateur
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

cmd-update = Mise à jour
cmd-update-desc = Chercher et installer la dernière version
cmd-update-rollback = Revenir à la version précédente
cmd-update-rollback-desc = Réinstalle la version d'avant la dernière mise à jour
cmd-update-channel = Canal de mise à jour : { $channel }
cmd-update-channel-desc = main : chaque nouveauté ; stable : versions taguées
cmd-version = Version
cmd-stats = Statistiques
cmd-stats-desc = Mémoire, réveils, temps d'affichage, moteur de rendu
cmd-export = Exporter
cmd-export-desc = Sauvegarder réglages, thèmes, langues et données dans un fichier .flair
cmd-import = Importer
cmd-import-desc = Restaurer un fichier .flair
cmd-undo = Annuler
cmd-undo-desc = Revenir à l'état d'avant le dernier import
cmd-uninstall = Désinstaller Flair
cmd-uninstall-desc = Retirer Flair de ce PC
cmd-install = Installer Flair
cmd-quit = Quitter Flair
update-available-line = Mise à jour disponible : { $version } — > mise à jour

update-checking = Recherche d'une mise à jour…
update-up-to-date = Flair est à jour ({ $version })
update-available = Flair { $version } disponible (actuelle : { $current })
update-available-hint = Entrée : installer
update-reset-warning = Cette mise à jour réinitialise les réglages. Une sauvegarde sera faite.
update-manual = Cette version doit être téléchargée manuellement
update-manual-hint = Entrée : ouvrir la page des versions
update-downloading = Téléchargement… { $percent } %
update-installing = Vérification et installation…
update-failed = La mise à jour a échoué : { $error }
update-disabled-dev = Mises à jour désactivées pour une version de développement
update-disabled-package = Flair a été installé par un gestionnaire de paquets : mettez-le à jour avec lui
update-disabled-readonly = Le dossier de Flair n'est pas accessible en écriture
update-done = Flair mis à jour vers { $version }
update-rolled-back = La mise à jour a échoué, retour à la version { $version }
update-rollback-done = Version précédente restaurée, redémarrage…
update-rollback-none = Aucune version précédente n'est conservée
update-channel-set = Canal de mise à jour : { $channel }
version-line = Flair { $version }
version-detail = commit { $commit }, compilé le { $date }, canal { $channel }

export-all = Tout
export-all-desc = Réglages, thèmes, langues, historique et données d'usage
export-settings = Réglages seulement
export-settings-desc = Sans historique ni données d'usage : à choisir pour partager
export-dialog-title = Exporter Flair
export-done = Exporté : { $path } ({ $size })
export-done-hint = Entrée : ouvrir le dossier
export-failed = L'export a échoué : { $error }

import-pick = Choisir un fichier…
import-pick-desc = Ouvrir un fichier .flair
import-backup = Sauvegarde du { $date } ({ $reason })
import-dialog-title = Importer dans Flair
import-summary = Exporté le { $date } depuis Flair { $version } sur { $host } ({ $os })
import-contents = Contenu : { $contents }
import-contents-settings = réglages
import-contents-themes = { $count } thème(s)
import-contents-lexicons = { $count } lexique(s)
import-contents-history = historique ({ $count } calculs)
import-contents-frecency = données d'usage
import-warning-root = Dossier absent sur ce PC : { $path }
import-warning-invalid = Ignoré (invalide) : { $path }
import-warning-ignored = Ignoré (inconnu) : { $path }
import-replace = Tout remplacer
import-replace-desc = La configuration, les thèmes, les lexiques et les données deviennent ceux du fichier
import-merge = Ajouter les thèmes et lexiques seulement
import-merge-desc = Les réglages actuels ne changent pas
import-newer = Ce fichier vient d'une version plus récente de Flair. Faites d'abord > mise à jour.
import-failed = Import impossible : { $error }
import-done = Importé. > annuler pour revenir en arrière.
import-renamed = Renommé : { $from } → { $to }
undo-done = Retour à l'état d'avant l'import
undo-nothing = Rien à annuler

install-title = Installer Flair
install-desc = Dans { $path }, sans droits administrateur
install-autostart = Lancer au démarrage : { $state }
install-portable = Utiliser sans installer
install-portable-desc = Flair reste où il est (mode portable)
install-done = Flair est installé
install-failed = Installation impossible : { $error }
state-on = oui
state-off = non

uninstall-keep = Désinstaller et garder mes réglages
uninstall-keep-desc = La configuration et les données restent sur ce PC
uninstall-all = Tout supprimer
uninstall-all-desc = Flair, sa configuration et ses données
uninstall-done = Flair est désinstallé
uninstall-failed = Désinstallation impossible : { $error }
backup-reason-update = avant mise à jour
backup-reason-import = avant import
backup-reason-reset = avant réinitialisation
backup-reason-migration = avant migration

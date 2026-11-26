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

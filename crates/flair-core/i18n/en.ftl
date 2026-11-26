# Flair interface — English.
# Every id must also exist in fr.ftl (checked by a test).

window-title = Flair
search-placeholder = Search…

action-open = Open
action-copy = Copy
action-reveal = Show folder
action-run = Run

kind-application = Application
kind-folder = Folder
kind-file = File
kind-command = Command
kind-system = System

web-search = Search the web for “{ $query }”
web-search-with = Search { $engine } for “{ $query }”
read-as = read as: { $reading }
contrast-adjusted = contrast adjusted
confirm-again = Press Enter again to confirm

config-error-title = Configuration error
config-error-hint = Faulty values are replaced by their defaults.
config-error-invalid = { $key }: invalid value ({ $error })
config-error-unknown = unknown setting: { $key }
config-error-syntax = syntax error: { $error }
config-error-io = cannot read: { $error }

copied = Copied: { $text }

value-not-bool = must be on or off (true / false)
value-not-integer = must be a whole number
value-not-number = must be a number
value-out-of-range = must be between { $min } and { $max }
value-not-text = must be a text
value-not-choice = possible values: { $allowed }
value-not-list = must be a list
value-empty-list = the list must not be empty
value-invalid-language = invalid language code: { $value }
value-invalid-hotkey = invalid shortcut: { $value }
value-invalid-color = invalid color: { $value } (expected #RGB or #RRGGBB)
value-invalid-web-engine = invalid search engine: { $value } (needs keyword, name and a url containing {"{query}"})

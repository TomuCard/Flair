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

tray-tooltip = Flair ({ $hotkey })
tray-show = Show
tray-reindex = Reindex
tray-open-config = Open configuration
tray-check-update = Check for updates
tray-autostart = Launch at startup
tray-stats = Statistics
tray-quit = Quit

hotkey-taken-title = Shortcut { $hotkey } unavailable
hotkey-taken-detail = Another application already uses it. Enter: open the configuration to change "hotkey".
hotkey-failed-title = Shortcut { $hotkey } not registered
hotkey-failed-detail = { $error }. Enter: open the configuration.
hotkey-wayland-title = Wayland: bind a desktop shortcut to "flair --toggle"
hotkey-wayland-detail = Applications cannot grab global shortcuts on Wayland. Enter: copy the command.

stats-title = Flair statistics
stats-version = Version
stats-uptime = Running for
stats-ram = Memory used
stats-index-size = Index size (file / RAM)
stats-index-entries = Index entries
stats-watched = Watched folders
stats-wakeups = Wake-ups since start
stats-last-show = Last show
stats-renderer = Renderer
stats-hotkey = Global shortcut
stats-tray = Tray icon
stats-none = —
stats-yes = yes
stats-no = no
not-running = Flair is not running.

cmd-update = Update
cmd-update-desc = Look for and install the latest version
cmd-update-rollback = Go back to the previous version
cmd-update-rollback-desc = Reinstalls the version from before the last update
cmd-update-channel = Update channel: { $channel }
cmd-update-channel-desc = main: every change; stable: tagged versions
cmd-version = Version
cmd-export = Export
cmd-export-desc = Save settings, themes, languages and data to a .flair file
cmd-import = Import
cmd-import-desc = Restore a .flair file
cmd-undo = Undo
cmd-undo-desc = Go back to the state before the last import
cmd-uninstall = Uninstall Flair
cmd-uninstall-desc = Remove Flair from this PC
cmd-install = Install Flair
cmd-quit = Quit Flair
update-available-line = Update available: { $version } — > update

update-checking = Looking for an update…
update-up-to-date = Flair is up to date ({ $version })
update-available = Flair { $version } available (current: { $current })
update-available-hint = Enter: install
update-reset-warning = This update resets the settings. A backup will be made.
update-manual = This version must be downloaded manually
update-manual-hint = Enter: open the releases page
update-downloading = Downloading… { $percent } %
update-installing = Verifying and installing…
update-failed = Update failed: { $error }
update-disabled-dev = Updates are disabled for a development build
update-disabled-package = Flair was installed by a package manager: update it with it
update-disabled-readonly = Flair's folder is not writable
update-done = Flair updated to { $version }
update-rolled-back = The update failed, back to version { $version }
update-rollback-done = Previous version restored, restarting…
update-rollback-none = No previous version is kept
update-channel-set = Update channel: { $channel }
version-line = Flair { $version }
version-detail = commit { $commit }, built on { $date }, { $channel } channel

export-all = Everything
export-all-desc = Settings, themes, languages, history and usage data
export-settings = Settings only
export-settings-desc = Without history nor usage data: pick this to share
export-dialog-title = Export Flair
export-done = Exported: { $path } ({ $size })
export-done-hint = Enter: open the folder
export-failed = Export failed: { $error }

import-pick = Choose a file…
import-pick-desc = Open a .flair file
import-backup = Backup of { $date } ({ $reason })
import-dialog-title = Import into Flair
import-summary = Exported on { $date } from Flair { $version } on { $host } ({ $os })
import-contents = Contents: { $contents }
import-contents-settings = settings
import-contents-themes = { $count } theme(s)
import-contents-lexicons = { $count } lexicon(s)
import-contents-history = history ({ $count } calculations)
import-contents-frecency = usage data
import-warning-root = Folder missing on this PC: { $path }
import-warning-invalid = Ignored (invalid): { $path }
import-warning-ignored = Ignored (unknown): { $path }
import-replace = Replace everything
import-replace-desc = Configuration, themes, lexicons and data become those of the file
import-merge = Add themes and lexicons only
import-merge-desc = Current settings do not change
import-newer = This file comes from a more recent version of Flair. Run > update first.
import-failed = Cannot import: { $error }
import-done = Imported. > undo to go back.
import-renamed = Renamed: { $from } → { $to }
undo-done = Back to the state before the import
undo-nothing = Nothing to undo

install-title = Install Flair
install-desc = In { $path }, without administrator rights
install-autostart = Launch at startup: { $state }
install-portable = Use without installing
install-portable-desc = Flair stays where it is (portable mode)
install-done = Flair is installed
install-failed = Cannot install: { $error }
state-on = yes
state-off = no

uninstall-keep = Uninstall and keep my settings
uninstall-keep-desc = Configuration and data stay on this PC
uninstall-all = Remove everything
uninstall-all-desc = Flair, its configuration and its data
uninstall-done = Flair is uninstalled
uninstall-failed = Cannot uninstall: { $error }
backup-reason-update = before update
backup-reason-import = before import
backup-reason-reset = before reset
backup-reason-migration = before migration

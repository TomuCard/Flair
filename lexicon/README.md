# Language files of the calculator

Flair understands calculations typed in several languages at the same time
(`5 km en miles`, `5 km to miles`, `now + 3 jours`). The words of each
language live here, never in the code:

| File | Content |
|---|---|
| `common.toml` | Neutral part, always active: symbols, international units with their exact case (`MB`, `Mb`, `KiB`), compact bit units (`Mbit`), constants. |
| `fr.toml`, `en.toml` | Words of one language (`fois`, `times`, `jours`, `days`) and its conventions (decimal separator, date order). |
| `tests/<code>.toml` | Cases of one language: `input → expected result`, run alone and combined with the other languages. |

Adding a language means adding a file (and its tests): no code change. Users
can add or override files in the `lexicon/` folder of their configuration
(`~/.config/flair/lexicon/`, `%AppData%\Flair\lexicon\`); a file with the code
of a built-in one replaces it. An invalid file is ignored and the reason is
logged.

## Format

```toml
[meta]
code = "fr"                  # language code, lowercase
name = "Français"
decimal_separator = ","      # "," or "." (not in common.toml)
date_order = "dmy"           # "dmy", "mdy" or "ymd" (not in common.toml)

[[word]]
forms = ["en", "vers"]       # what users type; several words allowed ("il y a")
sense = "convert_to"         # canonical meaning (see below)
when = { after = ["quantity", "number"] }   # optional conditions
priority = 30                # optional, default 0: higher wins
case_sensitive = false       # optional: "Mo" (megabyte) but never "mo"
```

Forms are matched on **whole tokens** after the input is split (numbers,
words, symbols), never inside a word: `de` does not match inside `dernier`.
Multi-word forms are tried first, the longest first. Matching ignores case
and accents (`annee` = `année`, `a` = `à`) unless `case_sensitive = true`.

## Senses

| Sense | Meaning | Becomes |
|---|---|---|
| `op:+`, `op:-`, `op:*`, `op:/`, `op:^`, `op:mod` | operators | the operator |
| `convert_to` | conversion (`en`, `to`, `in`, `→`) | `to` |
| `of` | percentage of (`15% de 240`) | `of` |
| `percent` | `%` written as a word | `%` |
| `one` | article meaning one (`an hour`, `une heure`) | `1` |
| `ignore` | filler word | nothing |
| `unit:<name>` | a unit known to the engine (`unit:day`, `unit:MB`, `unit:inch`) | the name |
| `func:<name>` | a function (`func:sqrt`, `func:gcd`, `func:isprime`) | the function |
| `const:<name>` | a constant (`const:pi`, `const:boltzmann`) | the constant |
| `fend:<text>` | any text of the engine's syntax (`fend:hex`, `fend:^2`) | the text |
| `now`, `ago`, `relative_in`, `until`, `since`, `what_day`, `rate_at`, `time_in`, `month:<1-12>`, `weekday:<1-7>` | dates, times and transfer rates | evaluated from milestone 5 |

## Conditions (`when`)

A closed list: no code and no regular expressions in language files. A case
that cannot be expressed calls for a new condition in the code, with tests.

| Condition | Meaning |
|---|---|
| `after = "<class>"` | the previous token has this class |
| `before = "<class>"` | the next token has this class |
| `between = ["<class>", "<class>"]` | both at once |
| `position = "start"` / `"end"` | the token is the first / last one |
| `in_duration = true` | the token is part of a compound duration (`1h30m`, `5m30s`) |
| `case_sensitive = true` | the case must match exactly |

Classes: `number`, `quantity` (a number and its unit), `unit`, `duration`
(a quantity of time), `time` (time of day), `date`, `percent` (`%`), `start`,
`end`, and `sense:<name>` (a token that can have this sense, for example
`sense:convert_to`). A condition may accept a list: `before = ["end",
"sense:convert_to"]`.

## Ambiguities

When a token has several possible senses, Flair builds the possible readings
(16 at most), **evaluates them all** and drops those that fail (`5 m en ft`
cannot be minutes). The remaining readings are ranked by: the priorities of
the senses used, then how many conditions they met, then the order of the
input languages (`input_languages`), then the fewest reinterpretations. The
best one gives the result; up to two others are shown as variants with
"read as: …".

## Conflicts

The same form with different senses in two active files (or in one) is a
conflict. It is **resolved** when the priorities differ or when the
conditions can never hold together; otherwise it is reported.

```bash
flair lexicon check                 # every combination of built-in languages
flair lexicon check --langs fr,en   # one combination (user files included)
```

The command lists every conflict with an example input and exits with an
error if one is unresolved. The CI runs it.

# normen

A vim-style reader for German federal law from gesetze-im-internet.de.

## Language

**CORE**:
The working set of laws the user can open, jump to with `i`, list with `normen laws`, and query from the CLI. Default is the shipped seed in its compiled order. Empty CORE is allowed.
_Avoid_: catalog, favorites, pin list

**BUNDESRECHT**:
The full gesetze-im-internet.de index of consolidated federal Gesetze and Verordnungen. It is not listed on MENU. Membership in CORE changes through the Bundesrecht pane.
_Avoid_: catalog, Bundesgesetze, GII (as the collection name)

**Shortcut**:
The abbreviation shown to the user and stored in CORE order (BGB, GG, BauNVO).
_Avoid_: abbreviation, jurabk (except when talking to the GII XML)

**Slug**:
The gesetze-im-internet.de URL folder used to download `xml.zip` and to name the on-disk cache (`bgb`, `bbaug`).
_Avoid_: id, path (except as the URL path)

**Priority order**:
CORE display and `normen laws` default order: the `[core] order` list in the conf file, or the shipped seed when that section is missing. A saved workspace may carry a different CORE for that session only.
_Avoid_: custom order, config order (say priority order or conf order)

**Mark**:
A toggle in the Bundesrecht pane. Space marks a law to add (if it is not on CORE) or to remove (if it is). Enter applies every mark; Esc discards them.
_Avoid_: select, pin, checkbox

# Conf stores CORE as a shortcut CSV

`[core] order = BGB, GG, StVG` is what users edit. Slugs and titles are not duplicated in the conf: they resolve from the shipped seed first, then the cached Bundesrecht index. That keeps the file human-editable and ordered, at the cost of needing the index (fetch if needed) for shortcuts the seed does not know. Unknown names are skipped with a warning so a typo cannot empty CORE.

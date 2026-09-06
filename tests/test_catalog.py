from normen.catalog import LAWS, resolve_law


def test_supported_shortcuts() -> None:
    assert {law.shortcut.upper() for law in LAWS} == {"BGB", "GG", "VWGO", "VWVFG"}


def test_resolve_law_is_case_insensitive() -> None:
    assert resolve_law("bgb").shortcut == "BGB"
    assert resolve_law("VwVfG").slug == "vwvfg"
    assert resolve_law("vwgo").title.startswith("Verwaltungsgerichts")


def test_resolve_law_accepts_common_typo_alias() -> None:
    assert resolve_law("vwvwfg").shortcut == "VwVfG"


def test_resolve_unknown_law_returns_none() -> None:
    assert resolve_law("stgb") is None

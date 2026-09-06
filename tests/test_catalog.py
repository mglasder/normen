from normen.catalog import LAWS, filter_laws, resolve_law


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


def test_filter_laws_empty_query_returns_all() -> None:
    assert filter_laws("") == list(LAWS)
    assert filter_laws("   ") == list(LAWS)


def test_filter_laws_matches_shortcut_slug_title_and_alias() -> None:
    assert [law.shortcut for law in filter_laws("bgb")] == ["BGB"]
    assert [law.shortcut for law in filter_laws("BÜRGER")] == ["BGB"]
    assert [law.shortcut for law in filter_laws("grund")] == ["GG"]
    assert [law.shortcut for law in filter_laws("vwvwfg")] == ["VwVfG"]
    assert [law.shortcut for law in filter_laws("vw")] == ["VwGO", "VwVfG"]


def test_filter_laws_unknown_query_is_empty() -> None:
    assert filter_laws("stgb") == []

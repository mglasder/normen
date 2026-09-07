from normen.catalog import LAWS, filter_laws, resolve_law


def test_supported_shortcuts() -> None:
    assert {law.shortcut.upper() for law in LAWS} == {
        "BGB",
        "GG",
        "VWGO",
        "VWVFG",
        "BVERFGG",
        "GOBT",
        "GOBR",
        "PARTG",
        "VEREINSG",
        "VERSAMMLG",
        "BAUGB",
        "BAUNVO",
        "VWZG",
        "VWVG",
        "STGB",
        "ZPO",
        "STPO",
        "HGB",
        "EGBGB",
    }


def test_resolve_law_is_case_insensitive() -> None:
    assert resolve_law("bgb").shortcut == "BGB"
    assert resolve_law("VwVfG").slug == "vwvfg"
    assert resolve_law("vwgo").title.startswith("Verwaltungsgerichts")


def test_resolve_law_accepts_common_typo_alias() -> None:
    assert resolve_law("vwvwfg").shortcut == "VwVfG"


def test_resolve_law_accepts_new_shortcuts_and_slugs() -> None:
    assert resolve_law("bverfgg").slug == "bverfgg"
    assert resolve_law("gobt").slug == "btgo_2025"
    assert resolve_law("btgo").shortcut == "GOBT"
    assert resolve_law("gobr").slug == "brgo_2025"
    assert resolve_law("partg").title.startswith("Gesetz über die politischen Parteien")
    assert resolve_law("parteiengesetz").shortcut == "PartG"
    assert resolve_law("vereinsg").shortcut == "VereinsG"
    assert resolve_law("versammlungsgesetz").shortcut == "VersammlG"
    assert resolve_law("baugb").slug == "bbaug"
    assert resolve_law("baunvo").shortcut == "BauNVO"
    assert resolve_law("vwzg").slug == "vwzg_2005"
    assert resolve_law("vwvg").shortcut == "VwVG"
    assert resolve_law("stgb").shortcut == "StGB"
    assert resolve_law("zpo").shortcut == "ZPO"
    assert resolve_law("stpo").shortcut == "StPO"
    assert resolve_law("hgb").shortcut == "HGB"
    assert resolve_law("egbgb").slug == "bgbeg"
    assert resolve_law("bgbeg").shortcut == "EGBGB"


def test_resolve_unknown_law_returns_none() -> None:
    assert resolve_law("xyzzy") is None


def test_filter_laws_empty_query_returns_all() -> None:
    assert filter_laws("") == list(LAWS)
    assert filter_laws("   ") == list(LAWS)


def test_filter_laws_matches_shortcut_slug_title_and_alias() -> None:
    assert [law.shortcut for law in filter_laws("bgb")] == ["BGB", "EGBGB"]
    assert [law.shortcut for law in filter_laws("BÜRGER")] == ["BGB", "EGBGB"]
    assert [law.shortcut for law in filter_laws("grund")] == ["GG"]
    assert [law.shortcut for law in filter_laws("vwvwfg")] == ["VwVfG"]
    assert [law.shortcut for law in filter_laws("vw")] == ["VwGO", "VwVfG", "VwZG", "VwVG"]


def test_filter_laws_unknown_query_is_empty() -> None:
    assert filter_laws("xyzzy") == []

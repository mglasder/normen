from pathlib import Path

from normen.session import SessionStore


def test_session_remembers_in_the_same_instance() -> None:
    store = SessionStore()
    store.set("bgb", "§ 433")
    assert store.get("bgb") == "§ 433"


def test_session_is_not_shared_across_instances() -> None:
    store = SessionStore()
    store.set("bgb", "§ 433")
    assert SessionStore().get("bgb") is None


def test_session_does_not_write_state_file(tmp_path: Path) -> None:
    store = SessionStore()
    store.set("bgb", "§ 433")
    assert list(tmp_path.iterdir()) == []


def test_session_missing_key_is_none() -> None:
    store = SessionStore()
    assert store.get("gg") is None

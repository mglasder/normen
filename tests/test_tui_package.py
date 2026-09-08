from normen.tui.keys import WINDOW_RADIUS, window_range
from normen.tui.widgets import CommandInput, NormBlock, TabBar


def test_window_range_is_imported_from_tui_keys() -> None:
    assert window_range(0, 80) == (0, WINDOW_RADIUS + 1)
    start, end = window_range(70, 80)
    assert end == 80
    assert end - start <= 2 * WINDOW_RADIUS + 1
    assert start <= 70 < end


def test_widgets_are_imported_from_tui_widgets() -> None:
    from normen.tui import NormBlock as PublicBlock
    from normen.tui import TabBar as PublicBar

    assert CommandInput.__name__ == "CommandInput"
    assert NormBlock is PublicBlock
    assert TabBar is PublicBar


def test_picker_screen_is_imported_from_tui_picker() -> None:
    from normen.tui.picker import PickerScreen
    from normen.tui import PickerScreen as PublicPicker

    assert PickerScreen is PublicPicker


def test_reader_screen_is_imported_from_tui_reader() -> None:
    from normen.tui.reader import ReaderScreen
    from normen.tui import ReaderScreen as PublicReader

    assert ReaderScreen is PublicReader
    assert window_range(0, 80) == (0, WINDOW_RADIUS + 1)
    start, end = window_range(70, 80)
    assert end == 80
    assert end - start <= 2 * WINDOW_RADIUS + 1
    assert start <= 70 < end

from textual.theme import Theme

# NvChad base46 pastelDark — same palette as Ghostty ~/.config/ghostty/config
PASTEL_DARK = Theme(
    name="pastel-dark",
    primary="#9ce5c0",
    secondary="#a3b8ef",
    accent="#f5d595",
    foreground="#ced4df",
    background="#10171e",
    success="#9ce5c0",
    warning="#f5d595",
    error="#ef8891",
    surface="#131a21",
    panel="#131a21",
    dark=True,
    variables={
        "block-cursor-foreground": "#10171e",
        "block-cursor-background": "#9ce5c0",
        "block-cursor-blurred-foreground": "#10171e",
        "block-cursor-blurred-background": "#9ce5c0",
        "block-cursor-text-style": "bold",
        "block-cursor-blurred-text-style": "bold",
        "footer-background": "#131a21",
        "footer-key-foreground": "#9ce5c0",
        "footer-description-foreground": "#ced4df",
        "input-selection-background": "#2a3138",
        "border": "#40474e",
    },
)

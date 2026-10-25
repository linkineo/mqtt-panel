# Validated mockups

`mqtt-panel-mockups.html` is the implementation reference for the user interface: twenty-seven screens and states. Open it in any browser; it is self-contained (no network, no external fonts). The header switches light, dark and the three accents.

Rules the mockups encode and the app must follow:
- Labels in plain words, never jargon in user-facing text.
- Form rows never wrap; helper text is short and the full explanation sits in an ⓘ bubble.
- Every file field is a native picker with drag-and-drop; nobody types a path.
- Every security option has an info bubble in plain words (EN, FR, DE).
- Window chrome, menus, dialogs and scrollbars are the native ones of each OS; the two menu-bar strips in the file are samples.

The app's screens are compared against these mockups in the visual regression suite.

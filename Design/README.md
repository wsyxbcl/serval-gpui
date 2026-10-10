# Design

This directory is the first source for Waxbill's design. The implementation follows it: a change to behaviour
or appearance edits the spec and the board here in the same change as the code. Every topic has one spec and
one board; a replaced rule is edited in place and an abandoned one is deleted, never kept as a versioned copy.
(The method is borrowed from [Cida's Design directory](https://github.com/Xuanwo/cida/tree/main/Design).)

[`spec/decisions.md`](spec/decisions.md) tracks the numbered proposals and their status. A proposal is not
implemented before it is confirmed; once confirmed, the topic's spec and board are updated in place.

| Topic | Rules | States |
| --- | --- | --- |
| Shell: window, sidebar, screen anatomy, keyboard | [`spec/shell.md`](spec/shell.md) | [`boards/shell.html`](boards/shell.html) |
| Run: from ready to result, progress, results, errors, stopping | [`spec/run.md`](spec/run.md) | [`boards/run-states.html`](boards/run-states.html) |
| Questions: what Serval used to ask while running, answered as fields | [`spec/questions.md`](spec/questions.md) | [`boards/questions.html`](boards/questions.html), conflicts in [`boards/run-states.html`](boards/run-states.html) |
| Browse and pick: choose from a table, then view in Caracal or a review folder, write an edited table back | [`spec/browse.md`](spec/browse.md) | to be drawn |
| Data: reading inputs, table check and repair, choices from real values, previews | [`spec/data.md`](spec/data.md) | [`boards/data.html`](boards/data.html) |
| Serval protocol: flags, level numbering, progress and result output | [`spec/serval-protocol.md`](spec/serval-protocol.md) | none |
| Serval delivery and Settings: choosing the program, compatibility, language, appearance | [`spec/settings.md`](spec/settings.md) | [`boards/settings.html`](boards/settings.html), first run in [`boards/shell.html`](boards/shell.html) |
| Appearance: tokens, light and dark, type, copy | [`spec/appearance.md`](spec/appearance.md) | [`boards/tokens.css`](boards/tokens.css) |

## Boards

Boards are plain HTML drawn from shared files:

- [`boards/tokens.css`](boards/tokens.css) holds every design token: colours, type, spacing, radii, layout sizes
  and motion. `:root` is the light appearance and `[data-appearance="dark"]` the dark one. Colour names follow
  gpui-component's theme keys; the app's theme mirrors this file one to one, and a test fails when they drift
  (to be added with the implementation). Specs name tokens rather than repeating numbers.
- [`boards/components.css`](boards/components.css) and [`boards/components.js`](boards/components.js) draw the
  parts every state shares: the window, title bar, sidebar, fields, run bar, banners, dialog. A state is only
  the markup that differs.

Each state carries `data-state="<name>"`. The app will accept `--design-state <name>` to open directly in that
state, so implementation screenshots can be taken without clicking and compared with the board.

Example data on the boards (paths, counts, versions) is illustrative. Serval 0.8.0 on the boards stands for the
first Serval release with the flags in [`spec/serval-protocol.md`](spec/serval-protocol.md).

## Rendering

```sh
scripts/render-design.sh                 # every board
scripts/render-design.sh run-states.html # one board
HEIGHT=6000 scripts/render-design.sh run-states.html   # taller page
```

Renders go to `rendered/boards/<board>.png` with headless Chromium (or Chrome).

App screenshots for review are taken in an isolated session so nothing else on the desktop is captured:
a private D-Bus session with a headless KWin (`kwin_wayland --virtual`), the app on that display, and
`spectacle -b -n -f` there. Xvfb does not work (GPUI needs GPU surfaces).

## Pull requests that change the UI

Attach before and after screenshots of the real app for every changed state. A UI change without screenshots
is not ready for review.

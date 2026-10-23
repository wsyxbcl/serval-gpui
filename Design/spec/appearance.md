# Appearance

Tokens: [`boards/tokens.css`](../boards/tokens.css). Proposal [W09](decisions.md); framework [W01](decisions.md).

## 1. Palette

- From the app icon: warm paper background (`--background`), white surfaces (`--surface`), teal ink for text
  and the primary action (`--foreground`, `--primary`), the waxbill's coral-red bill as the accent
  (`--accent`), used only for the selected sidebar marker and inline links.
- Status colours are separate from the accent: `--success`, `--warning`, `--danger`, each with a soft fill for
  banners. Red means a problem, never decoration.
- Dark appearance: the same roles in `[data-appearance="dark"]`; the primary turns light teal on dark ink.
  Appearance follows the system unless set in Settings.

## 2. Type

- System UI faces with Chinese fallbacks (`--font-ui`); monospace for paths in pickers, the command line and the
  log (`--font-mono`).
- Sizes: `--text-xl` screen titles, `--text-lg` dialog titles, `--text-md` body and controls, `--text-sm` help
  lines and dense lists, `--text-xs` the command line and captions.

## 3. Copy

- Chinese is the default language; English is complete. Labels name the task in the user's words (标签读取, not
  observe); the Serval command name appears in the command line and the help.
- Sentences say what happened and what to do next. Numbers use thousands separators. No status codes or raw
  errors as the only text.
- Buttons are verbs (运行, 停止, 继续运行, 打开文件夹); a button and its shortcut do the same thing.

## 4. Implementation

- `gpui-kit` 0.7, pinned with `=`; bumped deliberately (weekly upstream snapshots).
- A Waxbill theme for gpui-component is generated from `tokens.css` (same names), and a test compares the two
  so they cannot drift.
- gpui-component's widgets are used as they are (input with IME, select, switch/checkbox/radio, dialog,
  notification, progress, table, sidebar); custom drawing only for the path picker and the run bar.

# Button

A clickable button with text.

## Syntax

```xml
<button x="..." y="..." width="..." height="...">Button Text</button>
```

## Attributes

| Attribute | Required | Type   | Description                      |
|-----------|----------|--------|----------------------------------|
| `x`       | Yes      | number | X offset from content area       |
| `y`       | Yes      | number | Y offset from content area       |
| `width`   | Yes      | number | Button width in pixels           |
| `height`  | Yes      | number | Button height in pixels          |

## Example

```xml
<app title="Demo" width="300" height="150" x="100" y="100">
    <button x="20" y="50" width="100" height="30">OK</button>
    <button x="130" y="50" width="100" height="30">Cancel</button>
</app>
```

## Notes

- Button text is left-aligned with 4px padding
- Buttons have a border and background from the theme
- Buttons scale proportionally when the window is resized
- Click handling is not yet implemented (visual only)

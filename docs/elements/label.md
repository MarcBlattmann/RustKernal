# Label

Displays static text.

## Syntax

```xml
<label x="..." y="...">Text content</label>
```

## Attributes

| Attribute | Required | Type   | Description                      |
|-----------|----------|--------|----------------------------------|
| `x`       | Yes      | number | X offset from content area       |
| `y`       | Yes      | number | Y offset from content area       |

## Example

```xml
<app title="Demo" width="200" height="100" x="100" y="100">
    <label x="20" y="30">Hello World!</label>
    <label x="20" y="50">Second line</label>
</app>
```

## Notes

- Text is rendered in the default system font (8x8 pixels per character)
- Labels scale proportionally when the window is resized
- Text color uses the theme foreground color

# Panel

A decorative container/divider element.

## Syntax

```xml
<panel x="..." y="..." width="..." height="..."/>
```

## Attributes

| Attribute | Required | Type   | Description                      |
|-----------|----------|--------|----------------------------------|
| `x`       | Yes      | number | X offset from content area       |
| `y`       | Yes      | number | Y offset from content area       |
| `width`   | Yes      | number | Panel width in pixels            |
| `height`  | Yes      | number | Panel height in pixels           |

## Example

```xml
<app title="Demo" width="300" height="200" x="100" y="100">
    <label x="20" y="20">Section 1</label>
    <panel x="20" y="45" width="260" height="2"/>
    <label x="20" y="60">Section 2</label>
</app>
```

## Use Cases

- Horizontal divider: `<panel x="10" y="50" width="280" height="2"/>`
- Vertical divider: `<panel x="150" y="10" width="2" height="180"/>`
- Content box: `<panel x="10" y="10" width="280" height="180"/>`

## Notes

- Panels have a dark background with a border
- Use small height (1-2px) for horizontal dividers
- Use small width (1-2px) for vertical dividers

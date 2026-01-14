# VBox (Vertical Box)

A layout container that arranges children vertically (top to bottom).

## Syntax

```xml
<vbox gap="..." padding="...">
    <!-- children -->
</vbox>
```

## Attributes

| Attribute | Required | Type   | Default | Description                      |
|-----------|----------|--------|---------|----------------------------------|
| `gap`     | No       | number | 0       | Space between children (pixels)  |
| `padding` | No       | number | 0       | Inner padding around content     |

## Example

```xml
<app title="VBox Demo" width="200" height="200" x="100" y="100">
    <vbox gap="10" padding="20">
        <label>First item</label>
        <label>Second item</label>
        <label>Third item</label>
        <button width="100" height="30">Click Me</button>
    </vbox>
</app>
```

## Visual Layout

```
+---------------------------+
|  padding                  |
|  +---------------------+  |
|  | First item          |  |
|  +---------------------+  |
|  | gap                 |  |
|  +---------------------+  |
|  | Second item         |  |
|  +---------------------+  |
|  | gap                 |  |
|  +---------------------+  |
|  | Third item          |  |
|  +---------------------+  |
|  padding                  |
+---------------------------+
```

## Notes

- Children are stacked vertically
- Use `gap` to add consistent spacing between items
- Use `padding` for space around the edges
- Can be nested inside other containers
- See also: [hbox](hbox.md), [spacer](spacer.md)

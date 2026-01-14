# HBox (Horizontal Box)

A layout container that arranges children horizontally (left to right).

## Syntax

```xml
<hbox gap="..." padding="...">
    <!-- children -->
</hbox>
```

## Attributes

| Attribute | Required | Type   | Default | Description                      |
|-----------|----------|--------|---------|----------------------------------|
| `gap`     | No       | number | 0       | Space between children (pixels)  |
| `padding` | No       | number | 0       | Inner padding around content     |

## Example

```xml
<app title="HBox Demo" width="300" height="100" x="100" y="100">
    <hbox gap="10" padding="20">
        <button width="80" height="30">OK</button>
        <button width="80" height="30">Cancel</button>
        <button width="80" height="30">Help</button>
    </hbox>
</app>
```

## Visual Layout

```
+-----------------------------------------------+
|  padding                                      |
|  +--------+ gap +--------+ gap +--------+     |
|  |  OK    |     | Cancel |     |  Help  |     |
|  +--------+     +--------+     +--------+     |
|  padding                                      |
+-----------------------------------------------+
```

## Common Use Cases

### Button Row
```xml
<hbox gap="10" padding="0">
    <button width="80" height="30">Save</button>
    <button width="80" height="30">Cancel</button>
</hbox>
```

### Toolbar
```xml
<hbox gap="5" padding="5">
    <button width="30" height="30">+</button>
    <button width="30" height="30">-</button>
    <button width="30" height="30">*</button>
    <button width="30" height="30">/</button>
</hbox>
```

## Notes

- Children are arranged left to right
- Use `gap` for consistent spacing between items
- Use `padding` for space around the edges
- Can be nested inside vbox or other hbox
- See also: [vbox](vbox.md), [spacer](spacer.md)

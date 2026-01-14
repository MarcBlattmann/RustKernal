# TextBox

An input field area (currently visual only).

## Syntax

```xml
<textbox x="..." y="..." width="..." height="..."/>
```

## Attributes

| Attribute | Required | Type   | Description                      |
|-----------|----------|--------|----------------------------------|
| `x`       | Yes      | number | X offset from content area       |
| `y`       | Yes      | number | Y offset from content area       |
| `width`   | Yes      | number | TextBox width in pixels          |
| `height`  | Yes      | number | TextBox height in pixels         |

## Example

```xml
<app title="Login" width="300" height="200" x="100" y="100">
    <label x="20" y="30">Username:</label>
    <textbox x="20" y="50" width="260" height="25"/>
    
    <label x="20" y="90">Password:</label>
    <textbox x="20" y="110" width="260" height="25"/>
    
    <button x="100" y="150" width="100" height="30">Login</button>
</app>
```

## Notes

- TextBox has a dark background with a light border
- Currently visual only - text input not yet implemented
- Scales proportionally when window is resized

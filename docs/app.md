# App Container

The root element that defines a window application.

## Syntax

```xml
<app title="..." width="..." height="..." x="..." y="...">
    <!-- elements -->
</app>
```

## Attributes

| Attribute | Required | Type   | Description                    |
|-----------|----------|--------|--------------------------------|
| `title`   | Yes      | string | Window title bar text          |
| `width`   | Yes      | number | Window width in pixels         |
| `height`  | Yes      | number | Window height in pixels        |
| `x`       | Yes      | number | Initial X position on screen   |
| `y`       | Yes      | number | Initial Y position on screen   |

## Example

```xml
<app title="My Application" width="400" height="300" x="100" y="50">
    <label x="20" y="30">Welcome!</label>
</app>
```

## Notes

- The content area starts below the title bar
- Windows can be dragged by the title bar
- Windows can be resized from the bottom-right corner
- Click the X button to close
